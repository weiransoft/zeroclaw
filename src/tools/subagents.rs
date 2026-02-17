use super::traits::{Tool, ToolResult};
use crate::config::Config;
use crate::security::SecurityPolicy;
use crate::swarm::{SwarmContext, SwarmManager};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub struct SubagentsTool {
    security: Arc<SecurityPolicy>,
    config: Arc<Config>,
    manager: Arc<SwarmManager>,
    ctx: SwarmContext,
}

impl SubagentsTool {
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
impl Tool for SubagentsTool {
    fn name(&self) -> &str {
        "subagents"
    }

    fn description(&self) -> &str {
        "Manage sub-agent runs: list/get/wait/kill/steer."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "action": { "type": "string", "enum": ["list", "get", "wait", "kill", "steer"] },
                "run_id": { "type": "string", "description": "UUID of the sub-agent run" },
                "timeout_secs": { "type": "integer", "minimum": 0, "description": "Only for action=wait (0 = no timeout)" },
                "message": { "type": "string", "description": "Only for action=steer (new task/instructions)" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .ok_or_else(|| anyhow::anyhow!("Missing 'action'"))?;

        match action {
            "list" => {
                let mut runs = self.manager.list().await;
                runs.sort_by_key(|r| r.started_at_unix);
                let mut out = String::new();
                for r in runs {
                    out.push_str(&format!(
                        "run_id={} agent={} status={:?} label={}\n",
                        r.run_id,
                        r.agent_name,
                        r.status,
                        r.label.clone().unwrap_or_default()
                    ));
                }
                Ok(ToolResult {
                    success: true,
                    output: out.trim_end().to_string(),
                    error: None,
                })
            }
            "get" => {
                let run_id = parse_run_id(&args)?;
                let Some(run) = self.manager.get(run_id).await else {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some("unknown run_id".into()),
                    });
                };
                Ok(ToolResult {
                    success: true,
                    output: render_run(&run),
                    error: None,
                })
            }
            "wait" => {
                let run_id = parse_run_id(&args)?;
                let timeout_secs = args
                    .get("timeout_secs")
                    .and_then(|v| v.as_u64())
                    .or(Some(0));
                let run = self.manager.wait(run_id, timeout_secs).await?;
                Ok(ToolResult {
                    success: true,
                    output: render_run(&run),
                    error: None,
                })
            }
            "kill" => {
                let run_id = parse_run_id(&args)?;
                let ok = self.manager.kill(&self.security, run_id).await?;
                Ok(ToolResult {
                    success: ok,
                    output: if ok {
                        format!("killed run_id={run_id}")
                    } else {
                        format!("no-op run_id={run_id}")
                    },
                    error: None,
                })
            }
            "steer" => {
                let run_id = parse_run_id(&args)?;
                let msg = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .ok_or_else(|| anyhow::anyhow!("Missing 'message' for steer"))?;
                if msg.is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some("'message' must not be empty".into()),
                    });
                }
                let new_id = self
                    .manager
                    .steer(&self.security, self.config.clone(), self.ctx, run_id, msg)
                    .await?;
                Ok(ToolResult {
                    success: true,
                    output: format!("steered old_run_id={run_id} new_run_id={new_id}"),
                    error: None,
                })
            }
            _ => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("unknown action".into()),
            }),
        }
    }
}

fn parse_run_id(args: &serde_json::Value) -> anyhow::Result<Uuid> {
    let run_id = args
        .get("run_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .ok_or_else(|| anyhow::anyhow!("Missing 'run_id'"))?;
    Uuid::parse_str(run_id).map_err(|_| anyhow::anyhow!("Invalid 'run_id' (must be UUID)"))
}

fn render_run(run: &crate::swarm::SubagentRun) -> String {
    let mut out = String::new();
    out.push_str(&format!("run_id={}\n", run.run_id));
    out.push_str(&format!("agent={}\n", run.agent_name));
    out.push_str(&format!("status={:?}\n", run.status));
    if let Some(l) = run.label.as_deref() {
        if !l.is_empty() {
            out.push_str(&format!("label={l}\n"));
        }
    }
    out.push_str(&format!("depth={}\n", run.depth));
    out.push_str("task:\n");
    out.push_str(&run.task);
    out.push('\n');
    if let Some(o) = run.output.as_deref() {
        out.push_str("\noutput:\n");
        out.push_str(o);
        out.push('\n');
    }
    if let Some(e) = run.error.as_deref() {
        out.push_str("\nerror:\n");
        out.push_str(e);
        out.push('\n');
    }
    out.trim_end().to_string()
}
