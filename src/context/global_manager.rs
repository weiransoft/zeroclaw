//! Global context manager with LLM enhancement and caching
//! 
//! This module provides comprehensive global context management:
//! - LLM-enhanced summarization
//! - Intelligent caching with TTL
//! - Context persistence abstraction
//! - Version control and history

use super::llm_client::LLMClient;
use super::summarizer::{ContextSummarizer, AbstractionLevel, FocusArea};
use super::conflict_resolver::{ConflictResolver, GlobalContext as ConflictGlobalContext};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use lru::LruCache;
use std::time::Duration;
use std::num::NonZeroUsize;

/// Global context structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalContext {
    /// User ID
    pub user_id: String,
    /// User profile (JSON string for flexibility)
    pub user_profile: String,
    /// Domain knowledge (JSON string)
    pub domain_knowledge: String,
    /// Historical experience (JSON string)
    pub historical_experience: String,
    /// Version number
    pub version: u64,
    /// Last update time
    pub last_updated: DateTime<Local>,
}

impl GlobalContext {
    /// Create a new global context
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            user_profile: "{}".to_string(),
            domain_knowledge: "{}".to_string(),
            historical_experience: "{}".to_string(),
            version: 1,
            last_updated: Local::now(),
        }
    }
    
    /// Increment version number
    pub fn increment_version(&mut self) {
        self.version += 1;
        self.last_updated = Local::now();
    }
}

/// Context backend trait for persistence
#[async_trait::async_trait]
pub trait ContextBackend: Send + Sync {
    /// Save global context
    async fn save(&self, context: &GlobalContext) -> Result<()>;
    
    /// Load global context
    async fn load(&self, user_id: &str) -> Result<Option<GlobalContext>>;
    
    /// Delete global context
    async fn delete(&self, user_id: &str) -> Result<()>;
    
    /// Get history versions
    async fn get_history(&self, user_id: &str, from_version: u64, to_version: u64) -> Result<Vec<GlobalContext>>;
}

/// In-memory backend for testing
pub struct InMemoryBackend {
    store: RwLock<HashMap<String, GlobalContext>>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl ContextBackend for InMemoryBackend {
    async fn save(&self, context: &GlobalContext) -> Result<()> {
        let mut store = self.store.write().await;
        store.insert(context.user_id.clone(), context.clone());
        Ok(())
    }
    
    async fn load(&self, user_id: &str) -> Result<Option<GlobalContext>> {
        let store = self.store.read().await;
        Ok(store.get(user_id).cloned())
    }
    
    async fn delete(&self, user_id: &str) -> Result<()> {
        let mut store = self.store.write().await;
        store.remove(user_id);
        Ok(())
    }
    
    async fn get_history(&self, _user_id: &str, _from_version: u64, _to_version: u64) -> Result<Vec<GlobalContext>> {
        // In-memory backend doesn't support history
        Ok(Vec::new())
    }
}

/// Error type for context operations
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("Context not found: {0}")]
    NotFound(String),
    
    #[error("LLM error: {0}")]
    LLMError(#[from] super::llm_client::LLMError),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Backend error: {0}")]
    BackendError(String),
}

pub type Result<T> = std::result::Result<T, ContextError>;

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum cache size
    pub max_size: usize,
    /// Time to live
    pub ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            ttl: Duration::from_hours(1),
        }
    }
}

/// Context cache with TTL
pub struct ContextCache {
    cache: Arc<RwLock<LruCache<String, (GlobalContext, DateTime<Local>)>>>,
    config: CacheConfig,
}

impl ContextCache {
    pub fn new(config: CacheConfig) -> Self {
        let max_size = NonZeroUsize::new(config.max_size).unwrap();
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(max_size))),
            config,
        }
    }
    
    /// Get from cache
    pub async fn get(&self, user_id: &str) -> Option<GlobalContext> {
        let mut cache = self.cache.write().await;
        
        if let Some((context, timestamp)) = cache.get(user_id) {
            // Check if expired
            let now = Local::now();
            let age_ms = now.signed_duration_since(*timestamp).num_milliseconds();
            if age_ms < self.config.ttl.as_millis() as i64 {
                return Some(context.clone());
            } else {
                // Remove expired entry
                cache.pop(user_id);
            }
        }
        
        None
    }
    
    /// Put to cache
    pub async fn put(&self, user_id: String, context: GlobalContext) {
        let mut cache = self.cache.write().await;
        cache.put(user_id, (context, Local::now()));
    }
    
    /// Remove from cache
    pub async fn remove(&self, user_id: &str) {
        let mut cache = self.cache.write().await;
        cache.pop(user_id);
    }
    
    /// Clear all cache
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

/// Global context manager
pub struct GlobalContextManager {
    /// Context backend for persistence
    backend: Arc<dyn ContextBackend>,
    /// Context cache for performance
    cache: Arc<ContextCache>,
    /// LLM client
    llm_client: Arc<dyn LLMClient>,
    /// Context summarizer
    summarizer: Arc<ContextSummarizer>,
    /// Conflict resolver
    conflict_resolver: Arc<ConflictResolver>,
}

impl GlobalContextManager {
    /// Create a new global context manager
    pub fn new(
        backend: Arc<dyn ContextBackend>,
        cache_config: CacheConfig,
        llm_client: Arc<dyn LLMClient>,
    ) -> Self {
        let cache = Arc::new(ContextCache::new(cache_config));
        let summarizer = Arc::new(ContextSummarizer::new(
            llm_client.clone(),
            2000,
            AbstractionLevel::Balanced,
        ));
        let conflict_resolver = Arc::new(ConflictResolver::new(llm_client.clone()));
        
        Self {
            backend,
            cache,
            llm_client,
            summarizer,
            conflict_resolver,
        }
    }
    
    /// Get or create global context
    pub async fn get_or_create(&self, user_id: &str) -> Result<GlobalContext> {
        // Try cache first
        if let Some(cached) = self.cache.get(user_id).await {
            tracing::debug!("Cache hit for user: {}", user_id);
            return Ok(cached);
        }
        
        // Try backend
        if let Some(context) = self.backend.load(user_id).await? {
            tracing::debug!("Backend loaded context for user: {}", user_id);
            // Populate cache
            self.cache.put(user_id.to_string(), context.clone()).await;
            return Ok(context);
        }
        
        // Create new context
        tracing::info!("Creating new context for user: {}", user_id);
        let context = GlobalContext::new(user_id.to_string());
        
        // Save to backend and cache
        self.backend.save(&context).await?;
        self.cache.put(user_id.to_string(), context.clone()).await;
        
        Ok(context)
    }
    
    /// Update global context
    pub async fn update<F>(&self, user_id: &str, updater: F) -> Result<GlobalContext>
    where
        F: FnOnce(&mut GlobalContext) -> (),
    {
        let mut context = self.get_or_create(user_id).await?;
        
        // Apply updater
        updater(&mut context);
        
        // Increment version
        context.increment_version();
        
        // Save to backend and cache
        self.backend.save(&context).await?;
        self.cache.put(user_id.to_string(), context.clone()).await;
        
        Ok(context)
    }
    
    /// Generate context summary using LLM
    pub async fn generate_summary(
        &self,
        user_id: &str,
        focus_areas: &[FocusArea],
    ) -> Result<String> {
        let context = self.get_or_create(user_id).await?;
        
        let summary = self.summarizer
            .summarize_global_context(
                &context.user_profile,
                &context.domain_knowledge,
                &context.historical_experience,
                focus_areas,
            )
            .await
            .map_err(|e| ContextError::LLMError(e))?;
        
        Ok(summary)
    }
    
    /// Detect and resolve conflicts
    pub async fn resolve_conflicts(
        &self,
        user_id: &str,
        task_context: &super::conflict_resolver::TaskContext,
    ) -> Result<Vec<super::conflict_resolver::ConflictResolution>> {
        let global_context = self.get_or_create(user_id).await?;
        
        // Convert to conflict resolver format
        let conflict_global = ConflictGlobalContext {
            user_id: global_context.user_id.clone(),
            user_profile: global_context.user_profile.clone(),
            domain_knowledge: global_context.domain_knowledge.clone(),
            historical_experience: global_context.historical_experience.clone(),
        };
        
        let resolutions = self.conflict_resolver
            .resolve_conflict(&conflict_global, task_context)
            .await
            .map_err(|e| ContextError::BackendError(e.to_string()))?;
        
        Ok(resolutions)
    }
    
    /// Delete global context
    pub async fn delete(&self, user_id: &str) -> Result<()> {
        self.backend.delete(user_id).await?;
        self.cache.remove(user_id).await;
        Ok(())
    }
    
    /// Clear cache
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }
    
    /// Get context history
    pub async fn get_history(
        &self,
        user_id: &str,
        from_version: u64,
        to_version: u64,
    ) -> Result<Vec<GlobalContext>> {
        self.backend.get_history(user_id, from_version, to_version).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::llm_client::MockLLMClient;
    
    #[tokio::test]
    async fn test_get_or_create_context() {
        let backend = Arc::new(InMemoryBackend::new());
        let llm_client = Arc::new(MockLLMClient::with_response("Mock response".to_string()));
        let manager = GlobalContextManager::new(backend, CacheConfig::default(), llm_client);
        
        let context = manager.get_or_create("user1").await.unwrap();
        
        assert_eq!(context.user_id, "user1");
        assert_eq!(context.version, 1);
    }
    
    #[tokio::test]
    async fn test_update_context() {
        let backend = Arc::new(InMemoryBackend::new());
        let llm_client = Arc::new(MockLLMClient::with_response("Mock response".to_string()));
        let manager = GlobalContextManager::new(backend, CacheConfig::default(), llm_client);
        
        let updated = manager.update("user1", |ctx| {
            ctx.user_profile = r#"{"preference": "Python"}"#.to_string();
        }).await.unwrap();
        
        assert_eq!(updated.version, 2);
        assert!(updated.user_profile.contains("Python"));
    }
    
    #[tokio::test]
    async fn test_cache_ttl() {
        let mut config = CacheConfig::default();
        config.ttl = Duration::from_millis(100);
        
        let cache = ContextCache::new(config);
        
        // Create a test context
        let context = GlobalContext::new("user1".to_string());
        
        // Put in cache
        cache.put("user1".to_string(), context.clone()).await;
        
        // Should be in cache immediately
        tokio::time::sleep(Duration::from_millis(10)).await;
        let cached = cache.get("user1").await;
        assert!(cached.is_some(), "Context should be in cache");
        assert_eq!(cached.unwrap().user_id, "user1");
        
        // Wait for TTL
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should be expired
        let expired = cache.get("user1").await;
        assert!(expired.is_none(), "Context should be expired after TTL");
    }
}
