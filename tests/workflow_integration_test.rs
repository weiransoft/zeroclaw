use zeroclaw::config::{Config, DelegateAgentConfig};
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{SwarmContext, SwarmManager};
use zeroclaw::tools::{GroupChatTool, SubagentsTool, WorkflowTool, Tool};
use std::sync::Arc;

#[tokio::test]
async fn test_workflow_tool_basic() {
    // 创建配置
    let mut cfg = Config::default();
    cfg.api_key = Some("test-key".to_string());
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    
    // 添加Scrum团队角色
    let product_owner = DelegateAgentConfig {
        provider: "glm".to_string(),
        model: "glm-5".to_string(),
        system_prompt: Some("You are a product owner".to_string()),
        api_key: None,
        temperature: Some(0.7),
        max_depth: 3,
            soul_preset: None,
    };
    
    let scrum_master = DelegateAgentConfig {
        provider: "glm".to_string(),
        model: "glm-5".to_string(),
        system_prompt: Some("You are a scrum master".to_string()),
        api_key: None,
        temperature: Some(0.7),
        max_depth: 3,
            soul_preset: None,
    };
    
    cfg.agents.insert("product_owner".to_string(), product_owner);
    cfg.agents.insert("scrum_master".to_string(), scrum_master);
    
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
}

#[tokio::test]
async fn test_workflow_integration() {
    // 创建配置
    let mut cfg = Config::default();
    cfg.api_key = Some("test-key".to_string());
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    
    // 添加Scrum团队角色
    let product_owner = DelegateAgentConfig {
        provider: "glm".to_string(),
        model: "glm-5".to_string(),
        system_prompt: Some("You are a product owner".to_string()),
        api_key: None,
        temperature: Some(0.7),
        max_depth: 3,
            soul_preset: None,
    };
    
    let scrum_master = DelegateAgentConfig {
        provider: "glm".to_string(),
        model: "glm-5".to_string(),
        system_prompt: Some("You are a scrum master".to_string()),
        api_key: None,
        temperature: Some(0.7),
        max_depth: 3,
            soul_preset: None,
    };
    
    let architect = DelegateAgentConfig {
        provider: "glm".to_string(),
        model: "glm-5".to_string(),
        system_prompt: Some("You are a solution architect".to_string()),
        api_key: None,
        temperature: Some(0.7),
        max_depth: 3,
            soul_preset: None,
    };
    
    cfg.agents.insert("product_owner".to_string(), product_owner);
    cfg.agents.insert("scrum_master".to_string(), scrum_master);
    cfg.agents.insert("architect".to_string(), architect);
    
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
    
    // 创建群聊工具
    let group_chat_tool = GroupChatTool::new(
        security.clone(),
        cfg_arc.clone(),
        ctx.clone(),
    );
    
    // 创建子任务工具
    let _subagents_tool = SubagentsTool::new(
        security.clone(),
        cfg_arc.clone(),
        swarm_manager.clone(),
        ctx.clone(),
    );
    
    // 测试完整工作流
    println!("=== Testing Complete Workflow ===");
    
    // 1. 创建工作流
    let create_args = serde_json::json!({
        "action": "create",
        "workflow_name": "Scrum Sprint Workflow",
        "description": "Complete Scrum workflow for software development",
        "roles": ["product_owner", "scrum_master", "architect"],
        "steps": [
            {
                "name": "Product Requirement Analysis",
                "description": "Analyze and document product requirements",
                "assigned_to": "product_owner",
                "dependencies": []
            },
            {
                "name": "Architecture Design",
                "description": "Design system architecture",
                "assigned_to": "architect",
                "dependencies": ["Product Requirement Analysis"]
            },
            {
                "name": "Sprint Planning",
                "description": "Plan sprint backlog",
                "assigned_to": "scrum_master",
                "dependencies": ["Architecture Design"]
            }
        ]
    });
    
    let create_result = workflow_tool.execute(create_args).await.unwrap();
    println!("1. Workflow created: {}", create_result.output);
    assert!(create_result.success);
    
    // 2. 模拟群聊通信
    let chat_args = serde_json::json!({
        "action": "send",
        "message_type": "status",
        "content": "Starting Scrum workflow test",
        "task_id": "workflow-test"
    });
    
    let chat_result = group_chat_tool.execute(chat_args).await.unwrap();
    println!("2. Chat message sent: {}", chat_result.output);
    assert!(chat_result.success);
    
    // 3. 读取群聊消息
    let read_args = serde_json::json!({
        "action": "read",
        "limit": 10
    });
    
    let read_result = group_chat_tool.execute(read_args).await.unwrap();
    println!("3. Chat messages: {}", read_result.output);
    assert!(read_result.success);
    
    // 4. 测试工作流状态
    let status_args = serde_json::json!({
        "action": "status",
        "workflow_id": "test-workflow"
    });
    
    let status_result = workflow_tool.execute(status_args).await.unwrap();
    println!("4. Workflow status: {}", status_result.output);
    assert!(status_result.success);
    
    println!("=== Workflow Integration Test Complete ===");
}

#[tokio::test]
async fn test_workflow_with_real_roles() {
    // 创建配置
    let mut cfg = Config::default();
    cfg.api_key = Some("test-key".to_string());
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    
    // 添加完整的Scrum团队角色
    let roles = vec![
        ("product_owner", "You are a product owner responsible for defining and prioritizing product requirements"),
        ("scrum_master", "You are a Scrum Master responsible for facilitating the Scrum process"),
        ("architect", "You are a solution architect responsible for designing the technical architecture"),
        ("frontend_developer", "You are a frontend developer responsible for building the user interface"),
        ("backend_developer", "You are a backend developer responsible for building the server-side functionality"),
        ("qa_engineer", "You are a QA engineer responsible for ensuring the quality of the product"),
    ];
    
    for (name, prompt) in roles {
        let agent_config = DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some(prompt.to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 3,
            soul_preset: None,
        };
        cfg.agents.insert(name.to_string(), agent_config);
    }
    
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
    
    // 测试自动生成工作流
    let auto_generate_args = serde_json::json!({
        "action": "auto_generate",
        "description": "Build an e-commerce website with product catalog, shopping cart, and payment integration"
    });
    
    let auto_generate_result = workflow_tool.execute(auto_generate_args).await.unwrap();
    println!("Auto generated workflow: {}", auto_generate_result.output);
    assert!(auto_generate_result.success);
    
    // 测试工作流调整
    let adjust_args = serde_json::json!({
        "action": "adjust",
        "workflow_id": "test-workflow",
        "adjustments": {
            "name": "E-commerce Development Workflow",
            "steps": [
                {
                    "name": "Product Requirements",
                    "description": "Analyze e-commerce requirements",
                    "assigned_to": "product_owner",
                    "dependencies": []
                },
                {
                    "name": "System Architecture",
                    "description": "Design e-commerce architecture",
                    "assigned_to": "architect",
                    "dependencies": ["Product Requirements"]
                },
                {
                    "name": "Frontend Development",
                    "description": "Build e-commerce frontend",
                    "assigned_to": "frontend_developer",
                    "dependencies": ["System Architecture"]
                },
                {
                    "name": "Backend Development",
                    "description": "Build e-commerce backend",
                    "assigned_to": "backend_developer",
                    "dependencies": ["System Architecture"]
                },
                {
                    "name": "Testing",
                    "description": "Test e-commerce functionality",
                    "assigned_to": "qa_engineer",
                    "dependencies": ["Frontend Development", "Backend Development"]
                }
            ]
        }
    });
    
    let adjust_result = workflow_tool.execute(adjust_args).await.unwrap();
    println!("Workflow adjusted: {}", adjust_result.output);
    assert!(adjust_result.success);
}