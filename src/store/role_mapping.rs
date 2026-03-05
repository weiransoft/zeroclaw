//! Role Mapping Store - 角色映射存储
//!
//! 提供角色到智能体映射的数据库持久化存储

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

use crate::db::{DbPool, SqliteConfig};

/// 角色映射
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleMapping {
    pub role: String,
    pub agent_name: String,
    pub agent_config: serde_json::Value,
}

/// Role Mapping Store
///
/// 使用统一的数据库连接池管理角色映射相关的持久化数据
pub struct RoleMappingStore {
    pool: Arc<DbPool>,
}

impl RoleMappingStore {
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        let config = SqliteConfig::from_workspace(workspace_dir, "role_mappings.db");
        let db_path = config.path.clone();
        let pool = Arc::new(DbPool::new(config.clone())?);
        
        tracing::debug!("[RoleMappingStore] Initializing database at: {:?}", db_path);
        
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
            CREATE TABLE IF NOT EXISTS role_mappings (
                role TEXT PRIMARY KEY,
                agent_name TEXT NOT NULL,
                agent_config TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_role_mappings_agent ON role_mappings(agent_name);
        "#;
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute_batch(schema)?;
            Ok(())
        })?;
        
        Ok(())
    }

    pub fn create_mapping(&self, role: &str, agent_name: &str, agent_config: serde_json::Value) -> Result<RoleMapping> {
        // 输入验证
        if role.trim().is_empty() {
            return Err(anyhow::anyhow!("Role cannot be empty"));
        }
        
        if agent_name.trim().is_empty() {
            return Err(anyhow::anyhow!("Agent name cannot be empty"));
        }
        
        // 检查重复性 - 验证角色是否已存在
        if self.get_mapping(role)?.is_some() {
            return Err(anyhow::anyhow!("Role mapping for role '{}' already exists", role));
        }
        
        let now = chrono::Utc::now().timestamp_millis();
        
        let mapping = RoleMapping {
            role: role.to_string(),
            agent_name: agent_name.to_string(),
            agent_config,
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO role_mappings (role, agent_name, agent_config, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    mapping.role,
                    mapping.agent_name,
                    serde_json::to_string(&mapping.agent_config)?,
                    now,
                    now,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(mapping)
    }

    pub fn get_mapping(&self, role: &str) -> Result<Option<RoleMapping>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT role, agent_name, agent_config FROM role_mappings WHERE role = ?1",
                params![role],
                |row| {
                    let agent_config_str: String = row.get(2)?;
                    Ok(RoleMapping {
                        role: row.get(0)?,
                        agent_name: row.get(1)?,
                        agent_config: serde_json::from_str(&agent_config_str).unwrap_or_default(),
                    })
                },
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }

    pub fn list_mappings(&self) -> Result<Vec<RoleMapping>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT role, agent_name, agent_config FROM role_mappings ORDER BY role"
            )?;
            
            let rows = stmt.query_map([], |row| {
                let agent_config_str: String = row.get(2)?;
                Ok(RoleMapping {
                    role: row.get(0)?,
                    agent_name: row.get(1)?,
                    agent_config: serde_json::from_str(&agent_config_str).unwrap_or_default(),
                })
            })?;
            
            let mut mappings = Vec::new();
            for mapping in rows {
                mappings.push(mapping?);
            }
            Ok(mappings)
        })
    }

    pub fn update_mapping(&self, role: &str, agent_name: Option<&str>, agent_config: Option<serde_json::Value>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(agent_name) = agent_name {
                conn.execute(
                    "UPDATE role_mappings SET agent_name = ?1, updated_at = ?2 WHERE role = ?3",
                    params![agent_name, now, role],
                )?;
            }
            
            if let Some(agent_config) = agent_config {
                conn.execute(
                    "UPDATE role_mappings SET agent_config = ?1, updated_at = ?2 WHERE role = ?3",
                    params![serde_json::to_string(&agent_config)?, now, role],
                )?;
            }
            
            Ok(())
        })?;
        
        Ok(())
    }

    pub fn delete_mapping(&self, role: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM role_mappings WHERE role = ?1", params![role])?;
            Ok(())
        })
    }

    pub fn upsert_mapping(&self, role: &str, agent_name: &str, agent_config: serde_json::Value) -> Result<RoleMapping> {
        let existing = self.get_mapping(role)?;
        
        if existing.is_some() {
            self.update_mapping(role, Some(agent_name), Some(agent_config.clone()))?;
            Ok(RoleMapping {
                role: role.to_string(),
                agent_name: agent_name.to_string(),
                agent_config,
            })
        } else {
            self.create_mapping(role, agent_name, agent_config)
        }
    }
}
