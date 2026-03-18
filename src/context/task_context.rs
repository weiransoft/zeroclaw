//! Task context management
//! 
//! This module provides task-specific context management:
//! - Task context structure for temporary, task-specific information
//! - Task context manager for lifecycle management
//! - Synchronization with global context
//! - Memory management for task-level data

use super::filter::TaskType;
use super::global_manager::{GlobalContext, GlobalContextManager};

use crate::memory::traits::{Memory, MemoryCategory};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Task ID
    pub task_id: String,
    /// Task type
    pub task_type: TaskType,
    /// Task description
    pub description: String,
    /// Task priority (1-10)
    pub priority: u8,
    /// Expected token budget
    pub token_budget: usize,
}

/// Task status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Task created, not started
    Pending,
    /// Task in progress
    Running,
    /// Task paused
    Paused,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task cancelled
    Cancelled,
}

/// Conversation turn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    /// Role (user/assistant/system)
    pub role: String,
    /// Message content
    pub content: String,
    /// Timestamp
    pub timestamp: DateTime<Local>,
    /// Metadata (optional)
    pub metadata: Option<String>,
}

/// Intermediate result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateResult {
    /// Result ID
    pub result_id: String,
    /// Result description
    pub description: String,
    /// Result data (JSON string)
    pub data: String,
    /// Creation timestamp
    pub created_at: DateTime<Local>,
}

/// Task context - task-specific temporary context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    /// Task ID
    pub task_id: String,
    /// Task definition
    pub task_definition: TaskDefinition,
    /// Task status
    pub status: TaskStatus,
    /// Conversation history
    pub conversation_history: Vec<ConversationTurn>,
    /// Intermediate results
    pub intermediate_results: Vec<IntermediateResult>,
    /// Task memories (temporary) - simplified as strings
    pub memories: Vec<String>,
    /// Creation time
    pub created_at: DateTime<Local>,
    /// Last update time
    pub updated_at: DateTime<Local>,
    /// Token usage counter
    pub token_count: usize,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(task_def: TaskDefinition) -> Self {
        let now = Local::now();
        Self {
            task_id: task_def.task_id.clone(),
            task_definition: task_def,
            status: TaskStatus::Pending,
            conversation_history: Vec::new(),
            intermediate_results: Vec::new(),
            memories: Vec::new(),
            created_at: now,
            updated_at: now,
            token_count: 0,
        }
    }
    
    /// Update task status
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.updated_at = Local::now();
    }
    
    /// Add conversation turn
    pub fn add_conversation(&mut self, role: &str, content: &str) {
        self.conversation_history.push(ConversationTurn {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Local::now(),
            metadata: None,
        });
        self.updated_at = Local::now();
    }
    
    /// Add intermediate result
    pub fn add_intermediate_result(&mut self, result: IntermediateResult) {
        self.intermediate_results.push(result);
        self.updated_at = Local::now();
    }
    
    /// Add memory entry
    pub fn add_memory(&mut self, content: String) {
        self.memories.push(content);
        self.updated_at = Local::now();
    }
    
    /// Update token count
    pub fn update_token_count(&mut self, count: usize) {
        self.token_count = count;
        self.updated_at = Local::now();
    }
    
    /// Check if token budget exceeded
    pub fn is_over_budget(&self) -> bool {
        self.token_count > self.task_definition.token_budget
    }
    
    /// Get conversation summary
    pub fn get_conversation_summary(&self) -> String {
        if self.conversation_history.is_empty() {
            return String::new();
        }
        
        self.conversation_history
            .iter()
            .map(|turn| format!("{}: {}", turn.role, turn.content))
            .collect::<Vec<_>>()
            .join("\n")
    }
    
    /// Get intermediate results summary
    pub fn get_results_summary(&self) -> String {
        if self.intermediate_results.is_empty() {
            return String::new();
        }
        
        self.intermediate_results
            .iter()
            .map(|r| format!("{}: {}", r.description, r.data))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Task context manager
pub struct TaskContextManager {
    /// Current task context
    context: Arc<tokio::sync::RwLock<TaskContext>>,
    /// Reference to global context manager
    global_manager: Arc<GlobalContextManager>,
    /// Memory backend for task storage
    memory_backend: Box<dyn Memory>,
}

impl TaskContextManager {
    /// Create a new task context manager
    pub fn new(
        task_def: TaskDefinition,
        global_manager: Arc<GlobalContextManager>,
        memory_backend: Box<dyn Memory>,
    ) -> Self {
        let context = Arc::new(tokio::sync::RwLock::new(
            TaskContext::new(task_def)
        ));
        
        Self {
            context,
            global_manager,
            memory_backend,
        }
    }
    
    /// Get task context (read-only)
    pub async fn get_context(&self) -> tokio::sync::RwLockReadGuard<'_, TaskContext> {
        self.context.read().await
    }
    
    /// Get task context (mutable)
    pub async fn get_context_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, TaskContext> {
        self.context.write().await
    }
    
    /// Synchronize from global context
    pub async fn sync_from_global(&self) -> crate::context::global_manager::Result<()> {
        let user_id = {
            let ctx = self.context.read().await;
            // Extract user_id from task_id or use default
            ctx.task_id.split('_').next().unwrap_or("default").to_string()
        };
        
        // Get global context
        let global_ctx = self.global_manager.get_or_create(&user_id).await?;
        
        // Apply relevant global knowledge to task context
        // This is a simplified version - in production, use ContextFilter
        tracing::debug!(
            "Synced global context for task {}: user_profile={}, domain_knowledge={}",
            self.context.read().await.task_id,
            global_ctx.user_profile.len(),
            global_ctx.domain_knowledge.len()
        );
        
        Ok(())
    }
    
    /// Synchronize to global context (extract valuable learnings)
    pub async fn sync_to_global(&self) -> crate::context::global_manager::Result<()> {
        let _user_id = {
            let ctx = self.context.read().await;
            ctx.task_id.split('_').next().unwrap_or("default").to_string()
        };
        
        let ctx = self.context.read().await;
        
        // Extract valuable information from task
        // - New user preferences discovered
        // - Lessons learned
        // - Important decisions and rationale
        // This is simplified - in production, use ContextSummarizer
        
        tracing::debug!(
            "Task {} completed with {} conversation turns and {} intermediate results",
            ctx.task_id,
            ctx.conversation_history.len(),
            ctx.intermediate_results.len()
        );
        
        Ok(())
    }
    
    /// Add task memory
    pub async fn add_memory(
        &self,
        content: &str,
        _category: MemoryCategory,
    ) -> anyhow::Result<()> {
        let mut ctx = self.context.write().await;
        
        // Store in memory backend - simplified
        let key = format!("{}_mem_{}", ctx.task_id, ctx.memories.len());
        self.memory_backend.store(&key, content, _category).await?;
        
        // Add to task context - simplified as string
        ctx.memories.push(content.to_string());
        
        Ok(())
    }
    
    /// Retrieve task memories
    pub async fn retrieve_memories(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        // Search in task-specific memories
        let entries = self.memory_backend.recall(query, limit).await?;
        Ok(entries.into_iter().map(|e| e.content).collect())
    }
    
    /// Update task status
    pub async fn update_status(&self, status: TaskStatus) {
        let mut ctx = self.context.write().await;
        ctx.set_status(status);
    }
    
    /// Add conversation to task context
    pub async fn add_conversation(&self, role: &str, content: &str) {
        let mut ctx = self.context.write().await;
        ctx.add_conversation(role, content);
    }
    
    /// Add intermediate result
    pub async fn add_result(&self, description: &str, data: &str) {
        let mut ctx = self.context.write().await;
        
        let result = IntermediateResult {
            result_id: format!("{}_res_{}", ctx.task_id, ctx.intermediate_results.len()),
            description: description.to_string(),
            data: data.to_string(),
            created_at: Local::now(),
        };
        
        ctx.add_intermediate_result(result);
    }
    
    /// Get task ID
    pub fn task_id(&self) -> String {
        self.context.blocking_read().task_id.clone()
    }
    
    /// Check if task is complete
    pub async fn is_complete(&self) -> bool {
        let ctx = self.context.read().await;
        matches!(
            ctx.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

/// Complete context - combination of global and task context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteContext {
    /// Global context (filtered)
    pub global_context: GlobalContext,
    /// Task context
    pub task_context: TaskContext,
    /// Build time
    pub build_time: DateTime<Local>,
    /// Relevance score
    pub relevance_score: f64,
}

impl CompleteContext {
    /// Build complete context from global and task contexts
    pub fn new(
        global_context: GlobalContext,
        task_context: TaskContext,
        relevance_score: f64,
    ) -> Self {
        Self {
            global_context,
            task_context,
            build_time: Local::now(),
            relevance_score,
        }
    }
    
    /// Convert to prompt format
    pub fn to_prompt(&self) -> String {
        let mut prompt = String::new();
        
        // Add global context
        prompt.push_str("=== Global Context ===\n");
        prompt.push_str(&format!("User Profile: {}\n", self.global_context.user_profile));
        prompt.push_str(&format!("Domain Knowledge: {}\n", self.global_context.domain_knowledge));
        prompt.push_str(&format!(
            "Historical Experience: {}\n",
            self.global_context.historical_experience
        ));
        
        // Add task context
        prompt.push_str("\n=== Task Context ===\n");
        prompt.push_str(&format!("Task ID: {}\n", self.task_context.task_id));
        prompt.push_str(&format!(
            "Task Type: {:?}\n",
            self.task_context.task_definition.task_type
        ));
        prompt.push_str(&format!("Task Status: {:?}\n", self.task_context.status));
        prompt.push_str(&format!(
            "Task Description: {}\n",
            self.task_context.task_definition.description
        ));
        
        // Add conversation history
        if !self.task_context.conversation_history.is_empty() {
            prompt.push_str("\n=== Conversation History ===\n");
            prompt.push_str(&self.task_context.get_conversation_summary());
        }
        
        // Add intermediate results
        if !self.task_context.intermediate_results.is_empty() {
            prompt.push_str("\n=== Intermediate Results ===\n");
            prompt.push_str(&self.task_context.get_results_summary());
        }
        
        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::global_manager::{CacheConfig, InMemoryBackend};
    use crate::context::llm_client::MockLLMClient;
    use crate::memory::none::NoneMemory;
    
    #[test]
    fn test_task_context_creation() {
        let task_def = TaskDefinition {
            task_id: "task_1".to_string(),
            task_type: TaskType::Technical,
            description: "Test task".to_string(),
            priority: 5,
            token_budget: 4000,
        };
        
        let ctx = TaskContext::new(task_def.clone());
        
        assert_eq!(ctx.task_id, "task_1");
        assert_eq!(ctx.status, TaskStatus::Pending);
        assert!(ctx.conversation_history.is_empty());
        assert!(ctx.intermediate_results.is_empty());
    }
    
    #[test]
    fn test_task_context_updates() {
        let mut ctx = TaskContext::new(TaskDefinition {
            task_id: "task_1".to_string(),
            task_type: TaskType::Technical,
            description: "Test".to_string(),
            priority: 5,
            token_budget: 4000,
        });
        
        // Add conversation
        ctx.add_conversation("user", "Hello");
        ctx.add_conversation("assistant", "Hi there!");
        
        assert_eq!(ctx.conversation_history.len(), 2);
        
        // Add result
        ctx.add_intermediate_result(IntermediateResult {
            result_id: "res_1".to_string(),
            description: "Test result".to_string(),
            data: "{}".to_string(),
            created_at: Local::now(),
        });
        
        assert_eq!(ctx.intermediate_results.len(), 1);
    }
    
    #[tokio::test]
    async fn test_task_context_manager() {
        let backend = Arc::new(InMemoryBackend::new());
        let llm_client = Arc::new(MockLLMClient::with_response("Mock".to_string()));
        let global_manager = Arc::new(GlobalContextManager::new(
            backend,
            CacheConfig::default(),
            llm_client,
        ));
        
        let task_def = TaskDefinition {
            task_id: "task_1".to_string(),
            task_type: TaskType::Technical,
            description: "Test task".to_string(),
            priority: 5,
            token_budget: 4000,
        };
        
        let memory = Box::new(NoneMemory::new());
        let task_manager = TaskContextManager::new(task_def, global_manager, memory);
        
        // Test conversation
        task_manager.add_conversation("user", "Hello").await;
        task_manager.add_conversation("assistant", "Hi").await;
        
        let ctx = task_manager.get_context().await;
        assert_eq!(ctx.conversation_history.len(), 2);
        
        // Test status update
        task_manager.update_status(TaskStatus::Running).await;
        let ctx = task_manager.get_context().await;
        assert_eq!(ctx.status, TaskStatus::Running);
    }
    
    #[test]
    fn test_complete_context() {
        let global = GlobalContext::new("user1".to_string());
        let task = TaskContext::new(TaskDefinition {
            task_id: "task_1".to_string(),
            task_type: TaskType::Technical,
            description: "Test".to_string(),
            priority: 5,
            token_budget: 4000,
        });
        
        let complete = CompleteContext::new(global, task.clone(), 0.95);
        
        assert_eq!(complete.relevance_score, 0.95);
        assert_eq!(complete.task_context.task_id, "task_1");
        
        // Test prompt generation
        let prompt = complete.to_prompt();
        assert!(prompt.contains("Global Context"));
        assert!(prompt.contains("Task Context"));
    }
}
