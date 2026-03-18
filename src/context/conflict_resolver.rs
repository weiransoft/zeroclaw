//! LLM-enhanced context conflict resolver
//! 
//! This module provides intelligent conflict resolution using LLM:
//! - Conflict detection (factual, preference, logical)
//! - Intelligent resolution strategies
//! - Conflict history tracking
//! - Confidence scoring

use super::llm_client::{LLMClient, Result};
use crate::context::filter::TaskType;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Conflict types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ConflictType {
    /// Factual conflict: contradictory facts from different sources
    FactualConflict {
        statement_a: String,
        statement_b: String,
        source_a: String,
        source_b: String,
    },
    /// Preference conflict: user preference changes
    PreferenceConflict {
        old_preference: String,
        new_preference: String,
        context: String,
    },
    /// Logical conflict: inconsistent reasoning results
    LogicalConflict {
        reasoning_chain_a: Vec<String>,
        reasoning_chain_b: Vec<String>,
        conclusion_a: String,
        conclusion_b: String,
    },
}

/// Resolution strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "strategy", content = "content")]
pub enum ResolutionStrategy {
    /// Adopt the latest information
    AdoptLatest,
    /// Adopt from more credible source
    AdoptMoreCredible {
        source: String,
    },
    /// Merge both viewpoints
    Merge {
        merged_content: String,
    },
    /// Keep both with context labels
    KeepBoth {
        context_a: String,
        context_b: String,
    },
    /// Requires user confirmation
    RequiresUserConfirmation,
}

/// Conflict resolution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolution {
    /// Unique conflict ID
    pub conflict_id: String,
    /// Type of conflict
    pub conflict_type: ConflictType,
    /// Resolution strategy used
    pub resolution_strategy: ResolutionStrategy,
    /// Resolved content
    pub resolved_content: String,
    /// Confidence score (0.0-1.0)
    pub confidence: f64,
    /// Reasoning for the decision
    pub reasoning: String,
}

/// Conflict record for history tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecord {
    /// The conflict detected
    pub conflict: ConflictType,
    /// The resolution applied
    pub resolution: ConflictResolution,
    /// Timestamp of resolution
    pub timestamp: DateTime<Local>,
}

/// Global context structure (simplified for conflict detection)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalContext {
    pub user_id: String,
    pub user_profile: String,
    pub domain_knowledge: String,
    pub historical_experience: String,
}

/// Task context structure (simplified for conflict detection)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: String,
    pub task_definition: TaskDefinition,
    pub status: String,
    pub conversation_history: Vec<String>,
    pub intermediate_results: Vec<String>,
}

/// Task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub task_type: TaskType,
    pub description: String,
    pub goals: Vec<String>,
}

/// LLM-enhanced conflict resolver
pub struct ConflictResolver {
    llm_client: Arc<dyn LLMClient>,
    conflict_history: Arc<Mutex<Vec<ConflictRecord>>>,
}

impl ConflictResolver {
    /// Create a new conflict resolver
    pub fn new(llm_client: Arc<dyn LLMClient>) -> Self {
        Self {
            llm_client,
            conflict_history: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Detect and resolve conflicts between global and task contexts
    pub async fn resolve_conflict(
        &self,
        global: &GlobalContext,
        task: &TaskContext,
    ) -> Result<Vec<ConflictResolution>> {
        // Step 1: Detect potential conflicts
        let conflicts = self.detect_conflicts(global, task).await?;
        
        // Step 2: Resolve each conflict
        let mut resolutions = Vec::new();
        for conflict in conflicts {
            let resolution = self.resolve_single_conflict(global, task, &conflict).await?;
            resolutions.push(resolution);
        }
        
        Ok(resolutions)
    }
    
    /// Detect conflicts between global and task contexts
    async fn detect_conflicts(
        &self,
        global: &GlobalContext,
        task: &TaskContext,
    ) -> Result<Vec<ConflictType>> {
        let mut conflicts = Vec::new();
        
        // Detect preference conflicts
        if let Some(pref_conflict) = self.detect_preference_conflicts(global, task) {
            conflicts.push(pref_conflict);
        }
        
        // Detect factual conflicts
        if let Some(fact_conflict) = self.detect_factual_conflicts(global, task) {
            conflicts.push(fact_conflict);
        }
        
        // Detect logical conflicts
        if let Some(logic_conflict) = self.detect_logical_conflicts(global, task) {
            conflicts.push(logic_conflict);
        }
        
        Ok(conflicts)
    }
    
    /// Detect preference conflicts
    fn detect_preference_conflicts(
        &self,
        global: &GlobalContext,
        task: &TaskContext,
    ) -> Option<ConflictType> {
        // Extract preferences from task conversation
        let task_preferences = self.extract_preferences_from_conversation(
            &task.conversation_history
        );
        
        // Check if task preferences contradict global preferences
        if !task_preferences.is_empty() {
            // Check if global profile contains preference keywords
            let has_global_preference = global.user_profile.contains("喜欢") 
                || global.user_profile.contains("偏好")
                || global.user_profile.contains("倾向");
            
            // Check if task conversation contains contrasting preferences
            let has_task_contrast = task_preferences.iter().any(|pref| {
                // Look for preference changes or contrasts
                pref.contains("更") || pref.contains("现在") || pref.contains("改为")
            });
            
            if has_global_preference && has_task_contrast {
                return Some(ConflictType::PreferenceConflict {
                    old_preference: global.user_profile.clone(),
                    new_preference: task_preferences.join("; "),
                    context: format!("Task {} conversation", task.task_id),
                });
            }
        }
        
        None
    }
    
    /// Detect factual conflicts
    fn detect_factual_conflicts(
        &self,
        global: &GlobalContext,
        task: &TaskContext,
    ) -> Option<ConflictType> {
        // Extract factual statements from task results
        let task_facts = self.extract_facts_from_results(&task.intermediate_results);
        
        // Check for contradictions with global knowledge
        for fact in task_facts {
            if self.is_contradictory(&fact, &global.domain_knowledge) {
                return Some(ConflictType::FactualConflict {
                    statement_a: global.domain_knowledge.clone(),
                    statement_b: fact,
                    source_a: "Global Context".to_string(),
                    source_b: format!("Task {}", task.task_id),
                });
            }
        }
        
        None
    }
    
    /// Detect logical conflicts
    fn detect_logical_conflicts(
        &self,
        global: &GlobalContext,
        task: &TaskContext,
    ) -> Option<ConflictType> {
        // Extract reasoning chains from task
        let (reasoning_a, conclusion_a) = self.extract_reasoning_chain(
            &global.historical_experience
        );
        
        let (reasoning_b, conclusion_b) = self.extract_reasoning_chain(
            &task.intermediate_results.join("\n")
        );
        
        // Check if conclusions contradict
        if !conclusion_a.is_empty() && !conclusion_b.is_empty() {
            if self.are_conclusions_contradictory(&conclusion_a, &conclusion_b) {
                return Some(ConflictType::LogicalConflict {
                    reasoning_chain_a: reasoning_a,
                    reasoning_chain_b: reasoning_b,
                    conclusion_a,
                    conclusion_b,
                });
            }
        }
        
        None
    }
    
    /// Resolve a single conflict using LLM
    async fn resolve_single_conflict(
        &self,
        global: &GlobalContext,
        task: &TaskContext,
        conflict: &ConflictType,
    ) -> Result<ConflictResolution> {
        // Build conflict resolution prompt
        let prompt = self.build_conflict_resolution_prompt(global, task, conflict);
        
        // Call LLM for reasoning
        let response = self.llm_client.generate(&prompt).await?;
        
        // Parse LLM's decision
        let resolution = self.parse_resolution_response(&response.content, conflict)?;
        
        // Record resolution history
        self.record_resolution(conflict, &resolution);
        
        Ok(resolution)
    }
    
    /// Build prompt for conflict resolution
    fn build_conflict_resolution_prompt(
        &self,
        _global: &GlobalContext,
        task: &TaskContext,
        conflict: &ConflictType,
    ) -> String {
        match conflict {
            ConflictType::FactualConflict {
                statement_a,
                statement_b,
                source_a,
                source_b,
            } => format!(
                r#"发现事实冲突:

来源 A ({}): {}
来源 B ({}): {}

上下文信息:
- 任务类型：{:?}
- 任务状态：{}

请分析:
1. 哪个陈述更可信？为什么？
2. 是否存在调和的可能性？
3. 应该采用什么策略解决这个冲突？

可选策略:
- AdoptLatest: 采纳最新信息
- AdoptMoreCredible: 采纳更可信的来源
- Merge: 合并两种观点
- KeepBoth: 保留两者，标注不同上下文
- RequiresUserConfirmation: 需要用户确认

请提供详细的推理过程和最终决策。
最后，请以 JSON 格式返回决策:
{{
    "strategy": "策略名称",
    "resolved_content": "解决后的内容",
    "confidence": 0.0-1.0,
    "reasoning": "推理过程摘要"
}}"#,
                source_a, statement_a,
                source_b, statement_b,
                task.task_definition.task_type,
                task.status,
            ),
            
            ConflictType::PreferenceConflict {
                old_preference,
                new_preference,
                context,
            } => format!(
                r#"发现偏好冲突:

旧偏好：{}
新偏好：{}

变化上下文：{}

请分析:
1. 这是真正的偏好变化，还是上下文相关的临时偏好？
2. 如果是偏好变化，是否应该更新全局上下文？
3. 变化的原因可能是什么？

请提供详细的推理过程和最终决策。
最后，请以 JSON 格式返回决策:
{{
    "strategy": "策略名称",
    "resolved_content": "解决后的内容",
    "confidence": 0.0-1.0,
    "reasoning": "推理过程摘要"
}}"#,
                old_preference,
                new_preference,
                context,
            ),
            
            ConflictType::LogicalConflict {
                reasoning_chain_a,
                reasoning_chain_b,
                conclusion_a,
                conclusion_b,
            } => format!(
                r#"发现逻辑冲突:

推理链 A:
{}
结论：{}

推理链 B:
{}
结论：{}

请分析:
1. 哪个推理链更合理？为什么？
2. 是否存在逻辑谬误或假设错误？
3. 如何调和两个结论？

请提供详细的推理过程和最终决策。
最后，请以 JSON 格式返回决策:
{{
    "strategy": "策略名称",
    "resolved_content": "解决后的内容",
    "confidence": 0.0-1.0,
    "reasoning": "推理过程摘要"
}}"#,
                reasoning_chain_a.join("\n"),
                conclusion_a,
                reasoning_chain_b.join("\n"),
                conclusion_b,
            ),
        }
    }
    
    /// Parse LLM response into resolution
    fn parse_resolution_response(
        &self,
        response: &str,
        conflict: &ConflictType,
    ) -> Result<ConflictResolution> {
        // Try to extract JSON from response
        let json_start = response.find('{').unwrap_or(0);
        let json_end = response.rfind('}').unwrap_or(response.len());
        let json_str = &response[json_start..=json_end];
        
        // Parse JSON (simplified parsing, in production use proper JSON parser)
        let strategy = if json_str.contains("AdoptLatest") {
            ResolutionStrategy::AdoptLatest
        } else if json_str.contains("AdoptMoreCredible") {
            ResolutionStrategy::AdoptMoreCredible {
                source: "Source".to_string(),
            }
        } else if json_str.contains("Merge") {
            ResolutionStrategy::Merge {
                merged_content: "Merged content".to_string(),
            }
        } else if json_str.contains("KeepBoth") {
            ResolutionStrategy::KeepBoth {
                context_a: "Context A".to_string(),
                context_b: "Context B".to_string(),
            }
        } else {
            ResolutionStrategy::RequiresUserConfirmation
        };
        
        // Extract confidence (default to 0.7)
        let confidence = if let Some(start) = json_str.find("\"confidence\":") {
            let rest = &json_str[start + 13..];
            rest.split(',').next()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(0.7)
        } else {
            0.7
        };
        
        // Generate conflict ID
        let conflict_id = format!("conflict_{}", chrono::Local::now().timestamp_nanos());
        
        Ok(ConflictResolution {
            conflict_id,
            conflict_type: conflict.clone(),
            resolution_strategy: strategy,
            resolved_content: "Resolved content".to_string(),
            confidence,
            reasoning: response.to_string(),
        })
    }
    
    /// Record resolution in history
    fn record_resolution(&self, conflict: &ConflictType, resolution: &ConflictResolution) {
        let mut history = self.conflict_history.lock().unwrap();
        history.push(ConflictRecord {
            conflict: conflict.clone(),
            resolution: resolution.clone(),
            timestamp: Local::now(),
        });
    }
    
    /// Get conflict history
    pub fn get_history(&self) -> Vec<ConflictRecord> {
        let history = self.conflict_history.lock().unwrap();
        history.clone()
    }
    
    // Helper methods (simplified implementations)
    fn extract_preferences_from_conversation(&self, conversation: &[String]) -> Vec<String> {
        // Extract preference statements from conversation
        conversation.iter()
            .filter(|msg| {
                msg.contains("喜欢") || msg.contains("偏好") || msg.contains("倾向") ||
                msg.contains("更") || msg.contains("选择")
            })
            .cloned()
            .collect()
    }
    
    fn extract_facts_from_results(&self, results: &[String]) -> Vec<String> {
        // Extract factual statements from results
        results.iter()
            .filter(|msg| msg.contains("是") || msg.contains("应该") || msg.contains("必须"))
            .cloned()
            .collect()
    }
    
    fn is_contradictory(&self, statement: &str, knowledge: &str) -> bool {
        // Simple heuristic for contradiction detection
        // In production, use NLP or LLM for better detection
        statement.contains("不") && knowledge.contains(statement.replace("不", "").trim())
    }
    
    fn extract_reasoning_chain(&self, text: &str) -> (Vec<String>, String) {
        // Extract reasoning chain and conclusion from text
        let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
        let conclusion = lines.last().cloned().unwrap_or_default();
        (lines, conclusion)
    }
    
    fn are_conclusions_contradictory(&self, conclusion_a: &str, conclusion_b: &str) -> bool {
        // Simple heuristic for contradiction detection
        conclusion_a.contains("不") && conclusion_b.contains(conclusion_a.replace("不", "").trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::llm_client::MockLLMClient;
    
    #[tokio::test]
    async fn test_detect_preference_conflict() {
        let llm_client = Arc::new(MockLLMClient::with_response(
            r#"{"strategy": "AdoptLatest", "confidence": 0.8, "reasoning": "新偏好更新"}"#.to_string()
        ));
        let resolver = ConflictResolver::new(llm_client);
        
        let global = GlobalContext {
            user_id: "user1".to_string(),
            user_profile: "用户喜欢 Python".to_string(),
            domain_knowledge: "Python 是动态类型语言".to_string(),
            historical_experience: "使用 Python 开发多年".to_string(),
        };
        
        let task = TaskContext {
            task_id: "task1".to_string(),
            task_definition: TaskDefinition {
                task_type: TaskType::Technical,
                description: "讨论编程语言".to_string(),
                goals: vec![],
            },
            status: "In Progress".to_string(),
            conversation_history: vec![
                "我觉得 Rust 更好".to_string(),
                "我更喜欢 Rust 的安全性".to_string(),
            ],
            intermediate_results: vec![],
        };
        
        let conflicts = resolver.detect_conflicts(&global, &task).await.unwrap();
        
        assert!(!conflicts.is_empty());
        assert!(matches!(conflicts[0], ConflictType::PreferenceConflict { .. }));
    }
    
    #[tokio::test]
    async fn test_resolve_conflict() {
        let llm_client = Arc::new(MockLLMClient::with_response(
            r#"{"strategy": "AdoptLatest", "resolved_content": "用户现在偏好 Rust", "confidence": 0.85, "reasoning": "用户明确表达了新偏好"}"#.to_string()
        ));
        let resolver = ConflictResolver::new(llm_client);
        
        let global = GlobalContext {
            user_id: "user1".to_string(),
            user_profile: "用户喜欢 Python".to_string(),
            domain_knowledge: "Python 知识".to_string(),
            historical_experience: "Python 经验".to_string(),
        };
        
        let task = TaskContext {
            task_id: "task1".to_string(),
            task_definition: TaskDefinition {
                task_type: TaskType::Technical,
                description: "讨论语言".to_string(),
                goals: vec![],
            },
            status: "Completed".to_string(),
            conversation_history: vec!["我更喜欢 Rust".to_string()],
            intermediate_results: vec![],
        };
        
        let resolutions = resolver.resolve_conflict(&global, &task).await.unwrap();
        
        assert!(!resolutions.is_empty());
        assert!(resolutions[0].confidence > 0.0);
        assert!(resolutions[0].confidence <= 1.0);
    }
}
