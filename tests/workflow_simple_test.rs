use zeroclaw::config::Config;
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{SwarmContext, SwarmManager};
use zeroclaw::tools::{WorkflowTool, Tool};
use std::sync::Arc;

#[tokio::test]
async fn test_workflow_tool_simple() {
    // 创建配置
    let mut cfg = Config::default();
    cfg.api_key = Some("test-key".to_string());
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    
    let cfg_arc = Arc::new(cfg);
    let security = Arc::new(SecurityPolicy::default());
    let swarm_manager = SwarmManager::new(cfg_arc.workspace_dir.clone(), 5);
    let ctx = SwarmContext::root();
    
    // 创建工作流工具
    let workflow_tool = WorkflowTool::new(
        security.clone(),
        cfg_arc.clone(),
        swarm_manager.clone(),
        ctx.clone(),
    );
    
    // 测试工作流创建
    let create_args = serde_json::json!({
        "action": "create",
        "workflow_name": "Test Workflow",
        "description": "Test workflow for validation",
        "roles": ["product_owner", "scrum_master"],
        "steps": [
            {
                "name": "Requirement Analysis",
                "description": "Analyze product requirements",
                "assigned_to": "product_owner",
                "dependencies": []
            },
            {
                "name": "Sprint Planning",
                "description": "Plan sprint backlog",
                "assigned_to": "scrum_master",
                "dependencies": ["Requirement Analysis"]
            }
        ]
    });
    
    let create_result = workflow_tool.execute(create_args).await.unwrap();
    println!("Create workflow result: {}", create_result.output);
    assert!(create_result.success);
    
    // 测试工作流自动生成
    let auto_generate_args = serde_json::json!({
        "action": "auto_generate",
        "description": "Build a website with user authentication and product catalog"
    });
    
    let auto_generate_result = workflow_tool.execute(auto_generate_args).await.unwrap();
    println!("Auto generate workflow result: {}", auto_generate_result.output);
    assert!(auto_generate_result.success);
    
    println!("=== Workflow Tool Test PASSED ===");
}