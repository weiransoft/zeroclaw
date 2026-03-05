//! Agent Group Store - 智能体团队存储
//!
//! 提供智能体团队的数据库持久化存储

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbPool, SqliteConfig};

/// 智能体团队成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGroupMember {
    pub id: String,
    pub agent_id: String,
    pub role: Option<String>,
}

/// 智能体团队
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub agents: Vec<String>,
    pub auto_generate: bool,
    pub team_members: Vec<serde_json::Value>,
}

/// Agent Group Store
///
/// 使用统一的数据库连接池管理智能体团队相关的持久化数据
pub struct AgentGroupStore {
    pool: Arc<DbPool>,
}

impl AgentGroupStore {
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        let config = SqliteConfig::from_workspace(workspace_dir, "agent_groups.db");
        let db_path = config.path.clone();
        let pool = Arc::new(DbPool::new(config.clone())?);
        
        tracing::debug!("[AgentGroupStore] Initializing database at: {:?}", db_path);
        
        let store = Self { pool };
        store.init_schema()?;
        Ok(store)
    }

    pub fn from_pool(pool: Arc<DbPool>) -> Result<Self> {
        let store = Self { pool };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let schema = r#"
            CREATE TABLE IF NOT EXISTS agent_groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                agents TEXT NOT NULL DEFAULT '[]',
                auto_generate INTEGER NOT NULL DEFAULT 0,
                team_members TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_groups_name ON agent_groups(name);
        "#;
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute_batch(schema)?;
            Ok(())
        })?;
        
        Ok(())
    }

    pub fn create_group(&self, name: &str, description: &str, agents: Vec<String>, auto_generate: bool) -> Result<AgentGroup> {
        // 输入验证
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Group name cannot be empty"));
        }
        
        // 检查重复性 - 验证名称是否已存在
        if self.get_group_by_name(name)?.is_some() {
            return Err(anyhow::anyhow!("Agent group with name '{}' already exists", name));
        }
        
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let group = AgentGroup {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            agents,
            auto_generate,
            team_members: vec![],
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO agent_groups (id, name, description, agents, auto_generate, team_members, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    group.id,
                    group.name,
                    group.description,
                    serde_json::to_string(&group.agents)?,
                    group.auto_generate as i32,
                    serde_json::to_string(&group.team_members)?,
                    now,
                    now,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(group)
    }

    pub fn get_group(&self, id: &str) -> Result<Option<AgentGroup>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, agents, auto_generate, team_members FROM agent_groups WHERE id = ?1",
                params![id],
                |row| {
                    let agents_str: String = row.get(3)?;
                    let team_members_str: String = row.get(5)?;
                    Ok(AgentGroup {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        agents: serde_json::from_str(&agents_str).unwrap_or_default(),
                        auto_generate: row.get::<_, i32>(4)? != 0,
                        team_members: serde_json::from_str(&team_members_str).unwrap_or_default(),
                    })
                },
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }

    pub fn get_group_by_name(&self, name: &str) -> Result<Option<AgentGroup>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, agents, auto_generate, team_members FROM agent_groups WHERE name = ?1",
                params![name],
                |row| {
                    let agents_str: String = row.get(3)?;
                    let team_members_str: String = row.get(5)?;
                    Ok(AgentGroup {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        agents: serde_json::from_str(&agents_str).unwrap_or_default(),
                        auto_generate: row.get::<_, i32>(4)? != 0,
                        team_members: serde_json::from_str(&team_members_str).unwrap_or_default(),
                    })
                },
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }

    pub fn list_groups(&self) -> Result<Vec<AgentGroup>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, agents, auto_generate, team_members FROM agent_groups ORDER BY name"
            )?;
            
            let rows = stmt.query_map([], |row| {
                let agents_str: String = row.get(3)?;
                let team_members_str: String = row.get(5)?;
                Ok(AgentGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    agents: serde_json::from_str(&agents_str).unwrap_or_default(),
                    auto_generate: row.get::<_, i32>(4)? != 0,
                    team_members: serde_json::from_str(&team_members_str).unwrap_or_default(),
                })
            })?;
            
            let mut groups = Vec::new();
            for group in rows {
                groups.push(group?);
            }
            Ok(groups)
        })
    }

    pub fn update_group(&self, id: &str, name: Option<&str>, description: Option<&str>, agents: Option<Vec<String>>, auto_generate: Option<bool>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE agent_groups SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE agent_groups SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            if let Some(agents) = agents {
                conn.execute(
                    "UPDATE agent_groups SET agents = ?1, updated_at = ?2 WHERE id = ?3",
                    params![serde_json::to_string(&agents)?, now, id],
                )?;
            }
            
            if let Some(auto_generate) = auto_generate {
                conn.execute(
                    "UPDATE agent_groups SET auto_generate = ?1, updated_at = ?2 WHERE id = ?3",
                    params![auto_generate as i32, now, id],
                )?;
            }
            
            Ok(())
        })?;
        
        Ok(())
    }

    pub fn delete_group(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM agent_groups WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}
