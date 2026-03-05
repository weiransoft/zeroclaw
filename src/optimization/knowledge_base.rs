use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use lru::LruCache;
use std::num::NonZeroUsize;

const DEFAULT_CACHE_SIZE: usize = 1000;
const KNOWLEDGE_MIN_LENGTH: usize = 500;
const KNOWLEDGE_IMPORTANCE_THRESHOLD: f64 = 0.6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct KnowledgeId(String);

impl KnowledgeId {
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: KnowledgeId,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub category: KnowledgeCategory,
    pub tags: Vec<String>,
    pub source_type: KnowledgeSource,
    pub source_agent: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub access_count: u32,
    pub importance_score: f64,
    pub version: u32,
    pub related_entries: Vec<KnowledgeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum KnowledgeCategory {
    Technical,
    Domain,
    Project,
    Pattern,
    Solution,
    Error,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum KnowledgeSource {
    MemoryPromotion,
    AgentSummary,
    DocumentImport,
    ConsensusDecision,
    LearningExtraction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub entry: KnowledgeEntry,
    pub score: f64,
    pub match_type: MatchType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchType {
    Exact,
    Semantic,
    Keyword,
    Tag,
}

pub struct KnowledgeBase {
    workspace_dir: PathBuf,
    entries: Arc<RwLock<HashMap<KnowledgeId, KnowledgeEntry>>>,
    tag_index: Arc<RwLock<HashMap<String, Vec<KnowledgeId>>>>,
    category_index: Arc<RwLock<HashMap<KnowledgeCategory, Vec<KnowledgeId>>>>,
    search_cache: Arc<RwLock<LruCache<String, Vec<KnowledgeSearchResult>>>>,
    content_cache: Arc<RwLock<LruCache<KnowledgeId, String>>>,
    long_term_memory: Arc<dyn crate::memory::Memory>,
    config: KnowledgeBaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBaseConfig {
    pub cache_size: usize,
    pub min_content_length: usize,
    pub importance_threshold: f64,
    pub auto_promote_enabled: bool,
    pub auto_summarize_enabled: bool,
    pub max_summary_length: usize,
    pub version_history_enabled: bool,
}

impl Default for KnowledgeBaseConfig {
    fn default() -> Self {
        Self {
            cache_size: DEFAULT_CACHE_SIZE,
            min_content_length: KNOWLEDGE_MIN_LENGTH,
            importance_threshold: KNOWLEDGE_IMPORTANCE_THRESHOLD,
            auto_promote_enabled: true,
            auto_summarize_enabled: true,
            max_summary_length: 200,
            version_history_enabled: true,
        }
    }
}

impl KnowledgeBase {
    pub fn new(
        workspace_dir: PathBuf,
        long_term_memory: Arc<dyn crate::memory::Memory>,
        config: KnowledgeBaseConfig,
    ) -> Self {
        let cache_size = NonZeroUsize::new(config.cache_size).unwrap_or(NonZeroUsize::new(1000).unwrap());
        
        Self {
            workspace_dir,
            entries: Arc::new(RwLock::new(HashMap::new())),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
            category_index: Arc::new(RwLock::new(HashMap::new())),
            search_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            content_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            long_term_memory,
            config,
        }
    }
    
    pub async fn add_entry(
        &self,
        title: &str,
        content: &str,
        category: KnowledgeCategory,
        tags: Vec<String>,
        source_type: KnowledgeSource,
        source_agent: &str,
        importance: f64,
    ) -> KnowledgeId {
        let id = KnowledgeId::new(&Uuid::new_v4().to_string());
        let now = Utc::now();
        
        let summary = if self.config.auto_summarize_enabled {
            self.generate_summary(content)
        } else {
            content.chars().take(self.config.max_summary_length).collect()
        };
        
        let entry = KnowledgeEntry {
            id: id.clone(),
            title: title.to_string(),
            content: content.to_string(),
            summary,
            category: category.clone(),
            tags: tags.clone(),
            source_type,
            source_agent: source_agent.to_string(),
            created_at: now,
            updated_at: now,
            access_count: 0,
            importance_score: importance,
            version: 1,
            related_entries: vec![],
        };
        
        {
            let mut entries = self.entries.write().await;
            entries.insert(id.clone(), entry);
        }
        
        {
            let mut tag_index = self.tag_index.write().await;
            for tag in tags {
                tag_index.entry(tag).or_default().push(id.clone());
            }
        }
        
        {
            let mut category_index = self.category_index.write().await;
            category_index.entry(category).or_default().push(id.clone());
        }
        
        let _ = self.long_term_memory.store(
            id.as_str(),
            &format!("{}: {}", title, content),
            crate::memory::MemoryCategory::Core,
        ).await;
        
        self.invalidate_cache(&id).await;
        
        id
    }
    
    pub async fn get(&self, id: &KnowledgeId) -> Option<KnowledgeEntry> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(id) {
            entry.access_count += 1;
            entry.updated_at = Utc::now();
            return Some(entry.clone());
        }
        None
    }
    
    pub async fn search(
        &self,
        query: &str,
        categories: Option<&[KnowledgeCategory]>,
        tags: Option<&[String]>,
        limit: usize,
    ) -> Vec<KnowledgeSearchResult> {
        let cache_key = format!("{}:{:?}:{:?}:{}", query, categories, tags, limit);
        
        {
            let mut cache = self.search_cache.write().await;
            if let Some(cached) = cache.get(&cache_key) {
                return cached.clone();
            }
        }
        
        let entries = self.entries.read().await;
        let query_lower = query.to_lowercase();
        let mut results: Vec<KnowledgeSearchResult> = Vec::new();
        
        for entry in entries.values() {
            if let Some(cats) = categories {
                if !cats.contains(&entry.category) {
                    continue;
                }
            }
            
            if let Some(required_tags) = tags {
                if !required_tags.iter().all(|t| entry.tags.contains(t)) {
                    continue;
                }
            }
            
            let (score, match_type) = self.calculate_relevance(&entry, &query_lower);
            
            if score > 0.0 {
                results.push(KnowledgeSearchResult {
                    entry: entry.clone(),
                    score,
                    match_type,
                });
            }
        }
        
        results.sort_by(|a, b| 
            b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
        );
        results.truncate(limit);
        
        {
            let mut cache = self.search_cache.write().await;
            cache.put(cache_key, results.clone());
        }
        
        results
    }
    
    fn calculate_relevance(&self, entry: &KnowledgeEntry, query: &str) -> (f64, MatchType) {
        let title_lower = entry.title.to_lowercase();
        let content_lower = entry.content.to_lowercase();
        let summary_lower = entry.summary.to_lowercase();
        
        if title_lower == query {
            return (1.0, MatchType::Exact);
        }
        
        if title_lower.contains(query) {
            return (0.9, MatchType::Keyword);
        }
        
        if entry.tags.iter().any(|t| t.to_lowercase() == query) {
            return (0.85, MatchType::Tag);
        }
        
        if content_lower.contains(query) || summary_lower.contains(query) {
            let query_words: Vec<&str> = query.split_whitespace().collect();
            let mut match_count = 0;
            for word in &query_words {
                if content_lower.contains(word) {
                    match_count += 1;
                }
            }
            let coverage = match_count as f64 / query_words.len().max(1) as f64;
            return (0.5 + coverage * 0.3, MatchType::Keyword);
        }
        
        (0.0, MatchType::Keyword)
    }
    
    pub async fn update_entry(
        &self,
        id: &KnowledgeId,
        content: Option<&str>,
        tags: Option<Vec<String>>,
        importance: Option<f64>,
    ) -> bool {
        let mut entries = self.entries.write().await;
        
        if let Some(entry) = entries.get_mut(id) {
            if let Some(new_content) = content {
                if self.config.version_history_enabled {
                    entry.version += 1;
                }
                entry.content = new_content.to_string();
                entry.summary = self.generate_summary(new_content);
            }
            
            if let Some(new_tags) = tags {
                {
                    let mut tag_index = self.tag_index.write().await;
                    for old_tag in &entry.tags {
                        if let Some(ids) = tag_index.get_mut(old_tag) {
                            ids.retain(|i| i != id);
                        }
                    }
                    for new_tag in &new_tags {
                        tag_index.entry(new_tag.clone()).or_default().push(id.clone());
                    }
                }
                entry.tags = new_tags;
            }
            
            if let Some(new_importance) = importance {
                entry.importance_score = new_importance;
            }
            
            entry.updated_at = Utc::now();
            
            self.invalidate_cache(id).await;
            
            return true;
        }
        
        false
    }
    
    pub async fn link_entries(&self, id1: &KnowledgeId, id2: &KnowledgeId) -> bool {
        let mut entries = self.entries.write().await;
        
        let exists = |entries: &HashMap<KnowledgeId, KnowledgeEntry>, id: &KnowledgeId| {
            entries.contains_key(id)
        };
        
        if !exists(&entries, id1) || !exists(&entries, id2) {
            return false;
        }
        
        if let Some(entry1) = entries.get_mut(id1) {
            if !entry1.related_entries.contains(id2) {
                entry1.related_entries.push(id2.clone());
            }
        }
        
        if let Some(entry2) = entries.get_mut(id2) {
            if !entry2.related_entries.contains(id1) {
                entry2.related_entries.push(id1.clone());
            }
        }
        
        true
    }
    
    pub async fn get_related(&self, id: &KnowledgeId) -> Vec<KnowledgeEntry> {
        let entries = self.entries.read().await;
        
        if let Some(entry) = entries.get(id) {
            return entry.related_entries.iter()
                .filter_map(|related_id| entries.get(related_id).cloned())
                .collect();
        }
        
        vec![]
    }
    
    pub async fn promote_from_memory(
        &self,
        memory_content: &str,
        memory_type: &str,
        source_agent: &str,
        importance: f64,
    ) -> Option<KnowledgeId> {
        if memory_content.len() < self.config.min_content_length {
            return None;
        }
        
        if importance < self.config.importance_threshold {
            return None;
        }
        
        let category = self.infer_category(memory_type);
        let tags = self.extract_tags(memory_content);
        let title = self.generate_title(memory_content);
        
        Some(self.add_entry(
            &title,
            memory_content,
            category,
            tags,
            KnowledgeSource::MemoryPromotion,
            source_agent,
            importance,
        ).await)
    }
    
    fn infer_category(&self, memory_type: &str) -> KnowledgeCategory {
        match memory_type.to_lowercase().as_str() {
            "error" | "bug" | "fix" => KnowledgeCategory::Error,
            "solution" | "answer" => KnowledgeCategory::Solution,
            "pattern" | "design" => KnowledgeCategory::Pattern,
            "decision" | "consensus" => KnowledgeCategory::Project,
            "reference" | "doc" => KnowledgeCategory::Reference,
            _ => KnowledgeCategory::Technical,
        }
    }
    
    fn extract_tags(&self, content: &str) -> Vec<String> {
        let mut tags = Vec::new();
        
        let tech_keywords = [
            "rust", "python", "javascript", "typescript", "go", "c++",
            "api", "database", "cache", "async", "thread", "socket",
            "http", "rest", "grpc", "websocket", "mqtt",
            "sqlite", "postgres", "redis", "mongodb",
            "docker", "kubernetes", "linux", "macos", "windows",
        ];
        
        let content_lower = content.to_lowercase();
        for keyword in tech_keywords {
            if content_lower.contains(keyword) {
                tags.push(keyword.to_string());
            }
        }
        
        tags
    }
    
    fn generate_title(&self, content: &str) -> String {
        let first_line = content.lines().next().unwrap_or("");
        let title = first_line.chars().take(50).collect::<String>();
        
        if title.len() < 10 {
            format!("Knowledge {}", &Uuid::new_v4().to_string()[..8])
        } else {
            title.trim_end_matches(|c| c == '.' || c == ':' || c == '-').to_string()
        }
    }
    
    fn generate_summary(&self, content: &str) -> String {
        let sentences: Vec<&str> = content.split(|c| c == '.' || c == '!' || c == '?')
            .filter(|s| !s.trim().is_empty())
            .take(3)
            .collect();
        
        let summary = sentences.join(". ");
        
        if summary.len() > self.config.max_summary_length {
            format!("{}...", &summary[..self.config.max_summary_length - 3])
        } else {
            summary
        }
    }
    
    async fn invalidate_cache(&self, id: &KnowledgeId) {
        {
            let mut content_cache = self.content_cache.write().await;
            content_cache.pop(id);
        }
    }
    
    pub async fn get_stats(&self) -> KnowledgeBaseStats {
        let entries = self.entries.read().await;
        
        let mut by_category: HashMap<KnowledgeCategory, usize> = HashMap::new();
        let mut by_source: HashMap<KnowledgeSource, usize> = HashMap::new();
        
        for entry in entries.values() {
            *by_category.entry(entry.category.clone()).or_default() += 1;
            *by_source.entry(entry.source_type.clone()).or_default() += 1;
        }
        
        KnowledgeBaseStats {
            total_entries: entries.len(),
            by_category,
            by_source,
            total_access_count: entries.values().map(|e| e.access_count).sum(),
        }
    }
    
    pub async fn cleanup_unused(&self, days_threshold: i64) -> usize {
        let now = Utc::now();
        let mut entries = self.entries.write().await;
        
        let to_remove: Vec<KnowledgeId> = entries.iter()
            .filter(|(_, entry)| {
                let days_since_access = (now - entry.updated_at).num_seconds() / 86400;
                days_since_access > days_threshold && entry.access_count == 0
            })
            .map(|(id, _)| id.clone())
            .collect();
        
        let removed = to_remove.len();
        
        for id in &to_remove {
            entries.remove(id);
            
            let mut tag_index = self.tag_index.write().await;
            for (_, ids) in tag_index.iter_mut() {
                ids.retain(|i| i != id);
            }
            
            let mut category_index = self.category_index.write().await;
            for (_, ids) in category_index.iter_mut() {
                ids.retain(|i| i != id);
            }
        }
        
        removed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBaseStats {
    pub total_entries: usize,
    pub by_category: HashMap<KnowledgeCategory, usize>,
    pub by_source: HashMap<KnowledgeSource, usize>,
    pub total_access_count: u32,
}

pub struct MemoryKnowledgeCoordinator {
    knowledge_base: Arc<KnowledgeBase>,
    promotion_threshold: f64,
    auto_promote_interval_hours: i64,
    last_promotion: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl MemoryKnowledgeCoordinator {
    pub fn new(knowledge_base: Arc<KnowledgeBase>) -> Self {
        Self {
            knowledge_base,
            promotion_threshold: 0.6,
            auto_promote_interval_hours: 6,
            last_promotion: Arc::new(RwLock::new(None)),
        }
    }
    
    pub async fn check_and_promote(
        &self,
        memory_content: &str,
        memory_type: &str,
        source_agent: &str,
        importance: f64,
    ) -> Option<KnowledgeId> {
        if importance < self.promotion_threshold {
            return None;
        }
        
        self.knowledge_base.promote_from_memory(
            memory_content,
            memory_type,
            source_agent,
            importance,
        ).await
    }
    
    pub async fn build_knowledge_context(
        &self,
        query: &str,
        max_tokens: usize,
    ) -> String {
        let results = self.knowledge_base.search(
            query,
            None,
            None,
            5,
        ).await;
        
        let mut context = String::new();
        let mut current_tokens = 0;
        
        for result in results {
            let entry_text = format!(
                "## {}\n{}\n\n",
                result.entry.title,
                result.entry.summary
            );
            
            let tokens = entry_text.len() / 4;
            
            if current_tokens + tokens > max_tokens {
                break;
            }
            
            context.push_str(&entry_text);
            current_tokens += tokens;
        }
        
        context
    }
    
    pub async fn update_from_learning(
        &self,
        topic: &str,
        new_knowledge: &str,
        source_agent: &str,
    ) -> Option<KnowledgeId> {
        let existing = self.knowledge_base.search(topic, None, None, 1).await;
        
        if let Some(result) = existing.first() {
            let updated_content = format!(
                "{}\n\n---\n\nUpdated:\n{}",
                result.entry.content,
                new_knowledge
            );
            
            self.knowledge_base.update_entry(
                &result.entry.id,
                Some(&updated_content),
                None,
                None,
            ).await;
            
            Some(result.entry.id.clone())
        } else {
            Some(self.knowledge_base.add_entry(
                topic,
                new_knowledge,
                KnowledgeCategory::Technical,
                vec![],
                KnowledgeSource::LearningExtraction,
                source_agent,
                0.7,
            ).await)
        }
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
    
    #[tokio::test]
    async fn test_knowledge_base_add_entry() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        let id = kb.add_entry(
            "Test Knowledge",
            "This is a test knowledge entry with enough content to be valid for the knowledge base system.",
            KnowledgeCategory::Technical,
            vec!["test".to_string()],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.8,
        ).await;
        
        assert!(!id.as_str().is_empty());
        
        let entry = kb.get(&id).await;
        assert!(entry.is_some());
    }
    
    #[tokio::test]
    async fn test_knowledge_base_search() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        kb.add_entry(
            "Rust Programming",
            "Rust is a systems programming language focused on safety and performance.",
            KnowledgeCategory::Technical,
            vec!["rust".to_string(), "programming".to_string()],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.9,
        ).await;
        
        let results = kb.search("rust", None, None, 10).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.title, "Rust Programming");
    }
    
    #[tokio::test]
    async fn test_promote_from_memory() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        let long_content = "This is a very long memory content that should be promoted to the knowledge base because it contains important information about the system architecture and design decisions that were made during the development process. We need to ensure this content is long enough to meet the minimum content length requirement for promotion to the knowledge base system. This additional text ensures we exceed the 500 character threshold for knowledge promotion testing purposes. Adding more content to ensure we pass the test requirements for minimum content length validation.";
        
        let id = kb.promote_from_memory(
            long_content,
            "decision",
            "agent1",
            0.8,
        ).await;
        
        assert!(id.is_some());
    }
    
    #[tokio::test]
    async fn test_knowledge_base_stats() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        kb.add_entry(
            "Test 1",
            "Content 1 with enough length for the knowledge base validation requirements.",
            KnowledgeCategory::Technical,
            vec![],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.7,
        ).await;
        
        kb.add_entry(
            "Test 2",
            "Content 2 with enough length for the knowledge base validation requirements.",
            KnowledgeCategory::Pattern,
            vec![],
            KnowledgeSource::MemoryPromotion,
            "agent2",
            0.8,
        ).await;
        
        let stats = kb.get_stats().await;
        assert_eq!(stats.total_entries, 2);
    }
    
    #[tokio::test]
    async fn test_knowledge_base_get() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        let id = kb.add_entry(
            "Test Entry",
            "Content for testing get functionality with enough length.",
            KnowledgeCategory::Technical,
            vec![],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.7,
        ).await;
        
        let entry = kb.get(&id).await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().title, "Test Entry");
        
        let non_existent = kb.get(&KnowledgeId::new("non-existent")).await;
        assert!(non_existent.is_none());
    }
    
    #[tokio::test]
    async fn test_knowledge_base_update_entry() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        let id = kb.add_entry(
            "Original Title",
            "Original content with enough length for validation.",
            KnowledgeCategory::Technical,
            vec!["original".to_string()],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.7,
        ).await;
        
        let updated = kb.update_entry(
            &id,
            Some("Updated content with new information added for testing purposes."),
            Some(vec!["updated".to_string()]),
            Some(0.9),
        ).await;
        
        assert!(updated);
        
        let entry = kb.get(&id).await.unwrap();
        assert_eq!(entry.importance_score, 0.9);
    }
    
    #[tokio::test]
    async fn test_knowledge_base_link_entries() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        let id1 = kb.add_entry(
            "Entry 1",
            "First entry content with enough length for validation.",
            KnowledgeCategory::Technical,
            vec![],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.7,
        ).await;
        
        let id2 = kb.add_entry(
            "Entry 2",
            "Second entry content with enough length for validation.",
            KnowledgeCategory::Technical,
            vec![],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.7,
        ).await;
        
        let linked = kb.link_entries(&id1, &id2).await;
        assert!(linked);
        
        let related = kb.get_related(&id1).await;
        assert_eq!(related.len(), 1);
    }
    
    #[tokio::test]
    async fn test_knowledge_base_search_with_filters() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        kb.add_entry(
            "Rust Guide",
            "A comprehensive guide to Rust programming language.",
            KnowledgeCategory::Technical,
            vec!["rust".to_string()],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.9,
        ).await;
        
        kb.add_entry(
            "Python Tutorial",
            "A beginner friendly Python tutorial.",
            KnowledgeCategory::Reference,
            vec!["python".to_string()],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.8,
        ).await;
        
        let results = kb.search(
            "guide",
            Some(&[KnowledgeCategory::Technical]),
            None,
            10,
        ).await;
        assert_eq!(results.len(), 1);
        
        let tag_results = kb.search(
            "rust",
            None,
            Some(&["rust".to_string()]),
            10,
        ).await;
        assert_eq!(tag_results.len(), 1);
    }
    
    #[tokio::test]
    async fn test_knowledge_base_cleanup_unused() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        kb.add_entry(
            "Test Entry",
            "Content for cleanup test with enough length.",
            KnowledgeCategory::Technical,
            vec![],
            KnowledgeSource::AgentSummary,
            "agent1",
            0.7,
        ).await;
        
        let removed = kb.cleanup_unused(30).await;
        assert_eq!(removed, 0);
    }
    
    #[tokio::test]
    async fn test_memory_knowledge_coordinator() {
        let long_term = Arc::new(MockMemory);
        let kb = Arc::new(KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        ));
        
        let coordinator = MemoryKnowledgeCoordinator::new(kb.clone());
        
        let long_content = "This is a comprehensive memory content that should be promoted to the knowledge base because it contains important technical information about system design and architecture decisions made during development. We need enough content to meet the minimum length requirement for knowledge promotion testing. Adding more content to ensure we pass the minimum content length validation. This additional text provides more context and details about the system architecture, design patterns, and implementation decisions that were made during the development process.";
        
        let id = coordinator.check_and_promote(
            long_content,
            "technical",
            "agent1",
            0.8,
        ).await;
        
        assert!(id.is_some());
        
        let context = coordinator.build_knowledge_context("technical", 500).await;
        assert!(!context.is_empty());
    }
    
    #[tokio::test]
    async fn test_knowledge_id() {
        let id = KnowledgeId::new("test-id-123");
        assert_eq!(id.as_str(), "test-id-123");
    }
    
    #[tokio::test]
    async fn test_promote_from_memory_short_content() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        let short_content = "Short content";
        let id = kb.promote_from_memory(
            short_content,
            "decision",
            "agent1",
            0.8,
        ).await;
        
        assert!(id.is_none());
    }
    
    #[tokio::test]
    async fn test_promote_from_memory_low_importance() {
        let long_term = Arc::new(MockMemory);
        let kb = KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        );
        
        let long_content = "This is a very long memory content that should not be promoted because the importance score is too low even though the content length is sufficient for the knowledge base system requirements.";
        let id = kb.promote_from_memory(
            long_content,
            "decision",
            "agent1",
            0.3,
        ).await;
        
        assert!(id.is_none());
    }
    
    #[tokio::test]
    async fn test_update_from_learning() {
        let long_term = Arc::new(MockMemory);
        let kb = Arc::new(KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        ));
        
        let coordinator = MemoryKnowledgeCoordinator::new(kb);
        
        let id = coordinator.update_from_learning(
            "Rust Programming",
            "Rust is a systems programming language focused on safety and performance. It provides memory safety without garbage collection and supports zero-cost abstractions. This knowledge was learned from practical experience.",
            "agent1",
        ).await;
        
        assert!(id.is_some());
    }
}
