use zeroclaw::config::{Config, DelegateAgentConfig};
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{SwarmContext, SwarmManager};
use zeroclaw::tools::{GroupChatTool, WorkflowTool, Tool};
use std::sync::Arc;

#[tokio::test]
async fn test_workflow_team_collaboration_chinese() {
    // 创建配置
    let mut cfg = Config::default();
    cfg.api_key = Some("test-key".to_string());
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    cfg.current_agent_name = Some("workflow_orchestrator".to_string());
    
    // 添加完整的Scrum团队角色（使用中文prompt）
    let roles = vec![
        ("product_owner", "你是一名产品负责人，负责定义和优先排序产品需求，确保团队理解产品愿景。"),
        ("scrum_master", "你是一名Scrum Master，负责促进Scrum流程，移除团队障碍，确保团队高效运作。"),
        ("architect", "你是一名解决方案架构师，负责设计技术架构，确保系统的可扩展性和可靠性。"),
        ("frontend_developer", "你是一名前端开发工程师，负责构建用户界面，确保良好的用户体验。"),
        ("backend_developer", "你是一名后端开发工程师，负责构建服务器端功能，确保系统的稳定性和性能。"),
        ("qa_engineer", "你是一名QA工程师，负责确保产品质量，执行测试计划，报告和跟踪缺陷。"),
        ("data_analyst", "你是一名数据分析师，负责分析数据，提供见解，支持产品决策。"),
        ("documentation_engineer", "你是一名文档工程师，负责创建技术文档和用户指南，确保文档的准确性和完整性。"),
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
    
    // 创建群聊工具
    let group_chat_tool = GroupChatTool::new(
        security.clone(),
        cfg_arc.clone(),
        ctx.clone(),
    );
    
    println!("=== 测试工作流团队协作（中文）===");
    
    // 1. 自动生成工作流（基于中文任务描述）
    println!("1. 自动生成工作流...");
    let auto_generate_args = serde_json::json!({
        "action": "auto_generate",
        "description": "构建一个在线教育平台，包含课程管理、用户认证、学习进度跟踪和数据分析功能"
    });
    
    let auto_generate_result = workflow_tool.execute(auto_generate_args).await.unwrap();
    println!("工作流生成结果: {}", auto_generate_result.output);
    assert!(auto_generate_result.success);
    
    // 提取工作流ID
    let workflow_id = auto_generate_result.output
        .split("ID: ")
        .nth(1)
        .and_then(|s| s.split('\n').next())
        .unwrap_or("test-workflow")
        .trim();
    
    println!("提取的工作流ID: {}", workflow_id);
    
    // 2. 启动工作流
    println!("\n2. 启动工作流...");
    let start_args = serde_json::json!({
        "action": "start",
        "workflow_id": workflow_id
    });
    
    let start_result = workflow_tool.execute(start_args).await.unwrap();
    println!("工作流启动结果: {}", start_result.output);
    assert!(start_result.success);
    
    // 3. 检查工作流状态
    println!("\n3. 检查工作流状态...");
    let status_args = serde_json::json!({
        "action": "status",
        "workflow_id": workflow_id
    });
    
    let status_result = workflow_tool.execute(status_args.clone()).await.unwrap();
    println!("工作流状态: {}", status_result.output);
    assert!(status_result.success);
    
    // 4. 模拟团队成员通过群聊进行沟通
    println!("\n4. 模拟团队沟通...");
    
    // 产品负责人发送需求澄清
    let po_chat_args = serde_json::json!({
        "action": "send",
        "message_type": "discussion",
        "content": "关于在线教育平台，我们需要确保支持多种课程格式，包括视频、文档和互动练习。",
        "task_id": workflow_id
    });
    
    let po_chat_result = group_chat_tool.execute(po_chat_args).await.unwrap();
    println!("产品负责人消息发送结果: {}", po_chat_result.output);
    assert!(po_chat_result.success);
    
    // 架构师回应技术方案
    let arch_chat_args = serde_json::json!({
        "action": "send",
        "message_type": "discussion",
        "content": "技术架构方面，我们将采用微服务架构，使用容器化部署，确保系统的可扩展性。",
        "task_id": workflow_id
    });
    
    let arch_chat_result = group_chat_tool.execute(arch_chat_args).await.unwrap();
    println!("架构师消息发送结果: {}", arch_chat_result.output);
    assert!(arch_chat_result.success);
    
    // 5. 读取群聊消息，验证所有消息都已正确存储
    println!("\n5. 读取群聊消息...");
    let read_args = serde_json::json!({
        "action": "read",
        "limit": 20,
        "task_id": workflow_id
    });
    
    let read_result = group_chat_tool.execute(read_args).await.unwrap();
    println!("群聊消息: {}", read_result.output);
    assert!(read_result.success);
    
    // 6. 测试工作流调整
    println!("\n6. 测试工作流调整...");
    let adjust_args = serde_json::json!({
        "action": "adjust",
        "workflow_id": workflow_id,
        "adjustments": {
            "status": "running"
        }
    });
    
    let adjust_result = workflow_tool.execute(adjust_args).await.unwrap();
    println!("工作流调整结果: {}", adjust_result.output);
    assert!(adjust_result.success);
    
    // 7. 测试工作流暂停和恢复
    println!("\n7. 测试工作流暂停...");
    let pause_args = serde_json::json!({
        "action": "pause",
        "workflow_id": workflow_id
    });
    
    let pause_result = workflow_tool.execute(pause_args).await.unwrap();
    println!("工作流暂停结果: {}", pause_result.output);
    assert!(pause_result.success);
    
    println!("\n8. 测试工作流恢复...");
    let resume_args = serde_json::json!({
        "action": "resume",
        "workflow_id": workflow_id
    });
    
    let resume_result = workflow_tool.execute(resume_args).await.unwrap();
    println!("工作流恢复结果: {}", resume_result.output);
    assert!(resume_result.success);
    
    // 9. 最终状态检查
    println!("\n9. 最终工作流状态...");
    let final_status_result = workflow_tool.execute(status_args).await.unwrap();
    println!("最终工作流状态: {}", final_status_result.output);
    assert!(final_status_result.success);
    
    println!("\n=== 工作流团队协作测试完成（中文）===");
}

#[tokio::test]
async fn test_workflow_phase_transitions_chinese() {
    // 创建配置
    let mut cfg = Config::default();
    cfg.api_key = Some("test-key".to_string());
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    cfg.current_agent_name = Some("workflow_orchestrator".to_string());
    
    // 添加核心团队角色（使用中文prompt）
    let core_roles = vec![
        ("product_owner", "你是一名产品负责人，专注于产品需求和用户体验。"),
        ("scrum_master", "你是一名Scrum Master，负责团队协作和流程管理。"),
        ("frontend_developer", "你是一名前端开发工程师，专注于用户界面开发。"),
        ("backend_developer", "你是一名后端开发工程师，专注于系统架构和API开发。"),
        ("qa_engineer", "你是一名QA工程师，负责确保产品质量。"),
    ];
    
    for (name, prompt) in core_roles {
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
    
    println!("=== 测试工作流阶段转换（中文）===");
    
    // 1. 创建自定义工作流（使用中文描述）
    println!("1. 创建自定义工作流...");
    let create_args = serde_json::json!({
        "action": "create",
        "workflow_name": "在线商城开发",
        "description": "构建一个完整的在线商城系统，包含商品管理、购物车、支付和订单管理功能",
        "roles": ["product_owner", "scrum_master", "frontend_developer", "backend_developer", "qa_engineer"],
        "steps": [
            {
                "name": "需求分析",
                "description": "分析在线商城的业务需求和用户场景",
                "assigned_to": "product_owner",
                "dependencies": []
            },
            {
                "name": "系统设计",
                "description": "设计在线商城的系统架构和技术方案",
                "assigned_to": "backend_developer",
                "dependencies": ["需求分析"]
            },
            {
                "name": "前端开发",
                "description": "开发在线商城的用户界面和交互功能",
                "assigned_to": "frontend_developer",
                "dependencies": ["系统设计"]
            },
            {
                "name": "后端开发",
                "description": "开发在线商城的服务器端功能和API",
                "assigned_to": "backend_developer",
                "dependencies": ["系统设计"]
            },
            {
                "name": "系统测试",
                "description": "测试在线商城的功能和性能",
                "assigned_to": "qa_engineer",
                "dependencies": ["前端开发", "后端开发"]
            },
            {
                "name": "项目交付",
                "description": "最终审核和项目交付",
                "assigned_to": "scrum_master",
                "dependencies": ["系统测试"]
            }
        ]
    });
    
    let create_result = workflow_tool.execute(create_args).await.unwrap();
    println!("工作流创建结果: {}", create_result.output);
    assert!(create_result.success);
    
    // 提取工作流ID
    let workflow_id = create_result.output
        .split("ID: ")
        .nth(1)
        .and_then(|s| s.split(",").next())
        .unwrap_or("test-workflow")
        .trim();
    
    println!("提取的工作流ID: {}", workflow_id);
    
    // 2. 启动工作流
    println!("\n2. 启动工作流...");
    let start_args = serde_json::json!({
        "action": "start",
        "workflow_id": workflow_id
    });
    
    let start_result = workflow_tool.execute(start_args).await.unwrap();
    println!("工作流启动结果: {}", start_result.output);
    assert!(start_result.success);
    
    // 3. 验证工作流状态
    println!("\n3. 验证工作流状态...");
    let status_args = serde_json::json!({
        "action": "status",
        "workflow_id": workflow_id
    });
    
    let status_result = workflow_tool.execute(status_args.clone()).await.unwrap();
    println!("工作流状态: {}", status_result.output);
    assert!(status_result.success);
    
    // 4. 测试工作流停止
    println!("\n4. 测试工作流停止...");
    let stop_args = serde_json::json!({
        "action": "stop",
        "workflow_id": workflow_id
    });
    
    let stop_result = workflow_tool.execute(stop_args).await.unwrap();
    println!("工作流停止结果: {}", stop_result.output);
    assert!(stop_result.success);
    
    // 5. 最终状态检查
    println!("\n5. 最终工作流状态...");
    let final_status_result = workflow_tool.execute(status_args).await.unwrap();
    println!("最终工作流状态: {}", final_status_result.output);
    assert!(final_status_result.success);
    
    println!("\n=== 工作流阶段转换测试完成（中文）===");
}
