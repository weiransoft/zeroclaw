use tempfile::TempDir;
use uuid::Uuid;
use zeroclaw::swarm::{RunStatus, SubagentRun, SwarmManager};

#[tokio::test]
async fn loads_persisted_runs_and_terminates_inflight() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(workspace.join(".zeroclaw")).unwrap();

    let run_id = Uuid::new_v4();
    let runs = vec![SubagentRun {
        run_id,
        parent_run_id: None,
        agent_name: "worker".to_string(),
        label: Some("l".to_string()),
        task: "t".to_string(),
        orchestrator: false,
        status: RunStatus::Running,
        depth: 1,
        started_at_unix: 1,
        ended_at_unix: None,
        output: None,
        error: None,
        children: Vec::new(),
        cleanup: false,
    }];

    let path = workspace.join(".zeroclaw").join("subagents.json");
    std::fs::write(&path, serde_json::to_string_pretty(&runs).unwrap()).unwrap();

    let mgr = SwarmManager::new(workspace.clone(), 1);
    let _ = mgr.list().await;

    let list = mgr.list().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].run_id, run_id);
    assert_eq!(list[0].status, RunStatus::Terminated);
    assert!(list[0].error.as_deref().unwrap_or("").contains("restart"));
}
