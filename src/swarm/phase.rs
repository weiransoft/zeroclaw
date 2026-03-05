use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PhaseStatus {
    Pending,
    InProgress {
        started_at: u64,
        estimated_completion: u64,
        progress: f64,
    },
    WaitingForDependencies {
        dependencies: Vec<String>,
    },
    WaitingForApproval {
        approver: String,
        approval_type: ApprovalType,
    },
    WaitingForConsensus {
        proposal_id: String,
        vote_status: VoteStatus,
    },
    Completed {
        completed_at: u64,
        deliverables: Vec<Deliverable>,
    },
    NeedsAdjustment {
        reason: String,
        suggestions: Vec<String>,
    },
}

impl Default for PhaseStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl PhaseStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }
    
    pub fn is_in_progress(&self) -> bool {
        matches!(self, Self::InProgress { .. })
    }
    
    pub fn is_waiting(&self) -> bool {
        matches!(
            self,
            Self::WaitingForDependencies { .. }
                | Self::WaitingForApproval { .. }
                | Self::WaitingForConsensus { .. }
        )
    }
    
    pub fn progress(&self) -> f64 {
        match self {
            Self::Pending => 0.0,
            Self::InProgress { progress, .. } => *progress,
            Self::WaitingForDependencies { .. } => 0.0,
            Self::WaitingForApproval { .. } => 0.9,
            Self::WaitingForConsensus { .. } => 0.85,
            Self::Completed { .. } => 1.0,
            Self::NeedsAdjustment { .. } => 0.0,
        }
    }
    
    pub fn start(&mut self, started_at: u64, estimated_completion: u64) -> bool {
        if matches!(self, Self::Pending) {
            *self = Self::InProgress {
                started_at,
                estimated_completion,
                progress: 0.0,
            };
            return true;
        }
        false
    }
    
    pub fn update_progress(&mut self, new_progress: f64) -> bool {
        if let Self::InProgress {
            progress,
            estimated_completion: _,
            started_at: _,
        } = self
        {
            *progress = new_progress.clamp(0.0, 1.0);
            return true;
        }
        false
    }
    
    pub fn complete(&mut self, completed_at: u64, deliverables: Vec<Deliverable>) -> bool {
        if matches!(self, Self::InProgress { .. } | Self::WaitingForApproval { .. } | Self::WaitingForConsensus { .. }) {
            *self = Self::Completed {
                completed_at,
                deliverables,
            };
            return true;
        }
        false
    }
    
    pub fn wait_for_dependencies(&mut self, dependencies: Vec<String>) -> bool {
        if matches!(self, Self::Pending | Self::InProgress { .. }) {
            *self = Self::WaitingForDependencies { dependencies };
            return true;
        }
        false
    }
    
    pub fn wait_for_approval(&mut self, approver: String, approval_type: ApprovalType) -> bool {
        if matches!(self, Self::InProgress { .. }) {
            *self = Self::WaitingForApproval {
                approver,
                approval_type,
            };
            return true;
        }
        false
    }
    
    pub fn wait_for_consensus(&mut self, proposal_id: String, vote_status: VoteStatus) -> bool {
        if matches!(self, Self::InProgress { .. }) {
            *self = Self::WaitingForConsensus {
                proposal_id,
                vote_status,
            };
            return true;
        }
        false
    }
    
    pub fn needs_adjustment(&mut self, reason: String, suggestions: Vec<String>) -> bool {
        if !self.is_terminal() {
            *self = Self::NeedsAdjustment {
                reason,
                suggestions,
            };
            return true;
        }
        false
    }
    
    pub fn resume(&mut self, estimated_completion: u64) -> bool {
        if let Self::NeedsAdjustment { .. } = self {
            let now = super::now_unix();
            *self = Self::InProgress {
                started_at: now,
                estimated_completion,
                progress: 0.0,
            };
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApprovalType {
    BossApproval,
    TechnicalReview,
    CustomerAcceptance,
    TeamConsensus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoteStatus {
    pub total_voters: u32,
    pub approvals: u32,
    pub rejections: u32,
    pub pending: u32,
    pub required_approvals: u32,
}

impl VoteStatus {
    pub fn new(total_voters: u32, required_approvals: u32) -> Self {
        Self {
            total_voters,
            approvals: 0,
            rejections: 0,
            pending: total_voters,
            required_approvals,
        }
    }
    
    pub fn is_approved(&self) -> bool {
        self.approvals >= self.required_approvals
    }
    
    pub fn is_rejected(&self) -> bool {
        self.rejections > self.total_voters - self.required_approvals
    }
    
    pub fn vote(&mut self, approve: bool) -> bool {
        if self.pending == 0 {
            return false;
        }
        self.pending -= 1;
        if approve {
            self.approvals += 1;
        } else {
            self.rejections += 1;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Deliverable {
    pub id: String,
    pub name: String,
    pub description: String,
    pub deliverable_type: DeliverableType,
    pub content: Option<String>,
    pub file_path: Option<String>,
    pub is_knowledge: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeliverableType {
    Document,
    Code,
    TestResult,
    ReviewReport,
    KnowledgeEntry,
    ExperienceRecord,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhaseCompletionCriteria {
    pub required_tasks: Vec<String>,
    pub required_documents: Vec<String>,
    pub required_reviews: Vec<String>,
    pub required_consensus: Vec<String>,
    pub quality_metrics: HashMap<String, f64>,
}

impl PhaseCompletionCriteria {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_required_tasks(mut self, tasks: Vec<String>) -> Self {
        self.required_tasks = tasks;
        self
    }
    
    pub fn with_required_documents(mut self, docs: Vec<String>) -> Self {
        self.required_documents = docs;
        self
    }
    
    pub fn with_required_reviews(mut self, reviews: Vec<String>) -> Self {
        self.required_reviews = reviews;
        self
    }
    
    pub fn with_required_consensus(mut self, consensus: Vec<String>) -> Self {
        self.required_consensus = consensus;
        self
    }
    
    pub fn with_quality_metric(mut self, name: &str, target: f64) -> Self {
        self.quality_metrics.insert(name.to_string(), target);
        self
    }
    
    pub fn check_completion(
        &self,
        completed_tasks: &HashSet<String>,
        completed_documents: &HashSet<String>,
        completed_reviews: &HashSet<String>,
        completed_consensus: &HashSet<String>,
        current_metrics: &HashMap<String, f64>,
    ) -> CompletionStatus {
        let missing_tasks: Vec<String> = self
            .required_tasks
            .iter()
            .filter(|t| !completed_tasks.contains(*t))
            .cloned()
            .collect();
        
        let missing_documents: Vec<String> = self
            .required_documents
            .iter()
            .filter(|d| !completed_documents.contains(*d))
            .cloned()
            .collect();
        
        let missing_reviews: Vec<String> = self
            .required_reviews
            .iter()
            .filter(|r| !completed_reviews.contains(*r))
            .cloned()
            .collect();
        
        let missing_consensus: Vec<String> = self
            .required_consensus
            .iter()
            .filter(|c| !completed_consensus.contains(*c))
            .cloned()
            .collect();
        
        let mut unmet_metrics: Vec<(String, f64, f64)> = Vec::new();
        for (metric, target) in &self.quality_metrics {
            if let Some(current) = current_metrics.get(metric) {
                if current < target {
                    unmet_metrics.push((metric.clone(), *current, *target));
                }
            } else {
                unmet_metrics.push((metric.clone(), 0.0, *target));
            }
        }
        
        let is_complete = missing_tasks.is_empty()
            && missing_documents.is_empty()
            && missing_reviews.is_empty()
            && missing_consensus.is_empty()
            && unmet_metrics.is_empty();
        
        let total_requirements = self.required_tasks.len()
            + self.required_documents.len()
            + self.required_reviews.len()
            + self.required_consensus.len()
            + self.quality_metrics.len();
        
        let completed_requirements = total_requirements
            - missing_tasks.len()
            - missing_documents.len()
            - missing_reviews.len()
            - missing_consensus.len()
            - unmet_metrics.len();
        
        let progress = if total_requirements > 0 {
            completed_requirements as f64 / total_requirements as f64
        } else {
            1.0
        };
        
        CompletionStatus {
            is_complete,
            progress,
            missing_tasks,
            missing_documents,
            missing_reviews,
            missing_consensus,
            unmet_metrics,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionStatus {
    pub is_complete: bool,
    pub progress: f64,
    pub missing_tasks: Vec<String>,
    pub missing_documents: Vec<String>,
    pub missing_reviews: Vec<String>,
    pub missing_consensus: Vec<String>,
    pub unmet_metrics: Vec<(String, f64, f64)>,
}

impl CompletionStatus {
    pub fn summary(&self) -> String {
        if self.is_complete {
            return "所有完成条件已满足".to_string();
        }
        
        let mut issues = Vec::new();
        
        if !self.missing_tasks.is_empty() {
            issues.push(format!("缺失任务: {}", self.missing_tasks.join(", ")));
        }
        if !self.missing_documents.is_empty() {
            issues.push(format!("缺失文档: {}", self.missing_documents.join(", ")));
        }
        if !self.missing_reviews.is_empty() {
            issues.push(format!("缺失评审: {}", self.missing_reviews.join(", ")));
        }
        if !self.missing_consensus.is_empty() {
            issues.push(format!("缺失共识: {}", self.missing_consensus.join(", ")));
        }
        if !self.unmet_metrics.is_empty() {
            let metrics: Vec<String> = self
                .unmet_metrics
                .iter()
                .map(|(name, current, target)| {
                    format!("{}: {}/{}", name, current, target)
                })
                .collect();
            issues.push(format!("未达标指标: {}", metrics.join(", ")));
        }
        
        issues.join("; ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PhaseType {
    RequirementAnalysis {
        collection_progress: f64,
        review_pass_rate: f64,
    },
    ArchitectureDesign {
        design_progress: f64,
        tech_review_status: ReviewStatus,
    },
    TaskBreakdown {
        breakdown_progress: f64,
        estimation_progress: f64,
    },
    Development {
        dev_progress: f64,
        code_review_rate: f64,
        test_coverage: f64,
    },
    Testing {
        test_case_progress: f64,
        test_pass_rate: f64,
        bug_fix_rate: f64,
    },
    ReviewDelivery {
        review_progress: f64,
        delivery_readiness: f64,
    },
    Retrospective {
        retro_progress: f64,
        improvement_count: u32,
    },
    Custom {
        name: String,
        progress: f64,
    },
}

impl Default for PhaseType {
    fn default() -> Self {
        Self::Custom {
            name: "未定义阶段".to_string(),
            progress: 0.0,
        }
    }
}

impl PhaseType {
    pub fn name(&self) -> &str {
        match self {
            Self::RequirementAnalysis { .. } => "需求分析",
            Self::ArchitectureDesign { .. } => "架构设计",
            Self::TaskBreakdown { .. } => "任务分解",
            Self::Development { .. } => "开发实现",
            Self::Testing { .. } => "测试验证",
            Self::ReviewDelivery { .. } => "评审交付",
            Self::Retrospective { .. } => "回顾改进",
            Self::Custom { name, .. } => name,
        }
    }
    
    pub fn progress(&self) -> f64 {
        match self {
            Self::RequirementAnalysis {
                collection_progress,
                review_pass_rate,
            } => (collection_progress + review_pass_rate) / 2.0,
            Self::ArchitectureDesign {
                design_progress,
                tech_review_status,
            } => {
                let review_factor = match tech_review_status {
                    ReviewStatus::Pending => 0.0,
                    ReviewStatus::InProgress => 0.5,
                    ReviewStatus::Passed => 1.0,
                    ReviewStatus::Failed => 0.3,
                };
                (design_progress + review_factor) / 2.0
            }
            Self::TaskBreakdown {
                breakdown_progress,
                estimation_progress,
            } => (breakdown_progress + estimation_progress) / 2.0,
            Self::Development {
                dev_progress,
                code_review_rate,
                test_coverage,
            } => (dev_progress + code_review_rate + test_coverage) / 3.0,
            Self::Testing {
                test_case_progress,
                test_pass_rate,
                bug_fix_rate,
            } => (test_case_progress + test_pass_rate + bug_fix_rate) / 3.0,
            Self::ReviewDelivery {
                review_progress,
                delivery_readiness,
            } => (review_progress + delivery_readiness) / 2.0,
            Self::Retrospective {
                retro_progress, ..
            } => *retro_progress,
            Self::Custom { progress, .. } => *progress,
        }
    }
    
    pub fn update_progress(&mut self, new_progress: f64) -> bool {
        let clamped = new_progress.clamp(0.0, 1.0);
        match self {
            Self::RequirementAnalysis {
                collection_progress, ..
            } => {
                *collection_progress = clamped;
                true
            }
            Self::ArchitectureDesign {
                design_progress, ..
            } => {
                *design_progress = clamped;
                true
            }
            Self::TaskBreakdown {
                breakdown_progress, ..
            } => {
                *breakdown_progress = clamped;
                true
            }
            Self::Development { dev_progress, .. } => {
                *dev_progress = clamped;
                true
            }
            Self::Testing {
                test_case_progress, ..
            } => {
                *test_case_progress = clamped;
                true
            }
            Self::ReviewDelivery {
                review_progress, ..
            } => {
                *review_progress = clamped;
                true
            }
            Self::Retrospective {
                retro_progress, ..
            } => {
                *retro_progress = clamped;
                true
            }
            Self::Custom { progress, .. } => {
                *progress = clamped;
                true
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReviewStatus {
    Pending,
    InProgress,
    Passed,
    Failed,
}

impl Default for ReviewStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub id: String,
    pub name: String,
    pub description: String,
    pub phase_type: PhaseType,
    pub status: PhaseStatus,
    pub assigned_to: Vec<String>,
    pub dependencies: Vec<String>,
    pub completion_criteria: PhaseCompletionCriteria,
    pub deliverables: Vec<Deliverable>,
    pub requires_boss_approval: bool,
    pub estimated_duration_hours: Option<f64>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub metadata: serde_json::Value,
}

impl WorkflowPhase {
    pub fn new(id: &str, name: &str, description: &str, phase_type: PhaseType) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            phase_type,
            status: PhaseStatus::Pending,
            assigned_to: Vec::new(),
            dependencies: Vec::new(),
            completion_criteria: PhaseCompletionCriteria::default(),
            deliverables: Vec::new(),
            requires_boss_approval: false,
            estimated_duration_hours: None,
            created_at: super::now_unix(),
            started_at: None,
            completed_at: None,
            metadata: serde_json::json!({}),
        }
    }
    
    pub fn with_assignees(mut self, assignees: Vec<String>) -> Self {
        self.assigned_to = assignees;
        self
    }
    
    pub fn with_dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }
    
    pub fn with_completion_criteria(mut self, criteria: PhaseCompletionCriteria) -> Self {
        self.completion_criteria = criteria;
        self
    }
    
    pub fn with_boss_approval(mut self, required: bool) -> Self {
        self.requires_boss_approval = required;
        self
    }
    
    pub fn with_estimated_duration(mut self, hours: f64) -> Self {
        self.estimated_duration_hours = Some(hours);
        self
    }
    
    pub fn start(&mut self) -> bool {
        if self.status.start(
            super::now_unix(),
            self.estimated_duration_hours
                .map(|h| super::now_unix() + (h * 3600.0) as u64)
                .unwrap_or(super::now_unix() + 86400),
        ) {
            self.started_at = Some(super::now_unix());
            true
        } else {
            false
        }
    }
    
    pub fn complete(&mut self, deliverables: Vec<Deliverable>) -> bool {
        let completed_at = super::now_unix();
        if self.status.complete(completed_at, deliverables.clone()) {
            self.completed_at = Some(completed_at);
            self.deliverables = deliverables;
            true
        } else {
            false
        }
    }
    
    pub fn elapsed_hours(&self) -> f64 {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => (end - start) as f64 / 3600.0,
            (Some(start), None) => (super::now_unix() - start) as f64 / 3600.0,
            _ => 0.0,
        }
    }
    
    pub fn is_overdue(&self) -> bool {
        if let Some(estimated) = self.estimated_duration_hours {
            self.elapsed_hours() > estimated
        } else {
            false
        }
    }
    
    pub fn add_deliverable(&mut self, deliverable: Deliverable) {
        self.deliverables.push(deliverable);
    }
    
    pub fn check_completion(
        &self,
        completed_tasks: &HashSet<String>,
        completed_documents: &HashSet<String>,
        completed_reviews: &HashSet<String>,
        completed_consensus: &HashSet<String>,
        current_metrics: &HashMap<String, f64>,
    ) -> CompletionStatus {
        self.completion_criteria.check_completion(
            completed_tasks,
            completed_documents,
            completed_reviews,
            completed_consensus,
            current_metrics,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTransition {
    pub from_phase: Option<String>,
    pub to_phase: Option<String>,
    pub transition_type: TransitionType,
    pub timestamp: u64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransitionType {
    Started,
    Advanced,
    WaitingForDependencies,
    WaitingForApproval,
    WaitingForConsensus,
    NeedsAdjustment,
    Completed,
    Cancelled,
}

impl PhaseTransition {
    pub fn started(phase_name: &str) -> Self {
        Self {
            from_phase: None,
            to_phase: Some(phase_name.to_string()),
            transition_type: TransitionType::Started,
            timestamp: super::now_unix(),
            reason: None,
        }
    }
    
    pub fn advanced(from: &str, to: &str) -> Self {
        Self {
            from_phase: Some(from.to_string()),
            to_phase: Some(to.to_string()),
            transition_type: TransitionType::Advanced,
            timestamp: super::now_unix(),
            reason: None,
        }
    }
    
    pub fn waiting_for_dependencies(phase_name: &str, dependencies: &[String]) -> Self {
        Self {
            from_phase: Some(phase_name.to_string()),
            to_phase: Some(phase_name.to_string()),
            transition_type: TransitionType::WaitingForDependencies,
            timestamp: super::now_unix(),
            reason: Some(format!("等待依赖: {}", dependencies.join(", "))),
        }
    }
    
    pub fn waiting_for_approval(phase_name: &str, approver: &str) -> Self {
        Self {
            from_phase: Some(phase_name.to_string()),
            to_phase: Some(phase_name.to_string()),
            transition_type: TransitionType::WaitingForApproval,
            timestamp: super::now_unix(),
            reason: Some(format!("等待 {} 审批", approver)),
        }
    }
    
    pub fn waiting_for_consensus(phase_name: &str, proposal_id: &str) -> Self {
        Self {
            from_phase: Some(phase_name.to_string()),
            to_phase: Some(phase_name.to_string()),
            transition_type: TransitionType::WaitingForConsensus,
            timestamp: super::now_unix(),
            reason: Some(format!("等待共识决策: {}", proposal_id)),
        }
    }
    
    pub fn needs_adjustment(phase_name: &str, reason: &str) -> Self {
        Self {
            from_phase: Some(phase_name.to_string()),
            to_phase: Some(phase_name.to_string()),
            transition_type: TransitionType::NeedsAdjustment,
            timestamp: super::now_unix(),
            reason: Some(reason.to_string()),
        }
    }
    
    pub fn completed(phase_name: &str) -> Self {
        Self {
            from_phase: Some(phase_name.to_string()),
            to_phase: None,
            transition_type: TransitionType::Completed,
            timestamp: super::now_unix(),
            reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn now_unix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    
    #[test]
    fn test_phase_status_transitions() {
        let mut status = PhaseStatus::Pending;
        
        assert!(status.start(now_unix(), now_unix() + 3600));
        assert!(status.is_in_progress());
        
        assert!(status.update_progress(0.5));
        assert_eq!(status.progress(), 0.5);
        
        let deliverables = vec![Deliverable {
            id: "d1".to_string(),
            name: "Test Doc".to_string(),
            description: "Test".to_string(),
            deliverable_type: DeliverableType::Document,
            content: Some("content".to_string()),
            file_path: None,
            is_knowledge: false,
            created_at: now_unix(),
        }];
        
        assert!(status.complete(now_unix(), deliverables));
        assert!(status.is_terminal());
    }
    
    #[test]
    fn test_phase_status_waiting_states() {
        let mut status = PhaseStatus::Pending;
        assert!(status.start(now_unix(), now_unix() + 3600));
        
        assert!(status.wait_for_dependencies(vec!["phase1".to_string()]));
        assert!(status.is_waiting());
        
        let status2 = PhaseStatus::InProgress {
            started_at: now_unix(),
            estimated_completion: now_unix() + 3600,
            progress: 0.0,
        };
        
        let mut status3 = status2;
        assert!(status3.wait_for_approval("boss".to_string(), ApprovalType::BossApproval));
        assert!(status3.is_waiting());
        
        let status4 = PhaseStatus::InProgress {
            started_at: now_unix(),
            estimated_completion: now_unix() + 3600,
            progress: 0.0,
        };
        
        let mut status5 = status4;
        let vote_status = VoteStatus::new(5, 3);
        assert!(status5.wait_for_consensus("proposal-1".to_string(), vote_status));
        assert!(status5.is_waiting());
    }
    
    #[test]
    fn test_vote_status() {
        let mut vote = VoteStatus::new(5, 3);
        
        assert!(!vote.is_approved());
        assert!(!vote.is_rejected());
        
        vote.vote(true);
        vote.vote(true);
        vote.vote(true);
        
        assert!(vote.is_approved());
        assert!(!vote.is_rejected());
    }
    
    #[test]
    fn test_completion_criteria() {
        let criteria = PhaseCompletionCriteria::new()
            .with_required_tasks(vec!["task1".to_string(), "task2".to_string()])
            .with_required_documents(vec!["doc1".to_string()])
            .with_quality_metric("coverage", 0.8);
        
        let completed_tasks: HashSet<String> = vec!["task1".to_string()].into_iter().collect();
        let completed_docs: HashSet<String> = vec!["doc1".to_string()].into_iter().collect();
        let completed_reviews: HashSet<String> = HashSet::new();
        let completed_consensus: HashSet<String> = HashSet::new();
        let metrics: HashMap<String, f64> = vec![("coverage".to_string(), 0.9)].into_iter().collect();
        
        let status = criteria.check_completion(
            &completed_tasks,
            &completed_docs,
            &completed_reviews,
            &completed_consensus,
            &metrics,
        );
        
        assert!(!status.is_complete);
        assert!(status.missing_tasks.contains(&"task2".to_string()));
        assert!(status.progress > 0.0 && status.progress < 1.0);
    }
    
    #[test]
    fn test_phase_type_progress() {
        let phase_type = PhaseType::Development {
            dev_progress: 0.8,
            code_review_rate: 0.9,
            test_coverage: 0.7,
        };
        
        let progress = phase_type.progress();
        assert!((progress - 0.8).abs() < 0.01);
    }
    
    #[test]
    fn test_workflow_phase() {
        let mut phase = WorkflowPhase::new(
            "phase-1",
            "开发阶段",
            "实现功能代码",
            PhaseType::Development {
                dev_progress: 0.0,
                code_review_rate: 0.0,
                test_coverage: 0.0,
            },
        )
        .with_assignees(vec!["developer1".to_string()])
        .with_estimated_duration(8.0);
        
        assert!(phase.start());
        assert!(phase.started_at.is_some());
        assert!(phase.status.is_in_progress());
        
        let deliverables = vec![Deliverable {
            id: "d1".to_string(),
            name: "代码".to_string(),
            description: "实现代码".to_string(),
            deliverable_type: DeliverableType::Code,
            content: None,
            file_path: Some("src/main.rs".to_string()),
            is_knowledge: false,
            created_at: now_unix(),
        }];
        
        assert!(phase.complete(deliverables));
        assert!(phase.completed_at.is_some());
        assert!(phase.status.is_terminal());
    }
    
    #[test]
    fn test_phase_transition() {
        let transition = PhaseTransition::advanced("需求分析", "架构设计");
        
        assert_eq!(transition.from_phase, Some("需求分析".to_string()));
        assert_eq!(transition.to_phase, Some("架构设计".to_string()));
        assert_eq!(transition.transition_type, TransitionType::Advanced);
    }
}
