use crate::config::Config;
use crate::security::SecurityPolicy;
use crate::swarm::{TaskStatus, TaskPriority, AssigneeType};
use crate::swarm::store::SwarmSqliteStore;
use crate::tools::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct TaskManagerTool {
    security: Arc<SecurityPolicy>,
    config: Arc<Config>,
    store: SwarmSqliteStore,
}

impl TaskManagerTool {
    pub fn new(
        security: Arc<SecurityPolicy>,
        config: Arc<Config>,
    ) -> Self {
        let store = SwarmSqliteStore::new(&config.workspace_dir);
        Self {
            security,
            config,
            store,
        }
    }

    fn parse_message(&self, message: &str, _sender: &str) -> ParsedTaskAction {
        let message_lower = message.to_lowercase();
        
        let mut action = ParsedTaskAction {
            action_type: TaskActionType::Unknown,
            title: String::new(),
            description: message.to_string(),
            priority: TaskPriority::Medium,
            assignee_type: AssigneeType::Unassigned,
            assignees: Vec::new(),
            mentions: Vec::new(),
            is_correction: false,
            parent_task_id: None,
        };

        // 检查是否是纠正任务
        if message_lower.contains("重新做") || message_lower.contains("纠正") || 
           message_lower.contains("修改") || message_lower.contains("不对") ||
           message_lower.contains("重新设计") || message_lower.contains("重做")
        {
            action.is_correction = true;
            action.action_type = TaskActionType::Correct;
        }

        // 检查关键词
        let keywords = vec![
            ("紧急", TaskPriority::Urgent),
            ("立刻", TaskPriority::Urgent),
            ("优先", TaskPriority::High),
            ("重要", TaskPriority::High),
            ("关键", TaskPriority::Critical),
        ];
        
        for (keyword, priority) in keywords {
            if message_lower.contains(keyword) {
                action.priority = priority;
                break;
            }
        }

        // 检查是否分配给团队
        if message_lower.contains("大家") || message_lower.contains("团队") || 
           message_lower.contains("一起")
        {
            action.assignee_type = AssigneeType::Team;
            action.action_type = TaskActionType::AssignTeam;
        }

        // 检查 @提及
        for agent_name in self.config.agents.keys() {
            let mention_pattern = format!("@{}", agent_name);
            if message.contains(&mention_pattern) {
                action.mentions.push(agent_name.clone());
                action.assignees.push(agent_name.clone());
            }
            // 也检查没有 @ 的情况，但内容中有角色名
            else if message_lower.contains(agent_name.to_lowercase().as_str()) {
                action.assignees.push(agent_name.clone());
            }
        }

        if !action.assignees.is_empty() {
            action.assignee_type = AssigneeType::Individual;
            if action.is_correction {
                action.action_type = TaskActionType::CorrectIndividual;
            } else {
                action.action_type = TaskActionType::AssignIndividual;
            }
        }

        // 提取任务标题
        if !action.is_correction && action.action_type == TaskActionType::Unknown {
            action.action_type = TaskActionType::Assign;
        }

        // 简单的标题提取
        action.title = self.extract_title(message);

        action
    }

    fn extract_title(&self, message: &str) -> String {
        // 简单的标题提取逻辑
        let title = message.trim().to_string();
        
        // 移除 @提及
        let mut cleaned = title.clone();
        for agent_name in self.config.agents.keys() {
            let mention_pattern = format!("@{}", agent_name);
            cleaned = cleaned.replace(&mention_pattern, "");
        }
        
        // 移除一些常见的开头词
        let prefixes = vec!["大家", "团队", "一起", "请", "帮我", "需要", "完成", "做"];
        let mut result = cleaned.trim().to_string();
        for prefix in prefixes {
            if result.starts_with(prefix) {
                result = result[prefix.len()..].trim().to_string();
            }
        }
        
        if result.is_empty() {
            cleaned.trim().to_string()
        } else {
            result
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedTaskAction {
    pub action_type: TaskActionType,
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
    pub assignee_type: AssigneeType,
    pub assignees: Vec<String>,
    pub mentions: Vec<String>,
    pub is_correction: bool,
    pub parent_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskActionType {
    Unknown,
    Assign,
    AssignTeam,
    AssignIndividual,
    Correct,
    CorrectIndividual,
    Update,
    Complete,
}

#[async_trait]
impl Tool for TaskManagerTool {
    fn name(&self) -> &str {
        "task_manager"
    }

    fn description(&self) -> &str {
        "Task management tool for parsing, creating, and managing tasks from group chat messages. Can identify task assignments, corrections, and updates."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["parse", "create", "update", "correct", "list", "get", "assign"],
                    "description": "Action to perform: parse (parse message for task), create (create new task), update (update existing task), correct (correct a task), list (list tasks), get (get task details), assign (assign task to someone)"
                },
                "message": {
                    "type": "string",
                    "description": "Message to parse for task content (for 'parse' action)"
                },
                "sender": {
                    "type": "string",
                    "description": "Sender of the message (for 'parse' action)"
                },
                "title": {
                    "type": "string",
                    "description": "Task title (for 'create' action)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (for 'create' action)"
                },
                "task_id": {
                    "type": "string",
                    "description": "Task ID (for 'update', 'correct', 'get' actions)"
                },
                "assignees": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Assignee names (for 'assign' action)"
                },
                "priority": {
                    "type": "string",
                    "enum": ["low", "medium", "high", "critical", "urgent"],
                    "description": "Task priority (for 'create' action)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of tasks to list (for 'list' action)"
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
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        
        match action {
            "parse" => self.handle_parse(&args).await,
            "create" => self.handle_create(&args).await,
            "update" => self.handle_update(&args).await,
            "correct" => self.handle_correct(&args).await,
            "list" => self.handle_list(&args).await,
            "get" => self.handle_get(&args).await,
            "assign" => self.handle_assign(&args).await,
            _ => anyhow::bail!("Unknown action: {}", action),
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(Self::new(
            self.security.clone(),
            self.config.clone(),
        ))
    }
}

impl TaskManagerTool {
    async fn handle_parse(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let message = args.get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("message is required for parse action"))?;
        
        let sender = args.get("sender")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let parsed = self.parse_message(message, sender);
        
        let output = json!({
            "action_type": format!("{:?}", parsed.action_type),
            "title": parsed.title,
            "description": parsed.description,
            "priority": format!("{:?}", parsed.priority),
            "assignee_type": format!("{:?}", parsed.assignee_type),
            "assignees": parsed.assignees,
            "mentions": parsed.mentions,
            "is_correction": parsed.is_correction,
        });

        Ok(ToolResult {
            success: true,
            output: output.to_string(),
            error: None,
        })
    }

    async fn handle_create(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let title = args.get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("title is required for create action"))?;
        
        let description = args.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        let priority_str = args.get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("medium");
        
        let priority = match priority_str {
            "low" => TaskPriority::Low,
            "high" => TaskPriority::High,
            "critical" => TaskPriority::Critical,
            "urgent" => TaskPriority::Urgent,
            _ => TaskPriority::Medium,
        };

        let assignee_type_str = args.get("assignee_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unassigned");
        
        let assignee_type = match assignee_type_str {
            "team" => AssigneeType::Team,
            "individual" => AssigneeType::Individual,
            _ => AssigneeType::Unassigned,
        };

        let assigned_by = self.config.current_agent_name.as_deref().unwrap_or("orchestrator").to_string();
        
        let task_id = self.store.create_intelligent_task(
            title.to_string(),
            description.to_string(),
            priority,
            assignee_type.clone(),
            assigned_by,
            None,
            None,
            json!({}),
        )?;

        if let Some(assignees) = args.get("assignees").and_then(|v| v.as_array()) {
            for assignee in assignees {
                if let Some(assignee_name) = assignee.as_str() {
                    self.store.add_task_assignee(task_id.clone(), assignee_name.to_string())?;
                }
            }
        }

        Ok(ToolResult {
            success: true,
            output: format!("Task created successfully. ID: {}", task_id),
            error: None,
        })
    }

    async fn handle_update(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let task_id = args.get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("task_id is required for update action"))?;
        
        if let Some(mut task) = self.store.get_intelligent_task(task_id)? {
            if let Some(title) = args.get("title").and_then(|v| v.as_str()) {
                task.title = title.to_string();
            }
            
            if let Some(description) = args.get("description").and_then(|v| v.as_str()) {
                task.description = description.to_string();
            }
            
            if let Some(status_str) = args.get("status").and_then(|v| v.as_str()) {
                task.status = match status_str {
                    "draft" => TaskStatus::Draft,
                    "pending" => TaskStatus::Pending,
                    "assigned" => TaskStatus::Assigned,
                    "in_progress" => TaskStatus::InProgress,
                    "review" => TaskStatus::Review,
                    "completed" => TaskStatus::Completed,
                    "cancelled" => TaskStatus::Cancelled,
                    "corrected" => TaskStatus::Corrected,
                    _ => task.status,
                };
            }
            
            if let Some(progress) = args.get("progress").and_then(|v| v.as_f64()) {
                task.progress = progress;
            }
            
            task.updated_at = now_unix();
            
            self.store.store_intelligent_task(&task)?;
            
            Ok(ToolResult {
                success: true,
                output: format!("Task updated successfully. ID: {}", task_id),
                error: None,
            })
        } else {
            Ok(ToolResult {
                success: false,
                output: "".to_string(),
                error: Some(format!("Task not found: {}", task_id)),
            })
        }
    }

    async fn handle_correct(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let task_id = args.get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("task_id is required for correct action"))?;
        
        let description = args.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        if let Some(mut task) = self.store.get_intelligent_task(task_id)? {
            task.status = TaskStatus::Corrected;
            task.description = format!("{}\n\n[Correction]: {}", task.description, description);
            task.updated_at = now_unix();
            
            self.store.store_intelligent_task(&task)?;
            
            Ok(ToolResult {
                success: true,
                output: format!("Task corrected successfully. ID: {}", task_id),
                error: None,
            })
        } else {
            Ok(ToolResult {
                success: false,
                output: "".to_string(),
                error: Some(format!("Task not found: {}", task_id)),
            })
        }
    }

    async fn handle_list(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
        
        let tasks = self.store.list_intelligent_tasks(Some(limit))?;
        
        let output = if tasks.is_empty() {
            "No tasks found.".to_string()
        } else {
            let formatted: Vec<String> = tasks.iter().map(|t| {
                format!(
                    "[{}] {} ({:?} - {:?}",
                    t.id, t.title, t.status, t.priority
                )
            }).collect();
            
            format!("Tasks:\n{}", formatted.join("\n"))
        };

        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }

    async fn handle_get(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let task_id = args.get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("task_id is required for get action"))?;
        
        if let Some(task) = self.store.get_intelligent_task(task_id)? {
            let assignees = self.store.get_task_assignees(task_id)?;
            
            let output = json!({
                "id": task.id,
                "title": task.title,
                "description": task.description,
                "status": format!("{:?}", task.status),
                "priority": format!("{:?}", task.priority),
                "assignee_type": format!("{:?}", task.assignee_type),
                "assignees": assignees.iter().map(|a| a.assignee_name.clone()).collect::<Vec<_>>(),
                "progress": task.progress,
            });
            
            Ok(ToolResult {
                success: true,
                output: output.to_string(),
                error: None,
            })
        } else {
            Ok(ToolResult {
                success: false,
                output: "".to_string(),
                error: Some(format!("Task not found: {}", task_id)),
            })
        }
    }

    async fn handle_assign(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let task_id = args.get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("task_id is required for assign action"))?;
        
        let assignee_type_str = args.get("assignee_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unassigned");
        
        let assignee_type = match assignee_type_str {
            "team" => AssigneeType::Team,
            "individual" => AssigneeType::Individual,
            _ => AssigneeType::Unassigned,
        };
        
        if let Some(mut task) = self.store.get_intelligent_task(task_id)? {
            task.assignee_type = assignee_type.clone();
            task.status = TaskStatus::Assigned;
            task.updated_at = now_unix();
            
            self.store.store_intelligent_task(&task)?;
            
            if let Some(assignees) = args.get("assignees").and_then(|v| v.as_array()) {
                for assignee in assignees {
                    if let Some(assignee_name) = assignee.as_str() {
                        self.store.add_task_assignee(task_id.to_string(), assignee_name.to_string())?;
                    }
                }
            }
            
            Ok(ToolResult {
                success: true,
                output: format!("Task assigned successfully. ID: {}", task_id),
                error: None,
            })
        } else {
            Ok(ToolResult {
                success: false,
                output: "".to_string(),
                error: Some(format!("Task not found: {}", task_id)),
            })
        }
    }
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
