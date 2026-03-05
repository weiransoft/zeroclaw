//! 聊天会话和消息存储

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

/// 聊天会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub agent_id: Option<String>,
    pub metadata: serde_json::Value,
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    pub tool_calls: Option<serde_json::Value>,
    pub metadata: serde_json::Value,
}

/// 聊天存储
///
/// 使用统一的数据库连接池管理聊天相关的持久化数据
pub struct ChatStore {
    /// 数据库连接池
    pool: Arc<DbPool>,
}

impl ChatStore {
    /// 创建新的聊天存储
    ///
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    ///
    /// # Returns
    /// 初始化好的 ChatStore 实例
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        // 使用统一配置创建连接池
        let config = SqliteConfig::from_workspace(workspace_dir, "chat.db");
        let pool = Arc::new(DbPool::new(config)?);
        
        Ok(Self {
            pool,
        })
    }
    
    /// 从现有连接池创建聊天存储
    ///
    /// # Arguments
    /// * `pool` - 数据库连接池
    ///
    /// # Returns
    /// ChatStore 实例
    pub fn from_pool(pool: Arc<DbPool>) -> Self {
        Self {
            pool,
        }
    }
    

    
    /// 初始化数据库 Schema
    ///
    /// 从 SQL 文件加载并执行数据库初始化脚本
    pub fn init_schema(&self) -> Result<()> {
        let loader = SqlLoader::default()?;
        let schema_sql = loader.load_schema(DatabaseType::Chat)?;
        
        self.pool.init_schema(&schema_sql)
    }
    
    /// 创建新会话
    ///
    /// # Arguments
    /// * `name` - 会话名称（可选）
    /// * `agent_id` - 代理ID（可选）
    ///
    /// # Returns
    /// 创建的聊天会话实例
    pub fn create_session(&self, name: Option<String>, agent_id: Option<String>) -> Result<ChatSession> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let session_name = name.unwrap_or_else(|| format!("Chat {}", chrono::Utc::now().format("%Y-%m-%d")));
        
        let session = ChatSession {
            id: id.clone(),
            name: session_name,
            created_at: now,
            updated_at: now,
            agent_id,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO chat_sessions (id, name, created_at, updated_at, agent_id, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session.id,
                    session.name,
                    session.created_at,
                    session.updated_at,
                    session.agent_id,
                    serde_json::to_string(&session.metadata)?,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(session)
    }
    
    /// 获取会话
    ///
    /// # Arguments
    /// * `id` - 会话ID
    ///
    /// # Returns
    /// 会话实例，如果不存在则返回 None
    pub fn get_session(&self, id: &str) -> Result<Option<ChatSession>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, created_at, updated_at, agent_id, metadata
                 FROM chat_sessions WHERE id = ?1",
                params![id],
                |row| parse_session_row(row),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 列出所有会话
    ///
    /// # Arguments
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 会话列表
    pub fn list_sessions(&self, limit: Option<usize>) -> Result<Vec<ChatSession>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT id, name, created_at, updated_at, agent_id, metadata
                 FROM chat_sessions ORDER BY updated_at DESC",
            )?;
            
            let mut rows = stmt.query([])?;
            let mut sessions = Vec::new();
            
            while let Some(row) = rows.next()? {
                sessions.push(parse_session_row(row)?);
            }
            
            if let Some(limit) = limit {
                sessions.truncate(limit);
            }
            
            Ok(sessions)
        })
    }
    
    /// 更新会话
    ///
    /// # Arguments
    /// * `id` - 会话ID
    /// * `name` - 新名称（可选）
    /// * `agent_id` - 新代理ID（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_session(&self, id: &str, name: Option<String>, agent_id: Option<String>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE chat_sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            
            if let Some(agent_id) = agent_id {
                conn.execute(
                    "UPDATE chat_sessions SET agent_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![agent_id, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除会话
    ///
    /// # Arguments
    /// * `id` - 会话ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM chat_sessions WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 添加消息
    ///
    /// # Arguments
    /// * `session_id` - 会话ID
    /// * `role` - 消息角色
    /// * `content` - 消息内容
    /// * `tool_calls` - 工具调用（可选）
    ///
    /// # Returns
    /// 创建的聊天消息实例
    pub fn add_message(&self, session_id: &str, role: &str, content: &str, tool_calls: Option<serde_json::Value>) -> Result<ChatMessage> {
        let id = Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().timestamp_millis();
        
        let message = ChatMessage {
            id: id.clone(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp,
            tool_calls,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO chat_messages (id, session_id, role, content, timestamp, tool_calls, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    message.id,
                    message.session_id,
                    message.role,
                    message.content,
                    message.timestamp,
                    message.tool_calls.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default()),
                    serde_json::to_string(&message.metadata)?,
                ],
            )?;
            
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE chat_sessions SET updated_at = ?1 WHERE id = ?2",
                params![now, session_id],
            )?;
            
            Ok(())
        })?;
        
        Ok(message)
    }
    
    /// 获取会话消息
    ///
    /// # Arguments
    /// * `session_id` - 会话ID
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 消息列表
    pub fn get_messages(&self, session_id: &str, limit: Option<usize>) -> Result<Vec<ChatMessage>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, role, content, timestamp, tool_calls, metadata
                 FROM chat_messages WHERE session_id = ?1 ORDER BY timestamp ASC",
            )?;
            
            let mut rows = stmt.query(params![session_id])?;
            let mut messages = Vec::new();
            
            while let Some(row) = rows.next()? {
                messages.push(parse_message_row(row)?);
            }
            
            if let Some(limit) = limit {
                if messages.len() > limit {
                    messages = messages.split_off(messages.len() - limit);
                }
            }
            
            Ok(messages)
        })
    }
    
    /// 获取消息数量
    ///
    /// # Arguments
    /// * `session_id` - 会话ID
    ///
    /// # Returns
    /// 消息数量
    pub fn get_message_count(&self, session_id: &str) -> Result<usize> {
        self.pool.with_connection(|conn: &Connection| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM chat_messages WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
    }
}

fn parse_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatSession> {
    let metadata_raw: String = row.get(5)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(ChatSession {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        agent_id: row.get(4)?,
        metadata,
    })
}

fn parse_message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatMessage> {
    let tool_calls_raw: Option<String> = row.get(5)?;
    let tool_calls = tool_calls_raw.and_then(|t| serde_json::from_str(&t).ok());
    
    let metadata_raw: String = row.get(6)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(ChatMessage {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        timestamp: row.get(4)?,
        tool_calls,
        metadata,
    })
}
