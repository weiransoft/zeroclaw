use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;
use zeroclaw::config::{Config, DelegateAgentConfig, SwarmConfig};
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{manager_for_workspace, SwarmContext};
use zeroclaw::tools::sessions_spawn::SessionsSpawnTool;
use zeroclaw::tools::{SubagentsTool, Tool};

fn parse_run_id(output: &str) -> Uuid {
    let first = output.lines().next().unwrap_or("");
    let (_, id) = first.split_once("run_id=").unwrap();
    Uuid::parse_str(id.trim()).unwrap()
}

#[tokio::test]
async fn test_simple_orchestrator_with_subagents() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== Simple Orchestrator Test ===");
    tracing::info!("Workspace: {:?}", workspace_dir);

    let mut cfg = Config::default();
    cfg.workspace_dir = workspace_dir.clone();
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    cfg.swarm = SwarmConfig {
        subagent_max_concurrent: 5,
        orchestrator_prompt: None,
    };

    let mut agents = HashMap::new();

    agents.insert(
        "orchestrator".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are the coordinator. You MUST use the subagents tool to assign tasks.

Steps:
1. Analyze the task and break it down into subtasks
2. Use the subagents tool's spawn action to create subtasks
3. Wait for subtasks to complete (using wait action)
4. Aggregate the results from subtasks

Important:
- You MUST use the subagents tool, do not complete all work yourself
- Each subtask should have a clear agent name and task description
- Use appropriate labels to identify subtasks
- Ensure the orchestrator parameter is set correctly for subtasks

Language: Chinese or English.".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 5,
            soul_preset: None,
        },
    );

    agents.insert(
        "worker".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a worker agent. Complete the task assigned to you.".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 3,
            soul_preset: None,
        },
    );

    cfg.agents = agents;

    let security = Arc::new(SecurityPolicy::default());
    let mgr = manager_for_workspace(&workspace_dir, cfg.swarm.subagent_max_concurrent).unwrap();

    let ctx = SwarmContext {
        depth: 0,
        allow_spawn: true,
    };

    let spawn = SessionsSpawnTool::new(
        security.clone(),
        Arc::new(cfg.clone()),
        mgr.clone(),
        ctx.clone(),
    );

    let _subagents = SubagentsTool::new(
        security.clone(),
        Arc::new(cfg.clone()),
        mgr.clone(),
        ctx.clone(),
    );

    let task = "Use the subagents tool to create a simple task for the worker agent. The worker should say 'Hello from worker!'";

    tracing::info!("Starting orchestrator task");
    tracing::info!("Task: {}", task);

    let r1 = spawn
        .execute(json!({
            "agent": "orchestrator",
            "task": task,
            "label": "orchestrator-task",
            "orchestrator": true
        }))
        .await
        .unwrap();

    tracing::info!("Orchestrator task started: {}", r1.success);
    assert!(r1.success);
    let orchestrator_run_id = parse_run_id(&r1.output);
    tracing::info!("Orchestrator run ID: {}", orchestrator_run_id);

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("=== Monitoring subtask creation ===");
    let mut subtasks_created = false;
    let mut check_count = 0;
    let max_checks = 60;

    while !subtasks_created && check_count < max_checks {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let all_runs = mgr.list().await;

        tracing::info!("Check {}/{}: Total runs = {}", check_count + 1, max_checks, all_runs.len());

        if all_runs.len() > 1 {
            subtasks_created = true;
            tracing::info!("✓ Subtasks detected!");

            for run in &all_runs {
                if run.run_id != orchestrator_run_id {
                    tracing::info!("  Subtask - Agent: {}, Label: {:?}, Status: {:?}",
                        run.agent_name, run.label, run.status);
                }
            }
        }

        check_count += 1;
    }

    if !subtasks_created {
        let all_runs = mgr.list().await;
        tracing::error!("No subtasks created after {} checks", max_checks);
        tracing::error!("Total runs: {}", all_runs.len());
        for run in &all_runs {
            tracing::error!("  Run - ID: {}, Agent: {}, Label: {:?}, Status: {:?}, Output: {:?}",
                run.run_id, run.agent_name, run.label, run.status, run.output);
        }
        panic!("Orchestrator should create subtasks, but none were detected");
    }

    tracing::info!("=== Test completed successfully ===");
}
