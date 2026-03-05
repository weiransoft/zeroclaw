use super::traits::{ChatResponse, Provider};
use async_trait::async_trait;

pub struct EchoProvider;

impl EchoProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for EchoProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        message: &str,
        _model: &str,
        _temperature: f64,
    ) -> anyhow::Result<ChatResponse> {
        Ok(ChatResponse::with_text(message))
    }
}

