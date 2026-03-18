//! LLM client interface and implementations
//! 
//! This module provides a unified interface for LLM interactions:
//! - Trait definition for LLM clients
//! - Multiple backend support (OpenAI, Anthropic, local models)
//! - Embedding model interface
//! - Error handling and retry mechanisms

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// LLM client error types
#[derive(Error, Debug)]
pub enum LLMError {
    #[error("API request failed: {0}")]
    ApiError(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    
    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

/// Result type for LLM operations
pub type Result<T> = std::result::Result<T, LLMError>;

/// LLM response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    /// Generated content
    pub content: String,
    /// Model used
    pub model: String,
    /// Usage statistics
    pub usage: UsageStats,
    /// Finish reason
    pub finish_reason: String,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageStats {
    /// Prompt tokens
    pub prompt_tokens: u32,
    /// Completion tokens
    pub completion_tokens: u32,
    /// Total tokens
    pub total_tokens: u32,
}

/// Message role
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Model name
    pub model: String,
    /// Messages
    pub messages: Vec<ChatMessage>,
    /// Max tokens
    pub max_tokens: Option<u32>,
    /// Temperature
    pub temperature: Option<f32>,
    /// Top p
    pub top_p: Option<f32>,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// Model name
    pub model: String,
    /// Input text
    pub input: String,
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// Embedding vector
    pub embedding: Vec<f32>,
    /// Model used
    pub model: String,
    /// Usage statistics
    pub usage: UsageStats,
}

/// LLM client trait
#[async_trait]
pub trait LLMClient: Send + Sync {
    /// Generate text completion
    async fn generate(&self, prompt: &str) -> Result<LLMResponse>;
    
    /// Chat completion
    async fn chat(&self, messages: &[ChatMessage]) -> Result<LLMResponse>;
    
    /// Generate embedding
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    
    /// Get model info
    fn get_model_info(&self) -> ModelInfo;
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model name
    pub name: String,
    /// Max context length
    pub max_context_length: usize,
    /// Supports embeddings
    pub supports_embeddings: bool,
    /// Supports chat
    pub supports_chat: bool,
}

/// OpenAI-compatible client
pub struct OpenAIClient {
    api_key: String,
    base_url: String,
    model: String,
    embedding_model: String,
}

impl OpenAIClient {
    /// Create a new OpenAI client
    pub fn new(api_key: String, model: String, embedding_model: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            model,
            embedding_model,
        }
    }
    
    /// Create with custom base URL (for compatible APIs)
    pub fn with_base_url(
        api_key: String,
        base_url: String,
        model: String,
        embedding_model: String,
    ) -> Self {
        Self {
            api_key,
            base_url,
            model,
            embedding_model,
        }
    }
}

#[async_trait]
impl LLMClient for OpenAIClient {
    async fn generate(&self, prompt: &str) -> Result<LLMResponse> {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: prompt.to_string(),
        }];
        
        self.chat(&messages).await
    }
    
    async fn chat(&self, messages: &[ChatMessage]) -> Result<LLMResponse> {
        let _request = ChatRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            max_tokens: Some(4096),
            temperature: Some(0.7),
            top_p: Some(0.9),
        };
        
        // Implementation would make HTTP request to API
        // For now, return a placeholder
        tracing::info!("Chat request to {}: {} messages", self.model, messages.len());
        
        Ok(LLMResponse {
            content: "[Mock] LLM response".to_string(),
            model: self.model.clone(),
            usage: UsageStats::default(),
            finish_reason: "stop".to_string(),
        })
    }
    
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let _request = EmbeddingRequest {
            model: self.embedding_model.clone(),
            input: text.to_string(),
        };
        
        // Implementation would make HTTP request to API
        // For now, return a placeholder
        tracing::info!("Embedding request to {}: {} chars", self.embedding_model, text.len());
        
        // Return a dummy embedding (1536 dimensions for OpenAI)
        Ok(vec![0.0; 1536])
    }
    
    fn get_model_info(&self) -> ModelInfo {
        ModelInfo {
            name: self.model.clone(),
            max_context_length: 128000,
            supports_embeddings: true,
            supports_chat: true,
        }
    }
}

/// Local model client (for offline use)
pub struct LocalModelClient {
    model_path: String,
}

impl LocalModelClient {
    pub fn new(model_path: String) -> Self {
        Self { model_path }
    }
}

#[async_trait]
impl LLMClient for LocalModelClient {
    async fn generate(&self, prompt: &str) -> Result<LLMResponse> {
        tracing::info!("Local model generation: {} chars", prompt.len());
        
        Ok(LLMResponse {
            content: "[Local] Mock response".to_string(),
            model: "local".to_string(),
            usage: UsageStats::default(),
            finish_reason: "stop".to_string(),
        })
    }
    
    async fn chat(&self, messages: &[ChatMessage]) -> Result<LLMResponse> {
        tracing::info!("Local model chat: {} messages", messages.len());
        
        Ok(LLMResponse {
            content: "[Local] Mock chat response".to_string(),
            model: "local".to_string(),
            usage: UsageStats::default(),
            finish_reason: "stop".to_string(),
        })
    }
    
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        tracing::info!("Local model embedding: {} chars", text.len());
        
        Ok(vec![0.0; 768])
    }
    
    fn get_model_info(&self) -> ModelInfo {
        ModelInfo {
            name: "local".to_string(),
            max_context_length: 8192,
            supports_embeddings: true,
            supports_chat: true,
        }
    }
}

/// Mock client for testing
#[cfg(test)]
pub struct MockLLMClient {
    response_override: Option<String>,
}

#[cfg(test)]
impl MockLLMClient {
    pub fn with_response(response: String) -> Self {
        Self {
            response_override: Some(response),
        }
    }
}

#[async_trait]
#[cfg(test)]
impl LLMClient for MockLLMClient {
    async fn generate(&self, prompt: &str) -> Result<LLMResponse> {
        let content = self.response_override.clone()
            .unwrap_or_else(|| format!("[Mock] Response to: {}", prompt));
        
        Ok(LLMResponse {
            content,
            model: "mock".to_string(),
            usage: UsageStats::default(),
            finish_reason: "stop".to_string(),
        })
    }
    
    async fn chat(&self, messages: &[ChatMessage]) -> Result<LLMResponse> {
        let content = self.response_override.clone()
            .unwrap_or_else(|| format!("[Mock] Chat response to {} messages", messages.len()));
        
        Ok(LLMResponse {
            content,
            model: "mock".to_string(),
            usage: UsageStats::default(),
            finish_reason: "stop".to_string(),
        })
    }
    
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(vec![1.0; 1536])
    }
    
    fn get_model_info(&self) -> ModelInfo {
        ModelInfo {
            name: "mock".to_string(),
            max_context_length: 4096,
            supports_embeddings: true,
            supports_chat: true,
        }
    }
}
