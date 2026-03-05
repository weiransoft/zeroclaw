use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use zeroclaw::config::{Config, DelegateAgentConfig, SwarmConfig};
use zeroclaw::swarm::{manager_for_workspace, SwarmContext};
use zeroclaw::tools::sessions_spawn::SessionsSpawnTool;
use zeroclaw::tools::{SubagentsTool, Tool};

#[tokio::test]
async fn test_simple_orchestrator() {
    println!("=== Simple Orchestrator Test Started ===");
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    println!("Workspace: {:?}", workspace_dir);
    tracing::info!("=== Simple Orchestrator Test ===");
    tracing::info!("Workspace: {:?}", workspace_dir);

    let mut cfg = Config::default();
    cfg.workspace_dir = workspace_dir.clone();
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    cfg.api_key = Some("35d8f027d4a64ebebd76c4b5dc2665a3.XuWlGaHk4i8exBbw".to_string());
    cfg.swarm = SwarmConfig {
        subagent_max_concurrent: 5,
        orchestrator_prompt: None,
    };

    let mut agents = HashMap::new();

    let orchestrator_prompt = r#"You are an orchestrator. Your job is to coordinate work by delegating tasks to sub-agents.

CRITICAL: You MUST use the sessions_spawn tool to create sub-agents. Do NOT complete work yourself.

## Your Role
- You coordinate and delegate work to specialized sub-agents
- You synthesize results from multiple sub-agents
- You provide a final summary to user

## Rules
1. **Delegate, don't do** - Use sessions_spawn to create sub-agents for specific tasks
2. **Wait for results** - Sub-agents will report their results back to you automatically
3. **Synthesize** - Combine results from all sub-agents into a coherent response
4. **Stay focused** - Don't do the work yourself, just coordinate

## Workflow
Step 1: Analyze the task and identify what needs to be done
Step 2: Use sessions_spawn to create a sub-agent for the specific subtask
Step 3: Wait for the sub-agent to complete and report the result
Step 4: Synthesize and summarize the result

## Example: Using sessions_spawn

To create a sub-agent, use the sessions_spawn tool with:
- agent: "worker"
- task: "say hello"
- label: "hello-task"

The sub-agent will:
- Complete the assigned task
- Report the result back to you automatically
- Be terminated after completion

## Important Notes
- Each sub-agent handles one specific task
- Use labels to organize and track sub-agents
- Results are automatically reported - no need to poll
- Synthesize all results before responding to the user

Language: English."#;

    agents.insert(
        "orchestrator".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some(orchestrator_prompt.to_string()),
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
            system_prompt: Some("You are a worker. Complete the task assigned to you. Return a simple response.".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 3,
            soul_preset: None,
        },
    );

    cfg.agents = agents;

    let security = Arc::new(zeroclaw::security::SecurityPolicy::default());
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

    let task = "Use the sessions_spawn tool to create a simple task for the worker agent. The worker should respond with 'Hello from worker!'";

    println!("Starting orchestrator task");
    println!("Task: {}", task);
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
    let max_checks = 120;

    while !subtasks_created && check_count < max_checks {
        tokio::time::sleep(Duration::from_millis(500)).await;

        let all_runs = mgr.list().await;

        println!("Check {}/{}: Total runs = {}", check_count + 1, max_checks, all_runs.len());
        tracing::info!("Check {}/{}: Total runs = {}", check_count + 1, max_checks, all_runs.len());

        for run in &all_runs {
            if run.run_id == orchestrator_run_id {
                println!("  Orchestrator - Status: {:?}, Output length: {:?}",
                    run.status, run.output.as_ref().map(|o| o.len()));
                tracing::info!("  Orchestrator - Status: {:?}, Output length: {:?}",
                    run.status, run.output.as_ref().map(|o| o.len()));
                if let Some(output) = &run.output {
                    println!("  Orchestrator - Output preview: {}",
                        output.chars().take(200).collect::<String>());
                    tracing::info!("  Orchestrator - Output preview: {}",
                        output.chars().take(200).collect::<String>());
                }
            }
        }

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
        println!("No subtasks created after {} checks", max_checks);
        println!("Total runs: {}", all_runs.len());
        tracing::error!("No subtasks created after {} checks", max_checks);
        tracing::error!("Total runs: {}", all_runs.len());
        for run in &all_runs {
            println!("  Run - ID: {}, Agent: {}, Label: {:?}, Status: {:?}, Error: {:?}",
                run.run_id, run.agent_name, run.label, run.status, run.error);
            tracing::error!("  Run - ID: {}, Agent: {}, Label: {:?}, Status: {:?}, Output: {:?}, Error: {:?}",
                run.run_id, run.agent_name, run.label, run.status, run.output, run.error);
        }
        panic!("Orchestrator should create subtasks, but none were detected");
    }

    tracing::info!("=== Test completed successfully ===");
}

fn parse_run_id(output: &str) -> uuid::Uuid {
    let first = output.lines().next().unwrap_or("");
    let (_, id) = first.split_once("run_id=").unwrap();
    uuid::Uuid::parse_str(id.trim()).unwrap()
}
