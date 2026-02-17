use crate::swarm::store::SwarmSqliteStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatMessageType {
    TaskAssignment,
    TaskStatus,
    TaskProgress,
    TaskCompletion,
    TaskFailure,
    ConsensusRequest,
    ConsensusResponse,
    Disagreement,
    Clarification,
    Correction,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub author: String,
    pub author_type: String,
    pub message_type: ChatMessageType,
    pub content: String,
    pub lang: String,
    pub timestamp: u64,
    pub parent_id: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusState {
    pub task_id: String,
    pub status: String,
    pub participants: Vec<String>,
    pub agreements: Vec<String>,
    pub disagreements: Vec<String>,
    pub pending_clarifications: Vec<String>,
    pub resolution: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

pub struct SwarmChatManager {
    store: Arc<SwarmSqliteStore>,
    workspace_dir: std::path::PathBuf,
}

impl SwarmChatManager {
    pub fn new(workspace_dir: &std::path::Path) -> Self {
        Self {
            store: Arc::new(SwarmSqliteStore::new(workspace_dir)),
            workspace_dir: workspace_dir.to_path_buf(),
        }
    }

    pub fn send_message(
        &self,
        run_id: Option<Uuid>,
        task_id: Option<String>,
        author: String,
        author_type: String,
        message_type: ChatMessageType,
        content: String,
        lang: String,
        parent_id: Option<String>,
        metadata: serde_json::Value,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let message = ChatMessage {
            id: id.clone(),
            run_id: run_id.map(|u| u.to_string()),
            task_id,
            author,
            author_type,
            message_type,
            content,
            lang,
            timestamp: now,
            parent_id,
            metadata,
        };
        self.store.append_chat_extended(&message)?;
        
        tracing::info!(
            "[SwarmChat] {} ({}) [{}]: {}",
            message.author,
            message.lang,
            format!("{:?}", message.message_type),
            message.content
        );
        
        Ok(id)
    }

    pub fn send_task_assignment(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        task_description: String,
        lang: String,
    ) -> Result<String> {
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::TaskAssignment,
            task_description,
            lang,
            None,
            serde_json::json!({}),
        )
    }

    pub fn send_task_status(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        status: String,
        details: String,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            format!("状态: {} - {}", status, details)
        } else {
            format!("Status: {} - {}", status, details)
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::TaskStatus,
            content,
            lang,
            None,
            serde_json::json!({ "status": status }),
        )
    }

    pub fn send_task_progress(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        progress: f64,
        total: f64,
        message: String,
        lang: String,
    ) -> Result<String> {
        let progress_percent = if total > 0.0 {
            (progress / total * 100.0) as i32
        } else {
            0
        };
        
        let content = if lang == "zh" {
            format!("进度: {}/{} ({}%) - {}", progress, total, progress_percent, message)
        } else {
            format!("Progress: {}/{} ({}%) - {}", progress, total, progress_percent, message)
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::TaskProgress,
            content,
            lang,
            None,
            serde_json::json!({ "progress": progress, "total": total }),
        )
    }

    pub fn send_task_completion(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        result: String,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            format!("任务完成: {}", result)
        } else {
            format!("Task completed: {}", result)
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::TaskCompletion,
            content,
            lang,
            None,
            serde_json::json!({ "result": result }),
        )
    }

    pub fn send_task_failure(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        error: String,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            format!("任务失败: {}", error)
        } else {
            format!("Task failed: {}", error)
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::TaskFailure,
            content,
            lang,
            None,
            serde_json::json!({ "error": error }),
        )
    }

    pub fn request_consensus(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        topic: String,
        participants: Vec<String>,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            format!("请求共识: {} (参与者: {})", topic, participants.join(", "))
        } else {
            format!("Requesting consensus: {} (participants: {})", topic, participants.join(", "))
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::ConsensusRequest,
            content,
            lang,
            None,
            serde_json::json!({ "topic": topic, "participants": participants }),
        )
    }

    pub fn respond_consensus(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        agrees: bool,
        reason: String,
        parent_id: String,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            if agrees {
                format!("同意: {}", reason)
            } else {
                format!("不同意: {}", reason)
            }
        } else {
            if agrees {
                format!("Agree: {}", reason)
            } else {
                format!("Disagree: {}", reason)
            }
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::ConsensusResponse,
            content,
            lang,
            Some(parent_id),
            serde_json::json!({ "agrees": agrees, "reason": reason }),
        )
    }

    pub fn report_disagreement(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        issue: String,
        suggestion: Option<String>,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            if let Some(ref s) = suggestion {
                format!("异议: {} (建议: {})", issue, s)
            } else {
                format!("异议: {}", issue)
            }
        } else {
            if let Some(ref s) = suggestion {
                format!("Disagreement: {} (suggestion: {})", issue, s)
            } else {
                format!("Disagreement: {}", issue)
            }
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::Disagreement,
            content,
            lang,
            None,
            serde_json::json!({ "issue": issue, "suggestion": suggestion }),
        )
    }

    pub fn request_clarification(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        question: String,
        parent_id: String,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            format!("需要澄清: {}", question)
        } else {
            format!("Clarification needed: {}", question)
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::Clarification,
            content,
            lang,
            Some(parent_id),
            serde_json::json!({ "question": question }),
        )
    }

    pub fn provide_correction(
        &self,
        run_id: Option<Uuid>,
        task_id: String,
        author: String,
        author_type: String,
        correction: String,
        parent_id: String,
        lang: String,
    ) -> Result<String> {
        let content = if lang == "zh" {
            format!("修正: {}", correction)
        } else {
            format!("Correction: {}", correction)
        };
        
        self.send_message(
            run_id,
            Some(task_id.clone()),
            author,
            author_type,
            ChatMessageType::Correction,
            content,
            lang,
            Some(parent_id),
            serde_json::json!({ "correction": correction }),
        )
    }

    pub fn get_messages(
        &self,
        run_id: Option<Uuid>,
        task_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<ChatMessage>> {
        self.store.list_chat_extended(run_id, task_id, limit)
    }

    pub fn get_conversation_history(
        &self,
        run_id: Option<Uuid>,
        task_id: Option<String>,
    ) -> Result<Vec<ChatMessage>> {
        let mut messages = self.store.list_chat_extended(run_id, task_id, 100)?;
        messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(messages)
    }

    pub fn analyze_consensus(
        &self,
        task_id: String,
    ) -> Result<ConsensusState> {
        let messages = self.store.list_chat_extended(None, Some(task_id.clone()), 500)?;
        let task_messages: Vec<_> = messages
            .iter()
            .collect();
        
        let mut participants = std::collections::HashSet::new();
        let mut agreements = Vec::new();
        let mut disagreements = Vec::new();
        let mut pending_clarifications = Vec::new();
        let mut consensus_request: Option<&ChatMessage> = None;
        
        for msg in &task_messages {
            participants.insert(msg.author.clone());
            
            match msg.message_type {
                ChatMessageType::ConsensusRequest => {
                    consensus_request = Some(msg);
                }
                ChatMessageType::ConsensusResponse => {
                    if let Some(agrees) = msg.metadata.get("agrees").and_then(|v| v.as_bool()) {
                        if agrees {
                            agreements.push(msg.author.clone());
                        } else {
                            disagreements.push(msg.author.clone());
                        }
                    }
                }
                ChatMessageType::Clarification => {
                    pending_clarifications.push(msg.content.clone());
                }
                _ => {}
            }
        }
        
        let status = if consensus_request.is_none() {
            if lang_is_chinese(&task_messages) {
                "未请求共识".to_string()
            } else {
                "Consensus not requested".to_string()
            }
        } else if !pending_clarifications.is_empty() {
            if lang_is_chinese(&task_messages) {
                "等待澄清".to_string()
            } else {
                "Waiting for clarification".to_string()
            }
        } else if !disagreements.is_empty() {
            if lang_is_chinese(&task_messages) {
                "存在异议".to_string()
            } else {
                "Disagreements exist".to_string()
            }
        } else if agreements.len() > 0 {
            if lang_is_chinese(&task_messages) {
                "达成共识".to_string()
            } else {
                "Consensus reached".to_string()
            }
        } else {
            if lang_is_chinese(&task_messages) {
                "等待响应".to_string()
            } else {
                "Waiting for response".to_string()
            }
        };
        
        let created_at = task_messages
            .first()
            .map(|m| m.timestamp)
            .unwrap_or_else(now_unix);
        let updated_at = task_messages
            .last()
            .map(|m| m.timestamp)
            .unwrap_or_else(now_unix);
        
        Ok(ConsensusState {
            task_id,
            status,
            participants: participants.into_iter().collect(),
            agreements,
            disagreements,
            pending_clarifications,
            resolution: None,
            created_at,
            updated_at,
        })
    }
}

fn lang_is_chinese(messages: &[&ChatMessage]) -> bool {
    messages.iter().any(|m| m.lang == "zh")
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
    fn test_send_and_receive_message() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SwarmChatManager::new(temp_dir.path());

        let msg_id = manager
            .send_message(
                None,
                Some("task-1".to_string()),
                "agent-1".to_string(),
                "main_agent".to_string(),
                ChatMessageType::Info,
                "Test message".to_string(),
                "en".to_string(),
                None,
                serde_json::json!({}),
            )
            .unwrap();

        assert!(!msg_id.is_empty());
        
        let messages = manager.get_messages(None, Some("task-1".to_string()), 10).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Test message");
    }

    #[test]
    fn test_task_assignment() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SwarmChatManager::new(temp_dir.path());

        let msg_id = manager
            .send_task_assignment(
                None,
                "task-2".to_string(),
                "main".to_string(),
                "orchestrator".to_string(),
                "Complete the analysis".to_string(),
                "en".to_string(),
            )
            .unwrap();

        assert!(!msg_id.is_empty());
    }

    #[test]
    fn test_consensus_flow() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SwarmChatManager::new(temp_dir.path());

        let request_id = manager
            .request_consensus(
                None,
                "task-3".to_string(),
                "main".to_string(),
                "orchestrator".to_string(),
                "Should we proceed?".to_string(),
                vec!["agent-1".to_string(), "agent-2".to_string()],
                "en".to_string(),
            )
            .unwrap();

        let response_id = manager
            .respond_consensus(
                None,
                "task-3".to_string(),
                "agent-1".to_string(),
                "subagent".to_string(),
                true,
                "Looks good".to_string(),
                request_id,
                "en".to_string(),
            )
            .unwrap();

        assert!(!response_id.is_empty());
        
        let consensus = manager.analyze_consensus("task-3".to_string()).unwrap();
        assert_eq!(consensus.agreements.len(), 1);
        assert!(consensus.participants.contains(&"agent-1".to_string()));
    }

    #[test]
    fn test_chinese_messages() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SwarmChatManager::new(temp_dir.path());

        let msg_id = manager
            .send_task_status(
                None,
                "task-4".to_string(),
                "agent-1".to_string(),
                "subagent".to_string(),
                "进行中".to_string(),
                "正在处理数据".to_string(),
                "zh".to_string(),
            )
            .unwrap();

        assert!(!msg_id.is_empty());
        
        let messages = manager.get_messages(None, Some("task-4".to_string()), 10).unwrap();
        assert!(messages[0].content.contains("进行中"));
    }
}
