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
        "CRITICAL TOOL for orchestrators to manage sub-agent runs. REQUIRED for task delegation.

Actions:
- spawn: Create a new sub-agent run (DELEGATE work to other agents)
- list: List all sub-agent runs
- get: Get details of a specific sub-agent run
- wait: Wait for a sub-agent run to complete (BLOCKING - use sparingly)
- poll: Check status of a sub-agent run WITHOUT waiting (NON-BLOCKING - preferred)
- check_all: Check status of all sub-agent runs (NON-BLOCKING)
- kill: Terminate a running sub-agent
- steer: Send a message to a running sub-agent

IMPORTANT: 
- Use 'poll' and 'check_all' for non-blocking status checks
- Use 'group_chat' to communicate progress with other agents
- Only use 'wait' when absolutely necessary
- As an orchestrator, you MUST use this tool to delegate work. Do NOT complete tasks yourself."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "action": { "type": "string", "enum": ["list", "get", "wait", "poll", "check_all", "kill", "steer"] },
                "run_id": { "type": "string", "description": "UUID of the sub-agent run" },
                "timeout_secs": { "type": "integer", "minimum": 0, "description": "Only for action=wait (0 = no timeout)" },
                "message": { "type": "string", "description": "Only for action=steer (new task/instructions)" }
            },
            "required": ["action"]
        })
    }

    fn spec(&self) -> crate::tools::ToolSpec {
        crate::tools::ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .ok_or_else(|| anyhow::anyhow!("Missing 'action'"))?;

        tracing::debug!("[Subagents] Executing action: {}", action);
        tracing::debug!("[Subagents] Action arguments: {:?}", args);

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
            "poll" => {
                let run_id = parse_run_id(&args)?;
                let Some(run) = self.manager.get(run_id).await else {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some("unknown run_id".into()),
                    });
                };
                let status_summary = format!(
                    "run_id={} agent={} status={:?} progress=checking\nUse group_chat to communicate with this agent.",
                    run.run_id, run.agent_name, run.status
                );
                Ok(ToolResult {
                    success: true,
                    output: status_summary,
                    error: None,
                })
            }
            "check_all" => {
                let runs = self.manager.list().await;
                let mut completed = 0;
                let mut running = 0;
                let mut pending = 0;
                let mut failed = 0;
                
                for run in &runs {
                    match run.status {
                        crate::swarm::RunStatus::Completed => completed += 1,
                        crate::swarm::RunStatus::Running => running += 1,
                        crate::swarm::RunStatus::Pending => pending += 1,
                        crate::swarm::RunStatus::Failed => failed += 1,
                        _ => {}
                    }
                }
                
                let summary = format!(
                    "Sub-agent status summary:\n- Completed: {}\n- Running: {}\n- Pending: {}\n- Failed: {}\n\nUse group_chat to communicate with agents. Use poll to check individual status.",
                    completed, running, pending, failed
                );
                
                Ok(ToolResult {
                    success: true,
                    output: summary,
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

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(SubagentsTool::new(
            self.security.clone(),
            self.config.clone(),
            self.manager.clone(),
            self.ctx.clone()
        ))
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
