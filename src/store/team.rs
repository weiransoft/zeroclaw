//! 团队存储

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

/// 团队成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub id: String,
    pub team_id: String,
    pub user_id: String,
    pub role: String,
    pub joined_at: i64,
    pub left_at: Option<i64>,
    pub is_active: bool,
    pub metadata: serde_json::Value,
}

/// 团队
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_id: String,
    pub members: Vec<TeamMember>,
    pub created_at: i64,
    pub updated_at: i64,
    pub metadata: serde_json::Value,
}

/// 团队存储
///
/// 使用统一的数据库连接池管理团队相关的持久化数据
pub struct TeamStore {
    /// 数据库连接池
    pool: Arc<DbPool>,
}

impl TeamStore {
    /// 创建新的团队存储
    ///
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    ///
    /// # Returns
    /// 初始化好的 TeamStore 实例
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        // 使用统一配置创建连接池
        let config = SqliteConfig::from_workspace(workspace_dir, "team.db");
        let db_path = config.path.clone();
        let pool = Arc::new(DbPool::new(config.clone())?);
        
        tracing::debug!("[TeamStore] Initializing database at: {:?}", db_path);
        
        let store = Self {
            pool,
        };
        
        // 使用 SQL 文件初始化 schema
        store.init_schema()?;
        
        Ok(store)
    }
    
    /// 从现有连接池创建团队存储
    ///
    /// # Arguments
    /// * `pool` - 数据库连接池
    ///
    /// # Returns
    /// TeamStore 实例
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
        let schema_sql = loader.load_schema(DatabaseType::Team)
            .context("Failed to load team schema")?;
        
        self.pool.init_schema(&schema_sql)
            .context("Failed to initialize team schema")?;
        
        Ok(())
    }
    
    /// 创建团队
    ///
    /// # Arguments
    /// * `name` - 团队名称
    /// * `description` - 团队描述
    /// * `owner_id` - 所有者ID
    ///
    /// # Returns
    /// 创建的团队实例
    pub fn create_team(&self, name: &str, description: &str, owner_id: &str) -> Result<Team> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let team = Team {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            owner_id: owner_id.to_string(),
            members: vec![],
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO teams (id, name, description, owner_id, created_at, updated_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    team.id,
                    team.name,
                    team.description,
                    team.owner_id,
                    team.created_at,
                    team.updated_at,
                    serde_json::to_string(&team.metadata)?,
                ],
            )?;
            
            let owner_member_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO team_members (id, team_id, user_id, role, joined_at, is_active, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    owner_member_id,
                    team.id,
                    team.owner_id,
                    "owner",
                    now,
                    true,
                    "{}",
                ],
            )?;
            
            Ok(())
        })?;
        
        Ok(team)
    }
    
    /// 获取团队
    ///
    /// # Arguments
    /// * `id` - 团队ID
    ///
    /// # Returns
    /// 团队实例，如果不存在则返回 None
    pub fn get_team(&self, id: &str) -> Result<Option<Team>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, owner_id, created_at, updated_at, metadata
                 FROM teams WHERE id = ?1",
                params![id],
                |row| parse_team_row(row, conn),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 列出用户的团队
    ///
    /// # Arguments
    /// * `user_id` - 用户ID
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 用户所属的团队列表
    pub fn list_teams_by_user(&self, user_id: &str, limit: Option<usize>) -> Result<Vec<Team>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT t.id, t.name, t.description, t.owner_id, t.created_at, t.updated_at, t.metadata
                 FROM teams t
                 INNER JOIN team_members tm ON t.id = tm.team_id
                 WHERE tm.user_id = ?1 AND tm.is_active = 1
                 ORDER BY t.updated_at DESC",
            )?;
            
            let mut rows = stmt.query(params![user_id])?;
            let mut teams = Vec::new();
            
            while let Some(row) = rows.next()? {
                teams.push(parse_team_row(row, conn)?);
            }
            
            if let Some(limit) = limit {
                teams.truncate(limit);
            }
            
            Ok(teams)
        })
    }
    
    /// 更新团队
    ///
    /// # Arguments
    /// * `id` - 团队ID
    /// * `name` - 新名称（可选）
    /// * `description` - 新描述（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_team(&self, id: &str, name: Option<&str>, description: Option<&str>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE teams SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE teams SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除团队
    ///
    /// # Arguments
    /// * `id` - 团队ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_team(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM teams WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 添加成员
    ///
    /// # Arguments
    /// * `team_id` - 团队ID
    /// * `user_id` - 用户ID
    /// * `role` - 成员角色
    ///
    /// # Returns
    /// 创建的团队成员实例
    pub fn add_member(&self, team_id: &str, user_id: &str, role: &str) -> Result<TeamMember> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let member = TeamMember {
            id: id.clone(),
            team_id: team_id.to_string(),
            user_id: user_id.to_string(),
            role: role.to_string(),
            joined_at: now,
            left_at: None,
            is_active: true,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO team_members (id, team_id, user_id, role, joined_at, is_active, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    member.id,
                    member.team_id,
                    member.user_id,
                    member.role,
                    member.joined_at,
                    member.is_active,
                    serde_json::to_string(&member.metadata)?,
                ],
            )?;
            
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE teams SET updated_at = ?1 WHERE id = ?2",
                params![now, team_id],
            )?;
            
            Ok(())
        })?;
        
        Ok(member)
    }
    
    /// 更新成员
    ///
    /// # Arguments
    /// * `id` - 成员ID
    /// * `role` - 新角色（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_member(&self, id: &str, role: Option<&str>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            if let Some(role) = role {
                conn.execute(
                    "UPDATE team_members SET role = ?1 WHERE id = ?2",
                    params![role, id],
                )?;
            }
            
            if let Ok(team_id) = conn.query_row(
                "SELECT team_id FROM team_members WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            ) {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute(
                    "UPDATE teams SET updated_at = ?1 WHERE id = ?2",
                    params![now, team_id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 移除成员
    ///
    /// # Arguments
    /// * `id` - 成员ID
    ///
    /// # Returns
    /// 操作结果
    pub fn remove_member(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            let result = conn.query_row(
                "SELECT team_id FROM team_members WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
            
            conn.execute(
                "UPDATE team_members SET is_active = 0, left_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            
            if let Some(team_id) = result {
                conn.execute(
                    "UPDATE teams SET updated_at = ?1 WHERE id = ?2",
                    params![now, team_id],
                )?;
            }
            
            Ok(())
        })
    }
}

fn parse_team_row(row: &rusqlite::Row<'_>, conn: &Connection) -> rusqlite::Result<Team> {
    let id: String = row.get(0)?;
    
    let mut stmt = conn.prepare(
        "SELECT id, team_id, user_id, role, joined_at, left_at, is_active, metadata
         FROM team_members WHERE team_id = ?1 AND is_active = 1",
    )?;
    
    let mut members = Vec::new();
    let mut member_rows = stmt.query(params![id])?;
    
    while let Some(member_row) = member_rows.next()? {
        members.push(parse_member_row(member_row)?);
    }
    
    let metadata_raw: String = row.get(6)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(Team {
        id,
        name: row.get(1)?,
        description: row.get(2)?,
        owner_id: row.get(3)?,
        members,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        metadata,
    })
}

fn parse_member_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TeamMember> {
    let metadata_raw: String = row.get(7)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(TeamMember {
        id: row.get(0)?,
        team_id: row.get(1)?,
        user_id: row.get(2)?,
        role: row.get(3)?,
        joined_at: row.get(4)?,
        left_at: row.get(5)?,
        is_active: row.get(6)?,
        metadata,
    })
}
