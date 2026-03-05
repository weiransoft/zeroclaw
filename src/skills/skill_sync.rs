use anyhow::Result;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::clawhub::{ClawHubClient, ClawHubConfig, SkillInfo, SkillCategory, SkillApproval, SkillApprovalStatus, SkillDownloadRequest, SkillPriority};
use crate::optimization::knowledge_base::{KnowledgeBase, KnowledgeCategory, KnowledgeSource};

const SYNC_INTERVAL_HOURS: i64 = 24;
const HIGH_RISK_THRESHOLD: f64 = 0.6;
const MEDIUM_RISK_THRESHOLD: f64 = 0.3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillKnowledgeEntry {
    pub skill_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: SkillCategory,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub author: String,
    pub rating: f64,
    pub downloads: u64,
    pub verified: bool,
    pub risk_level: RiskLevel,
    pub risk_factors: Vec<RiskFactor>,
    pub last_synced: DateTime<Utc>,
    pub install_count: u32,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "低风险"),
            Self::Medium => write!(f, "中等风险"),
            Self::High => write!(f, "高风险"),
            Self::Critical => write!(f, "严重风险"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskFactor {
    UnverifiedAuthor,
    LowDownloads,
    LowRating,
    NewSkill,
    HasSystemAccess,
    HasNetworkAccess,
    HasFileAccess,
    UnknownSource,
    NoDocumentation,
    BreakingChanges,
}

impl RiskFactor {
    pub fn weight(&self) -> f64 {
        match self {
            Self::UnverifiedAuthor => 0.2,
            Self::LowDownloads => 0.1,
            Self::LowRating => 0.15,
            Self::NewSkill => 0.1,
            Self::HasSystemAccess => 0.25,
            Self::HasNetworkAccess => 0.2,
            Self::HasFileAccess => 0.15,
            Self::UnknownSource => 0.3,
            Self::NoDocumentation => 0.1,
            Self::BreakingChanges => 0.35,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::UnverifiedAuthor => "作者未验证",
            Self::LowDownloads => "下载量较低",
            Self::LowRating => "评分较低",
            Self::NewSkill => "新发布的技能",
            Self::HasSystemAccess => "需要系统权限",
            Self::HasNetworkAccess => "需要网络访问",
            Self::HasFileAccess => "需要文件访问",
            Self::UnknownSource => "来源未知",
            Self::NoDocumentation => "缺少文档",
            Self::BreakingChanges => "可能存在破坏性变更",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSyncConfig {
    pub enabled: bool,
    pub sync_interval_hours: i64,
    pub auto_sync: bool,
    pub sync_categories: Vec<SkillCategory>,
    pub min_rating: f64,
    pub min_downloads: u64,
    pub require_verification: bool,
    pub boss_approval_threshold: RiskLevel,
}

impl Default for SkillSyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sync_interval_hours: SYNC_INTERVAL_HOURS,
            auto_sync: true,
            sync_categories: vec![
                SkillCategory::Memory,
                SkillCategory::Analysis,
                SkillCategory::Automation,
                SkillCategory::Integration,
                SkillCategory::Communication,
                SkillCategory::Development,
                SkillCategory::Research,
                SkillCategory::Creative,
            ],
            min_rating: 0.0,
            min_downloads: 0,
            require_verification: false,
            boss_approval_threshold: RiskLevel::Medium,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSyncStats {
    pub last_sync: Option<DateTime<Utc>>,
    pub total_synced: usize,
    pub by_category: HashMap<String, usize>,
    pub by_risk_level: HashMap<String, usize>,
    pub pending_approvals: usize,
    pub next_sync: Option<DateTime<Utc>>,
}

pub struct SkillKnowledgeSync {
    client: ClawHubClient,
    knowledge_base: Arc<KnowledgeBase>,
    config: SkillSyncConfig,
    workspace_dir: PathBuf,
    synced_skills: Arc<RwLock<HashMap<String, SkillKnowledgeEntry>>>,
    last_sync: Arc<RwLock<Option<DateTime<Utc>>>>,
    pending_approvals: Arc<RwLock<Vec<SkillApproval>>>,
}

impl SkillKnowledgeSync {
    pub fn new(
        workspace_dir: PathBuf,
        knowledge_base: Arc<KnowledgeBase>,
        hub_config: ClawHubConfig,
        sync_config: SkillSyncConfig,
    ) -> Self {
        let client = ClawHubClient::new(workspace_dir.clone(), hub_config);
        
        Self {
            client,
            knowledge_base,
            config: sync_config,
            workspace_dir,
            synced_skills: Arc::new(RwLock::new(HashMap::new())),
            last_sync: Arc::new(RwLock::new(None)),
            pending_approvals: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn sync_to_knowledge_base(&self) -> Result<SkillSyncStats> {
        let mut stats = SkillSyncStats {
            last_sync: *self.last_sync.read().await,
            total_synced: 0,
            by_category: HashMap::new(),
            by_risk_level: HashMap::new(),
            pending_approvals: 0,
            next_sync: None,
        };

        for category in &self.config.sync_categories {
            let result = self.client.list_skills(Some(super::clawhub::SkillListOptions {
                category: Some(category.clone()),
                sort: Some("downloads".to_string()),
                limit: Some(100),
                offset: None,
            })).await?;

            for skill in result.skills {
                if self.should_sync(&skill) {
                    let entry = self.create_knowledge_entry(&skill).await;
                    
                    self.add_to_knowledge_base(&entry).await?;
                    
                    let risk_level_str = entry.risk_level.to_string();
                    let category_str = format!("{:?}", category);
                    
                    {
                        let mut synced = self.synced_skills.write().await;
                        synced.insert(entry.skill_id.clone(), entry);
                    }
                    
                    stats.total_synced += 1;
                    *stats.by_category.entry(category_str).or_default() += 1;
                    *stats.by_risk_level.entry(risk_level_str).or_default() += 1;
                }
            }
        }

        let now = Utc::now();
        *self.last_sync.write().await = Some(now);
        stats.last_sync = Some(now);
        stats.next_sync = Some(now + Duration::hours(self.config.sync_interval_hours));

        stats.pending_approvals = self.pending_approvals.read().await.len();

        Ok(stats)
    }

    fn should_sync(&self, skill: &SkillInfo) -> bool {
        if skill.rating < self.config.min_rating {
            return false;
        }
        if skill.downloads < self.config.min_downloads {
            return false;
        }
        if self.config.require_verification && !skill.verified {
            return false;
        }
        true
    }

    async fn create_knowledge_entry(&self, skill: &SkillInfo) -> SkillKnowledgeEntry {
        let risk_assessment = self.assess_risk(skill).await;
        
        SkillKnowledgeEntry {
            skill_id: skill.id.clone(),
            name: skill.name.clone(),
            version: skill.version.clone(),
            description: skill.description.clone(),
            category: skill.category.clone(),
            capabilities: skill.capabilities.clone(),
            tags: skill.tags.clone(),
            author: skill.author.clone(),
            rating: skill.rating,
            downloads: skill.downloads,
            verified: skill.verified,
            risk_level: risk_assessment.level,
            risk_factors: risk_assessment.factors,
            last_synced: Utc::now(),
            install_count: 0,
            success_rate: 1.0,
        }
    }

    async fn assess_risk(&self, skill: &SkillInfo) -> RiskAssessment {
        let mut factors = Vec::new();
        let mut total_risk = 0.0;

        if !skill.verified {
            factors.push(RiskFactor::UnverifiedAuthor);
            total_risk += RiskFactor::UnverifiedAuthor.weight();
        }

        if skill.downloads < 100 {
            factors.push(RiskFactor::LowDownloads);
            total_risk += RiskFactor::LowDownloads.weight();
        }

        if skill.rating < 3.0 {
            factors.push(RiskFactor::LowRating);
            total_risk += RiskFactor::LowRating.weight();
        }

        let age_days = (Utc::now().timestamp() - skill.created_at) / 86400;
        if age_days < 30 {
            factors.push(RiskFactor::NewSkill);
            total_risk += RiskFactor::NewSkill.weight();
        }

        for cap in &skill.capabilities {
            match cap.to_lowercase().as_str() {
                "system" | "shell" | "execute" => {
                    factors.push(RiskFactor::HasSystemAccess);
                    total_risk += RiskFactor::HasSystemAccess.weight();
                }
                "network" | "http" | "api" => {
                    factors.push(RiskFactor::HasNetworkAccess);
                    total_risk += RiskFactor::HasNetworkAccess.weight();
                }
                "file" | "filesystem" | "storage" => {
                    factors.push(RiskFactor::HasFileAccess);
                    total_risk += RiskFactor::HasFileAccess.weight();
                }
                _ => {}
            }
        }

        if skill.documentation.is_none() || skill.documentation.as_ref().map_or(true, |d| d.is_empty()) {
            factors.push(RiskFactor::NoDocumentation);
            total_risk += RiskFactor::NoDocumentation.weight();
        }

        let level = if total_risk >= 0.8 {
            RiskLevel::Critical
        } else if total_risk >= HIGH_RISK_THRESHOLD {
            RiskLevel::High
        } else if total_risk >= MEDIUM_RISK_THRESHOLD {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        RiskAssessment {
            level,
            factors,
            score: total_risk.min(1.0),
        }
    }

    async fn add_to_knowledge_base(&self, entry: &SkillKnowledgeEntry) -> Result<String> {
        let content = format!(
            "# {}\n\n\
            **版本**: {}\n\
            **作者**: {}\n\
            **分类**: {:?}\n\
            **风险等级**: {}\n\n\
            ## 描述\n{}\n\n\
            ## 能力\n{}\n\n\
            ## 标签\n{}\n\n\
            ## 统计\n- 评分: {:.1}/5.0\n- 下载量: {}\n- 已验证: {}\n\n\
            ## 风险因素\n{}",
            entry.name,
            entry.version,
            entry.author,
            entry.category,
            entry.risk_level,
            entry.description,
            entry.capabilities.join(", "),
            entry.tags.join(", "),
            entry.rating,
            entry.downloads,
            if entry.verified { "是" } else { "否" },
            entry.risk_factors.iter()
                .map(|f| format!("- {} (权重: {:.2})", f.description(), f.weight()))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let knowledge_id = self.knowledge_base.add_entry(
            &format!("Skill: {}", entry.name),
            &content,
            KnowledgeCategory::Technical,
            entry.tags.iter().cloned()
                .chain(vec!["skill".to_string(), "clawhub".to_string()])
                .collect(),
            KnowledgeSource::LearningExtraction,
            "skill-sync",
            entry.rating / 5.0,
        ).await;

        Ok(knowledge_id.as_str().to_string())
    }

    pub async fn request_skill_for_task(
        &self,
        skill_id: &str,
        task_context: &str,
        requested_by: &str,
    ) -> Result<SkillApprovalRequest> {
        let synced = self.synced_skills.read().await;
        let entry = synced.get(skill_id).cloned();
        drop(synced);

        let skill_info = if let Some(e) = entry {
            Some(SkillInfo {
                id: e.skill_id.clone(),
                name: e.name.clone(),
                version: e.version.clone(),
                description: e.description.clone(),
                author: e.author.clone(),
                category: e.category.clone(),
                tags: e.tags.clone(),
                dependencies: vec![],
                capabilities: e.capabilities.clone(),
                rating: e.rating,
                downloads: e.downloads,
                created_at: 0,
                updated_at: 0,
                repository: None,
                documentation: None,
                examples: vec![],
                verified: e.verified,
                size: None,
            })
        } else {
            self.client.get_skill(skill_id).await?
        };

        let skill = match skill_info {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("Skill not found: {}", skill_id)),
        };

        let risk_assessment = self.assess_risk(&skill).await;
        let needs_boss_approval = self.needs_boss_approval(&risk_assessment.level);

        let request = SkillApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            skill_id: skill_id.to_string(),
            skill_name: skill.name.clone(),
            task_context: task_context.to_string(),
            requested_by: requested_by.to_string(),
            risk_level: risk_assessment.level.clone(),
            risk_factors: risk_assessment.factors.clone(),
            needs_boss_approval,
            status: if needs_boss_approval {
                SkillRequestStatus::PendingBossApproval
            } else {
                SkillRequestStatus::PendingInstall
            },
            created_at: Utc::now(),
            boss_approved: None,
            boss_comment: None,
        };

        if needs_boss_approval {
            let mut pending = self.pending_approvals.write().await;
            pending.push(SkillApproval {
                id: request.id.clone(),
                request: SkillDownloadRequest {
                    skill_id: skill_id.to_string(),
                    version: None,
                    target_path: None,
                    requested_by: requested_by.to_string(),
                    reason: task_context.to_string(),
                    priority: SkillPriority::Medium,
                },
                skill: Some(skill),
                status: SkillApprovalStatus::Pending,
                reviewed_by: None,
                reviewed_at: None,
                comment: None,
                download_progress: None,
                error: None,
            });
        }

        Ok(request)
    }

    fn needs_boss_approval(&self, risk_level: &RiskLevel) -> bool {
        match risk_level {
            RiskLevel::Critical => true,
            RiskLevel::High => true,
            RiskLevel::Medium => self.config.boss_approval_threshold == RiskLevel::Medium 
                || self.config.boss_approval_threshold == RiskLevel::Low,
            RiskLevel::Low => self.config.boss_approval_threshold == RiskLevel::Low,
        }
    }

    pub async fn boss_approve(&self, request_id: &str, approved: bool, comment: Option<&str>) -> Result<()> {
        let mut pending = self.pending_approvals.write().await;
        
        if let Some(approval) = pending.iter_mut().find(|a| a.id == request_id) {
            if approved {
                approval.status = SkillApprovalStatus::Approved;
                approval.reviewed_by = Some("boss".to_string());
                approval.reviewed_at = Some(Utc::now().timestamp());
                approval.comment = comment.map(|c| c.to_string());
            } else {
                approval.status = SkillApprovalStatus::Rejected;
                approval.reviewed_by = Some("boss".to_string());
                approval.reviewed_at = Some(Utc::now().timestamp());
                approval.comment = comment.map(|c| c.to_string());
            }
        }

        Ok(())
    }

    pub async fn get_pending_boss_approvals(&self) -> Vec<SkillApproval> {
        let pending = self.pending_approvals.read().await;
        pending.iter()
            .filter(|a| a.status == SkillApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    pub async fn find_skills_for_task(&self, task_description: &str, limit: usize) -> Vec<SkillKnowledgeEntry> {
        let synced = self.synced_skills.read().await;
        let task_lower = task_description.to_lowercase();
        
        let mut matches: Vec<(f64, &SkillKnowledgeEntry)> = synced.values()
            .filter_map(|entry| {
                let mut score = 0.0;
                
                for cap in &entry.capabilities {
                    if task_lower.contains(&cap.to_lowercase()) {
                        score += 0.3;
                    }
                }
                
                for tag in &entry.tags {
                    if task_lower.contains(&tag.to_lowercase()) {
                        score += 0.2;
                    }
                }
                
                if task_lower.contains(&entry.name.to_lowercase()) {
                    score += 0.5;
                }
                
                if task_lower.contains(&entry.description.to_lowercase()) {
                    score += 0.1;
                }
                
                score += entry.rating / 10.0;
                score += (entry.downloads as f64).log10() / 100.0;
                
                if score > 0.1 {
                    Some((score, entry))
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        matches.into_iter()
            .take(limit)
            .map(|(_, entry)| entry.clone())
            .collect()
    }

    pub async fn get_sync_stats(&self) -> SkillSyncStats {
        let synced = self.synced_skills.read().await;
        let last_sync = self.last_sync.read().await;
        let pending = self.pending_approvals.read().await;

        let mut by_category = HashMap::new();
        let mut by_risk_level = HashMap::new();

        for entry in synced.values() {
            *by_category.entry(format!("{:?}", entry.category)).or_default() += 1;
            *by_risk_level.entry(entry.risk_level.to_string()).or_default() += 1;
        }

        SkillSyncStats {
            last_sync: *last_sync,
            total_synced: synced.len(),
            by_category,
            by_risk_level,
            pending_approvals: pending.len(),
            next_sync: last_sync.map(|t| t + Duration::hours(self.config.sync_interval_hours)),
        }
    }

    pub async fn get_skill_entry(&self, skill_id: &str) -> Option<SkillKnowledgeEntry> {
        let synced = self.synced_skills.read().await;
        synced.get(skill_id).cloned()
    }

    pub fn config(&self) -> &SkillSyncConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: SkillSyncConfig) {
        self.config = config;
    }
}

#[derive(Debug, Clone)]
struct RiskAssessment {
    level: RiskLevel,
    factors: Vec<RiskFactor>,
    score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillApprovalRequest {
    pub id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub task_context: String,
    pub requested_by: String,
    pub risk_level: RiskLevel,
    pub risk_factors: Vec<RiskFactor>,
    pub needs_boss_approval: bool,
    pub status: SkillRequestStatus,
    pub created_at: DateTime<Utc>,
    pub boss_approved: Option<bool>,
    pub boss_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillRequestStatus {
    PendingBossApproval,
    PendingInstall,
    Installing,
    Installed,
    Failed,
    Rejected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "低风险");
        assert_eq!(RiskLevel::Medium.to_string(), "中等风险");
        assert_eq!(RiskLevel::High.to_string(), "高风险");
        assert_eq!(RiskLevel::Critical.to_string(), "严重风险");
    }

    #[test]
    fn test_risk_factor_weights() {
        assert_eq!(RiskFactor::UnverifiedAuthor.weight(), 0.2);
        assert_eq!(RiskFactor::HasSystemAccess.weight(), 0.25);
        assert_eq!(RiskFactor::UnknownSource.weight(), 0.3);
    }

    #[test]
    fn test_default_sync_config() {
        let config = SkillSyncConfig::default();
        assert!(config.enabled);
        assert!(config.auto_sync);
        assert_eq!(config.sync_interval_hours, 24);
        assert_eq!(config.boss_approval_threshold, RiskLevel::Medium);
    }

    #[test]
    fn test_skill_request_status_serde() {
        let status = SkillRequestStatus::PendingBossApproval;
        let json = serde_json::to_string(&status).unwrap();
        let parsed: SkillRequestStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SkillRequestStatus::PendingBossApproval);
    }

    #[test]
    fn test_needs_boss_approval_logic() {
        let config = SkillSyncConfig {
            boss_approval_threshold: RiskLevel::Medium,
            ..Default::default()
        };

        assert!(matches!(config.boss_approval_threshold, RiskLevel::Medium | RiskLevel::High | RiskLevel::Critical | RiskLevel::Low));
        
        fn needs_approval(risk: &RiskLevel, threshold: &RiskLevel) -> bool {
            match risk {
                RiskLevel::Critical => true,
                RiskLevel::High => true,
                RiskLevel::Medium => *threshold == RiskLevel::Medium || *threshold == RiskLevel::Low,
                RiskLevel::Low => *threshold == RiskLevel::Low,
            }
        }

        assert!(needs_approval(&RiskLevel::Critical, &RiskLevel::Medium));
        assert!(needs_approval(&RiskLevel::High, &RiskLevel::Medium));
        assert!(needs_approval(&RiskLevel::Medium, &RiskLevel::Medium));
        assert!(!needs_approval(&RiskLevel::Low, &RiskLevel::Medium));
    }
}
