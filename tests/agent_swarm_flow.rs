use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;
use zeroclaw::config::{Config, DelegateAgentConfig, SwarmConfig};
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{manager_for_workspace, SwarmContext};
use zeroclaw::tools::{SessionsSpawnTool, SubagentsTool, Tool};

fn parse_run_id(output: &str) -> Uuid {
    let first = output.lines().next().unwrap_or("");
    let (_, id) = first.split_once("run_id=").unwrap();
    Uuid::parse_str(id.trim()).unwrap()
}

#[tokio::test]
async fn swarm_spawn_wait_kill_flow() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    let mut cfg = Config::default();
    cfg.workspace_dir = workspace_dir.clone();
    cfg.default_provider = Some("delay".to_string());
    cfg.default_model = Some("delay:10".to_string());
    cfg.swarm = SwarmConfig {
        subagent_max_concurrent: 1,
    };

    let mut agents = HashMap::new();
    agents.insert(
        "worker".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:800".to_string(),
            system_prompt: None,
            api_key: None,
            temperature: Some(0.0),
            max_depth: 3,
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
    let subagents = SubagentsTool::new(
        security.clone(),
        cfg_arc,
        mgr.clone(),
        SwarmContext::root(),
    );

    let r1 = spawn
        .execute(json!({"agent":"worker","task":"task-1","label":"L1","orchestrator":false}))
        .await
        .unwrap();
    assert!(r1.success);
    let run1 = parse_run_id(&r1.output);

    let r2 = spawn
        .execute(json!({"agent":"worker","task":"task-2","label":"L2","orchestrator":false}))
        .await
        .unwrap();
    assert!(r2.success);
    let run2 = parse_run_id(&r2.output);

    let killed = subagents
        .execute(json!({"action":"kill","run_id":run2.to_string()}))
        .await
        .unwrap();
    assert!(killed.success);

    let waited = subagents
        .execute(json!({"action":"wait","run_id":run1.to_string(),"timeout_secs":5}))
        .await
        .unwrap();
    assert!(waited.success);
    assert!(waited.output.contains("task-1"));

    let listed = subagents.execute(json!({"action":"list"})).await.unwrap();
    assert!(listed.success);
    assert!(listed.output.contains(&run1.to_string()));
    assert!(listed.output.contains(&run2.to_string()));
}

