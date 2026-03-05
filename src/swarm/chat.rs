use crate::swarm::store::SwarmSqliteStore;
use anyhow::Result;
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chacha20poly1305::aead::{Aead};
use chacha20poly1305::KeyInit;
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
    encryption_key: Key,
}

impl SwarmChatManager {
    pub fn new(workspace_dir: &std::path::Path) -> Self {
        // 生成或加载加密密钥
        let encryption_key = Self::get_or_create_encryption_key(workspace_dir);
        
        Self {
            store: Arc::new(SwarmSqliteStore::new(workspace_dir)),
            workspace_dir: workspace_dir.to_path_buf(),
            encryption_key,
        }
    }

    fn get_or_create_encryption_key(workspace_dir: &std::path::Path) -> Key {
        let key_path = workspace_dir.join(".zeroclaw").join("chat_encryption_key");
        
        // 尝试加载现有密钥
        if let Ok(key_data) = std::fs::read(&key_path) {
            if key_data.len() == 32 {
                let mut key = Key::default();
                key.copy_from_slice(&key_data);
                return key;
            }
        }
        
        // 生成新密钥
        let mut key_data = [0u8; 32];
        // 使用系统随机数生成器
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.fill(&mut key_data);
        
        // 保存密钥
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&key_path, &key_data).ok();
        
        let mut key = Key::default();
        key.copy_from_slice(&key_data);
        key
    }

    // 加密消息内容
    fn encrypt_content(&self, content: &str) -> Result<String> {
        let cipher = ChaCha20Poly1305::new(&self.encryption_key);
        let nonce = Nonce::from_slice(&[0u8; 12]); // 使用固定 nonce 用于演示，实际应使用随机 nonce
        
        let ciphertext = cipher.encrypt(nonce, content.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;
        
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&ciphertext))
    }

    // 解密消息内容
    fn decrypt_content(&self, encrypted_content: &str) -> Result<String> {
        let cipher = ChaCha20Poly1305::new(&self.encryption_key);
        let nonce = Nonce::from_slice(&[0u8; 12]); // 使用固定 nonce 用于演示，实际应使用随机 nonce
        
        use base64::Engine;
        let ciphertext = base64::engine::general_purpose::STANDARD.decode(encrypted_content)
            .map_err(|e| anyhow::anyhow!("Base64 decode failed: {:?}", e))?;
        
        let plaintext = cipher.decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;
        
        String::from_utf8(plaintext)
            .map_err(|e| anyhow::anyhow!("UTF-8 decode failed: {:?}", e))
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
        tracing::debug!("[SwarmChat] Sending message: author={}, type={:?}, task_id={:?}", author, message_type, task_id);
        
        // 验证参数
        if author.trim().is_empty() {
            tracing::error!("[SwarmChat] Author cannot be empty");
            return Err(anyhow::anyhow!("Author cannot be empty"));
        }
        
        if content.trim().is_empty() {
            tracing::error!("[SwarmChat] Content cannot be empty");
            return Err(anyhow::anyhow!("Content cannot be empty"));
        }
        
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        
        // 加密消息内容
        let encrypted_content = match self.encrypt_content(&content) {
            Ok(encrypted) => {
                tracing::debug!("[SwarmChat] Message encrypted successfully");
                encrypted
            }
            Err(e) => {
                tracing::error!("[SwarmChat] Encryption failed: {:?}", e);
                return Err(anyhow::anyhow!("Failed to encrypt message content: {:?}", e));
            }
        };
        
        let message = ChatMessage {
            id: id.clone(),
            run_id: run_id.map(|u| u.to_string()),
            task_id,
            author: author.clone(),
            author_type: author_type.clone(),
            message_type,
            content: encrypted_content,
            lang: lang.clone(),
            timestamp: now,
            parent_id,
            metadata,
        };
        
        // 保存消息
        match self.store.append_chat_extended(&message) {
            Ok(_) => {
                tracing::info!(
                    "[SwarmChat] {} ({}) [{}]: {}",
                    message.author,
                    message.lang,
                    format!("{:?}", message.message_type),
                    content // 日志中使用原始内容，便于调试
                );
                Ok(id)
            }
            Err(e) => {
                tracing::error!("[SwarmChat] Failed to save message: {:?}", e);
                Err(anyhow::anyhow!("Failed to save message: {:?}", e))
            }
        }
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
        // 验证参数
        let limit = limit.clamp(1, 500);
        
        let mut messages = match self.store.list_chat_extended(run_id, task_id, limit) {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::error!("[SwarmChat] Failed to retrieve messages: {:?}", e);
                return Err(anyhow::anyhow!("Failed to retrieve messages: {:?}", e));
            }
        };
        
        // 解密消息内容
        for message in &mut messages {
            match self.decrypt_content(&message.content) {
                Ok(decrypted) => {
                    message.content = decrypted;
                }
                Err(e) => {
                    tracing::warn!("[SwarmChat] Failed to decrypt message {}: {:?}", message.id, e);
                    // 保留加密内容，以便后续可能的恢复
                }
            }
        }
        
        Ok(messages)
    }
    
    pub fn get_conversation_history(
        &self,
        run_id: Option<Uuid>,
        task_id: Option<String>,
    ) -> Result<Vec<ChatMessage>> {
        // 获取最多 500 条消息
        self.get_messages(run_id, task_id, 500)
    }
    
    pub fn analyze_consensus(&self, task_id: String) -> Result<ConsensusState> {
        let messages = self.get_messages(None, Some(task_id.clone()), 100)?;
        let task_messages: Vec<&ChatMessage> = messages.iter().collect();
        
        let consensus_request = messages.iter()
            .find(|m| m.message_type == ChatMessageType::ConsensusRequest);
        
        let mut agreements = Vec::new();
        let mut disagreements = Vec::new();
        let mut pending_clarifications = Vec::new();
        let mut participants = std::collections::HashSet::new();
        
        for message in &messages {
            participants.insert(message.author.clone());
            
            match message.message_type {
                ChatMessageType::ConsensusResponse => {
                    if let Some(agrees) = message.metadata.get("agrees").and_then(|v| v.as_bool()) {
                        if agrees {
                            agreements.push(message.author.clone());
                        } else {
                            disagreements.push(message.author.clone());
                        }
                    }
                }
                ChatMessageType::Clarification => {
                    pending_clarifications.push(message.id.clone());
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
