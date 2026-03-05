use std::collections::HashMap;
use std::sync::Arc;

use tempfile::TempDir;
use zeroclaw::config::{Config, DelegateAgentConfig, SwarmConfig};
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::manager_for_workspace;
use zeroclaw::tools::{GroupChatTool, SubagentsTool, Tool};

#[tokio::test]
async fn test_group_chat_communication() {
    println!("=== Test Group Chat Communication ===");
    
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
    
    let security = Arc::new(SecurityPolicy::default());
    let config = Arc::new(cfg);
    
    let chat_tool = GroupChatTool::new(
        security.clone(),
        config.clone(),
        zeroclaw::swarm::SwarmContext::root(),
    );
    
    println!("\n--- Testing group_chat send action ---");
    let send_result = chat_tool.execute(serde_json::json!({
        "action": "send",
        "message_type": "status",
        "content": "Test message from orchestrator",
        "task_id": "test-task-001"
    })).await.unwrap();
    
    println!("Send result: success={}, output={}", send_result.success, send_result.output);
    assert!(send_result.success, "Send should succeed");
    assert!(send_result.output.contains("Message sent successfully"), "Should confirm message sent");
    
    println!("\n--- Testing group_chat read action ---");
    let read_result = chat_tool.execute(serde_json::json!({
        "action": "read",
        "limit": 10
    })).await.unwrap();
    
    println!("Read result: success={}, output={}", read_result.success, read_result.output);
    assert!(read_result.success, "Read should succeed");
    assert!(read_result.output.contains("Test message from orchestrator"), "Should contain sent message");
    
    println!("\n--- Testing group_chat subscribe action ---");
    let subscribe_result = chat_tool.execute(serde_json::json!({
        "action": "subscribe",
        "since": 0
    })).await.unwrap();
    
    println!("Subscribe result: success={}", subscribe_result.success);
    assert!(subscribe_result.success, "Subscribe should succeed");
    
    let subscribe_data: serde_json::Value = serde_json::from_str(&subscribe_result.output).unwrap();
    assert!(subscribe_data["has_new"].as_bool().unwrap(), "Should have new messages");
    
    println!("\n=== Group Chat Test Completed Successfully ===");
}

#[tokio::test]
async fn test_non_blocking_task_coordination() {
    println!("=== Test Non-Blocking Task Coordination ===");
    
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
        "worker".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a worker agent. Complete your task and report progress using group_chat tool.

When you start: Send a 'status' message saying you started.
When you make progress: Send 'progress' messages.
When you finish: Send a 'result' message with your output.

Example:
<tool>
{\"name\": \"group_chat\", \"arguments\": {\"action\": \"send\", \"message_type\": \"status\", \"content\": \"Starting task execution\"}}
</tool>".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 5,
            soul_preset: None,
        },
    );
    
    agents.insert(
        "coordinator".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a coordinator. Your job is to:
1. Spawn a worker agent using sessions_spawn tool
2. Use 'poll' action to check status WITHOUT waiting
3. Use 'check_all' to see overall progress
4. Use group_chat to communicate with the worker
5. Do NOT use 'wait' action - use non-blocking 'poll' instead

Example workflow:
1. Spawn worker: sessions_spawn with agent='worker', task='Say hello'
2. Poll status: subagents with action='poll', run_id=<id>
3. Check all: subagents with action='check_all'
4. Read messages: group_chat with action='read'

IMPORTANT: Never use 'wait' - always use 'poll' for non-blocking checks.".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 5,
            soul_preset: None,
        },
    );
    
    cfg.agents = agents;
    
    let config = Arc::new(cfg);
    let security = Arc::new(SecurityPolicy::default());
    
    let mgr = manager_for_workspace(&config.workspace_dir, config.swarm.subagent_max_concurrent).unwrap();
    
    let subagents_tool = SubagentsTool::new(
        security.clone(),
        config.clone(),
        mgr.clone(),
        zeroclaw::swarm::SwarmContext::root(),
    );
    
    println!("\n--- Testing check_all action (no sub-agents yet) ---");
    let check_result = subagents_tool.execute(serde_json::json!({
        "action": "check_all"
    })).await.unwrap();
    
    println!("Check all result: {}", check_result.output);
    assert!(check_result.success, "check_all should succeed");
    
    println!("\n--- Testing list action ---");
    let list_result = subagents_tool.execute(serde_json::json!({
        "action": "list"
    })).await.unwrap();
    
    println!("List result: {}", list_result.output);
    assert!(list_result.success, "list should succeed");
    
    println!("\n=== Non-Blocking Task Coordination Test Completed ===");
}
