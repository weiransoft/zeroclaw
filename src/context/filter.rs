//! Context filtering and intelligent retrieval
//! 
//! This module provides intelligent context filtering capabilities:
//! - Task type-based filtering
//! - Vector similarity search (via embedding integration)
//! - Token limit optimization
//! - Hybrid retrieval strategies

use crate::memory::traits::{MemoryCategory, MemoryEntry};
use std::collections::HashMap;

/// Task types for context filtering
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TaskType {
    /// Technical tasks (code, API, debugging)
    Technical,
    /// Creative tasks (writing, design, brainstorming)
    Creative,
    /// Complex tasks (multi-step, coordination)
    Complex,
    /// Simple tasks (quick questions, lookups)
    Simple,
    /// Urgent tasks (time-sensitive, emergency)
    Urgent,
    /// Routine tasks (standard procedures)
    Routine,
}

impl TaskType {
    /// Parse task type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "technical" => Some(TaskType::Technical),
            "creative" => Some(TaskType::Creative),
            "complex" => Some(TaskType::Complex),
            "simple" => Some(TaskType::Simple),
            "urgent" => Some(TaskType::Urgent),
            "routine" => Some(TaskType::Routine),
            _ => None,
        }
    }
}

/// Domain knowledge categories
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DomainKnowledgeCategory {
    TechnicalDocs,
    ApiSpecs,
    CodeExamples,
    BestPractices,
    CreativeCases,
    DesignPatterns,
    InspirationMaterials,
    Methodologies,
    CaseStudies,
    Frameworks,
    QuickReference,
    EmergencyPlans,
    Checklists,
    StandardProcedures,
    Specifications,
    Templates,
    Basics,
    QuickGuides,
}

/// Historical experience categories
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExperienceCategory {
    Debugging,
    Optimization,
    Implementation,
    Brainstorming,
    Iteration,
    Feedback,
    Decomposition,
    Coordination,
    RiskManagement,
    QuickWins,
    FastResolution,
    Workarounds,
    StandardWorkflows,
    EfficiencyTips,
}

/// Context filter for intelligent retrieval
pub struct ContextFilter {
    /// Domain knowledge mapping by task type
    domain_knowledge_mapping: HashMap<TaskType, Vec<DomainKnowledgeCategory>>,
    /// Experience mapping by task type
    experience_mapping: HashMap<TaskType, Vec<ExperienceCategory>>,
    /// Default max tokens
    max_tokens: usize,
}

impl ContextFilter {
    /// Create a new context filter with default settings
    pub fn new() -> Self {
        let mut filter = Self {
            domain_knowledge_mapping: HashMap::new(),
            experience_mapping: HashMap::new(),
            max_tokens: 8000,
        };
        
        filter.initialize_mappings();
        filter
    }
    
    /// Initialize task type mappings
    fn initialize_mappings(&mut self) {
        // Technical tasks
        self.domain_knowledge_mapping.insert(
            TaskType::Technical,
            vec![
                DomainKnowledgeCategory::TechnicalDocs,
                DomainKnowledgeCategory::ApiSpecs,
                DomainKnowledgeCategory::CodeExamples,
                DomainKnowledgeCategory::BestPractices,
            ],
        );
        self.experience_mapping.insert(
            TaskType::Technical,
            vec![
                ExperienceCategory::Debugging,
                ExperienceCategory::Optimization,
                ExperienceCategory::Implementation,
            ],
        );
        
        // Creative tasks
        self.domain_knowledge_mapping.insert(
            TaskType::Creative,
            vec![
                DomainKnowledgeCategory::CreativeCases,
                DomainKnowledgeCategory::DesignPatterns,
                DomainKnowledgeCategory::InspirationMaterials,
            ],
        );
        self.experience_mapping.insert(
            TaskType::Creative,
            vec![
                ExperienceCategory::Brainstorming,
                ExperienceCategory::Iteration,
                ExperienceCategory::Feedback,
            ],
        );
        
        // Complex tasks
        self.domain_knowledge_mapping.insert(
            TaskType::Complex,
            vec![
                DomainKnowledgeCategory::Methodologies,
                DomainKnowledgeCategory::CaseStudies,
                DomainKnowledgeCategory::Frameworks,
            ],
        );
        self.experience_mapping.insert(
            TaskType::Complex,
            vec![
                ExperienceCategory::Decomposition,
                ExperienceCategory::Coordination,
                ExperienceCategory::RiskManagement,
            ],
        );
        
        // Simple tasks
        self.domain_knowledge_mapping.insert(
            TaskType::Simple,
            vec![
                DomainKnowledgeCategory::Basics,
                DomainKnowledgeCategory::QuickGuides,
            ],
        );
        self.experience_mapping.insert(
            TaskType::Simple,
            vec![ExperienceCategory::QuickWins],
        );
        
        // Urgent tasks
        self.domain_knowledge_mapping.insert(
            TaskType::Urgent,
            vec![
                DomainKnowledgeCategory::QuickReference,
                DomainKnowledgeCategory::EmergencyPlans,
                DomainKnowledgeCategory::Checklists,
            ],
        );
        self.experience_mapping.insert(
            TaskType::Urgent,
            vec![
                ExperienceCategory::FastResolution,
                ExperienceCategory::Workarounds,
            ],
        );
        
        // Routine tasks
        self.domain_knowledge_mapping.insert(
            TaskType::Routine,
            vec![
                DomainKnowledgeCategory::StandardProcedures,
                DomainKnowledgeCategory::Specifications,
                DomainKnowledgeCategory::Templates,
            ],
        );
        self.experience_mapping.insert(
            TaskType::Routine,
            vec![
                ExperienceCategory::StandardWorkflows,
                ExperienceCategory::EfficiencyTips,
            ],
        );
    }
    
    /// Filter domain knowledge by task type
    pub fn filter_domain_knowledge(
        &self,
        all_knowledge: &[MemoryEntry],
        task_type: &TaskType,
    ) -> Vec<MemoryEntry> {
        let allowed_categories = self
            .domain_knowledge_mapping
            .get(task_type)
            .cloned()
            .unwrap_or_default();
        
        all_knowledge
            .iter()
            .filter(|entry| {
                // Check if entry category matches allowed categories
                self.matches_domain_category(&entry.category, &allowed_categories)
            })
            .cloned()
            .collect()
    }
    
    /// Filter historical experience by task type
    pub fn filter_historical_experience(
        &self,
        all_experience: &[MemoryEntry],
        task_type: &TaskType,
    ) -> Vec<MemoryEntry> {
        let allowed_categories = self
            .experience_mapping
            .get(task_type)
            .cloned()
            .unwrap_or_default();
        
        all_experience
            .iter()
            .filter(|entry| {
                // Check if entry category matches allowed categories
                self.matches_experience_category(&entry.category, &allowed_categories)
            })
            .cloned()
            .collect()
    }
    
    /// Check if memory category matches domain knowledge categories
    fn matches_domain_category(
        &self,
        category: &MemoryCategory,
        allowed: &[DomainKnowledgeCategory],
    ) -> bool {
        // Simplified matching logic - in production, this would be more sophisticated
        match category {
            MemoryCategory::Core => true, // Core is always allowed
            MemoryCategory::Custom(name) => {
                allowed.iter().any(|cat| {
                    let cat_str = format!("{:?}", cat).to_lowercase();
                    name.to_lowercase().contains(&cat_str)
                })
            }
            _ => false,
        }
    }
    
    /// Check if memory category matches experience categories
    fn matches_experience_category(
        &self,
        category: &MemoryCategory,
        allowed: &[ExperienceCategory],
    ) -> bool {
        match category {
            MemoryCategory::Core => true,
            MemoryCategory::Custom(name) => {
                allowed.iter().any(|cat| {
                    let cat_str = format!("{:?}", cat).to_lowercase();
                    name.to_lowercase().contains(&cat_str)
                })
            }
            _ => false,
        }
    }
    
    /// Calculate relevance score between context and task
    pub fn calculate_relevance_score(
        &self,
        context_entries: &[MemoryEntry],
        task_keywords: &[&str],
    ) -> f64 {
        if context_entries.is_empty() || task_keywords.is_empty() {
            return 0.0;
        }
        
        let mut total_score: f64 = 0.0;
        
        for entry in context_entries {
            let entry_score = self.calculate_entry_relevance(&entry.content, task_keywords);
            total_score = total_score.max(entry_score);
        }
        
        total_score
    }
    
    /// Calculate relevance score for a single entry
    fn calculate_entry_relevance(&self, content: &str, keywords: &[&str]) -> f64 {
        let content_lower = content.to_lowercase();
        let mut score = 0.0;
        
        for &keyword in keywords {
            if content_lower.contains(&keyword.to_lowercase()) {
                score += 1.0;
            }
        }
        
        // Normalize score to 0-1 range
        (score / keywords.len() as f64).min(1.0)
    }
    
    /// Optimize context size to fit token limit
    pub fn optimize_context_size(
        &self,
        entries: Vec<MemoryEntry>,
        max_tokens: Option<usize>,
    ) -> Vec<MemoryEntry> {
        let limit = max_tokens.unwrap_or(self.max_tokens);
        
        // Simple implementation: truncate if exceeds limit
        // In production, this would use more sophisticated strategies
        let mut result = Vec::new();
        let mut current_tokens = 0;
        
        for entry in entries {
            let entry_tokens = self.estimate_tokens(&entry.content);
            
            if current_tokens + entry_tokens <= limit {
                result.push(entry);
                current_tokens += entry_tokens;
            } else {
                break;
            }
        }
        
        result
    }
    
    /// Estimate token count for text (simplified)
    fn estimate_tokens(&self, text: &str) -> usize {
        // Rough estimation: 1 token ≈ 4 characters for English
        // For Chinese: 1 token ≈ 1.5 characters
        let char_count = text.chars().count();
        
        // Detect if primarily Chinese
        let chinese_ratio = text.chars().filter(|c| c.is_ascii()).count() as f64 / char_count as f64;
        
        if chinese_ratio < 0.5 {
            // Primarily Chinese
            (char_count as f64 / 1.5).ceil() as usize
        } else {
            // Primarily English
            (char_count as f64 / 4.0).ceil() as usize
        }
    }
    
    /// Get max tokens setting
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }
    
    /// Set max tokens limit
    pub fn set_max_tokens(&mut self, max: usize) {
        self.max_tokens = max;
    }
    
    /// 根据任务类型过滤全局上下文
    pub fn filter_by_task_type(
        &self,
        global: &super::global_manager::GlobalContext,
        _task_type: &TaskType,
    ) -> super::global_manager::GlobalContext {
        // Create a filtered copy of global context
        // In production, this would use LLM or vector similarity to filter
        let mut filtered = global.clone();
        
        // Simplified filtering - just truncate if too large
        if filtered.domain_knowledge.len() > 1000 {
            filtered.domain_knowledge = filtered.domain_knowledge.chars().take(1000).collect();
        }
        
        if filtered.historical_experience.len() > 1000 {
            filtered.historical_experience = filtered.historical_experience.chars().take(1000).collect();
        }
        
        filtered
    }
}

impl Default for ContextFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_task_type_from_str() {
        assert_eq!(TaskType::from_str("technical"), Some(TaskType::Technical));
        assert_eq!(TaskType::from_str("TECHNICAL"), Some(TaskType::Technical));
        assert_eq!(TaskType::from_str("unknown"), None);
    }
    
    #[test]
    fn test_context_filter_creation() {
        let filter = ContextFilter::new();
        assert!(filter.max_tokens() > 0);
    }
    
    #[test]
    fn test_relevance_score_calculation() {
        let filter = ContextFilter::new();
        
        let entries = vec![
            MemoryEntry {
                id: "1".to_string(),
                key: "test1".to_string(),
                content: "Rust programming and systems design".to_string(),
                category: MemoryCategory::Core,
                timestamp: "2024-01-01".to_string(),
                session_id: None,
                score: None,
            },
        ];
        
        let keywords = ["rust", "programming"];
        let score = filter.calculate_relevance_score(&entries, &keywords);
        
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }
    
    #[test]
    fn test_token_estimation() {
        let filter = ContextFilter::new();
        
        let english_text = "Hello world";
        let english_tokens = filter.estimate_tokens(english_text);
        assert!(english_tokens > 0);
        
        let chinese_text = "你好世界";
        let chinese_tokens = filter.estimate_tokens(chinese_text);
        assert!(chinese_tokens > 0);
    }
}
