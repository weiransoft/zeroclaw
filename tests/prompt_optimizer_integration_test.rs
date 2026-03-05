//! Integration tests for Prompt Optimizer
//!
//! Tests the integration of prompt optimizer with the agent system.

use tempfile::TempDir;

fn create_test_workspace() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("SOUL.md"), "# Soul\nBe helpful.").unwrap();
    std::fs::write(tmp.path().join("IDENTITY.md"), "# Identity\nName: TestAgent").unwrap();
    tmp
}

mod tests {
    use super::*;
    use zeroclaw::prompt_optimizer::{PromptOptimizer, PromptOptimizerConfig, TaskType, CompressionLevel, PromptComponent};

    #[test]
    fn test_optimizer_quick_task() {
        let optimizer = PromptOptimizer::default();
        let workspace = create_test_workspace();
        
        let tools: Vec<(&str, &str)> = vec![
            ("shell", "Execute commands"),
            ("file_read", "Read files"),
        ];
        
        let result = optimizer.build_optimized_system_prompt(
            workspace.path(),
            "test-model",
            &tools,
            &[],
            None,
            "what is 2+2",
        );
        
        assert_eq!(result.task_type, TaskType::Quick);
        assert!(result.compression_ratio >= 0.0);
        assert!(result.components_included.contains(&PromptComponent::Task));
        assert!(result.components_included.contains(&PromptComponent::Safety));
    }
    
    #[test]
    fn test_optimizer_complex_task() {
        let optimizer = PromptOptimizer::default();
        let workspace = create_test_workspace();
        
        let tools: Vec<(&str, &str)> = vec![
            ("shell", "Execute commands"),
            ("file_write", "Write files"),
        ];
        
        let result = optimizer.build_optimized_system_prompt(
            workspace.path(),
            "test-model",
            &tools,
            &[],
            None,
            "design and implement a complete authentication system with OAuth2 support",
        );
        
        assert_eq!(result.task_type, TaskType::Complex);
        assert!(result.components_included.contains(&PromptComponent::Task));
    }
    
    #[test]
    fn test_optimizer_with_soul() {
        let optimizer = PromptOptimizer::default();
        let workspace = create_test_workspace();
        
        let soul = zeroclaw::soul::Soul::from_preset(zeroclaw::soul::SoulPreset::Clara);
        
        let tools: Vec<(&str, &str)> = vec![];
        
        let result = optimizer.build_optimized_system_prompt(
            workspace.path(),
            "test-model",
            &tools,
            &[],
            Some(&soul),
            "let's talk about philosophy",
        );
        
        assert_eq!(result.task_type, TaskType::Conversation);
        assert!(result.components_included.contains(&PromptComponent::Soul) 
            || result.components_included.contains(&PromptComponent::Identity));
    }
    
    #[test]
    fn test_compression_levels() {
        let optimizer = PromptOptimizer::default();
        
        assert_eq!(optimizer.get_compression_level(TaskType::Quick), CompressionLevel::Aggressive);
        assert_eq!(optimizer.get_compression_level(TaskType::Simple), CompressionLevel::Aggressive);
        assert_eq!(optimizer.get_compression_level(TaskType::Standard), CompressionLevel::Moderate);
        assert_eq!(optimizer.get_compression_level(TaskType::Complex), CompressionLevel::Light);
        assert_eq!(optimizer.get_compression_level(TaskType::Technical), CompressionLevel::Light);
        assert_eq!(optimizer.get_compression_level(TaskType::Creative), CompressionLevel::Moderate);
        assert_eq!(optimizer.get_compression_level(TaskType::Conversation), CompressionLevel::Moderate);
        assert_eq!(optimizer.get_compression_level(TaskType::Orchestrator), CompressionLevel::Minimal);
    }
    
    #[test]
    fn test_soul_inclusion_logic() {
        let optimizer = PromptOptimizer::default();
        
        // 只有 Conversation 和 Creative 才注入完整人格
        assert!(!optimizer.should_include_soul(TaskType::Quick));
        assert!(!optimizer.should_include_soul(TaskType::Simple));
        assert!(!optimizer.should_include_soul(TaskType::Standard));
        assert!(!optimizer.should_include_soul(TaskType::Complex));
        assert!(optimizer.should_include_soul(TaskType::Creative));
        assert!(optimizer.should_include_soul(TaskType::Conversation));
        assert!(!optimizer.should_include_soul(TaskType::Technical));
        assert!(!optimizer.should_include_soul(TaskType::Orchestrator));
    }
    
    #[test]
    fn test_identity_inclusion_logic() {
        let optimizer = PromptOptimizer::default();
        
        // Complex/Technical/Orchestrator 注入简短身份
        assert!(!optimizer.should_include_identity(TaskType::Quick));
        assert!(!optimizer.should_include_identity(TaskType::Simple));
        assert!(!optimizer.should_include_identity(TaskType::Standard));
        assert!(optimizer.should_include_identity(TaskType::Complex));
        assert!(!optimizer.should_include_identity(TaskType::Creative));
        assert!(!optimizer.should_include_identity(TaskType::Conversation));
        assert!(optimizer.should_include_identity(TaskType::Technical));
        assert!(optimizer.should_include_identity(TaskType::Orchestrator));
    }
    
    #[test]
    fn test_memory_inclusion_logic() {
        let optimizer = PromptOptimizer::default();
        
        assert!(!optimizer.should_include_memory(TaskType::Quick));
        assert!(!optimizer.should_include_memory(TaskType::Simple));
        assert!(optimizer.should_include_memory(TaskType::Conversation));
        assert!(optimizer.should_include_memory(TaskType::Complex));
        assert!(optimizer.should_include_memory(TaskType::Technical));
    }
    
    #[test]
    fn test_experience_inclusion_logic() {
        let optimizer = PromptOptimizer::default();
        
        assert!(!optimizer.should_include_experience(TaskType::Quick));
        assert!(!optimizer.should_include_experience(TaskType::Simple));
        assert!(!optimizer.should_include_experience(TaskType::Conversation));
        assert!(optimizer.should_include_experience(TaskType::Complex));
        assert!(optimizer.should_include_experience(TaskType::Technical));
    }
    
    #[test]
    fn test_prompt_length_limit() {
        let config = PromptOptimizerConfig {
            enable_compression: true,
            max_system_prompt_chars: 500,
            prefer_concise: true,
        };
        let optimizer = PromptOptimizer::new(config);
        let workspace = create_test_workspace();
        
        let tools: Vec<(&str, &str)> = vec![
            ("shell", "Execute terminal commands. Use when: running local checks, build/test commands, diagnostics."),
            ("file_read", "Read file contents. Use when: inspecting project files, configs, logs."),
            ("file_write", "Write file contents. Use when: applying focused edits, scaffolding files."),
            ("memory_store", "Save to memory. Use when: preserving durable preferences, decisions."),
            ("memory_recall", "Search memory. Use when: retrieving prior decisions, user preferences."),
        ];
        
        let result = optimizer.build_optimized_system_prompt(
            workspace.path(),
            "test-model",
            &tools,
            &[],
            None,
            "read the file",
        );
        
        assert!(result.system_prompt.len() <= 600);
    }
    
    #[test]
    fn test_chinese_task_analysis() {
        let optimizer = PromptOptimizer::default();
        
        assert_eq!(optimizer.analyze_task("什么是人工智能", &[]), TaskType::Quick);
        assert_eq!(optimizer.analyze_task("设计并实现一个完整的认证系统", &[]), TaskType::Complex);
        assert_eq!(optimizer.analyze_task("聊聊人生", &[]), TaskType::Conversation);
    }
    
    #[test]
    fn test_system_prompt_contains_essential_sections() {
        let optimizer = PromptOptimizer::default();
        let workspace = create_test_workspace();
        
        let tools: Vec<(&str, &str)> = vec![
            ("shell", "Execute commands"),
        ];
        
        let result = optimizer.build_optimized_system_prompt(
            workspace.path(),
            "test-model",
            &tools,
            &[],
            None,
            "hello world",
        );
        
        let prompt = result.system_prompt.as_str();
        assert!(prompt.contains("## Task") || prompt.contains("Task"));
        assert!(prompt.contains("## Safety") || prompt.contains("Safety"));
    }
}
