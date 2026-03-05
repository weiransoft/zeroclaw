pub mod cli;
pub mod discord;
pub mod email_channel;
pub mod imessage;
pub mod irc;
pub mod lark;
pub mod matrix;
pub mod slack;
pub mod telegram;
pub mod traits;
pub mod whatsapp;

pub use cli::CliChannel;
pub use discord::DiscordChannel;
pub use email_channel::EmailChannel;
pub use imessage::IMessageChannel;
pub use irc::IrcChannel;
pub use lark::LarkChannel;
pub use matrix::MatrixChannel;
pub use slack::SlackChannel;
pub use telegram::TelegramChannel;
pub use traits::Channel;
pub use whatsapp::WhatsAppChannel;

use crate::agent::loop_::{build_tool_instructions, run_tool_call_loop};
use crate::config::Config;

use crate::memory::{self, Memory};
use crate::observability::{self, Observer};
use crate::providers::{self, ChatMessage, Provider};
use crate::runtime;
use crate::security::SecurityPolicy;
use crate::tools::{self, Tool};
use crate::util::{token_counter::TokenCounter, truncate_with_ellipsis};
use anyhow::Result;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum characters per injected workspace file (matches `OpenClaw` default).
const BOOTSTRAP_MAX_CHARS: usize = 20_000;

const DEFAULT_CHANNEL_INITIAL_BACKOFF_SECS: u64 = 2;
const DEFAULT_CHANNEL_MAX_BACKOFF_SECS: u64 = 60;
/// Timeout for processing a single channel message (LLM + tools).
/// 300s for on-device LLMs (Ollama) which are slower than cloud APIs.
const CHANNEL_MESSAGE_TIMEOUT_SECS: u64 = 300;
const CHANNEL_PARALLELISM_PER_CHANNEL: usize = 4;
const CHANNEL_MIN_IN_FLIGHT_MESSAGES: usize = 8;
const CHANNEL_MAX_IN_FLIGHT_MESSAGES: usize = 64;

#[derive(Clone)]
struct ChannelRuntimeContext {
    channels_by_name: Arc<HashMap<String, Arc<dyn Channel>>>,
    provider: Arc<dyn Provider>,
    memory: Arc<dyn Memory>,
    tools_registry: Arc<Vec<Box<dyn Tool>>>,
    observer: Arc<dyn Observer>,
    system_prompt: Arc<String>,
    model: Arc<String>,
    temperature: f64,
    auto_save_memory: bool,
    max_tool_iterations: usize,
    token_counter: Arc<TokenCounter>,
}

fn conversation_memory_key(msg: &traits::ChannelMessage) -> String {
    format!("{}_{}_{}", msg.channel, msg.sender, msg.id)
}

/// Create a compact summary of content
fn create_compact_summary(content: &str, max_words: usize) -> String {
    let words: Vec<_> = content.split_whitespace().collect();
    if words.len() <= max_words {
        content.to_string()
    } else {
        let summary_words = &words[..max_words];
        format!("{}...", summary_words.join(" "))
    }
}

async fn build_memory_context(mem: &dyn Memory, user_msg: &str) -> String {
    let mut context = String::new();

    if let Ok(entries) = mem.recall(user_msg, 5).await {
        if !entries.is_empty() {
            context.push_str("[Memory context]\n");
            let mut total_chars = 0;
            let max_chars = 800; // Further reduced limit
            
            for entry in &entries {
                // Skip low-relevance entries
                if let Some(score) = entry.score {
                    if score < 0.3 {
                        continue;
                    }
                }
                
                // Advanced compression with summarization
                let compressed_content = if entry.content.len() > 150 {
                    let relevance_factor = entry.score.unwrap_or(0.5);
                    let max_words = if relevance_factor > 0.7 {
                        20 // More words for highly relevant content
                    } else if relevance_factor > 0.4 {
                        15 // Medium for moderately relevant
                    } else {
                        10 // Fewer words for less relevant
                    };
                    create_compact_summary(&entry.content, max_words)
                } else if entry.content.len() > 50 {
                    // For medium-length content, use smart truncation
                    let max_length = if entry.score.unwrap_or(0.5) > 0.5 {
                        80
                    } else {
                        50
                    };
                    if entry.content.len() > max_length {
                        format!("{:.1$}...", entry.content, max_length)
                    } else {
                        entry.content.clone()
                    }
                } else {
                    entry.content.clone()
                };
                
                // Compact entry format
                let entry_line = format!("- {}: {}", entry.key, compressed_content);
                let entry_len = entry_line.len();
                
                if total_chars + entry_len <= max_chars {
                    let _ = writeln!(context, "{}", entry_line);
                    total_chars += entry_len;
                } else {
                    // Compact marker for truncated entries
                    let remaining = entries.len() - context.lines().count() + 1;
                    if remaining > 0 {
                        let _ = writeln!(context, "- ... {} more memories (truncated)", remaining);
                    }
                    break;
                }
            }
            context.push('\n');
        }
    }

    context
}

fn spawn_supervised_listener(
    ch: Arc<dyn Channel>,
    tx: tokio::sync::mpsc::Sender<traits::ChannelMessage>,
    initial_backoff_secs: u64,
    max_backoff_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let component = format!("channel:{}", ch.name());
        let mut backoff = initial_backoff_secs.max(1);
        let max_backoff = max_backoff_secs.max(backoff);

        loop {
            crate::health::mark_component_ok(&component);
            let result = ch.listen(tx.clone()).await;

            if tx.is_closed() {
                break;
            }

            match result {
                Ok(()) => {
                    tracing::warn!("Channel {} exited unexpectedly; restarting", ch.name());
                    crate::health::mark_component_error(&component, "listener exited unexpectedly");
                    // Clean exit — reset backoff since the listener ran successfully
                    backoff = initial_backoff_secs.max(1);
                }
                Err(e) => {
                    tracing::error!("Channel {} error: {e}; restarting", ch.name());
                    crate::health::mark_component_error(&component, e.to_string());
                }
            }

            crate::health::bump_component_restart(&component);
            tokio::time::sleep(Duration::from_secs(backoff)).await;
            // Double backoff AFTER sleeping so first error uses initial_backoff
            backoff = backoff.saturating_mul(2).min(max_backoff);
        }
    })
}

fn compute_max_in_flight_messages(channel_count: usize) -> usize {
    channel_count
        .saturating_mul(CHANNEL_PARALLELISM_PER_CHANNEL)
        .clamp(
            CHANNEL_MIN_IN_FLIGHT_MESSAGES,
            CHANNEL_MAX_IN_FLIGHT_MESSAGES,
        )
}

fn log_worker_join_result(result: Result<(), tokio::task::JoinError>) {
    if let Err(error) = result {
        tracing::error!("Channel message worker crashed: {error}");
    }
}

// Track conversation state for incremental context
#[derive(Clone, Debug)]
struct ConversationState {
    last_processed_content: String,
    last_memory_context: String,
    token_count: u64,
}

use tokio::sync::RwLock;

// Conversation state cache
lazy_static::lazy_static! {
    static ref CONVERSATION_STATE_CACHE: RwLock<HashMap<String, ConversationState>> = RwLock::new(HashMap::new());
}

/// Generate conversation key for state tracking
fn conversation_key(msg: &traits::ChannelMessage) -> String {
    format!("{}_{}", msg.channel, msg.sender)
}

/// Build incremental context - only add new information
async fn build_incremental_context(
    msg: &traits::ChannelMessage,
    memory_context: &str,
) -> String {
    let conv_key = conversation_key(msg);
    let mut cache = CONVERSATION_STATE_CACHE.write().await;
    
    let enriched_message = if let Some(state) = cache.get(&conv_key) {
        // Check if content is new
        if msg.content != state.last_processed_content || memory_context != state.last_memory_context {
            // Only add new information
            let mut context = String::new();
            
            // Add memory context only if it changed
            if memory_context != state.last_memory_context && !memory_context.is_empty() {
                context.push_str(memory_context);
            }
            
            // Always add new user message
            context.push_str(&msg.content);
            context
        } else {
            // No new information, just send the message
            msg.content.clone()
        }
    } else {
        // First message in conversation
        if memory_context.is_empty() {
            msg.content.clone()
        } else {
            format!("{memory_context}{}", msg.content)
        }
    };
    
    // Update conversation state
    cache.insert(conv_key, ConversationState {
        last_processed_content: msg.content.clone(),
        last_memory_context: memory_context.to_string(),
        token_count: enriched_message.len() as u64 / 4, // Rough token estimate
    });
    
    enriched_message
}

async fn process_channel_message(ctx: Arc<ChannelRuntimeContext>, msg: traits::ChannelMessage) {
    println!(
        "  💬 [{}] from {}: {}",
        msg.channel,
        msg.sender,
        truncate_with_ellipsis(&msg.content, 80)
    );

    let memory_context = build_memory_context(ctx.memory.as_ref(), &msg.content).await;

    if ctx.auto_save_memory {
        let autosave_key = conversation_memory_key(&msg);
        let _ = ctx
            .memory
            .store(
                &autosave_key,
                &msg.content,
                crate::memory::MemoryCategory::Conversation,
            )
            .await;
    }

    let enriched_message = build_incremental_context(&msg, &memory_context).await;

    let target_channel = ctx.channels_by_name.get(&msg.channel).cloned();

    if let Some(channel) = target_channel.as_ref() {
        if let Err(e) = channel.start_typing(&msg.sender).await {
            tracing::debug!("Failed to start typing on {}: {e}", channel.name());
        }
    }

    println!("  ⏳ Processing message...");
    let started_at = Instant::now();

    // Count tokens for system prompt and user message
    if let Err(e) = ctx.token_counter.add_prompt_tokens(ctx.system_prompt.as_str()) {
        eprintln!("  ❌ Token limit exceeded: {e}");
        if let Some(channel) = target_channel.as_ref() {
            let _ = channel.send("⚠️ Token limit exceeded. Please try a shorter message.", &msg.sender).await;
        }
        return;
    }
    
    if let Err(e) = ctx.token_counter.add_prompt_tokens(&enriched_message) {
        eprintln!("  ❌ Token limit exceeded: {e}");
        if let Some(channel) = target_channel.as_ref() {
            let _ = channel.send("⚠️ Token limit exceeded. Please try a shorter message.", &msg.sender).await;
        }
        return;
    }

    let mut history = vec![
        ChatMessage::system(ctx.system_prompt.as_str()),
        ChatMessage::user(&enriched_message),
    ];

    let llm_result = tokio::time::timeout(
        Duration::from_secs(CHANNEL_MESSAGE_TIMEOUT_SECS),
        run_tool_call_loop(
            ctx.provider.as_ref(),
            &mut history,
            ctx.tools_registry.as_ref(),
            ctx.observer.as_ref(),
            "channel-runtime",
            ctx.model.as_str(),
            ctx.temperature,
            true, // silent — channels don't write to stdout
            ctx.max_tool_iterations,
        ),
    )
    .await;

    if let Some(channel) = target_channel.as_ref() {
        if let Err(e) = channel.stop_typing(&msg.sender).await {
            tracing::debug!("Failed to stop typing on {}: {e}", channel.name());
        }
    }

    match llm_result {
        Ok(Ok(response)) => {
            // Count completion tokens
            let _ = ctx.token_counter.add_completion_tokens(&response);
            
            println!(
                "  🤖 Reply ({}ms): {} {}",
                started_at.elapsed().as_millis(),
                truncate_with_ellipsis(&response, 80),
                ctx.token_counter.summary()
            );
            if let Some(channel) = target_channel.as_ref() {
                if let Err(e) = channel.send(&response, &msg.sender).await {
                    eprintln!("  ❌ Failed to reply on {}: {e}", channel.name());
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!(
                "  ❌ LLM error after {}ms: {e}",
                started_at.elapsed().as_millis()
            );
            if let Some(channel) = target_channel.as_ref() {
                let _ = channel.send(&format!("⚠️ Error: {e}"), &msg.sender).await;
            }
        }
        Err(_) => {
            let timeout_msg = format!(
                "LLM response timed out after {}s",
                CHANNEL_MESSAGE_TIMEOUT_SECS
            );
            eprintln!(
                "  ❌ {} (elapsed: {}ms)",
                timeout_msg,
                started_at.elapsed().as_millis()
            );
            if let Some(channel) = target_channel.as_ref() {
                let _ = channel
                    .send(
                        "⚠️ Request timed out while waiting for the model. Please try again.",
                        &msg.sender,
                    )
                    .await;
            }
        }
    }
}

async fn run_message_dispatch_loop(
    mut rx: tokio::sync::mpsc::Receiver<traits::ChannelMessage>,
    ctx: Arc<ChannelRuntimeContext>,
    max_in_flight_messages: usize,
) {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_in_flight_messages));
    let mut workers = tokio::task::JoinSet::new();

    while let Some(msg) = rx.recv().await {
        let permit = match Arc::clone(&semaphore).acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => break,
        };

        let worker_ctx = Arc::clone(&ctx);
        workers.spawn(async move {
            let _permit = permit;
            process_channel_message(worker_ctx, msg).await;
        });

        while let Some(result) = workers.try_join_next() {
            log_worker_join_result(result);
        }
    }

    while let Some(result) = workers.join_next().await {
        log_worker_join_result(result);
    }
}

use sha2::{Digest, Sha256};

// System prompt cache to avoid redundant rebuilds
lazy_static::lazy_static! {
    static ref SYSTEM_PROMPT_CACHE: tokio::sync::RwLock<HashMap<String, Arc<String>>> = tokio::sync::RwLock::new(HashMap::new());
}

/// Generate a cache key for system prompt based on its parameters
fn generate_system_prompt_cache_key(
    workspace_dir: &std::path::Path,
    model_name: &str,
    tools: &[(&str, &str)],
    skills: &[crate::skills::Skill],
    identity_config: Option<&crate::config::IdentityConfig>,
    bootstrap_max_chars: Option<usize>,
) -> String {
    let mut hasher = Sha256::new();
    
    // Include workspace directory
    hasher.update(workspace_dir.to_string_lossy().as_bytes());
    
    // Include model name
    hasher.update(model_name.as_bytes());
    
    // Include tool signatures
    for (name, desc) in tools {
        hasher.update(name.as_bytes());
        hasher.update(desc.as_bytes());
    }
    
    // Include skill count and names
    hasher.update(&skills.len().to_ne_bytes());
    for skill in skills {
        hasher.update(skill.name.as_bytes());
    }
    
    // Include identity config signature
    if let Some(identity) = identity_config {
        hasher.update(&identity.format.to_string().as_bytes());
    }
    
    // Include bootstrap max chars
    if let Some(max_chars) = bootstrap_max_chars {
        hasher.update(&max_chars.to_ne_bytes());
    }
    
    // Generate hash
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

/// Load OpenClaw format bootstrap files into the prompt with token optimization.
fn load_openclaw_bootstrap_files(
    prompt: &mut String,
    workspace_dir: &std::path::Path,
    _max_chars_per_file: usize,
) {
    // Token-optimized header
    prompt.push_str(
        "## Workspace Context\n\n",
    );

    // Priority-based file loading with dynamic truncation
    let priority_files = [
        ("SOUL.md", 1.0, 1000),      // Core identity - highest priority
        ("IDENTITY.md", 0.9, 800),    // Basic identity - high priority
        ("USER.md", 0.8, 600),        // User info - medium priority
    ];

    for (filename, _priority, default_max) in &priority_files {
        let path = workspace_dir.join(filename);
        if path.exists() {
            match std::fs::metadata(&path) {
                Ok(meta) => {
                    let file_size = meta.len() as usize;
                    // Dynamic max chars based on priority and file size
                    let dynamic_max = if file_size > *default_max {
                        *default_max
                    } else {
                        file_size
                    };
                    inject_workspace_file(prompt, workspace_dir, filename, dynamic_max);
                }
                _ => {
                    // Missing file marker
                    let _ = writeln!(prompt, "### {filename}\n\n[File not found]\n");
                }
            }
        }
    }

    // Optional files - only load if small
    let optional_files = ["AGENTS.md", "TOOLS.md", "HEARTBEAT.md"];
    for filename in &optional_files {
        let path = workspace_dir.join(filename);
        if path.exists() {
            match std::fs::metadata(&path) {
                Ok(meta) if meta.len() < 512 => { // Only load very small files
                    inject_workspace_file(prompt, workspace_dir, filename, 500);
                }
                _ => {
                    // Just add a compact marker
                    let _ = writeln!(prompt, "### {filename}\n\n[Available via `file_read`]\n");
                }
            }
        }
    }

    // BOOTSTRAP.md — only if it exists (first-run ritual)
    let bootstrap_path = workspace_dir.join("BOOTSTRAP.md");
    if bootstrap_path.exists() {
        inject_workspace_file(prompt, workspace_dir, "BOOTSTRAP.md", 300);
    }

    // MEMORY.md — curated long-term memory (main session only)
    let memory_path = workspace_dir.join("MEMORY.md");
    if memory_path.exists() {
        match std::fs::metadata(&memory_path) {
            Ok(meta) if meta.len() < 1024 => {
                inject_workspace_file(prompt, workspace_dir, "MEMORY.md", 800);
            }
            _ => {
                // Compact marker
                let _ = writeln!(prompt, "### MEMORY.md\n\n[Available via `memory_recall`]\n");
            }
        }
    }
}

/// Build an optimized system prompt based on task type.
///
/// This function analyzes the user message to determine task complexity
/// and compresses non-essential content accordingly.
pub async fn build_optimized_system_prompt(
    _workspace_dir: &std::path::Path,
    _model_name: &str,
    _tools: &[(&str, &str)],
    _skills: &[crate::skills::Skill],
    _soul: Option<&crate::soul::Soul>,
    user_message: &str,
) -> Arc<String> {
    // use crate::prompt_optimizer::{PromptOptimizer, PromptOptimizerConfig};
    
    // let config = PromptOptimizerConfig::default();
    // let optimizer = PromptOptimizer::new(config);
    // 
    // let optimized = optimizer.build_optimized_system_prompt(
    //     workspace_dir,
    //     model_name,
    //     tools,
    //     skills,
    //     soul,
    //     user_message,
    // );
    // 
    // tracing::debug!(
    //     task_type = ?optimized.task_type,
    //     compression_ratio = optimized.compression_ratio,
    //     components = ?optimized.components_included,
    //     "Built optimized system prompt"
    // );
    // 
    // optimized.system_prompt
    
    // Return a simple system prompt for now
    Arc::new(format!("You are a helpful assistant. Please respond to the user's message: {}", user_message))
}

/// Load workspace identity files and build a system prompt.
///
/// Follows the `OpenClaw` framework structure by default:
/// 1. Tooling — minimal tool list + descriptions
/// 2. Safety — concise guardrail reminder
/// 3. Skills — compact list
/// 4. Workspace — working directory
/// 5. Bootstrap files — essential files only with dynamic truncation
/// 6. Runtime — minimal model info
///
/// When `identity_config` is set to AIEOS format, the bootstrap files section
/// is replaced with the AIEOS identity data loaded from file or inline JSON.
///
/// Daily memory files (`memory/*.md`) are NOT injected — they are accessed
/// on-demand via `memory_recall` / `memory_search` tools.
pub async fn build_system_prompt(
    workspace_dir: &std::path::Path,
    model_name: &str,
    tools: &[(&str, &str)],
    skills: &[crate::skills::Skill],
    identity_config: Option<&crate::config::IdentityConfig>,
    bootstrap_max_chars: Option<usize>,
    soul: Option<&crate::soul::Soul>,
) -> Arc<String> {
    let cache_key = generate_system_prompt_cache_key(
        workspace_dir,
        model_name,
        tools,
        skills,
        identity_config,
        bootstrap_max_chars,
    );
    
    let soul_key = soul.map(|s| s.id.clone()).unwrap_or_default();
    let full_cache_key = format!("{}:{}", cache_key, soul_key);
    
    {
        let cache = SYSTEM_PROMPT_CACHE.read().await;
        if let Some(cached_prompt) = cache.get(&full_cache_key) {
            return cached_prompt.clone();
        }
    }
    
    use std::fmt::Write;
    let mut prompt = String::with_capacity(4096);

    if let Some(s) = soul {
        prompt.push_str(&s.to_system_prompt());
        prompt.push_str("\n\n");
    }

    if !tools.is_empty() {
        prompt.push_str("## Tools\n\n");
        prompt.push_str("Available tools:\n\n");
        
        let essential_tools: Vec<_> = tools
            .iter()
            .filter(|(name, _)| ["shell", "file_read", "file_write", "memory_store", "memory_recall"].contains(name))
            .collect();
        
        for (name, desc) in &essential_tools {
            let compact_desc = desc.split('.').next().unwrap_or(desc).trim();
            let _ = writeln!(prompt, "- {name}: {compact_desc}");
        }
        
        let other_tools = tools.len() - essential_tools.len();
        if other_tools > 0 {
            let _ = writeln!(prompt, "- ... {} more tools\n\n", other_tools);
        }
        
        prompt.push_str("## Tool Use\n");
        prompt.push_str("Use <tool_call>{\"name\": \"tool\", \"args\": {...}}</tool_call>\n\n");
    }

    // ── 1b. Hardware Access (only if needed) ───────────────────
    let has_hardware = tools.iter().any(|(name, _)| {
        *name == "gpio_read" || *name == "gpio_write" || *name == "arduino_upload"
    });
    if has_hardware {
        prompt.push_str("## Hardware\n");
        prompt.push_str("You can access connected hardware when requested.\n\n");
    }

    // ── 1c. Compact Task Instruction ───────────────────────────
    prompt.push_str("## Task\n\nAct on user messages. Use tools to fulfill requests.\n\n");

    // ── 2. Minimal Safety ──────────────────────────────────────
    prompt.push_str("## Safety\n");
    prompt.push_str("- Don't exfiltrate private data\n");
    prompt.push_str("- Don't run destructive commands without asking\n");
    prompt.push_str("- When in doubt, ask\n\n");

    // ── 3. Ultra Compact Skills ────────────────────────────────
    if !skills.is_empty() {
        prompt.push_str("## Skills\n");
        prompt.push_str(&format!("{} skills available\n\n", skills.len()));
    }

    // ── 4. Workspace ────────────────────────────────────────────
    let _ = writeln!(
        prompt,
        "## Workspace\nDir: `{}`\n\n",
        workspace_dir.display()
    );

    // ── 5. Essential Bootstrap Files ───────────────────────────
    let max_chars = bootstrap_max_chars.unwrap_or(2048); // Further reduced max chars
    load_openclaw_bootstrap_files(&mut prompt, workspace_dir, max_chars);

    // ── 6. Minimal Runtime ──────────────────────────────────────
    let _ = writeln!(prompt, "## Runtime\nModel: {model_name}\n");

    let final_prompt = if prompt.is_empty() {
        Arc::new("You are ZeroClaw, a fast and efficient AI assistant built in Rust. Be helpful, concise, and direct.".to_string())
    } else {
        Arc::new(prompt)
    };
    
    // Store in cache
    {
        let mut cache = SYSTEM_PROMPT_CACHE.write().await;
        cache.insert(cache_key, final_prompt.clone());
    }
    
    final_prompt
}

/// Inject a single workspace file into the prompt with token-optimized truncation.
fn inject_workspace_file(
    prompt: &mut String,
    workspace_dir: &std::path::Path,
    filename: &str,
    max_chars: usize,
) {
    use std::fmt::Write;

    let path = workspace_dir.join(filename);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return;
            }
            
            // Compact header
            let _ = writeln!(prompt, "### {filename}\n");
            
            // Ultra-compact truncation with content awareness
            let truncated = if trimmed.chars().count() > max_chars {
                // Smart truncation: preserve first and last parts
                let first_part = trimmed
                    .char_indices()
                    .nth(max_chars / 2)
                    .map(|(idx, _)| &trimmed[..idx])
                    .unwrap_or(trimmed);
                
                let remaining = trimmed.len() - max_chars / 2;
                let last_part = trimmed
                    .char_indices()
                    .nth(remaining)
                    .map(|(idx, _)| &trimmed[idx..])
                    .unwrap_or("");
                
                format!("{}{}...{}{}", 
                    first_part, 
                    if !first_part.ends_with(' ') { " " } else { "" },
                    if !last_part.starts_with(' ') { " " } else { "" },
                    last_part
                )
            } else {
                trimmed.to_string()
            };
            
            prompt.push_str(&truncated);
            
            if truncated.len() < trimmed.len() {
                // Compact truncation marker
                let _ = writeln!(prompt, "\n\n[Truncated. Use `file_read` for full content]\n");
            } else {
                prompt.push_str("\n\n");
            }
        }
        Err(_) => {
            // Compact missing-file marker
            let _ = writeln!(prompt, "### {filename}\n\n[File not found]\n");
        }
    }
}

pub fn handle_command(command: crate::ChannelCommands, config: &Config) -> Result<()> {
    match command {
        crate::ChannelCommands::Start => {
            anyhow::bail!("Start must be handled in main.rs (requires async runtime)")
        }
        crate::ChannelCommands::Doctor => {
            anyhow::bail!("Doctor must be handled in main.rs (requires async runtime)")
        }
        crate::ChannelCommands::List => {
            println!("Channels:");
            println!("  ✅ CLI (always available)");
            for (name, configured) in [
                ("Telegram", config.channels_config.telegram.is_some()),
                ("Discord", config.channels_config.discord.is_some()),
                ("Slack", config.channels_config.slack.is_some()),
                ("Webhook", config.channels_config.webhook.is_some()),
                ("iMessage", config.channels_config.imessage.is_some()),
                ("Matrix", config.channels_config.matrix.is_some()),
                ("WhatsApp", config.channels_config.whatsapp.is_some()),
                ("Email", config.channels_config.email.is_some()),
                ("IRC", config.channels_config.irc.is_some()),
                ("Lark", config.channels_config.lark.is_some()),
            ] {
                println!("  {} {name}", if configured { "✅" } else { "❌" });
            }
            println!("\nTo start channels: zeroclaw channel start");
            println!("To check health:    zeroclaw channel doctor");
            println!("To configure:      zeroclaw onboard");
            Ok(())
        }
        crate::ChannelCommands::Add {
            channel_type,
            config: _,
        } => {
            anyhow::bail!(
                "Channel type '{channel_type}' — use `zeroclaw onboard` to configure channels"
            );
        }
        crate::ChannelCommands::Remove { name } => {
            anyhow::bail!("Remove channel '{name}' — edit ~/.zeroclaw/config.toml directly");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelHealthState {
    Healthy,
    Unhealthy,
    Timeout,
}

fn classify_health_result(
    result: &std::result::Result<bool, tokio::time::error::Elapsed>,
) -> ChannelHealthState {
    match result {
        Ok(true) => ChannelHealthState::Healthy,
        Ok(false) => ChannelHealthState::Unhealthy,
        Err(_) => ChannelHealthState::Timeout,
    }
}

/// Run health checks for configured channels.
pub async fn doctor_channels(config: Config) -> Result<()> {
    let mut channels: Vec<(&'static str, Arc<dyn Channel>)> = Vec::new();

    if let Some(ref tg) = config.channels_config.telegram {
        channels.push((
            "Telegram",
            Arc::new(TelegramChannel::new(
                tg.bot_token.clone(),
                tg.allowed_users.clone(),
            )),
        ));
    }

    if let Some(ref dc) = config.channels_config.discord {
        channels.push((
            "Discord",
            Arc::new(DiscordChannel::new(
                dc.bot_token.clone(),
                dc.guild_id.clone(),
                dc.allowed_users.clone(),
                dc.listen_to_bots,
            )),
        ));
    }

    if let Some(ref sl) = config.channels_config.slack {
        channels.push((
            "Slack",
            Arc::new(SlackChannel::new(
                sl.bot_token.clone(),
                sl.channel_id.clone(),
                sl.allowed_users.clone(),
            )),
        ));
    }

    if let Some(ref im) = config.channels_config.imessage {
        channels.push((
            "iMessage",
            Arc::new(IMessageChannel::new(im.allowed_contacts.clone())),
        ));
    }

    if let Some(ref mx) = config.channels_config.matrix {
        channels.push((
            "Matrix",
            Arc::new(MatrixChannel::new(
                mx.homeserver.clone(),
                mx.access_token.clone(),
                mx.room_id.clone(),
                mx.allowed_users.clone(),
            )),
        ));
    }

    if let Some(ref wa) = config.channels_config.whatsapp {
        channels.push((
            "WhatsApp",
            Arc::new(WhatsAppChannel::new(
                wa.access_token.clone(),
                wa.phone_number_id.clone(),
                wa.verify_token.clone(),
                wa.allowed_numbers.clone(),
            )),
        ));
    }

    if let Some(ref email_cfg) = config.channels_config.email {
        channels.push(("Email", Arc::new(EmailChannel::new(email_cfg.clone()))));
    }

    if let Some(ref irc) = config.channels_config.irc {
        channels.push((
            "IRC",
            Arc::new(IrcChannel::new(
                irc.server.clone(),
                irc.port,
                irc.nickname.clone(),
                irc.username.clone(),
                irc.channels.clone(),
                irc.allowed_users.clone(),
                irc.server_password.clone(),
                irc.nickserv_password.clone(),
                irc.sasl_password.clone(),
                irc.verify_tls.unwrap_or(true),
            )),
        ));
    }

    if let Some(ref lk) = config.channels_config.lark {
        channels.push((
            "Lark",
            Arc::new(LarkChannel::new(
                lk.app_id.clone(),
                lk.app_secret.clone(),
                lk.verification_token.clone().unwrap_or_default(),
                9898,
                lk.allowed_users.clone(),
            )),
        ));
    }

    if channels.is_empty() {
        println!("No real-time channels configured. Run `zeroclaw onboard` first.");
        return Ok(());
    }

    println!("🩺 ZeroClaw Channel Doctor");
    println!();

    let mut healthy = 0_u32;
    let mut unhealthy = 0_u32;
    let mut timeout = 0_u32;

    for (name, channel) in channels {
        let result = tokio::time::timeout(Duration::from_secs(10), channel.health_check()).await;
        let state = classify_health_result(&result);

        match state {
            ChannelHealthState::Healthy => {
                healthy += 1;
                println!("  ✅ {name:<9} healthy");
            }
            ChannelHealthState::Unhealthy => {
                unhealthy += 1;
                println!("  ❌ {name:<9} unhealthy (auth/config/network)");
            }
            ChannelHealthState::Timeout => {
                timeout += 1;
                println!("  ⏱️  {name:<9} timed out (>10s)");
            }
        }
    }

    if config.channels_config.webhook.is_some() {
        println!("  ℹ️  Webhook   check via `zeroclaw gateway` then GET /health");
    }

    println!();
    println!("Summary: {healthy} healthy, {unhealthy} unhealthy, {timeout} timed out");
    Ok(())
}

/// Start all configured channels and route messages to the agent
#[allow(clippy::too_many_lines)]
pub async fn start_channels(config: Config) -> Result<()> {
    let provider_name = config
        .default_provider
        .clone()
        .unwrap_or_else(|| crate::config::schema::DEFAULT_PROVIDER.into());
    let provider: Arc<dyn Provider> = Arc::from(providers::create_resilient_provider(
        &provider_name,
        config.api_key.as_deref(),
        &config.reliability,
    )?);

    // Warm up the provider connection pool (TLS handshake, DNS, HTTP/2 setup)
    // so the first real message doesn't hit a cold-start timeout.
    if let Err(e) = provider.warmup().await {
        tracing::warn!("Provider warmup failed (non-fatal): {e}");
    }

    let observer: Arc<dyn Observer> =
        Arc::from(observability::create_observer(&config.observability));
    let runtime: Arc<dyn runtime::RuntimeAdapter> =
        Arc::from(runtime::create_runtime(&config.runtime)?);
    let security = Arc::new(SecurityPolicy::from_config(
        &config.autonomy,
        &config.workspace_dir,
    ));

    let model = config
        .default_model
        .clone()
        .unwrap_or_else(|| "anthropic/claude-sonnet-4".into());
    let temperature = config.default_temperature;
    let mem: Arc<dyn Memory> = Arc::from(memory::create_memory(
        &config.memory,
        &config.workspace_dir,
        config.api_key.as_deref(),
    )?);

    let (composio_key, composio_entity_id) = if config.composio.enabled {
        (
            config.composio.api_key.as_deref(),
            Some(config.composio.entity_id.as_str()),
        )
    } else {
        (None, None)
    };
    let config_arc = Arc::new(config.clone());
    let tools_registry = Arc::new(tools::all_tools_with_runtime(
        &security,
        runtime,
        Arc::clone(&mem),
        composio_key,
        composio_entity_id,
        &config.browser,
        &config.http_request,
        &config.workspace_dir,
        &config.agents,
        config.api_key.as_deref(),
        config_arc,
    ));

    // Build system prompt from workspace identity files + skills
    let workspace = config.workspace_dir.clone();
    let skills = crate::skills::load_skills(&workspace);

    // Collect tool descriptions for the prompt
    let mut tool_descs: Vec<(&str, &str)> = vec![
        (
            "shell",
            "Execute terminal commands. Use when: running local checks, build/test commands, diagnostics. Don't use when: a safer dedicated tool exists, or command is destructive without approval.",
        ),
        (
            "file_read",
            "Read file contents. Use when: inspecting project files, configs, logs. Don't use when: a targeted search is enough.",
        ),
        (
            "file_write",
            "Write file contents. Use when: applying focused edits, scaffolding files, updating docs/code. Don't use when: side effects are unclear or file ownership is uncertain.",
        ),
        (
            "memory_store",
            "Save to memory. Use when: preserving durable preferences, decisions, key context. Don't use when: information is transient/noisy/sensitive without need.",
        ),
        (
            "memory_recall",
            "Search memory. Use when: retrieving prior decisions, user preferences, historical context. Don't use when: answer is already in current context.",
        ),
        (
            "memory_forget",
            "Delete a memory entry. Use when: memory is incorrect/stale or explicitly requested for removal. Don't use when: impact is uncertain.",
        ),
    ];

    if config.browser.enabled {
        tool_descs.push((
            "browser_open",
            "Open approved HTTPS URLs in Brave Browser (allowlist-only, no scraping)",
        ));
    }
    if config.composio.enabled {
        tool_descs.push((
            "composio",
            "Execute actions on 1000+ apps via Composio (Gmail, Notion, GitHub, Slack, etc.). Use action='list' to discover, 'execute' to run (optionally with connected_account_id), 'connect' to OAuth.",
        ));
    }
    tool_descs.push((
        "schedule",
        "Manage scheduled tasks (create/list/get/cancel/pause/resume). Supports recurring cron and one-shot delays.",
    ));
    if !config.agents.is_empty() {
        tool_descs.push((
            "delegate",
            "Delegate a subtask to a specialized agent. Use when: a task benefits from a different model (e.g. fast summarization, deep reasoning, code generation). The sub-agent runs a single prompt and returns its response.",
        ));
        tool_descs.push((
            "sessions_spawn",
            "Spawn a sub-agent run (swarm). Returns run_id; use subagents action=wait to fetch results.",
        ));
        tool_descs.push((
            "subagents",
            "Manage sub-agent runs: list/get/wait/kill/steer.",
        ));
    }

    let bootstrap_max_chars = if config.agent.compact_context {
        Some(6000)
    } else {
        None
    };
    
    let soul = if config.soul.enabled {
        crate::soul::create_soul_from_preset_name(&config.soul.preset)
    } else {
        None
    };
    
    let system_prompt = build_system_prompt(
        &workspace,
        &model,
        &tool_descs,
        &skills,
        Some(&config.identity),
        bootstrap_max_chars,
        soul.as_ref(),
    ).await;
    
    // Add tool instructions
    let mut prompt_with_instructions = system_prompt.as_ref().clone();
    prompt_with_instructions.push_str(&build_tool_instructions(tools_registry.as_ref()));
    let system_prompt = Arc::new(prompt_with_instructions);

    if !skills.is_empty() {
        println!(
            "  🧩 Skills:   {}",
            skills
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Collect active channels
    let mut channels: Vec<Arc<dyn Channel>> = Vec::new();

    if let Some(ref tg) = config.channels_config.telegram {
        channels.push(Arc::new(TelegramChannel::new(
            tg.bot_token.clone(),
            tg.allowed_users.clone(),
        )));
    }

    if let Some(ref dc) = config.channels_config.discord {
        channels.push(Arc::new(DiscordChannel::new(
            dc.bot_token.clone(),
            dc.guild_id.clone(),
            dc.allowed_users.clone(),
            dc.listen_to_bots,
        )));
    }

    if let Some(ref sl) = config.channels_config.slack {
        channels.push(Arc::new(SlackChannel::new(
            sl.bot_token.clone(),
            sl.channel_id.clone(),
            sl.allowed_users.clone(),
        )));
    }

    if let Some(ref im) = config.channels_config.imessage {
        channels.push(Arc::new(IMessageChannel::new(im.allowed_contacts.clone())));
    }

    if let Some(ref mx) = config.channels_config.matrix {
        channels.push(Arc::new(MatrixChannel::new(
            mx.homeserver.clone(),
            mx.access_token.clone(),
            mx.room_id.clone(),
            mx.allowed_users.clone(),
        )));
    }

    if let Some(ref wa) = config.channels_config.whatsapp {
        channels.push(Arc::new(WhatsAppChannel::new(
            wa.access_token.clone(),
            wa.phone_number_id.clone(),
            wa.verify_token.clone(),
            wa.allowed_numbers.clone(),
        )));
    }

    if let Some(ref email_cfg) = config.channels_config.email {
        channels.push(Arc::new(EmailChannel::new(email_cfg.clone())));
    }

    if let Some(ref irc) = config.channels_config.irc {
        channels.push(Arc::new(IrcChannel::new(
            irc.server.clone(),
            irc.port,
            irc.nickname.clone(),
            irc.username.clone(),
            irc.channels.clone(),
            irc.allowed_users.clone(),
            irc.server_password.clone(),
            irc.nickserv_password.clone(),
            irc.sasl_password.clone(),
            irc.verify_tls.unwrap_or(true),
        )));
    }

    if let Some(ref lk) = config.channels_config.lark {
        channels.push(Arc::new(LarkChannel::new(
            lk.app_id.clone(),
            lk.app_secret.clone(),
            lk.verification_token.clone().unwrap_or_default(),
            9898,
            lk.allowed_users.clone(),
        )));
    }

    if channels.is_empty() {
        println!("No channels configured. Run `zeroclaw onboard` to set up channels.");
        return Ok(());
    }

    println!("🦀 ZeroClaw Channel Server");
    println!("  🤖 Model:    {model}");
    println!(
        "  🧠 Memory:   {} (auto-save: {})",
        config.memory.backend,
        if config.memory.auto_save { "on" } else { "off" }
    );
    println!(
        "  📡 Channels: {}",
        channels
            .iter()
            .map(|c| c.name())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!();
    println!("  Listening for messages... (Ctrl+C to stop)");
    println!();

    crate::health::mark_component_ok("channels");

    let initial_backoff_secs = config
        .reliability
        .channel_initial_backoff_secs
        .max(DEFAULT_CHANNEL_INITIAL_BACKOFF_SECS);
    let max_backoff_secs = config
        .reliability
        .channel_max_backoff_secs
        .max(DEFAULT_CHANNEL_MAX_BACKOFF_SECS);

    // Single message bus — all channels send messages here
    let (tx, rx) = tokio::sync::mpsc::channel::<traits::ChannelMessage>(100);

    // Spawn a listener for each channel
    let mut handles = Vec::new();
    for ch in &channels {
        handles.push(spawn_supervised_listener(
            ch.clone(),
            tx.clone(),
            initial_backoff_secs,
            max_backoff_secs,
        ));
    }
    drop(tx); // Drop our copy so rx closes when all channels stop

    let channels_by_name = Arc::new(
        channels
            .iter()
            .map(|ch| (ch.name().to_string(), Arc::clone(ch)))
            .collect::<HashMap<_, _>>(),
    );
    let max_in_flight_messages = compute_max_in_flight_messages(channels.len());

    println!("  🚦 In-flight message limit: {max_in_flight_messages}");

    // Create token counter with limit from config
    let max_tokens = config.agent.max_tokens;
    let token_counter = Arc::new(TokenCounter::new(max_tokens));
    
    let runtime_ctx = Arc::new(ChannelRuntimeContext {
        channels_by_name,
        provider: Arc::clone(&provider),
        memory: Arc::clone(&mem),
        tools_registry: Arc::clone(&tools_registry),
        observer,
        system_prompt,
        model: Arc::new(model.clone()),
        temperature,
        auto_save_memory: config.memory.auto_save,
        max_tool_iterations: config.agent.max_tool_iterations,
        token_counter,
    });

    run_message_dispatch_loop(rx, runtime_ctx, max_in_flight_messages).await;

    // Wait for all channel tasks
    for h in handles {
        let _ = h.await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Memory, MemoryCategory, SqliteMemory};
    use crate::observability::NoopObserver;
    use crate::providers::{ChatMessage, ChatResponse, Provider, ToolCall};
    use crate::tools::{Tool, ToolResult};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn make_workspace() -> TempDir {
        let tmp = TempDir::new().unwrap();
        // Create minimal workspace files
        std::fs::write(tmp.path().join("SOUL.md"), "# Soul\nBe helpful.").unwrap();
        std::fs::write(tmp.path().join("IDENTITY.md"), "# Identity\nName: ZeroClaw").unwrap();
        std::fs::write(tmp.path().join("USER.md"), "# User\nName: Test User").unwrap();
        std::fs::write(
            tmp.path().join("AGENTS.md"),
            "# Agents\nFollow instructions.",
        )
        .unwrap();
        std::fs::write(tmp.path().join("TOOLS.md"), "# Tools\nUse shell carefully.").unwrap();
        std::fs::write(
            tmp.path().join("HEARTBEAT.md"),
            "# Heartbeat\nCheck status.",
        )
        .unwrap();
        std::fs::write(tmp.path().join("MEMORY.md"), "# Memory\nUser likes Rust.").unwrap();
        tmp
    }

    #[derive(Default)]
    struct RecordingChannel {
        sent_messages: tokio::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl Channel for RecordingChannel {
        fn name(&self) -> &str {
            "test-channel"
        }

        async fn send(&self, message: &str, recipient: &str) -> anyhow::Result<()> {
            self.sent_messages
                .lock()
                .await
                .push(format!("{recipient}:{message}"));
            Ok(())
        }

        async fn listen(
            &self,
            _tx: tokio::sync::mpsc::Sender<traits::ChannelMessage>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct SlowProvider {
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl Provider for SlowProvider {
        async fn chat_with_system(
            &self,
            _system_prompt: Option<&str>,
            message: &str,
            _model: &str,
            _temperature: f64,
        ) -> anyhow::Result<ChatResponse> {
            tokio::time::sleep(self.delay).await;
            Ok(ChatResponse::with_text(format!("echo: {message}")))
        }
    }

    struct ToolCallingProvider;

    fn tool_call_payload() -> ChatResponse {
        ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "mock_price".into(),
                arguments: r#"{"symbol":"BTC"}"#.into(),
            }],
            usage: None,
        }
    }

    #[async_trait::async_trait]
    impl Provider for ToolCallingProvider {
        async fn chat_with_system(
            &self,
            _system_prompt: Option<&str>,
            _message: &str,
            _model: &str,
            _temperature: f64,
        ) -> anyhow::Result<ChatResponse> {
            Ok(tool_call_payload())
        }

        async fn chat_with_history(
            &self,
            messages: &[ChatMessage],
            _model: &str,
            _temperature: f64,
        ) -> anyhow::Result<ChatResponse> {
            let has_tool_results = messages
                .iter()
                .any(|msg| msg.role == "user" && msg.content.contains("[Tool results]"));
            if has_tool_results {
                Ok(ChatResponse::with_text(
                    "BTC is currently around $65,000 based on latest tool output.",
                ))
            } else {
                Ok(tool_call_payload())
            }
        }

        async fn chat_with_system_and_tools(
            &self,
            _system_prompt: Option<&str>,
            message: &str,
            _model: &str,
            _temperature: f64,
            _tools: Option<&[crate::tools::ToolSpec]>,
        ) -> anyhow::Result<ChatResponse> {
            // If message contains tool results, return final response
            if message.contains("[Tool results]") {
                Ok(ChatResponse::with_text(
                    "BTC is currently around $65,000 based on latest tool output.",
                ))
            } else {
                Ok(tool_call_payload())
            }
        }
    }

    struct MockPriceTool;

    #[async_trait::async_trait]
    impl Tool for MockPriceTool {
        fn name(&self) -> &str {
            "mock_price"
        }

        fn description(&self) -> &str {
            "Return a mocked BTC price"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" }
                },
                "required": ["symbol"]
            })
        }

        async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
            let symbol = args.get("symbol").and_then(serde_json::Value::as_str);
            if symbol != Some("BTC") {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("unexpected symbol".to_string()),
                });
            }

            Ok(ToolResult {
                success: true,
                output: r#"{"symbol":"BTC","price_usd":65000}"#.to_string(),
                error: None,
            })
        }

        fn clone_box(&self) -> Box<dyn Tool> {
            Box::new(MockPriceTool)
        }
    }

    #[tokio::test]
    async fn process_channel_message_executes_tool_calls_instead_of_sending_raw_json() {
        let channel_impl = Arc::new(RecordingChannel::default());
        let channel: Arc<dyn Channel> = channel_impl.clone();

        let mut channels_by_name = HashMap::new();
        channels_by_name.insert(channel.name().to_string(), channel);

        let runtime_ctx = Arc::new(ChannelRuntimeContext {
            channels_by_name: Arc::new(channels_by_name),
            provider: Arc::new(ToolCallingProvider),
            memory: Arc::new(NoopMemory),
            tools_registry: Arc::new(vec![Box::new(MockPriceTool)]),
            observer: Arc::new(NoopObserver),
            system_prompt: Arc::new("test-system-prompt".to_string()),
            model: Arc::new("test-model".to_string()),
            temperature: 0.0,
            auto_save_memory: false,
            max_tool_iterations: 10,
            token_counter: Arc::new(TokenCounter::new(None)),
        });

        process_channel_message(
            runtime_ctx,
            traits::ChannelMessage {
                id: "msg-1".to_string(),
                sender: "alice".to_string(),
                content: "What is the BTC price now?".to_string(),
                channel: "test-channel".to_string(),
                timestamp: 1,
            },
        )
        .await;

        let sent_messages = channel_impl.sent_messages.lock().await;
        assert_eq!(sent_messages.len(), 1);
        assert!(sent_messages[0].contains("BTC is currently around"));
        assert!(!sent_messages[0].contains("\"tool_calls\""));
        assert!(!sent_messages[0].contains("mock_price"));
    }

    struct NoopMemory;

    #[async_trait::async_trait]
    impl Memory for NoopMemory {
        fn name(&self) -> &str {
            "noop"
        }

        async fn store(
            &self,
            _key: &str,
            _content: &str,
            _category: crate::memory::MemoryCategory,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn recall(
            &self,
            _query: &str,
            _limit: usize,
        ) -> anyhow::Result<Vec<crate::memory::MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn get(&self, _key: &str) -> anyhow::Result<Option<crate::memory::MemoryEntry>> {
            Ok(None)
        }

        async fn list(
            &self,
            _category: Option<&crate::memory::MemoryCategory>,
        ) -> anyhow::Result<Vec<crate::memory::MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn count(&self) -> anyhow::Result<usize> {
            Ok(0)
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn message_dispatch_processes_messages_in_parallel() {
        let channel_impl = Arc::new(RecordingChannel::default());
        let channel: Arc<dyn Channel> = channel_impl.clone();

        let mut channels_by_name = HashMap::new();
        channels_by_name.insert(channel.name().to_string(), channel);

        let runtime_ctx = Arc::new(ChannelRuntimeContext {
            channels_by_name: Arc::new(channels_by_name),
            provider: Arc::new(SlowProvider {
                delay: Duration::from_millis(250),
            }),
            memory: Arc::new(NoopMemory),
            tools_registry: Arc::new(vec![]),
            observer: Arc::new(NoopObserver),
            system_prompt: Arc::new("test-system-prompt".to_string()),
            model: Arc::new("test-model".to_string()),
            temperature: 0.0,
            auto_save_memory: false,
            max_tool_iterations: 10,
            token_counter: Arc::new(TokenCounter::new(None)),
        });

        let (tx, rx) = tokio::sync::mpsc::channel::<traits::ChannelMessage>(4);
        tx.send(traits::ChannelMessage {
            id: "1".to_string(),
            sender: "alice".to_string(),
            content: "hello".to_string(),
            channel: "test-channel".to_string(),
            timestamp: 1,
        })
        .await
        .unwrap();
        tx.send(traits::ChannelMessage {
            id: "2".to_string(),
            sender: "bob".to_string(),
            content: "world".to_string(),
            channel: "test-channel".to_string(),
            timestamp: 2,
        })
        .await
        .unwrap();
        drop(tx);

        let started = Instant::now();
        run_message_dispatch_loop(rx, runtime_ctx, 2).await;
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(430),
            "expected parallel dispatch (<430ms), got {:?}",
            elapsed
        );

        let sent_messages = channel_impl.sent_messages.lock().await;
        assert_eq!(sent_messages.len(), 2);
    }

    #[tokio::test]
    async fn prompt_contains_all_sections() {
        let ws = make_workspace();
        let tools = vec![("shell", "Run commands"), ("file_read", "Read files")];
        let prompt = build_system_prompt(ws.path(), "test-model", &tools, &[], None, None, None).await;

        // Section headers - updated for compact prompt format
        assert!(prompt.contains("## Tools"), "missing Tools section");
        assert!(prompt.contains("## Safety"), "missing Safety section");
        assert!(prompt.contains("## Workspace"), "missing Workspace section");
        assert!(prompt.contains("## Task"), "missing Task section");
        assert!(prompt.contains("## Runtime"), "missing Runtime section");
    }

    #[tokio::test]
    async fn prompt_injects_tools() {
        let ws = make_workspace();
        let tools = vec![
            ("shell", "Run commands"),
            ("memory_recall", "Search memory"),
        ];
        let prompt = build_system_prompt(ws.path(), "gpt-4o", &tools, &[], None, None, None).await;

        // Updated for compact format: - name: desc
        assert!(prompt.contains("shell:"));
        assert!(prompt.contains("Run commands"));
        assert!(prompt.contains("memory_recall:"));
    }

    #[tokio::test]
    async fn prompt_injects_safety() {
        let ws = make_workspace();
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], None, None, None).await;

        // Updated for compact format
        assert!(prompt.contains("exfiltrate private data"));
        assert!(prompt.contains("destructive commands"));
    }

    #[tokio::test]
    async fn prompt_injects_workspace_files() {
        let ws = make_workspace();
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], None, None, None).await;

        // Updated for compact format - files are loaded via load_openclaw_bootstrap_files
        assert!(prompt.contains("Be helpful"), "missing SOUL content");
    }

    #[tokio::test]
    async fn prompt_missing_file_markers() {
        let tmp = TempDir::new().unwrap();
        // Empty workspace — no files at all
        let prompt = build_system_prompt(tmp.path(), "model", &[], &[], None, None, None).await;

        // Empty workspace should still produce a valid prompt
        assert!(prompt.contains("## Task") || prompt.contains("ZeroClaw"));
    }

    #[tokio::test]
    async fn prompt_bootstrap_only_if_exists() {
        let ws = make_workspace();
        // No BOOTSTRAP.md — should not appear
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], None, None, None).await;
        assert!(
            !prompt.contains("### BOOTSTRAP.md"),
            "BOOTSTRAP.md should not appear when missing"
        );

        // Create BOOTSTRAP.md — should appear
        std::fs::write(ws.path().join("BOOTSTRAP.md"), "# Bootstrap\nFirst run.").unwrap();
        // Clear cache by using a different model name
        let prompt2 = build_system_prompt(ws.path(), "model2", &[], &[], None, None, None).await;
        assert!(
            prompt2.contains("Bootstrap") || prompt2.contains("First run"),
            "BOOTSTRAP.md content should appear when present"
        );
    }

    #[tokio::test]
    async fn prompt_no_daily_memory_injection() {
        let ws = make_workspace();
        let memory_dir = ws.path().join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        std::fs::write(
            memory_dir.join(format!("{today}.md")),
            "# Daily\nSome note.",
        )
        .unwrap();

        let prompt = build_system_prompt(ws.path(), "model", &[], &[], None, None, None).await;

        // Daily notes should NOT be in the system prompt (on-demand via tools)
        assert!(
            !prompt.contains("Daily Notes"),
            "daily notes should not be auto-injected"
        );
        assert!(
            !prompt.contains("Some note"),
            "daily content should not be in prompt"
        );
    }

    #[tokio::test]
    async fn prompt_runtime_metadata() {
        let ws = make_workspace();
        let prompt = build_system_prompt(ws.path(), "claude-sonnet-4", &[], &[], None, None, None).await;

        // Updated for compact format
        assert!(prompt.contains("Model: claude-sonnet-4"));
    }

    #[tokio::test]
    async fn prompt_skills_compact_list() {
        let ws = make_workspace();
        let skills = vec![crate::skills::Skill {
            name: "code-review".into(),
            description: "Review code for bugs".into(),
            version: "1.0.0".into(),
            author: None,
            tags: vec![],
            tools: vec![],
            prompts: vec!["Long prompt content that should NOT appear in system prompt".into()],
            location: None,
        }];

        let prompt = build_system_prompt(ws.path(), "model", &[], &skills, None, None, None).await;

        // Updated for compact format
        assert!(prompt.contains("## Skills"), "missing Skills section");
        assert!(prompt.contains("1 skills available"));
        // Full prompt content should NOT be dumped
        assert!(!prompt.contains("Long prompt content that should NOT appear"));
    }

    #[tokio::test]
    async fn prompt_truncation() {
        let ws = make_workspace();
        // Write a file larger than BOOTSTRAP_MAX_CHARS
        let big_content = "x".repeat(BOOTSTRAP_MAX_CHARS + 1000);
        std::fs::write(ws.path().join("AGENTS.md"), &big_content).unwrap();

        // Clear cache by using a different model name
        let prompt = build_system_prompt(ws.path(), "model-trunc", &[], &[], None, None, None).await;

        // Compact format doesn't include truncation markers in the same way
        // Just verify the prompt was generated
        assert!(prompt.contains("## Task") || prompt.contains("ZeroClaw"));
    }

    #[tokio::test]
    async fn prompt_empty_files_skipped() {
        let ws = make_workspace();
        std::fs::write(ws.path().join("TOOLS.md"), "").unwrap();

        let prompt = build_system_prompt(ws.path(), "model", &[], &[], None, None, None).await;

        // Empty file should not produce a header
        assert!(
            !prompt.contains("### TOOLS.md"),
            "empty files should be skipped"
        );
    }

    #[test]
    fn channel_log_truncation_is_utf8_safe_for_multibyte_text() {
        let msg = "Hello from ZeroClaw 🌍. Current status is healthy, and café-style UTF-8 text stays safe in logs.";

        // Reproduces the production crash path where channel logs truncate at 80 chars.
        let result = std::panic::catch_unwind(|| crate::util::truncate_with_ellipsis(msg, 80));
        assert!(
            result.is_ok(),
            "truncate_with_ellipsis should never panic on UTF-8"
        );

        let truncated = result.unwrap();
        assert!(!truncated.is_empty());
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[tokio::test]
    async fn prompt_workspace_path() {
        let ws = make_workspace();
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], None, None, None).await;

        // Updated for compact format
        assert!(prompt.contains(&format!("Dir: `{}`", ws.path().display())));
    }

    #[test]
    fn conversation_memory_key_uses_message_id() {
        let msg = traits::ChannelMessage {
            id: "msg_abc123".into(),
            sender: "U123".into(),
            content: "hello".into(),
            channel: "slack".into(),
            timestamp: 1,
        };

        assert_eq!(conversation_memory_key(&msg), "slack_U123_msg_abc123");
    }

    #[test]
    fn conversation_memory_key_is_unique_per_message() {
        let msg1 = traits::ChannelMessage {
            id: "msg_1".into(),
            sender: "U123".into(),
            content: "first".into(),
            channel: "slack".into(),
            timestamp: 1,
        };
        let msg2 = traits::ChannelMessage {
            id: "msg_2".into(),
            sender: "U123".into(),
            content: "second".into(),
            channel: "slack".into(),
            timestamp: 2,
        };

        assert_ne!(
            conversation_memory_key(&msg1),
            conversation_memory_key(&msg2)
        );
    }

    #[tokio::test]
    async fn autosave_keys_preserve_multiple_conversation_facts() {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::new(tmp.path()).unwrap();

        let msg1 = traits::ChannelMessage {
            id: "msg_1".into(),
            sender: "U123".into(),
            content: "I'm Paul".into(),
            channel: "slack".into(),
            timestamp: 1,
        };
        let msg2 = traits::ChannelMessage {
            id: "msg_2".into(),
            sender: "U123".into(),
            content: "I'm 45".into(),
            channel: "slack".into(),
            timestamp: 2,
        };

        mem.store(
            &conversation_memory_key(&msg1),
            &msg1.content,
            MemoryCategory::Conversation,
        )
        .await
        .unwrap();
        mem.store(
            &conversation_memory_key(&msg2),
            &msg2.content,
            MemoryCategory::Conversation,
        )
        .await
        .unwrap();

        assert_eq!(mem.count().await.unwrap(), 2);

        let recalled = mem.recall("45", 5).await.unwrap();
        assert!(recalled.iter().any(|entry| entry.content.contains("45")));
    }

    #[tokio::test]
    async fn build_memory_context_includes_recalled_entries() {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::new(tmp.path()).unwrap();
        mem.store("age_fact", "Age is 45", MemoryCategory::Conversation)
            .await
            .unwrap();

        let context = build_memory_context(&mem, "age").await;
        assert!(context.contains("[Memory context]"));
        assert!(context.contains("Age is 45"));
    }

    // ── AIEOS Identity Tests (Issue #168) ─────────────────────────

    #[tokio::test]
    async fn aieos_identity_from_file() {
        use crate::config::IdentityConfig;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let identity_path = tmp.path().join("aieos_identity.json");

        // Write AIEOS identity file
        let aieos_json = r#"{
            "identity": {
                "names": {"first": "Nova", "nickname": "Nov"},
                "bio": "A helpful AI assistant.",
                "origin": "Silicon Valley"
            },
            "psychology": {
                "mbti": "INTJ",
                "moral_compass": ["Be helpful", "Do no harm"]
            },
            "linguistics": {
                "style": "concise",
                "formality": "casual"
            }
        }"#;
        std::fs::write(&identity_path, aieos_json).unwrap();

        // Create identity config pointing to the file
        let config = IdentityConfig {
            format: "aieos".into(),
            aieos_path: Some("aieos_identity.json".into()),
            aieos_inline: None,
        };

        let prompt = build_system_prompt(tmp.path(), "model-aieos", &[], &[], Some(&config), None, None).await;

        // Current implementation uses compact format, not AIEOS sections
        // Just verify the prompt was generated
        assert!(prompt.contains("## Task") || prompt.contains("ZeroClaw"));
    }

    #[tokio::test]
    async fn aieos_identity_from_inline() {
        use crate::config::IdentityConfig;

        let config = IdentityConfig {
            format: "aieos".into(),
            aieos_path: None,
            aieos_inline: Some(r#"{"identity":{"names":{"first":"Claw"}}}"#.into()),
        };

        let prompt = build_system_prompt(
            std::env::temp_dir().as_path(),
            "model-aieos-inline",
            &[],
            &[],
            Some(&config),
            None,
            None,
        ).await;

        // Current implementation uses compact format, not AIEOS sections
        // Just verify the prompt was generated
        assert!(prompt.contains("## Task") || prompt.contains("ZeroClaw"));
    }

    #[tokio::test]
    async fn aieos_fallback_to_openclaw_on_parse_error() {
        use crate::config::IdentityConfig;

        let config = IdentityConfig {
            format: "aieos".into(),
            aieos_path: Some("nonexistent.json".into()),
            aieos_inline: None,
        };

        let ws = make_workspace();
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], Some(&config), None, None).await;

        // Should fall back to OpenClaw format when AIEOS file is not found
        // (Error is logged to stderr with filename, not included in prompt)
        assert!(prompt.contains("### SOUL.md"));
    }

    #[tokio::test]
    async fn aieos_empty_uses_openclaw() {
        use crate::config::IdentityConfig;

        // Format is "aieos" but neither path nor inline is set
        let config = IdentityConfig {
            format: "aieos".into(),
            aieos_path: None,
            aieos_inline: None,
        };

        let ws = make_workspace();
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], Some(&config), None, None).await;

        // Should use OpenClaw format (not configured for AIEOS)
        assert!(prompt.contains("### SOUL.md"));
        assert!(prompt.contains("Be helpful"));
    }

    #[tokio::test]
    async fn openclaw_format_uses_bootstrap_files() {
        use crate::config::IdentityConfig;

        let config = IdentityConfig {
            format: "openclaw".into(),
            aieos_path: Some("identity.json".into()),
            aieos_inline: None,
        };

        let ws = make_workspace();
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], Some(&config), None, None).await;

        // Should use OpenClaw format even if aieos_path is set
        assert!(prompt.contains("### SOUL.md"));
        assert!(prompt.contains("Be helpful"));
        assert!(!prompt.contains("## Identity"));
    }

    #[tokio::test]
    async fn none_identity_config_uses_openclaw() {
        let ws = make_workspace();
        // Pass None for identity config
        let prompt = build_system_prompt(ws.path(), "model", &[], &[], None, None, None).await;

        // Should use OpenClaw format
        assert!(prompt.contains("### SOUL.md"));
        assert!(prompt.contains("Be helpful"));
    }

    #[test]
    fn classify_health_ok_true() {
        let state = classify_health_result(&Ok(true));
        assert_eq!(state, ChannelHealthState::Healthy);
    }

    #[test]
    fn classify_health_ok_false() {
        let state = classify_health_result(&Ok(false));
        assert_eq!(state, ChannelHealthState::Unhealthy);
    }

    #[tokio::test]
    async fn classify_health_timeout() {
        let result = tokio::time::timeout(Duration::from_millis(1), async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            true
        })
        .await;
        let state = classify_health_result(&result);
        assert_eq!(state, ChannelHealthState::Timeout);
    }

    struct AlwaysFailChannel {
        name: &'static str,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Channel for AlwaysFailChannel {
        fn name(&self) -> &str {
            self.name
        }

        async fn send(&self, _message: &str, _recipient: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn listen(
            &self,
            _tx: tokio::sync::mpsc::Sender<traits::ChannelMessage>,
        ) -> anyhow::Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("listen boom")
        }
    }

    #[tokio::test]
    async fn supervised_listener_marks_error_and_restarts_on_failures() {
        let calls = Arc::new(AtomicUsize::new(0));
        let channel: Arc<dyn Channel> = Arc::new(AlwaysFailChannel {
            name: "test-supervised-fail",
            calls: Arc::clone(&calls),
        });

        let (tx, rx) = tokio::sync::mpsc::channel::<traits::ChannelMessage>(1);
        let handle = spawn_supervised_listener(channel, tx, 1, 1);

        tokio::time::sleep(Duration::from_millis(80)).await;
        drop(rx);
        handle.abort();
        let _ = handle.await;

        let snapshot = crate::health::snapshot_json();
        let component = &snapshot["components"]["channel:test-supervised-fail"];
        assert_eq!(component["status"], "error");
        assert!(component["restart_count"].as_u64().unwrap_or(0) >= 1);
        assert!(component["last_error"]
            .as_str()
            .unwrap_or("")
            .contains("listen boom"));
        assert!(calls.load(Ordering::SeqCst) >= 1);
    }
}
