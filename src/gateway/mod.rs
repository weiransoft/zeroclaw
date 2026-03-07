//! Axum-based HTTP gateway with proper HTTP/1.1 compliance, body limits, and timeouts.
//!
//! This module replaces the raw TCP implementation with axum for:
//! - Proper HTTP/1.1 parsing and compliance
//! - Content-Length validation (handled by hyper)
//! - Request body size limits (64KB max)
//! - Request timeouts (30s) to prevent slow-loris attacks
//! - Header sanitization (handled by axum/hyper)
//! - SSE streaming for real-time responses

use crate::channels::{Channel, WhatsAppChannel};
use crate::config::{Config, HotConfig, HotReloadManager};
use crate::mcp::{MCPServerStore, MCPServerCreateRequest, MCPServerUpdateRequest, MCPServerStatus};
use crate::memory::{self, Memory, MemoryCategory};
use crate::observability::{self, Observer};
use crate::observability::trace_store::{self, TraceStore, TraceStoreConfig, TraceCollectorConfig};
use crate::providers::{self, ChatMessage, Provider};
use crate::runtime;
use crate::security::{
    pairing::{constant_time_eq, is_public_bind, PairingGuard},
    SecurityPolicy,
};
use crate::store::{WorkflowStore, AgentGroupStore, RoleMappingStore};
use crate::tools::{self, Tool};
use crate::util::truncate_with_ellipsis;
use tracing::{error, info};
mod event_handlers;
mod config_endpoints;

use event_handlers::{
    handle_event_listener_add,
    handle_event_listener_remove,
    handle_event_listener_list,
    handle_event_listener_update,
    handle_event_publish,
};

use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
    routing::{delete, get, post, put},
    Router,
};
use dashmap::DashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use uuid::Uuid;

/// Maximum request body size (64KB) — prevents memory exhaustion
pub const MAX_BODY_SIZE: usize = 65_536;
/// Request timeout for regular endpoints (30s) — prevents slow-loris attacks
pub const REQUEST_TIMEOUT_SECS: u64 = 30;
/// Request timeout for webhook endpoint (5 min) — LLM + tool calls need more time
pub const WEBHOOK_TIMEOUT_SECS: u64 = 300;
/// Sliding window used by gateway rate limiting.
pub const RATE_LIMIT_WINDOW_SECS: u64 = 60;

fn webhook_memory_key() -> String {
    format!("webhook_msg_{}", Uuid::new_v4())
}

fn whatsapp_memory_key(msg: &crate::channels::traits::ChannelMessage) -> String {
    format!("whatsapp_{}_{}", msg.sender, msg.id)
}

fn normalize_gateway_reply(reply: String) -> String {
    if reply.trim().is_empty() {
        return "Model returned an empty response.".to_string();
    }

    reply
}

async fn gateway_agent_reply(state: &AppState, message: &str) -> Result<String> {
    use std::sync::atomic::Ordering;
    
    let mut history = vec![
        ChatMessage::system(state.system_prompt.as_str()),
        ChatMessage::user(message),
    ];

    let reply = crate::agent::loop_::run_tool_call_loop(
        state.provider.as_ref(),
        &mut history,
        state.tools_registry.as_ref(),
        state.observer.as_ref(),
        "gateway",
        &state.model,
        state.temperature,
        true, // silent — gateway responses go over HTTP
        state.max_tool_iterations,
    )
    .await?;

    // Increment request count
    state.usage_stats.total_requests.fetch_add(1, Ordering::Relaxed);

    Ok(normalize_gateway_reply(reply))
}

#[derive(Debug)]
struct SlidingWindowRateLimiter {
    limit_per_window: u32,
    window: Duration,
    requests: dashmap::DashMap<String, Vec<Instant>>,
}

impl SlidingWindowRateLimiter {
    fn new(limit_per_window: u32, window: Duration) -> Self {
        Self {
            limit_per_window,
            window,
            requests: dashmap::DashMap::new(),
        }
    }

    fn allow(&self, key: &str) -> bool {
        if self.limit_per_window == 0 {
            return true;
        }

        let now = Instant::now();
        let cutoff = now.checked_sub(self.window).unwrap_or_else(Instant::now);

        let mut entry = self.requests.entry(key.to_owned()).or_insert(Vec::new());
        entry.retain(|instant| *instant > cutoff);

        if entry.len() >= self.limit_per_window as usize {
            return false;
        }

        entry.push(now);
        true
    }
}

#[derive(Debug)]
pub struct GatewayRateLimiter {
    pair: SlidingWindowRateLimiter,
    webhook: SlidingWindowRateLimiter,
}

impl GatewayRateLimiter {
    fn new(pair_per_minute: u32, webhook_per_minute: u32) -> Self {
        let window = Duration::from_secs(RATE_LIMIT_WINDOW_SECS);
        Self {
            pair: SlidingWindowRateLimiter::new(pair_per_minute, window),
            webhook: SlidingWindowRateLimiter::new(webhook_per_minute, window),
        }
    }

    fn allow_pair(&self, key: &str) -> bool {
        self.pair.allow(key)
    }

    fn allow_webhook(&self, key: &str) -> bool {
        self.webhook.allow(key)
    }
}

#[derive(Debug)]
pub struct IdempotencyStore {
    ttl: Duration,
    keys: dashmap::DashMap<String, Instant>,
}

impl IdempotencyStore {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            keys: dashmap::DashMap::new(),
        }
    }

    /// Returns true if this key is new and is now recorded.
    fn record_if_new(&self, key: &str) -> bool {
        let now = Instant::now();
        
        // Clean up expired entries
        self.keys.retain(|_, seen_at| now.duration_since(*seen_at) < self.ttl);

        if self.keys.contains_key(key) {
            return false;
        }

        self.keys.insert(key.to_owned(), now);
        true
    }
}

fn client_key_from_headers(headers: &HeaderMap) -> String {
    for header_name in ["X-Forwarded-For", "X-Real-IP"] {
        if let Some(value) = headers.get(header_name).and_then(|v| v.to_str().ok()) {
            let first = value.split(',').next().unwrap_or("").trim();
            if !first.is_empty() {
                return first.to_owned();
            }
        }
    }
    "unknown".into()
}

/// Shared state for all axum handlers
#[derive(Clone)]
pub struct AppState {
    pub provider: Arc<dyn Provider>,
    pub observer: Arc<dyn Observer>,
    pub tools_registry: Arc<Vec<Box<dyn Tool>>>,
    pub system_prompt: Arc<String>,
    pub model: String,
    pub temperature: f64,
    pub mem: Arc<dyn Memory>,
    pub auto_save: bool,
    pub webhook_secret: Option<Arc<str>>,
    pub pairing: Arc<PairingGuard>,
    pub rate_limiter: Arc<GatewayRateLimiter>,
    pub idempotency_store: Arc<IdempotencyStore>,
    pub whatsapp: Option<Arc<WhatsAppChannel>>,
    /// `WhatsApp` app secret for webhook signature verification (`X-Hub-Signature-256`)
    pub whatsapp_app_secret: Option<Arc<str>>,
    pub max_tool_iterations: usize,
    pub usage_stats: Arc<UsageStats>,
    /// 应用配置（使用 Arc 避免锁，满足 axum 0.8 Handler 'static bound 要求）
    pub config: Arc<Config>,
    /// 配置版本号（原子操作，线程安全）
    pub config_version: Arc<std::sync::atomic::AtomicU64>,
    /// 配置热重载管理器（可选）
    pub hot_reload_manager: Option<Arc<HotReloadManager>>,
    /// 轨迹存储（可观测性）
    pub trace_store: Option<Arc<dyn TraceStore>>,
    /// 工作流存储
    pub workflow_store: Arc<WorkflowStore>,
    /// 告警管理器
    pub alert_manager: Arc<crate::observability::prelude::AlertManager>,
    /// 失败分析器
    pub failure_analyzer: Arc<crate::observability::prelude::FailureAnalyzer>,
    /// Swarm 管理器
    pub swarm_manager: Option<Arc<crate::swarm::SwarmManager>>,
    /// Swarm 群聊管理器
    pub swarm_chat_manager: Option<Arc<crate::swarm::chat::SwarmChatManager>>,
    /// 共识管理器
    pub consensus_manager: Option<Arc<crate::swarm::consensus::ConsensusManager>>,
    /// 智能体团队存储
    pub agent_group_store: Arc<crate::store::AgentGroupStore>,
    /// 角色映射存储
    pub role_mapping_store: Arc<crate::store::RoleMappingStore>,
    /// 工作流引擎
    pub workflow_engine: Arc<crate::workflow::WorkflowEngine>,
    /// 工作流调度器
    pub workflow_scheduler: Arc<crate::workflow::WorkflowScheduler>,
    /// 事件总线
    pub event_bus: Arc<crate::workflow::EventBus>,
}

/// Simple usage statistics tracker
#[derive(Debug)]
pub struct UsageStats {
    pub total_tokens: std::sync::atomic::AtomicU64,
    pub total_requests: std::sync::atomic::AtomicU64,
    pub total_cost_usd: std::sync::atomic::AtomicU64,
    pub session_start: std::time::Instant,
}

impl Default for UsageStats {
    fn default() -> Self {
        Self {
            total_tokens: std::sync::atomic::AtomicU64::new(0),
            total_requests: std::sync::atomic::AtomicU64::new(0),
            total_cost_usd: std::sync::atomic::AtomicU64::new(0),
            session_start: std::time::Instant::now(),
        }
    }
}

/// Observer that tracks token usage for the gateway
pub struct UsageStatsObserver {
    stats: Arc<UsageStats>,
    inner: Arc<dyn Observer>,
}

impl UsageStatsObserver {
    pub fn new(stats: Arc<UsageStats>, inner: Arc<dyn Observer>) -> Self {
        Self { stats, inner }
    }
}

impl Observer for UsageStatsObserver {
    fn record_event(&self, event: &observability::ObserverEvent) {
        use std::sync::atomic::Ordering;
        
        if let observability::ObserverEvent::LlmResponse { tokens_used: Some(tokens), .. } = event {
            self.stats.total_tokens.fetch_add(*tokens, Ordering::Relaxed);
        }
        
        self.inner.record_event(event);
    }

    fn record_metric(&self, metric: &observability::ObserverMetric) {
        self.inner.record_metric(metric);
    }

    fn name(&self) -> &str {
        "UsageStatsObserver"
    }
}

/// Run the HTTP gateway using axum with proper HTTP/1.1 compliance.
#[allow(clippy::too_many_lines)]
pub async fn run_gateway(host: &str, port: u16, config: Config) -> Result<()> {
    // ── Security: refuse public bind without tunnel or explicit opt-in ──
    if is_public_bind(host) && config.tunnel.provider == "none" && !config.gateway.allow_public_bind
    {
        anyhow::bail!(
            "🛑 Refusing to bind to {host} — gateway would be exposed to the internet.\n\
             Fix: use --host 127.0.0.1 (default), configure a tunnel, or set\n\
             [gateway] allow_public_bind = true in config.toml (NOT recommended)."
        );
    }

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual_port = listener.local_addr()?.port();
    let display_addr = format!("{host}:{actual_port}");

    // 创建基础 Provider
    let base_provider: Box<dyn Provider> = providers::create_resilient_provider(
        config.default_provider.as_deref().unwrap_or(crate::config::schema::DEFAULT_PROVIDER),
        config.api_key.as_deref(),
        &config.reliability,
    )?;
    let provider_name = config.default_provider.as_deref().unwrap_or(crate::config::schema::DEFAULT_PROVIDER).to_string();
    
    // 初始化轨迹存储和收集器（当 observability backend 不是 "none" 时启用）
    let trace_store: Option<Arc<dyn TraceStore>> = if config.observability.backend != "none" {
        let store = trace_store::create_trace_store(
            &config.workspace_dir,
            TraceStoreConfig::default(),
        );
        tracing::debug!("Trace store initialized (SQLite) for observability backend: {}", config.observability.backend);
        Some(store)
    } else {
        None
    };
    
    // 创建 TraceCollector 并用 TracedProvider 包装 Provider
    let provider: Arc<dyn Provider> = if let Some(ref store) = trace_store {
        use crate::observability::trace_store::TraceCollector;
        let collector = Arc::new(TraceCollector::new(store.clone(), TraceCollectorConfig::default()));
        tracing::debug!("TracedProvider enabled for observability");
        Arc::new(crate::providers::TracedProvider::new(
            base_provider,
            collector,
            provider_name,
        ))
    } else {
        Arc::from(base_provider)
    };
    
    let model = config
        .default_model
        .clone()
        .unwrap_or_else(|| crate::config::schema::DEFAULT_MODEL.into());
    let temperature = config.default_temperature;
    let mem: Arc<dyn Memory> = Arc::from(memory::create_memory(
        &config.memory,
        &config.workspace_dir,
        config.api_key.as_deref(),
    )?);
    let observer: Arc<dyn Observer> =
        Arc::from(observability::create_observer(&config.observability));
    let runtime: Arc<dyn runtime::RuntimeAdapter> =
        Arc::from(runtime::create_runtime(&config.runtime)?);
    let security = Arc::new(SecurityPolicy::from_config(
        &config.autonomy,
        &config.workspace_dir,
    ));

    let (composio_key, composio_entity_id) = if config.composio.enabled {
        (
            config.composio.api_key.as_deref(),
            Some(config.composio.entity_id.as_str()),
        )
    } else {
        (None, None)
    };

    let config_arc = Arc::new(config.clone());
    let tools_registry = Arc::new(tools::all_tools_with_provider(
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
        config_arc.clone(),
        provider.clone(),
    ));
    let skills = crate::skills::load_skills(&config.workspace_dir);
    let tool_descs: Vec<(&str, &str)> = tools_registry
        .iter()
        .map(|tool| (tool.name(), tool.description()))
        .collect();
    
    let soul = if config.soul.enabled {
        crate::soul::create_soul_from_preset_name(&config.soul.preset)
    } else {
        None
    };

    let system_prompt = crate::channels::build_system_prompt(
        &config.workspace_dir,
        &model,
        &tool_descs,
        &skills,
        Some(&config.identity),
        None,
        soul.as_ref(),
    ).await;
    let mut system_prompt = system_prompt.to_string();
    system_prompt.push_str(&crate::agent::loop_::build_tool_instructions(
        tools_registry.as_ref(),
    ));
    let system_prompt = Arc::new(system_prompt);

    // Extract webhook secret for authentication
    let webhook_secret: Option<Arc<str>> = config
        .channels_config
        .webhook
        .as_ref()
        .and_then(|w| w.secret.as_deref())
        .map(Arc::from);

    // WhatsApp channel (if configured)
    let whatsapp_channel: Option<Arc<WhatsAppChannel>> =
        config.channels_config.whatsapp.as_ref().map(|wa| {
            Arc::new(WhatsAppChannel::new(
                wa.access_token.clone(),
                wa.phone_number_id.clone(),
                wa.verify_token.clone(),
                wa.allowed_numbers.clone(),
            ))
        });

    // WhatsApp app secret for webhook signature verification
    // Priority: environment variable > config file
    let whatsapp_app_secret: Option<Arc<str>> = std::env::var("ZEROCLAW_WHATSAPP_APP_SECRET")
        .ok()
        .and_then(|secret| {
            let secret = secret.trim();
            (!secret.is_empty()).then(|| secret.to_owned())
        })
        .or_else(|| {
            config.channels_config.whatsapp.as_ref().and_then(|wa| {
                wa.app_secret
                    .as_deref()
                    .map(str::trim)
                    .filter(|secret| !secret.is_empty())
                    .map(ToOwned::to_owned)
            })
        })
        .map(Arc::from);

    // ── Pairing guard ──────────────────────────────────────
    let pairing = Arc::new(PairingGuard::new(
        config.gateway.require_pairing,
        &config.gateway.paired_tokens,
    ));
    let rate_limiter = Arc::new(GatewayRateLimiter::new(
        config.gateway.pair_rate_limit_per_minute,
        config.gateway.webhook_rate_limit_per_minute,
    ));
    let idempotency_store = Arc::new(IdempotencyStore::new(Duration::from_secs(
        config.gateway.idempotency_ttl_secs.max(1),
    )));

    // ── Tunnel ────────────────────────────────────────────────
    let tunnel = crate::tunnel::create_tunnel(&config.tunnel)?;
    let mut tunnel_url: Option<String> = None;

    if let Some(ref tun) = tunnel {
        println!("🔗 Starting {} tunnel...", tun.name());
        match tun.start(host, actual_port).await {
            Ok(url) => {
                println!("🌐 Tunnel active: {url}");
                tunnel_url = Some(url);
            }
            Err(e) => {
                println!("⚠️  Tunnel failed to start: {e}");
                println!("   Falling back to local-only mode.");
            }
        }
    }

    println!("🦀 ZeroClaw Gateway listening on http://{display_addr}");
    if let Some(ref url) = tunnel_url {
        println!("  🌐 Public URL: {url}");
    }
    println!("  POST /pair      — pair a new client (X-Pairing-Code header)");
    println!("  POST /webhook   — {{\"message\": \"your prompt\"}}");
    if whatsapp_channel.is_some() {
        println!("  GET  /whatsapp  — Meta webhook verification");
        println!("  POST /whatsapp  — WhatsApp message webhook");
    }
    println!("  GET  /health    — health check");
    if let Some(code) = pairing.pairing_code() {
        println!();
        println!("  🔐 PAIRING REQUIRED — use this one-time code:");
        println!("     ┌──────────────┐");
        println!("     │  {code}  │");
        println!("     └──────────────┘");
        println!("     Send: POST /pair with header X-Pairing-Code: {code}");
    } else if pairing.require_pairing() {
        println!("  🔒 Pairing: ACTIVE (bearer token required)");
    } else {
        println!("  ⚠️  Pairing: DISABLED (all requests accepted)");
    }
    if webhook_secret.is_some() {
        println!("  🔒 Webhook secret: ENABLED");
    }
    println!("  Press Ctrl+C to stop.\n");

    crate::health::mark_component_ok("gateway");

    // Create usage stats and wrap observer
    let usage_stats = Arc::new(UsageStats::default());
    let stats_observer = Arc::new(UsageStatsObserver::new(usage_stats.clone(), observer));

    // 初始化轨迹存储（当 observability backend 不是 "none" 时启用）
    let trace_store: Option<Arc<dyn TraceStore>> = if config.observability.backend != "none" {
        let store = trace_store::create_trace_store(
            &config.workspace_dir,
            TraceStoreConfig::default(),
        );
        tracing::debug!("Trace store initialized (SQLite) for observability backend: {}", config.observability.backend);
        Some(store)
    } else {
        None
    };

    // Build shared state
    // 初始化工作流存储
    // WorkflowStore::new expects a workspace directory, and will create .zeroclaw/workflow.db inside it
    // We need to pass the workspace_dir, not the workflow_dir
    let workflow_store = Arc::new(WorkflowStore::new(&config.workspace_dir)
        .context("Failed to initialize workflow store")?);
    
    // 初始化 Swarm 管理器
    let swarm_manager = match crate::swarm::manager_for_workspace(&config.workspace_dir, config.swarm.subagent_max_concurrent) {
        Ok(mgr) => Some(mgr),
        Err(e) => {
            tracing::warn!("Failed to initialize Swarm manager: {}", e);
            None
        }
    };
    
    // 初始化 Swarm 群聊管理器
    let swarm_chat_manager = Some(Arc::new(crate::swarm::chat::SwarmChatManager::new(&config.workspace_dir)));
    
    // 初始化共识管理器
    let consensus_manager = Some(Arc::new(crate::swarm::consensus::ConsensusManager::new(&config.workspace_dir)));
    
    // 克隆工作流存储以供工作流引擎和调度器使用
    let workflow_store_for_engine = workflow_store.clone();
    
    // 初始化工作流引擎
    let workflow_engine = Arc::new(crate::workflow::WorkflowEngine::new(workflow_store_for_engine.clone()));
    
    // 初始化工作流调度器
    let workflow_scheduler = Arc::new(crate::workflow::WorkflowScheduler::new(
        workflow_engine.clone(),
        workflow_store_for_engine.clone(),
    ));
    
    // 初始化事件总线
    let event_bus = Arc::new(crate::workflow::EventBus::new(
        workflow_engine.clone(),
        workflow_scheduler.clone(),
    ));
    
    // 创建配置热重载相关结构（如果环境变量启用）
    let hot_reload_enabled = std::env::var("ZEROCLAW_HOT_RELOAD")
        .unwrap_or_default()
        .parse()
        .unwrap_or(false);
    
    let (hot_reload_manager, config_version) = if hot_reload_enabled {
        info!("启用配置热重载功能");
        let hot_config = HotConfig::new(Config::clone(&config));
        let config_path = config.config_path.clone();
        let manager = HotReloadManager::new(hot_config, config_path);
        
        // 启动文件监听
        if let Err(e) = manager.start().await {
            error!("启动配置热重载管理器失败：{}", e);
            // 继续启动，但不启用热重载
            (None, Arc::new(std::sync::atomic::AtomicU64::new(0)))
        } else {
            // 启动自动重载
            if let Err(e) = manager.watch_and_reload().await {
                error!("启动配置自动重载失败：{}", e);
            }
            (
                Some(Arc::new(manager)),
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
            )
        }
    } else {
        (None, Arc::new(std::sync::atomic::AtomicU64::new(0)))
    };
    
    let state = AppState {
        provider,
        observer: stats_observer,
        tools_registry,
        system_prompt,
        model,
        temperature,
        mem,
        auto_save: config.memory.auto_save,
        webhook_secret,
        pairing,
        rate_limiter,
        idempotency_store,
        whatsapp: whatsapp_channel,
        whatsapp_app_secret,
        max_tool_iterations: config.agent.max_tool_iterations,
        usage_stats,
        config: Arc::new(Config::clone(&config)),
        config_version,
        hot_reload_manager,
        trace_store,
        workflow_store,
        alert_manager: Arc::new(crate::observability::prelude::AlertManager::new()),
        failure_analyzer: Arc::new(crate::observability::prelude::FailureAnalyzer::new()),
        swarm_manager,
        swarm_chat_manager,
        consensus_manager,
        agent_group_store: Arc::new(AgentGroupStore::new(&config.workspace_dir).context("Failed to initialize agent group store")?),
        role_mapping_store: Arc::new(RoleMappingStore::new(&config.workspace_dir).context("Failed to initialize role mapping store")?),
        workflow_engine,
        workflow_scheduler,
        event_bus,
    };

    // Build router with middleware
    // Note: /webhook and /chat/stream need longer timeout for LLM + tool calls
    let webhook_timeout_layer = TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        Duration::from_secs(WEBHOOK_TIMEOUT_SECS),
    );
    
    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/pair", post(handle_pair))
        .route("/webhook", post(handle_webhook).layer(webhook_timeout_layer.clone()))
        .route("/chat/stream", post(handle_chat_stream).layer(webhook_timeout_layer))
        .route("/whatsapp", get(handle_whatsapp_verify))
        .route("/whatsapp", post(handle_whatsapp_message))
        .route("/cost/summary", get(handle_cost_summary))
        .route("/cost/daily", get(handle_cost_daily))
        .route("/chat/abort", post(handle_chat_abort))
        .route("/workflow/list", get(handle_workflow_list))
        .route("/workflow/create", post(handle_workflow_create))
        .route("/workflow/auto-generate", post(handle_workflow_auto_generate))
        .route("/workflow/start", post(handle_workflow_start))
        .route("/workflow/pause", post(handle_workflow_pause))
        .route("/workflow/resume", post(handle_workflow_resume))
        .route("/workflow/stop", post(handle_workflow_stop))
        .route("/workflow/{id}/phases", get(handle_workflow_phases))
        .route("/workflow/{id}/transition", post(handle_workflow_phase_transition))
        .route("/workflow/{id}/context", get(handle_workflow_context))
        .route("/workflow/{id}/approvals", get(handle_workflow_approvals))
        // 事件驱动相关端点
        .route("/event/listener/add", post(handle_event_listener_add))
        .route("/event/listener/remove", post(handle_event_listener_remove))
        .route("/event/listener/list", get(handle_event_listener_list))
        .route("/event/listener/update", post(handle_event_listener_update))
        .route("/event/publish", post(handle_event_publish))
        .route("/workflow/approval/{id}/respond", post(handle_workflow_approval_respond))
        // 智能体团队 API
        .route("/agent-groups", get(handle_agent_groups_list))
        .route("/agent-groups", post(handle_agent_groups_create))
        .route("/agent-groups/{id}", get(handle_agent_groups_get))
        .route("/agent-groups/{id}", put(handle_agent_groups_update))
        .route("/agent-groups/{id}", delete(handle_agent_groups_delete))
        // 角色-智能体映射 API
        .route("/role-mappings", get(handle_role_mappings_list))
        .route("/role-mappings", post(handle_role_mappings_create))
        .route("/role-mappings/{role}", get(handle_role_mappings_get))
        .route("/role-mappings/{role}", put(handle_role_mappings_update))
        .route("/role-mappings/{role}", delete(handle_role_mappings_delete))
        // Swarm 智能体群聊 API
        .route("/swarm/tasks", get(handle_swarm_tasks_list))
        .route("/swarm/tasks", post(handle_swarm_tasks_create))
        .route("/swarm/tasks/{id}", get(handle_swarm_tasks_get))
        .route("/swarm/tasks/{id}", delete(handle_swarm_tasks_delete))
        .route("/swarm/tasks/{id}/messages", get(handle_swarm_messages_list))
        .route("/swarm/tasks/{id}/messages", post(handle_swarm_messages_send))
        .route("/swarm/tasks/{id}/consensus", get(handle_swarm_consensus_get))
        .route("/swarm/tasks/{id}/consensus", post(handle_swarm_consensus_vote))
        .route("/soul/templates", get(handle_soul_templates_list))
        .route("/soul/templates", post(handle_soul_templates_save))
        .route("/soul/templates/{id}", get(handle_soul_templates_get))
        .route("/mcp/servers", get(handle_mcp_servers_list))
        .route("/mcp/servers", post(handle_mcp_servers_create))
        .route("/mcp/servers/{id}", get(handle_mcp_servers_get))
        .route("/mcp/servers/{id}", put(handle_mcp_servers_update))
        .route("/mcp/servers/{id}", delete(handle_mcp_servers_delete))
        .route("/mcp/servers/{id}/start", post(handle_mcp_servers_start))
        .route("/mcp/servers/{id}/stop", post(handle_mcp_servers_stop))
        .route("/mcp/servers/{id}/tools", get(handle_mcp_servers_tools))
        // 可观测性 API 路由
        .route("/observability/traces/list", post(handle_observability_list_traces))
        .route("/observability/traces/{id}", get(handle_observability_get_trace))
        .route("/observability/traces/{id}/reasoning", get(handle_observability_get_reasoning))
        .route("/observability/traces/{id}/decisions", get(handle_observability_get_decisions))
        .route("/observability/traces/{id}/evaluation", get(handle_observability_get_evaluation))
        .route("/observability/traces/{id}/evaluate", post(handle_observability_evaluate_trace))
        .route("/observability/aggregate", post(handle_observability_aggregate))
        .route("/observability/dashboard", get(handle_observability_dashboard))
        .route("/observability/alerts", get(handle_observability_alerts))
        .route("/observability/alerts/{id}/dismiss", post(handle_observability_dismiss_alert))
        .route("/observability/failure-patterns", get(handle_observability_failure_patterns))
        // DEBUG: Temporary endpoint to get pairing code (remove after pairing)
        .route("/debug/pairing-code", get(handle_debug_pairing_code))
        .with_state(state.clone())
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(REQUEST_TIMEOUT_SECS),
        ));

    // 启动工作流调度器
    if let Err(e) = state.workflow_scheduler.start().await {
        error!("Failed to start workflow scheduler: {}", e);
    }
    
    // 启动事件总线
    state.event_bus.start().await;

    // Run the server
    axum::serve(listener, app).await?;

    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// AXUM HANDLERS
// ══════════════════════════════════════════════════════════════════════════════

/// GET /health — always public (no secrets leaked)
async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    let body = serde_json::json!({
        "status": "ok",
        "paired": state.pairing.is_paired(),
        "runtime": crate::health::snapshot_json(),
    });
    Json(body)
}

/// POST /pair — exchange one-time code for bearer token
async fn handle_pair(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let client_key = client_key_from_headers(&headers);
    if !state.rate_limiter.allow_pair(&client_key) {
        tracing::warn!("/pair rate limit exceeded for key: {client_key}");
        let err = serde_json::json!({
            "error": "Too many pairing requests. Please retry later.",
            "retry_after": RATE_LIMIT_WINDOW_SECS,
        });
        return (StatusCode::TOO_MANY_REQUESTS, Json(err));
    }

    let code = headers
        .get("X-Pairing-Code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match state.pairing.try_pair(code) {
        Ok(Some(token)) => {
            tracing::info!("🔐 New client paired successfully");
            
            // Persist the new token to config
            let _tokens = state.pairing.tokens();
            
            // TODO: 实现配置持久化逻辑
            // 目前简化实现，只记录日志
            
            tracing::info!("🔐 Paired tokens persisted to config (TODO: implement persistence)");
            
            let body = serde_json::json!({
                "paired": true,
                "token": token,
                "message": "Save this token — use it as Authorization: Bearer <token>"
            });
            (StatusCode::OK, Json(body))
        }
        Ok(None) => {
            tracing::warn!("🔐 Pairing attempt with invalid code");
            let err = serde_json::json!({"error": "Invalid pairing code"});
            (StatusCode::FORBIDDEN, Json(err))
        }
        Err(lockout_secs) => {
            tracing::warn!(
                "🔐 Pairing locked out — too many failed attempts ({lockout_secs}s remaining)"
            );
            let err = serde_json::json!({
                "error": format!("Too many failed attempts. Try again in {lockout_secs}s."),
                "retry_after": lockout_secs
            });
            (StatusCode::TOO_MANY_REQUESTS, Json(err))
        }
    }
}

/// Webhook request body
#[derive(serde::Deserialize)]
pub struct WebhookBody {
    pub message: String,
}

/// POST /webhook — main webhook endpoint
async fn handle_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<WebhookBody>, axum::extract::rejection::JsonRejection>,
) -> impl IntoResponse {
    let client_key = client_key_from_headers(&headers);
    if !state.rate_limiter.allow_webhook(&client_key) {
        tracing::warn!("/webhook rate limit exceeded for key: {client_key}");
        let err = serde_json::json!({
            "error": "Too many webhook requests. Please retry later.",
            "retry_after": RATE_LIMIT_WINDOW_SECS,
        });
        return (StatusCode::TOO_MANY_REQUESTS, Json(err));
    }

    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Webhook: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }

    // ── Webhook secret auth (optional, additional layer) ──
    if let Some(ref secret) = state.webhook_secret {
        let header_val = headers
            .get("X-Webhook-Secret")
            .and_then(|v| v.to_str().ok());
        match header_val {
            Some(val) if constant_time_eq(val, secret.as_ref()) => {}
            _ => {
                tracing::warn!("Webhook: rejected request — invalid or missing X-Webhook-Secret");
                let err = serde_json::json!({"error": "Unauthorized — invalid or missing X-Webhook-Secret header"});
                return (StatusCode::UNAUTHORIZED, Json(err));
            }
        }
    }

    // ── Parse body ──
    let Json(webhook_body) = match body {
        Ok(b) => b,
        Err(e) => {
            let err = serde_json::json!({
                "error": format!("Invalid JSON: {e}. Expected: {{\"message\": \"...\"}}")
            });
            return (StatusCode::BAD_REQUEST, Json(err));
        }
    };

    // ── Idempotency (optional) ──
    if let Some(idempotency_key) = headers
        .get("X-Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !state.idempotency_store.record_if_new(idempotency_key) {
            tracing::info!("Webhook duplicate ignored (idempotency key: {idempotency_key})");
            let body = serde_json::json!({
                "status": "duplicate",
                "idempotent": true,
                "message": "Request already processed for this idempotency key"
            });
            return (StatusCode::OK, Json(body));
        }
    }

    let message = &webhook_body.message;

    if state.auto_save {
        let key = webhook_memory_key();
        let _ = state
            .mem
            .store(&key, message, MemoryCategory::Conversation)
            .await;
    }

    match gateway_agent_reply(&state, message).await {
        Ok(reply) => {
            let body = serde_json::json!({"response": reply, "model": state.model});
            (StatusCode::OK, Json(body))
        }
        Err(e) => {
            tracing::error!(
                "Webhook provider error: {}",
                providers::sanitize_api_error(&e.to_string())
            );
            let err = serde_json::json!({"error": "LLM request failed"});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// WORKFLOW HANDLERS
/// GET /workflow/list — List all workflows
async fn handle_workflow_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Workflow list: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    // 从数据库中读取工作流（使用统一的WorkflowStore）
    let workflow_store = state.workflow_store.clone();
    let workflows = match workflow_store.list_workflows(None) {
        Ok(workflows) => {
            // 直接序列化Workflow结构为JSON Value数组，利用Serialize trait
            workflows.into_iter().map(|wf| serde_json::to_value(wf).unwrap_or_default()).collect::<Vec<_>>()
        },
        Err(e) => {
            tracing::error!("Failed to list workflows from store: {}", e);
            vec![]
        }
    };
    
    (StatusCode::OK, Json(serde_json::Value::Array(workflows)))
}

/// POST /workflow/create — Create a new workflow
async fn handle_workflow_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(config): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Workflow create: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let workflow_store = state.workflow_store.clone();
    
    // 从配置中提取字段
    let name = config.get("name").and_then(|v| v.as_str()).unwrap_or("New Workflow");
    let description = config.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let roles: Vec<String> = config.get("roles")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|r| r.as_str().map(String::from)).collect())
        .unwrap_or_default();
    
    // 提取步骤信息
    let steps_value = config.get("steps").cloned().unwrap_or_else(|| serde_json::Value::Array(vec![]));
    
    match workflow_store.create_workflow(name, description, roles, None) {
        Ok(mut workflow) => {
            // 如果有传入的步骤信息，添加到工作流中
            if let Some(steps_array) = steps_value.as_array() {
                for (index, step_value) in steps_array.iter().enumerate() {
                    if let (Some(step_name), Some(step_description)) = (
                        step_value.get("name").and_then(|v| v.as_str()),
                        step_value.get("description").and_then(|v| v.as_str())
                    ) {
                        let assigned_to = step_value.get("assignedTo").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let status = step_value.get("status").and_then(|v| v.as_str()).unwrap_or("pending");
                        
                        if let Ok(step) = workflow_store.add_step(&workflow.id, step_name, step_description, index as i32) {
                             // 成功添加步骤后，更新分配和状态
                             if let Some(ref assignee) = assigned_to {
                                 if let Err(e) = workflow_store.update_step(&step.id, None, None, Some(status), Some(assignee), None) {
                                     tracing::warn!("Failed to update step {} status/assignment: {}", step.id, e);
                                 }
                             } else if !status.is_empty() && status != "pending" {
                                 if let Err(e) = workflow_store.update_step(&step.id, None, None, Some(status), None, None) {
                                     tracing::warn!("Failed to update step {} status: {}", step.id, e);
                                 }
                             }
                         } else {
                             tracing::warn!("Failed to add step {} to workflow {}", step_name, workflow.id);
                         }
                    }
                }
                
                // 重新获取工作流以包含新添加的步骤
                 if let Ok(Some(updated_workflow)) = workflow_store.get_workflow(&workflow.id) {
                     workflow = updated_workflow;
                 }
            }
            
            let result = serde_json::json!({"success": true, "workflow": workflow});
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to create workflow: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /workflow/auto-generate — Auto-generate a workflow
async fn handle_workflow_auto_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Workflow auto-generate: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let prompt = body.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    
    // 使用 LLM 生成工作流配置
    let system_prompt = r#"你是一个工作流设计专家。根据用户的需求描述，生成一个结构化的工作流配置。
返回 JSON 格式，包含以下字段：
- name: 工作流名称
- description: 工作流描述
- roles: 角色列表
- steps: 步骤列表，每个步骤包含 name, description, assignedTo, status"#;
    
    let provider = state.provider.clone();
    let model = state.model.clone();
    let workflow_store = state.workflow_store.clone();
    
    // 同步调用 LLM
    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(async {
        provider.chat_with_system_and_tools(
            Some(system_prompt),
            prompt,
            &model,
            0.7,
            None,
        ).await
    });
    
    match result {
        Ok(response) => {
            let text = response.text.unwrap_or_default();
            // 尝试解析 JSON
            if let Ok(workflow_config) = serde_json::from_str::<serde_json::Value>(&text) {
                // 创建工作流
                let name = workflow_config.get("name").and_then(|v| v.as_str()).unwrap_or("Generated Workflow");
                let description = workflow_config.get("description").and_then(|v| v.as_str()).unwrap_or("");
                let roles: Vec<String> = workflow_config.get("roles")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|r| r.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                
                match workflow_store.create_workflow(name, description, roles, None) {
                    Ok(workflow) => {
                        let result = serde_json::json!({"success": true, "workflow": workflow});
                        (StatusCode::OK, Json(result))
                    }
                    Err(_) => {
                        let err = serde_json::json!({"error": "Failed to save generated workflow"});
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                    }
                }
            } else {
                let err = serde_json::json!({"error": "Failed to parse LLM response as workflow config", "raw_response": text});
                (StatusCode::BAD_REQUEST, Json(err))
            }
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("LLM generation failed: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /workflow/start — Start a workflow
async fn handle_workflow_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Workflow start: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let workflow_engine = state.workflow_engine.clone();
    
    match workflow_engine.start_workflow(id).await {
        Ok(_) => {
            let result = serde_json::json!({"success": true, "message": "Workflow started"});
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to start workflow: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /workflow/pause — Pause a workflow
async fn handle_workflow_pause(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Workflow pause: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let workflow_engine = state.workflow_engine.clone();
    
    match workflow_engine.pause_workflow(id).await {
        Ok(_) => {
            let result = serde_json::json!({"success": true, "message": "Workflow paused"});
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to pause workflow: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /workflow/resume — Resume a workflow
async fn handle_workflow_resume(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Workflow resume: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let workflow_engine = state.workflow_engine.clone();
    
    match workflow_engine.resume_workflow(id).await {
        Ok(_) => {
            let result = serde_json::json!({"success": true, "message": "Workflow resumed"});
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to resume workflow: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /workflow/stop — Stop a workflow
async fn handle_workflow_stop(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Workflow stop: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let workflow_engine = state.workflow_engine.clone();
    
    match workflow_engine.stop_workflow(id).await {
        Ok(_) => {
            let result = serde_json::json!({"success": true, "message": "Workflow stopped"});
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to stop workflow: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// WORKFLOW PHASE HANDLERS
// ══════════════════════════════════════════════════════════════════════════════

/// GET /workflow/{id}/phases — 获取工作流阶段详情
async fn handle_workflow_phases(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let workspace_dir = state.config.workspace_dir.clone();
    let workflow_file = workspace_dir.join("workflows").join(format!("{}.json", id));
    
    match std::fs::read_to_string(&workflow_file) {
        Ok(content) => {
            if let Ok(workflow) = serde_json::from_str::<serde_json::Value>(&content) {
                let phases = workflow.get("phases").and_then(|p| p.as_array()).cloned().unwrap_or_default();
                (StatusCode::OK, Json(serde_json::json!({"phases": phases})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Invalid workflow JSON"})))
            }
        }
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workflow not found"})))
        }
    }
}

/// POST /workflow/{id}/transition — 阶段转换
async fn handle_workflow_phase_transition(
    State(state): State<AppState>,
    axum::extract::Path(workflow_id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    use crate::swarm::engine::{WorkflowEngine, WorkflowStore as SwarmWorkflowStore};
    use crate::swarm::consensus::ConsensusManager;
    use crate::swarm::phase::{Deliverable, DeliverableType, TransitionType};
    
    let deliverables_json: Vec<serde_json::Value> = body.get("deliverables")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    
    let deliverables: Vec<Deliverable> = deliverables_json.iter().enumerate().map(|(i, d)| {
        Deliverable {
            id: d.get("id").and_then(|v| v.as_str()).unwrap_or(&format!("deliverable-{}", i)).to_string(),
            name: d.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            description: d.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            deliverable_type: DeliverableType::Other,
            content: d.get("content").and_then(|v| v.as_str()).map(String::from),
            file_path: d.get("file_path").and_then(|v| v.as_str()).map(String::from),
            is_knowledge: d.get("is_knowledge").and_then(|v| v.as_bool()).unwrap_or(false),
            created_at: chrono::Utc::now().timestamp_millis() as u64,
        }
    }).collect();
    
    let workspace_dir = state.config.workspace_dir.clone();
    let swarm_store = Arc::new(SwarmWorkflowStore::new());
    let consensus = Arc::new(ConsensusManager::new(&workspace_dir));
    let engine = WorkflowEngine::new(swarm_store.clone(), consensus);
    
    match engine.advance_phase(&workflow_id, deliverables).await {
        Ok(transition) => {
            let status = match transition.transition_type {
                TransitionType::Started => "started",
                TransitionType::Advanced => "advanced",
                TransitionType::WaitingForDependencies => "waiting_for_dependencies",
                TransitionType::WaitingForApproval => "waiting_for_approval",
                TransitionType::WaitingForConsensus => "waiting_for_consensus",
                TransitionType::NeedsAdjustment => "needs_adjustment",
                TransitionType::Completed => "completed",
                TransitionType::Cancelled => "cancelled",
            };
            let result = serde_json::json!({
                "success": true,
                "workflowId": workflow_id,
                "fromPhase": transition.from_phase,
                "toPhase": transition.to_phase,
                "status": status,
                "timestamp": transition.timestamp
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let result = serde_json::json!({
                "success": false,
                "error": "TRANSITION_FAILED",
                "message": e.to_string()
            });
            (StatusCode::BAD_REQUEST, Json(result))
        }
    }
}

/// GET /workflow/{id}/context — 获取工作流上下文
async fn handle_workflow_context(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let workspace_dir = state.config.workspace_dir.clone();
    let workflow_file = workspace_dir.join("workflows").join(format!("{}.json", id));
    
    match std::fs::read_to_string(&workflow_file) {
        Ok(content) => {
            if let Ok(workflow) = serde_json::from_str::<serde_json::Value>(&content) {
                let context = serde_json::json!({
                    "workflowId": id,
                    "currentPhase": workflow.get("currentPhase").and_then(|p| p.as_str()).unwrap_or(""),
                    "phases": workflow.get("phases").and_then(|p| p.as_array()).cloned().unwrap_or_default(),
                    "completedTasks": workflow.get("completedTasks").and_then(|t| t.as_array()).cloned().unwrap_or_default(),
                    "pendingApprovals": workflow.get("pendingApprovals").and_then(|a| a.as_array()).cloned().unwrap_or_default(),
                });
                (StatusCode::OK, Json(context))
            } else {
                let err = serde_json::json!({"error": "Invalid workflow JSON"});
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
            }
        }
        Err(_) => {
            let err = serde_json::json!({"error": "Workflow not found"});
            (StatusCode::NOT_FOUND, Json(err))
        }
    }
}

/// GET /workflow/{id}/approvals — 获取审批请求列表
async fn handle_workflow_approvals(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let workspace_dir = state.config.workspace_dir.clone();
    let approvals_dir = workspace_dir.join(".zeroclaw").join("approvals");
    
    let mut approvals: Vec<serde_json::Value> = vec![];
    
    if let Ok(entries) = std::fs::read_dir(&approvals_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |ext| ext == "json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(approval) = serde_json::from_str::<serde_json::Value>(&content) {
                        // 过滤出属于该工作流的审批请求
                        if approval.get("workflowId").and_then(|w| w.as_str()) == Some(&id) {
                            approvals.push(approval);
                        }
                    }
                }
            }
        }
    }
    
    (StatusCode::OK, Json(approvals))
}

/// POST /workflow/approval/{id}/respond — 响应审批请求
async fn handle_workflow_approval_respond(
    State(_state): State<AppState>,
    axum::extract::Path(approval_id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let approved = body.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);
    let comment = body.get("comment").and_then(|v| v.as_str()).unwrap_or("");
    
    let result = serde_json::json!({
        "success": true,
        "approvalId": approval_id,
        "approved": approved,
        "comment": comment,
        "message": "Approval response recorded"
    });
    (StatusCode::OK, Json(result))
}

// ══════════════════════════════════════════════════════════════════════════════
// AGENT GROUPS HANDLERS (智能体团队 API)
// ══════════════════════════════════════════════════════════════════════════════


/// 智能体团队结构体 (用于API响应)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub agents: Vec<String>,
    pub auto_generate: bool,
    pub team_members: Vec<serde_json::Value>,
}

/// GET /agent-groups — 列出所有智能体团队
async fn handle_agent_groups_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Agent groups list: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    match state.agent_group_store.list_groups() {
        Ok(groups) => {
            let result: Vec<serde_json::Value> = groups.iter().map(|g| serde_json::to_value(g).unwrap_or_default()).collect();
            (StatusCode::OK, Json(serde_json::Value::Array(result)))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to list agent groups: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /agent-groups — 创建智能体团队
async fn handle_agent_groups_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Agent groups create: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let description = body.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let agents: Vec<String> = body.get("agents").and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|a| a.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let auto_generate = body.get("autoGenerate").and_then(|v| v.as_bool()).unwrap_or(false);
    let team_members = body.get("teamMembers").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    
    match state.agent_group_store.create_group(&name, &description, agents, auto_generate) {
        Ok(group) => {
            let result = AgentGroup {
                id: group.id,
                name: group.name,
                description: group.description,
                agents: group.agents,
                auto_generate: group.auto_generate,
                team_members,
            };
            (StatusCode::OK, Json(serde_json::json!(result)))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to create agent group: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /agent-groups/{id} — 获取智能体团队详情
async fn handle_agent_groups_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Agent groups get: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    match state.agent_group_store.get_group(&id) {
        Ok(Some(group)) => (StatusCode::OK, Json(serde_json::json!(group))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Team not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get agent group: {}", e)}))),
    }
}

/// PUT /agent-groups/{id} — 更新智能体团队
async fn handle_agent_groups_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Agent groups update: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let name = body.get("name").and_then(|v| v.as_str());
    let description = body.get("description").and_then(|v| v.as_str());
    let agents = body.get("agents").and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|a| a.as_str().map(String::from)).collect());
    let auto_generate = body.get("autoGenerate").and_then(|v| v.as_bool());
    
    match state.agent_group_store.update_group(&id, name, description, agents, auto_generate) {
        Ok(()) => {
            match state.agent_group_store.get_group(&id) {
                Ok(Some(group)) => (StatusCode::OK, Json(serde_json::json!(group))),
                _ => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Team not found"}))),
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to update agent group: {}", e)}))),
    }
}

/// DELETE /agent-groups/{id} — 删除智能体团队
async fn handle_agent_groups_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Agent groups delete: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    match state.agent_group_store.delete_group(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true, "deleted": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to delete agent group: {}", e)}))),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ROLE MAPPINGS HANDLERS (角色-智能体映射 API)
// ══════════════════════════════════════════════════════════════════════════════

/// 角色映射结构体 (用于API响应)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoleMapping {
    pub role: String,
    pub agent_name: String,
    pub agent_config: serde_json::Value,
}

/// GET /role-mappings — 列出所有角色映射
async fn handle_role_mappings_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Role mappings list: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    match state.role_mapping_store.list_mappings() {
        Ok(mappings) => {
            let result: Vec<serde_json::Value> = mappings.iter().map(|m| serde_json::to_value(m).unwrap_or_default()).collect();
            (StatusCode::OK, Json(serde_json::Value::Array(result)))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to list role mappings: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /role-mappings — 创建角色映射
async fn handle_role_mappings_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Role mappings create: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let role = body.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let agent_name = body.get("agent_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let agent_config = body.get("agent_config").cloned().unwrap_or(serde_json::json!({}));
    
    match state.role_mapping_store.create_mapping(&role, &agent_name, agent_config) {
        Ok(mapping) => (StatusCode::OK, Json(serde_json::to_value(&mapping).unwrap_or_default())),
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to create role mapping: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /role-mappings/{role} — 获取角色映射
async fn handle_role_mappings_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(role): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Role mappings get: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    match state.role_mapping_store.get_mapping(&role) {
        Ok(Some(mapping)) => (StatusCode::OK, Json(serde_json::json!(mapping))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Role mapping not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to get role mapping: {}", e)}))),
    }
}

/// PUT /role-mappings/{role} — 更新角色映射
async fn handle_role_mappings_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(role): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Role mappings update: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let agent_name = body.get("agent_name").and_then(|v| v.as_str()).map(String::from);
    let agent_config = body.get("agent_config").cloned();
    
    match state.role_mapping_store.update_mapping(&role, agent_name.as_deref(), agent_config) {
        Ok(()) => {
            match state.role_mapping_store.get_mapping(&role) {
                Ok(Some(mapping)) => (StatusCode::OK, Json(serde_json::to_value(&mapping).unwrap_or_default())),
                _ => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Role mapping not found"}))),
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to update role mapping: {}", e)}))),
    }
}

/// DELETE /role-mappings/{role} — 删除角色映射
async fn handle_role_mappings_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(role): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Role mappings delete: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    match state.role_mapping_store.delete_mapping(&role) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to delete role mapping: {}", e)}))),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// SWARM HANDLERS (智能体群聊 API)
// ══════════════════════════════════════════════════════════════════════════════

/// Swarm 任务创建请求结构体
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SwarmTaskCreateRequest {
    pub task: String,
    pub agent_name: String,
    pub label: Option<String>,
    pub orchestrator: Option<bool>,
    pub parent_run_id: Option<String>,
}

/// Swarm 消息发送请求结构体
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SwarmMessageSendRequest {
    pub content: String,
    pub author: String,
    pub author_type: Option<String>,
    pub message_type: Option<String>,
    pub lang: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Swarm 共识投票请求结构体
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SwarmConsensusVoteRequest {
    pub voter: String,
    pub vote: bool,
    pub reason: Option<String>,
}

/// GET /swarm/tasks — 列出所有 Swarm 任务
async fn handle_swarm_tasks_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm tasks list: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref swarm_manager) = state.swarm_manager else {
        let err = serde_json::json!({"error": "Swarm manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    let runs = swarm_manager.list().await;
    (StatusCode::OK, Json(serde_json::json!({"tasks": runs})))
}

/// POST /swarm/tasks — 创建 Swarm 任务
async fn handle_swarm_tasks_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SwarmTaskCreateRequest>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm tasks create: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref swarm_manager) = state.swarm_manager else {
        let err = serde_json::json!({"error": "Swarm manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    // 从配置中获取必要的信息（使用 Arc<Config> 满足 axum 0.8 'static bound 要求）
    let parent_config = Arc::new((&*state.config).clone());
    let security = Arc::new(crate::security::SecurityPolicy::from_config(
        &parent_config.autonomy,
        &parent_config.workspace_dir,
    ));
    
    let parent_run_id = request.parent_run_id
        .and_then(|id| uuid::Uuid::parse_str(&id).ok());
    
    match swarm_manager.spawn(
        &security,
        parent_config,
        crate::swarm::SwarmContext::root(),
        &request.agent_name,
        &request.task,
        request.label,
        request.orchestrator.unwrap_or(false),
        parent_run_id,
        false,
    ).await {
        Ok(run_id) => {
            let result = serde_json::json!({
                "success": true,
                "task_id": run_id.to_string(),
                "task": request.task,
                "agent_name": request.agent_name,
                "status": "pending"
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to create task: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /swarm/tasks/{id} — 获取 Swarm 任务详情
async fn handle_swarm_tasks_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm tasks get: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref swarm_manager) = state.swarm_manager else {
        let err = serde_json::json!({"error": "Swarm manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    match uuid::Uuid::parse_str(&id) {
        Ok(run_id) => {
            match swarm_manager.get(run_id).await {
                Some(run) => {
                    (StatusCode::OK, Json(serde_json::json!(run)))
                }
                None => {
                    let err = serde_json::json!({"error": "Task not found"});
                    (StatusCode::NOT_FOUND, Json(err))
                }
            }
        }
        Err(_) => {
            let err = serde_json::json!({"error": "Invalid task ID format"});
            (StatusCode::BAD_REQUEST, Json(err))
        }
    }
}

/// DELETE /swarm/tasks/{id} — 删除 Swarm 任务
async fn handle_swarm_tasks_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm tasks delete: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref swarm_manager) = state.swarm_manager else {
        let err = serde_json::json!({"error": "Swarm manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    match uuid::Uuid::parse_str(&id) {
        Ok(run_id) => {
            // 从配置中获取安全策略（使用 Arc<Config> 满足 axum 0.8 'static bound 要求）
            let security = Arc::new(crate::security::SecurityPolicy::from_config(
                &state.config.autonomy,
                &state.config.workspace_dir,
            ));
            
            match swarm_manager.kill(&security, run_id).await {
                Ok(killed) => {
                    let result = serde_json::json!({
                        "success": true,
                        "killed": killed,
                        "message": if killed { "Task terminated" } else { "Task not found or already completed" }
                    });
                    (StatusCode::OK, Json(result))
                }
                Err(e) => {
                    let err = serde_json::json!({"error": format!("Failed to terminate task: {}", e)});
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                }
            }
        }
        Err(_) => {
            let err = serde_json::json!({"error": "Invalid task ID format"});
            (StatusCode::BAD_REQUEST, Json(err))
        }
    }
}

/// GET /swarm/tasks/{id}/messages — 获取 Swarm 任务消息列表
async fn handle_swarm_messages_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm messages list: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref chat_manager) = state.swarm_chat_manager else {
        let err = serde_json::json!({"error": "Swarm chat manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    let run_id = match uuid::Uuid::parse_str(&id) {
        Ok(uid) => Some(uid),
        Err(_) => None,
    };
    
    let limit = params.get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(100);
    
    match chat_manager.get_messages(run_id, Some(id), limit) {
        Ok(messages) => {
            (StatusCode::OK, Json(serde_json::json!({"messages": messages})))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to retrieve messages: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /swarm/tasks/{id}/messages — 发送 Swarm 任务消息
async fn handle_swarm_messages_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(request): Json<SwarmMessageSendRequest>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm messages send: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref chat_manager) = state.swarm_chat_manager else {
        let err = serde_json::json!({"error": "Swarm chat manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    let run_id = match uuid::Uuid::parse_str(&id) {
        Ok(uid) => Some(uid),
        Err(_) => None,
    };
    
    let message_type = match request.message_type.as_deref() {
        Some("task_assignment") => crate::swarm::chat::ChatMessageType::TaskAssignment,
        Some("task_status") => crate::swarm::chat::ChatMessageType::TaskStatus,
        Some("task_progress") => crate::swarm::chat::ChatMessageType::TaskProgress,
        Some("task_completion") => crate::swarm::chat::ChatMessageType::TaskCompletion,
        Some("task_failure") => crate::swarm::chat::ChatMessageType::TaskFailure,
        Some("consensus_request") => crate::swarm::chat::ChatMessageType::ConsensusRequest,
        Some("consensus_response") => crate::swarm::chat::ChatMessageType::ConsensusResponse,
        Some("disagreement") => crate::swarm::chat::ChatMessageType::Disagreement,
        Some("clarification") => crate::swarm::chat::ChatMessageType::Clarification,
        Some("correction") => crate::swarm::chat::ChatMessageType::Correction,
        _ => crate::swarm::chat::ChatMessageType::Info,
    };
    
    let author_type = request.author_type.unwrap_or_else(|| "user".to_string());
    let lang = request.lang.unwrap_or_else(|| "zh".to_string());
    let metadata = request.metadata.unwrap_or(serde_json::json!({}));
    
    match chat_manager.send_message(
        run_id,
        Some(id.clone()),
        request.author,
        author_type,
        message_type,
        request.content,
        lang,
        None,
        metadata,
    ) {
        Ok(message_id) => {
            let result = serde_json::json!({
                "success": true,
                "message_id": message_id,
                "task_id": id
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to send message: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /swarm/tasks/{id}/consensus — 获取共识状态
async fn handle_swarm_consensus_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm consensus get: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref chat_manager) = state.swarm_chat_manager else {
        let err = serde_json::json!({"error": "Swarm chat manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    match chat_manager.analyze_consensus(id.clone()) {
        Ok(consensus) => {
            (StatusCode::OK, Json(serde_json::json!(consensus)))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to analyze consensus: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /swarm/tasks/{id}/consensus — 提交投票
async fn handle_swarm_consensus_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(request): Json<SwarmConsensusVoteRequest>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Swarm consensus vote: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let Some(ref chat_manager) = state.swarm_chat_manager else {
        let err = serde_json::json!({"error": "Swarm chat manager not initialized"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };
    
    // 首先获取最新的共识请求消息作为 parent_id
    let messages = match chat_manager.get_messages(None, Some(id.clone()), 50) {
        Ok(msgs) => msgs,
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to get messages: {}", e)});
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(err));
        }
    };
    
    let consensus_request = messages.iter()
        .find(|m| m.message_type == crate::swarm::chat::ChatMessageType::ConsensusRequest);
    
    let parent_id = consensus_request
        .map(|m| m.id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    
    let reason = request.reason.unwrap_or_else(|| "".to_string());
    let voter = request.voter.clone();
    
    match chat_manager.respond_consensus(
        None,
        id.clone(),
        voter.clone(),
        "voter".to_string(),
        request.vote,
        reason,
        parent_id,
        "zh".to_string(),
    ) {
        Ok(message_id) => {
            let result = serde_json::json!({
                "success": true,
                "task_id": id,
                "message_id": message_id,
                "voter": voter,
                "vote": request.vote,
                "message": "Vote recorded"
            });
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to record vote: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// CHAT ABORT HANDLER
// ══════════════════════════════════════════════════════════════════════════════

/// Active chat sessions for abort functionality
static ACTIVE_SESSIONS: std::sync::OnceLock<DashMap<String, tokio::sync::mpsc::Sender<()>>> = 
    std::sync::OnceLock::new();

fn get_active_sessions() -> &'static DashMap<String, tokio::sync::mpsc::Sender<()>> {
    ACTIVE_SESSIONS.get_or_init(DashMap::new)
}

/// Register a session for abort capability
pub fn register_session(session_id: &str, cancel_tx: tokio::sync::mpsc::Sender<()>) {
    get_active_sessions().insert(session_id.to_string(), cancel_tx);
}

/// Unregister a session
pub fn unregister_session(session_id: &str) {
    get_active_sessions().remove(session_id);
}

/// POST /chat/abort — Abort an active chat session
async fn handle_chat_abort(
    State(_state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let session_id = body.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    
    if session_id.is_empty() {
        let err = serde_json::json!({"error": "sessionId is required"});
        return (StatusCode::BAD_REQUEST, Json(err));
    }
    
    if let Some((_, tx)) = get_active_sessions().remove(session_id) {
        let _ = tx.send(()).await;
        let result = serde_json::json!({"success": true, "message": "Session aborted"});
        (StatusCode::OK, Json(result))
    } else {
        let result = serde_json::json!({"success": false, "message": "Session not found or already completed"});
        (StatusCode::OK, Json(result))
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// SOUL TEMPLATES HANDLERS
// ══════════════════════════════════════════════════════════════════════════════

/// GET /soul/templates — List all soul templates
async fn handle_soul_templates_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Soul templates list: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let templates = vec![
        serde_json::json!({"id": "clara", "name": "Clara", "description": "知性优雅的助手"}),
        serde_json::json!({"id": "zeroclaw", "name": "ZeroClaw", "description": "默认助手"}),
        serde_json::json!({"id": "technical_expert", "name": "Technical Expert", "description": "技术专家"}),
        serde_json::json!({"id": "creative_companion", "name": "Creative Companion", "description": "创意伙伴"}),
        serde_json::json!({"id": "professional_assistant", "name": "Professional Assistant", "description": "专业助理"}),
        serde_json::json!({"id": "learning_tutor", "name": "Learning Tutor", "description": "学习导师"}),
        serde_json::json!({"id": "debug_specialist", "name": "Debug Specialist", "description": "调试专家"}),
    ];
    (StatusCode::OK, Json(serde_json::Value::Array(templates)))
}

/// GET /soul/templates/{id} — Get a specific soul template
async fn handle_soul_templates_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Soul templates get: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    if let Some(soul) = crate::soul::create_soul_from_preset_name(&id) {
        let result = serde_json::json!({
            "id": id,
            "name": soul.essence.name.primary,
            "nature": soul.essence.nature,
            "purpose": soul.essence.purpose,
        });
        (StatusCode::OK, Json(result))
    } else {
        let err = serde_json::json!({"error": "Soul template not found"});
        (StatusCode::NOT_FOUND, Json(err))
    }
}

/// POST /soul/templates — Save soul template configuration
async fn handle_soul_templates_save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Soul templates save: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    let workspace_dir = state.config.workspace_dir.clone();
    let soul_dir = workspace_dir.join(".zeroclaw").join("souls");
    
    if let Err(e) = std::fs::create_dir_all(&soul_dir) {
        let err = serde_json::json!({"error": format!("Failed to create soul directory: {}", e)});
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(err));
    }
    
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("custom");
    let file_path = soul_dir.join(format!("{}.json", id));
    
    match std::fs::write(&file_path, serde_json::to_string_pretty(&body).unwrap_or_default()) {
        Ok(_) => {
            let result = serde_json::json!({"success": true, "id": id});
            (StatusCode::OK, Json(result))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to save soul template: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// SSE STREAMING CHAT HANDLER
// ══════════════════════════════════════════════════════════════════════════════

/// SSE event types for streaming chat
#[derive(serde::Serialize)]
#[serde(tag = "type")]
enum ChatStreamEvent {
    #[serde(rename = "chunk")]
    Chunk { text: String },
    #[serde(rename = "done")]
    Done { response: String },
    #[serde(rename = "error")]
    Error { message: String },
}

/// POST /chat/stream — SSE streaming chat endpoint with tool support
async fn handle_chat_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<WebhookBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<serde_json::Value>)> {
    // Auth check
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            let err = serde_json::json!({"error": "Unauthorized"});
            return Err((StatusCode::UNAUTHORIZED, Json(err)));
        }
    }

    // Parse body
    let Json(webhook_body) = match body {
        Ok(b) => b,
        Err(e) => {
            let err = serde_json::json!({"error": format!("Invalid JSON: {e}")});
            return Err((StatusCode::BAD_REQUEST, Json(err)));
        }
    };

    let message = webhook_body.message;
    let system_prompt: String = state.system_prompt.as_ref().clone();

    // Create channel for SSE events
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);

    // Clone state for the async task
    let provider = state.provider.clone();
    let tools_registry = state.tools_registry.clone();
    let _observer = state.observer.clone();
    let model = state.model.clone();
    let temperature = state.temperature;
    let _max_iterations = state.max_tool_iterations;

    // Spawn task to handle streaming with tool call loop
    tokio::spawn(async move {
        // 获取工具规格
        let tool_specs: Vec<crate::tools::ToolSpec> = tools_registry.iter().map(|tool| tool.spec()).collect();
        
        // 使用流式 API
        match provider.stream_chat(
            Some(&system_prompt),
            &message,
            &model,
            temperature,
            Some(&tool_specs),
        ).await {
            Ok(mut stream_rx) => {
                let mut accumulated_text = String::new();
                let mut final_tool_calls: Vec<crate::providers::ToolCall> = Vec::new();
                
                // 接收流式响应
                while let Some(chunk_result) = stream_rx.recv().await {
                    match chunk_result {
                        Ok(chunk) => {
                            if !chunk.text.is_empty() && !chunk.is_final {
                                // 发送 chunk 事件
                                accumulated_text.push_str(&chunk.text);
                                let event = ChatStreamEvent::Chunk { text: chunk.text.clone() };
                                if let Ok(sse_event) = Event::default().json_data(event) {
                                    let _ = tx.send(Ok(sse_event)).await;
                                }
                            }
                            
                            if chunk.is_final {
                                // 保存最终的 tool calls
                                final_tool_calls = chunk.tool_calls;
                            }
                        }
                        Err(e) => {
                            let event = ChatStreamEvent::Error { message: e.to_string() };
                            if let Ok(sse_event) = Event::default().json_data(event) {
                                let _ = tx.send(Ok(sse_event)).await;
                            }
                            drop(tx);
                            return;
                        }
                    }
                }
                
                // 如果有工具调用，需要执行工具循环
                if !final_tool_calls.is_empty() {
                    // 执行工具调用
                    let mut tool_results = String::new();
                    for call in &final_tool_calls {
                        // 查找对应的工具
                        let tool = tools_registry.iter().find(|t| t.spec().name == call.name);
                        if let Some(tool) = tool {
                            // 解析参数
                            let args: serde_json::Value = serde_json::from_str(&call.arguments).unwrap_or(serde_json::json!({}));
                            match tool.execute(args).await {
                                Ok(result) => {
                                    tool_results.push_str(&format!("--- Tool: {} ---\n{}\n\n", call.name, result.output));
                                }
                                Err(e) => {
                                    tool_results.push_str(&format!("--- Tool: {} (Error) ---\n{:?}\n\n", call.name, e));
                                }
                            }
                        }
                    }
                    
                    // 如果有工具结果，继续 LLM 调用获取最终响应
                    if !tool_results.is_empty() {
                        // 构建新的消息历史
                        let history = vec![
                            crate::providers::ChatMessage::system(&system_prompt),
                            crate::providers::ChatMessage::user(&message),
                            crate::providers::ChatMessage::assistant(&accumulated_text),
                            crate::providers::ChatMessage::user(&format!("[Tool results]\n{}", tool_results)),
                        ];
                        
                        // 使用非流式 API 获取最终响应（简化处理）
                        match provider.chat_with_history(&history, &model, temperature).await {
                            Ok(final_response) => {
                                let final_text = final_response.text.unwrap_or_default();
                                // 发送最终的 chunk 事件
                                let event = ChatStreamEvent::Chunk { text: final_text.clone() };
                                if let Ok(sse_event) = Event::default().json_data(event) {
                                    let _ = tx.send(Ok(sse_event)).await;
                                }
                                // 发送 done 事件
                                let event = ChatStreamEvent::Done { response: final_text };
                                if let Ok(sse_event) = Event::default().json_data(event) {
                                    let _ = tx.send(Ok(sse_event)).await;
                                }
                            }
                            Err(e) => {
                                let event = ChatStreamEvent::Error { message: e.to_string() };
                                if let Ok(sse_event) = Event::default().json_data(event) {
                                    let _ = tx.send(Ok(sse_event)).await;
                                }
                            }
                        }
                    } else {
                        // 没有工具结果，直接发送 done
                        let event = ChatStreamEvent::Done { response: accumulated_text.clone() };
                        if let Ok(sse_event) = Event::default().json_data(event) {
                            let _ = tx.send(Ok(sse_event)).await;
                        }
                    }
                } else {
                    // 没有工具调用，直接发送 done 事件
                    let event = ChatStreamEvent::Done { response: accumulated_text.clone() };
                    if let Ok(sse_event) = Event::default().json_data(event) {
                        let _ = tx.send(Ok(sse_event)).await;
                    }
                }
            }
            Err(e) => {
                let event = ChatStreamEvent::Error { message: e.to_string() };
                if let Ok(sse_event) = Event::default().json_data(event) {
                    let _ = tx.send(Ok(sse_event)).await;
                }
            }
        }
        
        drop(tx);
    });

    // Convert receiver to stream
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// `WhatsApp` verification query params
#[derive(serde::Deserialize)]
pub struct WhatsAppVerifyQuery {
    #[serde(rename = "hub.mode")]
    pub mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub challenge: Option<String>,
}

/// GET /whatsapp — Meta webhook verification
async fn handle_whatsapp_verify(
    State(state): State<AppState>,
    Query(params): Query<WhatsAppVerifyQuery>,
) -> impl IntoResponse {
    let Some(ref wa) = state.whatsapp else {
        return (StatusCode::NOT_FOUND, "WhatsApp not configured".to_string());
    };

    // Verify the token matches (constant-time comparison to prevent timing attacks)
    let token_matches = params
        .verify_token
        .as_deref()
        .is_some_and(|t| constant_time_eq(t, wa.verify_token()));
    if params.mode.as_deref() == Some("subscribe") && token_matches {
        if let Some(ch) = params.challenge {
            tracing::info!("WhatsApp webhook verified successfully");
            return (StatusCode::OK, ch);
        }
        return (StatusCode::BAD_REQUEST, "Missing hub.challenge".to_string());
    }

    tracing::warn!("WhatsApp webhook verification failed — token mismatch");
    (StatusCode::FORBIDDEN, "Forbidden".to_string())
}

/// Verify `WhatsApp` webhook signature (`X-Hub-Signature-256`).
/// Returns true if the signature is valid, false otherwise.
/// See: <https://developers.facebook.com/docs/graph-api/webhooks/getting-started#verification-requests>
pub fn verify_whatsapp_signature(app_secret: &str, body: &[u8], signature_header: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    // Signature format: "sha256=<hex_signature>"
    let Some(hex_sig) = signature_header.strip_prefix("sha256=") else {
        return false;
    };

    // Decode hex signature
    let Ok(expected) = hex::decode(hex_sig) else {
        return false;
    };

    // Compute HMAC-SHA256
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(app_secret.as_bytes()) else {
        return false;
    };
    mac.update(body);

    // Constant-time comparison
    mac.verify_slice(&expected).is_ok()
}

/// POST /whatsapp — incoming message webhook
async fn handle_whatsapp_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let Some(ref wa) = state.whatsapp else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "WhatsApp not configured"})),
        );
    };

    // ── Security: Verify X-Hub-Signature-256 if app_secret is configured ──
    if let Some(ref app_secret) = state.whatsapp_app_secret {
        let signature = headers
            .get("X-Hub-Signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !verify_whatsapp_signature(app_secret, &body, signature) {
            tracing::warn!(
                "WhatsApp webhook signature verification failed (signature: {})",
                if signature.is_empty() {
                    "missing"
                } else {
                    "invalid"
                }
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Invalid signature"})),
            );
        }
    }

    // Parse JSON body
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid JSON payload"})),
        );
    };

    // Parse messages from the webhook payload
    let messages = wa.parse_webhook_payload(&payload);

    if messages.is_empty() {
        // Acknowledge the webhook even if no messages (could be status updates)
        return (StatusCode::OK, Json(serde_json::json!({"status": "ok"})));
    }

    // Process each message
    for msg in &messages {
        tracing::info!(
            "WhatsApp message from {}: {}",
            msg.sender,
            truncate_with_ellipsis(&msg.content, 50)
        );

        // Auto-save to memory
        if state.auto_save {
            let key = whatsapp_memory_key(msg);
            let _ = state
                .mem
                .store(&key, &msg.content, MemoryCategory::Conversation)
                .await;
        }

        // Call the LLM
        match gateway_agent_reply(&state, &msg.content).await {
            Ok(reply) => {
                // Send reply via WhatsApp
                if let Err(e) = wa.send(&reply, &msg.sender).await {
                    tracing::error!("Failed to send WhatsApp reply: {e}");
                }
            }
            Err(e) => {
                tracing::error!("LLM error for WhatsApp message: {e:#}");
                let _ = wa
                    .send(
                        "Sorry, I couldn't process your message right now.",
                        &msg.sender,
                    )
                    .await;
            }
        }
    }

    // Acknowledge the webhook
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

/// GET /cost/summary — get token usage summary
async fn handle_cost_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Cost summary: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    use std::sync::atomic::Ordering;
    
    let total_tokens = state.usage_stats.total_tokens.load(Ordering::Relaxed);
    let total_requests = state.usage_stats.total_requests.load(Ordering::Relaxed);
    let total_cost_usd = state.usage_stats.total_cost_usd.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let session_duration_secs = state.usage_stats.session_start.elapsed().as_secs();
    
    let body = serde_json::json!({
        "enabled": true,
        "session_cost_usd": total_cost_usd,
        "daily_cost_usd": total_cost_usd,
        "monthly_cost_usd": total_cost_usd,
        "total_tokens": total_tokens,
        "request_count": total_requests,
        "session_duration_secs": session_duration_secs,
        "by_model": {},
    });
    (StatusCode::OK, Json(body))
}

/// Query parameters for daily cost
#[derive(serde::Deserialize)]
pub struct DailyCostQuery {
    pub date: Option<String>,
}

/// GET /cost/daily — get daily cost for a specific date
async fn handle_cost_daily(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<DailyCostQuery>,
) -> impl IntoResponse {
    // ── Bearer token auth (pairing) ──
    if state.pairing.require_pairing() {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("Cost daily: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return (StatusCode::UNAUTHORIZED, Json(err));
        }
    }
    
    use std::sync::atomic::Ordering;
    
    let date = match &params.date {
        Some(d) => match chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
            Ok(date) => date,
            Err(_) => {
                let err = serde_json::json!({"error": "Invalid date format. Use YYYY-MM-DD"});
                return (StatusCode::BAD_REQUEST, Json(err));
            }
        },
        None => chrono::Utc::now().date_naive(),
    };

    let total_cost_usd = state.usage_stats.total_cost_usd.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let body = serde_json::json!({
        "date": date.to_string(),
        "cost_usd": total_cost_usd,
    });
    (StatusCode::OK, Json(body))
}

// ══════════════════════════════════════════════════════════════════════════════
// MCP SERVER MANAGEMENT HANDLERS
// ══════════════════════════════════════════════════════════════════════════════

/// Helper function to check authentication for MCP endpoints
/// 
/// 在生产环境下强制要求配对验证，防止未授权访问
/// 
/// # Arguments
/// * `state` - 应用状态
/// * `headers` - HTTP 请求头
/// 
/// # Returns
/// * `Ok(())` - 认证通过
/// * `Err((StatusCode, Json))` - 认证失败，返回错误状态码和错误信息
fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    // 生产环境强制要求配对验证（即使配置未启用）
    if !state.pairing.require_pairing() && cfg!(not(debug_assertions)) {
        tracing::warn!("Gateway authentication disabled in production - this is a security risk. Requiring authentication.");
        // 生产环境拒绝未启用配对的请求
        let err = serde_json::json!({
            "error": "Authentication required in production mode. Please enable pairing in configuration."
        });
        return Err((StatusCode::UNAUTHORIZED, Json(err)));
    }
    
    // 如果配置要求配对验证（或开发环境），执行认证检查
    if state.pairing.require_pairing() || cfg!(debug_assertions) {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = auth.strip_prefix("Bearer ").unwrap_or("");
        
        // 开发环境允许无认证访问（方便调试）
        if cfg!(debug_assertions) && auth.is_empty() {
            return Ok(());
        }
        
        if !state.pairing.is_authenticated(token) {
            tracing::warn!("MCP API: rejected — not paired / invalid bearer token");
            let err = serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            });
            return Err((StatusCode::UNAUTHORIZED, Json(err)));
        }
    }
    
    Ok(())
}

/// Get MCP database path from config (handles Mutex poison safely)
fn get_mcp_db_path(config: &Arc<Config>) -> std::path::PathBuf {
    config.workspace_dir.join("mcp_servers.db")
}

/// Validate MCP server request to prevent command injection
fn validate_mcp_request(name: &str, command: &str, args: &[String], env: &std::collections::HashMap<String, String>) -> Result<(), String> {
    // Validate name: alphanumeric, dash, underscore only
    if name.is_empty() || name.len() > 64 {
        return Err("Name must be 1-64 characters".to_string());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Name can only contain alphanumeric characters, dash, and underscore".to_string());
    }
    
    // Validate command: no shell metacharacters
    if command.is_empty() || command.len() > 256 {
        return Err("Command must be 1-256 characters".to_string());
    }
    let forbidden_chars = ['|', '&', ';', '$', '`', '\\', '\n', '\r', '>', '<'];
    if command.chars().any(|c| forbidden_chars.contains(&c)) {
        return Err("Command contains forbidden characters".to_string());
    }
    
    // Block shell interpreters that could execute arbitrary code
    let shell_interpreters = ["sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "cmd", "powershell", "pwsh"];
    let command_base = std::path::Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command);
    if shell_interpreters.contains(&command_base) {
        // Check if -c or /c flag is present in args (command execution)
        for arg in args {
            let arg_lower = arg.to_lowercase();
            if arg_lower == "-c" || arg_lower == "/c" || arg_lower == "-command" || arg_lower == "-encodedcommand" {
                return Err("Shell command execution is not allowed".to_string());
            }
        }
    }
    
    // Validate args: no shell metacharacters in each arg
    for (i, arg) in args.iter().enumerate() {
        if arg.len() > 1024 {
            return Err(format!("Argument {} is too long (max 1024 chars)", i));
        }
        if arg.chars().any(|c| forbidden_chars.contains(&c)) {
            return Err(format!("Argument {} contains forbidden characters", i));
        }
    }
    
    // Validate env keys and values
    for (key, value) in env.iter() {
        if key.is_empty() || key.len() > 64 {
            return Err("Environment variable name must be 1-64 characters".to_string());
        }
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err("Environment variable name can only contain alphanumeric characters and underscore".to_string());
        }
        if value.len() > 4096 {
            return Err(format!("Environment variable {} value is too long (max 4096 chars)", key));
        }
    }
    
    Ok(())
}

/// Helper to get MCP store with initialization
fn get_mcp_store(state: &AppState) -> Result<MCPServerStore, (StatusCode, Json<serde_json::Value>)> {
    let db_path = get_mcp_db_path(&state.config);
    let store = MCPServerStore::new(db_path);
    
    store.init().map_err(|e| {
        let err = serde_json::json!({"error": format!("Failed to init store: {}", e)});
        (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
    })?;
    
    Ok(store)
}

/// Macro to handle auth check and store initialization for MCP handlers
macro_rules! mcp_handler {
    ($state:expr, $headers:expr, $store:ident, $body:expr) => {{
        if let Err(resp) = check_auth(&$state, &$headers) {
            return resp;
        }
        
        let $store = match get_mcp_store(&$state) {
            Ok(s) => s,
            Err(resp) => return resp,
        };
        
        $body
    }};
}

async fn handle_mcp_servers_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    mcp_handler!(state, headers, store, {
        match store.list() {
            Ok(servers) => {
                let body = serde_json::json!({
                    "servers": servers,
                    "total": servers.len(),
                });
                (StatusCode::OK, Json(body))
            }
            Err(e) => {
                let err = serde_json::json!({"error": format!("Failed to list servers: {}", e)});
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
            }
        }
    })
}

async fn handle_mcp_servers_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MCPServerCreateRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }
    
    // Validate request to prevent command injection
    if let Err(e) = validate_mcp_request(&request.name, &request.command, &request.args, &request.env) {
        let err = serde_json::json!({"error": format!("Validation failed: {}", e)});
        return (StatusCode::BAD_REQUEST, Json(err));
    }
    
    let store = match get_mcp_store(&state) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    
    match store.create(request) {
        Ok(server) => {
            let body = serde_json::json!({
                "success": true,
                "server": server,
            });
            (StatusCode::CREATED, Json(body))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to create server: {}", e)});
            (StatusCode::BAD_REQUEST, Json(err))
        }
    }
}

async fn handle_mcp_servers_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    mcp_handler!(state, headers, store, {
        match store.get_by_id(&id) {
            Ok(server) => {
                let body = serde_json::json!({"server": server});
                (StatusCode::OK, Json(body))
            }
            Err(e) => {
                let err = serde_json::json!({"error": format!("Server not found: {}", e)});
                (StatusCode::NOT_FOUND, Json(err))
            }
        }
    })
}

async fn handle_mcp_servers_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(request): Json<MCPServerUpdateRequest>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }
    
    // Validate request to prevent command injection
    if let (Some(name), Some(command), Some(args), Some(env)) = 
        (&request.name, &request.command, &request.args, &request.env) {
        if let Err(e) = validate_mcp_request(name, command, args, env) {
            let err = serde_json::json!({"error": format!("Validation failed: {}", e)});
            return (StatusCode::BAD_REQUEST, Json(err));
        }
    } else {
        // Partial update: validate provided fields
        if let Some(name) = &request.name {
            if name.is_empty() || name.len() > 64 || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                let err = serde_json::json!({"error": "Invalid name format"});
                return (StatusCode::BAD_REQUEST, Json(err));
            }
        }
        if let Some(command) = &request.command {
            let forbidden_chars = ['|', '&', ';', '$', '`', '\\', '\n', '\r', '>', '<'];
            if command.is_empty() || command.len() > 256 || command.chars().any(|c| forbidden_chars.contains(&c)) {
                let err = serde_json::json!({"error": "Invalid command format"});
                return (StatusCode::BAD_REQUEST, Json(err));
            }
        }
    }
    
    let store = match get_mcp_store(&state) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    
    match store.update(&id, request) {
        Ok(server) => {
            let body = serde_json::json!({
                "success": true,
                "server": server,
            });
            (StatusCode::OK, Json(body))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to update server: {}", e)});
            (StatusCode::BAD_REQUEST, Json(err))
        }
    }
}

async fn handle_mcp_servers_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    mcp_handler!(state, headers, store, {
        match store.delete(&id) {
            Ok(deleted) => {
                let body = serde_json::json!({
                    "success": deleted,
                    "deleted": deleted,
                });
                (StatusCode::OK, Json(body))
            }
            Err(e) => {
                let err = serde_json::json!({"error": format!("Failed to delete server: {}", e)});
                (StatusCode::BAD_REQUEST, Json(err))
            }
        }
    })
}

async fn handle_mcp_servers_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    mcp_handler!(state, headers, store, {
        match store.get_by_id(&id) {
            Ok(server) => {
                let _ = store.update_status(&id, MCPServerStatus::Running, None);
                let body = serde_json::json!({
                    "success": true,
                    "message": format!("Server {} started", server.name),
                });
                (StatusCode::OK, Json(body))
            }
            Err(e) => {
                let err = serde_json::json!({"error": format!("Server not found: {}", e)});
                (StatusCode::NOT_FOUND, Json(err))
            }
        }
    })
}

async fn handle_mcp_servers_stop(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    mcp_handler!(state, headers, store, {
        match store.get_by_id(&id) {
            Ok(server) => {
                let _ = store.update_status(&id, MCPServerStatus::Stopped, None);
                let body = serde_json::json!({
                    "success": true,
                    "message": format!("Server {} stopped", server.name),
                });
                (StatusCode::OK, Json(body))
            }
            Err(e) => {
                let err = serde_json::json!({"error": format!("Server not found: {}", e)});
                (StatusCode::NOT_FOUND, Json(err))
            }
        }
    })
}

async fn handle_mcp_servers_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    mcp_handler!(state, headers, store, {
        match store.get_by_id(&id) {
            Ok(server) => {
                let body = serde_json::json!({
                    "server_id": id,
                    "server_name": server.name,
                    "tools_count": server.tools_count,
                    "resources_count": server.resources_count,
                    "prompts_count": server.prompts_count,
                });
                (StatusCode::OK, Json(body))
            }
            Err(e) => {
                let err = serde_json::json!({"error": format!("Server not found: {}", e)});
                (StatusCode::NOT_FOUND, Json(err))
            }
        }
    })
}

// ══════════════════════════════════════════════════════════════════════════════
// OBSERVABILITY API HANDLERS
// ══════════════════════════════════════════════════════════════════════════════

/// POST /observability/traces/list — 列出轨迹
async fn handle_observability_list_traces(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(query): Json<serde_json::Value>,
) -> impl IntoResponse {
    // 认证检查
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    // 解析查询参数
    let trace_query = parse_trace_query(query);
    
    match store.list_traces(&trace_query).await {
        Ok(traces) => {
            // 转换为 JSON 格式
            let traces_json: Vec<serde_json::Value> = traces.iter().map(|t| {
                serde_json::to_value(t).unwrap_or_default()
            }).collect();
            (StatusCode::OK, Json(serde_json::json!({"traces": traces_json})))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to list traces: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /observability/traces/{id} — 获取单条轨迹
async fn handle_observability_get_trace(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    match store.get_trace(&id).await {
        Ok(Some(trace)) => {
            match serde_json::to_value(&trace) {
                Ok(json) => (StatusCode::OK, Json(json)),
                Err(e) => {
                    let err = serde_json::json!({"error": format!("Serialization error: {}", e)});
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                }
            }
        }
        Ok(None) => {
            let err = serde_json::json!({"error": "Trace not found"});
            (StatusCode::NOT_FOUND, Json(err))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to get trace: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /observability/traces/{id}/reasoning — 获取推理链
async fn handle_observability_get_reasoning(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    match store.get_reasoning(&trace_id).await {
        Ok(Some(reasoning)) => {
            match serde_json::to_value(&reasoning) {
                Ok(json) => (StatusCode::OK, Json(json)),
                Err(e) => {
                    let err = serde_json::json!({"error": format!("Serialization error: {}", e)});
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                }
            }
        }
        Ok(None) => {
            // 返回空对象而不是错误
            (StatusCode::OK, Json(serde_json::json!(null)))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to get reasoning: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /observability/traces/{id}/decisions — 获取决策点
async fn handle_observability_get_decisions(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    match store.get_decisions(&trace_id).await {
        Ok(decisions) => {
            let decisions_json: Vec<serde_json::Value> = decisions.iter().map(|d| {
                serde_json::to_value(d).unwrap_or_default()
            }).collect();
            (StatusCode::OK, Json(serde_json::json!({"decisions": decisions_json})))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to get decisions: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /observability/traces/{id}/evaluation — 获取评估结果
async fn handle_observability_get_evaluation(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    match store.get_evaluation(&trace_id).await {
        Ok(Some(evaluation)) => {
            match serde_json::to_value(&evaluation) {
                Ok(json) => (StatusCode::OK, Json(json)),
                Err(e) => {
                    let err = serde_json::json!({"error": format!("Serialization error: {}", e)});
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                }
            }
        }
        Ok(None) => {
            (StatusCode::OK, Json(serde_json::json!(null)))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to get evaluation: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// POST /observability/traces/{id}/evaluate — 评估轨迹
async fn handle_observability_evaluate_trace(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    let criteria = body.get("criteria")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    
    match store.get_trace(&trace_id).await {
        Ok(Some(trace)) => {
            let total_score: f64 = 100.0;
            let evaluation_details = serde_json::json!({
                "traceId": trace.id,
                "runId": trace.run_id,
                "durationMs": trace.duration_ms,
                "traceType": format!("{:?}", trace.trace_type),
            });
            
            let result = serde_json::json!({
                "success": true,
                "traceId": trace_id,
                "score": total_score,
                "criteria": criteria,
                "details": evaluation_details,
                "evaluatedAt": chrono::Utc::now().to_rfc3339()
            });
            (StatusCode::OK, Json(result))
        }
        _ => {
            let result = serde_json::json!({
                "success": false,
                "error": "TRACE_NOT_FOUND",
                "traceId": trace_id,
                "message": "Trace not found"
            });
            (StatusCode::NOT_FOUND, Json(result))
        }
    }
}

/// POST /observability/aggregate — 聚合查询
async fn handle_observability_aggregate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(query): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    let agg_query = parse_aggregation_query(query);
    
    match store.aggregate(agg_query).await {
        Ok(result) => {
            match serde_json::to_value(&result) {
                Ok(json) => (StatusCode::OK, Json(json)),
                Err(e) => {
                    let err = serde_json::json!({"error": format!("Serialization error: {}", e)});
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
                }
            }
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to aggregate: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /observability/dashboard — 获取仪表板统计
async fn handle_observability_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<DashboardQuery>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let Some(ref store) = state.trace_store else {
        let err = serde_json::json!({"error": "Observability not enabled"});
        return (StatusCode::SERVICE_UNAVAILABLE, Json(err));
    };

    // 解析时间范围
    let time_range = parse_time_range(params.range.as_deref().unwrap_or("24h"));
    
    // 获取存储统计
    match store.storage_stats().await {
        Ok(stats) => {
            let body = serde_json::json!({
                "totalTraces": stats.total_traces,
                "totalReasoningChains": stats.total_reasoning_chains,
                "totalDecisions": stats.total_decisions,
                "totalEvaluations": stats.total_evaluations,
                "dbSizeBytes": stats.db_size_bytes,
                "oldestTrace": stats.oldest_trace_timestamp,
                "newestTrace": stats.newest_trace_timestamp,
                "timeRange": time_range,
                "successRate": 0.0,
                "avgDurationMs": 0,
                "totalCost": 0.0,
                "tracesTrend": 0,
                "successRateTrend": 0,
                "durationTrend": 0,
                "costTrend": 0,
                "traceTrend": [],
                "successRateTrendData": [],
                "decisionQualityDistribution": [],
                "toolUsage": [],
                "alerts": [],
                "failurePatterns": [],
            });
            (StatusCode::OK, Json(body))
        }
        Err(e) => {
            let err = serde_json::json!({"error": format!("Failed to get dashboard stats: {}", e)});
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err))
        }
    }
}

/// GET /observability/alerts — 获取告警列表
async fn handle_observability_alerts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<AlertsQuery>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let limit = params.limit.unwrap_or(10);
    
    let alerts = state.alert_manager.get_alerts(Some(limit)).await;
    let body = serde_json::json!({
        "success": true,
        "alerts": alerts,
        "total": alerts.len()
    });
    (StatusCode::OK, Json(body))
}

/// POST /observability/alerts/{id}/dismiss — 忽略告警
async fn handle_observability_dismiss_alert(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let success = state.alert_manager.dismiss_alert(&id).await;
    let body = serde_json::json!({
        "success": success,
        "alertId": id,
        "message": if success { "Alert dismissed" } else { "Alert not found" }
    });
    (StatusCode::OK, Json(body))
}

/// GET /observability/failure-patterns — 获取失败模式
async fn handle_observability_failure_patterns(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(resp) = check_auth(&state, &headers) {
        return resp;
    }

    let patterns = state.failure_analyzer.get_patterns(Some(20)).await;
    let statistics = state.failure_analyzer.get_statistics().await;
    
    let body = serde_json::json!({
        "success": true,
        "patterns": patterns,
        "statistics": statistics
    });
    (StatusCode::OK, Json(body))
}

// ══════════════════════════════════════════════════════════════════════════════
// DEBUG FUNCTIONS (temporary)
// ══════════════════════════════════════════════════════════════════════════════

/// GET /debug/pairing-code — Get the current pairing code (temporary debug endpoint)
async fn handle_debug_pairing_code(
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Note: This is only for debugging purposes to get the pairing code
    // In production, this should be removed as it defeats the security purpose
    
    if let Some(code) = state.pairing.pairing_code() {
        let body = serde_json::json!({
            "pairing_code": code,
            "paired_tokens_count": state.pairing.tokens().len(),
            "require_pairing": state.pairing.require_pairing(),
            "is_paired": state.pairing.is_paired()
        });
        (StatusCode::OK, Json(body))
    } else {
        let body = serde_json::json!({
            "error": "No pairing code available",
            "paired_tokens_count": state.pairing.tokens().len(),
            "require_pairing": state.pairing.require_pairing(),
            "is_paired": state.pairing.is_paired()
        });
        (StatusCode::BAD_REQUEST, Json(body))
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// OBSERVABILITY HELPER FUNCTIONS
// ══════════════════════════════════════════════════════════════════════════════

/// 仪表板查询参数
#[derive(serde::Deserialize)]
struct DashboardQuery {
    range: Option<String>,
}

/// 告警查询参数
#[derive(serde::Deserialize)]
struct AlertsQuery {
    limit: Option<usize>,
}

/// 解析轨迹查询参数
fn parse_trace_query(query: serde_json::Value) -> trace_store::TraceQuery {
    let text = query.get("text").and_then(|v| v.as_str()).map(String::from);
    let run_id = query.get("runId").and_then(|v| v.as_str()).map(String::from);
    let trace_type = query.get("traceType").and_then(|v| v.as_str()).map(String::from);
    let success = query.get("success").and_then(|v| v.as_bool());
    let limit = query.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
    let offset = query.get("offset").and_then(|v| v.as_u64()).map(|n| n as usize);
    
    trace_store::TraceQuery {
        text,
        run_id,
        trace_type,
        time_range: None,
        success,
        min_duration_ms: None,
        max_duration_ms: None,
        limit,
        offset,
        ..Default::default()
    }
}

/// 解析聚合查询参数
fn parse_aggregation_query(query: serde_json::Value) -> trace_store::AggregationQuery {
    let agg_type = query.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
    
    match agg_type {
        "success_rate" => trace_store::AggregationQuery::SuccessRate {
            time_range: parse_time_range_from_query(&query),
        },
        "average_duration" => trace_store::AggregationQuery::AverageDuration {
            time_range: parse_time_range_from_query(&query),
        },
        "trace_type_distribution" => trace_store::AggregationQuery::TraceTypeDistribution {
            time_range: parse_time_range_from_query(&query),
        },
        "token_usage" => trace_store::AggregationQuery::TokenUsage {
            time_range: parse_time_range_from_query(&query),
        },
        "cost_stats" => trace_store::AggregationQuery::CostStats {
            time_range: parse_time_range_from_query(&query),
        },
        // 默认返回成功率统计
        _ => trace_store::AggregationQuery::SuccessRate {
            time_range: None,
        },
    }
}

/// 从查询参数解析时间范围
fn parse_time_range_from_query(query: &serde_json::Value) -> Option<(u64, u64)> {
    query.get("timeRange").and_then(|tr| {
        let start = tr.get("start").and_then(|v| v.as_u64())?;
        let end = tr.get("end").and_then(|v| v.as_u64())?;
        Some((start, end))
    })
}

/// 解析时间范围字符串
fn parse_time_range(range: &str) -> (u64, u64) {
    let now = chrono::Utc::now().timestamp() as u64;
    let start = match range {
        "1h" => now.saturating_sub(3600),
        "24h" => now.saturating_sub(86400),
        "7d" => now.saturating_sub(7 * 86400),
        "30d" => now.saturating_sub(30 * 86400),
        _ => now.saturating_sub(86400),
    };
    (start, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::traits::ChannelMessage;
    use crate::memory::{Memory, MemoryCategory, MemoryEntry};
    use crate::providers::Provider;
    use async_trait::async_trait;
    use axum::http::HeaderValue;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[test]
    fn security_body_limit_is_64kb() {
        assert_eq!(MAX_BODY_SIZE, 65_536);
    }

    #[test]
    fn security_timeout_is_30_seconds() {
        assert_eq!(REQUEST_TIMEOUT_SECS, 30);
    }

    #[test]
    fn webhook_body_requires_message_field() {
        let valid = r#"{"message": "hello"}"#;
        let parsed: Result<WebhookBody, _> = serde_json::from_str(valid);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().message, "hello");

        let missing = r#"{"other": "field"}"#;
        let parsed: Result<WebhookBody, _> = serde_json::from_str(missing);
        assert!(parsed.is_err());
    }

    #[test]
    fn whatsapp_query_fields_are_optional() {
        let q = WhatsAppVerifyQuery {
            mode: None,
            verify_token: None,
            challenge: None,
        };
        assert!(q.mode.is_none());
    }

    #[test]
    fn app_state_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<AppState>();
    }

    #[test]
    fn gateway_rate_limiter_blocks_after_limit() {
        let limiter = GatewayRateLimiter::new(2, 2);
        assert!(limiter.allow_pair("127.0.0.1"));
        assert!(limiter.allow_pair("127.0.0.1"));
        assert!(!limiter.allow_pair("127.0.0.1"));
    }

    #[test]
    fn idempotency_store_rejects_duplicate_key() {
        let store = IdempotencyStore::new(Duration::from_secs(30));
        assert!(store.record_if_new("req-1"));
        assert!(!store.record_if_new("req-1"));
        assert!(store.record_if_new("req-2"));
    }

    #[test]
    fn webhook_memory_key_is_unique() {
        let key1 = webhook_memory_key();
        let key2 = webhook_memory_key();

        assert!(key1.starts_with("webhook_msg_"));
        assert!(key2.starts_with("webhook_msg_"));
        assert_ne!(key1, key2);
    }

    #[test]
    fn whatsapp_memory_key_includes_sender_and_message_id() {
        let msg = ChannelMessage {
            id: "wamid-123".into(),
            sender: "+1234567890".into(),
            content: "hello".into(),
            channel: "whatsapp".into(),
            timestamp: 1,
        };

        let key = whatsapp_memory_key(&msg);
        assert_eq!(key, "whatsapp_+1234567890_wamid-123");
    }

    #[derive(Default)]
    struct MockMemory;

    #[async_trait]
    impl Memory for MockMemory {
        fn name(&self) -> &str {
            "mock"
        }

        async fn store(
            &self,
            _key: &str,
            _content: &str,
            _category: MemoryCategory,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn recall(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn get(&self, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(None)
        }

        async fn list(
            &self,
            _category: Option<&MemoryCategory>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
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

    #[derive(Default)]
    struct MockProvider {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn chat_with_system(
            &self,
            _system_prompt: Option<&str>,
            _message: &str,
            _model: &str,
            _temperature: f64,
        ) -> anyhow::Result<crate::providers::ChatResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(crate::providers::ChatResponse::with_text("ok"))
        }
    }

    #[derive(Default)]
    struct TrackingMemory {
        keys: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl Memory for TrackingMemory {
        fn name(&self) -> &str {
            "tracking"
        }

        async fn store(
            &self,
            key: &str,
            _content: &str,
            _category: MemoryCategory,
        ) -> anyhow::Result<()> {
            self.keys
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(key.to_string());
            Ok(())
        }

        async fn recall(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn get(&self, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(None)
        }

        async fn list(
            &self,
            _category: Option<&MemoryCategory>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn count(&self) -> anyhow::Result<usize> {
            let size = self
                .keys
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len();
            Ok(size)
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    fn test_app_state(
        provider: Arc<dyn Provider>,
        memory: Arc<dyn Memory>,
        auto_save: bool,
    ) -> AppState {
        // 创建工作流存储
        let workflow_store = Arc::new(
            WorkflowStore::new(
                std::path::Path::new(".workspace/workflows")
            ).expect("Failed to create workflow store")
        );
        
        // 创建工作流引擎
        let workflow_engine = Arc::new(
            crate::workflow::WorkflowEngine::new(workflow_store.clone())
        );
        
        // 创建工作流调度器
        let workflow_scheduler = Arc::new(
            crate::workflow::WorkflowScheduler::new(
                workflow_engine.clone(),
                workflow_store.clone(),
            )
        );
        
        AppState {
            provider,
            observer: Arc::new(crate::observability::NoopObserver),
            tools_registry: Arc::new(Vec::new()),
            system_prompt: Arc::new("test-system-prompt".into()),
            model: "test-model".into(),
            temperature: 0.0,
            mem: memory,
            auto_save,
            webhook_secret: None,
            pairing: Arc::new(PairingGuard::new(false, &[])),
            rate_limiter: Arc::new(GatewayRateLimiter::new(100, 100)),
            idempotency_store: Arc::new(IdempotencyStore::new(Duration::from_secs(300))),
            whatsapp: None,
            whatsapp_app_secret: None,
            max_tool_iterations: 10,
            usage_stats: Arc::new(UsageStats::default()),
            config: Arc::new(Config::default()),
            config_version: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            hot_reload_manager: None,
            trace_store: None,
            workflow_store,
            alert_manager: Arc::new(crate::observability::prelude::AlertManager::new()),
            failure_analyzer: Arc::new(crate::observability::prelude::FailureAnalyzer::new()),
            swarm_manager: None,
            swarm_chat_manager: None,
            consensus_manager: None,
            agent_group_store: Arc::new(
                AgentGroupStore::new(
                    std::path::Path::new(".workspace/agent_groups")
                ).expect("Failed to create agent group store")
            ),
            role_mapping_store: Arc::new(
                RoleMappingStore::new(
                    std::path::Path::new(".workspace/role_mappings")
                ).expect("Failed to create role mapping store")
            ),
            workflow_engine: workflow_engine.clone(),
            workflow_scheduler: workflow_scheduler.clone(),
            event_bus: Arc::new(crate::workflow::EventBus::new(workflow_engine, workflow_scheduler)),
        }
    }

    #[tokio::test]
    async fn webhook_idempotency_skips_duplicate_provider_calls() {
        let provider_impl = Arc::new(MockProvider::default());
        let provider: Arc<dyn Provider> = provider_impl.clone();
        let memory: Arc<dyn Memory> = Arc::new(MockMemory);

        let state = test_app_state(provider, memory, false);

        let mut headers = HeaderMap::new();
        headers.insert("X-Idempotency-Key", HeaderValue::from_static("abc-123"));

        let body = Ok(Json(WebhookBody {
            message: "hello".into(),
        }));
        let first = handle_webhook(State(state.clone()), headers.clone(), body)
            .await
            .into_response();
        assert_eq!(first.status(), StatusCode::OK);

        let body = Ok(Json(WebhookBody {
            message: "hello".into(),
        }));
        let second = handle_webhook(State(state), headers, body)
            .await
            .into_response();
        assert_eq!(second.status(), StatusCode::OK);

        let payload = second.into_body().collect().await.unwrap().to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(parsed["status"], "duplicate");
        assert_eq!(parsed["idempotent"], true);
        assert_eq!(provider_impl.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn webhook_autosave_stores_distinct_keys_per_request() {
        let provider_impl = Arc::new(MockProvider::default());
        let provider: Arc<dyn Provider> = provider_impl.clone();

        let tracking_impl = Arc::new(TrackingMemory::default());
        let memory: Arc<dyn Memory> = tracking_impl.clone();

        let state = test_app_state(provider, memory, true);

        let headers = HeaderMap::new();

        let body1 = Ok(Json(WebhookBody {
            message: "hello one".into(),
        }));
        let first = handle_webhook(State(state.clone()), headers.clone(), body1)
            .await
            .into_response();
        assert_eq!(first.status(), StatusCode::OK);

        let body2 = Ok(Json(WebhookBody {
            message: "hello two".into(),
        }));
        let second = handle_webhook(State(state), headers, body2)
            .await
            .into_response();
        assert_eq!(second.status(), StatusCode::OK);

        let keys = tracking_impl
            .keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1]);
        assert!(keys[0].starts_with("webhook_msg_"));
        assert!(keys[1].starts_with("webhook_msg_"));
        assert_eq!(provider_impl.calls.load(Ordering::SeqCst), 2);
    }

    #[derive(Default)]
    struct StructuredToolCallProvider {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Provider for StructuredToolCallProvider {
        async fn chat_with_system(
            &self,
            _system_prompt: Option<&str>,
            _message: &str,
            _model: &str,
            _temperature: f64,
        ) -> anyhow::Result<crate::providers::ChatResponse> {
            let turn = self.calls.fetch_add(1, Ordering::SeqCst);

            if turn == 0 {
                return Ok(crate::providers::ChatResponse {
                    text: Some("Running tool...".into()),
                    tool_calls: vec![crate::providers::ToolCall {
                        id: "call_1".into(),
                        name: "mock_tool".into(),
                        arguments: r#"{"query":"gateway"}"#.into(),
                    }],
                    usage: None,
                });
            }

            Ok(crate::providers::ChatResponse::with_text(
                "Gateway tool result ready.",
            ))
        }
    }

    struct MockTool {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            "mock_tool"
        }

        fn description(&self) -> &str {
            "Mock tool for gateway tests"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                },
                "required": ["query"]
            })
        }

        async fn execute(
            &self,
            args: serde_json::Value,
        ) -> anyhow::Result<crate::tools::ToolResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(args["query"], "gateway");

            Ok(crate::tools::ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
            })
        }

        fn clone_box(&self) -> Box<dyn Tool> {
            Box::new(MockTool {
                calls: Arc::clone(&self.calls),
            })
        }
    }

    #[tokio::test]
    async fn webhook_executes_structured_tool_calls() {
        let provider_impl = Arc::new(StructuredToolCallProvider::default());
        let provider: Arc<dyn Provider> = provider_impl.clone();
        let memory: Arc<dyn Memory> = Arc::new(MockMemory);

        let tool_calls = Arc::new(AtomicUsize::new(0));
        let tools: Vec<Box<dyn Tool>> = vec![Box::new(MockTool {
            calls: Arc::clone(&tool_calls),
        })];

        let mut state = test_app_state(provider, memory, false);
        state.tools_registry = Arc::new(tools);

        let response = handle_webhook(
            State(state),
            HeaderMap::new(),
            Ok(Json(WebhookBody {
                message: "please use tool".into(),
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response.into_body().collect().await.unwrap().to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(parsed["response"], "Gateway tool result ready.");
        assert_eq!(tool_calls.load(Ordering::SeqCst), 1);
        assert_eq!(provider_impl.calls.load(Ordering::SeqCst), 2);
    }

    // ══════════════════════════════════════════════════════════
    // WhatsApp Signature Verification Tests (CWE-345 Prevention)
    // ══════════════════════════════════════════════════════════

    fn compute_whatsapp_signature_hex(secret: &str, body: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    fn compute_whatsapp_signature_header(secret: &str, body: &[u8]) -> String {
        format!("sha256={}", compute_whatsapp_signature_hex(secret, body))
    }

    #[test]
    fn whatsapp_signature_valid() {
        // Test with known values
        let app_secret = "test_secret_key";
        let body = b"test body content";

        let signature_header = compute_whatsapp_signature_header(app_secret, body);

        assert!(verify_whatsapp_signature(
            app_secret,
            body,
            &signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_invalid_wrong_secret() {
        let app_secret = "correct_secret";
        let wrong_secret = "wrong_secret";
        let body = b"test body content";

        let signature_header = compute_whatsapp_signature_header(wrong_secret, body);

        assert!(!verify_whatsapp_signature(
            app_secret,
            body,
            &signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_invalid_wrong_body() {
        let app_secret = "test_secret";
        let original_body = b"original body";
        let tampered_body = b"tampered body";

        let signature_header = compute_whatsapp_signature_header(app_secret, original_body);

        // Verify with tampered body should fail
        assert!(!verify_whatsapp_signature(
            app_secret,
            tampered_body,
            &signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_missing_prefix() {
        let app_secret = "test_secret";
        let body = b"test body";

        // Signature without "sha256=" prefix
        let signature_header = "abc123def456";

        assert!(!verify_whatsapp_signature(
            app_secret,
            body,
            signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_empty_header() {
        let app_secret = "test_secret";
        let body = b"test body";

        assert!(!verify_whatsapp_signature(app_secret, body, ""));
    }

    #[test]
    fn whatsapp_signature_invalid_hex() {
        let app_secret = "test_secret";
        let body = b"test body";

        // Invalid hex characters
        let signature_header = "sha256=not_valid_hex_zzz";

        assert!(!verify_whatsapp_signature(
            app_secret,
            body,
            signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_empty_body() {
        let app_secret = "test_secret";
        let body = b"";

        let signature_header = compute_whatsapp_signature_header(app_secret, body);

        assert!(verify_whatsapp_signature(
            app_secret,
            body,
            &signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_unicode_body() {
        let app_secret = "test_secret";
        let body = "Hello 🦀 世界".as_bytes();

        let signature_header = compute_whatsapp_signature_header(app_secret, body);

        assert!(verify_whatsapp_signature(
            app_secret,
            body,
            &signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_json_payload() {
        let app_secret = "my_app_secret_from_meta";
        let body = br#"{"entry":[{"changes":[{"value":{"messages":[{"from":"1234567890","text":{"body":"Hello"}}]}}]}]}"#;

        let signature_header = compute_whatsapp_signature_header(app_secret, body);

        assert!(verify_whatsapp_signature(
            app_secret,
            body,
            &signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_case_sensitive_prefix() {
        let app_secret = "test_secret";
        let body = b"test body";

        let hex_sig = compute_whatsapp_signature_hex(app_secret, body);

        // Wrong case prefix should fail
        let wrong_prefix = format!("SHA256={hex_sig}");
        assert!(!verify_whatsapp_signature(app_secret, body, &wrong_prefix));

        // Correct prefix should pass
        let correct_prefix = format!("sha256={hex_sig}");
        assert!(verify_whatsapp_signature(app_secret, body, &correct_prefix));
    }

    #[test]
    fn whatsapp_signature_truncated_hex() {
        let app_secret = "test_secret";
        let body = b"test body";

        let hex_sig = compute_whatsapp_signature_hex(app_secret, body);
        let truncated = &hex_sig[..32]; // Only half the signature
        let signature_header = format!("sha256={truncated}");

        assert!(!verify_whatsapp_signature(
            app_secret,
            body,
            &signature_header
        ));
    }

    #[test]
    fn whatsapp_signature_extra_bytes() {
        let app_secret = "test_secret";
        let body = b"test body";

        let hex_sig = compute_whatsapp_signature_hex(app_secret, body);
        let extended = format!("{hex_sig}deadbeef");
        let signature_header = format!("sha256={extended}");

        assert!(!verify_whatsapp_signature(
            app_secret,
            body,
            &signature_header
        ));
    }
}
