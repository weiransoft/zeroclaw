use crate::swarm::engine::{LLMProvider, Workflow, WorkflowStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn simple_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub struct DynamicPlanningEngine {
    llm_client: Option<Arc<dyn LLMProvider>>,
    workflow_store: Arc<WorkflowStore>,
    knowledge_base: Arc<KnowledgeBase>,
    experience_store: Arc<ExperienceStore>,
    planning_cache: RwLock<HashMap<String, CachedPlan>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConstraints {
    pub available_roles: Vec<String>,
    pub max_parallelism: usize,
    pub priority: String,
    pub deadline: Option<u64>,
    pub budget_hours: Option<f64>,
}

impl Default for WorkflowConstraints {
    fn default() -> Self {
        Self {
            available_roles: vec![
                "product_owner".to_string(),
                "architect".to_string(),
                "frontend_developer".to_string(),
                "backend_developer".to_string(),
                "qa_engineer".to_string(),
            ],
            max_parallelism: 3,
            priority: "medium".to_string(),
            deadline: None,
            budget_hours: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedWorkflow {
    pub name: String,
    pub description: String,
    pub phases: Vec<GeneratedPhase>,
    pub risk_assessment: Vec<RiskAssessment>,
    pub estimated_total_hours: f64,
    pub recommended_team: Vec<TeamRoleAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedPhase {
    pub name: String,
    pub description: String,
    pub assigned_to: Vec<String>,
    pub dependencies: Vec<String>,
    pub completion_criteria: PhaseCompletionCriteriaJson,
    pub estimated_duration_hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseCompletionCriteriaJson {
    pub required_tasks: Vec<String>,
    pub required_documents: Vec<String>,
    pub quality_metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub risk: String,
    pub probability: String,
    pub mitigation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRoleAssignment {
    pub role: String,
    pub responsibilities: Vec<String>,
    pub estimated_hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowAdjustment {
    pub modifications: Vec<PhaseModification>,
    pub reassignments: Vec<TaskReassignment>,
    pub new_risks: Vec<RiskAssessment>,
    pub updated_estimate_hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseModification {
    pub phase: String,
    pub action: String,
    pub changes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReassignment {
    pub task: String,
    pub from: String,
    pub to: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionPrediction {
    pub estimated_completion_time: u64,
    pub confidence_level: String,
    pub key_factors: Vec<String>,
    pub risks: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDecomposition {
    pub subtasks: Vec<SubtaskDefinition>,
    pub estimated_total_hours: f64,
    pub parallel_execution_possible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskDefinition {
    pub title: String,
    pub description: String,
    pub priority: String,
    pub dependencies: Vec<String>,
    pub estimated_hours: f64,
    pub skills_required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPlan {
    pub task_hash: String,
    pub workflow: GeneratedWorkflow,
    pub created_at: u64,
    pub usage_count: u32,
}

impl CachedPlan {
    const CACHE_TTL_SECONDS: u64 = 3600;
    
    pub fn is_expired(&self) -> bool {
        let now = now_unix();
        now.saturating_sub(self.created_at) > Self::CACHE_TTL_SECONDS
    }
}

pub struct KnowledgeBase {
    entries: RwLock<Vec<KnowledgeEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub category: String,
    pub tags: Vec<String>,
    pub created_at: u64,
}

impl KnowledgeBase {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }
    
    pub async fn add_entry(&self, entry: KnowledgeEntry) {
        let mut entries = self.entries.write().await;
        entries.push(entry);
    }
    
    pub async fn search(&self, query: &str, limit: usize) -> Vec<KnowledgeEntry> {
        let entries = self.entries.read().await;
        let query_lower = query.to_lowercase();
        
        let results: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.title.to_lowercase().contains(&query_lower)
                    || e.summary.to_lowercase().contains(&query_lower)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .take(limit)
            .collect();
        
        results
    }
}

pub struct ExperienceStore {
    experiences: RwLock<Vec<Experience>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub id: String,
    pub title: String,
    pub description: String,
    pub content: String,
    pub tags: Vec<String>,
    pub rating: f64,
    pub created_at: u64,
}

impl ExperienceStore {
    pub fn new() -> Self {
        Self {
            experiences: RwLock::new(Vec::new()),
        }
    }
    
    pub async fn add_experience(&self, experience: Experience) {
        let mut experiences = self.experiences.write().await;
        experiences.push(experience);
    }
    
    pub async fn search(&self, query: &str, limit: usize) -> Vec<Experience> {
        let experiences = self.experiences.read().await;
        let query_lower = query.to_lowercase();
        
        experiences
            .iter()
            .filter(|e| {
                e.title.to_lowercase().contains(&query_lower)
                    || e.description.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .take(limit)
            .collect()
    }
}

impl DynamicPlanningEngine {
    pub fn new(
        workflow_store: Arc<WorkflowStore>,
        knowledge_base: Arc<KnowledgeBase>,
        experience_store: Arc<ExperienceStore>,
    ) -> Self {
        Self {
            llm_client: None,
            workflow_store,
            knowledge_base,
            experience_store,
            planning_cache: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn with_llm_client(mut self, client: Arc<dyn LLMProvider>) -> Self {
        self.llm_client = Some(client);
        self
    }
    
    pub async fn generate_workflow(
        &self,
        task_description: &str,
        constraints: &WorkflowConstraints,
    ) -> anyhow::Result<GeneratedWorkflow> {
        let task_hash = simple_hash(task_description);
        
        if let Some(cached) = self.get_cached_plan(&task_hash).await {
            return Ok(cached);
        }
        
        let knowledge = self.knowledge_base.search(task_description, 5).await;
        let experience = self.experience_store.search(task_description, 5).await;
        
        if let Some(ref client) = self.llm_client {
            let prompt = self.build_workflow_generation_prompt(
                task_description,
                constraints,
                &knowledge,
                &experience,
            );
            
            let response = client.complete(&prompt).await?;
            let workflow = self.parse_workflow_response(&response)?;
            
            self.cache_plan(&task_hash, &workflow).await;
            
            return Ok(workflow);
        }
        
        Ok(self.generate_default_workflow(task_description, constraints))
    }
    
    fn build_workflow_generation_prompt(
        &self,
        task: &str,
        constraints: &WorkflowConstraints,
        knowledge: &[KnowledgeEntry],
        experience: &[Experience],
    ) -> String {
        let knowledge_str = knowledge
            .iter()
            .map(|k| format!("- {}: {}", k.title, k.summary))
            .collect::<Vec<_>>()
            .join("\n");
        
        let experience_str = experience
            .iter()
            .map(|e| format!("- {}: {}", e.title, e.description))
            .collect::<Vec<_>>()
            .join("\n");
        
        format!(
            r#"你是一个专业的项目管理专家，需要为以下任务设计一个高效的工作流程。

## 任务描述
{}

## 约束条件
- 可用角色：{:?}
- 最大并行度：{}
- 优先级：{}
- 截止时间：{}
- 预算工时：{}

## 相关知识
{}

## 历史经验
{}

## 请设计工作流
请以 JSON 格式返回工作流设计，包含：
1. 工作流名称和描述
2. 阶段划分（每个阶段包含：名称、描述、负责人、依赖、完成条件）
3. 角色分配建议
4. 风险评估
5. 预计时间线

```json
{{
  "name": "工作流名称",
  "description": "工作流描述",
  "phases": [
    {{
      "name": "阶段名称",
      "description": "阶段描述",
      "assigned_to": ["角色列表"],
      "dependencies": ["依赖的阶段"],
      "completion_criteria": {{
        "required_tasks": ["必须完成的任务"],
        "required_documents": ["必须产出的文档"],
        "quality_metrics": {{"指标名": 目标值}}
      }},
      "estimated_duration_hours": 预计时长
    }}
  ],
  "risk_assessment": [
    {{
      "risk": "风险描述",
      "probability": "高/中/低",
      "mitigation": "缓解措施"
    }}
  ],
  "estimated_total_hours": 总预计工时,
  "recommended_team": [
    {{
      "role": "角色名",
      "responsibilities": ["职责列表"],
      "estimated_hours": 预计工时
    }}
  ]
}}
```"#,
            task,
            constraints.available_roles,
            constraints.max_parallelism,
            constraints.priority,
            constraints.deadline.map(|d| d.to_string()).unwrap_or_else(|| "无".to_string()),
            constraints.budget_hours.map(|h| h.to_string()).unwrap_or_else(|| "无".to_string()),
            if knowledge_str.is_empty() { "无" } else { &knowledge_str },
            if experience_str.is_empty() { "无" } else { &experience_str },
        )
    }
    
    fn parse_workflow_response(&self, response: &str) -> anyhow::Result<GeneratedWorkflow> {
        let json_start = response.find('{').unwrap_or(0);
        let json_end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
        let json_str = &response[json_start..json_end];
        
        let workflow: GeneratedWorkflow = serde_json::from_str(json_str)?;
        Ok(workflow)
    }
    
    fn generate_default_workflow(
        &self,
        task_description: &str,
        constraints: &WorkflowConstraints,
    ) -> GeneratedWorkflow {
        GeneratedWorkflow {
            name: format!("工作流: {}", task_description.chars().take(50).collect::<String>()),
            description: task_description.to_string(),
            phases: vec![
                GeneratedPhase {
                    name: "需求分析".to_string(),
                    description: "分析任务需求，确定功能范围".to_string(),
                    assigned_to: vec!["product_owner".to_string()],
                    dependencies: vec![],
                    completion_criteria: PhaseCompletionCriteriaJson {
                        required_tasks: vec!["需求文档".to_string()],
                        required_documents: vec!["PRD".to_string()],
                        quality_metrics: vec![("需求覆盖率".to_string(), 1.0)].into_iter().collect(),
                    },
                    estimated_duration_hours: 8.0,
                },
                GeneratedPhase {
                    name: "架构设计".to_string(),
                    description: "设计系统架构和技术方案".to_string(),
                    assigned_to: vec!["architect".to_string()],
                    dependencies: vec!["需求分析".to_string()],
                    completion_criteria: PhaseCompletionCriteriaJson {
                        required_tasks: vec!["架构设计".to_string()],
                        required_documents: vec!["架构文档".to_string()],
                        quality_metrics: vec![("设计评审通过率".to_string(), 1.0)].into_iter().collect(),
                    },
                    estimated_duration_hours: 8.0,
                },
                GeneratedPhase {
                    name: "开发实现".to_string(),
                    description: "实现功能代码".to_string(),
                    assigned_to: vec!["frontend_developer".to_string(), "backend_developer".to_string()],
                    dependencies: vec!["架构设计".to_string()],
                    completion_criteria: PhaseCompletionCriteriaJson {
                        required_tasks: vec!["代码实现".to_string(), "单元测试".to_string()],
                        required_documents: vec![],
                        quality_metrics: vec![("代码覆盖率".to_string(), 0.8)].into_iter().collect(),
                    },
                    estimated_duration_hours: 24.0,
                },
                GeneratedPhase {
                    name: "测试验证".to_string(),
                    description: "执行功能测试和集成测试".to_string(),
                    assigned_to: vec!["qa_engineer".to_string()],
                    dependencies: vec!["开发实现".to_string()],
                    completion_criteria: PhaseCompletionCriteriaJson {
                        required_tasks: vec!["功能测试".to_string(), "集成测试".to_string()],
                        required_documents: vec!["测试报告".to_string()],
                        quality_metrics: vec![("测试通过率".to_string(), 0.95)].into_iter().collect(),
                    },
                    estimated_duration_hours: 8.0,
                },
            ],
            risk_assessment: vec![
                RiskAssessment {
                    risk: "需求变更风险".to_string(),
                    probability: "中".to_string(),
                    mitigation: "建立需求变更流程，及时沟通".to_string(),
                },
            ],
            estimated_total_hours: 48.0,
            recommended_team: constraints
                .available_roles
                .iter()
                .map(|role| TeamRoleAssignment {
                    role: role.clone(),
                    responsibilities: vec![],
                    estimated_hours: 16.0,
                })
                .collect(),
        }
    }
    
    async fn get_cached_plan(&self, task_hash: &str) -> Option<GeneratedWorkflow> {
        let cache = self.planning_cache.read().await;
        if let Some(cached) = cache.get(task_hash) {
            if !cached.is_expired() {
                return Some(cached.workflow.clone());
            }
        }
        None
    }
    
    async fn cache_plan(&self, task_hash: &str, workflow: &GeneratedWorkflow) {
        let mut cache = self.planning_cache.write().await;
        
        if cache.len() > 100 {
            let now = now_unix();
            let expired_keys: Vec<String> = cache
                .iter()
                .filter(|(_, v)| v.is_expired() || now.saturating_sub(v.created_at) > 7200)
                .map(|(k, _)| k.clone())
                .collect();
            
            for key in expired_keys {
                cache.remove(&key);
            }
        }
        
        cache.insert(
            task_hash.to_string(),
            CachedPlan {
                task_hash: task_hash.to_string(),
                workflow: workflow.clone(),
                created_at: now_unix(),
                usage_count: 0,
            },
        );
    }
    
    pub async fn adjust_workflow(
        &self,
        workflow_id: &str,
        adjustment_reason: &str,
        context: &AdjustmentContext,
    ) -> anyhow::Result<WorkflowAdjustment> {
        let workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        if let Some(ref client) = self.llm_client {
            let prompt = self.build_adjustment_prompt(&workflow, adjustment_reason, context);
            let response = client.complete(&prompt).await?;
            return self.parse_adjustment_response(&response);
        }
        
        Ok(WorkflowAdjustment {
            modifications: vec![],
            reassignments: vec![],
            new_risks: vec![],
            updated_estimate_hours: workflow.phases.len() as f64 * 10.0,
        })
    }
    
    fn build_adjustment_prompt(
        &self,
        workflow: &Workflow,
        adjustment_reason: &str,
        context: &AdjustmentContext,
    ) -> String {
        format!(
            r#"工作流需要动态调整，请分析并给出调整方案。

## 当前工作流状态
{}

## 调整原因
{}

## 上下文信息
{}

## 请提供调整方案
以 JSON 格式返回，包含：
1. 需要修改的阶段
2. 需要添加或删除的任务
3. 角色重新分配建议
4. 新的风险评估
5. 调整后的时间预估

```json
{{
  "modifications": [
    {{
      "phase": "阶段名",
      "action": "add/modify/remove",
      "changes": {{}}
    }}
  ],
  "reassignments": [
    {{
      "task": "任务名",
      "from": "原负责人",
      "to": "新负责人",
      "reason": "原因"
    }}
  ],
  "new_risks": [],
  "updated_estimate_hours": 0
}}
```"#,
            serde_json::to_string_pretty(workflow).unwrap_or_default(),
            adjustment_reason,
            serde_json::to_string_pretty(context).unwrap_or_default(),
        )
    }
    
    fn parse_adjustment_response(&self, response: &str) -> anyhow::Result<WorkflowAdjustment> {
        let json_start = response.find('{').unwrap_or(0);
        let json_end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
        let json_str = &response[json_start..json_end];
        
        let adjustment: WorkflowAdjustment = serde_json::from_str(json_str)?;
        Ok(adjustment)
    }
    
    pub async fn predict_completion(
        &self,
        workflow_id: &str,
    ) -> anyhow::Result<CompletionPrediction> {
        let workflow = self.workflow_store.get_workflow(workflow_id).await
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;
        
        let now = now_unix();
        let elapsed = workflow.started_at.map(|s| now - s).unwrap_or(0);
        let progress = workflow.overall_progress();
        
        let estimated_remaining = if progress > 0.0 && elapsed > 0 {
            let total_estimated = (elapsed as f64 / progress) as u64;
            total_estimated.saturating_sub(elapsed)
        } else {
            86400
        };
        
        let estimated_completion = now + estimated_remaining;
        
        let confidence = if progress > 0.5 {
            "高"
        } else if progress > 0.2 {
            "中"
        } else {
            "低"
        };
        
        Ok(CompletionPrediction {
            estimated_completion_time: estimated_completion,
            confidence_level: confidence.to_string(),
            key_factors: vec![
                "当前进度".to_string(),
                "团队效率".to_string(),
                "任务复杂度".to_string(),
            ],
            risks: vec!["需求变更".to_string(), "技术风险".to_string()],
            recommendations: vec!["保持进度跟踪".to_string(), "及时沟通问题".to_string()],
        })
    }
    
    pub async fn decompose_task(
        &self,
        parent_task: &str,
        complexity: &TaskComplexity,
    ) -> anyhow::Result<TaskDecomposition> {
        if let Some(ref client) = self.llm_client {
            let prompt = self.build_decomposition_prompt(parent_task, complexity);
            let response = client.complete(&prompt).await?;
            return self.parse_decomposition_response(&response);
        }
        
        Ok(TaskDecomposition {
            subtasks: vec![
                SubtaskDefinition {
                    title: format!("{} - 分析", parent_task),
                    description: "分析任务需求".to_string(),
                    priority: "medium".to_string(),
                    dependencies: vec![],
                    estimated_hours: 2.0,
                    skills_required: vec!["分析能力".to_string()],
                },
                SubtaskDefinition {
                    title: format!("{} - 实现", parent_task),
                    description: "实现任务功能".to_string(),
                    priority: "medium".to_string(),
                    dependencies: vec![format!("{} - 分析", parent_task)],
                    estimated_hours: 4.0,
                    skills_required: vec!["开发能力".to_string()],
                },
                SubtaskDefinition {
                    title: format!("{} - 测试", parent_task),
                    description: "测试任务功能".to_string(),
                    priority: "medium".to_string(),
                    dependencies: vec![format!("{} - 实现", parent_task)],
                    estimated_hours: 2.0,
                    skills_required: vec!["测试能力".to_string()],
                },
            ],
            estimated_total_hours: 8.0,
            parallel_execution_possible: false,
        })
    }
    
    fn build_decomposition_prompt(&self, task: &str, complexity: &TaskComplexity) -> String {
        format!(
            r#"请将以下任务分解为可执行的子任务。

## 父任务
{}

## 复杂度评估
- 技术复杂度：{}
- 业务复杂度：{}
- 依赖复杂度：{}

## 请分解任务
返回 JSON 数组，每个子任务包含：
```json
[
  {{
    "title": "子任务标题",
    "description": "详细描述",
    "priority": "medium",
    "dependencies": ["依赖的其他子任务标题"],
    "estimated_hours": 2.0,
    "skills_required": ["技能1", "技能2"]
  }}
]
```"#,
            task,
            complexity.technical,
            complexity.business,
            complexity.dependencies,
        )
    }
    
    fn parse_decomposition_response(&self, response: &str) -> anyhow::Result<TaskDecomposition> {
        let json_start = response.find('[').unwrap_or(0);
        let json_end = response.rfind(']').map(|i| i + 1).unwrap_or(response.len());
        let json_str = &response[json_start..json_end];
        
        let subtasks: Vec<SubtaskDefinition> = serde_json::from_str(json_str)?;
        
        let total: f64 = subtasks.iter().map(|s| s.estimated_hours).sum();
        
        Ok(TaskDecomposition {
            subtasks,
            estimated_total_hours: total,
            parallel_execution_possible: true,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustmentContext {
    pub current_progress: f64,
    pub blockers: Vec<String>,
    pub team_availability: HashMap<String, bool>,
    pub recent_changes: Vec<String>,
}

impl Default for AdjustmentContext {
    fn default() -> Self {
        Self {
            current_progress: 0.0,
            blockers: vec![],
            team_availability: HashMap::new(),
            recent_changes: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComplexity {
    pub technical: f64,
    pub business: f64,
    pub dependencies: f64,
}

impl Default for TaskComplexity {
    fn default() -> Self {
        Self {
            technical: 0.5,
            business: 0.5,
            dependencies: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_generate_default_workflow() {
        let workflow_store = Arc::new(WorkflowStore::new());
        let knowledge_base = Arc::new(KnowledgeBase::new());
        let experience_store = Arc::new(ExperienceStore::new());
        
        let engine = DynamicPlanningEngine::new(
            workflow_store,
            knowledge_base,
            experience_store,
        );
        
        let constraints = WorkflowConstraints::default();
        let workflow = engine.generate_workflow("实现用户登录功能", &constraints).await.unwrap();
        
        assert!(!workflow.name.is_empty());
        assert!(!workflow.phases.is_empty());
        assert!(workflow.estimated_total_hours > 0.0);
    }
    
    #[tokio::test]
    async fn test_predict_completion() {
        let workflow_store = Arc::new(WorkflowStore::new());
        let knowledge_base = Arc::new(KnowledgeBase::new());
        let experience_store = Arc::new(ExperienceStore::new());
        
        let engine = DynamicPlanningEngine::new(
            workflow_store.clone(),
            knowledge_base,
            experience_store,
        );
        
        let mut workflow = crate::swarm::engine::Workflow::new("test-1", "测试工作流", "测试");
        workflow.status = crate::swarm::engine::WorkflowStatus::InProgress;
        workflow.started_at = Some(now_unix() - 3600);
        workflow.phases = vec!["phase-1".to_string(), "phase-2".to_string()];
        workflow_store.create_workflow(&workflow).await.unwrap();
        
        let prediction = engine.predict_completion("test-1").await.unwrap();
        
        assert!(prediction.estimated_completion_time > 0);
        assert!(!prediction.key_factors.is_empty());
    }
    
    #[tokio::test]
    async fn test_decompose_task() {
        let workflow_store = Arc::new(WorkflowStore::new());
        let knowledge_base = Arc::new(KnowledgeBase::new());
        let experience_store = Arc::new(ExperienceStore::new());
        
        let engine = DynamicPlanningEngine::new(
            workflow_store,
            knowledge_base,
            experience_store,
        );
        
        let complexity = TaskComplexity::default();
        let decomposition = engine.decompose_task("实现用户认证", &complexity).await.unwrap();
        
        assert!(!decomposition.subtasks.is_empty());
        assert!(decomposition.estimated_total_hours > 0.0);
    }
    
    #[tokio::test]
    async fn test_knowledge_base() {
        let kb = KnowledgeBase::new();
        
        kb.add_entry(KnowledgeEntry {
            id: "1".to_string(),
            title: "用户认证最佳实践".to_string(),
            summary: "关于用户认证的最佳实践指南".to_string(),
            content: "详细内容...".to_string(),
            category: "security".to_string(),
            tags: vec!["认证".to_string(), "安全".to_string()],
            created_at: 0,
        }).await;
        
        let results = kb.search("认证", 10).await;
        assert_eq!(results.len(), 1);
    }
    
    #[tokio::test]
    async fn test_experience_store() {
        let store = ExperienceStore::new();
        
        store.add_experience(Experience {
            id: "1".to_string(),
            title: "登录功能开发经验".to_string(),
            description: "成功实现登录功能的经验总结".to_string(),
            content: "详细经验...".to_string(),
            tags: vec!["登录".to_string()],
            rating: 4.5,
            created_at: 0,
        }).await;
        
        let results = store.search("登录", 10).await;
        assert_eq!(results.len(), 1);
    }
}
