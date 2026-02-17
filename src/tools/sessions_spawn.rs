use super::traits::{Tool, ToolResult};
use crate::config::Config;
use crate::security::SecurityPolicy;
use crate::swarm::{SwarmContext, SwarmManager};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub struct SessionsSpawnTool {
    security: Arc<SecurityPolicy>,
    config: Arc<Config>,
    manager: Arc<SwarmManager>,
    ctx: SwarmContext,
}

impl SessionsSpawnTool {
    pub fn new(
        security: Arc<SecurityPolicy>,
        config: Arc<Config>,
        manager: Arc<SwarmManager>,
        ctx: SwarmContext,
    ) -> Self {
        Self {
            security,
            config,
            manager,
            ctx,
        }
    }
}

#[async_trait]
impl Tool for SessionsSpawnTool {
    fn name(&self) -> &str {
        "sessions_spawn"
    }

    fn description(&self) -> &str {
        "Spawn a sub-agent run in the swarm. Returns a run_id; use subagents action=wait to fetch results."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        let agent_names: Vec<&str> = self
            .config
            .agents
            .keys()
            .map(|s: &String| s.as_str())
            .collect();
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "agent": {
                    "type": "string",
                    "minLength": 1,
                    "description": format!(
                        "Agent name from config.agents. Available: {}",
                        if agent_names.is_empty() { "(none configured)".to_string() } else { agent_names.join(", ") }
                    )
                },
                "task": { "type": "string", "minLength": 1 },
                "label": { "type": "string" },
                "orchestrator": { "type": "boolean", "default": false },
                "parent_run_id": { "type": "string", "description": "Optional UUID for parent run" },
                "cleanup": { "type": "boolean", "default": false }
            },
            "required": ["agent", "task"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let agent_name = args
            .get("agent")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .ok_or_else(|| anyhow::anyhow!("Missing 'agent'"))?;
        if agent_name.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("'agent' must not be empty".into()),
            });
        }

        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .ok_or_else(|| anyhow::anyhow!("Missing 'task'"))?;
        if task.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("'task' must not be empty".into()),
            });
        }

        let label = args
            .get("label")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let orchestrator = args
            .get("orchestrator")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let cleanup = args.get("cleanup").and_then(|v| v.as_bool()).unwrap_or(false);

        let parent_run_id = args
            .get("parent_run_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| Uuid::parse_str(s))
            .transpose()
            .map_err(|_| anyhow::anyhow!("Invalid 'parent_run_id' (must be UUID)"))?;
        if let Some(pid) = parent_run_id {
            if self.manager.get(pid).await.is_none() {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("unknown parent_run_id".into()),
                });
            }
        }

        let run_id = self
            .manager
            .spawn(
                &self.security,
                self.config.clone(),
                self.ctx,
                agent_name,
                task,
                label,
                orchestrator,
                parent_run_id,
                cleanup,
            )
            .await?;

        Ok(ToolResult {
            success: true,
            output: format!(
                "spawned run_id={run_id}\nuse subagents action=wait run_id={run_id} to fetch result"
            ),
            error: None,
        })
    }
}
