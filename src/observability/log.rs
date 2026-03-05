use super::traits::{Observer, ObserverEvent, ObserverMetric};
use tracing::info;

/// Log-based observer — uses tracing, zero external deps
pub struct LogObserver;

impl LogObserver {
    pub fn new() -> Self {
        Self
    }
}

impl Observer for LogObserver {
    fn record_event(&self, event: &ObserverEvent) {
        match event {
            ObserverEvent::AgentStart { provider, model } => {
                info!(provider = %provider, model = %model, "agent.start");
            }
            ObserverEvent::LlmRequest {
                provider,
                model,
                messages_count,
            } => {
                info!(
                    provider = %provider,
                    model = %model,
                    messages_count = messages_count,
                    "llm.request"
                );
            }
            ObserverEvent::LlmResponse {
                provider,
                model,
                duration,
                success,
                error_message,
                tokens_used,
            } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(
                    provider = %provider,
                    model = %model,
                    duration_ms = ms,
                    success = success,
                    error = ?error_message,
                    tokens = ?tokens_used,
                    "llm.response"
                );
            }
            ObserverEvent::AgentEnd {
                duration,
                tokens_used,
            } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(duration_ms = ms, tokens = ?tokens_used, "agent.end");
            }
            ObserverEvent::ToolCallStart { tool } => {
                info!(tool = %tool, "tool.start");
            }
            ObserverEvent::ToolCall {
                tool,
                duration,
                success,
            } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(tool = %tool, duration_ms = ms, success = success, "tool.call");
            }
            ObserverEvent::TurnComplete => {
                info!("turn.complete");
            }
            ObserverEvent::ChannelMessage { channel, direction } => {
                info!(channel = %channel, direction = %direction, "channel.message");
            }
            ObserverEvent::HeartbeatTick => {
                info!("heartbeat.tick");
            }
            ObserverEvent::Error { component, message } => {
                info!(component = %component, error = %message, "error");
            }
            
            // ── Enhanced Events ─────────────────────────────────────────
            ObserverEvent::SwarmSpawn { run_id, agent_name, depth } => {
                info!(run_id = %run_id, agent = %agent_name, depth = depth, "swarm.spawn");
            }
            ObserverEvent::SwarmComplete { run_id, agent_name, duration, success } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(run_id = %run_id, agent = %agent_name, duration_ms = ms, success = success, "swarm.complete");
            }
            ObserverEvent::ProviderFallback { from_provider, to_provider, reason } => {
                info!(from = %from_provider, to = %to_provider, reason = %reason, "provider.fallback");
            }
            ObserverEvent::RateLimited { provider, retry_after_ms } => {
                info!(provider = %provider, retry_after_ms = ?retry_after_ms, "provider.rate_limited");
            }
            ObserverEvent::ToolExecute { tool, action, duration, success, error } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(tool = %tool, action = %action, duration_ms = ms, success = success, error = ?error, "tool.execute");
            }
            ObserverEvent::DatabaseOperation { operation, table, duration, success } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(operation = %operation, table = %table, duration_ms = ms, success = success, "db.operation");
            }
            ObserverEvent::ConfigLoad { config_path, duration, success } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(path = %config_path, duration_ms = ms, success = success, "config.load");
            }
            ObserverEvent::ProviderCreate { provider, duration, success } => {
                let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                info!(provider = %provider, duration_ms = ms, success = success, "provider.create");
            }
            ObserverEvent::CryptoOperation { operation, duration, success } => {
                let us = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
                info!(operation = %operation, duration_us = us, success = success, "crypto.operation");
            }
            ObserverEvent::QueueOperation { operation, queue_depth, duration } => {
                let us = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
                info!(operation = %operation, depth = queue_depth, duration_us = us, "queue.operation");
            }
        }
    }

    fn record_metric(&self, metric: &ObserverMetric) {
        match metric {
            ObserverMetric::RequestLatency(d) => {
                let ms = u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
                info!(latency_ms = ms, "metric.request_latency");
            }
            ObserverMetric::TokensUsed(t) => {
                info!(tokens = t, "metric.tokens_used");
            }
            ObserverMetric::ActiveSessions(s) => {
                info!(sessions = s, "metric.active_sessions");
            }
            ObserverMetric::QueueDepth(d) => {
                info!(depth = d, "metric.queue_depth");
            }
        }
    }

    fn name(&self) -> &str {
        "log"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn log_observer_name() {
        assert_eq!(LogObserver::new().name(), "log");
    }

    #[test]
    fn log_observer_all_events_no_panic() {
        let obs = LogObserver::new();
        obs.record_event(&ObserverEvent::AgentStart {
            provider: "openrouter".into(),
            model: "claude-sonnet".into(),
        });
        obs.record_event(&ObserverEvent::LlmRequest {
            provider: "openrouter".into(),
            model: "claude-sonnet".into(),
            messages_count: 2,
        });
        obs.record_event(&ObserverEvent::LlmResponse {
            provider: "openrouter".into(),
            model: "claude-sonnet".into(),
            duration: Duration::from_millis(250),
            success: true,
            error_message: None,
            tokens_used: Some(100),
        });
        obs.record_event(&ObserverEvent::AgentEnd {
            duration: Duration::from_millis(500),
            tokens_used: Some(100),
        });
        obs.record_event(&ObserverEvent::AgentEnd {
            duration: Duration::ZERO,
            tokens_used: None,
        });
        obs.record_event(&ObserverEvent::ToolCallStart {
            tool: "shell".into(),
        });
        obs.record_event(&ObserverEvent::ToolCall {
            tool: "shell".into(),
            duration: Duration::from_millis(10),
            success: false,
        });
        obs.record_event(&ObserverEvent::TurnComplete);
        obs.record_event(&ObserverEvent::ChannelMessage {
            channel: "telegram".into(),
            direction: "outbound".into(),
        });
        obs.record_event(&ObserverEvent::HeartbeatTick);
        obs.record_event(&ObserverEvent::Error {
            component: "provider".into(),
            message: "timeout".into(),
        });
        // Enhanced events
        obs.record_event(&ObserverEvent::SwarmSpawn {
            run_id: "test-run-id".into(),
            agent_name: "test-agent".into(),
            depth: 1,
        });
        obs.record_event(&ObserverEvent::SwarmComplete {
            run_id: "test-run-id".into(),
            agent_name: "test-agent".into(),
            duration: Duration::from_millis(100),
            success: true,
        });
        obs.record_event(&ObserverEvent::ProviderFallback {
            from_provider: "openrouter".into(),
            to_provider: "anthropic".into(),
            reason: "rate limit".into(),
        });
        obs.record_event(&ObserverEvent::RateLimited {
            provider: "openrouter".into(),
            retry_after_ms: Some(1000),
        });
        obs.record_event(&ObserverEvent::ToolExecute {
            tool: "shell".into(),
            action: "execute".into(),
            duration: Duration::from_millis(50),
            success: true,
            error: None,
        });
        obs.record_event(&ObserverEvent::DatabaseOperation {
            operation: "insert".into(),
            table: "messages".into(),
            duration: Duration::from_millis(5),
            success: true,
        });
        obs.record_event(&ObserverEvent::ConfigLoad {
            config_path: "/path/to/config.toml".into(),
            duration: Duration::from_millis(10),
            success: true,
        });
        obs.record_event(&ObserverEvent::ProviderCreate {
            provider: "openrouter".into(),
            duration: Duration::from_millis(1),
            success: true,
        });
        obs.record_event(&ObserverEvent::CryptoOperation {
            operation: "encrypt".into(),
            duration: Duration::from_micros(100),
            success: true,
        });
        obs.record_event(&ObserverEvent::QueueOperation {
            operation: "enqueue".into(),
            queue_depth: 5,
            duration: Duration::from_micros(50),
        });
    }

    #[test]
    fn log_observer_all_metrics_no_panic() {
        let obs = LogObserver::new();
        obs.record_metric(&ObserverMetric::RequestLatency(Duration::from_secs(2)));
        obs.record_metric(&ObserverMetric::TokensUsed(0));
        obs.record_metric(&ObserverMetric::TokensUsed(u64::MAX));
        obs.record_metric(&ObserverMetric::ActiveSessions(1));
        obs.record_metric(&ObserverMetric::QueueDepth(999));
    }
}
