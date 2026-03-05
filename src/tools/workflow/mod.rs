use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::traits::{Tool, ToolResult};
use crate::config::Config;
use crate::security::SecurityPolicy;
use crate::swarm::{SwarmContext, SwarmManager};
use crate::swarm::chat::{ChatMessageType, SwarmChatManager};
use crate::swarm::consensus::ConsensusManager;
use crate::providers::Provider;

// 工作流子模块
pub mod template;
pub mod store;
pub mod generator;

// 导入工作流模板系统
use self::template::WorkflowTemplate;
use self::store::WorkflowTemplateStore;
use self::generator::WorkflowTemplateGenerator;

fn emit_workflow_event(workflow: &Workflow) {
    let event = json!({
        "type": "workflow:update",
        "data": workflow
    });
    println!("{}", serde_json::to_string(&event).unwrap_or_default());
    let _ = stdout().flush();
}

// 工作流步骤结构
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowStep {
    pub name: String,
    pub description: String,
    pub assigned_to: String,
    pub dependencies: Vec<String>,
    pub status: String,
}

// 工作流结构
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub roles: Vec<String>,
    pub steps: Vec<WorkflowStep>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

// 工作流存储
struct WorkflowStore {
    workflows: Mutex<HashMap<String, Workflow>>,
    base_dir: PathBuf,
}

impl WorkflowStore {
    fn new(base_dir: PathBuf) -> Self {
        let store = Self {
            workflows: Mutex::new(HashMap::new()),
            base_dir,
        };
        store.ensure_directory();
        store.load_workflows();
        store
    }

    fn ensure_directory(&self) {
        if !self.base_dir.exists() {
            if let Err(e) = fs::create_dir_all(&self.base_dir) {
                tracing::error!("Failed to create workflow directory: {:?}", e);
                // 继续执行，后续操作会处理错误
            }
        }

        // Check directory permissions
        if let Ok(metadata) = self.base_dir.metadata() {
            if !metadata.is_dir() {
                tracing::error!("Workflow directory path exists but is not a directory: {:?}", self.base_dir);
            }
        } else {
            tracing::error!("Failed to get workflow directory metadata: {:?}", self.base_dir);
        }
    }

    fn load_workflows(&self) {
        let mut workflows = self.workflows.lock().unwrap();
        self.ensure_directory();

        match fs::read_dir(&self.base_dir) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(entry) = entry {
                        if entry.path().extension().unwrap_or_default() == "json" {
                            if let Ok(content) = fs::read_to_string(entry.path()) {
                                if let Ok(workflow) = serde_json::from_str::<Workflow>(&content) {
                                    workflows.insert(workflow.id.clone(), workflow);
                                } else {
                                    tracing::warn!("Failed to parse workflow file: {:?}", entry.path());
                                }
                            } else {
                                tracing::warn!("Failed to read workflow file: {:?}", entry.path());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to read workflow directory: {:?}", e);
            }
        }
    }

    fn save_workflow(&self, workflow: &Workflow) {
        self.ensure_directory();

        let file_path = self.base_dir.join(format!("{}.json", workflow.id));
        match File::create(&file_path) {
            Ok(mut file) => {
                if let Ok(json_str) = serde_json::to_string_pretty(workflow) {
                    if let Err(e) = write!(file, "{}", json_str) {
                        tracing::error!("Failed to write workflow file: {:?}", e);
                    }
                } else {
                    tracing::error!("Failed to serialize workflow: {:?}", workflow.id);
                }
            }
            Err(e) => {
                tracing::error!("Failed to create workflow file: {:?}", e);
            }
        }

        let mut workflows = self.workflows.lock().unwrap();
        workflows.insert(workflow.id.clone(), workflow.clone());
    }

    fn get_workflow(&self, workflow_id: &str) -> Option<Workflow> {
        let workflows = self.workflows.lock().unwrap();
        workflows.get(workflow_id).cloned()
    }

    fn list_workflows(&self) -> Vec<Workflow> {
        let workflows = self.workflows.lock().unwrap();
        workflows.values().cloned().collect()
    }
}

pub struct WorkflowTool {
    security: Arc<SecurityPolicy>,
    config: Arc<Config>,
    manager: Arc<SwarmManager>,
    ctx: SwarmContext,
    store: Arc<WorkflowStore>,
    chat_manager: SwarmChatManager,
    consensus_manager: ConsensusManager,
    template_store: Arc<WorkflowTemplateStore>,
    template_generator: WorkflowTemplateGenerator,
    provider: Option<Arc<dyn Provider>>,
}

impl WorkflowTool {
    pub fn new(
        security: Arc<SecurityPolicy>,
        config: Arc<Config>,
        manager: Arc<SwarmManager>,
        ctx: SwarmContext,
    ) -> Self {
        // WorkflowStore uses file-based storage in the workflow directory
        // Resolve workflow_dir relative to workspace_dir
        let workflow_dir = if config.workflow.workflow_dir.starts_with(".workspace") {
            let relative_path = config.workflow.workflow_dir.strip_prefix(".workspace").unwrap_or(&config.workflow.workflow_dir);
            let relative_path = relative_path.trim_start_matches('/');
            config.workspace_dir.join(relative_path)
        } else {
            std::path::PathBuf::from(&config.workflow.workflow_dir)
        };
        let store = Arc::new(WorkflowStore::new(workflow_dir));
        
        // 从配置文件加载模板存储目录（解析相对路径为绝对路径）
        let templates_dir = if config.workflow.templates_dir.starts_with(".workspace") {
            let relative_path = config.workflow.templates_dir.strip_prefix(".workspace").unwrap_or(&config.workflow.templates_dir);
            let relative_path = relative_path.trim_start_matches('/');
            config.workspace_dir.join(relative_path)
        } else {
            std::path::PathBuf::from(&config.workflow.templates_dir)
        };
        let template_dir = Path::new(&templates_dir);
        let template_store = Arc::new(WorkflowTemplateStore::new(template_dir.to_path_buf()));
        
        // 初始化模板生成器
        let template_generator = WorkflowTemplateGenerator::new(manager.clone());
        
        let chat_manager = SwarmChatManager::new(&config.workspace_dir);
        let consensus_manager = ConsensusManager::new(&config.workspace_dir);

        Self {
            security,
            config,
            manager,
            ctx,
            store,
            chat_manager,
            consensus_manager,
            template_store,
            template_generator,
            provider: None,
        }
    }

    pub fn with_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.provider = Some(provider);
        self
    }

    async fn analyze_workflow_with_llm(&self, description: &str) -> (Vec<String>, Vec<WorkflowStep>) {
        tracing::debug!("[Workflow] Starting LLM analysis for: {}", description);
        
        if let Some(provider) = &self.provider {
            tracing::debug!("[Workflow] Provider available, calling LLM...");
            
            let system_prompt = "你是一个工作流分析专家，只返回 JSON 格式的结果，不要有任何其他文字。";
            let prompt = format!(
                r#"请分析以下任务描述，并返回 JSON 格式的工作流配置。

任务描述：{}

请返回以下 JSON 格式（不要包含```json 标记）：
{{
  "roles": ["角色 1", "角色 2", ...],
  "steps": [
    {{
      "name": "步骤名称",
      "description": "步骤描述",
      "assigned_to": "负责角色",
      "dependencies": []
    }}
  ]
}}

要求：
1. 根据任务需求智能识别需要的角色（如产品经理、运营、开发、设计师等）
2. 生成合理的工作流步骤，每个步骤需要明确负责角色
3. 步骤之间可以有依赖关系
4. 返回纯 JSON，不要有任何其他文字"#,
                description
            );

            let model = self.config.default_model.as_deref().unwrap_or("glm-5");
            let temperature = 0.3;

            tracing::debug!("[Workflow] Calling LLM with model: {}", model);
            
            // Add timeout for LLM call
            let llm_future = provider.chat_with_system(Some(system_prompt), &prompt, model, temperature);
            
            tracing::debug!("[Workflow] LLM future created, waiting for response (timeout: 60s)...");
            
            match tokio::time::timeout(std::time::Duration::from_secs(60), llm_future).await {
                Ok(Ok(response)) => {
                    tracing::debug!("[Workflow] LLM response received");
                    let text = response.text_or_empty();
                    tracing::trace!("[Workflow] LLM response text: {}", text);
                    let cleaned = text.trim()
                        .trim_start_matches("```json")
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim();
                    
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(cleaned) {
                        let roles: Vec<String> = parsed.get("roles")
                            .and_then(|r| r.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                            .unwrap_or_default();

                        let steps: Vec<WorkflowStep> = parsed.get("steps")
                            .and_then(|s| s.as_array())
                            .map(|arr| {
                                arr.iter().filter_map(|v| {
                                    Some(WorkflowStep {
                                        name: v.get("name")?.as_str()?.to_string(),
                                        description: v.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                        assigned_to: v.get("assigned_to").and_then(|a| a.as_str()).unwrap_or("").to_string(),
                                        dependencies: v.get("dependencies")
                                            .and_then(|d| d.as_array())
                                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                            .unwrap_or_default(),
                                        status: "pending".to_string(),
                                    })
                                }).collect()
                            })
                            .unwrap_or_default();

                        if !roles.is_empty() || !steps.is_empty() {
                            return (roles, steps);
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("LLM workflow analysis failed: {}", e);
                }
                Err(_) => {
                    tracing::warn!("LLM workflow analysis timed out after 60 seconds");
                }
            }
        }

        // Fallback to rule-based analysis
        let roles = self.analyze_required_roles(description);
        let steps = self.generate_workflow_steps(&roles);
        (roles, steps)
    }

    // 根据任务描述分析需要的角色
    fn analyze_required_roles(&self, description: &str) -> Vec<String> {
        let mut roles = Vec::new();

        // 分析任务描述，确定需要的角色
        let description_lower = description.to_lowercase();

        // 从配置文件获取默认角色
        let default_roles = &self.config.workflow.default_roles;

        // 产品负责人关键词（英文和中文）
        if default_roles.contains(&"product_owner".to_string()) && 
           (description_lower.contains("product") || description_lower.contains("requirement") || description_lower.contains("user story") ||
            description_lower.contains("产品") || description_lower.contains("需求") || description_lower.contains("用户故事") ||
            description_lower.contains("运营")) {
            roles.push("product_owner".to_string());
        }

        // 架构师关键词（英文和中文）
        if default_roles.contains(&"architect".to_string()) && 
           (description_lower.contains("design") || description_lower.contains("architecture") || description_lower.contains("system") ||
            description_lower.contains("设计") || description_lower.contains("架构") || description_lower.contains("系统")) {
            roles.push("architect".to_string());
        }

        // 前端开发关键词（英文和中文）
        if default_roles.contains(&"frontend_developer".to_string()) && 
           (description_lower.contains("frontend") || description_lower.contains("ui") || description_lower.contains("interface") ||
            description_lower.contains("前端") || description_lower.contains("界面") || description_lower.contains("用户界面")) {
            roles.push("frontend_developer".to_string());
        }

        // 后端开发关键词（英文和中文）
        if default_roles.contains(&"backend_developer".to_string()) && 
           (description_lower.contains("backend") || description_lower.contains("api") || description_lower.contains("database") ||
            description_lower.contains("后端") || description_lower.contains("接口") || description_lower.contains("数据库")) {
            roles.push("backend_developer".to_string());
        }

        // QA工程师关键词（英文和中文）
        if default_roles.contains(&"qa_engineer".to_string()) && 
           (description_lower.contains("test") || description_lower.contains("qa") || description_lower.contains("quality") ||
            description_lower.contains("测试") || description_lower.contains("质量")) {
            roles.push("qa_engineer".to_string());
        }

        // 数据分析师关键词（英文和中文）
        if default_roles.contains(&"data_analyst".to_string()) && 
           (description_lower.contains("data") || description_lower.contains("analytics") || description_lower.contains("report") ||
            description_lower.contains("数据") || description_lower.contains("分析") || description_lower.contains("报表") ||
            description_lower.contains("运营")) {
            roles.push("data_analyst".to_string());
        }

        // 文档工程师关键词（英文和中文）
        if default_roles.contains(&"documentation_engineer".to_string()) && 
           (description_lower.contains("document") || description_lower.contains("doc") || description_lower.contains("manual") ||
            description_lower.contains("文档") || description_lower.contains("手册")) {
            roles.push("documentation_engineer".to_string());
        }

        // 总是添加 scrum_master 来协调工作（如果在默认角色中）
        if default_roles.contains(&"scrum_master".to_string()) && !roles.contains(&"scrum_master".to_string()) {
            roles.push("scrum_master".to_string());
        }
        
        // 如果没有匹配到任何角色，使用默认角色
        if roles.is_empty() {
            for role in default_roles.iter().take(4) {
                roles.push(role.clone());
            }
        }

        roles
    }

    // 发送工作流相关的群聊消息
    fn send_workflow_message(&self, workflow_id: &str, message_type: ChatMessageType, content: &str) {
        let task_id = Some(workflow_id.to_string());
        let author = self.config.current_agent_name.as_deref().unwrap_or("workflow_orchestrator");
        let author_type = "Workflow Orchestrator".to_string();
        let lang = "zh".to_string();
        
        let _ = self.chat_manager.send_message(
            None,
            task_id,
            author.to_string(),
            author_type,
            message_type,
            content.to_string(),
            lang,
            None,
            json!({"workflow_id": workflow_id}),
        );
    }

    // 基于角色生成工作流步骤
    fn generate_workflow_steps(&self, roles: &[String]) -> Vec<WorkflowStep> {
        let mut steps = Vec::new();

        // 从配置文件获取默认阶段
        let default_phases = &self.config.workflow.default_phases;

        // 产品需求分析
        if roles.contains(&"product_owner".to_string()) {
            let phase_name = if default_phases.contains(&"产品需求分析".to_string()) {
                "产品需求分析"
            } else {
                "Product Requirement Analysis"
            };
            steps.push(WorkflowStep {
                name: phase_name.to_string(),
                description: "分析并记录产品需求".to_string(),
                assigned_to: "product_owner".to_string(),
                dependencies: Vec::new(),
                status: "pending".to_string(),
            });
        }

        // 架构设计
        let mut arch_deps = Vec::new();
        if roles.contains(&"product_owner".to_string()) {
            let phase_name = if default_phases.contains(&"产品需求分析".to_string()) {
                "产品需求分析"
            } else {
                "Product Requirement Analysis"
            };
            arch_deps.push(phase_name.to_string());
        }
        if roles.contains(&"architect".to_string()) {
            let phase_name = if default_phases.contains(&"架构设计".to_string()) {
                "架构设计"
            } else {
                "Architecture Design"
            };
            steps.push(WorkflowStep {
                name: phase_name.to_string(),
                description: "设计系统架构和技术规范".to_string(),
                assigned_to: "architect".to_string(),
                dependencies: arch_deps,
                status: "pending".to_string(),
            });
        }

        // 开发阶段
        let mut dev_deps = Vec::new();
        if roles.contains(&"architect".to_string()) {
            let phase_name = if default_phases.contains(&"架构设计".to_string()) {
                "架构设计"
            } else {
                "Architecture Design"
            };
            dev_deps.push(phase_name.to_string());
        } else if roles.contains(&"product_owner".to_string()) {
            let phase_name = if default_phases.contains(&"产品需求分析".to_string()) {
                "产品需求分析"
            } else {
                "Product Requirement Analysis"
            };
            dev_deps.push(phase_name.to_string());
        }

        if roles.contains(&"frontend_developer".to_string()) {
            steps.push(WorkflowStep {
                name: "前端开发".to_string(),
                description: "实现用户界面和前端功能".to_string(),
                assigned_to: "frontend_developer".to_string(),
                dependencies: dev_deps.clone(),
                status: "pending".to_string(),
            });
        }

        if roles.contains(&"backend_developer".to_string()) {
            steps.push(WorkflowStep {
                name: "后端开发".to_string(),
                description: "实现后端API和服务器端功能".to_string(),
                assigned_to: "backend_developer".to_string(),
                dependencies: dev_deps.clone(),
                status: "pending".to_string(),
            });
        }

        // 测试阶段
        let mut test_deps = Vec::new();
        if roles.contains(&"frontend_developer".to_string()) {
            test_deps.push("前端开发".to_string());
        }
        if roles.contains(&"backend_developer".to_string()) {
            test_deps.push("后端开发".to_string());
        }

        if roles.contains(&"qa_engineer".to_string()) {
            let phase_name = if default_phases.contains(&"测试".to_string()) {
                "测试"
            } else {
                "Quality Assurance"
            };
            steps.push(WorkflowStep {
                name: phase_name.to_string(),
                description: "测试已实现的功能并报告缺陷".to_string(),
                assigned_to: "qa_engineer".to_string(),
                dependencies: test_deps,
                status: "pending".to_string(),
            });
        }

        // 数据分析
        if roles.contains(&"data_analyst".to_string()) {
            let mut data_deps = Vec::new();
            if roles.contains(&"backend_developer".to_string()) {
                data_deps.push("后端开发".to_string());
            }
            steps.push(WorkflowStep {
                name: "数据分析".to_string(),
                description: "分析数据并提供洞察".to_string(),
                assigned_to: "data_analyst".to_string(),
                dependencies: data_deps,
                status: "pending".to_string(),
            });
        }

        // 文档编写
        let mut doc_deps = Vec::new();
        if roles.contains(&"frontend_developer".to_string()) {
            doc_deps.push("前端开发".to_string());
        }
        if roles.contains(&"backend_developer".to_string()) {
            doc_deps.push("后端开发".to_string());
        }

        if roles.contains(&"documentation_engineer".to_string()) {
            steps.push(WorkflowStep {
                name: "文档编写".to_string(),
                description: "创建技术文档和用户指南".to_string(),
                assigned_to: "documentation_engineer".to_string(),
                dependencies: doc_deps,
                status: "pending".to_string(),
            });
        }

        // 项目交付
        let mut deliver_deps = Vec::new();
        if roles.contains(&"qa_engineer".to_string()) {
            let phase_name = if default_phases.contains(&"测试".to_string()) {
                "测试"
            } else {
                "Quality Assurance"
            };
            deliver_deps.push(phase_name.to_string());
        }
        if roles.contains(&"documentation_engineer".to_string()) {
            deliver_deps.push("文档编写".to_string());
        }
        if roles.contains(&"data_analyst".to_string()) {
            deliver_deps.push("数据分析".to_string());
        }
        if deliver_deps.is_empty() {
            if roles.contains(&"frontend_developer".to_string()) {
                deliver_deps.push("前端开发".to_string());
            } else if roles.contains(&"backend_developer".to_string()) {
                deliver_deps.push("后端开发".to_string());
            }
        }

        let phase_name = if default_phases.contains(&"交付".to_string()) {
            "交付"
        } else {
            "Project Delivery"
        };
        steps.push(WorkflowStep {
            name: phase_name.to_string(),
            description: "最终评审和项目交付".to_string(),
            assigned_to: "scrum_master".to_string(),
            dependencies: deliver_deps,
            status: "pending".to_string(),
        });

        steps
    }
}

#[async_trait]
impl Tool for WorkflowTool {
    fn name(&self) -> &str {
        "workflow"
    }

    fn description(&self) -> &str {
        "Workflow management tool for creating and managing team workflows. Use this to:
1. Create custom Scrum team workflows based on user prompts
2. Auto-generate workflows based on task requirements
3. Manage workflow execution and monitoring
4. Adjust workflow steps dynamically based on team feedback"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "auto_generate", "start", "pause", "resume", "stop", "status", "adjust", "boss_approve", "template_create", "template_list", "template_get", "template_delete", "template_generate", "template_validate", "create_from_template"],
                    "description": "Action to perform"
                },
                "workflow_name": {
                    "type": "string",
                    "description": "Name of the workflow"
                },
                "description": {
                    "type": "string",
                    "description": "Description of the workflow or task"
                },
                "roles": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Roles needed for the workflow"
                },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string"
                            },
                            "description": {
                                "type": "string"
                            },
                            "assigned_to": {
                                "type": "string"
                            },
                            "dependencies": {
                                "type": "array",
                                "items": {
                                    "type": "string"
                                }
                            }
                        }
                    },
                    "description": "Workflow steps"
                },
                "workflow_id": {
                    "type": "string",
                    "description": "ID of existing workflow"
                },
                "adjustments": {
                    "type": "object",
                    "description": "Workflow adjustments"
                },
                "template_id": {
                    "type": "string",
                    "description": "ID of workflow template"
                },
                "template_name": {
                    "type": "string",
                    "description": "Name of workflow template"
                },
                "template_author": {
                    "type": "string",
                    "description": "Author of workflow template"
                },
                "template": {
                    "type": "object",
                    "description": "Workflow template definition"
                },
                "prompt": {
                    "type": "string",
                    "description": "Prompt for generating workflow template"
                },
                "template_parameters": {
                    "type": "object",
                    "description": "Parameters for template generation or instantiation"
                }
            },
            "required": ["action"]
        })
    }

    fn spec(&self) -> crate::tools::ToolSpec {
        crate::tools::ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let execute_start = std::time::Instant::now();
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .ok_or_else(|| anyhow::anyhow!("Missing 'action'"))?;

        tracing::debug!("[Workflow] Executing action: {}", action);
        tracing::debug!("[Workflow] Action arguments: {:?}", args);

        // 验证 action 参数
        let valid_actions = ["create", "auto_generate", "start", "pause", "resume", "stop", "status", "adjust", "boss_approve", "template_create", "template_list", "template_get", "template_delete", "template_generate", "template_validate", "create_from_template"];
        if !valid_actions.contains(&action) {
            tracing::error!("[Workflow] Invalid action: {}", action);
            return Ok(ToolResult {
                success: false,
                output: "".to_string(),
                error: Some(format!("Invalid action: {}", action)),
            });
        }

        let result = match action {
            "create" => {
                let workflow_name = args.get("workflow_name").and_then(|v| v.as_str()).unwrap_or("custom_workflow");
                
                // 验证 workflow_name
                if workflow_name.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow name cannot be empty".to_string()),
                    });
                }
                
                let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
                
                // 为临时值创建持久绑定
                let empty_array = vec![];
                let roles = args.get("roles").and_then(|v| v.as_array()).unwrap_or(&empty_array);
                let steps = args.get("steps").and_then(|v| v.as_array()).unwrap_or(&empty_array);

                let workflow_id = Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();

                // 转换步骤
                let mut workflow_steps = Vec::new();
                for step in steps {
                    if let Some(name) = step.get("name").and_then(|v| v.as_str()) {
                        let step_description = step.get("description").and_then(|v| v.as_str()).unwrap_or("");
                        let assigned_to = step.get("assigned_to").and_then(|v| v.as_str()).unwrap_or("");
                        let dependencies = step.get("dependencies").and_then(|v| v.as_array()).unwrap_or(&empty_array);
                        let mut dep_vec = Vec::new();
                        for dep in dependencies {
                            if let Some(dep_name) = dep.as_str() {
                                dep_vec.push(dep_name.to_string());
                            }
                        }

                        workflow_steps.push(WorkflowStep {
                            name: name.to_string(),
                            description: step_description.to_string(),
                            assigned_to: assigned_to.to_string(),
                            dependencies: dep_vec,
                            status: "pending".to_string(),
                        });
                    }
                }

                // 转换角色
                let mut role_vec = Vec::new();
                for role in roles {
                    if let Some(role_name) = role.as_str() {
                        role_vec.push(role_name.to_string());
                    }
                }

                let workflow = Workflow {
                    id: workflow_id.clone(),
                    name: workflow_name.to_string(),
                    description: description.to_string(),
                    roles: role_vec,
                    steps: workflow_steps,
                    status: "created".to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                };

                // 保存工作流
                self.store.save_workflow(&workflow);
                
                // 发送工作流更新事件
                emit_workflow_event(&workflow);

                // 发送工作流创建通知
                let message = format!("工作流已创建: {}\n描述: {}\n参与角色: {}\n请各位团队成员查看并准备开始工作。", 
                    workflow.name, 
                    workflow.description, 
                    workflow.roles.join(", "));
                self.send_workflow_message(&workflow.id, ChatMessageType::TaskStatus, &message);

                Ok(ToolResult {
                    success: true,
                    output: format!("Workflow created successfully. ID: {}, Name: {}", workflow_id, workflow_name),
                    error: None,
                })
            }
            "auto_generate" => {
                let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
                
                tracing::debug!("[Workflow] Auto-generating workflow for: {}", description);
                
                // Check if a similar workflow already exists
                let existing_workflows = self.store.list_workflows();
                let description_lower = description.to_lowercase();
                
                // Look for existing workflow with similar description
                for existing in &existing_workflows {
                    if existing.description.to_lowercase().contains(&description_lower) 
                        || description_lower.contains(&existing.description.to_lowercase()) {
                        tracing::debug!("[Workflow] Found existing workflow: {}", existing.id);
                        
                        let mut output = format!("Found existing workflow. ID: {}\n", existing.id);
                        output.push_str(&format!("Name: {}\n", existing.name));
                        output.push_str(&format!("Description: {}\n", existing.description));
                        output.push_str("Required Roles:\n");
                        for role in &existing.roles {
                            output.push_str(&format!("- {}\n", role));
                        }
                        output.push_str("Workflow Steps:\n");
                        for (i, step) in existing.steps.iter().enumerate() {
                            output.push_str(&format!("{}. {} (Assigned to: {})\n", i + 1, step.name, step.assigned_to));
                        }
                        
                        return Ok(ToolResult {
                            success: true,
                            output,
                            error: None,
                        });
                    }
                }
                
                // No existing workflow found, create new one with LLM
                let (roles, steps) = self.analyze_workflow_with_llm(description).await;
                
                tracing::debug!("[Workflow] Analysis complete. Roles: {:?}, Steps: {}", roles, steps.len());
                
                let workflow_id = Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();
                
                let workflow = Workflow {
                    id: workflow_id.clone(),
                    name: format!("Auto-Generated Workflow for: {}", description.split(' ').take(3).collect::<Vec<&str>>().join(" ")),
                    description: description.to_string(),
                    roles,
                    steps,
                    status: "created".to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                };
                
                // Save workflow to store (persists to disk)
                self.store.save_workflow(&workflow);
                tracing::debug!("[Workflow] Saved workflow {} to store", workflow_id);
                
                // Emit workflow update event
                emit_workflow_event(&workflow);

                // Send workflow creation notification
                let message = format!("自动生成工作流: {}\n描述: {}\n参与角色: {}\n请各位团队成员查看并准备开始工作。", 
                    workflow.name, 
                    workflow.description, 
                    workflow.roles.join(", "));
                self.send_workflow_message(&workflow.id, ChatMessageType::TaskStatus, &message);
                
                // Generate detailed output
                let mut output = format!("Workflow auto-generated successfully. ID: {}\n", workflow_id);
                output.push_str(&format!("Name: {}\n", workflow.name));
                output.push_str(&format!("Description: {}\n", workflow.description));
                output.push_str("Required Roles:\n");
                for role in &workflow.roles {
                    output.push_str(&format!("- {}\n", role));
                }
                output.push_str("Workflow Steps:\n");
                for (i, step) in workflow.steps.iter().enumerate() {
                    output.push_str(&format!("{}. {} (Assigned to: {})\n", i + 1, step.name, step.assigned_to));
                    output.push_str(&format!("   Description: {}\n", step.description));
                    if !step.dependencies.is_empty() {
                        output.push_str(&format!("   Dependencies: {}\n", step.dependencies.join(", ")));
                    }
                }
                
                Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                })
            }
            "start" => {
                let workflow_id = args.get("workflow_id").and_then(|v| v.as_str()).unwrap_or("");
                
                // 验证 workflow_id
                if workflow_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(mut workflow) = self.store.get_workflow(workflow_id) {
                    // 检查是否存在分歧需要 Boss 批准
                    let has_disagreements = self.consensus_manager.has_disagreements(workflow_id.to_string()).unwrap_or(false);
                    
                    if has_disagreements {
                        // 自动解决分歧 - 继续执行
                        tracing::debug!("[Workflow] Auto-resolving disagreements for workflow: {}", workflow_id);
                        let _ = self.consensus_manager.provide_boss_approval(
                            workflow_id.to_string(),
                            None,
                            "Auto-Resolver".to_string(),
                            true,
                            "自动解决分歧，继续执行工作流".to_string(),
                            "zh",
                        );
                        
                        self.send_workflow_message(&workflow.id, ChatMessageType::TaskStatus, 
                            &format!("工作流 {} 存在分歧，已自动解决，继续执行", workflow.name));
                    }
                    
                    workflow.status = "running".to_string();
                    workflow.updated_at = chrono::Utc::now().to_rfc3339();
                    
                    // 启动第一个步骤
                    if !workflow.steps.is_empty() {
                        workflow.steps[0].status = "running".to_string();
                    }
                    
                    self.store.save_workflow(&workflow);
                    
                    // 发送工作流更新事件
                    emit_workflow_event(&workflow);

                    // 发送工作流启动通知
                    let message = format!("工作流已启动: {}\n当前状态: {}\n第一个任务已开始执行。", 
                        workflow.name, 
                        workflow.status);
                    self.send_workflow_message(&workflow.id, ChatMessageType::TaskStatus, &message);
                    
                    // 为每个步骤的负责人发送任务分配通知
                    for (_, step) in workflow.steps.iter().enumerate() {
                        let dependencies_str = if step.dependencies.is_empty() {
                            "无".to_string()
                        } else {
                            step.dependencies.join(", ")
                        };
                        let task_message = format!("任务分配: {}\n步骤: {}\n描述: {}\n依赖项: {}\n请做好准备，等待前序任务完成。", 
                            step.assigned_to, 
                            step.name, 
                            step.description, 
                            dependencies_str);
                        self.send_workflow_message(&workflow.id, ChatMessageType::TaskProgress, &task_message);
                    }
                    
                    Ok(ToolResult {
                        success: true,
                        output: format!("Workflow started successfully. ID: {}", workflow_id),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Workflow not found: {}", workflow_id)),
                    })
                }
            }
            "status" => {
                let workflow_id = args.get("workflow_id").and_then(|v| v.as_str()).unwrap_or("");
                
                // 验证 workflow_id
                if workflow_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(workflow) = self.store.get_workflow(workflow_id) {
                    let mut output = format!("Workflow Status\n");
                    output.push_str(&format!("ID: {}\n", workflow.id));
                    output.push_str(&format!("Name: {}\n", workflow.name));
                    output.push_str(&format!("Status: {}\n", workflow.status));
                    output.push_str(&format!("Created: {}\n", workflow.created_at));
                    output.push_str(&format!("Updated: {}\n", workflow.updated_at));
                    output.push_str("Steps:\n");
                    for (i, step) in workflow.steps.iter().enumerate() {
                        output.push_str(&format!("{}. {} - {}\n", i + 1, step.name, step.status));
                        output.push_str(&format!("   Assigned to: {}\n", step.assigned_to));
                    }
                    
                    Ok(ToolResult {
                        success: true,
                        output,
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Workflow not found: {}", workflow_id)),
                    })
                }
            }
            "pause" => {
                let workflow_id = args.get("workflow_id").and_then(|v| v.as_str()).unwrap_or("");
                
                // 验证 workflow_id
                if workflow_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(mut workflow) = self.store.get_workflow(workflow_id) {
                    workflow.status = "paused".to_string();
                    workflow.updated_at = chrono::Utc::now().to_rfc3339();
                    
                    // 同步更新所有步骤状态
                    for step in &mut workflow.steps {
                        if step.status == "running" {
                            step.status = "paused".to_string();
                        }
                    }
                    
                    self.store.save_workflow(&workflow);
                    
                    // 发送工作流更新事件
                    emit_workflow_event(&workflow);

                    Ok(ToolResult {
                        success: true,
                        output: format!("Workflow paused successfully. ID: {}", workflow_id),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Workflow not found: {}", workflow_id)),
                    })
                }
            }
            "resume" => {
                let workflow_id = args.get("workflow_id").and_then(|v| v.as_str()).unwrap_or("");
                
                // 验证 workflow_id
                if workflow_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(mut workflow) = self.store.get_workflow(workflow_id) {
                    workflow.status = "running".to_string();
                    workflow.updated_at = chrono::Utc::now().to_rfc3339();
                    
                    // 同步更新步骤状态
                    let mut found_running = false;
                    for step in &mut workflow.steps {
                        if step.status == "paused" {
                            step.status = "running".to_string();
                            found_running = true;
                            break;
                        }
                    }
                    
                    // 如果没有找到暂停的步骤，启动第一个未完成的步骤
                    if !found_running {
                        for step in &mut workflow.steps {
                            if step.status == "pending" {
                                step.status = "running".to_string();
                                break;
                            }
                        }
                    }
                    
                    self.store.save_workflow(&workflow);
                    
                    // 发送工作流更新事件
                    emit_workflow_event(&workflow);

                    Ok(ToolResult {
                        success: true,
                        output: format!("Workflow resumed successfully. ID: {}", workflow_id),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Workflow not found: {}", workflow_id)),
                    })
                }
            }
            "stop" => {
                let workflow_id = args.get("workflow_id").and_then(|v| v.as_str()).unwrap_or("");
                
                // 验证 workflow_id
                if workflow_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(mut workflow) = self.store.get_workflow(workflow_id) {
                    workflow.status = "stopped".to_string();
                    workflow.updated_at = chrono::Utc::now().to_rfc3339();
                    
                    // 同步更新所有步骤状态
                    for step in &mut workflow.steps {
                        if step.status != "completed" {
                            step.status = "stopped".to_string();
                        }
                    }
                    
                    self.store.save_workflow(&workflow);
                    
                    // 发送工作流更新事件
                    emit_workflow_event(&workflow);

                    Ok(ToolResult {
                        success: true,
                        output: format!("Workflow stopped successfully. ID: {}", workflow_id),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Workflow not found: {}", workflow_id)),
                    })
                }
            }
            "adjust" => {
                let workflow_id = args.get("workflow_id").and_then(|v| v.as_str()).unwrap_or("");
                
                // 验证 workflow_id
                if workflow_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow ID cannot be empty".to_string()),
                    });
                }
                
                // 为临时值创建持久绑定
                let empty_map = serde_json::Map::new();
                let adjustments = args.get("adjustments").and_then(|v| v.as_object()).unwrap_or(&empty_map);
                
                if let Some(mut workflow) = self.store.get_workflow(workflow_id) {
                    // 应用调整
                    if let Some(new_status) = adjustments.get("status").and_then(|v| v.as_str()) {
                        // 验证新状态
                        let valid_statuses = ["created", "running", "paused", "stopped", "completed", "waiting_for_boss_approval"];
                        if !valid_statuses.contains(&new_status) {
                            return Ok(ToolResult {
                                success: false,
                                output: "".to_string(),
                                error: Some(format!("Invalid status: {}", new_status)),
                            });
                        }
                        workflow.status = new_status.to_string();
                    }
                    
                    workflow.updated_at = chrono::Utc::now().to_rfc3339();
                    
                    self.store.save_workflow(&workflow);
                    
                    // 发送工作流更新事件
                    emit_workflow_event(&workflow);
                    
                    Ok(ToolResult {
                        success: true,
                        output: format!("Workflow adjusted successfully. ID: {}", workflow_id),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Workflow not found: {}", workflow_id)),
                    })
                }
            }
            "boss_approve" => {
                let workflow_id = args.get("workflow_id").and_then(|v| v.as_str()).unwrap_or("");
                let approved = args.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);
                let decision = args.get("decision").and_then(|v| v.as_str()).unwrap_or("");
                
                // 验证 workflow_id
                if workflow_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Workflow ID cannot be empty".to_string()),
                    });
                }
                
                // 验证 decision
                if decision.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Decision cannot be empty".to_string()),
                    });
                }
                
                if let Some(mut workflow) = self.store.get_workflow(workflow_id) {
                    // 提供 Boss 批准
                    let _ = self.consensus_manager.provide_boss_approval(
                        workflow_id.to_string(),
                        None,
                        "Boss".to_string(),
                        approved,
                        decision.to_string(),
                        "zh",
                    );
                    
                    // 更新工作流状态
                    if approved {
                        workflow.status = "running".to_string();
                        // 启动第一个步骤
                        if !workflow.steps.is_empty() {
                            workflow.steps[0].status = "running".to_string();
                        }
                    } else {
                        workflow.status = "stopped".to_string();
                        // 停止所有步骤
                        for step in &mut workflow.steps {
                            step.status = "stopped".to_string();
                        }
                    }
                    
                    workflow.updated_at = chrono::Utc::now().to_rfc3339();
                    self.store.save_workflow(&workflow);
                    
                    // 发送工作流更新事件
                    emit_workflow_event(&workflow);

                    // 发送工作流状态更新通知
                    let message = if approved {
                        format!("工作流已获得 Boss 批准: {}\n决定: {}\n工作流已开始执行。", 
                            workflow.name, 
                            decision)
                    } else {
                        format!("工作流已被 Boss 拒绝: {}\n决定: {}\n工作流已停止。", 
                            workflow.name, 
                            decision)
                    };
                    self.send_workflow_message(&workflow.id, ChatMessageType::TaskStatus, &message);
                    
                    Ok(ToolResult {
                        success: true,
                        output: format!("Boss approval processed successfully. ID: {}", workflow_id),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Workflow not found: {}", workflow_id)),
                    })
                }
            }
            "template_create" => {
                let template_name = args.get("template_name").and_then(|v| v.as_str()).unwrap_or("new_template");
                let template_author = args.get("template_author").and_then(|v| v.as_str()).unwrap_or("system");
                let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
                
                // 创建新模板
                let template = WorkflowTemplate::new(
                    template_name.to_string(),
                    description.to_string(),
                    template_author.to_string(),
                );
                
                // 保存模板
                self.template_store.save_template(&template);
                
                Ok(ToolResult {
                    success: true,
                    output: format!("Template created successfully. ID: {}, Name: {}", template.id, template_name),
                    error: None,
                })
            }
            "template_list" => {
                let templates = self.template_store.get_all_templates();
                
                let mut output = format!("Available Workflow Templates ({}):\n", templates.len());
                for template in templates {
                    output.push_str(&format!("- ID: {}\n", template.id));
                    output.push_str(&format!("  Name: {}\n", template.name));
                    output.push_str(&format!("  Description: {}\n", template.description));
                    output.push_str(&format!("  Version: {}\n", template.version));
                    output.push_str(&format!("  Author: {}\n", template.author));
                    output.push_str(&format!("  Categories: {}\n", template.categories.join(", ")));
                    output.push_str(&format!("  Phases: {}\n", template.phases.len()));
                    output.push_str(&format!("  Activities: {}\n", template.activities.len()));
                }
                
                Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                })
            }
            "template_get" => {
                let template_id = args.get("template_id").and_then(|v| v.as_str()).unwrap_or("");
                
                if template_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Template ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(template) = self.template_store.get_template(template_id) {
                    let output = serde_json::to_string_pretty(&template).unwrap();
                    Ok(ToolResult {
                        success: true,
                        output,
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Template not found: {}", template_id)),
                    })
                }
            }
            "template_delete" => {
                let template_id = args.get("template_id").and_then(|v| v.as_str()).unwrap_or("");
                
                if template_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Template ID cannot be empty".to_string()),
                    });
                }
                
                if self.template_store.delete_template(template_id) {
                    Ok(ToolResult {
                        success: true,
                        output: format!("Template deleted successfully. ID: {}", template_id),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Template not found: {}", template_id)),
                    })
                }
            }
            "template_generate" => {
                let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                let default_parameters = json!({});
                let parameters = args.get("template_parameters").unwrap_or(&default_parameters);
                let requester = args.get("template_author").and_then(|v| v.as_str()).unwrap_or("system");
                
                if prompt.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Prompt cannot be empty".to_string()),
                    });
                }
                
                // 生成模板
                match self.provider.as_ref() {
                    Some(provider) => {
                        let model = self.config.default_model.as_deref().unwrap_or("glm-5");
                        match self.template_generator.generate_template_from_prompt(
                            prompt.to_string(),
                            parameters.clone(),
                            requester.to_string(),
                            provider.as_ref(),
                            model,
                        ).await {
                            Ok(template) => {
                                // 保存生成的模板
                                self.template_store.save_template(&template);
                                
                                let mut output = format!("Template generated successfully. ID: {}\n", template.id);
                                output.push_str(&format!("Name: {}\n", template.name));
                                output.push_str(&format!("Description: {}\n", template.description));
                                output.push_str(&format!("Version: {}\n", template.version));
                                output.push_str(&format!("Author: {}\n", template.author));
                                output.push_str(&format!("Phases: {}\n", template.phases.len()));
                                output.push_str(&format!("Activities: {}\n", template.activities.len()));
                                output.push_str(&format!("Roles: {}\n", template.roles.len()));
                                
                                Ok(ToolResult {
                                    success: true,
                                    output,
                                    error: None,
                                })
                            }
                            Err(e) => {
                                Ok(ToolResult {
                                    success: false,
                                    output: "".to_string(),
                                    error: Some(format!("Failed to generate template: {:?}", e)),
                                })
                            }
                        }
                    }
                    None => {
                        Ok(ToolResult {
                            success: false,
                            output: "".to_string(),
                            error: Some("No provider available for template generation".to_string()),
                        })
                    }
                }
            }
            "template_validate" => {
                let template_id = args.get("template_id").and_then(|v| v.as_str()).unwrap_or("");
                
                if template_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Template ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(template) = self.template_store.get_template(template_id) {
                    let validation = template.validate();
                    
                    let mut output = format!("Template Validation Result:\n");
                    output.push_str(&format!("Valid: {}\n", validation.is_valid));
                    let error_message = validation.error_message.clone();
                    if let Some(error) = &validation.error_message {
                        output.push_str(&format!("Error: {}\n", error));
                    }
                    if !validation.warnings.is_empty() {
                        output.push_str("Warnings:\n");
                        for warning in validation.warnings {
                            output.push_str(&format!("- {}\n", warning));
                        }
                    }
                    
                    Ok(ToolResult {
                        success: validation.is_valid,
                        output,
                        error: error_message,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Template not found: {}", template_id)),
                    })
                }
            }
            "create_from_template" => {
                let template_id = args.get("template_id").and_then(|v| v.as_str()).unwrap_or("");
                let workflow_name = args.get("workflow_name").and_then(|v| v.as_str()).unwrap_or("");
                let _parameters = args.get("template_parameters").unwrap_or(&json!({}));
                
                if template_id.trim().is_empty() {
                    return Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some("Template ID cannot be empty".to_string()),
                    });
                }
                
                if let Some(template) = self.template_store.get_template(template_id) {
                    let workflow_id = Uuid::new_v4().to_string();
                    let now = chrono::Utc::now().to_rfc3339();
                    
                    // 从模板生成工作流步骤
                    let mut workflow_steps = Vec::new();
                    for activity in &template.activities {
                        workflow_steps.push(WorkflowStep {
                            name: activity.name.clone(),
                            description: activity.description.clone(),
                            assigned_to: activity.assigned_roles.first().unwrap_or(&"scrum_master".to_string()).clone(),
                            dependencies: activity.dependencies.clone(),
                            status: "pending".to_string(),
                        });
                    }
                    
                    // 提取角色
                    let roles: Vec<String> = template.roles.iter().map(|r| r.name.clone()).collect();
                    
                    let workflow = Workflow {
                        id: workflow_id.clone(),
                        name: if workflow_name.trim().is_empty() {
                            format!("{}", template.name)
                        } else {
                            workflow_name.to_string()
                        },
                        description: template.description.clone(),
                        roles,
                        steps: workflow_steps,
                        status: "created".to_string(),
                        created_at: now.clone(),
                        updated_at: now,
                    };
                    
                    // 保存工作流
                    self.store.save_workflow(&workflow);
                    
                    // 发送工作流更新事件
                    emit_workflow_event(&workflow);

                    // 发送工作流创建通知
                    let message = format!("基于模板创建工作流: {}\n描述: {}\n参与角色: {}\n请各位团队成员查看并准备开始工作。", 
                        workflow.name, 
                        workflow.description, 
                        workflow.roles.join(", "));
                    self.send_workflow_message(&workflow.id, ChatMessageType::TaskStatus, &message);
                    
                    Ok(ToolResult {
                        success: true,
                        output: format!("Workflow created from template successfully. ID: {}, Name: {}", workflow_id, workflow.name),
                        error: None,
                    })
                } else {
                    Ok(ToolResult {
                        success: false,
                        output: "".to_string(),
                        error: Some(format!("Template not found: {}", template_id)),
                    })
                }
            }
            _ => {
                Ok(ToolResult {
                    success: false,
                    output: "".to_string(),
                    error: Some(format!("Unsupported action: {}", action)),
                })
            }
        };
        
        let duration = execute_start.elapsed();
        tracing::debug!(
            "[Workflow] Action completed - action: {}, duration_ms: {}, success: {}",
            action,
            duration.as_millis(),
            result.as_ref().map(|r| r.success).unwrap_or(false)
        );
        
        result
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(WorkflowTool::new(
            self.security.clone(),
            self.config.clone(),
            self.manager.clone(),
            self.ctx.clone(),
        ))
    }
}
