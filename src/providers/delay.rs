use super::traits::{ChatResponse, Provider};
use async_trait::async_trait;
use tokio::time::{sleep, Duration};

pub struct DelayProvider;

impl DelayProvider {
    pub fn new() -> Self {
        Self
    }
}

fn parse_delay(model: &str) -> Duration {
    let model = model.trim();
    let Some((_, ms)) = model.split_once(':') else {
        return Duration::from_millis(100);
    };
    let Ok(ms) = ms.trim().parse::<u64>() else {
        return Duration::from_millis(100);
    };
    Duration::from_millis(ms.min(60_000))
}

#[async_trait]
impl Provider for DelayProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        message: &str,
        model: &str,
        _temperature: f64,
    ) -> anyhow::Result<ChatResponse> {
        sleep(parse_delay(model)).await;
        Ok(ChatResponse::with_text(message))
    }
}

