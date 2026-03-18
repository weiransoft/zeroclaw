//! Context management module
//! 
//! This module provides dual-layer context management:
//! - Global context layer for cross-task, long-term memory
//! - Task context layer for task-specific, temporary context
//! - Context synchronization between layers
//! - Intelligent filtering and retrieval
//! - LLM-enhanced context management

pub mod filter;
pub mod metrics;
pub mod llm_client;
pub mod summarizer;
pub mod conflict_resolver;
pub mod global_manager;
pub mod task_context;
pub mod synchronizer;
pub mod vector_store;
pub mod version_control;

pub use filter::{ContextFilter, TaskType};
pub use metrics::{ContextMetrics, OperationGuard};
pub use llm_client::{LLMClient, LLMResponse, LLMError, ModelInfo};
pub use summarizer::{ContextSummarizer, AbstractionLevel, FocusArea};
pub use conflict_resolver::{ConflictResolver, ConflictType, ConflictResolution, ResolutionStrategy};
pub use global_manager::{GlobalContextManager, GlobalContext, ContextBackend, InMemoryBackend, CacheConfig};
pub use task_context::{TaskContext, TaskContextManager, TaskDefinition, TaskStatus, CompleteContext};
pub use synchronizer::{ContextSynchronizer, ContextBuilder, ContextConflict};
pub use vector_store::{VectorStore, VectorEntry, Embedding, InMemoryVectorStore, ContextVectorRetriever};
pub use version_control::{SqliteContextStore, ContextVersion};
