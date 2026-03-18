//! Context synchronizer for bidirectional synchronization between global and task contexts
//! 
//! This module provides intelligent synchronization:
//! - Task to global synchronization (extracting valuable learnings)
//! - Global to task synchronization (injecting relevant knowledge)
//! - Conflict detection and resolution
//! - Version control for context updates

use super::conflict_resolver::{ConflictResolver, ConflictResolution};
use super::filter::{ContextFilter, TaskType};
use super::global_manager::{GlobalContext, GlobalContextManager};
use super::summarizer::ContextSummarizer;
use super::task_context::{TaskContext, TaskStatus};
use crate::context::global_manager::Result as ContextResult;
use std::sync::Arc;

/// Context conflict detected during synchronization
#[derive(Debug, Clone)]
pub struct ContextConflict {
    /// Conflict type
    pub conflict_type: String,
    /// Conflicting content from global context
    pub global_content: String,
    /// Conflicting content from task context
    pub task_content: String,
    /// Severity (0.0-1.0)
    pub severity: f64,
}

/// Context synchronizer
pub struct ContextSynchronizer {
    /// Conflict resolver
    conflict_resolver: Arc<ConflictResolver>,
    /// Context filter
    context_filter: Arc<ContextFilter>,
    /// Context summarizer
    summarizer: Arc<ContextSummarizer>,
}

impl ContextSynchronizer {
    /// Create a new context synchronizer
    pub fn new(
        conflict_resolver: Arc<ConflictResolver>,
        context_filter: Arc<ContextFilter>,
        summarizer: Arc<ContextSummarizer>,
    ) -> Self {
        Self {
            conflict_resolver,
            context_filter,
            summarizer,
        }
    }
    
    /// Synchronize from task to global context
    /// Extract valuable information from task and update global context
    pub async fn sync_to_global(
        &self,
        global_manager: &GlobalContextManager,
        task_ctx: &TaskContext,
    ) -> ContextResult<GlobalContext> {
        tracing::debug!("Synchronizing task {} to global context", task_ctx.task_id);
        
        // 1. Generate task summary
        let task_summary = self.summarizer
            .summarize_task_context(
                &task_ctx.task_id,
                &format!("{:?}", task_ctx.task_definition.task_type),
                &format!("{:?}", task_ctx.status),
                &task_ctx.get_conversation_summary(),
                &task_ctx.get_results_summary(),
                "", // decisions
                true,
            )
            .await
            .map_err(|e| crate::context::global_manager::ContextError::BackendError(
                format!("Summarizer error: {}", e)
            ))?;
        
        // 2. Extract valuable information
        let updates = self.extract_valuable_information(&task_summary, task_ctx);
        
        // 3. Update global context
        let updated_global = global_manager
            .update(&task_ctx.task_id.split('_').next().unwrap_or("default"), |ctx| {
                // Update historical experience with lessons learned
                if !updates.lessons_learned.is_empty() {
                    ctx.historical_experience = updates.lessons_learned;
                }
                
                // Update domain knowledge if new knowledge discovered
                if !updates.new_knowledge.is_empty() {
                    ctx.domain_knowledge = updates.new_knowledge;
                }
                
                // Increment version
                ctx.increment_version();
            })
            .await?;
        
        tracing::info!(
            "Successfully synced task {} to global context (version {})",
            task_ctx.task_id,
            updated_global.version
        );
        
        Ok(updated_global)
    }
    
    /// Synchronize from global to task context
    /// Inject relevant global knowledge into task context
    pub async fn sync_from_global(
        &self,
        global: &GlobalContext,
        task_ctx: &mut TaskContext,
    ) -> ContextResult<()> {
        tracing::debug!(
            "Synchronizing global context to task {} (type: {:?})",
            task_ctx.task_id,
            task_ctx.task_definition.task_type
        );
        
        // 1. Filter global context based on task type
        let filtered_global = self.context_filter.filter_by_task_type(
            global,
            &task_ctx.task_definition.task_type,
        );
        
        // 2. Calculate relevance score
        let relevance_score = self.calculate_relevance(
            &filtered_global,
            &task_ctx.task_definition,
        );
        
        // 3. Inject into task context (stored as memory string)
        let memory_content = format!(
            "Global Context (Relevance: {:.2}):\nUser Profile: {}\nDomain Knowledge: {}\nHistorical Experience: {}",
            relevance_score,
            filtered_global.user_profile,
            filtered_global.domain_knowledge,
            filtered_global.historical_experience,
        );
        task_ctx.add_memory(memory_content);
        
        tracing::info!(
            "Successfully synced global context to task {} (relevance: {:.2})",
            task_ctx.task_id,
            relevance_score
        );
        
        Ok(())
    }
    
    /// Detect and resolve conflicts between global and task contexts
    pub async fn resolve_conflicts(
        &self,
        global: &GlobalContext,
        task: &TaskContext,
    ) -> ContextResult<Vec<ConflictResolution>> {
        tracing::debug!("Detecting conflicts for task {}", task.task_id);
        
        // Create a temporary task context for conflict resolver
        let task_context_for_resolver = super::conflict_resolver::TaskContext {
            task_id: task.task_id.clone(),
            task_definition: super::conflict_resolver::TaskDefinition {
                task_type: task.task_definition.task_type.clone(),
                description: task.task_definition.description.clone(),
                goals: vec![],
            },
            status: format!("{:?}", task.status),
            conversation_history: task.conversation_history.iter().map(|c| c.content.clone()).collect(),
            intermediate_results: task.intermediate_results.iter().map(|r| r.data.clone()).collect(),
        };
        
        // Convert global context
        let global_context_for_resolver = super::conflict_resolver::GlobalContext {
            user_id: global.user_id.clone(),
            user_profile: global.user_profile.clone(),
            domain_knowledge: global.domain_knowledge.clone(),
            historical_experience: global.historical_experience.clone(),
        };
        
        // Use conflict resolver
        let resolutions = self.conflict_resolver
            .resolve_conflict(&global_context_for_resolver, &task_context_for_resolver)
            .await
            .map_err(|e| crate::context::global_manager::ContextError::BackendError(
                format!("Conflict resolver error: {}", e)
            ))?;
        
        tracing::info!("Resolved {} conflicts for task {}", resolutions.len(), task.task_id);
        
        Ok(resolutions)
    }
    
    /// Extract valuable information from task execution
    fn extract_valuable_information(
        &self,
        task_summary: &str,
        task_ctx: &TaskContext,
    ) -> ContextUpdates {
        // This is a simplified extraction
        // In production, use LLM to intelligently extract information
        
        let mut updates = ContextUpdates::default();
        
        // Extract lessons learned
        if task_ctx.status == TaskStatus::Completed {
            updates.lessons_learned = format!(
                "Task {} completed successfully. Summary: {}",
                task_ctx.task_id,
                task_summary
            );
        }
        
        // Extract new knowledge
        if !task_ctx.intermediate_results.is_empty() {
            updates.new_knowledge = format!(
                "New knowledge from task {}: {}",
                task_ctx.task_id,
                task_ctx.get_results_summary()
            );
        }
        
        updates
    }
    
    /// Calculate relevance score between global context and task
    fn calculate_relevance(
        &self,
        global: &GlobalContext,
        task_def: &super::task_context::TaskDefinition,
    ) -> f64 {
        // Simplified relevance calculation
        // In production, use vector similarity or LLM-based relevance scoring
        
        let mut score: f64 = 0.5; // Base score
        
        // Boost score if domain knowledge matches task type
        match task_def.task_type {
            TaskType::Technical => {
                if global.domain_knowledge.contains("technical") {
                    score += 0.2;
                }
            }
            TaskType::Creative => {
                if global.domain_knowledge.contains("creative") {
                    score += 0.2;
                }
            }
            TaskType::Complex => {
                if global.historical_experience.contains("complex") {
                    score += 0.2;
                }
            }
            _ => {}
        }
        
        // Cap at 1.0
        score.min(1.0f64)
    }
}

/// Context updates extracted from task
#[derive(Debug, Default)]
pub struct ContextUpdates {
    /// Lessons learned from task
    pub lessons_learned: String,
    /// New knowledge discovered
    pub new_knowledge: String,
    /// User preference updates
    pub preference_updates: String,
}

/// Context builder for creating optimized complete contexts
pub struct ContextBuilder {
    /// Maximum token limit
    max_tokens: usize,
}

impl ContextBuilder {
    /// Create a new context builder
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }
    
    /// Build complete context from global and task contexts
    pub fn build(
        &self,
        global: GlobalContext,
        task: TaskContext,
    ) -> super::task_context::CompleteContext {
        // Calculate relevance score
        let relevance_score = self.calculate_task_relevance(&global, &task);
        
        // Create complete context
        let complete = super::task_context::CompleteContext::new(
            global,
            task,
            relevance_score,
        );
        
        // Optimize for token limit if needed
        self.optimize_for_tokens(complete)
    }
    
    /// Calculate relevance score
    fn calculate_task_relevance(
        &self,
        _global: &GlobalContext,
        _task: &TaskContext,
    ) -> f64 {
        // Simplified relevance calculation
        // In production, use vector similarity or LLM-based relevance scoring
        
        let score: f64 = 0.5; // Base score
        
        // TODO: Implement proper relevance calculation
        // For now, return base score
        
        score.min(1.0f64)
    }
    
    /// Optimize context to fit within token limit
    fn optimize_for_tokens(
        &self,
        mut complete: super::task_context::CompleteContext,
    ) -> super::task_context::CompleteContext {
        // Simple truncation strategy
        // In production, use more sophisticated summarization
        
        let estimated_tokens = complete.to_prompt().len() / 4; // Rough estimate
        
        if estimated_tokens > self.max_tokens {
            tracing::warn!(
                "Context size ({}) exceeds limit ({}), truncating...",
                estimated_tokens,
                self.max_tokens
            );
            
            // Truncate conversation history
            let mut ctx_guard = complete.task_context.conversation_history.clone();
            while ctx_guard.len() > 2 && {
                let temp_ctx = super::task_context::TaskContext {
                    conversation_history: ctx_guard.clone(),
                    ..complete.task_context.clone()
                };
                let temp_complete = super::task_context::CompleteContext::new(
                    complete.global_context.clone(),
                    temp_ctx,
                    complete.relevance_score,
                );
                temp_complete.to_prompt().len() / 4 > self.max_tokens
            } {
                ctx_guard.remove(1); // Remove oldest (keep first system message)
            }
            
            complete.task_context.conversation_history = ctx_guard;
        }
        
        complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::global_manager::GlobalContextManager;
    use crate::context::llm_client::MockLLMClient;
    use crate::context::task_context::{TaskContext, TaskDefinition};
    
    #[test]
    fn test_context_synchronizer_creation() {
        let llm_client = Arc::new(MockLLMClient::with_response("Mock".to_string()));
        let conflict_resolver = Arc::new(ConflictResolver::new(llm_client.clone()));
        let context_filter = Arc::new(ContextFilter::new());
        let summarizer = Arc::new(ContextSummarizer::new(
            llm_client,
            1000,
            super::super::summarizer::AbstractionLevel::Balanced,
        ));
        
        let _synchronizer = ContextSynchronizer::new(
            conflict_resolver,
            context_filter,
            summarizer,
        );
        
        // Just test that it can be created
        assert!(true);
    }
    
    #[test]
    fn test_context_builder() {
        let builder = ContextBuilder::new(4000);
        
        let global = GlobalContext::new("user1".to_string());
        let task = TaskContext::new(TaskDefinition {
            task_id: "task_1".to_string(),
            task_type: TaskType::Technical,
            description: "Test task".to_string(),
            priority: 5,
            token_budget: 4000,
        });
        
        let complete = builder.build(global, task);
        
        assert!(complete.relevance_score > 0.0);
        assert!(complete.relevance_score <= 1.0);
    }
}
