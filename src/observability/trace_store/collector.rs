//! TraceCollector - 低侵入性轨迹收集器
//!
//! 设计原则：
//! 1. 最小侵入性 - 通过 Observer trait 集成，不修改核心业务逻辑
//! 2. 异步非阻塞 - 收集操作不阻塞主流程
//! 3. 批量写入 - 高性能缓冲和批量存储
//! 4. 上下文传播 - 支持跨调用链的上下文追踪

use crate::observability::trace_store::types::*;
use crate::observability::trace_store::store::TraceStore;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// TraceCollector 配置
#[derive(Debug, Clone)]
pub struct TraceCollectorConfig {
    /// 是否启用收集
    pub enabled: bool,
    /// 缓冲区大小
    pub buffer_size: usize,
    /// 批量刷新间隔（毫秒）
    pub flush_interval_ms: u64,
    /// 是否收集推理链
    pub collect_reasoning: bool,
    /// 是否收集决策点
    pub collect_decisions: bool,
    /// 采样率 (0.0 - 1.0)
    pub sampling_rate: f64,
}

impl Default for TraceCollectorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size: 1000,
            flush_interval_ms: 1000,
            collect_reasoning: true,
            collect_decisions: true,
            sampling_rate: 1.0,
        }
    }
}

/// 轨迹收集器
/// 
/// 提供低侵入性的轨迹收集能力，通过异步通道和批量写入
/// 实现高性能、非阻塞的轨迹记录。
pub struct TraceCollector {
    /// 存储后端
    store: Arc<dyn TraceStore>,
    /// 配置
    config: TraceCollectorConfig,
    /// 发送通道
    sender: mpsc::Sender<CollectorMessage>,
    /// 当前活跃的上下文
    active_contexts: Arc<RwLock<HashMap<String, TraceContext>>>,
    /// 是否正在运行
    running: Arc<std::sync::RwLock<bool>>,
}

/// 收集器消息
enum CollectorMessage {
    /// 存储轨迹
    StoreTrace(AgentTrace),
    /// 存储推理链
    StoreReasoning(String, ReasoningChain),
    /// 存储决策点
    StoreDecision(String, DecisionPoint),
    /// 刷新缓冲区
    Flush,
    /// 关闭收集器
    Shutdown,
}

/// 轨迹上下文
/// 用于追踪跨调用的关联关系
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// 运行 ID
    pub run_id: String,
    /// 父轨迹 ID
    pub parent_trace_id: Option<String>,
    /// 开始时间
    pub start_time: Instant,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
    /// 嵌套深度
    pub depth: u32,
}

impl TraceContext {
    /// 创建新的上下文
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            parent_trace_id: None,
            start_time: Instant::now(),
            metadata: HashMap::new(),
            depth: 0,
        }
    }
    
    /// 创建子上下文
    pub fn child(&self, parent_trace_id: impl Into<String>) -> Self {
        Self {
            run_id: self.run_id.clone(),
            parent_trace_id: Some(parent_trace_id.into()),
            start_time: Instant::now(),
            metadata: self.metadata.clone(),
            depth: self.depth + 1,
        }
    }
    
    /// 添加元数据
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

impl TraceCollector {
    /// 创建新的轨迹收集器
    pub fn new(store: Arc<dyn TraceStore>, config: TraceCollectorConfig) -> Self {
        let (sender, mut receiver) = mpsc::channel(config.buffer_size);
        let active_contexts = Arc::new(RwLock::new(HashMap::new()));
        let running = Arc::new(std::sync::RwLock::new(true));
        
        // 启动后台处理任务
        let store_clone = store.clone();
        let running_clone = running.clone();
        let flush_interval = config.flush_interval_ms;
        
        tokio::spawn(async move {
            let mut buffer: Vec<AgentTrace> = Vec::new();
            let mut flush_timer = tokio::time::interval(
                std::time::Duration::from_millis(flush_interval)
            );
            
            loop {
                tokio::select! {
                    // 接收消息
                    msg = receiver.recv() => {
                        match msg {
                            Some(CollectorMessage::StoreTrace(trace)) => {
                                buffer.push(trace);
                                // 达到批量阈值时刷新
                                if buffer.len() >= 100 {
                                    if let Err(e) = store_clone.store_traces_batch(&buffer).await {
                                        tracing::error!("Failed to flush traces: {}", e);
                                    }
                                    buffer.clear();
                                }
                            }
                            Some(CollectorMessage::StoreReasoning(trace_id, reasoning)) => {
                                if let Err(e) = store_clone.store_reasoning(&trace_id, &reasoning).await {
                                    tracing::error!("Failed to store reasoning: {}", e);
                                }
                            }
                            Some(CollectorMessage::StoreDecision(trace_id, decision)) => {
                                if let Err(e) = store_clone.store_decision(&trace_id, &decision).await {
                                    tracing::error!("Failed to store decision: {}", e);
                                }
                            }
                            Some(CollectorMessage::Flush) | None => {
                                // 刷新缓冲区
                                if !buffer.is_empty() {
                                    if let Err(e) = store_clone.store_traces_batch(&buffer).await {
                                        tracing::error!("Failed to flush traces: {}", e);
                                    }
                                    buffer.clear();
                                }
                                if msg.is_none() {
                                    // 通道关闭，退出
                                    break;
                                }
                            }
                            Some(CollectorMessage::Shutdown) => {
                                // 关闭前刷新
                                if !buffer.is_empty() {
                                    if let Err(e) = store_clone.store_traces_batch(&buffer).await {
                                        tracing::error!("Failed to flush traces on shutdown: {}", e);
                                    }
                                }
                                *running_clone.write().unwrap() = false;
                                break;
                            }
                        }
                    }
                    
                    // 定时刷新
                    _ = flush_timer.tick() => {
                        if !buffer.is_empty() {
                            if let Err(e) = store_clone.store_traces_batch(&buffer).await {
                                tracing::error!("Failed to flush traces: {}", e);
                            }
                            buffer.clear();
                        }
                    }
                }
            }
        });
        
        Self {
            store,
            config,
            sender,
            active_contexts,
            running,
        }
    }
    
    /// 检查是否应该采样
    fn should_sample(&self) -> bool {
        if self.config.sampling_rate >= 1.0 {
            return true;
        }
        if self.config.sampling_rate <= 0.0 {
            return false;
        }
        // 简单的随机采样
        rand::random::<f64>() < self.config.sampling_rate
    }
    
    /// 创建新的运行上下文
    pub async fn create_context(&self, run_id: impl Into<String>) -> TraceContext {
        let context = TraceContext::new(run_id);
        self.active_contexts.write().await.insert(
            context.run_id.clone(),
            context.clone(),
        );
        context
    }
    
    /// 获取现有上下文
    pub async fn get_context(&self, run_id: &str) -> Option<TraceContext> {
        self.active_contexts.read().await.get(run_id).cloned()
    }
    
    /// 移除上下文
    pub async fn remove_context(&self, run_id: &str) {
        self.active_contexts.write().await.remove(run_id);
    }
    
    /// 记录用户消息
    pub async fn record_user_message(
        &self,
        context: &TraceContext,
        content: impl Into<String>,
    ) -> Result<String> {
        if !self.config.enabled || !self.should_sample() {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: 0,
            trace_type: TraceType::UserMessage,
            input: TraceInput {
                content: content.into(),
                content_type: InputContentType::Text,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: String::new(),
                success: true,
                error: None,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: serde_json::to_value(&context.metadata).unwrap_or(serde_json::json!({})),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录 LLM 调用
    /// 
    /// 这是最核心的收集方法，用于记录所有 LLM 调用场景
    pub async fn record_llm_call(
        &self,
        context: &TraceContext,
        provider: impl Into<String>,
        model: impl Into<String>,
        input: impl Into<String>,
        output: impl Into<String>,
        success: bool,
        error: Option<String>,
        tokens: Option<TokenUsage>,
        cost: Option<f64>,
        duration_ms: u64,
    ) -> Result<String> {
        if !self.config.enabled || !self.should_sample() {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms,
            trace_type: TraceType::LlmCall {
                provider: provider.into(),
                model: model.into(),
            },
            input: TraceInput {
                content: input.into(),
                content_type: InputContentType::Text,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: output.into(),
                success,
                error,
                tokens_used: tokens,
                cost_usd: cost,
            },
            metadata: serde_json::to_value(&context.metadata).unwrap_or(serde_json::json!({})),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录工具调用
    pub async fn record_tool_call(
        &self,
        context: &TraceContext,
        tool: impl Into<String>,
        action: impl Into<String>,
        input: serde_json::Value,
        output: serde_json::Value,
        success: bool,
        error: Option<String>,
        duration_ms: u64,
    ) -> Result<String> {
        if !self.config.enabled || !self.should_sample() {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms,
            trace_type: TraceType::ToolCall {
                tool: tool.into(),
                action: action.into(),
            },
            input: TraceInput {
                content: serde_json::to_string(&input).unwrap_or_default(),
                content_type: InputContentType::Json,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: serde_json::to_string(&output).unwrap_or_default(),
                success,
                error,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: serde_json::to_value(&context.metadata).unwrap_or(serde_json::json!({})),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录子智能体调用
    pub async fn record_sub_agent_call(
        &self,
        context: &TraceContext,
        agent_name: impl Into<String>,
        task: impl Into<String>,
        input: serde_json::Value,
        output: serde_json::Value,
        success: bool,
        error: Option<String>,
        duration_ms: u64,
    ) -> Result<String> {
        if !self.config.enabled || !self.should_sample() {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms,
            trace_type: TraceType::SubAgentCall {
                agent_name: agent_name.into(),
                task: task.into(),
            },
            input: TraceInput {
                content: serde_json::to_string(&input).unwrap_or_default(),
                content_type: InputContentType::Json,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: serde_json::to_string(&output).unwrap_or_default(),
                success,
                error,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: serde_json::to_value(&context.metadata).unwrap_or(serde_json::json!({})),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录阶段转换
    pub async fn record_phase_transition(
        &self,
        context: &TraceContext,
        from: impl Into<String>,
        to: impl Into<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<String> {
        if !self.config.enabled || !self.should_sample() {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: 0,
            trace_type: TraceType::PhaseTransition {
                from: from.into(),
                to: to.into(),
            },
            input: TraceInput {
                content: String::new(),
                content_type: InputContentType::Text,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: String::new(),
                success: true,
                error: None,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: metadata.unwrap_or(serde_json::json!({})),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录审批请求
    pub async fn record_approval_request(
        &self,
        context: &TraceContext,
        approval_type: impl Into<String>,
        description: impl Into<String>,
        approved: bool,
    ) -> Result<String> {
        if !self.config.enabled || !self.should_sample() {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: 0,
            trace_type: TraceType::ApprovalRequest {
                approval_type: approval_type.into(),
            },
            input: TraceInput {
                content: description.into(),
                content_type: InputContentType::Text,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: if approved { "approved" } else { "rejected" }.to_string(),
                success: approved,
                error: None,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: serde_json::json!({}),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录错误
    pub async fn record_error(
        &self,
        context: &TraceContext,
        component: impl Into<String>,
        error_type: impl Into<String>,
        error_message: impl Into<String>,
        stack_trace: Option<String>,
    ) -> Result<String> {
        if !self.config.enabled {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: 0,
            trace_type: TraceType::Error {
                component: component.into(),
                error_type: error_type.into(),
            },
            input: TraceInput {
                content: error_message.into(),
                content_type: InputContentType::Text,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: stack_trace.unwrap_or_default(),
                success: false,
                error: None,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: serde_json::json!({}),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录系统事件
    pub async fn record_system_event(
        &self,
        context: &TraceContext,
        event: impl Into<String>,
        details: serde_json::Value,
    ) -> Result<String> {
        if !self.config.enabled || !self.should_sample() {
            return Ok(Uuid::new_v4().to_string());
        }
        
        let trace_id = Uuid::new_v4().to_string();
        let trace = AgentTrace {
            id: trace_id.clone(),
            run_id: context.run_id.clone(),
            parent_id: context.parent_trace_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: 0,
            trace_type: TraceType::SystemEvent {
                event: event.into(),
            },
            input: TraceInput {
                content: serde_json::to_string(&details).unwrap_or_default(),
                content_type: InputContentType::Json,
                params: HashMap::new(),
            },
            output: TraceOutput {
                content: String::new(),
                success: true,
                error: None,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: serde_json::json!({}),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        self.sender.send(CollectorMessage::StoreTrace(trace)).await?;
        Ok(trace_id)
    }
    
    /// 记录推理链
    pub async fn record_reasoning(
        &self,
        trace_id: &str,
        reasoning: ReasoningChain,
    ) -> Result<()> {
        if !self.config.enabled || !self.config.collect_reasoning {
            return Ok(());
        }
        
        self.sender.send(CollectorMessage::StoreReasoning(trace_id.to_string(), reasoning)).await?;
        Ok(())
    }
    
    /// 记录决策点
    pub async fn record_decision(
        &self,
        trace_id: &str,
        decision: DecisionPoint,
    ) -> Result<()> {
        if !self.config.enabled || !self.config.collect_decisions {
            return Ok(());
        }
        
        self.sender.send(CollectorMessage::StoreDecision(trace_id.to_string(), decision)).await?;
        Ok(())
    }
    
    /// 刷新缓冲区
    pub async fn flush(&self) -> Result<()> {
        self.sender.send(CollectorMessage::Flush).await?;
        Ok(())
    }
    
    /// 关闭收集器
    pub async fn shutdown(&self) -> Result<()> {
        self.sender.send(CollectorMessage::Shutdown).await?;
        Ok(())
    }
    
    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        *self.running.read().unwrap()
    }
    
    /// 获取存储后端
    pub fn store(&self) -> Arc<dyn TraceStore> {
        self.store.clone()
    }
}

/// LLM 调用追踪器
/// 
/// 用于追踪单个 LLM 调用的完整生命周期
pub struct LlmCallTracker {
    collector: Arc<TraceCollector>,
    context: TraceContext,
    provider: String,
    model: String,
    input: String,
    start_time: Instant,
    trace_id: Option<String>,
}

impl LlmCallTracker {
    /// 创建新的 LLM 调用追踪器
    pub fn new(
        collector: Arc<TraceCollector>,
        context: TraceContext,
        provider: impl Into<String>,
        model: impl Into<String>,
        input: impl Into<String>,
    ) -> Self {
        Self {
            collector,
            context,
            provider: provider.into(),
            model: model.into(),
            input: input.into(),
            start_time: Instant::now(),
            trace_id: None,
        }
    }
    
    /// 完成追踪（成功）
    pub async fn complete(self, output: impl Into<String>, tokens: Option<TokenUsage>, cost: Option<f64>) -> Result<String> {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        
        self.collector.record_llm_call(
            &self.context,
            self.provider,
            self.model,
            self.input,
            output,
            true,
            None,
            tokens,
            cost,
            duration_ms,
        ).await
    }
    
    /// 完成追踪（失败）
    pub async fn fail(self, error: impl Into<String>) -> Result<String> {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        
        self.collector.record_llm_call(
            &self.context,
            self.provider,
            self.model,
            self.input,
            "",
            false,
            Some(error.into()),
            None,
            None,
            duration_ms,
        ).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::trace_store::store::TraceStore;
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::collections::HashMap;
    
    /// 测试用的内存存储（本地定义，避免跨模块 cfg(test) 问题）
    struct TestTraceStore {
        traces: std::sync::RwLock<HashMap<String, AgentTrace>>,
        reasonings: std::sync::RwLock<HashMap<String, ReasoningChain>>,
    }
    
    impl TestTraceStore {
        fn new() -> Self {
            Self {
                traces: std::sync::RwLock::new(HashMap::new()),
                reasonings: std::sync::RwLock::new(HashMap::new()),
            }
        }
    }
    
    #[async_trait]
    impl TraceStore for TestTraceStore {
        async fn store_trace(&self, trace: &AgentTrace) -> anyhow::Result<()> {
            let mut traces = self.traces.write().unwrap();
            traces.insert(trace.id.clone(), trace.clone());
            Ok(())
        }
        
        async fn store_traces_batch(&self, traces: &[AgentTrace]) -> anyhow::Result<()> {
            let mut store = self.traces.write().unwrap();
            for trace in traces {
                store.insert(trace.id.clone(), trace.clone());
            }
            Ok(())
        }
        
        async fn get_trace(&self, id: &str) -> anyhow::Result<Option<AgentTrace>> {
            let traces = self.traces.read().unwrap();
            Ok(traces.get(id).cloned())
        }
        
        async fn delete_trace(&self, id: &str) -> anyhow::Result<()> {
            let mut traces = self.traces.write().unwrap();
            traces.remove(id);
            Ok(())
        }
        
        async fn list_traces(&self, _query: &TraceQuery) -> anyhow::Result<Vec<AgentTrace>> {
            let traces = self.traces.read().unwrap();
            Ok(traces.values().cloned().collect())
        }
        
        async fn count_traces(&self, _query: &TraceQuery) -> anyhow::Result<u64> {
            let traces = self.traces.read().unwrap();
            Ok(traces.len() as u64)
        }
        
        async fn store_reasoning(&self, trace_id: &str, reasoning: &ReasoningChain) -> anyhow::Result<()> {
            let mut reasonings = self.reasonings.write().unwrap();
            reasonings.insert(trace_id.to_string(), reasoning.clone());
            Ok(())
        }
        
        async fn get_reasoning(&self, trace_id: &str) -> anyhow::Result<Option<ReasoningChain>> {
            let reasonings = self.reasonings.read().unwrap();
            Ok(reasonings.get(trace_id).cloned())
        }
        
        async fn store_decision(&self, _trace_id: &str, _decision: &DecisionPoint) -> anyhow::Result<()> {
            Ok(())
        }
        
        async fn get_decisions(&self, _trace_id: &str) -> anyhow::Result<Vec<DecisionPoint>> {
            Ok(vec![])
        }
        
        async fn store_evaluation(&self, _trace_id: &str, _evaluation: &EvaluationResult) -> anyhow::Result<()> {
            Ok(())
        }
        
        async fn get_evaluation(&self, _trace_id: &str) -> anyhow::Result<Option<EvaluationResult>> {
            Ok(None)
        }
        
        async fn aggregate(&self, _query: AggregationQuery) -> anyhow::Result<AggregationResult> {
            Ok(AggregationResult::Unknown)
        }
        
        async fn cleanup_expired(&self, _retention_days: u32) -> anyhow::Result<u64> {
            Ok(0)
        }
        
        async fn storage_stats(&self) -> anyhow::Result<StorageStats> {
            let traces = self.traces.read().unwrap();
            Ok(StorageStats {
                total_traces: traces.len() as u64,
                total_reasoning_chains: 0,
                total_decisions: 0,
                total_evaluations: 0,
                db_size_bytes: 0,
                oldest_trace_timestamp: None,
                newest_trace_timestamp: None,
            })
        }
        
        async fn flush(&self) -> anyhow::Result<()> {
            Ok(())
        }
        
        fn backend_name(&self) -> &'static str {
            "test_memory"
        }
    }
    
    fn create_test_collector() -> Arc<TraceCollector> {
        let store = Arc::new(TestTraceStore::new());
        Arc::new(TraceCollector::new(store, TraceCollectorConfig::default()))
    }
    
    #[tokio::test]
    async fn test_create_context() {
        let collector = create_test_collector();
        let context = collector.create_context("test-run").await;
        
        assert_eq!(context.run_id, "test-run");
        assert!(context.parent_trace_id.is_none());
    }
    
    #[tokio::test]
    async fn test_child_context() {
        let collector = create_test_collector();
        let parent = collector.create_context("test-run").await;
        let child = parent.child("parent-trace-id");
        
        assert_eq!(child.run_id, "test-run");
        assert_eq!(child.parent_trace_id, Some("parent-trace-id".to_string()));
        assert_eq!(child.depth, 1);
    }
    
    #[tokio::test]
    async fn test_record_user_message() {
        let collector = create_test_collector();
        let context = collector.create_context("test-run").await;
        
        let trace_id = collector.record_user_message(&context, "Hello").await.unwrap();
        assert!(!trace_id.is_empty());
        
        // 等待异步处理
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let trace = collector.store().get_trace(&trace_id).await.unwrap();
        assert!(trace.is_some());
    }
    
    #[tokio::test]
    async fn test_record_llm_call() {
        let collector = create_test_collector();
        let context = collector.create_context("test-run").await;
        
        let trace_id = collector.record_llm_call(
            &context,
            "openai",
            "gpt-4",
            "What is Rust?",
            "Rust is a systems programming language...",
            true,
            None,
            Some(TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
            Some(0.001),
            500,
        ).await.unwrap();
        
        assert!(!trace_id.is_empty());
        
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let trace = collector.store().get_trace(&trace_id).await.unwrap().unwrap();
        assert!(trace.output.success);
        assert_eq!(trace.output.tokens_used.unwrap().total_tokens, 30);
    }
    
    #[tokio::test]
    async fn test_llm_call_tracker() {
        let collector = create_test_collector();
        let context = collector.create_context("test-run").await;
        
        let tracker = LlmCallTracker::new(
            collector.clone(),
            context,
            "openai",
            "gpt-4",
            "Hello",
        );
        
        let trace_id = tracker.complete(
            "Hi there!",
            Some(TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 10,
                total_tokens: 15,
            }),
            Some(0.0005),
        ).await.unwrap();
        
        assert!(!trace_id.is_empty());
        
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let trace = collector.store().get_trace(&trace_id).await.unwrap().unwrap();
        assert!(trace.output.success);
    }
    
    #[tokio::test]
    async fn test_record_tool_call() {
        let collector = create_test_collector();
        let context = collector.create_context("test-run").await;
        
        let trace_id = collector.record_tool_call(
            &context,
            "file_reader",
            "read",
            serde_json::json!({"path": "/test.txt"}),
            serde_json::json!({"content": "Hello"}),
            true,
            None,
            100,
        ).await.unwrap();
        
        assert!(!trace_id.is_empty());
    }
    
    #[tokio::test]
    async fn test_record_error() {
        let collector = create_test_collector();
        let context = collector.create_context("test-run").await;
        
        let trace_id = collector.record_error(
            &context,
            "llm_client",
            "timeout",
            "Request timed out after 30s",
            Some("stack trace...".to_string()),
        ).await.unwrap();
        
        assert!(!trace_id.is_empty());
        
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let trace = collector.store().get_trace(&trace_id).await.unwrap().unwrap();
        assert!(!trace.output.success);
    }
    
    #[tokio::test]
    async fn test_record_reasoning() {
        let collector = create_test_collector();
        let context = collector.create_context("test-run").await;
        
        let trace_id = collector.record_llm_call(
            &context,
            "openai",
            "gpt-4",
            "Hello",
            "Hi",
            true,
            None,
            None,
            None,
            100,
        ).await.unwrap();
        
        let reasoning = ReasoningChain {
            steps: vec![ReasoningStep {
                step: 1,
                reasoning_type: ReasoningType::ProblemUnderstanding,
                content: "Understanding the user's greeting".to_string(),
                evidence: vec![],
                hypotheses: vec![],
                timestamp: Some(1000),
            }],
            conclusion: Some("User is greeting me".to_string()),
            confidence: Some(0.95),
            quality_score: Some(0.9),
        };
        
        collector.record_reasoning(&trace_id, reasoning).await.unwrap();
        
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let stored_reasoning = collector.store().get_reasoning(&trace_id).await.unwrap();
        assert!(stored_reasoning.is_some());
    }
    
    #[tokio::test]
    async fn test_shutdown() {
        let collector = create_test_collector();
        assert!(collector.is_running());
        
        collector.shutdown().await.unwrap();
        
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!collector.is_running());
    }
}
