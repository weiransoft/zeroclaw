use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use zeroclaw::config::{Config, DelegateAgentConfig, SwarmConfig};
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{manager_for_workspace, SwarmContext};
use zeroclaw::tools::sessions_spawn::SessionsSpawnTool;
use zeroclaw::tools::{SubagentsTool, Tool};

#[tokio::test]
async fn test_pm_spawn_single_subtask() {
    println!("=== Test PM Spawn Single Subtask ===");
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    println!("Workspace: {:?}", workspace_dir);

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

    agents.insert(
        "project_manager".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a project manager. Your ONLY job is to use the sessions_spawn tool to create ONE subtask for a worker agent.

CRITICAL: You MUST call the sessions_spawn tool immediately. Do NOT write any text. Just call the tool.

Example:
<tool>
{\"name\": \"sessions_spawn\", \"arguments\": {\"agent\": \"worker\", \"task\": \"Say hello\", \"label\": \"test\"}}
</tool>".to_string()),
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
            system_prompt: Some("You are a worker. Just respond with 'Hello from worker!'".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 3,
            soul_preset: None,
        },
    );

    cfg.agents = agents;

    let security = Arc::new(SecurityPolicy::from_config(&cfg.autonomy, &cfg.workspace_dir));
    let mgr = manager_for_workspace(&cfg.workspace_dir, cfg.swarm.subagent_max_concurrent).unwrap();
    let cfg_arc = Arc::new(cfg);

    let spawn = SessionsSpawnTool::new(
        security.clone(),
        cfg_arc.clone(),
        mgr.clone(),
        SwarmContext::root(),
    );
    let _subagents = SubagentsTool::new(
        security.clone(),
        cfg_arc.clone(),
        mgr.clone(),
        SwarmContext::root(),
    );

    println!("Starting project manager...");
    
    let r1 = spawn
        .execute(json!({
            "agent": "project_manager",
            "task": "Create ONE subtask for the worker agent to say hello",
            "label": "pm-test",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    println!("Project manager started: {}", r1.success);
    assert!(r1.success);

    let first_line = r1.output.lines().next().unwrap_or("");
    let (_, run_id_str) = first_line.split_once("run_id=").unwrap();
    let pm_run_id = uuid::Uuid::parse_str(run_id_str.trim()).unwrap();
    println!("PM run ID: {}", pm_run_id);

    tokio::time::sleep(Duration::from_millis(100)).await;

    println!("Checking for subtask creation...");
    let mut subtask_found = false;
    let mut check_count = 0;
    let max_checks = 60;

    while !subtask_found && check_count < max_checks {
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let all_runs = mgr.list().await;
        println!("Check {}/{}: Total runs = {}", check_count + 1, max_checks, all_runs.len());
        
        if all_runs.len() > 1 {
            subtask_found = true;
            println!("✓ Subtask created!");
            
            for run in &all_runs {
                if run.run_id != pm_run_id {
                    println!("  Subtask - Agent: {}, Label: {:?}, Status: {:?}", 
                        run.agent_name, run.label, run.status);
                }
            }
        }
        
        check_count += 1;
    }

    assert!(subtask_found, "Project manager should create a subtask");
    println!("✓ Test passed!");
}
