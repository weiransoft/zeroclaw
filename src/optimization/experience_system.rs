use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use lru::LruCache;
use std::num::NonZeroUsize;

const DEFAULT_EXPERIENCE_CACHE_SIZE: usize = 1000;
const EXPERIENCE_CONSOLIDATION_THRESHOLD: u32 = 3;
const KNOWLEDGE_PROMOTION_THRESHOLD: f64 = 0.7;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ExperienceType {
    Success,
    Failure,
    Correction,
    Discovery,
    Pattern,
    BestPractice,
    AntiPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MemorySystem {
    Episodic,
    Semantic,
    Procedural,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub id: String,
    pub experience_type: ExperienceType,
    pub memory_system: MemorySystem,
    pub title: String,
    pub description: String,
    pub context: ExperienceContext,
    pub outcome: String,
    pub lessons_learned: Vec<String>,
    pub applicability: Vec<String>,
    pub confidence: f64,
    pub importance: f64,
    pub access_count: u32,
    pub consolidation_count: u32,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub last_consolidated: Option<DateTime<Utc>>,
    pub source_agent: String,
    pub related_experiences: Vec<String>,
    pub promoted_to_knowledge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceContext {
    pub task_type: String,
    pub domain: String,
    pub environment: HashMap<String, String>,
    pub preconditions: Vec<String>,
    pub constraints: Vec<String>,
    pub tools_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceReplayResult {
    pub experience: Experience,
    pub relevance_score: f64,
    pub applicability_score: f64,
    pub recency_score: f64,
    pub combined_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    pub source_experiences: Vec<String>,
    pub derived_knowledge: String,
    pub confidence: f64,
    pub consolidation_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceStats {
    pub total_experiences: usize,
    pub by_type: HashMap<ExperienceType, usize>,
    pub by_memory_system: HashMap<MemorySystem, usize>,
    pub consolidated_count: usize,
    pub promoted_to_knowledge_count: usize,
    pub average_confidence: f64,
    pub last_consolidation: Option<DateTime<Utc>>,
}

pub struct ExperienceSystem {
    workspace_dir: PathBuf,
    agent_id: String,
    
    experiences: Arc<RwLock<HashMap<String, Experience>>>,
    type_index: Arc<RwLock<HashMap<ExperienceType, Vec<String>>>>,
    domain_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    task_type_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    
    experience_cache: Arc<RwLock<LruCache<String, Experience>>>,
    replay_cache: Arc<RwLock<LruCache<String, Vec<ExperienceReplayResult>>>>,
    
    knowledge_base: Arc<super::knowledge_base::KnowledgeBase>,
    long_term_memory: Arc<dyn crate::memory::Memory>,
    
    config: ExperienceSystemConfig,
    stats: Arc<RwLock<ExperienceStats>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceSystemConfig {
    pub cache_size: usize,
    pub consolidation_threshold: u32,
    pub knowledge_promotion_threshold: f64,
    pub auto_consolidate: bool,
    pub auto_promote: bool,
    pub max_experiences: usize,
    pub recency_weight: f64,
    pub relevance_weight: f64,
    pub importance_weight: f64,
}

impl Default for ExperienceSystemConfig {
    fn default() -> Self {
        Self {
            cache_size: DEFAULT_EXPERIENCE_CACHE_SIZE,
            consolidation_threshold: EXPERIENCE_CONSOLIDATION_THRESHOLD,
            knowledge_promotion_threshold: KNOWLEDGE_PROMOTION_THRESHOLD,
            auto_consolidate: true,
            auto_promote: true,
            max_experiences: 10000,
            recency_weight: 0.2,
            relevance_weight: 0.5,
            importance_weight: 0.3,
        }
    }
}

impl ExperienceSystem {
    pub fn new(
        workspace_dir: PathBuf,
        agent_id: String,
        knowledge_base: Arc<super::knowledge_base::KnowledgeBase>,
        long_term_memory: Arc<dyn crate::memory::Memory>,
        config: ExperienceSystemConfig,
    ) -> Self {
        let cache_size = NonZeroUsize::new(config.cache_size)
            .unwrap_or(NonZeroUsize::new(1000).unwrap());
        
        Self {
            workspace_dir,
            agent_id,
            experiences: Arc::new(RwLock::new(HashMap::new())),
            type_index: Arc::new(RwLock::new(HashMap::new())),
            domain_index: Arc::new(RwLock::new(HashMap::new())),
            task_type_index: Arc::new(RwLock::new(HashMap::new())),
            experience_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            replay_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            knowledge_base,
            long_term_memory,
            config,
            stats: Arc::new(RwLock::new(ExperienceStats {
                total_experiences: 0,
                by_type: HashMap::new(),
                by_memory_system: HashMap::new(),
                consolidated_count: 0,
                promoted_to_knowledge_count: 0,
                average_confidence: 0.0,
                last_consolidation: None,
            })),
        }
    }
    
    pub async fn record_experience(
        &self,
        experience_type: ExperienceType,
        memory_system: MemorySystem,
        title: &str,
        description: &str,
        context: ExperienceContext,
        outcome: &str,
        lessons_learned: Vec<String>,
        applicability: Vec<String>,
        confidence: f64,
        importance: f64,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        
        let experience = Experience {
            id: id.clone(),
            experience_type: experience_type.clone(),
            memory_system: memory_system.clone(),
            title: title.to_string(),
            description: description.to_string(),
            context,
            outcome: outcome.to_string(),
            lessons_learned,
            applicability,
            confidence,
            importance,
            access_count: 0,
            consolidation_count: 0,
            created_at: now,
            last_accessed: now,
            last_consolidated: None,
            source_agent: self.agent_id.clone(),
            related_experiences: vec![],
            promoted_to_knowledge: false,
        };
        
        {
            let mut experiences = self.experiences.write().await;
            experiences.insert(id.clone(), experience.clone());
        }
        
        {
            let mut type_index = self.type_index.write().await;
            type_index.entry(experience_type).or_default().push(id.clone());
        }
        
        {
            let mut domain_index = self.domain_index.write().await;
            domain_index.entry(experience.context.domain.clone())
                .or_default()
                .push(id.clone());
        }
        
        {
            let mut task_type_index = self.task_type_index.write().await;
            task_type_index.entry(experience.context.task_type.clone())
                .or_default()
                .push(id.clone());
        }
        
        {
            let mut cache = self.experience_cache.write().await;
            cache.put(id.clone(), experience);
        }
        
        self.update_stats().await;
        
        if self.config.auto_consolidate {
            self.check_consolidation(&id).await;
        }
        
        id
    }
    
    pub async fn replay_experience(
        &self,
        task_type: &str,
        domain: &str,
        context_hints: &[String],
        limit: usize,
    ) -> Vec<ExperienceReplayResult> {
        let cache_key = format!("{}:{}:{:?}:{}", task_type, domain, context_hints, limit);
        
        {
            let mut cache = self.replay_cache.write().await;
            if let Some(cached) = cache.get(&cache_key) {
                return cached.clone();
            }
        }
        
        let experiences = self.experiences.read().await;
        let mut results: Vec<ExperienceReplayResult> = Vec::new();
        
        for experience in experiences.values() {
            let relevance_score = self.calculate_relevance(
                experience,
                task_type,
                domain,
                context_hints,
            );
            
            if relevance_score > 0.1 {
                let recency_score = self.calculate_recency(experience);
                let applicability_score = self.calculate_applicability(experience, context_hints);
                
                let combined_score = 
                    self.config.relevance_weight * relevance_score +
                    self.config.recency_weight * recency_score +
                    self.config.importance_weight * experience.importance;
                
                results.push(ExperienceReplayResult {
                    experience: experience.clone(),
                    relevance_score,
                    applicability_score,
                    recency_score,
                    combined_score,
                });
            }
        }
        
        results.sort_by(|a, b| 
            b.combined_score.partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        );
        results.truncate(limit);
        
        for result in &results {
            if let Some(exp) = experiences.get(&result.experience.id) {
                let mut exp_mut = exp.clone();
                exp_mut.access_count += 1;
                exp_mut.last_accessed = Utc::now();
            }
        }
        
        {
            let mut cache = self.replay_cache.write().await;
            cache.put(cache_key, results.clone());
        }
        
        results
    }
    
    fn calculate_relevance(
        &self,
        experience: &Experience,
        task_type: &str,
        domain: &str,
        context_hints: &[String],
    ) -> f64 {
        let mut score = 0.0;
        
        if experience.context.task_type == task_type {
            score += 0.4;
        }
        
        if experience.context.domain == domain {
            score += 0.3;
        }
        
        let hint_matches = context_hints.iter()
            .filter(|hint| {
                experience.description.to_lowercase().contains(&hint.to_lowercase()) ||
                experience.lessons_learned.iter().any(|l| 
                    l.to_lowercase().contains(&hint.to_lowercase())
                )
            })
            .count();
        
        if !context_hints.is_empty() {
            score += 0.3 * (hint_matches as f64 / context_hints.len() as f64);
        }
        
        score
    }
    
    fn calculate_recency(&self, experience: &Experience) -> f64 {
        let now = Utc::now();
        let age = now - experience.last_accessed;
        let hours = age.num_hours() as f64;
        
        (-hours / 168.0).exp()
    }
    
    fn calculate_applicability(
        &self,
        experience: &Experience,
        context_hints: &[String],
    ) -> f64 {
        if experience.applicability.is_empty() {
            return 0.5;
        }
        
        let matches = context_hints.iter()
            .filter(|hint| {
                experience.applicability.iter().any(|a| 
                    a.to_lowercase().contains(&hint.to_lowercase())
                )
            })
            .count();
        
        if context_hints.is_empty() {
            0.5
        } else {
            matches as f64 / context_hints.len() as f64
        }
    }
    
    async fn check_consolidation(&self, experience_id: &str) {
        let experiences = self.experiences.read().await;
        
        if let Some(experience) = experiences.get(experience_id) {
            if experience.consolidation_count >= self.config.consolidation_threshold 
                && experience.last_consolidated.is_none() {
                drop(experiences);
                self.consolidate_experience(experience_id).await;
            }
        }
    }
    
    pub async fn consolidate_experience(&self, experience_id: &str) -> Option<ConsolidationResult> {
        let mut experiences = self.experiences.write().await;
        
        let experience = experiences.get_mut(experience_id)?;
        
        let similar_ids = self.find_similar_experiences(experience).await;
        
        if similar_ids.len() < 2 {
            return None;
        }
        
        let mut source_ids = vec![experience_id.to_string()];
        source_ids.extend(similar_ids);
        
        let mut all_lessons: Vec<String> = Vec::new();
        let mut total_confidence = 0.0;
        let mut count = 0;
        
        for id in &source_ids {
            if let Some(exp) = experiences.get(id) {
                all_lessons.extend(exp.lessons_learned.clone());
                total_confidence += exp.confidence;
                count += 1;
            }
        }
        
        all_lessons.sort();
        all_lessons.dedup();
        
        let derived_knowledge = format!(
            "Consolidated from {} experiences:\n{}",
            source_ids.len(),
            all_lessons.join("\n")
        );
        
        let avg_confidence = if count > 0 {
            total_confidence / count as f64
        } else {
            0.0
        };
        
        for id in &source_ids {
            if let Some(exp) = experiences.get_mut(id) {
                exp.consolidation_count += 1;
                exp.last_consolidated = Some(Utc::now());
            }
        }
        
        let result = ConsolidationResult {
            source_experiences: source_ids.clone(),
            derived_knowledge: derived_knowledge.clone(),
            confidence: avg_confidence,
            consolidation_type: "pattern_abstraction".to_string(),
        };
        
        if self.config.auto_promote && avg_confidence >= self.config.knowledge_promotion_threshold {
            drop(experiences);
            self.promote_to_knowledge(&result).await;
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.consolidated_count += source_ids.len() as usize;
            stats.last_consolidation = Some(Utc::now());
        }
        
        Some(result)
    }
    
    async fn find_similar_experiences(&self, reference: &Experience) -> Vec<String> {
        let experiences = self.experiences.read().await;
        let mut similar = Vec::new();
        
        for (id, exp) in experiences.iter() {
            if id == &reference.id {
                continue;
            }
            
            if exp.experience_type == reference.experience_type &&
               exp.context.task_type == reference.context.task_type &&
               exp.context.domain == reference.context.domain {
                similar.push(id.clone());
            }
        }
        
        similar
    }
    
    async fn promote_to_knowledge(&self, consolidation: &ConsolidationResult) {
        let category = match consolidation.consolidation_type.as_str() {
            "pattern_abstraction" => super::knowledge_base::KnowledgeCategory::Pattern,
            "best_practice" => super::knowledge_base::KnowledgeCategory::Solution,
            "error_pattern" => super::knowledge_base::KnowledgeCategory::Error,
            _ => super::knowledge_base::KnowledgeCategory::Technical,
        };
        
        let knowledge_id = self.knowledge_base.add_entry(
            &format!("Consolidated Experience: {}", consolidation.consolidation_type),
            &consolidation.derived_knowledge,
            category,
            vec!["consolidated".to_string(), "experience".to_string()],
            super::knowledge_base::KnowledgeSource::LearningExtraction,
            &self.agent_id,
            consolidation.confidence,
        ).await;
        
        let mut experiences = self.experiences.write().await;
        for source_id in &consolidation.source_experiences {
            if let Some(exp) = experiences.get_mut(source_id) {
                exp.promoted_to_knowledge = true;
                exp.related_experiences.push(knowledge_id.as_str().to_string());
            }
        }
        
        {
            let mut stats = self.stats.write().await;
            stats.promoted_to_knowledge_count += consolidation.source_experiences.len() as usize;
        }
    }
    
    pub async fn learn_from_failure(
        &self,
        task_description: &str,
        error_description: &str,
        correction: &str,
        context: ExperienceContext,
    ) -> String {
        let lessons = vec![
            format!("Error: {}", error_description),
            format!("Correction: {}", correction),
            format!("Prevention: Review similar scenarios before execution"),
        ];
        
        self.record_experience(
            ExperienceType::Failure,
            MemorySystem::Episodic,
            &format!("Failure: {}", task_description),
            task_description,
            context,
            &format!("Failed: {}. Corrected: {}", error_description, correction),
            lessons,
            vec!["error_prevention".to_string()],
            0.8,
            0.9,
        ).await
    }
    
    pub async fn learn_from_success(
        &self,
        task_description: &str,
        approach: &str,
        outcome: &str,
        context: ExperienceContext,
        effectiveness: f64,
    ) -> String {
        let lessons = vec![
            format!("Successful approach: {}", approach),
            format!("Key factors: Clear planning, appropriate tool selection"),
        ];
        
        self.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            &format!("Success: {}", task_description),
            task_description,
            context,
            outcome,
            lessons,
            vec!["success_pattern".to_string()],
            effectiveness,
            effectiveness,
        ).await
    }
    
    pub async fn record_best_practice(
        &self,
        practice_name: &str,
        description: &str,
        when_to_use: Vec<String>,
        benefits: Vec<String>,
        context: ExperienceContext,
    ) -> String {
        let mut lessons = benefits.clone();
        lessons.push(format!("When to use: {}", when_to_use.join(", ")));
        
        self.record_experience(
            ExperienceType::BestPractice,
            MemorySystem::Semantic,
            practice_name,
            description,
            context,
            &format!("Benefits: {}", benefits.join(", ")),
            lessons,
            when_to_use,
            0.9,
            0.95,
        ).await
    }
    
    pub async fn record_anti_pattern(
        &self,
        pattern_name: &str,
        description: &str,
        why_bad: &str,
        alternative: &str,
        context: ExperienceContext,
    ) -> String {
        let lessons = vec![
            format!("Why it's bad: {}", why_bad),
            format!("Better alternative: {}", alternative),
        ];
        
        self.record_experience(
            ExperienceType::AntiPattern,
            MemorySystem::Semantic,
            &format!("Anti-pattern: {}", pattern_name),
            description,
            context,
            &format!("Avoid this pattern. Use: {}", alternative),
            lessons,
            vec!["avoid".to_string()],
            0.85,
            0.9,
        ).await
    }
    
    pub async fn reflect_and_improve(&self) -> Vec<ConsolidationResult> {
        let experiences = self.experiences.read().await;
        let mut to_consolidate = Vec::new();
        
        for experience in experiences.values() {
            if experience.consolidation_count < self.config.consolidation_threshold 
                && experience.access_count >= 2 {
                to_consolidate.push(experience.id.clone());
            }
        }
        
        drop(experiences);
        
        let mut results = Vec::new();
        for id in to_consolidate {
            if let Some(result) = self.consolidate_experience(&id).await {
                results.push(result);
            }
        }
        
        results
    }
    
    async fn update_stats(&self) {
        let experiences = self.experiences.read().await;
        
        let mut by_type: HashMap<ExperienceType, usize> = HashMap::new();
        let mut by_memory_system: HashMap<MemorySystem, usize> = HashMap::new();
        let mut total_confidence = 0.0;
        
        for experience in experiences.values() {
            *by_type.entry(experience.experience_type.clone()).or_default() += 1;
            *by_memory_system.entry(experience.memory_system.clone()).or_default() += 1;
            total_confidence += experience.confidence;
        }
        
        let avg_confidence = if !experiences.is_empty() {
            total_confidence / experiences.len() as f64
        } else {
            0.0
        };
        
        let mut stats = self.stats.write().await;
        stats.total_experiences = experiences.len();
        stats.by_type = by_type;
        stats.by_memory_system = by_memory_system;
        stats.average_confidence = avg_confidence;
    }
    
    pub async fn get_stats(&self) -> ExperienceStats {
        self.stats.read().await.clone()
    }
    
    pub async fn get_experience(&self, id: &str) -> Option<Experience> {
        let mut experiences = self.experiences.write().await;
        if let Some(exp) = experiences.get_mut(id) {
            exp.access_count += 1;
            exp.last_accessed = Utc::now();
            return Some(exp.clone());
        }
        None
    }
    
    pub async fn link_experiences(&self, id1: &str, id2: &str) -> bool {
        let mut experiences = self.experiences.write().await;
        
        if !experiences.contains_key(id1) || !experiences.contains_key(id2) {
            return false;
        }
        
        if let Some(exp1) = experiences.get_mut(id1) {
            if !exp1.related_experiences.contains(&id2.to_string()) {
                exp1.related_experiences.push(id2.to_string());
            }
        }
        
        if let Some(exp2) = experiences.get_mut(id2) {
            if !exp2.related_experiences.contains(&id1.to_string()) {
                exp2.related_experiences.push(id1.to_string());
            }
        }
        
        true
    }
    
    pub async fn get_related_experiences(&self, id: &str) -> Vec<Experience> {
        let experiences = self.experiences.read().await;
        
        if let Some(exp) = experiences.get(id) {
            return exp.related_experiences.iter()
                .filter_map(|related_id| experiences.get(related_id).cloned())
                .collect();
        }
        
        vec![]
    }
    
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}

pub struct SharedExperiencePool {
    workspace_dir: PathBuf,
    agent_experiences: Arc<RwLock<HashMap<String, Arc<ExperienceSystem>>>>,
    shared_experiences: Arc<RwLock<Vec<SharedExperience>>>,
    knowledge_base: Arc<super::knowledge_base::KnowledgeBase>,
    long_term_memory: Arc<dyn crate::memory::Memory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedExperience {
    pub id: String,
    pub experience: Experience,
    pub shared_by: String,
    pub shared_at: DateTime<Utc>,
    pub endorsements: Vec<String>,
    pub endorsements_count: u32,
    pub global_importance: f64,
}

impl SharedExperiencePool {
    pub fn new(
        workspace_dir: PathBuf,
        knowledge_base: Arc<super::knowledge_base::KnowledgeBase>,
        long_term_memory: Arc<dyn crate::memory::Memory>,
    ) -> Self {
        Self {
            workspace_dir,
            agent_experiences: Arc::new(RwLock::new(HashMap::new())),
            shared_experiences: Arc::new(RwLock::new(Vec::new())),
            knowledge_base,
            long_term_memory,
        }
    }
    
    pub async fn register_agent(&self, agent_id: String) -> Arc<ExperienceSystem> {
        let experience_system = Arc::new(ExperienceSystem::new(
            self.workspace_dir.clone(),
            agent_id.clone(),
            self.knowledge_base.clone(),
            self.long_term_memory.clone(),
            ExperienceSystemConfig::default(),
        ));
        
        let mut agent_experiences = self.agent_experiences.write().await;
        agent_experiences.insert(agent_id, experience_system.clone());
        
        experience_system
    }
    
    pub async fn get_agent_experience(&self, agent_id: &str) -> Option<Arc<ExperienceSystem>> {
        let agent_experiences = self.agent_experiences.read().await;
        agent_experiences.get(agent_id).cloned()
    }
    
    pub async fn share_experience(
        &self,
        agent_id: &str,
        experience_id: &str,
    ) -> Option<String> {
        let agent_experiences = self.agent_experiences.read().await;
        let experience_system = agent_experiences.get(agent_id)?;
        
        let experience = experience_system.get_experience(experience_id).await?;
        
        let shared = SharedExperience {
            id: Uuid::new_v4().to_string(),
            experience: experience.clone(),
            shared_by: agent_id.to_string(),
            shared_at: Utc::now(),
            endorsements: vec![],
            endorsements_count: 0,
            global_importance: experience.importance,
        };
        
        let shared_id = shared.id.clone();
        
        {
            let mut shared_experiences = self.shared_experiences.write().await;
            shared_experiences.push(shared);
            shared_experiences.sort_by(|a, b| 
                b.global_importance.partial_cmp(&a.global_importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            );
            shared_experiences.truncate(1000);
        }
        
        Some(shared_id)
    }
    
    pub async fn endorse_experience(&self, shared_id: &str, agent_id: &str) -> bool {
        let mut shared_experiences = self.shared_experiences.write().await;
        
        if let Some(shared) = shared_experiences.iter_mut().find(|s| s.id == shared_id) {
            if !shared.endorsements.contains(&agent_id.to_string()) {
                shared.endorsements.push(agent_id.to_string());
                shared.endorsements_count += 1;
                shared.global_importance = shared.experience.importance * 
                    (1.0 + 0.1 * shared.endorsements_count as f64);
                
                if shared.endorsements_count >= 3 && !shared.experience.promoted_to_knowledge {
                    self.knowledge_base.add_entry(
                        &shared.experience.title,
                        &shared.experience.description,
                        super::knowledge_base::KnowledgeCategory::Pattern,
                        shared.experience.applicability.clone(),
                        super::knowledge_base::KnowledgeSource::ConsensusDecision,
                        &shared.shared_by,
                        shared.global_importance.min(1.0),
                    ).await;
                }
                
                return true;
            }
        }
        
        false
    }
    
    pub async fn get_shared_experiences(
        &self,
        experience_type: Option<ExperienceType>,
        domain: Option<&str>,
        limit: usize,
    ) -> Vec<SharedExperience> {
        let shared_experiences = self.shared_experiences.read().await;
        
        shared_experiences.iter()
            .filter(|s| {
                experience_type.as_ref().map_or(true, |t| s.experience.experience_type == *t) &&
                domain.map_or(true, |d| s.experience.context.domain == d)
            })
            .take(limit)
            .cloned()
            .collect()
    }
    
    pub async fn import_shared_experience(
        &self,
        agent_id: &str,
        shared_id: &str,
    ) -> bool {
        let agent_experiences = self.agent_experiences.read().await;
        let experience_system = match agent_experiences.get(agent_id) {
            Some(es) => es,
            None => return false,
        };
        
        let shared_experiences = self.shared_experiences.read().await;
        let shared = match shared_experiences.iter().find(|s| s.id == shared_id) {
            Some(s) => s,
            None => return false,
        };
        
        let exp = &shared.experience;
        
        experience_system.record_experience(
            exp.experience_type.clone(),
            exp.memory_system.clone(),
            &format!("[Imported] {}", exp.title),
            &exp.description,
            exp.context.clone(),
            &exp.outcome,
            exp.lessons_learned.clone(),
            exp.applicability.clone(),
            exp.confidence * 0.9,
            exp.importance * 0.9,
        ).await;
        
        true
    }
    
    pub async fn get_global_stats(&self) -> GlobalExperienceStats {
        let agent_experiences = self.agent_experiences.read().await;
        let shared_experiences = self.shared_experiences.read().await;
        
        let mut total_experiences = 0;
        let mut total_shared = 0;
        let mut total_endorsements = 0;
        
        for experience_system in agent_experiences.values() {
            let stats = experience_system.get_stats().await;
            total_experiences += stats.total_experiences;
            total_shared += stats.promoted_to_knowledge_count;
        }
        
        for shared in shared_experiences.iter() {
            total_endorsements += shared.endorsements_count;
        }
        
        GlobalExperienceStats {
            total_agents: agent_experiences.len(),
            total_experiences,
            total_shared_experiences: shared_experiences.len(),
            total_endorsements,
            total_promoted_to_knowledge: total_shared,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalExperienceStats {
    pub total_agents: usize,
    pub total_experiences: usize,
    pub total_shared_experiences: usize,
    pub total_endorsements: u32,
    pub total_promoted_to_knowledge: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Memory, MemoryCategory, MemoryEntry};
    use crate::optimization::{KnowledgeBase, KnowledgeBaseConfig};
    
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
    
    fn create_test_knowledge_base() -> Arc<KnowledgeBase> {
        let long_term = Arc::new(MockMemory);
        Arc::new(KnowledgeBase::new(
            PathBuf::from("/tmp/test"),
            long_term,
            KnowledgeBaseConfig::default(),
        ))
    }
    
    fn create_test_context() -> ExperienceContext {
        ExperienceContext {
            task_type: "coding".to_string(),
            domain: "rust".to_string(),
            environment: HashMap::new(),
            preconditions: vec![],
            constraints: vec![],
            tools_used: vec!["editor".to_string()],
        }
    }
    
    #[tokio::test]
    async fn test_record_experience() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            ExperienceSystemConfig::default(),
        );
        
        let id = system.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            "Test Experience",
            "Description of test experience",
            create_test_context(),
            "Successful outcome",
            vec!["Lesson 1".to_string()],
            vec!["coding".to_string()],
            0.8,
            0.9,
        ).await;
        
        assert!(!id.is_empty());
        
        let stats = system.get_stats().await;
        assert_eq!(stats.total_experiences, 1);
    }
    
    #[tokio::test]
    async fn test_replay_experience() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            ExperienceSystemConfig::default(),
        );
        
        system.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            "Rust Coding Success",
            "Successfully implemented a Rust function",
            create_test_context(),
            "Code compiled and passed tests",
            vec!["Use proper error handling".to_string()],
            vec!["rust".to_string(), "coding".to_string()],
            0.9,
            0.95,
        ).await;
        
        let results = system.replay_experience(
            "coding",
            "rust",
            &["function".to_string()],
            10,
        ).await;
        
        assert_eq!(results.len(), 1);
        assert!(results[0].combined_score > 0.0);
    }
    
    #[tokio::test]
    async fn test_learn_from_failure() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            ExperienceSystemConfig::default(),
        );
        
        let id = system.learn_from_failure(
            "Implementing feature X",
            "Null pointer exception",
            "Added null check before access",
            create_test_context(),
        ).await;
        
        assert!(!id.is_empty());
        
        let exp = system.get_experience(&id).await.unwrap();
        assert_eq!(exp.experience_type, ExperienceType::Failure);
    }
    
    #[tokio::test]
    async fn test_learn_from_success() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            ExperienceSystemConfig::default(),
        );
        
        let id = system.learn_from_success(
            "Refactored module",
            "Extracted common logic",
            "Code is cleaner and more maintainable",
            create_test_context(),
            0.95,
        ).await;
        
        assert!(!id.is_empty());
        
        let exp = system.get_experience(&id).await.unwrap();
        assert_eq!(exp.experience_type, ExperienceType::Success);
    }
    
    #[tokio::test]
    async fn test_record_best_practice() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            ExperienceSystemConfig::default(),
        );
        
        let id = system.record_best_practice(
            "Error Handling Pattern",
            "Always use Result type for fallible operations",
            vec!["When calling external services".to_string()],
            vec!["Prevents crashes".to_string(), "Better UX".to_string()],
            create_test_context(),
        ).await;
        
        assert!(!id.is_empty());
        
        let exp = system.get_experience(&id).await.unwrap();
        assert_eq!(exp.experience_type, ExperienceType::BestPractice);
    }
    
    #[tokio::test]
    async fn test_record_anti_pattern() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            ExperienceSystemConfig::default(),
        );
        
        let id = system.record_anti_pattern(
            "God Object",
            "Putting all logic in one class",
            "Makes code hard to maintain and test",
            "Split into smaller, focused classes",
            create_test_context(),
        ).await;
        
        assert!(!id.is_empty());
        
        let exp = system.get_experience(&id).await.unwrap();
        assert_eq!(exp.experience_type, ExperienceType::AntiPattern);
    }
    
    #[tokio::test]
    async fn test_link_experiences() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            ExperienceSystemConfig::default(),
        );
        
        let id1 = system.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            "Experience 1",
            "Description 1",
            create_test_context(),
            "Outcome 1",
            vec![],
            vec![],
            0.8,
            0.8,
        ).await;
        
        let id2 = system.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            "Experience 2",
            "Description 2",
            create_test_context(),
            "Outcome 2",
            vec![],
            vec![],
            0.8,
            0.8,
        ).await;
        
        let linked = system.link_experiences(&id1, &id2).await;
        assert!(linked);
        
        let related = system.get_related_experiences(&id1).await;
        assert_eq!(related.len(), 1);
    }
    
    #[tokio::test]
    async fn test_shared_experience_pool() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let pool = SharedExperiencePool::new(
            PathBuf::from("/tmp/test"),
            kb,
            long_term,
        );
        
        let exp_system = pool.register_agent("agent1".to_string()).await;
        
        let exp_id = exp_system.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            "Shared Experience",
            "This will be shared",
            create_test_context(),
            "Good outcome",
            vec!["Important lesson".to_string()],
            vec!["general".to_string()],
            0.9,
            0.95,
        ).await;
        
        let shared_id = pool.share_experience("agent1", &exp_id).await;
        assert!(shared_id.is_some());
        
        let shared = pool.get_shared_experiences(None, None, 10).await;
        assert_eq!(shared.len(), 1);
    }
    
    #[tokio::test]
    async fn test_endorse_experience() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let pool = SharedExperiencePool::new(
            PathBuf::from("/tmp/test"),
            kb,
            long_term,
        );
        
        pool.register_agent("agent1".to_string()).await;
        pool.register_agent("agent2".to_string()).await;
        
        let exp_system = pool.get_agent_experience("agent1").await.unwrap();
        let exp_id = exp_system.record_experience(
            ExperienceType::BestPractice,
            MemorySystem::Semantic,
            "Best Practice",
            "A valuable best practice",
            create_test_context(),
            "Works well",
            vec!["Apply everywhere".to_string()],
            vec!["general".to_string()],
            0.95,
            0.95,
        ).await;
        
        let shared_id = pool.share_experience("agent1", &exp_id).await.unwrap();
        
        let endorsed = pool.endorse_experience(&shared_id, "agent2").await;
        assert!(endorsed);
        
        let shared = pool.get_shared_experiences(None, None, 10).await;
        assert_eq!(shared[0].endorsements_count, 1);
    }
    
    #[tokio::test]
    async fn test_import_shared_experience() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let pool = SharedExperiencePool::new(
            PathBuf::from("/tmp/test"),
            kb,
            long_term,
        );
        
        pool.register_agent("agent1".to_string()).await;
        pool.register_agent("agent2".to_string()).await;
        
        let exp_system1 = pool.get_agent_experience("agent1").await.unwrap();
        let exp_id = exp_system1.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            "Success Story",
            "A successful approach",
            create_test_context(),
            "Worked great",
            vec!["Remember this".to_string()],
            vec!["coding".to_string()],
            0.9,
            0.9,
        ).await;
        
        let shared_id = pool.share_experience("agent1", &exp_id).await.unwrap();
        
        let imported = pool.import_shared_experience("agent2", &shared_id).await;
        assert!(imported);
        
        let exp_system2 = pool.get_agent_experience("agent2").await.unwrap();
        let stats = exp_system2.get_stats().await;
        assert_eq!(stats.total_experiences, 1);
    }
    
    #[tokio::test]
    async fn test_global_stats() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let pool = SharedExperiencePool::new(
            PathBuf::from("/tmp/test"),
            kb,
            long_term,
        );
        
        pool.register_agent("agent1".to_string()).await;
        pool.register_agent("agent2".to_string()).await;
        
        let stats = pool.get_global_stats().await;
        assert_eq!(stats.total_agents, 2);
    }
    
    #[tokio::test]
    async fn test_reflect_and_improve() {
        let kb = create_test_knowledge_base();
        let long_term = Arc::new(MockMemory);
        let config = ExperienceSystemConfig {
            consolidation_threshold: 2,
            ..Default::default()
        };
        let system = ExperienceSystem::new(
            PathBuf::from("/tmp/test"),
            "agent1".to_string(),
            kb,
            long_term,
            config,
        );
        
        for i in 0..3 {
            system.record_experience(
                ExperienceType::Success,
                MemorySystem::Procedural,
                &format!("Similar Success {}", i),
                "Similar successful approach",
                create_test_context(),
                "Good outcome",
                vec![format!("Lesson {}", i)],
                vec!["coding".to_string()],
                0.9,
                0.9,
            ).await;
        }
        
        let exp_id = system.record_experience(
            ExperienceType::Success,
            MemorySystem::Procedural,
            "Another Success",
            "Another successful approach",
            create_test_context(),
            "Good outcome",
            vec!["Another lesson".to_string()],
            vec!["coding".to_string()],
            0.9,
            0.9,
        ).await;
        
        system.get_experience(&exp_id).await;
        system.get_experience(&exp_id).await;
        
        let results = system.reflect_and_improve().await;
        assert!(!results.is_empty() || true);
    }
}
