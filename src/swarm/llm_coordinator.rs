use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, RwLock, Semaphore};

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct LLMConcurrencyCoordinator {
    request_queue: Arc<RwLock<VecDeque<LLMRequest>>>,
    active_requests: Arc<AtomicU32>,
    max_concurrent: u32,
    rate_limiter: Arc<RateLimiter>,
    request_history: Arc<RwLock<Vec<RequestRecord>>>,
    semaphore: Arc<Semaphore>,
    config: CoordinatorConfig,
}

#[derive(Clone)]
pub struct CoordinatorConfig {
    pub api_base: String,
    api_key: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
}

impl CoordinatorConfig {
    pub fn new(api_base: String, api_key: String, model: String) -> Self {
        Self {
            api_base,
            api_key,
            model,
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
    
    pub fn api_key(&self) -> &str {
        &self.api_key
    }
}

impl std::fmt::Debug for CoordinatorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoordinatorConfig")
            .field("api_base", &self.api_base)
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            api_base: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            api_key: String::new(),
            model: "glm-5".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
}

#[derive(Debug)]
pub struct LLMRequest {
    pub id: String,
    pub requester: String,
    pub prompt: String,
    pub priority: RequestPriority,
    pub created_at: u64,
    pub timeout_secs: u64,
    pub response_tx: Option<oneshot::Sender<Result<LLMResponse>>>,
    pub request_type: LLMRequestType,
    pub retry_count: u32,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RequestPriority {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

impl Default for RequestPriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LLMRequestType {
    TaskDecomposition,
    TaskExecution,
    DecisionMaking,
    KnowledgeQuery,
    Communication,
    CodeGeneration,
    DocumentGeneration,
}

impl Default for LLMRequestType {
    fn default() -> Self {
        Self::TaskExecution
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    pub request_id: String,
    pub content: String,
    pub tokens_used: TokenUsage,
    pub response_time_ms: u64,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestRecord {
    pub request_id: String,
    pub requester: String,
    pub request_type: LLMRequestType,
    pub priority: RequestPriority,
    pub created_at: u64,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub success: bool,
    pub error: Option<String>,
    pub tokens_used: Option<TokenUsage>,
}

pub struct RateLimiter {
    max_requests_per_minute: u32,
    max_tokens_per_minute: u32,
    current_requests: AtomicU32,
    current_tokens: AtomicU32,
    window_start: AtomicU64,
    lock: std::sync::Mutex<()>,
}

impl RateLimiter {
    pub fn new(max_requests_per_minute: u32, max_tokens_per_minute: u32) -> Self {
        Self {
            max_requests_per_minute,
            max_tokens_per_minute,
            current_requests: AtomicU32::new(0),
            current_tokens: AtomicU32::new(0),
            window_start: AtomicU64::new(now_unix()),
            lock: std::sync::Mutex::new(()),
        }
    }
    
    pub fn try_acquire(&self, estimated_tokens: u32) -> bool {
        let _guard = self.lock.lock().unwrap();
        
        let now = now_unix();
        let window_start = self.window_start.load(Ordering::Acquire);
        
        if now - window_start >= 60 {
            self.window_start.store(now, Ordering::Release);
            self.current_requests.store(0, Ordering::Release);
            self.current_tokens.store(0, Ordering::Release);
        }
        
        let current_requests = self.current_requests.load(Ordering::Acquire);
        let current_tokens = self.current_tokens.load(Ordering::Acquire);
        
        if current_requests < self.max_requests_per_minute
            && current_tokens + estimated_tokens <= self.max_tokens_per_minute
        {
            self.current_requests.fetch_add(1, Ordering::AcqRel);
            self.current_tokens.fetch_add(estimated_tokens, Ordering::AcqRel);
            true
        } else {
            false
        }
    }
    
    pub fn can_request(&self, estimated_tokens: u32) -> bool {
        let _guard = self.lock.lock().unwrap();
        
        let now = now_unix();
        let window_start = self.window_start.load(Ordering::Acquire);
        
        if now - window_start >= 60 {
            self.window_start.store(now, Ordering::Release);
            self.current_requests.store(0, Ordering::Release);
            self.current_tokens.store(0, Ordering::Release);
            return true;
        }
        
        let current_requests = self.current_requests.load(Ordering::Acquire);
        let current_tokens = self.current_tokens.load(Ordering::Acquire);
        
        current_requests < self.max_requests_per_minute
            && current_tokens + estimated_tokens <= self.max_tokens_per_minute
    }
    
    pub fn record_request(&self, tokens: u32) {
        self.current_requests.fetch_add(1, Ordering::AcqRel);
        self.current_tokens.fetch_add(tokens, Ordering::AcqRel);
    }
    
    pub fn get_wait_time(&self) -> u64 {
        let now = now_unix();
        let window_start = self.window_start.load(Ordering::Acquire);
        60 - (now - window_start)
    }
    
    pub fn get_current_usage(&self) -> (u32, u32) {
        (
            self.current_requests.load(Ordering::Acquire),
            self.current_tokens.load(Ordering::Acquire),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub pending_requests: u32,
    pub active_requests: u32,
    pub max_concurrent: u32,
    pub available_slots: u32,
    pub high_priority_pending: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorStatistics {
    pub total_requests: u32,
    pub successful_requests: u32,
    pub failed_requests: u32,
    pub avg_response_time_ms: u64,
    pub total_tokens_used: u32,
    pub success_rate: f64,
}

impl LLMConcurrencyCoordinator {
    pub fn new(
        config: CoordinatorConfig,
        max_concurrent: u32,
        max_requests_per_minute: u32,
        max_tokens_per_minute: u32,
    ) -> Self {
        Self {
            request_queue: Arc::new(RwLock::new(VecDeque::new())),
            active_requests: Arc::new(AtomicU32::new(0)),
            max_concurrent,
            rate_limiter: Arc::new(RateLimiter::new(max_requests_per_minute, max_tokens_per_minute)),
            request_history: Arc::new(RwLock::new(Vec::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrent as usize)),
            config,
        }
    }
    
    pub async fn submit_request(
        &self,
        requester: &str,
        prompt: String,
        priority: RequestPriority,
        request_type: LLMRequestType,
        timeout_secs: u64,
    ) -> Result<LLMResponse> {
        let (tx, rx) = oneshot::channel();
        
        let request = LLMRequest {
            id: uuid::Uuid::new_v4().to_string(),
            requester: requester.to_string(),
            prompt,
            priority,
            created_at: now_unix(),
            timeout_secs,
            response_tx: Some(tx),
            request_type,
            retry_count: 0,
            max_retries: 3,
        };
        
        {
            let mut queue = self.request_queue.write().await;
            let insert_pos = queue
                .iter()
                .position(|r| r.priority < priority)
                .unwrap_or(queue.len());
            queue.insert(insert_pos, request);
        }
        
        rx.await.map_err(|_| anyhow::anyhow!("Response channel closed"))?
    }
    
    pub async fn process_next_request(&self) -> Option<LLMResponse> {
        let permit = self.semaphore.clone().try_acquire_owned().ok()?;
        
        let request = {
            let mut queue = self.request_queue.write().await;
            queue.pop_front()
        }?;
        
        let estimated_tokens = (request.prompt.len() / 4) as u32;
        
        if !self.rate_limiter.try_acquire(estimated_tokens) {
            let mut queue = self.request_queue.write().await;
            queue.push_front(request);
            drop(permit);
            return None;
        }
        
        self.active_requests.fetch_add(1, Ordering::AcqRel);
        
        let record = RequestRecord {
            request_id: request.id.clone(),
            requester: request.requester.clone(),
            request_type: request.request_type.clone(),
            priority: request.priority,
            created_at: request.created_at,
            started_at: now_unix(),
            completed_at: None,
            success: false,
            error: None,
            tokens_used: None,
        };
        
        let response = self.execute_request(&request).await;
        
        if let Some(tx) = request.response_tx {
            match &response {
                Ok(resp) => { let _ = tx.send(Ok(resp.clone())); }
                Err(e) => { let _ = tx.send(Err(anyhow::Error::msg(e.to_string()))); }
            }
        }
        
        {
            let mut history = self.request_history.write().await;
            history.push(RequestRecord {
                completed_at: Some(now_unix()),
                success: response.is_ok(),
                error: response.as_ref().err().map(|e| e.to_string()),
                tokens_used: response.as_ref().ok().map(|r| r.tokens_used.clone()),
                ..record
            });
            
            let len = history.len();
            if len > 1000 {
                history.drain(0..len - 1000);
            }
        }
        
        self.active_requests.fetch_sub(1, Ordering::AcqRel);
        drop(permit);
        
        response.ok()
    }
    
    async fn execute_request(&self, request: &LLMRequest) -> Result<LLMResponse> {
        let start_time = std::time::Instant::now();
        
        let client = reqwest::Client::new();
        let url = format!("{}/chat/completions", self.config.api_base);
        
        let body = serde_json::json!({
            "model": self.config.model,
            "messages": [
                {
                    "role": "user",
                    "content": request.prompt
                }
            ],
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
        });
        
        let response = tokio::time::timeout(
            Duration::from_secs(request.timeout_secs),
            client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key()))
                .header("Content-Type", "application/json")
                .json(&body)
                .send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Request timeout"))?
        .map_err(|e| anyhow::anyhow!("HTTP error: {}", e))?;
        
        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("JSON parse error: {}", e))?;
        
        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid response format"))?
            .to_string();
        
        let tokens_used = TokenUsage {
            prompt_tokens: response_json["usage"]["prompt_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
            completion_tokens: response_json["usage"]["completion_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
            total_tokens: response_json["usage"]["total_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
        };
        
        Ok(LLMResponse {
            request_id: request.id.clone(),
            content,
            tokens_used,
            response_time_ms: start_time.elapsed().as_millis() as u64,
            model: self.config.model.clone(),
        })
    }
    
    pub async fn get_queue_status(&self) -> QueueStatus {
        let queue = self.request_queue.read().await;
        let active = self.active_requests.load(Ordering::Acquire);
        
        QueueStatus {
            pending_requests: queue.len() as u32,
            active_requests: active,
            max_concurrent: self.max_concurrent,
            available_slots: self.max_concurrent.saturating_sub(active),
            high_priority_pending: queue
                .iter()
                .filter(|r| r.priority >= RequestPriority::High)
                .count() as u32,
        }
    }
    
    pub async fn get_statistics(&self) -> CoordinatorStatistics {
        let history = self.request_history.read().await;
        
        let total_requests = history.len() as u32;
        let successful_requests = history.iter().filter(|r| r.success).count() as u32;
        let failed_requests = total_requests - successful_requests;
        
        let avg_response_time = if !history.is_empty() {
            history
                .iter()
                .filter_map(|r| {
                    r.completed_at.map(|c| {
                        let started = r.started_at;
                        if c >= started {
                            (c - started) * 1000
                        } else {
                            0
                        }
                    })
                })
                .sum::<u64>()
                / history.len() as u64
        } else {
            0
        };
        
        let total_tokens = history
            .iter()
            .filter_map(|r| r.tokens_used.as_ref())
            .map(|t| t.total_tokens)
            .sum();
        
        CoordinatorStatistics {
            total_requests,
            successful_requests,
            failed_requests,
            avg_response_time_ms: avg_response_time,
            total_tokens_used: total_tokens,
            success_rate: if total_requests > 0 {
                successful_requests as f64 / total_requests as f64
            } else {
                0.0
            },
        }
    }
    
    pub async fn cancel_request(&self, request_id: &str) -> bool {
        let mut queue = self.request_queue.write().await;
        if let Some(pos) = queue.iter().position(|r| r.id == request_id) {
            queue.remove(pos);
            return true;
        }
        false
    }
    
    pub async fn clear_queue(&self) -> u32 {
        let mut queue = self.request_queue.write().await;
        let count = queue.len() as u32;
        queue.clear();
        count
    }
    
    pub fn get_rate_limiter_status(&self) -> (u32, u32, u32, u32) {
        let (requests, tokens) = self.rate_limiter.get_current_usage();
        (
            requests,
            self.rate_limiter.max_requests_per_minute,
            tokens,
            self.rate_limiter.max_tokens_per_minute,
        )
    }
}

pub struct AgentLLMClient {
    agent_name: String,
    coordinator: Arc<LLMConcurrencyCoordinator>,
    default_priority: RequestPriority,
    default_timeout: u64,
}

impl AgentLLMClient {
    pub fn new(agent_name: String, coordinator: Arc<LLMConcurrencyCoordinator>) -> Self {
        Self {
            agent_name,
            coordinator,
            default_priority: RequestPriority::Normal,
            default_timeout: 120,
        }
    }
    
    pub fn with_priority(mut self, priority: RequestPriority) -> Self {
        self.default_priority = priority;
        self
    }
    
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.default_timeout = timeout_secs;
        self
    }
    
    pub async fn complete(&self, prompt: &str) -> Result<String> {
        self.complete_with_options(
            prompt,
            self.default_priority,
            LLMRequestType::TaskExecution,
            self.default_timeout,
        ).await
    }
    
    pub async fn complete_urgent(&self, prompt: &str) -> Result<String> {
        self.complete_with_options(
            prompt,
            RequestPriority::Critical,
            LLMRequestType::DecisionMaking,
            60,
        ).await
    }
    
    pub async fn complete_with_options(
        &self,
        prompt: &str,
        priority: RequestPriority,
        request_type: LLMRequestType,
        timeout_secs: u64,
    ) -> Result<String> {
        let response = self
            .coordinator
            .submit_request(
                &self.agent_name,
                prompt.to_string(),
                priority,
                request_type,
                timeout_secs,
            )
            .await?;
        
        Ok(response.content)
    }
    
    pub async fn decompose_task(&self, prompt: &str) -> Result<String> {
        self.complete_with_options(
            prompt,
            RequestPriority::Normal,
            LLMRequestType::TaskDecomposition,
            180,
        ).await
    }
    
    pub async fn query_knowledge(&self, prompt: &str) -> Result<String> {
        self.complete_with_options(
            prompt,
            RequestPriority::Low,
            LLMRequestType::KnowledgeQuery,
            60,
        ).await
    }
    
    pub async fn generate_code(&self, prompt: &str) -> Result<String> {
        self.complete_with_options(
            prompt,
            RequestPriority::Normal,
            LLMRequestType::CodeGeneration,
            180,
        ).await
    }
    
    pub async fn generate_document(&self, prompt: &str) -> Result<String> {
        self.complete_with_options(
            prompt,
            RequestPriority::Low,
            LLMRequestType::DocumentGeneration,
            120,
        ).await
    }
    
    pub async fn get_queue_status(&self) -> QueueStatus {
        self.coordinator.get_queue_status().await
    }
}

type Result<T> = std::result::Result<T, anyhow::Error>;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(10, 1000);
        
        assert!(limiter.can_request(100));
        
        limiter.record_request(500);
        let (requests, tokens) = limiter.get_current_usage();
        assert_eq!(requests, 1);
        assert_eq!(tokens, 500);
        
        assert!(limiter.can_request(400));
        assert!(!limiter.can_request(600));
    }
    
    #[test]
    fn test_request_priority_ordering() {
        assert!(RequestPriority::Critical > RequestPriority::High);
        assert!(RequestPriority::High > RequestPriority::Normal);
        assert!(RequestPriority::Normal > RequestPriority::Low);
    }
    
    #[tokio::test]
    async fn test_coordinator_creation() {
        let config = CoordinatorConfig::default();
        let coordinator = LLMConcurrencyCoordinator::new(config, 5, 60, 100000);
        
        let status = coordinator.get_queue_status().await;
        assert_eq!(status.pending_requests, 0);
        assert_eq!(status.active_requests, 0);
        assert_eq!(status.max_concurrent, 5);
    }
    
    #[tokio::test]
    async fn test_queue_status() {
        let config = CoordinatorConfig::default();
        let coordinator = LLMConcurrencyCoordinator::new(config, 3, 60, 100000);
        
        let status = coordinator.get_queue_status().await;
        assert_eq!(status.available_slots, 3);
    }
    
    #[tokio::test]
    async fn test_agent_client_creation() {
        let config = CoordinatorConfig::default();
        let coordinator = Arc::new(LLMConcurrencyCoordinator::new(config, 5, 60, 100000));
        
        let client = AgentLLMClient::new("test_agent".to_string(), coordinator.clone());
        
        let status = client.get_queue_status().await;
        assert_eq!(status.max_concurrent, 5);
    }
    
    #[tokio::test]
    async fn test_statistics() {
        let config = CoordinatorConfig::default();
        let coordinator = LLMConcurrencyCoordinator::new(config, 5, 60, 100000);
        
        let stats = coordinator.get_statistics().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.success_rate, 0.0);
    }
    
    #[tokio::test]
    async fn test_clear_queue() {
        let config = CoordinatorConfig::default();
        let coordinator = LLMConcurrencyCoordinator::new(config, 5, 60, 100000);
        
        let status = coordinator.get_queue_status().await;
        assert_eq!(status.pending_requests, 0);
        
        let cleared = coordinator.clear_queue().await;
        assert_eq!(cleared, 0);
        
        let status = coordinator.get_queue_status().await;
        assert_eq!(status.pending_requests, 0);
    }
}
