use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use lru::LruCache;
use std::num::NonZeroUsize;

use super::knowledge_base::KnowledgeBase;
use super::hierarchical_memory::{MemoryDuration, MemoryScope, MemoryType};

const DEFAULT_MEMORY_CACHE_SIZE: usize = 500;
const SHORT_TERM_MAX_ENTRIES: usize = 100;
const MEDIUM_TERM_MAX_ENTRIES: usize = 300;
const KNOWLEDGE_PROMOTION_LENGTH: usize = 300;
const KNOWLEDGE_PROMOTION_IMPORTANCE: f64 = 0.65;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMemory {
    pub id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub scope: MemoryScope,
    pub importance: f64,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u32,
    pub compressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCompressResult {
    pub original_length: usize,
    pub compressed_length: usize,
    pub key_points: Vec<String>,
}

pub struct EnhancedHierarchicalMemory {
    workspace_dir: PathBuf,
    agent_id: String,
    session_id: Option<String>,
    
    short_term: Arc<RwLock<Vec<CachedMemory>>>,
    medium_term: Arc<RwLock<HashMap<String, CachedMemory>>>,
    
    memory_cache: Arc<RwLock<LruCache<String, CachedMemory>>>,
    search_cache: Arc<RwLock<LruCache<String, Vec<CachedMemory>>>>,
    
    long_term_memory: Arc<dyn crate::memory::Memory>,
    knowledge_base: Arc<KnowledgeBase>,
    
    config: EnhancedMemoryConfig,
    stats: Arc<RwLock<EnhancedMemoryStats>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedMemoryConfig {
    pub short_term_max_entries: usize,
    pub medium_term_max_entries: usize,
    pub short_term_ttl_seconds: i64,
    pub medium_term_ttl_hours: i64,
    pub cache_size: usize,
    pub knowledge_promotion_length: usize,
    pub knowledge_promotion_importance: f64,
    pub auto_compress: bool,
    pub compression_threshold: usize,
}

impl Default for EnhancedMemoryConfig {
    fn default() -> Self {
        Self {
            short_term_max_entries: SHORT_TERM_MAX_ENTRIES,
            medium_term_max_entries: MEDIUM_TERM_MAX_ENTRIES,
            short_term_ttl_seconds: 3600,
            medium_term_ttl_hours: 24,
            cache_size: DEFAULT_MEMORY_CACHE_SIZE,
            knowledge_promotion_length: KNOWLEDGE_PROMOTION_LENGTH,
            knowledge_promotion_importance: KNOWLEDGE_PROMOTION_IMPORTANCE,
            auto_compress: true,
            compression_threshold: 500,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnhancedMemoryStats {
    pub short_term_count: usize,
    pub medium_term_count: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub knowledge_promotions: u64,
    pub compressions: u64,
    pub last_cleanup: Option<DateTime<Utc>>,
}

impl EnhancedHierarchicalMemory {
    pub fn new(
        workspace_dir: PathBuf,
        agent_id: String,
        session_id: Option<String>,
        long_term_memory: Arc<dyn crate::memory::Memory>,
        knowledge_base: Arc<KnowledgeBase>,
        config: EnhancedMemoryConfig,
    ) -> Self {
        let cache_size = NonZeroUsize::new(config.cache_size).unwrap_or(NonZeroUsize::new(500).unwrap());
        
        Self {
            workspace_dir,
            agent_id,
            session_id,
            short_term: Arc::new(RwLock::new(Vec::new())),
            medium_term: Arc::new(RwLock::new(HashMap::new())),
            memory_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            search_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            long_term_memory,
            knowledge_base,
            config,
            stats: Arc::new(RwLock::new(EnhancedMemoryStats::default())),
        }
    }
    
    pub async fn store(
        &self,
        content: &str,
        memory_type: MemoryType,
        duration: MemoryDuration,
        scope: MemoryScope,
        importance: f64,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        
        let (final_content, compressed) = if self.config.auto_compress && content.len() > self.config.compression_threshold {
            let result = self.compress_content(content);
            (format!("{}\n\nKey Points:\n{}", result.compressed_length, result.key_points.join("\n")), true)
        } else {
            (content.to_string(), false)
        };
        
        let cached = CachedMemory {
            id: id.clone(),
            content: final_content.clone(),
            memory_type: memory_type.clone(),
            scope: scope.clone(),
            importance,
            created_at: now,
            last_accessed: now,
            access_count: 0,
            compressed,
        };
        
        {
            let mut cache = self.memory_cache.write().await;
            cache.put(id.clone(), cached.clone());
        }
        
        match duration {
            MemoryDuration::Short => {
                let mut short_term = self.short_term.write().await;
                short_term.push(cached.clone());
                
                if short_term.len() > self.config.short_term_max_entries {
                    self.evict_short_term(&mut short_term).await;
                }
            }
            MemoryDuration::Medium => {
                let mut medium_term = self.medium_term.write().await;
                medium_term.insert(id.clone(), cached.clone());
                
                if medium_term.len() > self.config.medium_term_max_entries {
                    self.evict_medium_term(&mut medium_term).await;
                }
            }
            MemoryDuration::Long => {
                let category = self.memory_type_to_category(&memory_type);
                let _ = self.long_term_memory.store(&id, &cached.content, category).await;
            }
        }
        
        self.invalidate_search_cache().await;
        self.update_stats().await;
        
        id
    }
    
    pub async fn recall(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<CachedMemory> {
        let cache_key = format!("recall:{}:{}", query, limit);
        
        {
            let mut cache = self.search_cache.write().await;
            if let Some(cached) = cache.get(&cache_key) {
                let mut stats = self.stats.write().await;
                stats.cache_hits += 1;
                return cached.clone();
            }
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.cache_misses += 1;
        }
        
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        
        {
            let short_term = self.short_term.read().await;
            for item in short_term.iter().rev() {
                if self.matches_query(&item, &query_lower) {
                    results.push(item.clone());
                }
                if results.len() >= limit {
                    break;
                }
            }
        }
        
        if results.len() < limit {
            let medium_term = self.medium_term.read().await;
            let mut medium_items: Vec<_> = medium_term.values()
                .filter(|item| self.matches_query(item, &query_lower))
                .collect();
            
            medium_items.sort_by(|a, b| 
                b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal)
            );
            
            for item in medium_items.into_iter().take(limit - results.len()) {
                results.push(item.clone());
            }
        }
        
        if results.len() < limit {
            if let Ok(long_results) = self.long_term_memory.recall(query, limit - results.len()).await {
                for entry in long_results {
                    results.push(CachedMemory {
                        id: entry.id,
                        content: entry.content,
                        memory_type: MemoryType::Fact,
                        scope: MemoryScope::Workspace,
                        importance: entry.score.unwrap_or(0.5),
                        created_at: Utc::now(),
                        last_accessed: Utc::now(),
                        access_count: 0,
                        compressed: false,
                    });
                }
            }
        }
        
        {
            let mut cache = self.search_cache.write().await;
            cache.put(cache_key, results.clone());
        }
        
        results
    }
    
    pub async fn recall_with_knowledge(
        &self,
        query: &str,
        limit: usize,
        include_knowledge: bool,
    ) -> Vec<CachedMemory> {
        let mut results = self.recall(query, limit).await;
        
        if include_knowledge && results.len() < limit {
            let kb_results = self.knowledge_base.search(
                query,
                None,
                None,
                limit - results.len(),
            ).await;
            
            for kb_result in kb_results {
                results.push(CachedMemory {
                    id: kb_result.entry.id.as_str().to_string(),
                    content: kb_result.entry.summary.clone(),
                    memory_type: MemoryType::Fact,
                    scope: MemoryScope::Workspace,
                    importance: kb_result.score,
                    created_at: kb_result.entry.created_at,
                    last_accessed: Utc::now(),
                    access_count: 0,
                    compressed: false,
                });
            }
        }
        
        results
    }
    
    fn matches_query(&self, item: &CachedMemory, query_lower: &str) -> bool {
        item.content.to_lowercase().contains(query_lower)
    }
    
    fn compress_content(&self, content: &str) -> MemoryCompressResult {
        let sentences: Vec<&str> = content.split(|c| c == '.' || c == '!' || c == '?')
            .filter(|s| !s.trim().is_empty())
            .collect();
        
        let mut key_points = Vec::new();
        let important_keywords = [
            "important", "critical", "must", "should", "need", "require",
            "决定", "重要", "关键", "必须", "需要", "注意",
            "error", "bug", "fix", "solution", "success", "fail",
        ];
        
        for sentence in &sentences {
            let sentence_lower = sentence.to_lowercase();
            if important_keywords.iter().any(|k| sentence_lower.contains(k)) {
                key_points.push(sentence.trim().to_string());
            }
        }
        
        if key_points.is_empty() {
            key_points = sentences.iter().take(3).map(|s| s.trim().to_string()).collect();
        }
        
        MemoryCompressResult {
            original_length: content.len(),
            compressed_length: key_points.join(". ").len(),
            key_points,
        }
    }
    
    async fn evict_short_term(&self, short_term: &mut Vec<CachedMemory>) {
        let now = Utc::now();
        let ttl = Duration::seconds(self.config.short_term_ttl_seconds);
        
        short_term.retain(|item| {
            now - item.last_accessed < ttl
        });
        
        while short_term.len() > self.config.short_term_max_entries {
            if let Some(promoted) = short_term.first() {
                if promoted.content.len() >= self.config.knowledge_promotion_length 
                    && promoted.importance >= self.config.knowledge_promotion_importance 
                    && promoted.scope == MemoryScope::Workspace {
                    
                    self.knowledge_base.promote_from_memory(
                        &promoted.content,
                        "auto",
                        &self.agent_id,
                        promoted.importance,
                    ).await;
                    
                    let mut stats = self.stats.write().await;
                    stats.knowledge_promotions += 1;
                } else if promoted.importance >= 0.5 {
                    let category = self.memory_type_to_category(&promoted.memory_type);
                    let _ = self.long_term_memory.store(
                        &promoted.id,
                        &promoted.content,
                        category,
                    ).await;
                }
            }
            short_term.remove(0);
        }
    }
    
    async fn evict_medium_term(&self, medium_term: &mut HashMap<String, CachedMemory>) {
        let now = Utc::now();
        let ttl = Duration::hours(self.config.medium_term_ttl_hours);
        
        let to_remove: Vec<String> = medium_term.iter()
            .filter(|(_, item)| now - item.last_accessed > ttl)
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in to_remove {
            if let Some(item) = medium_term.remove(&id) {
                if item.content.len() >= self.config.knowledge_promotion_length 
                    && item.importance >= self.config.knowledge_promotion_importance {
                    
                    self.knowledge_base.promote_from_memory(
                        &item.content,
                        "auto",
                        &self.agent_id,
                        item.importance,
                    ).await;
                    
                    let mut stats = self.stats.write().await;
                    stats.knowledge_promotions += 1;
                } else if item.importance >= 0.5 {
                    let category = self.memory_type_to_category(&item.memory_type);
                    let _ = self.long_term_memory.store(
                        &item.id,
                        &item.content,
                        category,
                    ).await;
                }
            }
        }
    }
    
    fn memory_type_to_category(&self, memory_type: &MemoryType) -> crate::memory::MemoryCategory {
        match memory_type {
            MemoryType::Preference | MemoryType::Fact => crate::memory::MemoryCategory::Core,
            MemoryType::Decision | MemoryType::Finding => crate::memory::MemoryCategory::Core,
            MemoryType::Conversation => crate::memory::MemoryCategory::Conversation,
            MemoryType::TaskProgress | MemoryType::ToolUsage => crate::memory::MemoryCategory::Daily,
            MemoryType::Error => crate::memory::MemoryCategory::Custom("error".to_string()),
        }
    }
    
    async fn invalidate_search_cache(&self) {
        let mut cache = self.search_cache.write().await;
        cache.clear();
    }
    
    async fn update_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.short_term_count = self.short_term.read().await.len();
        stats.medium_term_count = self.medium_term.read().await.len();
    }
    
    pub async fn cleanup(&self) {
        {
            let mut short_term = self.short_term.write().await;
            self.evict_short_term(&mut short_term).await;
        }
        
        {
            let mut medium_term = self.medium_term.write().await;
            self.evict_medium_term(&mut medium_term).await;
        }
        
        {
            let mut cache = self.memory_cache.write().await;
            cache.clear();
        }
        
        self.invalidate_search_cache().await;
        
        let mut stats = self.stats.write().await;
        stats.last_cleanup = Some(Utc::now());
    }
    
    pub async fn get_stats(&self) -> EnhancedMemoryStats {
        self.stats.read().await.clone()
    }
    
    pub async fn build_context_prompt(
        &self,
        query: &str,
        max_tokens: usize,
    ) -> String {
        let memories = self.recall_with_knowledge(query, 5, true).await;
        
        let mut context = String::new();
        let mut current_tokens = 0;
        
        for memory in memories {
            let entry_text = format!("{}\n\n", memory.content);
            let tokens = entry_text.len() / 4;
            
            if current_tokens + tokens > max_tokens {
                break;
            }
            
            context.push_str(&entry_text);
            current_tokens += tokens;
        }
        
        context
    }
    
    pub async fn clear_session(&self) {
        self.short_term.write().await.clear();
        
        self.medium_term.write().await.clear();
        
        self.memory_cache.write().await.clear();
        self.invalidate_search_cache().await;
        self.update_stats().await;
    }
    
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
    
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Memory, MemoryCategory, MemoryEntry};
    
    struct MockMemory;
    
    #[async_trait::async_trait]
    impl Memory for MockMemory {
        fn name(&self) -> &str { "mock" }
        async fn store(&self, _key: &str, _content: &str, _category: MemoryCategory) -> anyhow::Result<()> {
            Ok(())
        }
        async fn recall(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }
        async fn recall_paginated(
            &self,
            _query: &str,
            _limit: usize,
            _offset: usize,
        ) -> anyhow::Result<(Vec<MemoryEntry>, usize)> {
            Ok((vec![], 0))
        }
        async fn get(&self, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(None)
        }
        async fn list(&self, _category: Option<&MemoryCategory>) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }
        async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
            Ok(true)
        }
        async fn count(&self) -> anyhow::Result<usize> {
            Ok(0)
        }
        async fn health_check(&self) -> bool {
            true
        }
    }
    
    fn create_test_kb() -> Arc<KnowledgeBase> {
        let long_term = Arc::new(MockMemory);
        Arc::new(KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            super::super::knowledge_base::KnowledgeBaseConfig::default(),
        ))
    }
    
    #[tokio::test]
    async fn test_enhanced_memory_store() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        let id = memory.store(
            "Test content",
            MemoryType::Conversation,
            MemoryDuration::Short,
            MemoryScope::Private,
            0.5,
        ).await;
        
        assert!(!id.is_empty());
        
        let stats = memory.get_stats().await;
        assert_eq!(stats.short_term_count, 1);
    }
    
    #[tokio::test]
    async fn test_enhanced_memory_recall() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        memory.store(
            "Important decision about architecture",
            MemoryType::Decision,
            MemoryDuration::Medium,
            MemoryScope::Session,
            0.8,
        ).await;
        
        let results = memory.recall("decision", 10).await;
        assert_eq!(results.len(), 1);
    }
    
    #[tokio::test]
    async fn test_memory_compression() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let config = EnhancedMemoryConfig {
            auto_compress: true,
            compression_threshold: 100,
            ..Default::default()
        };
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            config,
        );
        
        let long_content = "This is a very long content. It contains important information. The critical decision was made. We must remember this. This is the end of the content.";
        
        memory.store(
            long_content,
            MemoryType::Conversation,
            MemoryDuration::Short,
            MemoryScope::Private,
            0.5,
        ).await;
        
        let stats = memory.get_stats().await;
        assert_eq!(stats.short_term_count, 1);
    }
    
    #[tokio::test]
    async fn test_cache_hit_miss() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        memory.store(
            "Test content for cache",
            MemoryType::Conversation,
            MemoryDuration::Medium,
            MemoryScope::Session,
            0.7,
        ).await;
        
        let _ = memory.recall("cache", 10).await;
        let stats = memory.get_stats().await;
        assert_eq!(stats.cache_misses, 1);
        
        let _ = memory.recall("cache", 10).await;
        let stats = memory.get_stats().await;
        assert_eq!(stats.cache_hits, 1);
    }
    
    #[tokio::test]
    async fn test_recall_with_knowledge() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        memory.store(
            "Test memory content",
            MemoryType::Conversation,
            MemoryDuration::Medium,
            MemoryScope::Session,
            0.7,
        ).await;
        
        let results = memory.recall_with_knowledge("test", 10, true).await;
        assert!(!results.is_empty());
    }
    
    #[tokio::test]
    async fn test_cleanup() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        memory.store(
            "Test content for cleanup",
            MemoryType::Conversation,
            MemoryDuration::Short,
            MemoryScope::Private,
            0.5,
        ).await;
        
        memory.cleanup().await;
        
        let stats = memory.get_stats().await;
        assert!(stats.last_cleanup.is_some());
    }
    
    #[tokio::test]
    async fn test_build_context_prompt() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        memory.store(
            "Important context information",
            MemoryType::Decision,
            MemoryDuration::Medium,
            MemoryScope::Session,
            0.8,
        ).await;
        
        let context = memory.build_context_prompt("context", 500).await;
        assert!(!context.is_empty());
    }
    
    #[tokio::test]
    async fn test_clear_session() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        memory.store(
            "Session content",
            MemoryType::Conversation,
            MemoryDuration::Short,
            MemoryScope::Private,
            0.5,
        ).await;
        
        memory.clear_session().await;
        
        let stats = memory.get_stats().await;
        assert_eq!(stats.short_term_count, 0);
    }
    
    #[tokio::test]
    async fn test_agent_id_and_session_id() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "test_agent".to_string(),
            Some("test_session".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        assert_eq!(memory.agent_id(), "test_agent");
        assert_eq!(memory.session_id(), Some("test_session"));
    }
    
    #[tokio::test]
    async fn test_store_long_term() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        let id = memory.store(
            "Long term important decision",
            MemoryType::Decision,
            MemoryDuration::Long,
            MemoryScope::Workspace,
            0.9,
        ).await;
        
        assert!(!id.is_empty());
    }
    
    #[tokio::test]
    async fn test_store_medium_term() {
        let long_term = Arc::new(MockMemory);
        let kb = create_test_kb();
        
        let memory = EnhancedHierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            kb,
            EnhancedMemoryConfig::default(),
        );
        
        let id = memory.store(
            "Medium term task progress",
            MemoryType::TaskProgress,
            MemoryDuration::Medium,
            MemoryScope::Session,
            0.6,
        ).await;
        
        assert!(!id.is_empty());
        
        let stats = memory.get_stats().await;
        assert_eq!(stats.medium_term_count, 1);
    }
}
