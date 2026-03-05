use crate::swarm::store::SwarmSqliteStore;
use crate::swarm::TaskPriority;
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

pub struct AgentTaskManager {
    agent_name: String,
    personal_tasks: Arc<RwLock<HashMap<String, AgentTask>>>,
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    team_sync: Option<Arc<TeamTaskSynchronizer>>,
    task_store: Option<Arc<SwarmSqliteStore>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: AgentTaskStatus,
    pub priority: TaskPriority,
    pub parent_task_id: Option<String>,
    pub subtasks: Vec<String>,
    pub dependencies: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub estimated_completion: Option<u64>,
    pub completed_at: Option<u64>,
    pub source: TaskSource,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentTaskStatus {
    Pending,
    WaitingForDependencies,
    InProgress,
    Completed,
    Cancelled,
    Blocked { reason: String },
}

impl Default for AgentTaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskSource {
    TeamAssigned { from: String },
    SelfCreated,
    Decomposed { parent_id: String },
}

impl TaskSource {
    pub fn get_agent_name(&self) -> String {
        match self {
            Self::TeamAssigned { from } => from.clone(),
            Self::SelfCreated => "self".to_string(),
            Self::Decomposed { parent_id: _ } => "decomposer".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    nodes: HashMap<String, Vec<String>>,
    reverse_index: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_dependency(&mut self, task_id: &str, depends_on: &str) {
        self.nodes
            .entry(task_id.to_string())
            .or_default()
            .push(depends_on.to_string());
        
        self.reverse_index
            .entry(depends_on.to_string())
            .or_default()
            .push(task_id.to_string());
    }
    
    pub fn remove_dependency(&mut self, task_id: &str, depends_on: &str) {
        if let Some(deps) = self.nodes.get_mut(task_id) {
            deps.retain(|d| d != depends_on);
        }
        if let Some(rev) = self.reverse_index.get_mut(depends_on) {
            rev.retain(|r| r != task_id);
        }
    }
    
    pub fn get_dependencies(&self, task_id: &str) -> Vec<String> {
        self.nodes.get(task_id).cloned().unwrap_or_default()
    }
    
    pub fn get_dependents(&self, task_id: &str) -> Vec<String> {
        self.reverse_index.get(task_id).cloned().unwrap_or_default()
    }
    
    pub fn get_ready_tasks(&self, completed_tasks: &HashSet<String>) -> Vec<String> {
        let mut ready: Vec<String> = Vec::new();
        
        for task_id in self.reverse_index.keys() {
            if completed_tasks.contains(task_id) {
                continue;
            }
            if self.nodes.contains_key(task_id) {
                continue;
            }
            ready.push(task_id.clone());
        }
        
        for (task_id, deps) in &self.nodes {
            if completed_tasks.contains(task_id) {
                continue;
            }
            if deps.iter().all(|d| completed_tasks.contains(d)) {
                ready.push(task_id.clone());
            }
        }
        
        ready
    }
    
    pub fn detect_cycle(&self) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();
        
        for task_id in self.nodes.keys() {
            if let Some(cycle) = self.dfs_detect_cycle(task_id, &mut visited, &mut recursion_stack)
            {
                return Some(cycle);
            }
        }
        
        None
    }
    
    fn dfs_detect_cycle(
        &self,
        task_id: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
    ) -> Option<Vec<String>> {
        if recursion_stack.contains(task_id) {
            return Some(vec![task_id.to_string()]);
        }
        
        if visited.contains(task_id) {
            return None;
        }
        
        visited.insert(task_id.to_string());
        recursion_stack.insert(task_id.to_string());
        
        if let Some(deps) = self.nodes.get(task_id) {
            for dep in deps {
                if let Some(mut cycle) = self.dfs_detect_cycle(dep, visited, recursion_stack) {
                    cycle.push(task_id.to_string());
                    return Some(cycle);
                }
            }
        }
        
        recursion_stack.remove(task_id);
        None
    }
    
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.reverse_index.clear();
    }
}

pub struct TeamTaskSynchronizer {
    member_tasks: RwLock<HashMap<String, Vec<String>>>,
    task_updates: RwLock<Vec<TaskUpdate>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUpdate {
    pub task_id: String,
    pub agent_name: String,
    pub update_type: TaskUpdateType,
    pub timestamp: u64,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskUpdateType {
    Created,
    StatusChanged,
    ProgressUpdated,
    Completed,
    Cancelled,
    SubtaskCreated,
}

impl TeamTaskSynchronizer {
    pub fn new() -> Self {
        Self {
            member_tasks: RwLock::new(HashMap::new()),
            task_updates: RwLock::new(Vec::new()),
        }
    }
    
    pub async fn register_task(&self, agent_name: &str, task_id: &str) {
        let mut member_tasks = self.member_tasks.write().await;
        member_tasks
            .entry(agent_name.to_string())
            .or_default()
            .push(task_id.to_string());
    }
    
    pub async fn unregister_task(&self, agent_name: &str, task_id: &str) {
        let mut member_tasks = self.member_tasks.write().await;
        if let Some(tasks) = member_tasks.get_mut(agent_name) {
            tasks.retain(|t| t != task_id);
        }
    }
    
    pub async fn broadcast_update(&self, update: &TaskUpdate) {
        let mut updates = self.task_updates.write().await;
        updates.push(update.clone());
        
        let len = updates.len();
        if len > 1000 {
            updates.drain(0..len - 1000);
        }
    }
    
    pub async fn get_member_tasks(&self, agent_name: &str) -> Vec<String> {
        let member_tasks = self.member_tasks.read().await;
        member_tasks.get(agent_name).cloned().unwrap_or_default()
    }
    
    pub async fn get_team_view(&self) -> TeamTaskView {
        let member_tasks = self.member_tasks.read().await;
        
        let mut view = TeamTaskView {
            total_tasks: 0,
            by_member: HashMap::new(),
        };
        
        for (member, tasks) in member_tasks.iter() {
            view.by_member.insert(member.clone(), tasks.len());
            view.total_tasks += tasks.len();
        }
        
        view
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTaskView {
    pub total_tasks: usize,
    pub by_member: HashMap<String, usize>,
}

impl AgentTaskManager {
    pub fn new(agent_name: &str) -> Self {
        Self {
            agent_name: agent_name.to_string(),
            personal_tasks: Arc::new(RwLock::new(HashMap::new())),
            dependency_graph: Arc::new(RwLock::new(DependencyGraph::new())),
            team_sync: None,
            task_store: None,
        }
    }
    
    pub fn with_team_sync(mut self, sync: Arc<TeamTaskSynchronizer>) -> Self {
        self.team_sync = Some(sync);
        self
    }
    
    pub fn with_task_store(mut self, store: Arc<SwarmSqliteStore>) -> Self {
        self.task_store = Some(store);
        self
    }
    
    pub async fn create_task(
        &self,
        title: &str,
        description: &str,
        priority: TaskPriority,
        dependencies: Vec<String>,
        source: TaskSource,
    ) -> anyhow::Result<AgentTask> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_unix();
        
        let status = if dependencies.is_empty() {
            AgentTaskStatus::Pending
        } else {
            AgentTaskStatus::WaitingForDependencies
        };
        
        let task = AgentTask {
            id: id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            status,
            priority: priority.clone(),
            parent_task_id: None,
            subtasks: Vec::new(),
            dependencies: dependencies.clone(),
            created_at: now,
            updated_at: now,
            estimated_completion: None,
            completed_at: None,
            source,
            metadata: serde_json::json!({}),
        };
        
        {
            let mut graph = self.dependency_graph.write().await;
            for dep in &dependencies {
                graph.add_dependency(&id, dep);
            }
        }
        
        {
            let mut tasks = self.personal_tasks.write().await;
            tasks.insert(id.clone(), task.clone());
        }
        
        if let Some(ref sync) = self.team_sync {
            sync.register_task(&self.agent_name, &id).await;
            let priority_val = format!("{:?}", priority);
            sync.broadcast_update(&TaskUpdate {
                task_id: id.clone(),
                agent_name: self.agent_name.clone(),
                update_type: TaskUpdateType::Created,
                timestamp: now,
                details: serde_json::json!({
                    "title": title,
                    "priority": priority_val,
                }),
            }).await;
        }
        
        Ok(task)
    }
    
    pub async fn get_task(&self, task_id: &str) -> Option<AgentTask> {
        let tasks = self.personal_tasks.read().await;
        tasks.get(task_id).cloned()
    }
    
    pub async fn list_tasks(&self) -> Vec<AgentTask> {
        let tasks = self.personal_tasks.read().await;
        tasks.values().cloned().collect()
    }
    
    pub async fn list_tasks_by_status(&self, status: &AgentTaskStatus) -> Vec<AgentTask> {
        let tasks = self.personal_tasks.read().await;
        tasks.values().filter(|t| t.status == *status).cloned().collect()
    }
    
    pub async fn update_task_status(
        &self,
        task_id: &str,
        new_status: AgentTaskStatus,
    ) -> anyhow::Result<()> {
        let should_activate = {
            let mut tasks = self.personal_tasks.write().await;
            
            if let Some(task) = tasks.get_mut(task_id) {
                let old_status = task.status.clone();
                task.status = new_status.clone();
                task.updated_at = now_unix();
                
                if new_status == AgentTaskStatus::Completed {
                    task.completed_at = Some(now_unix());
                }
                
                let updated_at = task.updated_at;
                if let Some(ref sync) = self.team_sync {
                    sync.broadcast_update(&TaskUpdate {
                        task_id: task_id.to_string(),
                        agent_name: self.agent_name.clone(),
                        update_type: TaskUpdateType::StatusChanged,
                        timestamp: updated_at,
                        details: serde_json::json!({
                            "old_status": old_status,
                            "new_status": new_status,
                        }),
                    }).await;
                }
                
                new_status == AgentTaskStatus::Completed
            } else {
                false
            }
        };
        
        if should_activate {
            self.check_and_activate_dependent_tasks(task_id).await;
        }
        
        Ok(())
    }
    
    async fn check_and_activate_dependent_tasks(&self, completed_task_id: &str) {
        let graph = self.dependency_graph.read().await;
        let dependent_ids = graph.get_dependents(completed_task_id);
        drop(graph);
        
        let mut tasks = self.personal_tasks.write().await;
        
        let mut to_update: Vec<(String, bool)> = Vec::new();
        
        for dep_id in &dependent_ids {
            if let Some(task) = tasks.get(dep_id) {
                if task.status == AgentTaskStatus::WaitingForDependencies {
                    let all_deps_completed = task.dependencies.iter().all(|d| {
                        tasks.get(d).map(|t| t.status == AgentTaskStatus::Completed).unwrap_or(false)
                    });
                    to_update.push((dep_id.clone(), all_deps_completed));
                }
            }
        }
        
        for (task_id, all_deps_completed) in to_update {
            if all_deps_completed {
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.status = AgentTaskStatus::Pending;
                    task.updated_at = now_unix();
                }
            }
        }
    }
    
    pub async fn get_next_executable_task(&self) -> Option<AgentTask> {
        let tasks = self.personal_tasks.read().await;
        
        let completed: HashSet<String> = tasks
            .values()
            .filter(|t| t.status == AgentTaskStatus::Completed)
            .map(|t| t.id.clone())
            .collect();
        
        let graph = self.dependency_graph.read().await;
        let ready_ids = graph.get_ready_tasks(&completed);
        drop(graph);
        
        let mut ready_tasks: Vec<&AgentTask> = tasks
            .values()
            .filter(|t| {
                if t.status != AgentTaskStatus::Pending {
                    return false;
                }
                if t.dependencies.is_empty() {
                    return true;
                }
                ready_ids.contains(&t.id)
            })
            .collect();
        
        ready_tasks.sort_by(|a, b| {
            let priority_order = |p: &TaskPriority| match p {
                TaskPriority::Urgent => 0,
                TaskPriority::Critical => 1,
                TaskPriority::High => 2,
                TaskPriority::Medium => 3,
                TaskPriority::Low => 4,
            };
            priority_order(&a.priority).cmp(&priority_order(&b.priority))
        });
        
        ready_tasks.first().cloned().cloned()
    }
    
    pub async fn create_subtask(
        &self,
        parent_task_id: &str,
        title: &str,
        description: &str,
        priority: TaskPriority,
        dependencies: Vec<String>,
    ) -> anyhow::Result<AgentTask> {
        let _parent = self.get_task(parent_task_id).await
            .ok_or_else(|| anyhow::anyhow!("Parent task not found: {}", parent_task_id))?;
        
        let subtask = self.create_task(
            title,
            description,
            priority,
            dependencies,
            TaskSource::Decomposed {
                parent_id: parent_task_id.to_string(),
            },
        ).await?;
        
        {
            let mut tasks = self.personal_tasks.write().await;
            if let Some(parent) = tasks.get_mut(parent_task_id) {
                parent.subtasks.push(subtask.id.clone());
                parent.updated_at = now_unix();
            }
        }
        
        if let Some(ref sync) = self.team_sync {
            sync.broadcast_update(&TaskUpdate {
                task_id: subtask.id.clone(),
                agent_name: self.agent_name.clone(),
                update_type: TaskUpdateType::SubtaskCreated,
                timestamp: subtask.created_at,
                details: serde_json::json!({
                    "parent_task_id": parent_task_id,
                }),
            }).await;
        }
        
        Ok(subtask)
    }
    
    pub async fn cancel_task(&self, task_id: &str, reason: &str) -> anyhow::Result<()> {
        let mut tasks = self.personal_tasks.write().await;
        
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = AgentTaskStatus::Cancelled;
            task.updated_at = now_unix();
            task.metadata["cancellation_reason"] = serde_json::json!(reason);
            
            let updated_at = task.updated_at;
            if let Some(ref sync) = self.team_sync {
                sync.broadcast_update(&TaskUpdate {
                    task_id: task_id.to_string(),
                    agent_name: self.agent_name.clone(),
                    update_type: TaskUpdateType::Cancelled,
                    timestamp: updated_at,
                    details: serde_json::json!({
                        "reason": reason,
                    }),
                }).await;
            }
        }
        
        Ok(())
    }
    
    pub async fn block_task(&self, task_id: &str, reason: &str) -> anyhow::Result<()> {
        self.update_task_status(
            task_id,
            AgentTaskStatus::Blocked {
                reason: reason.to_string(),
            },
        ).await
    }
    
    pub async fn unblock_task(&self, task_id: &str) -> anyhow::Result<()> {
        self.update_task_status(task_id, AgentTaskStatus::Pending).await
    }
    
    pub async fn get_statistics(&self) -> TaskStatistics {
        let tasks = self.personal_tasks.read().await;
        
        let mut stats = TaskStatistics {
            total: tasks.len(),
            pending: 0,
            in_progress: 0,
            completed: 0,
            blocked: 0,
            cancelled: 0,
            by_priority: HashMap::new(),
        };
        
        for task in tasks.values() {
            match task.status {
                AgentTaskStatus::Pending => stats.pending += 1,
                AgentTaskStatus::WaitingForDependencies => stats.pending += 1,
                AgentTaskStatus::InProgress => stats.in_progress += 1,
                AgentTaskStatus::Completed => stats.completed += 1,
                AgentTaskStatus::Cancelled => stats.cancelled += 1,
                AgentTaskStatus::Blocked { .. } => stats.blocked += 1,
            }
            
            let priority_key = format!("{:?}", task.priority);
            *stats.by_priority.entry(priority_key).or_insert(0) += 1;
        }
        
        stats
    }
    
    pub async fn check_for_cycles(&self) -> Option<Vec<String>> {
        let graph = self.dependency_graph.read().await;
        graph.detect_cycle()
    }
    
    pub async fn clear_completed_tasks(&self) -> usize {
        let mut tasks = self.personal_tasks.write().await;
        let initial_len = tasks.len();
        
        tasks.retain(|_, t| t.status != AgentTaskStatus::Completed);
        
        initial_len - tasks.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatistics {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub blocked: usize,
    pub cancelled: usize,
    pub by_priority: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_create_task() {
        let manager = AgentTaskManager::new("test_agent");
        
        let task = manager.create_task(
            "测试任务",
            "这是一个测试任务",
            TaskPriority::Medium,
            vec![],
            TaskSource::SelfCreated,
        ).await.unwrap();
        
        assert_eq!(task.title, "测试任务");
        assert_eq!(task.status, AgentTaskStatus::Pending);
        
        let tasks = manager.list_tasks().await;
        assert_eq!(tasks.len(), 1);
    }
    
    #[tokio::test]
    async fn test_task_status_transitions() {
        let manager = AgentTaskManager::new("test_agent");
        
        let task = manager.create_task(
            "测试任务",
            "描述",
            TaskPriority::High,
            vec![],
            TaskSource::SelfCreated,
        ).await.unwrap();
        
        manager.update_task_status(&task.id, AgentTaskStatus::InProgress).await.unwrap();
        let updated = manager.get_task(&task.id).await.unwrap();
        assert_eq!(updated.status, AgentTaskStatus::InProgress);
        
        manager.update_task_status(&task.id, AgentTaskStatus::Completed).await.unwrap();
        let completed = manager.get_task(&task.id).await.unwrap();
        assert_eq!(completed.status, AgentTaskStatus::Completed);
        assert!(completed.completed_at.is_some());
    }
    
    #[tokio::test]
    async fn test_task_dependencies() {
        let manager = AgentTaskManager::new("test_agent");
        
        let task1 = manager.create_task(
            "任务1",
            "描述",
            TaskPriority::Medium,
            vec![],
            TaskSource::SelfCreated,
        ).await.unwrap();
        
        let task2 = manager.create_task(
            "任务2",
            "描述",
            TaskPriority::Medium,
            vec![task1.id.clone()],
            TaskSource::SelfCreated,
        ).await.unwrap();
        
        assert_eq!(task2.status, AgentTaskStatus::WaitingForDependencies);
        
        manager.update_task_status(&task1.id, AgentTaskStatus::InProgress).await.unwrap();
        manager.update_task_status(&task1.id, AgentTaskStatus::Completed).await.unwrap();
        
        let updated_task2 = manager.get_task(&task2.id).await.unwrap();
        assert_eq!(updated_task2.status, AgentTaskStatus::Pending);
    }
    
    #[tokio::test]
    async fn test_get_next_executable_task() {
        let manager = AgentTaskManager::new("test_agent");
        
        let low_task = manager.create_task(
            "低优先级任务",
            "描述",
            TaskPriority::Low,
            vec![],
            TaskSource::SelfCreated,
        ).await.unwrap();
        
        let high_task = manager.create_task(
            "高优先级任务",
            "描述",
            TaskPriority::High,
            vec![],
            TaskSource::SelfCreated,
        ).await.unwrap();
        
        let next = manager.get_next_executable_task().await.unwrap();
        assert_eq!(next.id, high_task.id);
        
        manager.update_task_status(&high_task.id, AgentTaskStatus::InProgress).await.unwrap();
        
        let next = manager.get_next_executable_task().await.unwrap();
        assert_eq!(next.id, low_task.id);
    }
    
    #[tokio::test]
    async fn test_create_subtask() {
        let manager = AgentTaskManager::new("test_agent");
        
        let parent = manager.create_task(
            "父任务",
            "描述",
            TaskPriority::Medium,
            vec![],
            TaskSource::SelfCreated,
        ).await.unwrap();
        
        let subtask = manager.create_subtask(
            &parent.id,
            "子任务",
            "子任务描述",
            TaskPriority::Medium,
            vec![],
        ).await.unwrap();
        
        assert!(subtask.parent_task_id.is_none());
        
        let updated_parent = manager.get_task(&parent.id).await.unwrap();
        assert!(updated_parent.subtasks.contains(&subtask.id));
    }
    
    #[tokio::test]
    async fn test_cycle_detection() {
        let mut graph = DependencyGraph::new();
        
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "c");
        graph.add_dependency("c", "a");
        
        let cycle = graph.detect_cycle();
        assert!(cycle.is_some());
    }
    
    #[tokio::test]
    async fn test_team_synchronizer() {
        let sync = Arc::new(TeamTaskSynchronizer::new());
        
        sync.register_task("agent1", "task1").await;
        sync.register_task("agent1", "task2").await;
        sync.register_task("agent2", "task3").await;
        
        let view = sync.get_team_view().await;
        assert_eq!(view.total_tasks, 3);
        assert_eq!(view.by_member.get("agent1"), Some(&2));
        assert_eq!(view.by_member.get("agent2"), Some(&1));
    }
    
    #[tokio::test]
    async fn test_task_statistics() {
        let manager = AgentTaskManager::new("test_agent");
        
        manager.create_task("任务1", "描述", TaskPriority::High, vec![], TaskSource::SelfCreated).await.unwrap();
        manager.create_task("任务2", "描述", TaskPriority::Medium, vec![], TaskSource::SelfCreated).await.unwrap();
        manager.create_task("任务3", "描述", TaskPriority::Low, vec![], TaskSource::SelfCreated).await.unwrap();
        
        let stats = manager.get_statistics().await;
        assert_eq!(stats.total, 3);
        assert_eq!(stats.pending, 3);
    }
}
