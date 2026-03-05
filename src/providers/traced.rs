//! TracedProvider - 带轨迹追踪的 LLM 提供者包装器
//!
//! 提供低侵入性的 LLM 调用追踪能力。通过包装任何 Provider 实现，
//! 自动记录所有 LLM 调用的轨迹，无需修改现有代码。
//!
//! # 使用示例
//! ```ignore
//! use zeroclaw::providers::openai::OpenAIProvider;
//! use zeroclaw::observability::trace_store::{TraceCollector, TraceContext};
//!
//! // 创建原始提供者
//! let provider = OpenAIProvider::new(api_key);
//!
//! // 包装为带追踪的提供者
//! let traced = TracedProvider::new(Box::new(provider), collector, "openai".into());
//!
//! // 正常使用，自动记录轨迹
//! let response = traced.chat("Hello", "gpt-4", 0.7).await?;
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

use crate::observability::trace_store::{TraceCollector, TokenUsage};
use crate::providers::traits::{ChatMessage, ChatResponse, Provider, StreamChunk};
use crate::tools::ToolSpec;

/// 模型价格配置（每百万token价格，单位：美元）
/// 参考各模型提供商的官方定价
const MODEL_PRICES: &[(&str, f64, f64)] = &[
    // OpenAI 模型
    ("gpt-4", 30.0, 60.0),
    ("gpt-4-turbo", 10.0, 30.0),
    ("gpt-3.5-turbo", 0.5, 1.5),
    // Anthropic 模型
    ("claude-3-opus", 15.0, 75.0),
    ("claude-3-sonnet", 3.0, 15.0),
    ("claude-3-haiku", 0.25, 1.25),
    // GLM 模型（估算价格）
    ("glm-4", 1.0, 2.0),
    ("glm-4-plus", 2.0, 4.0),
    ("glm-5", 2.0, 4.0),
];

/// 计算模型调用成本
/// 
/// # 参数
/// - `model`: 模型名称
/// - `input_tokens`: 输入token数量
/// - `output_tokens`: 输出token数量
/// 
/// # 返回
/// - 成本（美元）
fn calculate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> Option<f64> {
    // 尝试精确匹配
    if let Some((_, input_price, output_price)) = MODEL_PRICES.iter().find(|(m, _, _)| *m == model) {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * input_price;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * output_price;
        return Some(input_cost + output_cost);
    }
    
    // 尝试部分匹配（处理模型版本后缀）
    for (known_model, input_price, output_price) in MODEL_PRICES.iter() {
        if model.starts_with(known_model) || known_model.starts_with(model) {
            let input_cost = (input_tokens as f64 / 1_000_000.0) * input_price;
            let output_cost = (output_tokens as f64 / 1_000_000.0) * output_price;
            return Some(input_cost + output_cost);
        }
    }
    
    // 未知模型，返回 None
    None
}

/// 带轨迹追踪的 LLM 提供者包装器
///
/// 包装任何 Provider 实现，自动记录所有 LLM 调用。
/// 这是实现低侵入性集成的核心组件。
pub struct TracedProvider {
    /// 内部被包装的提供者
    inner: Box<dyn Provider>,
    /// 轨迹收集器
    collector: Arc<TraceCollector>,
    /// 提供者名称（用于标识）
    provider_name: String,
}

impl TracedProvider {
    /// 创建新的带追踪的提供者
    ///
    /// # 参数
    /// - `inner`: 被包装的提供者
    /// - `collector`: 轨迹收集器
    /// - `provider_name`: 提供者名称（如 "openai", "anthropic"）
    pub fn new(
        inner: Box<dyn Provider>,
        collector: Arc<TraceCollector>,
        provider_name: String,
    ) -> Self {
        Self {
            inner,
            collector,
            provider_name,
        }
    }
    
    /// 记录 LLM 调用结果
    async fn record_call(
        &self,
        model: &str,
        input: String,
        result: &anyhow::Result<ChatResponse>,
        duration_ms: u64,
    ) {
        // 获取或创建默认上下文
        let context = match self.collector.get_context("default").await {
            Some(ctx) => ctx,
            None => self.collector.create_context("default").await,
        };
        
        match result {
            Ok(response) => {
                // 转换 Token 使用量
                let tokens = response.usage.as_ref().map(|u| TokenUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                });
                
                // 构建输出内容
                let output_content = if !response.tool_calls.is_empty() {
                    // 包含工具调用时，记录工具调用信息
                    serde_json::to_string(&response.tool_calls).unwrap_or_default()
                } else {
                    response.text_or_empty().to_string()
                };
                
                // 计算成本
                let cost = tokens.as_ref().and_then(|t| {
                    calculate_cost(model, t.prompt_tokens, t.completion_tokens)
                });
                
                let _ = self.collector.record_llm_call(
                    &context,
                    &self.provider_name,
                    model,
                    input,
                    output_content,
                    true,
                    None,
                    tokens,
                    cost,
                    duration_ms,
                ).await;
            }
            Err(e) => {
                let _ = self.collector.record_llm_call(
                    &context,
                    &self.provider_name,
                    model,
                    input,
                    String::new(),
                    false,
                    Some(e.to_string()),
                    None,
                    None,
                    duration_ms,
                ).await;
            }
        }
    }
    
    /// 获取内部提供者引用
    pub fn inner(&self) -> &dyn Provider {
        self.inner.as_ref()
    }
    
    /// 获取提供者名称
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }
}

#[async_trait]
impl Provider for TracedProvider {
    async fn chat(
        &self,
        message: &str,
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<ChatResponse> {
        let start = Instant::now();
        let input = format!("[temp={}] {}", temperature, message);
        
        let result = self.inner.chat(message, model, temperature).await;
        
        self.record_call(model, input, &result, start.elapsed().as_millis() as u64).await;
        
        result
    }
    
    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<ChatResponse> {
        let start = Instant::now();
        let input = match system_prompt {
            Some(sys) => format!("[system] {}\n[user] {}", sys, message),
            None => message.to_string(),
        };
        
        let result = self.inner.chat_with_system(system_prompt, message, model, temperature).await;
        
        self.record_call(model, input, &result, start.elapsed().as_millis() as u64).await;
        
        result
    }
    
    async fn chat_with_system_and_tools(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: f64,
        tools: Option<&[ToolSpec]>,
    ) -> anyhow::Result<ChatResponse> {
        let start = Instant::now();
        
        // 构建包含工具信息的输入记录
        let input = match (system_prompt, tools) {
            (Some(sys), Some(t)) => {
                let tool_names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
                format!("[system] {}\n[tools: {}]\n[user] {}", sys, tool_names.join(", "), message)
            }
            (None, Some(t)) => {
                let tool_names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
                format!("[tools: {}]\n[user] {}", tool_names.join(", "), message)
            }
            (Some(sys), None) => format!("[system] {}\n[user] {}", sys, message),
            (None, None) => message.to_string(),
        };
        
        let result = self.inner.chat_with_system_and_tools(system_prompt, message, model, temperature, tools).await;
        
        self.record_call(model, input, &result, start.elapsed().as_millis() as u64).await;
        
        result
    }
    
    async fn chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<ChatResponse> {
        let start = Instant::now();
        
        // 构建历史消息记录
        let input = messages.iter()
            .map(|m| format!("[{}] {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        
        let result = self.inner.chat_with_history(messages, model, temperature).await;
        
        self.record_call(model, input, &result, start.elapsed().as_millis() as u64).await;
        
        result
    }
    
    async fn stream_chat(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: f64,
        tools: Option<&[ToolSpec]>,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<anyhow::Result<StreamChunk>>> {
        // 流式响应需要特殊处理：记录开始时间，在流结束时记录
        let start = Instant::now();
        let input = match system_prompt {
            Some(sys) => format!("[system] {}\n[user] {}", sys, message),
            None => message.to_string(),
        };
        
        // 调用内部流式接口
        let mut inner_rx = self.inner.stream_chat(system_prompt, message, model, temperature, tools).await?;
        
        // 创建新的通道用于转发
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        
        // 克隆需要的变量用于异步任务
        let collector = self.collector.clone();
        let provider_name = self.provider_name.clone();
        let model_owned = model.to_string();
        let input_owned = input;
        
        tokio::spawn(async move {
            let mut full_text = String::new();
            let mut final_tool_calls = vec![];
            let mut has_error = false;
            let mut error_msg = String::new();
            
            while let Some(chunk_result) = inner_rx.recv().await {
                match chunk_result {
                    Ok(chunk) => {
                        full_text.push_str(&chunk.text);
                        if chunk.is_final {
                            final_tool_calls = chunk.tool_calls.clone();
                        }
                        // 转发给调用者
                        if tx.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        has_error = true;
                        error_msg = e.to_string();
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
            
            // 记录轨迹
            let duration_ms = start.elapsed().as_millis() as u64;
            let context = match collector.get_context("default").await {
                Some(ctx) => ctx,
                None => collector.create_context("default").await,
            };
            
            if has_error {
                let _ = collector.record_llm_call(
                    &context,
                    &provider_name,
                    &model_owned,
                    input_owned,
                    String::new(),
                    false,
                    Some(error_msg),
                    None,
                    None,
                    duration_ms,
                ).await;
            } else {
                let output_content = if !final_tool_calls.is_empty() {
                    serde_json::to_string(&final_tool_calls).unwrap_or(full_text.clone())
                } else {
                    full_text
                };
                
                let _ = collector.record_llm_call(
                    &context,
                    &provider_name,
                    &model_owned,
                    input_owned,
                    output_content,
                    true,
                    None,
                    None, // 流式响应通常不返回 token 使用量
                    None,
                    duration_ms,
                ).await;
            }
        });
        
        Ok(rx)
    }
    
    async fn warmup(&self) -> anyhow::Result<()> {
        self.inner.warmup().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试用的简单提供者
    #[derive(Clone)]
    struct MockProvider {
        response: String,
    }
    
    impl MockProvider {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
            }
        }
    }
    
    #[async_trait]
    impl Provider for MockProvider {
        async fn chat(
            &self,
            _message: &str,
            _model: &str,
            _temperature: f64,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse::with_text(&self.response))
        }
        
        async fn chat_with_system(
            &self,
            _system_prompt: Option<&str>,
            _message: &str,
            _model: &str,
            _temperature: f64,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse::with_text(&self.response))
        }
    }
    
    /// 测试 TracedProvider 的基本功能
    /// 注意：完整的 TraceCollector 集成测试在 trace_store 模块中
    #[tokio::test]
    async fn test_traced_provider_basic() {
        let mock = MockProvider::new("Hello, world!");
        
        // 测试基本调用
        let result = mock.chat("Hi", "test-model", 0.7).await.unwrap();
        assert_eq!(result.text_or_empty(), "Hello, world!");
    }
    
    #[tokio::test]
    async fn test_traced_provider_with_system_basic() {
        let mock = MockProvider::new("Response");
        
        let result = mock.chat_with_system(Some("Be helpful"), "Hello", "model", 0.5).await.unwrap();
        assert_eq!(result.text_or_empty(), "Response");
    }
}
