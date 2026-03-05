use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, Duration};

/// Token usage statistics
#[derive(Clone, Debug, Default)]
pub struct TokenStats {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub requests: u64,
    pub avg_tokens_per_request: f64,
    pub peak_tokens: u64,
    pub total_time: Duration,
}

/// Token counter for monitoring and limiting token usage
#[derive(Clone, Debug)]
pub struct TokenCounter {
    pub prompt_tokens: Arc<AtomicU64>,
    pub completion_tokens: Arc<AtomicU64>,
    pub total_tokens: Arc<AtomicU64>,
    pub max_tokens: Option<u64>,
    pub stats: Arc<std::sync::RwLock<TokenStats>>,
    pub last_reset: Arc<std::sync::RwLock<Instant>>,
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self {
            prompt_tokens: Arc::new(AtomicU64::new(0)),
            completion_tokens: Arc::new(AtomicU64::new(0)),
            total_tokens: Arc::new(AtomicU64::new(0)),
            max_tokens: None,
            stats: Arc::new(std::sync::RwLock::new(TokenStats::default())),
            last_reset: Arc::new(std::sync::RwLock::new(Instant::now())),
        }
    }
}

impl TokenCounter {
    /// Create a new token counter with optional max limit
    pub fn new(max_tokens: Option<u64>) -> Self {
        Self {
            max_tokens,
            ..Default::default()
        }
    }

    /// Estimate token count for text (simplified GPT-4 token counting)
    pub fn estimate_tokens(&self, text: &str) -> u64 {
        // Simplified token counting based on OpenAI's tokenizer behavior
        // This is an estimate - actual token count may vary
        
        // Count words (split by whitespace)
        let word_count = text.split_whitespace().count() as u64;
        
        // Count characters (for Asian languages and short words)
        let char_count = text.chars().count() as u64;
        
        // Average estimate: ~4 characters per token, ~1 word per token
        let token_count = (word_count + char_count / 4).max(1);
        
        token_count
    }

    /// Add prompt tokens and check limit
    pub fn add_prompt_tokens(&self, text: &str) -> Result<u64, String> {
        let tokens = self.estimate_tokens(text);
        self.add_prompt_tokens_exact(tokens)
    }

    /// Add exact prompt token count
    pub fn add_prompt_tokens_exact(&self, tokens: u64) -> Result<u64, String> {
        let current = self.prompt_tokens.fetch_add(tokens, Ordering::SeqCst);
        let _new = current + tokens;
        
        let total = self.total_tokens.fetch_add(tokens, Ordering::SeqCst) + tokens;
        
        // Update stats
        self.update_stats();
        
        self.check_limit(total)
    }

    /// Add completion tokens and check limit
    pub fn add_completion_tokens(&self, text: &str) -> Result<u64, String> {
        let tokens = self.estimate_tokens(text);
        self.add_completion_tokens_exact(tokens)
    }

    /// Add exact completion token count
    pub fn add_completion_tokens_exact(&self, tokens: u64) -> Result<u64, String> {
        let current = self.completion_tokens.fetch_add(tokens, Ordering::SeqCst);
        let _new = current + tokens;
        
        let total = self.total_tokens.fetch_add(tokens, Ordering::SeqCst) + tokens;
        
        // Update stats
        self.update_stats();
        
        self.check_limit(total)
    }

    /// Check if token limit is exceeded
    fn check_limit(&self, total: u64) -> Result<u64, String> {
        if let Some(max) = self.max_tokens {
            if total > max {
                return Err(format!("Token limit exceeded: {total} > {max}"));
            }
        }
        Ok(total)
    }

    /// Update token usage statistics
    fn update_stats(&self) {
        let (prompt, completion, total) = self.usage();
        let mut stats = self.stats.write().unwrap();
        let last_reset = self.last_reset.read().unwrap();
        
        stats.prompt_tokens = prompt;
        stats.completion_tokens = completion;
        stats.total_tokens = total;
        stats.requests += 1;
        stats.avg_tokens_per_request = total as f64 / stats.requests as f64;
        stats.peak_tokens = stats.peak_tokens.max(total);
        stats.total_time = last_reset.elapsed();
    }

    /// Reset token counts
    pub fn reset(&self) {
        self.prompt_tokens.store(0, Ordering::SeqCst);
        self.completion_tokens.store(0, Ordering::SeqCst);
        self.total_tokens.store(0, Ordering::SeqCst);
        *self.last_reset.write().unwrap() = Instant::now();
        
        // Reset stats but keep historical data
        let mut stats = self.stats.write().unwrap();
        stats.requests = 0;
        stats.avg_tokens_per_request = 0.0;
    }

    /// Get current token usage
    pub fn usage(&self) -> (u64, u64, u64) {
        (
            self.prompt_tokens.load(Ordering::SeqCst),
            self.completion_tokens.load(Ordering::SeqCst),
            self.total_tokens.load(Ordering::SeqCst),
        )
    }

    /// Get usage summary
    pub fn summary(&self) -> String {
        let (prompt, completion, total) = self.usage();
        let stats = self.stats.read().unwrap();
        format!("Tokens: prompt={prompt}, completion={completion}, total={total}{} | Requests: {} | Avg: {:.1}/req | Peak: {} | Time: {:?}", 
            self.max_tokens.map(|max| format!("/{max}")).unwrap_or_default(),
            stats.requests,
            stats.avg_tokens_per_request,
            stats.peak_tokens,
            stats.total_time
        )
    }

    /// Get detailed statistics
    pub fn detailed_stats(&self) -> String {
        let stats = self.stats.read().unwrap();
        let tokens_per_second = stats.total_tokens as f64 / stats.total_time.as_secs_f64();
        
        format!(
            "Token Statistics:\n  Prompt tokens: {}\n  Completion tokens: {}\n  Total tokens: {}\n  Requests: {}\n  Avg tokens/request: {:.2}\n  Peak tokens: {}\n  Total time: {:?}\n  Tokens/second: {:.2}",
            stats.prompt_tokens,
            stats.completion_tokens,
            stats.total_tokens,
            stats.requests,
            stats.avg_tokens_per_request,
            stats.peak_tokens,
            stats.total_time,
            tokens_per_second
        )
    }

    /// Get optimization suggestions
    pub fn optimization_suggestions(&self) -> Vec<String> {
        let stats = self.stats.read().unwrap();
        let mut suggestions = Vec::new();
        
        if stats.avg_tokens_per_request > 1000.0 {
            suggestions.push("High average tokens per request. Consider further context compression.".to_string());
        }
        
        if stats.prompt_tokens > stats.completion_tokens * 2 {
            suggestions.push("Prompt tokens exceed completion tokens significantly. Optimize system prompt.".to_string());
        }
        
        if stats.requests > 10 && stats.avg_tokens_per_request > 500.0 {
            suggestions.push("Consider implementing caching for repeated queries.".to_string());
        }
        
        suggestions
    }
}