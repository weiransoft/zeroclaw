use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::optimization::experience_system::{ExperienceSystem, ExperienceType, ExperienceContext, MemorySystem};
use crate::swarm::consensus::ConsensusManager;
use crate::swarm::chat::SwarmChatManager;

const DELIBERATION_TIMEOUT_SECONDS: u64 = 300;
const MIN_PARTICIPANTS_FOR_CONSENSUS: usize = 2;
const CONSENSUS_THRESHOLD: f64 = 0.6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDeliberationStatus {
    Proposed,
    Discussing,
    ExperienceReview,
    ConsensusBuilding,
    WaitingForBossApproval,
    Approved,
    Rejected,
    Implemented,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub name: String,
    pub description: String,
    pub order: u32,
    pub dependencies: Vec<String>,
    pub assigned_role: Option<String>,
    pub estimated_duration_seconds: Option<u64>,
    pub tools_required: Vec<String>,
    pub conditions: Vec<String>,
    pub error_handling: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub steps: Vec<WorkflowStep>,
    pub triggers: Vec<String>,
    pub success_criteria: Vec<String>,
    pub failure_handling: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowIssue {
    pub id: String,
    pub step_id: Option<String>,
    pub severity: IssueSeverity,
    pub description: String,
    pub suggested_fix: Option<String>,
    pub raised_by: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliberationParticipant {
    pub agent_id: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub vote: Option<bool>,
    pub concerns: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOptimizationProposal {
    pub id: String,
    pub current_workflow: WorkflowDefinition,
    pub proposed_workflow: WorkflowDefinition,
    pub rationale: String,
    pub expected_benefits: Vec<String>,
    pub potential_risks: Vec<String>,
    pub implementation_steps: Vec<String>,
    pub rollback_plan: Option<String>,
    pub proposed_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDeliberation {
    pub id: String,
    pub proposal: WorkflowOptimizationProposal,
    pub status: WorkflowDeliberationStatus,
    pub participants: HashMap<String, DeliberationParticipant>,
    pub issues: Vec<WorkflowIssue>,
    pub experience_references: Vec<String>,
    pub discussion_summary: String,
    pub consensus_result: Option<ConsensusResult>,
    pub boss_approval: Option<BossApproval>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    pub reached: bool,
    pub agreement_ratio: f64,
    pub total_participants: usize,
    pub agreements: usize,
    pub disagreements: usize,
    pub abstentions: usize,
    pub key_concerns: Vec<String>,
    pub resolved_issues: Vec<String>,
    pub unresolved_issues: Vec<String>,
    pub final_recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossApproval {
    pub approved: bool,
    pub approved_by: String,
    pub approved_at: DateTime<Utc>,
    pub comment: Option<String>,
    pub conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliberationStats {
    pub total_deliberations: usize,
    pub by_status: HashMap<String, usize>,
    pub successful_optimizations: usize,
    pub average_participants: f64,
    pub average_duration_seconds: f64,
    pub boss_approval_rate: f64,
}

pub struct WorkflowDeliberationEngine {
    workspace_dir: PathBuf,
    experience_system: Arc<ExperienceSystem>,
    consensus_manager: Arc<ConsensusManager>,
    chat_manager: Arc<SwarmChatManager>,
    deliberations: Arc<RwLock<HashMap<String, WorkflowDeliberation>>>,
    config: DeliberationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliberationConfig {
    pub min_participants: usize,
    pub consensus_threshold: f64,
    pub timeout_seconds: u64,
    pub require_boss_approval: bool,
    pub auto_approve_low_risk: bool,
    pub experience_weight: f64,
    pub enable_anonymous_voting: bool,
}

impl Default for DeliberationConfig {
    fn default() -> Self {
        Self {
            min_participants: MIN_PARTICIPANTS_FOR_CONSENSUS,
            consensus_threshold: CONSENSUS_THRESHOLD,
            timeout_seconds: DELIBERATION_TIMEOUT_SECONDS,
            require_boss_approval: true,
            auto_approve_low_risk: false,
            experience_weight: 0.3,
            enable_anonymous_voting: false,
        }
    }
}

impl WorkflowDeliberationEngine {
    pub fn new(
        workspace_dir: PathBuf,
        experience_system: Arc<ExperienceSystem>,
    ) -> Self {
        let consensus_manager = Arc::new(ConsensusManager::new(&workspace_dir));
        let chat_manager = Arc::new(SwarmChatManager::new(&workspace_dir));
        
        Self {
            workspace_dir,
            experience_system,
            consensus_manager,
            chat_manager,
            deliberations: Arc::new(RwLock::new(HashMap::new())),
            config: DeliberationConfig::default(),
        }
    }

    pub fn with_config(mut self, config: DeliberationConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn propose_workflow_optimization(
        &self,
        current_workflow: WorkflowDefinition,
        proposed_workflow: WorkflowDefinition,
        rationale: String,
        proposed_by: String,
    ) -> Result<WorkflowDeliberation> {
        let proposal = WorkflowOptimizationProposal {
            id: Uuid::new_v4().to_string(),
            current_workflow,
            proposed_workflow,
            rationale,
            expected_benefits: vec![],
            potential_risks: vec![],
            implementation_steps: vec![],
            rollback_plan: None,
            proposed_by,
            created_at: Utc::now(),
        };

        let deliberation = WorkflowDeliberation {
            id: Uuid::new_v4().to_string(),
            proposal,
            status: WorkflowDeliberationStatus::Proposed,
            participants: HashMap::new(),
            issues: Vec::new(),
            experience_references: Vec::new(),
            discussion_summary: String::new(),
            consensus_result: None,
            boss_approval: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deadline: Some(Utc::now() + chrono::Duration::seconds(self.config.timeout_seconds as i64)),
        };

        {
            let mut deliberations = self.deliberations.write().await;
            deliberations.insert(deliberation.id.clone(), deliberation.clone());
        }

        self.save_deliberation(&deliberation).await?;

        Ok(deliberation)
    }

    pub async fn join_deliberation(
        &self,
        deliberation_id: &str,
        agent_id: String,
        role: String,
    ) -> Result<()> {
        let mut deliberations = self.deliberations.write().await;
        
        if let Some(deliberation) = deliberations.get_mut(deliberation_id) {
            let participant = DeliberationParticipant {
                agent_id: agent_id.clone(),
                role,
                joined_at: Utc::now(),
                last_active: Utc::now(),
                vote: None,
                concerns: Vec::new(),
                suggestions: Vec::new(),
            };
            
            deliberation.participants.insert(agent_id, participant);
            deliberation.updated_at = Utc::now();
            
            if deliberation.participants.len() >= self.config.min_participants {
                deliberation.status = WorkflowDeliberationStatus::Discussing;
            }
        }

        Ok(())
    }

    pub async fn raise_concern(
        &self,
        deliberation_id: &str,
        agent_id: &str,
        step_id: Option<String>,
        severity: IssueSeverity,
        description: String,
        suggested_fix: Option<String>,
    ) -> Result<WorkflowIssue> {
        let issue = WorkflowIssue {
            id: Uuid::new_v4().to_string(),
            step_id,
            severity,
            description: description.clone(),
            suggested_fix,
            raised_by: agent_id.to_string(),
            timestamp: Utc::now(),
        };

        {
            let mut deliberations = self.deliberations.write().await;
            
            if let Some(deliberation) = deliberations.get_mut(deliberation_id) {
                deliberation.issues.push(issue.clone());
                deliberation.updated_at = Utc::now();
                
                if let Some(participant) = deliberation.participants.get_mut(agent_id) {
                    participant.concerns.push(description);
                    participant.last_active = Utc::now();
                }
            }
        }

        Ok(issue)
    }

    pub async fn add_suggestion(
        &self,
        deliberation_id: &str,
        agent_id: &str,
        suggestion: String,
    ) -> Result<()> {
        let mut deliberations = self.deliberations.write().await;
        
        if let Some(deliberation) = deliberations.get_mut(deliberation_id) {
            if let Some(participant) = deliberation.participants.get_mut(agent_id) {
                participant.suggestions.push(suggestion);
                participant.last_active = Utc::now();
            }
            deliberation.updated_at = Utc::now();
        }

        Ok(())
    }

    pub async fn query_relevant_experiences(
        &self,
        deliberation_id: &str,
    ) -> Result<Vec<String>> {
        let deliberations = self.deliberations.read().await;
        let _deliberation = deliberations.get(deliberation_id)
            .ok_or_else(|| anyhow::anyhow!("Deliberation not found"))?;

        let experiences = self.experience_system.replay_experience(
            "workflow_optimization",
            "process_improvement",
            &[],
            10,
        ).await;
        
        let references: Vec<String> = experiences.iter()
            .map(|e| format!("{}: {}", e.experience.title, e.experience.description))
            .collect();

        drop(deliberations);

        {
            let mut deliberations = self.deliberations.write().await;
            if let Some(d) = deliberations.get_mut(deliberation_id) {
                d.experience_references = references.clone();
                d.status = WorkflowDeliberationStatus::ExperienceReview;
                d.updated_at = Utc::now();
            }
        }

        Ok(references)
    }

    pub async fn cast_vote(
        &self,
        deliberation_id: &str,
        agent_id: &str,
        approve: bool,
    ) -> Result<()> {
        let mut deliberations = self.deliberations.write().await;
        
        if let Some(deliberation) = deliberations.get_mut(deliberation_id) {
            if let Some(participant) = deliberation.participants.get_mut(agent_id) {
                participant.vote = Some(approve);
                participant.last_active = Utc::now();
            }
            deliberation.updated_at = Utc::now();
            deliberation.status = WorkflowDeliberationStatus::ConsensusBuilding;
        }

        Ok(())
    }

    pub async fn check_consensus(&self, deliberation_id: &str) -> Result<ConsensusResult> {
        let deliberations = self.deliberations.read().await;
        let deliberation = deliberations.get(deliberation_id)
            .ok_or_else(|| anyhow::anyhow!("Deliberation not found"))?;

        let total_participants = deliberation.participants.len();
        let mut agreements = 0;
        let mut disagreements = 0;
        let mut abstentions = 0;
        let mut key_concerns = Vec::new();
        let mut resolved_issues = Vec::new();
        let mut unresolved_issues = Vec::new();

        for participant in deliberation.participants.values() {
            match participant.vote {
                Some(true) => agreements += 1,
                Some(false) => disagreements += 1,
                None => abstentions += 1,
            }
            
            key_concerns.extend(participant.concerns.clone());
        }

        for issue in &deliberation.issues {
            if issue.suggested_fix.is_some() {
                resolved_issues.push(issue.description.clone());
            } else {
                unresolved_issues.push(issue.description.clone());
            }
        }

        let voted = agreements + disagreements;
        let agreement_ratio = if voted > 0 {
            agreements as f64 / voted as f64
        } else {
            0.0
        };

        let reached = agreement_ratio >= self.config.consensus_threshold
            && voted >= self.config.min_participants;

        let final_recommendation = if reached {
            format!(
                "Consensus reached with {:.1}% agreement. {} participants approved, {} rejected.",
                agreement_ratio * 100.0,
                agreements,
                disagreements
            )
        } else {
            format!(
                "Consensus not reached. {:.1}% agreement (need {:.1}%). {} participants voted.",
                agreement_ratio * 100.0,
                self.config.consensus_threshold * 100.0,
                voted
            )
        };

        let result = ConsensusResult {
            reached,
            agreement_ratio,
            total_participants,
            agreements,
            disagreements,
            abstentions,
            key_concerns,
            resolved_issues,
            unresolved_issues,
            final_recommendation,
        };

        drop(deliberations);

        {
            let mut deliberations = self.deliberations.write().await;
            if let Some(d) = deliberations.get_mut(deliberation_id) {
                d.consensus_result = Some(result.clone());
                d.updated_at = Utc::now();
                
                if reached {
                    if self.config.require_boss_approval {
                        d.status = WorkflowDeliberationStatus::WaitingForBossApproval;
                    } else {
                        d.status = WorkflowDeliberationStatus::Approved;
                    }
                } else {
                    d.status = WorkflowDeliberationStatus::Failed;
                }
            }
        }

        Ok(result)
    }

    pub async fn boss_approve(
        &self,
        deliberation_id: &str,
        approved: bool,
        approved_by: &str,
        comment: Option<String>,
        conditions: Vec<String>,
    ) -> Result<()> {
        let mut deliberations = self.deliberations.write().await;
        
        if let Some(deliberation) = deliberations.get_mut(deliberation_id) {
            deliberation.boss_approval = Some(BossApproval {
                approved,
                approved_by: approved_by.to_string(),
                approved_at: Utc::now(),
                comment,
                conditions,
            });
            
            deliberation.status = if approved {
                WorkflowDeliberationStatus::Approved
            } else {
                WorkflowDeliberationStatus::Rejected
            };
            
            deliberation.updated_at = Utc::now();
        }

        Ok(())
    }

    pub async fn implement_optimization(&self, deliberation_id: &str) -> Result<()> {
        let mut deliberations = self.deliberations.write().await;
        
        if let Some(deliberation) = deliberations.get_mut(deliberation_id) {
            if deliberation.status == WorkflowDeliberationStatus::Approved {
                deliberation.status = WorkflowDeliberationStatus::Implemented;
                deliberation.updated_at = Utc::now();

                let title = format!("Workflow optimization: {}", deliberation.proposal.proposed_workflow.name);
                self.experience_system.record_experience(
                    ExperienceType::BestPractice,
                    MemorySystem::Semantic,
                    &title,
                    &deliberation.proposal.rationale,
                    ExperienceContext {
                        task_type: "workflow_optimization".to_string(),
                        domain: "process_improvement".to_string(),
                        environment: HashMap::new(),
                        preconditions: vec![],
                        constraints: vec![],
                        tools_used: vec![],
                    },
                    "optimization_implemented",
                    vec![
                        format!("Benefits: {:?}", deliberation.proposal.expected_benefits),
                        format!("Consensus: {:?}", deliberation.consensus_result.as_ref().map(|c| c.agreement_ratio)),
                    ],
                    vec!["workflow".to_string(), "optimization".to_string()],
                    0.8,
                    0.7,
                ).await;
            }
        }

        Ok(())
    }

    pub async fn get_deliberation(&self, deliberation_id: &str) -> Option<WorkflowDeliberation> {
        let deliberations = self.deliberations.read().await;
        deliberations.get(deliberation_id).cloned()
    }

    pub async fn get_active_deliberations(&self) -> Vec<WorkflowDeliberation> {
        let deliberations = self.deliberations.read().await;
        deliberations.values()
            .filter(|d| {
                !matches!(
                    d.status,
                    WorkflowDeliberationStatus::Implemented
                    | WorkflowDeliberationStatus::Rejected
                    | WorkflowDeliberationStatus::Failed
                )
            })
            .cloned()
            .collect()
    }

    pub async fn get_pending_boss_approvals(&self) -> Vec<WorkflowDeliberation> {
        let deliberations = self.deliberations.read().await;
        deliberations.values()
            .filter(|d| d.status == WorkflowDeliberationStatus::WaitingForBossApproval)
            .cloned()
            .collect()
    }

    pub async fn get_stats(&self) -> DeliberationStats {
        let deliberations = self.deliberations.read().await;
        
        let total = deliberations.len();
        let mut by_status = HashMap::new();
        let mut successful = 0;
        let mut total_participants = 0;
        let mut total_duration = 0i64;
        let mut boss_approved = 0;
        let mut boss_total = 0;

        for d in deliberations.values() {
            *by_status.entry(format!("{:?}", d.status)).or_default() += 1;
            total_participants += d.participants.len();
            
            if d.status == WorkflowDeliberationStatus::Implemented {
                successful += 1;
            }
            
            if let Some(duration) = (d.updated_at - d.created_at).num_seconds().checked_abs() {
                total_duration += duration;
            }
            
            if d.boss_approval.is_some() {
                boss_total += 1;
                if d.boss_approval.as_ref().map_or(false, |b| b.approved) {
                    boss_approved += 1;
                }
            }
        }

        DeliberationStats {
            total_deliberations: total,
            by_status,
            successful_optimizations: successful,
            average_participants: if total > 0 { total_participants as f64 / total as f64 } else { 0.0 },
            average_duration_seconds: if total > 0 { total_duration as f64 / total as f64 } else { 0.0 },
            boss_approval_rate: if boss_total > 0 { boss_approved as f64 / boss_total as f64 } else { 0.0 },
        }
    }

    async fn save_deliberation(&self, deliberation: &WorkflowDeliberation) -> Result<()> {
        let dir = self.workspace_dir.join("deliberations");
        std::fs::create_dir_all(&dir)?;
        
        let path = dir.join(format!("{}.json", deliberation.id));
        let content = serde_json::to_string_pretty(deliberation)?;
        std::fs::write(path, content)?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_deliberation_status_serde() {
        let status = WorkflowDeliberationStatus::WaitingForBossApproval;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"waiting_for_boss_approval\"");
    }

    #[test]
    fn test_default_config() {
        let config = DeliberationConfig::default();
        assert_eq!(config.min_participants, 2);
        assert!(config.consensus_threshold >= 0.6);
        assert!(config.require_boss_approval);
    }

    #[test]
    fn test_issue_severity_serde() {
        let severity = IssueSeverity::Critical;
        let json = serde_json::to_string(&severity).unwrap();
        let parsed: IssueSeverity = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, IssueSeverity::Critical);
    }

    #[test]
    fn test_consensus_result() {
        let result = ConsensusResult {
            reached: true,
            agreement_ratio: 0.75,
            total_participants: 4,
            agreements: 3,
            disagreements: 1,
            abstentions: 0,
            key_concerns: vec![],
            resolved_issues: vec![],
            unresolved_issues: vec![],
            final_recommendation: "Consensus reached".to_string(),
        };
        
        assert!(result.reached);
        assert_eq!(result.agreement_ratio, 0.75);
    }
}
