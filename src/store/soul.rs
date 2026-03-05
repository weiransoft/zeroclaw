//! 灵魂存储

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

/// 灵魂特质
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulTrait {
    pub id: String,
    pub soul_id: String,
    pub name: String,
    pub value: f64,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub metadata: serde_json::Value,
}

/// 灵魂记忆
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulMemory {
    pub id: String,
    pub soul_id: String,
    pub memory_type: String,
    pub content: String,
    pub importance: f64,
    pub timestamp: i64,
    pub metadata: serde_json::Value,
}

/// 灵魂
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Soul {
    pub id: String,
    pub name: String,
    pub description: String,
    pub personality: String,
    pub traits: Vec<SoulTrait>,
    pub memories: Vec<SoulMemory>,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_active: bool,
    pub metadata: serde_json::Value,
}

/// 灵魂存储
///
/// 使用统一的数据库连接池管理灵魂相关的持久化数据
pub struct SoulStore {
    /// 数据库连接池
    pool: Arc<DbPool>,
}

impl SoulStore {
    /// 创建新的灵魂存储
    ///
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    ///
    /// # Returns
    /// 初始化好的 SoulStore 实例
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        // 使用统一配置创建连接池
        let config = SqliteConfig::from_workspace(workspace_dir, "soul.db");
        let db_path = config.path.clone();
        let pool = Arc::new(DbPool::new(config.clone())?);
        
        tracing::debug!("[SoulStore] Initializing database at: {:?}", db_path);
        
        let store = Self {
            pool,
        };
        
        // 使用 SQL 文件初始化 schema
        store.init_schema()?;
        
        Ok(store)
    }
    
    /// 从现有连接池创建灵魂存储
    ///
    /// # Arguments
    /// * `pool` - 数据库连接池
    ///
    /// # Returns
    /// SoulStore 实例
    pub fn from_pool(pool: Arc<DbPool>) -> Result<Self> {
        let store = Self {
            pool,
        };
        store.init_schema()?;
        Ok(store)
    }
    

    
    /// 初始化数据库 Schema
    ///
    /// 从 SQL 文件加载并执行数据库初始化脚本
    fn init_schema(&self) -> Result<()> {
        let loader = SqlLoader::default()?;
        let schema_sql = loader.load_schema(DatabaseType::Soul)
            .context("Failed to load soul schema")?;
        
        self.pool.init_schema(&schema_sql)
            .context("Failed to initialize soul schema")?;
        
        Ok(())
    }
    
    /// 创建灵魂
    ///
    /// # Arguments
    /// * `name` - 灵魂名称
    /// * `description` - 灵魂描述
    /// * `personality` - 灵魂个性
    ///
    /// # Returns
    /// 创建的灵魂实例
    pub fn create_soul(&self, name: &str, description: &str, personality: &str) -> Result<Soul> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let soul = Soul {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            personality: personality.to_string(),
            traits: vec![],
            memories: vec![],
            created_at: now,
            updated_at: now,
            is_active: false,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO souls (id, name, description, personality, created_at, updated_at, is_active, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    soul.id,
                    soul.name,
                    soul.description,
                    soul.personality,
                    soul.created_at,
                    soul.updated_at,
                    soul.is_active,
                    serde_json::to_string(&soul.metadata)?,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(soul)
    }
    
    /// 获取灵魂
    ///
    /// # Arguments
    /// * `id` - 灵魂ID
    ///
    /// # Returns
    /// 灵魂实例，如果不存在则返回 None
    pub fn get_soul(&self, id: &str) -> Result<Option<Soul>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, personality, created_at, updated_at, is_active, metadata
                 FROM souls WHERE id = ?1",
                params![id],
                |row| parse_soul_row(row, conn),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 获取活跃灵魂
    ///
    /// # Returns
    /// 活跃的灵魂实例，如果不存在则返回 None
    pub fn get_active_soul(&self) -> Result<Option<Soul>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, personality, created_at, updated_at, is_active, metadata
                 FROM souls WHERE is_active = 1 LIMIT 1",
                [],
                |row| parse_soul_row(row, conn),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 列出所有灵魂
    ///
    /// # Arguments
    /// * `limit` - 返回数量限制
    ///
    /// # Returns
    /// 灵魂列表
    pub fn list_souls(&self, limit: Option<usize>) -> Result<Vec<Soul>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, personality, created_at, updated_at, is_active, metadata
                 FROM souls ORDER BY created_at DESC",
            )?;
            
            let mut rows = stmt.query([])?;
            let mut souls = Vec::new();
            
            while let Some(row) = rows.next()? {
                souls.push(parse_soul_row(row, conn)?);
            }
            
            if let Some(limit) = limit {
                souls.truncate(limit);
            }
            
            Ok(souls)
        })
    }
    
    /// 更新灵魂
    ///
    /// # Arguments
    /// * `id` - 灵魂ID
    /// * `name` - 新名称（可选）
    /// * `description` - 新描述（可选）
    /// * `personality` - 新个性（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_soul(&self, id: &str, name: Option<&str>, description: Option<&str>, personality: Option<&str>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE souls SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE souls SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            if let Some(personality) = personality {
                conn.execute(
                    "UPDATE souls SET personality = ?1, updated_at = ?2 WHERE id = ?3",
                    params![personality, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 激活灵魂
    ///
    /// # Arguments
    /// * `id` - 灵魂ID
    ///
    /// # Returns
    /// 操作结果
    pub fn activate_soul(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("UPDATE souls SET is_active = 0 WHERE is_active = 1", [])?;
            conn.execute(
                "UPDATE souls SET is_active = 1 WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
    }
    
    /// 删除灵魂
    ///
    /// # Arguments
    /// * `id` - 灵魂ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_soul(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM souls WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 添加特质
    ///
    /// # Arguments
    /// * `soul_id` - 灵魂ID
    /// * `name` - 特质名称
    /// * `value` - 特质值
    /// * `description` - 特质描述
    ///
    /// # Returns
    /// 创建的特质实例
    pub fn add_trait(&self, soul_id: &str, name: &str, value: f64, description: &str) -> Result<SoulTrait> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let trait_ = SoulTrait {
            id: id.clone(),
            soul_id: soul_id.to_string(),
            name: name.to_string(),
            value,
            description: description.to_string(),
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO soul_traits (id, soul_id, name, value, description, created_at, updated_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    trait_.id,
                    trait_.soul_id,
                    trait_.name,
                    trait_.value,
                    trait_.description,
                    trait_.created_at,
                    trait_.updated_at,
                    serde_json::to_string(&trait_.metadata)?,
                ],
            )?;
            
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE souls SET updated_at = ?1 WHERE id = ?2",
                params![now, soul_id],
            )?;
            
            Ok(())
        })?;
        
        Ok(trait_)
    }
    
    /// 更新特质
    ///
    /// # Arguments
    /// * `id` - 特质ID
    /// * `value` - 新值（可选）
    /// * `description` - 新描述（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_trait(&self, id: &str, value: Option<f64>, description: Option<&str>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(value) = value {
                conn.execute(
                    "UPDATE soul_traits SET value = ?1, updated_at = ?2 WHERE id = ?3",
                    params![value, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE soul_traits SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            if let Ok(soul_id) = conn.query_row(
                "SELECT soul_id FROM soul_traits WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            ) {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute(
                    "UPDATE souls SET updated_at = ?1 WHERE id = ?2",
                    params![now, soul_id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除特质
    ///
    /// # Arguments
    /// * `id` - 特质ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_trait(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let result = conn.query_row(
                "SELECT soul_id FROM soul_traits WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
            
            conn.execute("DELETE FROM soul_traits WHERE id = ?1", params![id])?;
            
            if let Some(soul_id) = result {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute(
                    "UPDATE souls SET updated_at = ?1 WHERE id = ?2",
                    params![now, soul_id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 添加记忆
    ///
    /// # Arguments
    /// * `soul_id` - 灵魂ID
    /// * `memory_type` - 记忆类型
    /// * `content` - 记忆内容
    /// * `importance` - 记忆重要性
    ///
    /// # Returns
    /// 创建的记忆实例
    pub fn add_memory(&self, soul_id: &str, memory_type: &str, content: &str, importance: f64) -> Result<SoulMemory> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let memory = SoulMemory {
            id: id.clone(),
            soul_id: soul_id.to_string(),
            memory_type: memory_type.to_string(),
            content: content.to_string(),
            importance,
            timestamp: now,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO soul_memories (id, soul_id, memory_type, content, importance, timestamp, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    memory.id,
                    memory.soul_id,
                    memory.memory_type,
                    memory.content,
                    memory.importance,
                    memory.timestamp,
                    serde_json::to_string(&memory.metadata)?,
                ],
            )?;
            
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE souls SET updated_at = ?1 WHERE id = ?2",
                params![now, soul_id],
            )?;
            
            Ok(())
        })?;
        
        Ok(memory)
    }
    
    /// 获取记忆
    ///
    /// # Arguments
    /// * `soul_id` - 灵魂ID
    /// * `memory_type` - 记忆类型（可选）
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 记忆列表
    pub fn get_memories(&self, soul_id: &str, memory_type: Option<&str>, limit: Option<usize>) -> Result<Vec<SoulMemory>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut memories = Vec::new();
            
            if let Some(memory_type_val) = memory_type {
                let mut stmt = conn.prepare(
                    "SELECT id, soul_id, memory_type, content, importance, timestamp, metadata
                     FROM soul_memories WHERE soul_id = ?1 AND memory_type = ?2 ORDER BY importance DESC, timestamp DESC",
                )?;
                
                let mut rows = stmt.query(params![soul_id, memory_type_val])?;
                while let Some(row) = rows.next()? {
                    memories.push(parse_memory_row(row)?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, soul_id, memory_type, content, importance, timestamp, metadata
                     FROM soul_memories WHERE soul_id = ?1 ORDER BY importance DESC, timestamp DESC",
                )?;
                
                let mut rows = stmt.query(params![soul_id])?;
                while let Some(row) = rows.next()? {
                    memories.push(parse_memory_row(row)?);
                }
            }
            
            if let Some(limit) = limit {
                memories.truncate(limit);
            }
            
            Ok(memories)
        })
    }
    
    /// 删除记忆
    ///
    /// # Arguments
    /// * `id` - 记忆ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_memory(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let result = conn.query_row(
                "SELECT soul_id FROM soul_memories WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
            
            conn.execute("DELETE FROM soul_memories WHERE id = ?1", params![id])?;
            
            if let Some(soul_id) = result {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute(
                    "UPDATE souls SET updated_at = ?1 WHERE id = ?2",
                    params![now, soul_id],
                )?;
            }
            
            Ok(())
        })
    }
}

fn parse_soul_row(row: &rusqlite::Row<'_>, conn: &Connection) -> rusqlite::Result<Soul> {
    let id: String = row.get(0)?;
    
    let mut traits_stmt = conn.prepare(
        "SELECT id, soul_id, name, value, description, created_at, updated_at, metadata
         FROM soul_traits WHERE soul_id = ?1",
    )?;
    
    let mut traits = Vec::new();
    let mut trait_rows = traits_stmt.query(params![id])?;
    
    while let Some(trait_row) = trait_rows.next()? {
        traits.push(parse_trait_row(trait_row)?);
    }
    
    let mut memories_stmt = conn.prepare(
        "SELECT id, soul_id, memory_type, content, importance, timestamp, metadata
         FROM soul_memories WHERE soul_id = ?1 ORDER BY importance DESC, timestamp DESC",
    )?;
    
    let mut memories = Vec::new();
    let mut memory_rows = memories_stmt.query(params![id])?;
    
    while let Some(memory_row) = memory_rows.next()? {
        memories.push(parse_memory_row(memory_row)?);
    }
    
    let metadata_raw: String = row.get(7)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(Soul {
        id,
        name: row.get(1)?,
        description: row.get(2)?,
        personality: row.get(3)?,
        traits,
        memories,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        is_active: row.get(6)?,
        metadata,
    })
}

fn parse_trait_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SoulTrait> {
    let metadata_raw: String = row.get(7)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(SoulTrait {
        id: row.get(0)?,
        soul_id: row.get(1)?,
        name: row.get(2)?,
        value: row.get(3)?,
        description: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        metadata,
    })
}

fn parse_memory_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SoulMemory> {
    let metadata_raw: String = row.get(6)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(SoulMemory {
        id: row.get(0)?,
        soul_id: row.get(1)?,
        memory_type: row.get(2)?,
        content: row.get(3)?,
        importance: row.get(4)?,
        timestamp: row.get(5)?,
        metadata,
    })
}
