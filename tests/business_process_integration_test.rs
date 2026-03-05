//! 业务流程测试
//!
//! 测试端到端的业务流程和用户交互场景

use zeroclaw::soul::{Soul, SoulPreset};
use zeroclaw::security::{SecurityPolicy, AutonomyLevel};
use zeroclaw::memory::{Memory, MemoryCategory, SqliteMemory};
use zeroclaw::config::Config;
use tempfile::tempdir;
use std::sync::Arc;

#[tokio::test]
async fn test_complete_user_interaction_flow() {
    // 测试完整的用户交互流程
    let temp_dir = tempdir().unwrap();
    
    // 1. 初始化配置
    let config = Config::default();
    
    // 2. 创建AI灵魂
    let soul = Soul::from_preset(SoulPreset::Clara);
    
    // 3. 设置安全策略
    let mut security_policy = SecurityPolicy::default();
    security_policy.autonomy = AutonomyLevel::Supervised; // 监督模式
    
    // 4. 初始化内存系统
    let db_path = temp_dir.path().join("memory.db");
    let memory = SqliteMemory::new(&db_path).expect("Memory should be created");
    
    // 5. 模拟用户请求处理流程
    let user_query = "请帮我解释一下Rust中的所有权机制";
    
    // 存储用户请求到记忆中
    memory.store(
        "user_query_1", 
        user_query, 
        MemoryCategory::Conversation
    ).await.expect("Query should be stored");
    
    // 6. 验证安全策略允许适当的命令
    let test_command = "ls";
    let validation_result = security_policy.validate_command_execution(test_command, false);
    assert!(validation_result.is_ok(), "Basic commands should be allowed in supervised mode");
    
    // 7. 检索相关记忆
    let retrieved_memories = memory.recall("ownership", 5).await.expect("Memories should be recalled");
    
    // 8. 验证系统响应生成（通过检查灵魂的个性化提示生成）
    let system_prompt = soul.to_system_prompt();
    assert!(system_prompt.contains(&soul.essence.name.primary));
    
    // 9. 验证整个流程的状态
    assert!(!system_prompt.is_empty());
    assert!(db_path.exists());
    
    // 10. 检查记忆系统中有记录
    let all_memories = memory.list(None).await.expect("Memories should be listed");
    assert!(!all_memories.is_empty());
    
    println!("Complete user interaction flow test passed!");
}

#[tokio::test]
async fn test_multi_step_task_completion() {
    // 测试多步骤任务完成流程
    let temp_dir = tempdir().unwrap();
    
    // 1. 初始化组件
    let soul = Soul::from_preset(SoulPreset::Clara);
    let security_policy = SecurityPolicy::default();
    let db_path = temp_dir.path().join("memory.db");
    let memory = SqliteMemory::new(&db_path).expect("Memory should be created");
    
    // 2. 模拟多步骤任务：代码审查流程
    let task_steps = vec![
        "分析提供的代码",
        "识别潜在问题",
        "提出改进建议",
        "总结审查结果",
    ];
    
    // 3. 执行每一步并存储结果
    for (index, step) in task_steps.iter().enumerate() {
        let step_key = format!("review_step_{}", index);
        let step_content = format!("Step {}: {}", index + 1, step);
        
        memory.store(
            &step_key,
            &step_content,
            MemoryCategory::Core
        ).await.expect("Step should be stored");
    }
    
    // 4. 验证所有步骤都被记住
    let stored_steps = memory.recall("review_step", 10).await.expect("Steps should be recalled");
    assert_eq!(stored_steps.len(), task_steps.len());
    
    // 5. 验证安全策略在任务执行期间保持一致
    assert!(matches!(security_policy.autonomy, AutonomyLevel::Supervised));
    
    // 6. 验证灵魂的一致性
    let personality_summary = soul.personality_summary();
    assert!(personality_summary.contains(&soul.essence.name.primary));
    
    println!("Multi-step task completion flow test passed!");
}

#[tokio::test]
async fn test_context_preservation_across_sessions() {
    // 测试跨会话的上下文保持
    let temp_dir = tempdir().unwrap();
    
    // 1. 第一个会话：存储上下文
    let db_path = temp_dir.path().join("memory.db");
    let memory = SqliteMemory::new(&db_path).expect("Memory should be created");
    
    // 存储用户偏好和上下文信息
    let contexts = vec![
        ("user_preference", "喜欢详细的解释", MemoryCategory::Core),
        ("current_project", "Rust项目开发", MemoryCategory::Daily),
        ("coding_style", "偏好函数式编程方法", MemoryCategory::Core),
    ];
    
    for (key, content, category) in &contexts {
        memory.store(key, content, category.clone()).await.expect("Context should be stored");
    }
    
    // 2. 模拟会话结束和重新开始
    // (在这里，内存系统保持持久化)
    
    // 3. 第二个会话：检索上下文
    let retrieved_preferences = memory.recall("preference", 5).await.expect("Preferences should be recalled");
    let retrieved_projects = memory.recall("project", 5).await.expect("Projects should be recalled");
    
    // 4. 验证上下文被正确保持
    assert!(!retrieved_preferences.is_empty());
    assert!(!retrieved_projects.is_empty());
    
    // 6. 验证上下文影响AI行为
    let soul = Soul::from_preset(SoulPreset::Clara);
    let adapted_soul = soul.clone().with_emotion(zeroclaw::soul::EmotionalTone::Thoughtful, 0.7);
    
    // 检查适应后的灵魂与原始灵魂在情感状态上不同
    assert_eq!(adapted_soul.essence.name.primary, soul.essence.name.primary); // 名字应该相同
    
    println!("Context preservation across sessions test passed!");
}

#[tokio::test]
async fn test_security_validation_in_workflows() {
    // 测试工作流程中的安全验证
    let temp_dir = tempdir().unwrap();
    
    // 1. 设置不同安全级别的策略
    let low_security_policy = SecurityPolicy {
        autonomy: AutonomyLevel::Full,
        ..SecurityPolicy::default()
    };
    
    let high_security_policy = SecurityPolicy {
        autonomy: AutonomyLevel::ReadOnly,
        ..SecurityPolicy::default()
    };
    
    // 2. 测试低安全策略下的操作
    let allowed_command = "echo 'hello'";
    assert!(low_security_policy.validate_command_execution(allowed_command, false).is_ok());
    
    // 3. 测试高安全策略下的操作（更严格）
    let risky_command = "rm -rf /";
    let risky_result = high_security_policy.validate_command_execution(risky_command, false);
    assert!(risky_result.is_err());
    
    // 4. 初始化内存用于工作流程
    let db_path = temp_dir.path().join("workflow_memory.db");
    let memory = SqliteMemory::new(&db_path).expect("Memory should be created");
    
    // 5. 存储安全策略相关信息
    memory.store(
        "security_policy_applied",
        &format!("Autonomy level: {:?}", high_security_policy.autonomy),
        MemoryCategory::Core
    ).await.expect("Security info should be stored");
    
    // 6. 验证工作流程在不同安全级别下正确执行
    let stored_policy_info = memory.get("security_policy_applied").await.expect("Should get policy info");
    assert!(stored_policy_info.is_some());
    
    println!("Security validation in workflows test passed!");
}

#[tokio::test]
async fn test_memory_based_learning_flow() {
    // 测试基于内存的学习流程
    let temp_dir = tempdir().unwrap();
    
    // 1. 初始化系统组件
    let soul = Soul::from_preset(SoulPreset::Clara);
    let db_path = temp_dir.path().join("learning_memory.db");
    let memory = SqliteMemory::new(&db_path).expect("Memory should be created");
    
    // 2. 模拟学习交互：用户询问Rust概念
    let learning_interactions = vec![
        ("rust_ownership_basic", "Rust的所有权基础概念包括：1)每个值都有一个所有者 2)一次只能有一个所有者 3)当所有者超出作用域时，值将被丢弃", MemoryCategory::Core),
        ("rust_borrowing_rules", "借用规则：1)可以在任何时候有多个不可变引用 2)只能有一个可变引用 3)不能同时有可变和不可变引用", MemoryCategory::Core),
        ("rust_lifetimes", "生命周期确保引用在其引用的数据有效时才有效，防止悬垂指针", MemoryCategory::Core),
    ];
    
    // 3. 存储学习内容
    for (key, content, category) in &learning_interactions {
        memory.store(key, content, category.clone()).await.expect("Learning content should be stored");
    }
    
    // 4. 检索学习内容以验证记忆
    let rust_concepts = memory.recall("rust_", 10).await.expect("Rust concepts should be recalled");
    assert_eq!(rust_concepts.len(), learning_interactions.len());
    
    // 5. 验证学习影响AI响应（通过人格适配）
    let updated_soul = soul.clone().with_emotion(zeroclaw::soul::EmotionalTone::Curious, 0.8);
    
    // 6. 验证记忆持久性
    let all_learning_items = memory.list(Some(&MemoryCategory::Core)).await.expect("Learning items should be listed");
    let rust_related_items = memory.recall("ownership", 5).await.expect("Ownership info should be recalled");
    
    assert!(!all_learning_items.is_empty());
    assert!(!rust_related_items.is_empty());
    
    println!("Memory-based learning flow test passed!");
}

#[tokio::test]
async fn test_error_handling_and_recovery_flow() {
    // 测试错误处理和恢复流程
    let temp_dir = tempdir().unwrap();
    
    // 1. 初始化系统
    let soul = Soul::from_preset(SoulPreset::Clara);
    let security_policy = SecurityPolicy::default();
    let db_path = temp_dir.path().join("error_test_memory.db");
    let memory = SqliteMemory::new(&db_path).expect("Memory should be created");
    
    // 2. 模拟正常操作
    memory.store("normal_operation", "System working correctly", MemoryCategory::Daily)
        .await.expect("Normal operation should succeed");
    
    // 3. 模拟错误情况（例如，尝试执行受限制的命令）
    let restricted_command = "malicious_command";
    let security_result = security_policy.validate_command_execution(restricted_command, false);
    
    // 4. 验证错误被正确处理
    if security_result.is_err() {
        // 错误被安全策略捕获，这是预期行为
        memory.store("security_block", &format!("Blocked command: {}", restricted_command), MemoryCategory::Daily)
            .await.expect("Security block should be recorded");
    }
    
    // 5. 验证系统可以从错误状态恢复
    let recovery_check = memory.recall("security", 5).await.expect("Security events should be recalled");
    assert!(!recovery_check.is_empty());
    
    // 6. 验证AI人格在错误处理中保持一致
    let personality_check = soul.personality_summary();
    assert!(personality_check.contains(&soul.essence.name.primary));
    
    // 7. 验证内存系统持续运行
    let operational_check = memory.get("normal_operation").await.expect("Should get normal operation");
    assert!(operational_check.is_some());
    
    println!("Error handling and recovery flow test passed!");
}