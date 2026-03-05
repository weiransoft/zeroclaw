use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

const MAX_SHORT_TERM_ENTRIES: usize = 100;
const MAX_MEDIUM_TERM_ENTRIES: usize = 500;
const SHORT_TERM_TTL_SECONDS: i64 = 3600;
const MEDIUM_TERM_TTL_HOURS: i64 = 24;
const LONG_TERM_SYNC_INTERVAL_HOURS: i64 = 6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryDuration {
    Short,
    Medium,
    Long,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryScope {
    Private,
    Session,
    Workspace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub duration: MemoryDuration,
    pub scope: MemoryScope,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u32,
    pub importance_score: f64,
    pub tags: Vec<String>,
    pub source_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryType {
    Conversation,
    Decision,
    Finding,
    Preference,
    Fact,
    TaskProgress,
    ToolUsage,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub short_term_count: usize,
    pub medium_term_count: usize,
    pub long_term_pending: usize,
    pub last_cleanup: Option<DateTime<Utc>>,
    pub last_long_term_sync: Option<DateTime<Utc>>,
}

pub struct HierarchicalMemory {
    workspace_dir: PathBuf,
    agent_id: String,
    session_id: Option<String>,
    
    short_term: Arc<RwLock<Vec<MemoryItem>>>,
    medium_term: Arc<RwLock<HashMap<String, MemoryItem>>>,
    
    long_term_memory: Arc<dyn crate::memory::Memory>,
    
    stats: Arc<RwLock<MemoryStats>>,
    config: MemoryHierarchyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHierarchyConfig {
    pub max_short_term_entries: usize,
    pub max_medium_term_entries: usize,
    pub short_term_ttl_seconds: i64,
    pub medium_term_ttl_hours: i64,
    pub long_term_sync_interval_hours: i64,
    pub importance_threshold_for_long_term: f64,
    pub auto_cleanup_enabled: bool,
    pub auto_sync_enabled: bool,
}

impl Default for MemoryHierarchyConfig {
    fn default() -> Self {
        Self {
            max_short_term_entries: MAX_SHORT_TERM_ENTRIES,
            max_medium_term_entries: MAX_MEDIUM_TERM_ENTRIES,
            short_term_ttl_seconds: SHORT_TERM_TTL_SECONDS,
            medium_term_ttl_hours: MEDIUM_TERM_TTL_HOURS,
            long_term_sync_interval_hours: LONG_TERM_SYNC_INTERVAL_HOURS,
            importance_threshold_for_long_term: 0.7,
            auto_cleanup_enabled: true,
            auto_sync_enabled: true,
        }
    }
}

impl HierarchicalMemory {
    pub fn new(
        workspace_dir: PathBuf,
        agent_id: String,
        session_id: Option<String>,
        long_term_memory: Arc<dyn crate::memory::Memory>,
        config: MemoryHierarchyConfig,
    ) -> Self {
        Self {
            workspace_dir,
            agent_id,
            session_id,
            short_term: Arc::new(RwLock::new(Vec::new())),
            medium_term: Arc::new(RwLock::new(HashMap::new())),
            long_term_memory,
            stats: Arc::new(RwLock::new(MemoryStats {
                short_term_count: 0,
                medium_term_count: 0,
                long_term_pending: 0,
                last_cleanup: None,
                last_long_term_sync: None,
            })),
            config,
        }
    }
    
    pub async fn store(
        &self,
        content: &str,
        memory_type: MemoryType,
        duration: MemoryDuration,
        scope: MemoryScope,
        importance: f64,
        tags: Vec<String>,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        
        let item = MemoryItem {
            id: id.clone(),
            content: content.to_string(),
            memory_type: memory_type.clone(),
            duration: duration.clone(),
            scope,
            agent_id: self.agent_id.clone(),
            session_id: self.session_id.clone(),
            created_at: now,
            last_accessed: now,
            access_count: 0,
            importance_score: importance,
            tags,
            source_agent: None,
        };
        
        match duration {
            MemoryDuration::Short => {
                let mut short_term = self.short_term.write().await;
                short_term.push(item);
                
                if short_term.len() > self.config.max_short_term_entries {
                    self.evict_short_term(&mut short_term).await;
                }
            }
            MemoryDuration::Medium => {
                let mut medium_term = self.medium_term.write().await;
                medium_term.insert(id.clone(), item);
                
                if medium_term.len() > self.config.max_medium_term_entries {
                    self.evict_medium_term(&mut medium_term).await;
                }
            }
            MemoryDuration::Long => {
                let category = self.memory_type_to_category(&memory_type);
                let _ = self.long_term_memory.store(&id, content, category).await;
            }
        }
        
        self.update_stats().await;
        id
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
    
    pub async fn recall(
        &self,
        query: &str,
        limit: usize,
        include_short: bool,
        include_medium: bool,
        include_long: bool,
    ) -> Vec<MemoryItem> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        
        if include_short {
            let short_term = self.short_term.read().await;
            for item in short_term.iter().rev() {
                if self.matches_query(&item, &query_lower) {
                    results.push(item.clone());
                }
                if results.len() >= limit {
                    return results;
                }
            }
        }
        
        if include_medium && results.len() < limit {
            let medium_term = self.medium_term.read().await;
            let mut medium_items: Vec<_> = medium_term.values()
                .filter(|item| self.matches_query(item, &query_lower))
                .collect();
            medium_items.sort_by(|a, b| 
                        b.importance_score.partial_cmp(&a.importance_score).unwrap_or(std::cmp::Ordering::Equal)
            );
            
            for item in medium_items.into_iter().take(limit - results.len()) {
                results.push(item.clone());
            }
        }
        
        if include_long && results.len() < limit {
            if let Ok(long_results) = self.long_term_memory.recall(query, limit - results.len()).await {
                for entry in long_results {
                    results.push(MemoryItem {
                        id: entry.id,
                        content: entry.content,
                        memory_type: MemoryType::Fact,
                        duration: MemoryDuration::Long,
                        scope: MemoryScope::Workspace,
                        agent_id: self.agent_id.clone(),
                        session_id: entry.session_id,
                        created_at: Utc::now(),
                        last_accessed: Utc::now(),
                        access_count: 0,
                        importance_score: entry.score.unwrap_or(0.5),
                        tags: vec![],
                        source_agent: None,
                    });
                }
            }
        }
        
        results
    }
    
    fn matches_query(&self, item: &MemoryItem, query_lower: &str) -> bool {
        item.content.to_lowercase().contains(query_lower) ||
        item.tags.iter().any(|t| t.to_lowercase().contains(query_lower))
    }
    
    async fn evict_short_term(&self, short_term: &mut Vec<MemoryItem>) {
        let now = Utc::now();
        let ttl = Duration::seconds(self.config.short_term_ttl_seconds);
        
        short_term.retain(|item| {
            now - item.last_accessed < ttl
        });
        
        while short_term.len() > self.config.max_short_term_entries {
            if let Some(promoted) = short_term.first() {
                if promoted.importance_score >= self.config.importance_threshold_for_long_term {
                    let _ = self.long_term_memory.store(
                        &promoted.id,
                        &promoted.content,
                        self.memory_type_to_category(&promoted.memory_type),
                    ).await;
                }
            }
            short_term.remove(0);
        }
    }
    
    async fn evict_medium_term(&self, medium_term: &mut HashMap<String, MemoryItem>) {
        let now = Utc::now();
        let ttl = Duration::hours(self.config.medium_term_ttl_hours);
        
        let to_remove: Vec<String> = medium_term.iter()
            .filter(|(_, item)| now - item.last_accessed > ttl)
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in to_remove {
            if let Some(item) = medium_term.remove(&id) {
                if item.importance_score >= self.config.importance_threshold_for_long_term {
                    let _ = self.long_term_memory.store(
                        &item.id,
                        &item.content,
                        self.memory_type_to_category(&item.memory_type),
                    ).await;
                }
            }
        }
    }
    
    pub async fn cleanup(&self) {
        if !self.config.auto_cleanup_enabled {
            return;
        }
        
        {
            let mut short_term = self.short_term.write().await;
            self.evict_short_term(&mut short_term).await;
        }
        
        {
            let mut medium_term = self.medium_term.write().await;
            self.evict_medium_term(&mut medium_term).await;
        }
        
        let mut stats = self.stats.write().await;
        stats.last_cleanup = Some(Utc::now());
    }
    
    pub async fn sync_to_long_term(&self) -> anyhow::Result<usize> {
        if !self.config.auto_sync_enabled {
            return Ok(0);
        }
        
        let mut synced = 0;
        let threshold = self.config.importance_threshold_for_long_term;
        
        {
            let medium_term = self.medium_term.read().await;
            for item in medium_term.values() {
                if item.importance_score >= threshold && item.scope == MemoryScope::Workspace {
                    self.long_term_memory.store(
                        &item.id,
                        &item.content,
                        self.memory_type_to_category(&item.memory_type),
                    ).await?;
                    synced += 1;
                }
            }
        }
        
        let mut stats = self.stats.write().await;
        stats.last_long_term_sync = Some(Utc::now());
        stats.long_term_pending = 0;
        
        Ok(synced)
    }
    
    pub async fn promote_to_long_term(&self, item_id: &str) -> anyhow::Result<bool> {
        let medium_term = self.medium_term.read().await;
        if let Some(item) = medium_term.get(item_id) {
            self.long_term_memory.store(
                &item.id,
                &item.content,
                self.memory_type_to_category(&item.memory_type),
            ).await?;
            return Ok(true);
        }
        Ok(false)
    }
    
    async fn update_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.short_term_count = self.short_term.read().await.len();
        stats.medium_term_count = self.medium_term.read().await.len();
    }
    
    pub async fn get_stats(&self) -> MemoryStats {
        self.stats.read().await.clone()
    }
    
    pub async fn clear_short_term(&self) {
        let mut short_term = self.short_term.write().await;
        short_term.clear();
        self.update_stats().await;
    }
    
    pub async fn clear_session_memories(&self, session_id: &str) {
        let mut medium_term = self.medium_term.write().await;
        medium_term.retain(|_, item| item.session_id.as_deref() != Some(session_id));
        self.update_stats().await;
    }
    
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
    
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

pub struct SwarmMemoryCoordinator {
    workspace_dir: PathBuf,
    session_id: Uuid,
    agent_memories: Arc<RwLock<HashMap<String, Arc<HierarchicalMemory>>>>,
    shared_findings: Arc<RwLock<Vec<SharedFinding>>>,
    consensus_decisions: Arc<RwLock<Vec<ConsensusDecision>>>,
    long_term_memory: Arc<dyn crate::memory::Memory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFinding {
    pub id: String,
    pub finding_type: String,
    pub content: String,
    pub source_agent: String,
    pub relevance_score: f64,
    pub timestamp: DateTime<Utc>,
    pub acknowledged_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusDecision {
    pub id: String,
    pub decision: String,
    pub rationale: String,
    pub proposer: String,
    pub voters: Vec<(String, bool)>,
    pub consensus_reached: bool,
    pub timestamp: DateTime<Utc>,
}

impl SwarmMemoryCoordinator {
    pub fn new(
        workspace_dir: PathBuf,
        session_id: Uuid,
        long_term_memory: Arc<dyn crate::memory::Memory>,
    ) -> Self {
        Self {
            workspace_dir,
            session_id,
            agent_memories: Arc::new(RwLock::new(HashMap::new())),
            shared_findings: Arc::new(RwLock::new(Vec::new())),
            consensus_decisions: Arc::new(RwLock::new(Vec::new())),
            long_term_memory,
        }
    }
    
    pub async fn register_agent(&self, agent_id: String) -> Arc<HierarchicalMemory> {
        let config = MemoryHierarchyConfig::default();
        let memory = Arc::new(HierarchicalMemory::new(
            self.workspace_dir.clone(),
            agent_id.clone(),
            Some(self.session_id.to_string()),
            self.long_term_memory.clone(),
            config,
        ));
        
        let mut agent_memories = self.agent_memories.write().await;
        agent_memories.insert(agent_id, memory.clone());
        memory
    }
    
    pub async fn share_finding(
        &self,
        finding_type: &str,
        content: &str,
        source_agent: &str,
        relevance: f64,
    ) -> String {
        let finding = SharedFinding {
            id: Uuid::new_v4().to_string(),
            finding_type: finding_type.to_string(),
            content: content.to_string(),
            source_agent: source_agent.to_string(),
            relevance_score: relevance,
            timestamp: Utc::now(),
            acknowledged_by: vec![source_agent.to_string()],
        };
        
        let id = finding.id.clone();
        let mut shared = self.shared_findings.write().await;
        shared.push(finding);
        
        shared.sort_by(|a, b| 
            b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal)
        );
        shared.truncate(50);
        
        id
    }
    
    pub async fn acknowledge_finding(&self, finding_id: &str, agent_id: &str) {
        let mut shared = self.shared_findings.write().await;
        if let Some(finding) = shared.iter_mut().find(|f| f.id == finding_id) {
            if !finding.acknowledged_by.contains(&agent_id.to_string()) {
                finding.acknowledged_by.push(agent_id.to_string());
            }
        }
    }
    
    pub async fn propose_decision(
        &self,
        decision: &str,
        rationale: &str,
        proposer: &str,
    ) -> String {
        let consensus = ConsensusDecision {
            id: Uuid::new_v4().to_string(),
            decision: decision.to_string(),
            rationale: rationale.to_string(),
            proposer: proposer.to_string(),
            voters: vec![(proposer.to_string(), true)],
            consensus_reached: false,
            timestamp: Utc::now(),
        };
        
        let id = consensus.id.clone();
        let mut decisions = self.consensus_decisions.write().await;
        decisions.push(consensus);
        id
    }
    
    pub async fn vote_decision(&self, decision_id: &str, agent_id: &str, approve: bool) -> bool {
        let mut decisions = self.consensus_decisions.write().await;
        if let Some(decision) = decisions.iter_mut().find(|d| d.id == decision_id) {
            decision.voters.push((agent_id.to_string(), approve));
            
            let total_agents = {
                let agent_memories = self.agent_memories.read().await;
                agent_memories.len()
            };
            
            let approve_count = decision.voters.iter().filter(|(_, v)| *v).count();
            let reject_count = decision.voters.iter().filter(|(_, v)| !*v).count();
            
            if approve_count * 2 > total_agents {
                decision.consensus_reached = true;
                
                let _ = self.long_term_memory.store(
                    &decision.id,
                    &format!("Decision: {}\nRationale: {}", decision.decision, decision.rationale),
                    crate::memory::MemoryCategory::Core,
                ).await;
                
                return true;
            }
            
            if reject_count * 2 > total_agents {
                return false;
            }
        }
        false
    }
    
    pub async fn get_shared_findings(&self, limit: usize) -> Vec<SharedFinding> {
        let shared = self.shared_findings.read().await;
        shared.iter().take(limit).cloned().collect()
    }
    
    pub async fn get_pending_decisions(&self) -> Vec<ConsensusDecision> {
        let decisions = self.consensus_decisions.read().await;
        decisions.iter()
            .filter(|d| !d.consensus_reached)
            .cloned()
            .collect()
    }
    
    pub async fn broadcast_to_agents(&self, message_type: &str, content: &str, exclude_agent: Option<&str>) {
        let agent_memories = self.agent_memories.read().await;
        for (agent_id, memory) in agent_memories.iter() {
            if exclude_agent.map_or(true, |ex| agent_id != ex) {
                memory.store(
                    content,
                    MemoryType::Finding,
                    MemoryDuration::Medium,
                    MemoryScope::Session,
                    0.5,
                    vec![message_type.to_string()],
                ).await;
            }
        }
    }
    
    pub async fn cleanup_session(&self) {
        let agent_memories = self.agent_memories.read().await;
        for memory in agent_memories.values() {
            memory.clear_short_term().await;
            memory.sync_to_long_term().await.ok();
        }
    }
    
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryCategory;
    
    struct MockMemory;
    
    #[async_trait::async_trait]
    impl crate::memory::Memory for MockMemory {
        fn name(&self) -> &str { "mock" }
        async fn store(&self, _key: &str, _content: &str, _category: MemoryCategory) -> anyhow::Result<()> {
            Ok(())
        }
        async fn recall(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<crate::memory::MemoryEntry>> {
            Ok(vec![])
        }
        async fn get(&self, _key: &str) -> anyhow::Result<Option<crate::memory::MemoryEntry>> {
            Ok(None)
        }
        async fn list(&self, _category: Option<&MemoryCategory>) -> anyhow::Result<Vec<crate::memory::MemoryEntry>> {
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
    async fn test_hierarchical_memory_store() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        let id = memory.store(
            "Test content",
            MemoryType::Conversation,
            MemoryDuration::Short,
            MemoryScope::Private,
            0.5,
            vec!["test".to_string()],
        ).await;
        
        assert!(!id.is_empty());
        
        let stats = memory.get_stats().await;
        assert_eq!(stats.short_term_count, 1);
    }
    
    #[tokio::test]
    async fn test_hierarchical_memory_recall() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        memory.store(
            "Important decision made",
            MemoryType::Decision,
            MemoryDuration::Medium,
            MemoryScope::Session,
            0.8,
            vec!["decision".to_string()],
        ).await;
        
        let results = memory.recall("decision", 10, false, true, false).await;
        assert_eq!(results.len(), 1);
    }
    
    #[tokio::test]
    async fn test_swarm_memory_coordinator() {
        let long_term = Arc::new(MockMemory);
        let coordinator = SwarmMemoryCoordinator::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
            long_term,
        );
        
        let agent1_memory = coordinator.register_agent("agent1".to_string()).await;
        assert_eq!(agent1_memory.agent_id(), "agent1");
        
        let finding_id = coordinator.share_finding(
            "CodePattern",
            "Found singleton pattern",
            "agent1",
            0.9,
        ).await;
        
        assert!(!finding_id.is_empty());
        
        let findings = coordinator.get_shared_findings(10).await;
        assert_eq!(findings.len(), 1);
    }
    
    #[tokio::test]
    async fn test_consensus_decision() {
        let long_term = Arc::new(MockMemory);
        let coordinator = SwarmMemoryCoordinator::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
            long_term,
        );
        
        coordinator.register_agent("agent1".to_string()).await;
        coordinator.register_agent("agent2".to_string()).await;
        coordinator.register_agent("agent3".to_string()).await;
        
        let decision_id = coordinator.propose_decision(
            "Use Rust for implementation",
            "Performance and safety",
            "agent1",
        ).await;
        
        let pending = coordinator.get_pending_decisions().await;
        assert_eq!(pending.len(), 1);
        
        coordinator.vote_decision(&decision_id, "agent2", true).await;
        let reached = coordinator.vote_decision(&decision_id, "agent3", true).await;
        
        assert!(reached);
    }
    
    #[tokio::test]
    async fn test_hierarchical_memory_cleanup() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        memory.store(
            "Test content",
            MemoryType::Conversation,
            MemoryDuration::Short,
            MemoryScope::Private,
            0.5,
            vec![],
        ).await;
        
        memory.cleanup().await;
        
        let stats = memory.get_stats().await;
        assert!(stats.last_cleanup.is_some());
    }
    
    #[tokio::test]
    async fn test_sync_to_long_term() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        memory.store(
            "Important decision",
            MemoryType::Decision,
            MemoryDuration::Medium,
            MemoryScope::Workspace,
            0.8,
            vec![],
        ).await;
        
        let synced = memory.sync_to_long_term().await.unwrap();
        assert!(synced == 1);
    }
    
    #[tokio::test]
    async fn test_promote_to_long_term() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        let id = memory.store(
            "Decision to promote",
            MemoryType::Decision,
            MemoryDuration::Medium,
            MemoryScope::Workspace,
            0.9,
            vec![],
        ).await;
        
        let promoted = memory.promote_to_long_term(&id).await.unwrap();
        assert!(promoted);
    }
    
    #[tokio::test]
    async fn test_clear_short_term() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        memory.store(
            "Short term content",
            MemoryType::Conversation,
            MemoryDuration::Short,
            MemoryScope::Private,
            0.5,
            vec![],
        ).await;
        
        memory.clear_short_term().await;
        
        let stats = memory.get_stats().await;
        assert_eq!(stats.short_term_count, 0);
    }
    
    #[tokio::test]
    async fn test_clear_session_memories() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        memory.store(
            "Session content",
            MemoryType::Conversation,
            MemoryDuration::Medium,
            MemoryScope::Session,
            0.5,
            vec![],
        ).await;
        
        memory.clear_session_memories("session1").await;
        
        let stats = memory.get_stats().await;
        assert_eq!(stats.medium_term_count, 0);
    }
    
    #[tokio::test]
    async fn test_agent_id_and_session_id() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "test_agent".to_string(),
            Some("test_session".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        assert_eq!(memory.agent_id(), "test_agent");
        assert_eq!(memory.session_id(), Some("test_session"));
    }
    
    #[tokio::test]
    async fn test_share_finding_acknowledge() {
        let long_term = Arc::new(MockMemory);
        let coordinator = SwarmMemoryCoordinator::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
            long_term,
        );
        
        coordinator.register_agent("agent1".to_string()).await;
        coordinator.register_agent("agent2".to_string()).await;
        
        let finding_id = coordinator.share_finding(
            "Error",
            "Found critical bug",
            "agent1",
            0.9,
        ).await;
        
        coordinator.acknowledge_finding(&finding_id, "agent2").await;
        
        let findings = coordinator.get_shared_findings(10).await;
        assert_eq!(findings[0].acknowledged_by.len(), 2);
    }
    
    #[tokio::test]
    async fn test_broadcast_to_agents() {
        let long_term = Arc::new(MockMemory);
        let coordinator = SwarmMemoryCoordinator::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
            long_term,
        );
        
        coordinator.register_agent("agent1".to_string()).await;
        coordinator.register_agent("agent2".to_string()).await;
        
        coordinator.broadcast_to_agents("notification", "Test broadcast", Some("agent1")).await;
    }
    
    #[tokio::test]
    async fn test_cleanup_session() {
        let long_term = Arc::new(MockMemory);
        let coordinator = SwarmMemoryCoordinator::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
            long_term,
        );
        
        coordinator.register_agent("agent1".to_string()).await;
        coordinator.cleanup_session().await;
    }
    
    #[tokio::test]
    async fn test_session_id_method() {
        let long_term = Arc::new(MockMemory);
        let session_id = Uuid::new_v4();
        let coordinator = SwarmMemoryCoordinator::new(
            PathBuf::from("/tmp/test"),
            session_id,
            long_term,
        );
        
        assert_eq!(coordinator.session_id(), session_id);
    }
    
    #[tokio::test]
    async fn test_recall_with_long_term() {
        let long_term = Arc::new(MockMemory);
        let memory = HierarchicalMemory::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            Some("session1".to_string()),
            long_term,
            MemoryHierarchyConfig::default(),
        );
        
        let results = memory.recall("test", 10, true, true, true).await;
        assert!(results.len() <= 10);
    }
    
    #[tokio::test]
    async fn test_vote_decision_reject() {
        let long_term = Arc::new(MockMemory);
        let coordinator = SwarmMemoryCoordinator::new(
            PathBuf::from("/tmp/test"),
            Uuid::new_v4(),
            long_term,
        );
        
        coordinator.register_agent("agent1".to_string()).await;
        coordinator.register_agent("agent2".to_string()).await;
        coordinator.register_agent("agent3".to_string()).await;
        
        let decision_id = coordinator.propose_decision(
            "Bad decision",
            "Should be rejected",
            "agent1",
        ).await;
        
        coordinator.vote_decision(&decision_id, "agent2", false).await;
        let reached = coordinator.vote_decision(&decision_id, "agent3", false).await;
        
        assert!(!reached);
    }
}
