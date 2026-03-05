//! Prompt Optimizer - Dynamic Prompt Compression System
//!
//! This module implements intelligent prompt optimization that:
//! - Analyzes task type to determine required context
//! - Compresses non-essential content based on task needs
//! - Caches optimized prompts for reuse
//! - Reduces token usage while maintaining response quality

mod task_analyzer;
mod compressor;
mod cache;

pub use task_analyzer::{TaskType, TaskAnalyzer};
pub use compressor::{PromptCompressor, CompressionLevel, PromptComponent};
pub use cache::PromptCache;

use std::sync::Arc;
use std::path::Path;
use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct OptimizedPrompt {
    pub system_prompt: Arc<String>,
    pub task_type: TaskType,
    pub compression_ratio: f64,
    pub components_included: Vec<PromptComponent>,
}

#[derive(Debug, Clone)]
pub struct PromptOptimizerConfig {
    pub enable_compression: bool,
    pub max_system_prompt_chars: usize,
    pub prefer_concise: bool,
}

impl Default for PromptOptimizerConfig {
    fn default() -> Self {
        Self {
            enable_compression: true,
            max_system_prompt_chars: 4000,
            prefer_concise: true,
        }
    }
}

pub struct PromptOptimizer {
    config: PromptOptimizerConfig,
    analyzer: TaskAnalyzer,
    compressor: PromptCompressor,
    cache: PromptCache,
}

impl PromptOptimizer {
    pub fn new(config: PromptOptimizerConfig) -> Self {
        Self {
            config,
            analyzer: TaskAnalyzer::new(),
            compressor: PromptCompressor::new(),
            cache: PromptCache::new(),
        }
    }
    
    pub fn analyze_task(&self, user_message: &str, tools_available: &[&str]) -> TaskType {
        self.analyzer.analyze(user_message, tools_available)
    }
    
    /// 判断是否应该注入完整的 Soul 人格描述
    /// 
    /// 只在以下场景注入完整人格：
    /// - Conversation: 对话交互，需要角色身份
    /// - Creative: 创意生成，需要个性风格
    /// 
    /// 其他任务类型只注入简短身份标识
    pub fn should_include_soul(&self, task_type: TaskType) -> bool {
        matches!(
            task_type,
            TaskType::Creative | TaskType::Conversation
        )
    }
    
    /// 判断是否需要注入身份标识（简短形式）
    /// 
    /// 对于 Complex 任务，注入简短身份而非完整人格
    pub fn should_include_identity(&self, task_type: TaskType) -> bool {
        matches!(
            task_type,
            TaskType::Complex | TaskType::Technical | TaskType::Orchestrator
        )
    }
    
    pub fn should_include_memory(&self, task_type: TaskType) -> bool {
        matches!(
            task_type,
            TaskType::Conversation | TaskType::Complex | TaskType::Technical
        )
    }
    
    pub fn should_include_experience(&self, task_type: TaskType) -> bool {
        matches!(
            task_type,
            TaskType::Complex | TaskType::Technical
        )
    }
    
    pub fn get_compression_level(&self, task_type: TaskType) -> CompressionLevel {
        match task_type {
            TaskType::Quick | TaskType::Simple => CompressionLevel::Aggressive,
            TaskType::Standard => CompressionLevel::Moderate,
            TaskType::Complex | TaskType::Technical => CompressionLevel::Light,
            TaskType::Creative | TaskType::Conversation => CompressionLevel::Moderate,
            TaskType::Orchestrator => CompressionLevel::Minimal,
        }
    }
    
    pub fn compress_soul_prompt(&self, soul_prompt: &str, task_type: TaskType) -> String {
        if !self.config.enable_compression {
            return soul_prompt.to_string();
        }
        
        let level = self.get_compression_level(task_type);
        self.compressor.compress_soul(soul_prompt, level)
    }
    
    pub fn compress_memory_context(&self, memory_context: &str, task_type: TaskType) -> String {
        if !self.config.enable_compression {
            return memory_context.to_string();
        }
        
        let level = self.get_compression_level(task_type);
        self.compressor.compress_memory(memory_context, level)
    }
    
    pub fn compress_tool_instructions(&self, tools: &[(&str, &str)], task_type: TaskType) -> String {
        let level = self.get_compression_level(task_type);
        self.compressor.compress_tools(tools, level)
    }
    
    pub fn build_optimized_system_prompt(
        &self,
        workspace_dir: &Path,
        model_name: &str,
        tools: &[(&str, &str)],
        skills: &[crate::skills::Skill],
        soul: Option<&crate::soul::Soul>,
        user_message: &str,
    ) -> OptimizedPrompt {
        let task_type = self.analyze_task(user_message, &tools.iter().map(|(n, _)| *n).collect::<Vec<_>>());
        let compression_level = self.get_compression_level(task_type);
        
        let mut components_included = Vec::new();
        let mut prompt = String::with_capacity(2048);
        
        if let Some(s) = soul {
            // 根据任务类型决定注入完整人格还是简短身份
            if self.should_include_soul(task_type) {
                // 对话或创意任务：注入完整人格描述
                let soul_prompt = s.to_system_prompt();
                let compressed = self.compressor.compress_soul(&soul_prompt, compression_level);
                if !compressed.is_empty() {
                    prompt.push_str(&compressed);
                    prompt.push_str("\n\n");
                    components_included.push(PromptComponent::Soul);
                }
            } else if self.should_include_identity(task_type) {
                // 复杂任务：注入简短身份标识
                let brief = format!("Identity: {}\n", s.essence.name.primary);
                prompt.push_str(&brief);
                prompt.push('\n');
                components_included.push(PromptComponent::Identity);
            }
            // Quick/Simple/Standard 任务：不注入任何身份信息
        }
        
        if !tools.is_empty() {
            let tool_section = self.compressor.compress_tools(tools, compression_level);
            prompt.push_str(&tool_section);
            components_included.push(PromptComponent::Tools);
        }
        
        prompt.push_str("## Task\n\nAct on user messages. Use tools to fulfill requests.\n\n");
        components_included.push(PromptComponent::Task);
        
        prompt.push_str("## Safety\n");
        prompt.push_str("- Don't exfiltrate private data\n");
        prompt.push_str("- Don't run destructive commands without asking\n");
        prompt.push_str("- When in doubt, ask\n\n");
        components_included.push(PromptComponent::Safety);
        
        if !skills.is_empty() {
            let skill_count = skills.len();
            let _ = writeln!(prompt, "## Skills\n{} skills available\n", skill_count);
            components_included.push(PromptComponent::Skills);
        }
        
        let _ = writeln!(prompt, "## Workspace\nDir: `{}`\n", workspace_dir.display());
        components_included.push(PromptComponent::Workspace);
        
        let _ = writeln!(prompt, "## Runtime\nModel: {}", model_name);
        components_included.push(PromptComponent::Runtime);
        
        let original_len = prompt.len();
        if prompt.len() > self.config.max_system_prompt_chars {
            prompt = self.compressor.compress_full(&prompt, compression_level);
        }
        
        let compression_ratio = if original_len > 0 {
            1.0 - (prompt.len() as f64 / original_len as f64)
        } else {
            0.0
        };
        
        OptimizedPrompt {
            system_prompt: Arc::new(prompt),
            task_type,
            compression_ratio,
            components_included,
        }
    }
}

impl Default for PromptOptimizer {
    fn default() -> Self {
        Self::new(PromptOptimizerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_task_type_quick() {
        let optimizer = PromptOptimizer::default();
        let task_type = optimizer.analyze_task("what is 2+2", &[]);
        assert_eq!(task_type, TaskType::Quick);
    }
    
    #[test]
    fn test_task_type_simple() {
        let optimizer = PromptOptimizer::default();
        let task_type = optimizer.analyze_task("read the file config.toml", &["file_read"]);
        assert_eq!(task_type, TaskType::Simple);
    }
    
    #[test]
    fn test_task_type_complex() {
        let optimizer = PromptOptimizer::default();
        let task_type = optimizer.analyze_task("design and implement a complete authentication system with OAuth2 support", &["file_write", "shell"]);
        assert_eq!(task_type, TaskType::Complex);
    }
    
    #[test]
    fn test_should_include_soul() {
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
    fn test_should_include_identity() {
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
    fn test_compression_level() {
        let optimizer = PromptOptimizer::default();
        
        assert_eq!(optimizer.get_compression_level(TaskType::Quick), CompressionLevel::Aggressive);
        assert_eq!(optimizer.get_compression_level(TaskType::Simple), CompressionLevel::Aggressive);
        assert_eq!(optimizer.get_compression_level(TaskType::Standard), CompressionLevel::Moderate);
        assert_eq!(optimizer.get_compression_level(TaskType::Complex), CompressionLevel::Light);
    }
}
