use crate::config::Config;
use crate::security::SecurityPolicy;
use crate::swarm::chat::{ChatMessageType, SwarmChatManager};
use crate::swarm::SwarmContext;
use crate::tools::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct GroupChatTool {
    security: Arc<SecurityPolicy>,
    config: Arc<Config>,
    ctx: SwarmContext,
    chat_manager: SwarmChatManager,
    agent_name: Option<String>,
}

impl GroupChatTool {
    pub fn new(
        security: Arc<SecurityPolicy>,
        config: Arc<Config>,
        ctx: SwarmContext,
    ) -> Self {
        let chat_manager = SwarmChatManager::new(&config.workspace_dir);
        Self {
            security,
            config,
            ctx,
            chat_manager,
            agent_name: None,
        }
    }
    
    pub fn with_agent_name(mut self, agent_name: String) -> Self {
        self.agent_name = Some(agent_name);
        self
    }
    
    fn get_agent_role(&self, agent_name: &str) -> String {
        if let Some(agent_config) = self.config.agents.get(agent_name) {
            if let Some(ref prompt) = agent_config.system_prompt {
                if prompt.contains("project manager") || prompt.contains("项目经理") {
                    return "Project Manager".to_string();
                } else if prompt.contains("frontend") || prompt.contains("前端") {
                    return "Frontend Developer".to_string();
                } else if prompt.contains("backend") || prompt.contains("后端") {
                    return "Backend Developer".to_string();
                } else if prompt.contains("data analyst") || prompt.contains("数据分析师") {
                    return "Data Analyst".to_string();
                } else if prompt.contains("qa") || prompt.contains("测试") {
                    return "QA Engineer".to_string();
                }
            }
        }
        agent_name.to_string()
    }
}

#[async_trait]
impl Tool for GroupChatTool {
    fn name(&self) -> &str {
        "group_chat"
    }

    fn description(&self) -> &str {
        "Group chat tool for real-time communication between agents. Use this to broadcast messages, report progress, ask questions, or share results with other team members. This enables asynchronous collaboration without blocking."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["send", "read", "subscribe"],
                    "description": "Action to perform: send (broadcast message), read (get recent messages), subscribe (get updates since last check)"
                },
                "message_type": {
                    "type": "string",
                    "enum": ["progress", "status", "question", "answer", "result", "alert"],
                    "description": "Type of message being sent"
                },
                "content": {
                    "type": "string",
                    "description": "Message content to send (required for 'send' action)"
                },
                "task_id": {
                    "type": "string",
                    "description": "Optional task ID this message relates to"
                },
                "since": {
                    "type": "integer",
                    "description": "Timestamp to get messages since (for 'subscribe' action)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to retrieve (default: 20)"
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
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("read");
        
        match action {
            "send" => self.handle_send(&args).await,
            "read" => self.handle_read(&args).await,
            "subscribe" => self.handle_subscribe(&args).await,
            _ => anyhow::bail!("Unknown action: {}", action),
        }
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        let mut cloned = GroupChatTool::new(
            self.security.clone(),
            self.config.clone(),
            self.ctx.clone()
        );
        if let Some(agent_name) = &self.agent_name {
            cloned = cloned.with_agent_name(agent_name.clone());
        }
        Box::new(cloned)
    }
}

impl GroupChatTool {
    // 检查发送消息的权限
    fn check_send_permission(&self, author: &str, message_type: &ChatMessageType) -> anyhow::Result<()> {
        // 验证 author
        if author.trim().is_empty() {
            return Err(anyhow::anyhow!("Author cannot be empty"));
        }
        
        // 检查是否是有效的 agent
        if !self.config.agents.contains_key(author) && self.config.current_agent_name.as_deref() != Some(author) {
            return Err(anyhow::anyhow!("Unauthorized agent: {}", author));
        }
        
        // 对于特定消息类型的额外权限检查
        match message_type {
            ChatMessageType::TaskAssignment => {
                // 只有项目经理或协调者可以分配任务
                let author_role = self.get_agent_role(author);
                if !author_role.contains("Manager") && !author_role.contains("Coordinator") {
                    return Err(anyhow::anyhow!("Only managers can assign tasks"));
                }
            }
            ChatMessageType::ConsensusRequest => {
                // 只有协调者可以请求共识
                let author_role = self.get_agent_role(author);
                if !author_role.contains("Coordinator") && !author_role.contains("Manager") {
                    return Err(anyhow::anyhow!("Only coordinators can request consensus"));
                }
            }
            _ => {
                // 其他消息类型没有特殊权限要求
            }
        }
        
        Ok(())
    }

    async fn handle_send(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let content = args.get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("content is required for send action"))?;
        
        // 验证 content
        if content.trim().is_empty() {
            return Ok(ToolResult {
                success: false,
                output: "".to_string(),
                error: Some("Message content cannot be empty".to_string()),
            });
        }
        
        let message_type_str = args.get("message_type")
            .and_then(|v| v.as_str())
            .unwrap_or("status");
        
        let task_id = args.get("task_id").and_then(|v| v.as_str()).map(String::from);
        
        let message_type = match message_type_str {
            "progress" => ChatMessageType::TaskProgress,
            "status" => ChatMessageType::TaskStatus,
            "question" => ChatMessageType::Clarification,
            "answer" => ChatMessageType::Info,
            "result" => ChatMessageType::TaskCompletion,
            "alert" => ChatMessageType::TaskFailure,
            _ => ChatMessageType::Info,
        };
        
        let author = self.config.current_agent_name.as_deref()
            .unwrap_or("orchestrator");
        
        // 检查发送权限
        if let Err(e) = self.check_send_permission(author, &message_type) {
            return Ok(ToolResult {
                success: false,
                output: "".to_string(),
                error: Some(e.to_string()),
            });
        }
        
        let author_type = self.get_agent_role(author);
        let lang = "en".to_string();
        
        let msg_id = self.chat_manager.send_message(
            None,
            task_id,
            author.to_string(),
            author_type.clone(),
            message_type,
            content.to_string(),
            lang,
            None,
            json!({"role": author_type}),
        )?;
        
        Ok(ToolResult {
            success: true,
            output: format!("Message sent successfully. ID: {}", msg_id),
            error: None,
        })
    }

    async fn handle_read(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
        
        let messages = self.chat_manager.get_messages(None, None, limit)?;
        
        let output = if messages.is_empty() {
            "No messages found.".to_string()
        } else {
            let formatted: Vec<String> = messages.iter().map(|m| {
                let role = m.metadata.get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&m.author_type);
                format!(
                    "[{}] {} [{}]: {}",
                    format_timestamp(m.timestamp),
                    role,
                    format!("{:?}", m.message_type),
                    m.content
                )
            }).collect();
            
            format!("Recent messages:\n{}", formatted.join("\n"))
        };
        
        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }

    async fn handle_subscribe(&self, args: &Value) -> anyhow::Result<ToolResult> {
        let since = args.get("since").and_then(|v| v.as_u64()).unwrap_or(0);
        
        let all_messages = self.chat_manager.get_messages(None, None, 100)?;
        let messages: Vec<_> = all_messages.into_iter().filter(|m| m.timestamp > since).collect();
        
        let output = if messages.is_empty() {
            json!({
                "has_new": false,
                "messages": [],
                "last_timestamp": since
            }).to_string()
        } else {
            let last_ts = messages.last().map(|m| m.timestamp).unwrap_or(since);
            json!({
                "has_new": true,
                "messages": messages.iter().map(|m| json!({
                    "id": m.id,
                    "author": m.author,
                    "type": format!("{:?}", m.message_type),
                    "content": m.content,
                    "timestamp": m.timestamp
                })).collect::<Vec<_>>(),
                "last_timestamp": last_ts
            }).to_string()
        };
        
        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }
}

fn format_timestamp(ts: u64) -> String {
    use std::time::UNIX_EPOCH;
    let duration = UNIX_EPOCH + std::time::Duration::from_secs(ts);
    let datetime: chrono::DateTime<chrono::Utc> = duration.into();
    datetime.format("%H:%M:%S").to_string()
}
