use crate::swarm::chat::{ChatMessage, ChatMessageType, SwarmChatManager};
use crate::swarm::consensus::{ConsensusManager, ConsensusProposal};
use crate::swarm::store::SwarmSqliteStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DependencyType {
    Sequential,
    Parallel,
    Conditional,
    DataFlow,
    ResourceSharing,
}

impl fmt::Display for DependencyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DependencyType::Sequential => write!(f, "Sequential"),
            DependencyType::Parallel => write!(f, "Parallel"),
            DependencyType::Conditional => write!(f, "Conditional"),
            DependencyType::DataFlow => write!(f, "DataFlow"),
            DependencyType::ResourceSharing => write!(f, "ResourceSharing"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskDependencyStatus {
    Pending,
    Ready,
    InProgress,
    Completed,
    Failed,
    Blocked,
    Skipped,
}

impl fmt::Display for TaskDependencyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskDependencyStatus::Pending => write!(f, "Pending"),
            TaskDependencyStatus::Ready => write!(f, "Ready"),
            TaskDependencyStatus::InProgress => write!(f, "InProgress"),
            TaskDependencyStatus::Completed => write!(f, "Completed"),
            TaskDependencyStatus::Failed => write!(f, "Failed"),
            TaskDependencyStatus::Blocked => write!(f, "Blocked"),
            TaskDependencyStatus::Skipped => write!(f, "Skipped"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependency {
    pub task_id: String,
    pub depends_on: Vec<String>,
    pub dependency_type: DependencyType,
    pub condition: Option<String>,
    pub required_data: Option<Vec<String>>,
    pub required_resources: Option<Vec<String>>,
    pub status: TaskDependencyStatus,
    pub blocking_reason: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub tasks: HashMap<String, TaskNode>,
    pub edges: Vec<DependencyEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub task_id: String,
    pub task_name: String,
    pub assigned_to: Option<String>,
    pub status: TaskDependencyStatus,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
    pub estimated_duration: Option<u64>,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from_task: String,
    pub to_task: String,
    pub dependency_type: DependencyType,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAnalysis {
    pub task_id: String,
    pub total_dependencies: usize,
    pub completed_dependencies: usize,
    pub pending_dependencies: usize,
    pub blocked_dependencies: usize,
    pub can_start: bool,
    pub blocking_tasks: Vec<String>,
    pub ready_tasks: Vec<String>,
    pub critical_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCoordinationRequest {
    pub request_id: String,
    pub from_task: String,
    pub to_task: String,
    pub coordination_type: CoordinationType,
    pub message: String,
    pub data: serde_json::Value,
    pub lang: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationType {
    RequestData,
    ProvideData,
    RequestResource,
    ReleaseResource,
    Synchronize,
    NotifyCompletion,
    RequestApproval,
    GrantApproval,
    RejectApproval,
    Handover,
}

impl fmt::Display for CoordinationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoordinationType::RequestData => write!(f, "RequestData"),
            CoordinationType::ProvideData => write!(f, "ProvideData"),
            CoordinationType::RequestResource => write!(f, "RequestResource"),
            CoordinationType::ReleaseResource => write!(f, "ReleaseResource"),
            CoordinationType::Synchronize => write!(f, "Synchronize"),
            CoordinationType::NotifyCompletion => write!(f, "NotifyCompletion"),
            CoordinationType::RequestApproval => write!(f, "RequestApproval"),
            CoordinationType::GrantApproval => write!(f, "GrantApproval"),
            CoordinationType::RejectApproval => write!(f, "RejectApproval"),
            CoordinationType::Handover => write!(f, "Handover"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCoordinationResponse {
    pub request_id: String,
    pub response_type: CoordinationResponseType,
    pub message: String,
    pub data: serde_json::Value,
    pub responder: String,
    pub responded_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationResponseType {
    Approved,
    Rejected,
    Deferred,
    Completed,
    Failed,
    Acknowledged,
}

pub struct TaskDependencyManager {
    chat_manager: Arc<SwarmChatManager>,
    consensus_manager: Arc<ConsensusManager>,
    store: Arc<SwarmSqliteStore>,
    workspace_dir: std::path::PathBuf,
}

impl TaskDependencyManager {
    pub fn new(workspace_dir: &std::path::Path) -> Self {
        let chat_manager = Arc::new(SwarmChatManager::new(workspace_dir));
        let consensus_manager = Arc::new(ConsensusManager::new(workspace_dir));
        let store = Arc::new(SwarmSqliteStore::new(workspace_dir));
        Self {
            chat_manager,
            consensus_manager,
            store,
            workspace_dir: workspace_dir.to_path_buf(),
        }
    }

    pub fn define_dependency(
        &self,
        task_id: String,
        depends_on: Vec<String>,
        dependency_type: DependencyType,
        condition: Option<String>,
        required_data: Option<Vec<String>>,
        required_resources: Option<Vec<String>>,
    ) -> Result<TaskDependency> {
        let now = now_unix();
        
        tracing::info!(
            "[TaskDependency] Defined dependency for task {}: depends on {:?}, type: {}",
            task_id,
            depends_on,
            dependency_type
        );

        let dependency = TaskDependency {
            task_id: task_id.clone(),
            depends_on: depends_on.clone(),
            dependency_type: dependency_type.clone(),
            condition,
            required_data,
            required_resources,
            status: TaskDependencyStatus::Pending,
            blocking_reason: None,
            created_at: now,
            updated_at: now,
        };

        Ok(dependency)
    }

    pub fn analyze_dependencies(
        &self,
        task_id: String,
        all_tasks: &HashMap<String, TaskNode>,
    ) -> Result<DependencyAnalysis> {
        let task = all_tasks.get(&task_id)
            .context(format!("Task {} not found in task list", task_id))?;

        let mut total_dependencies = task.dependencies.len();
        let mut completed_dependencies = 0;
        let mut pending_dependencies = 0;
        let mut blocked_dependencies = 0;
        let mut blocking_tasks = Vec::new();
        let mut ready_tasks = Vec::new();

        for dep_id in &task.dependencies {
            if let Some(dep_task) = all_tasks.get(dep_id) {
                match dep_task.status {
                    TaskDependencyStatus::Completed => {
                        completed_dependencies += 1;
                    }
                    TaskDependencyStatus::Pending | TaskDependencyStatus::Ready => {
                        pending_dependencies += 1;
                        ready_tasks.push(dep_id.clone());
                    }
                    TaskDependencyStatus::Blocked | TaskDependencyStatus::Failed => {
                        blocked_dependencies += 1;
                        blocking_tasks.push(dep_id.clone());
                    }
                    _ => {
                        pending_dependencies += 1;
                    }
                }
            }
        }

        let can_start = total_dependencies == 0 || completed_dependencies == total_dependencies;
        let critical_path = self.calculate_critical_path(task_id.clone(), all_tasks)?;

        Ok(DependencyAnalysis {
            task_id: task_id.clone(),
            total_dependencies,
            completed_dependencies,
            pending_dependencies,
            blocked_dependencies,
            can_start,
            blocking_tasks,
            ready_tasks,
            critical_path,
        })
    }

    pub fn build_dependency_graph(&self, tasks: Vec<TaskNode>) -> Result<DependencyGraph> {
        let mut task_map: HashMap<String, TaskNode> = HashMap::new();
        let mut edges: Vec<DependencyEdge> = Vec::new();

        for mut task in tasks {
            task.status = TaskDependencyStatus::Ready;
            task_map.insert(task.task_id.clone(), task.clone());
        }

        for task in task_map.values() {
            for dep_id in &task.dependencies {
                if let Some(dep_task) = task_map.get(dep_id) {
                    edges.push(DependencyEdge {
                        from_task: dep_id.clone(),
                        to_task: task.task_id.clone(),
                        dependency_type: DependencyType::Sequential,
                        condition: None,
                    });
                }
            }
        }

        Ok(DependencyGraph {
            tasks: task_map,
            edges,
        })
    }

    pub fn get_ready_tasks(&self, graph: &DependencyGraph) -> Vec<String> {
        let mut ready_tasks = Vec::new();

        for (task_id, task) in &graph.tasks {
            if task.status == TaskDependencyStatus::Ready {
                let all_deps_completed = task.dependencies.iter().all(|dep_id| {
                    graph.tasks.get(dep_id)
                        .map(|t| t.status == TaskDependencyStatus::Completed)
                        .unwrap_or(false)
                });

                if all_deps_completed {
                    ready_tasks.push(task_id.clone());
                }
            }
        }

        ready_tasks.sort_by(|a, b| {
            graph.tasks.get(a)
                .and_then(|t| Some(t.priority))
                .cmp(&graph.tasks.get(b).and_then(|t| Some(t.priority)))
                .reverse()
        });

        ready_tasks
    }

    pub fn update_task_status(
        &self,
        task_id: String,
        status: TaskDependencyStatus,
        reason: Option<String>,
    ) -> Result<()> {
        tracing::info!(
            "[TaskDependency] Updated task {} status to {:?}: {:?}",
            task_id,
            status,
            reason
        );

        Ok(())
    }

    pub fn request_coordination(
        &self,
        from_task: String,
        to_task: String,
        coordination_type: CoordinationType,
        message: String,
        data: serde_json::Value,
        lang: String,
    ) -> Result<String> {
        let request_id = Uuid::new_v4().to_string();
        let now = now_unix();

        let request = TaskCoordinationRequest {
            request_id: request_id.clone(),
            from_task: from_task.clone(),
            to_task: to_task.clone(),
            coordination_type: coordination_type.clone(),
            message: message.clone(),
            data: data.clone(),
            lang: lang.clone(),
            created_at: now,
        };

        let message_type = match coordination_type {
            CoordinationType::RequestData => ChatMessageType::Info,
            CoordinationType::ProvideData => ChatMessageType::Info,
            CoordinationType::RequestResource => ChatMessageType::Info,
            CoordinationType::ReleaseResource => ChatMessageType::Info,
            CoordinationType::Synchronize => ChatMessageType::Info,
            CoordinationType::NotifyCompletion => ChatMessageType::TaskCompletion,
            CoordinationType::RequestApproval => ChatMessageType::ConsensusRequest,
            CoordinationType::GrantApproval => ChatMessageType::ConsensusResponse,
            CoordinationType::RejectApproval => ChatMessageType::Disagreement,
            CoordinationType::Handover => ChatMessageType::Info,
        };

        let chat_message = format!(
            "[{}] {} -> {}: {}",
            coordination_type,
            from_task,
            to_task,
            message
        );

        self.chat_manager.send_message(
            None,
            Some(to_task.clone()),
            from_task.clone(),
            "subagent".to_string(),
            message_type,
            chat_message,
            lang,
            None,
            serde_json::to_value(&request)?,
        )?;

        tracing::info!(
            "[TaskCoordination] Requested coordination from {} to {}: {}",
            from_task,
            to_task,
            coordination_type
        );

        Ok(request_id)
    }

    pub fn respond_coordination(
        &self,
        request_id: String,
        response_type: CoordinationResponseType,
        message: String,
        data: serde_json::Value,
        responder: String,
        lang: String,
    ) -> Result<String> {
        let now = now_unix();
        let response = TaskCoordinationResponse {
            request_id: request_id.clone(),
            response_type: response_type.clone(),
            message: message.clone(),
            data: data.clone(),
            responder: responder.clone(),
            responded_at: now,
        };

        let message_type = match response_type {
            CoordinationResponseType::Approved => ChatMessageType::ConsensusResponse,
            CoordinationResponseType::Rejected => ChatMessageType::Disagreement,
            CoordinationResponseType::Deferred => ChatMessageType::Info,
            CoordinationResponseType::Completed => ChatMessageType::TaskCompletion,
            CoordinationResponseType::Failed => ChatMessageType::TaskFailure,
            CoordinationResponseType::Acknowledged => ChatMessageType::Info,
        };

        let chat_message = format!(
            "[{}] Response from {}: {}",
            format!("{:?}", response_type),
            responder,
            message
        );

        self.chat_manager.send_message(
            None,
            None,
            responder.clone(),
            "subagent".to_string(),
            message_type,
            chat_message,
            lang,
            None,
            serde_json::to_value(&response)?,
        )?;

        tracing::info!(
            "[TaskCoordination] {} responded to request {}: {:?}",
            responder,
            request_id,
            response_type
        );

        Ok(request_id)
    }

    pub fn initiate_dependency_consensus(
        &self,
        task_id: String,
        participants: Vec<String>,
        topic: String,
        description: String,
        lang: String,
    ) -> Result<String> {
        let proposal = ConsensusProposal {
            task_id: task_id.clone(),
            topic: topic.clone(),
            description: description.clone(),
            proposed_by: "orchestrator".to_string(),
            participants: participants.clone(),
            timeout_seconds: 300,
        };

        let message_id = self.consensus_manager.initiate_consensus(proposal, &lang)?;

        tracing::info!(
            "[TaskDependency] Initiated consensus for task dependencies: {} with participants {:?}",
            task_id,
            participants
        );

        Ok(message_id)
    }

    pub fn notify_task_completion(
        &self,
        task_id: String,
        result: String,
        output_data: Option<serde_json::Value>,
        lang: String,
    ) -> Result<()> {
        let message = if lang == "zh" {
            format!("任务 {} 已完成: {}", task_id, result)
        } else {
            format!("Task {} completed: {}", task_id, result)
        };

        self.chat_manager.send_task_completion(
            None,
            task_id.clone(),
            "subagent".to_string(),
            "worker".to_string(),
            result.clone(),
            lang.clone(),
        )?;

        if let Some(data) = output_data {
            self.request_coordination(
                task_id.clone(),
                "orchestrator".to_string(),
                CoordinationType::NotifyCompletion,
                message.clone(),
                data,
                lang.clone(),
            )?;
        }

        tracing::info!(
            "[TaskDependency] Task {} completed and notified: {}",
            task_id,
            result
        );

        Ok(())
    }

    pub fn request_data_from_predecessor(
        &self,
        current_task: String,
        predecessor_task: String,
        required_data: Vec<String>,
        lang: String,
    ) -> Result<String> {
        let message = if lang == "zh" {
            format!("请求数据: {:?} (来自任务 {})", required_data, predecessor_task)
        } else {
            format!("Requesting data: {:?} (from task {})", required_data, predecessor_task)
        };

        let data = serde_json::json!({
            "required_data": required_data,
            "requester": current_task
        });

        self.request_coordination(
            current_task,
            predecessor_task,
            CoordinationType::RequestData,
            message,
            data,
            lang,
        )
    }

    pub fn provide_data_to_successor(
        &self,
        predecessor_task: String,
        successor_task: String,
        data: HashMap<String, serde_json::Value>,
        lang: String,
    ) -> Result<String> {
        let message = if lang == "zh" {
            format!("提供数据: {} 个数据项", data.len())
        } else {
            format!("Providing data: {} items", data.len())
        };

        let data_json = serde_json::to_value(data)?;

        self.request_coordination(
            predecessor_task,
            successor_task,
            CoordinationType::ProvideData,
            message,
            data_json,
            lang,
        )
    }

    pub fn synchronize_with_peers(
        &self,
        task_id: String,
        peer_tasks: Vec<String>,
        sync_point: String,
        lang: String,
    ) -> Result<Vec<String>> {
        let mut request_ids = Vec::new();

        for peer in peer_tasks {
            let message = if lang == "zh" {
                format!("同步点: {} (来自任务 {})", sync_point, task_id)
            } else {
                format!("Sync point: {} (from task {})", sync_point, task_id)
            };

            let data = serde_json::json!({
                "sync_point": sync_point,
                "initiator": task_id
            });

            let request_id = self.request_coordination(
                task_id.clone(),
                peer,
                CoordinationType::Synchronize,
                message,
                data,
                lang.clone(),
            )?;

            request_ids.push(request_id);
        }

        tracing::info!(
            "[TaskDependency] Task {} initiated synchronization with {} peers at point {}",
            task_id,
            request_ids.len(),
            sync_point
        );

        Ok(request_ids)
    }

    fn calculate_critical_path(
        &self,
        task_id: String,
        tasks: &HashMap<String, TaskNode>,
    ) -> Result<Vec<String>> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        self.find_longest_path(task_id, tasks, &mut visited, &mut path);
        Ok(path)
    }

    fn find_longest_path(
        &self,
        task_id: String,
        tasks: &HashMap<String, TaskNode>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) {
        if visited.contains(&task_id) {
            return;
        }

        visited.insert(task_id.clone());
        path.push(task_id.clone());

        if let Some(task) = tasks.get(&task_id) {
            for dep_id in &task.dependencies {
                self.find_longest_path(dep_id.clone(), tasks, visited, path);
            }
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_define_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let manager = TaskDependencyManager::new(temp_dir.path());

        let dependency = manager
            .define_dependency(
                "task-2".to_string(),
                vec!["task-1".to_string()],
                DependencyType::Sequential,
                None,
                None,
                None,
            )
            .unwrap();

        assert_eq!(dependency.task_id, "task-2");
        assert_eq!(dependency.depends_on, vec!["task-1"]);
        assert_eq!(dependency.status, TaskDependencyStatus::Pending);
    }

    #[test]
    fn test_build_dependency_graph() {
        let temp_dir = TempDir::new().unwrap();
        let manager = TaskDependencyManager::new(temp_dir.path());

        let tasks = vec![
            TaskNode {
                task_id: "task-1".to_string(),
                task_name: "First Task".to_string(),
                assigned_to: Some("agent-1".to_string()),
                status: TaskDependencyStatus::Ready,
                dependencies: vec![],
                dependents: vec!["task-2".to_string()],
                estimated_duration: Some(60),
                priority: 1,
            },
            TaskNode {
                task_id: "task-2".to_string(),
                task_name: "Second Task".to_string(),
                assigned_to: Some("agent-2".to_string()),
                status: TaskDependencyStatus::Ready,
                dependencies: vec!["task-1".to_string()],
                dependents: vec![],
                estimated_duration: Some(30),
                priority: 2,
            },
        ];

        let graph = manager.build_dependency_graph(tasks).unwrap();
        assert_eq!(graph.tasks.len(), 2);
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn test_get_ready_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let manager = TaskDependencyManager::new(temp_dir.path());

        let mut tasks = vec![
            TaskNode {
                task_id: "task-1".to_string(),
                task_name: "First Task".to_string(),
                assigned_to: Some("agent-1".to_string()),
                status: TaskDependencyStatus::Ready,
                dependencies: vec![],
                dependents: vec!["task-2".to_string()],
                estimated_duration: Some(60),
                priority: 1,
            },
            TaskNode {
                task_id: "task-2".to_string(),
                task_name: "Second Task".to_string(),
                assigned_to: Some("agent-2".to_string()),
                status: TaskDependencyStatus::Ready,
                dependencies: vec!["task-1".to_string()],
                dependents: vec![],
                estimated_duration: Some(30),
                priority: 2,
            },
        ];

        let mut graph = manager.build_dependency_graph(tasks).unwrap();

        let ready = manager.get_ready_tasks(&graph);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "task-1");

        graph.tasks.get_mut("task-1").unwrap().status = TaskDependencyStatus::Completed;

        let ready = manager.get_ready_tasks(&graph);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "task-2");
    }

    #[test]
    fn test_request_coordination() {
        let temp_dir = TempDir::new().unwrap();
        let manager = TaskDependencyManager::new(temp_dir.path());

        let result = manager.request_coordination(
            "task-1".to_string(),
            "task-2".to_string(),
            CoordinationType::RequestData,
            "Please provide data".to_string(),
            serde_json::json!({ "data": ["item1", "item2"] }),
            "en".to_string(),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_analyze_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let manager = TaskDependencyManager::new(temp_dir.path());

        let mut tasks = HashMap::new();
        tasks.insert(
            "task-1".to_string(),
            TaskNode {
                task_id: "task-1".to_string(),
                task_name: "First Task".to_string(),
                assigned_to: Some("agent-1".to_string()),
                status: TaskDependencyStatus::Completed,
                dependencies: vec![],
                dependents: vec!["task-2".to_string()],
                estimated_duration: Some(60),
                priority: 1,
            },
        );
        tasks.insert(
            "task-2".to_string(),
            TaskNode {
                task_id: "task-2".to_string(),
                task_name: "Second Task".to_string(),
                assigned_to: Some("agent-2".to_string()),
                status: TaskDependencyStatus::Ready,
                dependencies: vec!["task-1".to_string()],
                dependents: vec![],
                estimated_duration: Some(30),
                priority: 2,
            },
        );

        let analysis = manager.analyze_dependencies("task-2".to_string(), &tasks).unwrap();
        assert_eq!(analysis.total_dependencies, 1);
        assert_eq!(analysis.completed_dependencies, 1);
        assert!(analysis.can_start);
    }
}
