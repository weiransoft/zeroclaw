use crate::swarm::phase::{
    CompletionStatus, Deliverable, PhaseStatus, PhaseTransition,
    WorkflowPhase,
};
use crate::swarm::consensus::ConsensusManager;
use crate::swarm::store::SwarmSqliteStore;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct WorkflowEngine {
    workflow_store: Arc<WorkflowStore>,
    consensus_manager: Arc<ConsensusManager>,
    llm_client: Option<Arc<dyn LLMProvider>>,
}

use std::future::Future;
use std::pin::Pin;

pub trait LLMProvider: Send + Sync {
    fn complete<'a>(&'a self, prompt: &'a str) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + 'a>>;
}

pub struct WorkflowStore {
    workflows: RwLock<HashMap<String, Workflow>>,
    phases: RwLock<HashMap<String, WorkflowPhase>>,
    transitions: RwLock<Vec<PhaseTransition>>,
    sqlite_store: Option<Arc<SwarmSqliteStore>>,
}

impl WorkflowStore {
    pub fn new() -> Self {
        Self {
            workflows: RwLock::new(HashMap::new()),
            phases: RwLock::new(HashMap::new()),
            transitions: RwLock::new(Vec::new()),
            sqlite_store: None,
        }
    }
    
    pub fn with_sqlite(mut self, store: Arc<SwarmSqliteStore>) -> Self {
        self.sqlite_store = Some(store);
        self
    }
    
    pub async fn create_workflow(&self, workflow: &Workflow) -> anyhow::Result<()> {
        let mut workflows = self.workflows.write().await;
        workflows.insert(workflow.id.clone(), workflow.clone());
        Ok(())
    }
    
    pub async fn get_workflow(&self, workflow_id: &str) -> Option<Workflow> {
        let workflows = self.workflows.read().await;
        workflows.get(workflow_id).cloned()
    }
    
    pub async fn update_workflow(&self, workflow: &Workflow) -> anyhow::Result<()> {
        let mut workflows = self.workflows.write().await;
        workflows.insert(workflow.id.clone(), workflow.clone());
        Ok(())
    }
    
    pub async fn add_phase(&self, phase: &WorkflowPhase) -> anyhow::Result<()> {
        let mut phases = self.phases.write().await;
        phases.insert(phase.id.clone(), phase.clone());
        Ok(())
    }
    
    pub async fn get_phase(&self, phase_id: &str) -> Option<WorkflowPhase> {
        let phases = self.phases.read().await;
        phases.get(phase_id).cloned()
    }
    
    pub async fn update_phase(&self, phase: &WorkflowPhase) -> anyhow::Result<()> {
        let mut phases = self.phases.write().await;
        phases.insert(phase.id.clone(), phase.clone());
        Ok(())
    }
    
    pub async fn record_transition(&self, transition: &PhaseTransition) -> anyhow::Result<()> {
        let mut transitions = self.transitions.write().await;
        transitions.push(transition.clone());
        Ok(())
    }
    
    pub async fn get_transitions(&self, workflow_id: &str) -> Vec<PhaseTransition> {
        let transitions = self.transitions.read().await;
        transitions
            .iter()
            .filter(|t| {
                t.from_phase.as_ref().map(|p| p.contains(workflow_id)).unwrap_or(false)
                    || t.to_phase.as_ref().map(|p| p.contains(workflow_id)).unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: WorkflowStatus,
    pub phases: Vec<String>,
    pub current_phase_index: usize,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub team_members: HashMap<String, TeamMemberStatus>,
    pub metadata: serde_json::Value,
}

impl Workflow {
    pub fn new(id: &str, name: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            status: WorkflowStatus::Draft,
            phases: Vec::new(),
            current_phase_index: 0,
            created_at: super::now_unix(),
            started_at: None,
            completed_at: None,
            team_members: HashMap::new(),
            metadata: serde_json::json!({}),
        }
    }
    
    pub fn with_phases(mut self, phases: Vec<String>) -> Self {
        self.phases = phases;
        self
    }
    
    pub fn with_team_members(mut self, members: HashMap<String, TeamMemberStatus>) -> Self {
        self.team_members = members;
        self
    }
    
    pub fn current_phase_id(&self) -> Option<&String> {
        self.phases.get(self.current_phase_index)
    }
    
    pub fn next_phase_id(&self) -> Option<&String> {
        self.phases.get(self.current_phase_index + 1)
    }
    
    pub fn start(&mut self) -> bool {
        if matches!(self.status, WorkflowStatus::Draft | WorkflowStatus::Pending) {
            self.status = WorkflowStatus::InProgress;
            self.started_at = Some(super::now_unix());
            true
        } else {
            false
        }
    }
    
    pub fn complete(&mut self) -> bool {
        if matches!(self.status, WorkflowStatus::InProgress) {
            self.status = WorkflowStatus::Completed;
            self.completed_at = Some(super::now_unix());
            true
        } else {
            false
        }
    }
    
    pub fn advance_phase(&mut self) -> bool {
        if self.current_phase_index < self.phases.len() - 1 {
            self.current_phase_index += 1;
            true
        } else {
            false
        }
    }
    
    pub fn overall_progress(&self) -> f64 {
        if self.phases.is_empty() {
            return 0.0;
        }
        
        let completed = self.current_phase_index as f64;
        let total = self.phases.len() as f64;
        completed / total
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowStatus {
    Draft,
    Pending,
    InProgress,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMemberStatus {
    pub name: String,
    pub role: String,
    pub status: MemberAvailability,
    pub current_task: Option<String>,
    pub completed_tasks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberAvailability {
    Available,
    Busy,
    Offline,
}

impl WorkflowEngine {
    pub fn new(
        workflow_store: Arc<WorkflowStore>,
        consensus_manager: Arc<ConsensusManager>,
    ) -> Self {
        Self {
            workflow_store,
            consensus_manager,
            llm_client: None,
        }
    }
    
    pub fn with_llm_client(mut self, client: Arc<dyn LLMProvider>) -> Self {
        self.llm_client = Some(client);
        self
    }
    
    pub async fn create_workflow(
        &self,
        name: &str,
        description: &str,
        phases: Vec<WorkflowPhase>,
    ) -> anyhow::Result<Workflow> {
        let id = uuid::Uuid::new_v4().to_string();
        
        let mut workflow = Workflow::new(&id, name, description);
        
        for phase in &phases {
            workflow.phases.push(phase.id.clone());
            self.workflow_store.add_phase(phase).await?;
        }
        
        workflow.status = WorkflowStatus::Pending;
        self.workflow_store.create_workflow(&workflow).await?;
        
        Ok(workflow)
    }
    
    pub async fn start_workflow(&self, workflow_id: &str) -> anyhow::Result<PhaseTransition> {
        let mut workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        if !workflow.start() {
            anyhow::bail!("Cannot start workflow in current state");
        }
        
        if let Some(phase_id) = workflow.current_phase_id() {
            if let Some(mut phase) = self.workflow_store.get_phase(phase_id).await {
                phase.start();
                self.workflow_store.update_phase(&phase).await?;
                
                let transition = PhaseTransition::started(&phase.name);
                self.workflow_store.record_transition(&transition).await?;
                
                self.workflow_store.update_workflow(&workflow).await?;
                
                return Ok(transition);
            }
        }
        
        anyhow::bail!("No phases found in workflow");
    }
    
    pub async fn advance_phase(
        &self,
        workflow_id: &str,
        deliverables: Vec<Deliverable>,
    ) -> anyhow::Result<PhaseTransition> {
        let mut workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        let current_phase_id = workflow.current_phase_id()
            .ok_or_else(|| anyhow::anyhow!("No current phase"))?
            .clone();
        
        let mut current_phase = self.workflow_store.get_phase(&current_phase_id).await
            .ok_or_else(|| anyhow::anyhow!("Phase not found: {}", current_phase_id))?;
        
        current_phase.complete(deliverables);
        self.workflow_store.update_phase(&current_phase).await?;
        
        let transition = if workflow.advance_phase() {
            if let Some(next_phase_id) = workflow.current_phase_id() {
                if let Some(mut next_phase) = self.workflow_store.get_phase(next_phase_id).await {
                    next_phase.start();
                    self.workflow_store.update_phase(&next_phase).await?;
                    
                    PhaseTransition::advanced(&current_phase.name, &next_phase.name)
                } else {
                    PhaseTransition::completed(&current_phase.name)
                }
            } else {
                workflow.complete();
                PhaseTransition::completed(&current_phase.name)
            }
        } else {
            workflow.complete();
            PhaseTransition::completed(&current_phase.name)
        };
        
        self.workflow_store.update_workflow(&workflow).await?;
        self.workflow_store.record_transition(&transition).await?;
        
        Ok(transition)
    }
    
    pub async fn check_phase_completion(
        &self,
        workflow_id: &str,
        completed_tasks: &HashSet<String>,
        completed_documents: &HashSet<String>,
        completed_reviews: &HashSet<String>,
        completed_consensus: &HashSet<String>,
        current_metrics: &HashMap<String, f64>,
    ) -> anyhow::Result<CompletionStatus> {
        let workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        let current_phase_id = workflow.current_phase_id()
            .ok_or_else(|| anyhow::anyhow!("No current phase"))?;
        
        let phase = self.workflow_store.get_phase(current_phase_id).await
            .ok_or_else(|| anyhow::anyhow!("Phase not found: {}", current_phase_id))?;
        
        Ok(phase.check_completion(
            completed_tasks,
            completed_documents,
            completed_reviews,
            completed_consensus,
            current_metrics,
        ))
    }
    
    pub async fn get_workflow_status(&self, workflow_id: &str) -> anyhow::Result<WorkflowStatusReport> {
        let workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        let mut phases_status = Vec::new();
        for phase_id in &workflow.phases {
            if let Some(phase) = self.workflow_store.get_phase(phase_id).await {
                phases_status.push(PhaseStatusReport {
                    id: phase.id.clone(),
                    name: phase.name.clone(),
                    status: phase.status.clone(),
                    progress: phase.status.progress(),
                    is_overdue: phase.is_overdue(),
                    elapsed_hours: phase.elapsed_hours(),
                });
            }
        }
        
        let transitions = self.workflow_store.get_transitions(workflow_id).await;
        
        Ok(WorkflowStatusReport {
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            status: workflow.status.clone(),
            overall_progress: workflow.overall_progress(),
            current_phase_index: workflow.current_phase_index,
            phases: phases_status,
            recent_transitions: transitions.into_iter().rev().take(10).collect(),
            team_members: workflow.team_members.clone(),
        })
    }
    
    pub async fn pause_workflow(&self, workflow_id: &str) -> anyhow::Result<()> {
        let mut workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        if workflow.status == WorkflowStatus::InProgress {
            workflow.status = WorkflowStatus::Paused;
            self.workflow_store.update_workflow(&workflow).await?;
        }
        
        Ok(())
    }
    
    pub async fn resume_workflow(&self, workflow_id: &str) -> anyhow::Result<()> {
        let mut workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        if workflow.status == WorkflowStatus::Paused {
            workflow.status = WorkflowStatus::InProgress;
            self.workflow_store.update_workflow(&workflow).await?;
        }
        
        Ok(())
    }
    
    pub async fn cancel_workflow(&self, workflow_id: &str, reason: &str) -> anyhow::Result<()> {
        let mut workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        workflow.status = WorkflowStatus::Cancelled;
        workflow.metadata["cancellation_reason"] = serde_json::json!(reason);
        self.workflow_store.update_workflow(&workflow).await?;
        
        Ok(())
    }
    
    pub async fn update_phase_progress(
        &self,
        workflow_id: &str,
        progress: f64,
    ) -> anyhow::Result<()> {
        let workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        if let Some(phase_id) = workflow.current_phase_id() {
            if let Some(mut phase) = self.workflow_store.get_phase(phase_id).await {
                phase.status.update_progress(progress);
                self.workflow_store.update_phase(&phase).await?;
            }
        }
        
        Ok(())
    }
    
    pub async fn request_phase_approval(
        &self,
        workflow_id: &str,
        approver: &str,
        approval_type: crate::swarm::phase::ApprovalType,
    ) -> anyhow::Result<PhaseTransition> {
        let workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        if let Some(phase_id) = workflow.current_phase_id() {
            if let Some(mut phase) = self.workflow_store.get_phase(phase_id).await {
                phase.status.wait_for_approval(approver.to_string(), approval_type);
                self.workflow_store.update_phase(&phase).await?;
                
                let transition = PhaseTransition::waiting_for_approval(&phase.name, approver);
                self.workflow_store.record_transition(&transition).await?;
                
                return Ok(transition);
            }
        }
        
        anyhow::bail!("No current phase to request approval");
    }
    
    pub async fn approve_phase(
        &self,
        workflow_id: &str,
        approved: bool,
        deliverables: Vec<Deliverable>,
    ) -> anyhow::Result<PhaseTransition> {
        let workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        if let Some(phase_id) = workflow.current_phase_id() {
            if let Some(mut phase) = self.workflow_store.get_phase(phase_id).await {
                if approved {
                    return self.advance_phase(workflow_id, deliverables).await;
                } else {
                    phase.status.needs_adjustment(
                        "审批未通过".to_string(),
                        vec!["请根据反馈进行修改".to_string()],
                    );
                    self.workflow_store.update_phase(&phase).await?;
                    
                    let transition = PhaseTransition::needs_adjustment(&phase.name, "审批未通过");
                    self.workflow_store.record_transition(&transition).await?;
                    
                    return Ok(transition);
                }
            }
        }
        
        anyhow::bail!("No current phase to approve");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStatusReport {
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: WorkflowStatus,
    pub overall_progress: f64,
    pub current_phase_index: usize,
    pub phases: Vec<PhaseStatusReport>,
    pub recent_transitions: Vec<PhaseTransition>,
    pub team_members: HashMap<String, TeamMemberStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseStatusReport {
    pub id: String,
    pub name: String,
    pub status: PhaseStatus,
    pub progress: f64,
    pub is_overdue: bool,
    pub elapsed_hours: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm::phase::{PhaseType, TransitionType};
    
    fn create_test_phase(id: &str, name: &str) -> WorkflowPhase {
        WorkflowPhase::new(
            id,
            name,
            "Test phase",
            PhaseType::Development {
                dev_progress: 0.0,
                code_review_rate: 0.0,
                test_coverage: 0.0,
            },
        )
    }
    
    #[tokio::test]
    async fn test_create_workflow() {
        let store = Arc::new(WorkflowStore::new());
        let temp_dir = std::env::temp_dir();
        let consensus = Arc::new(ConsensusManager::new(&temp_dir));
        let engine = WorkflowEngine::new(store.clone(), consensus);
        
        let phases = vec![
            create_test_phase("phase-1", "需求分析"),
            create_test_phase("phase-2", "开发实现"),
        ];
        
        let workflow = engine.create_workflow("测试工作流", "测试描述", phases).await.unwrap();
        
        assert_eq!(workflow.name, "测试工作流");
        assert_eq!(workflow.phases.len(), 2);
        assert_eq!(workflow.status, WorkflowStatus::Pending);
    }
    
    #[tokio::test]
    async fn test_start_workflow() {
        let store = Arc::new(WorkflowStore::new());
        let temp_dir = std::env::temp_dir();
        let consensus = Arc::new(ConsensusManager::new(&temp_dir));
        let engine = WorkflowEngine::new(store.clone(), consensus);
        
        let phases = vec![create_test_phase("phase-1", "需求分析")];
        let workflow = engine.create_workflow("测试工作流", "测试描述", phases).await.unwrap();
        
        let transition = engine.start_workflow(&workflow.id).await.unwrap();
        
        assert_eq!(transition.transition_type, TransitionType::Started);
        
        let updated = store.get_workflow(&workflow.id).await.unwrap();
        assert_eq!(updated.status, WorkflowStatus::InProgress);
    }
    
    #[tokio::test]
    async fn test_advance_phase() {
        let store = Arc::new(WorkflowStore::new());
        let temp_dir = std::env::temp_dir();
        let consensus = Arc::new(ConsensusManager::new(&temp_dir));
        let engine = WorkflowEngine::new(store.clone(), consensus);
        
        let phases = vec![
            create_test_phase("phase-1", "需求分析"),
            create_test_phase("phase-2", "开发实现"),
        ];
        let workflow = engine.create_workflow("测试工作流", "测试描述", phases).await.unwrap();
        
        engine.start_workflow(&workflow.id).await.unwrap();
        
        let deliverables = vec![Deliverable {
            id: "d1".to_string(),
            name: "需求文档".to_string(),
            description: "产品需求文档".to_string(),
            deliverable_type: crate::swarm::phase::DeliverableType::Document,
            content: Some("需求内容".to_string()),
            file_path: None,
            is_knowledge: false,
            created_at: now_unix(),
        }];
        
        let transition = engine.advance_phase(&workflow.id, deliverables).await.unwrap();
        
        assert_eq!(transition.transition_type, TransitionType::Advanced);
        assert_eq!(transition.from_phase, Some("需求分析".to_string()));
        assert_eq!(transition.to_phase, Some("开发实现".to_string()));
    }
    
    #[tokio::test]
    async fn test_workflow_status() {
        let store = Arc::new(WorkflowStore::new());
        let temp_dir = std::env::temp_dir();
        let consensus = Arc::new(ConsensusManager::new(&temp_dir));
        let engine = WorkflowEngine::new(store.clone(), consensus);
        
        let phases = vec![create_test_phase("phase-1", "需求分析")];
        let workflow = engine.create_workflow("测试工作流", "测试描述", phases).await.unwrap();
        
        let report = engine.get_workflow_status(&workflow.id).await.unwrap();
        
        assert_eq!(report.workflow_name, "测试工作流");
        assert_eq!(report.phases.len(), 1);
    }
    
    #[tokio::test]
    async fn test_pause_resume_workflow() {
        let store = Arc::new(WorkflowStore::new());
        let temp_dir = std::env::temp_dir();
        let consensus = Arc::new(ConsensusManager::new(&temp_dir));
        let engine = WorkflowEngine::new(store.clone(), consensus);
        
        let phases = vec![create_test_phase("phase-1", "需求分析")];
        let workflow = engine.create_workflow("测试工作流", "测试描述", phases).await.unwrap();
        
        engine.start_workflow(&workflow.id).await.unwrap();
        
        engine.pause_workflow(&workflow.id).await.unwrap();
        let paused = store.get_workflow(&workflow.id).await.unwrap();
        assert_eq!(paused.status, WorkflowStatus::Paused);
        
        engine.resume_workflow(&workflow.id).await.unwrap();
        let resumed = store.get_workflow(&workflow.id).await.unwrap();
        assert_eq!(resumed.status, WorkflowStatus::InProgress);
    }
    
    #[tokio::test]
    async fn test_cancel_workflow() {
        let store = Arc::new(WorkflowStore::new());
        let temp_dir = std::env::temp_dir();
        let consensus = Arc::new(ConsensusManager::new(&temp_dir));
        let engine = WorkflowEngine::new(store.clone(), consensus);
        
        let phases = vec![create_test_phase("phase-1", "需求分析")];
        let workflow = engine.create_workflow("测试工作流", "测试描述", phases).await.unwrap();
        
        engine.cancel_workflow(&workflow.id, "测试取消").await.unwrap();
        
        let cancelled = store.get_workflow(&workflow.id).await.unwrap();
        assert_eq!(cancelled.status, WorkflowStatus::Cancelled);
    }
}
