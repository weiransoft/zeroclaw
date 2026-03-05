use anyhow::{Result, Context};
use rusqlite::{params, Row};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerRecord {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub disabled: bool,
    pub status: MCPServerStatus,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_started_at: Option<DateTime<Utc>>,
    pub tools_count: u32,
    pub resources_count: u32,
    pub prompts_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MCPServerStatus {
    NotStarted,
    Running,
    Stopped,
    Error,
}

impl Default for MCPServerStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerCreateRequest {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MCPServerUpdateRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub disabled: Option<bool>,
}

pub struct MCPServerStore {
    pool: Arc<Pool<SqliteConnectionManager>>,
}

impl MCPServerStore {
    pub fn new(db_path: PathBuf) -> Self {
        let manager = SqliteConnectionManager::file(&db_path);
        let pool = Pool::builder()
            .max_size(5)
            .build(manager)
            .expect("Failed to create database pool");
        
        Self { pool: Arc::new(pool) }
    }

    pub fn init(&self) -> Result<()> {
        let conn = self.get_connection()?;
        
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                command TEXT NOT NULL,
                args TEXT NOT NULL DEFAULT '[]',
                env TEXT NOT NULL DEFAULT '{}',
                disabled INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'not_started',
                last_error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_started_at TEXT,
                tools_count INTEGER NOT NULL DEFAULT 0,
                resources_count INTEGER NOT NULL DEFAULT 0,
                prompts_count INTEGER NOT NULL DEFAULT 0
            )
            "#,
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_mcp_servers_name ON mcp_servers(name)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_mcp_servers_status ON mcp_servers(status)",
            [],
        )?;

        Ok(())
    }

    fn get_connection(&self) -> Result<PooledConnection<SqliteConnectionManager>> {
        self.pool.get()
            .with_context(|| "Failed to get database connection from pool")
    }

    fn row_to_record(row: &Row) -> rusqlite::Result<MCPServerRecord> {
        let args_str: String = row.get(3)?;
        let env_str: String = row.get(4)?;
        let status_str: String = row.get(6)?;
        
        let status = match status_str.as_str() {
            "not_started" => MCPServerStatus::NotStarted,
            "running" => MCPServerStatus::Running,
            "stopped" => MCPServerStatus::Stopped,
            "error" => MCPServerStatus::Error,
            _ => MCPServerStatus::NotStarted,
        };

        Ok(MCPServerRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            command: row.get(2)?,
            args: serde_json::from_str(&args_str).unwrap_or_default(),
            env: serde_json::from_str(&env_str).unwrap_or_default(),
            disabled: row.get(5)?,
            status,
            last_error: row.get(7)?,
            created_at: row.get::<_, String>(8)?.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: row.get::<_, String>(9)?.parse().unwrap_or_else(|_| Utc::now()),
            last_started_at: row.get::<_, Option<String>>(10)?
                .and_then(|s| s.parse().ok()),
            tools_count: row.get(11)?,
            resources_count: row.get(12)?,
            prompts_count: row.get(13)?,
        })
    }

    pub fn create(&self, request: MCPServerCreateRequest) -> Result<MCPServerRecord> {
        let conn = self.get_connection()?;
        
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let args_json = serde_json::to_string(&request.args)?;
        let env_json = serde_json::to_string(&request.env)?;

        conn.execute(
            r#"
            INSERT INTO mcp_servers (id, name, command, args, env, disabled, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                id,
                request.name,
                request.command,
                args_json,
                env_json,
                request.disabled as i32,
                "not_started",
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )?;

        self.get_by_id(&id)
    }

    pub fn get_by_id(&self, id: &str) -> Result<MCPServerRecord> {
        let conn = self.get_connection()?;
        
        let record = conn.query_row(
            "SELECT id, name, command, args, env, disabled, status, last_error, created_at, updated_at, last_started_at, tools_count, resources_count, prompts_count FROM mcp_servers WHERE id = ?1",
            params![id],
            Self::row_to_record,
        )?;

        Ok(record)
    }

    pub fn get_by_name(&self, name: &str) -> Result<Option<MCPServerRecord>> {
        let conn = self.get_connection()?;
        
        let result = conn.query_row(
            "SELECT id, name, command, args, env, disabled, status, last_error, created_at, updated_at, last_started_at, tools_count, resources_count, prompts_count FROM mcp_servers WHERE name = ?1",
            params![name],
            Self::row_to_record,
        );

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list(&self) -> Result<Vec<MCPServerRecord>> {
        let conn = self.get_connection()?;
        
        let mut stmt = conn.prepare(
            "SELECT id, name, command, args, env, disabled, status, last_error, created_at, updated_at, last_started_at, tools_count, resources_count, prompts_count FROM mcp_servers ORDER BY name"
        )?;

        let records = stmt.query_map([], Self::row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(records)
    }

    pub fn list_enabled(&self) -> Result<Vec<MCPServerRecord>> {
        let conn = self.get_connection()?;
        
        let mut stmt = conn.prepare(
            "SELECT id, name, command, args, env, disabled, status, last_error, created_at, updated_at, last_started_at, tools_count, resources_count, prompts_count FROM mcp_servers WHERE disabled = 0 ORDER BY name"
        )?;

        let records = stmt.query_map([], Self::row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(records)
    }

    pub fn update(&self, id: &str, request: MCPServerUpdateRequest) -> Result<MCPServerRecord> {
        let conn = self.get_connection()?;
        
        let existing = self.get_by_id(id)?;
        let now = Utc::now();

        let name = request.name.unwrap_or(existing.name);
        let command = request.command.unwrap_or(existing.command);
        let args = request.args.unwrap_or(existing.args);
        let env = request.env.unwrap_or(existing.env);
        let disabled = request.disabled.unwrap_or(existing.disabled);

        let args_json = serde_json::to_string(&args)?;
        let env_json = serde_json::to_string(&env)?;

        conn.execute(
            r#"
            UPDATE mcp_servers 
            SET name = ?1, command = ?2, args = ?3, env = ?4, disabled = ?5, updated_at = ?6
            WHERE id = ?7
            "#,
            params![
                name,
                command,
                args_json,
                env_json,
                disabled as i32,
                now.to_rfc3339(),
                id,
            ],
        )?;

        self.get_by_id(id)
    }

    pub fn update_status(&self, id: &str, status: MCPServerStatus, error: Option<&str>) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now();
        let status_str = match status {
            MCPServerStatus::NotStarted => "not_started",
            MCPServerStatus::Running => "running",
            MCPServerStatus::Stopped => "stopped",
            MCPServerStatus::Error => "error",
        };

        conn.execute(
            r#"
            UPDATE mcp_servers 
            SET status = ?1, last_error = ?2, updated_at = ?3, last_started_at = CASE WHEN ?1 = 'running' THEN ?4 ELSE last_started_at END
            WHERE id = ?5
            "#,
            params![
                status_str,
                error,
                now.to_rfc3339(),
                now.to_rfc3339(),
                id,
            ],
        )?;

        Ok(())
    }

    pub fn update_counts(&self, id: &str, tools: u32, resources: u32, prompts: u32) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now();

        conn.execute(
            "UPDATE mcp_servers SET tools_count = ?1, resources_count = ?2, prompts_count = ?3, updated_at = ?4 WHERE id = ?5",
            params![tools, resources, prompts, now.to_rfc3339(), id],
        )?;

        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.get_connection()?;
        
        let affected = conn.execute(
            "DELETE FROM mcp_servers WHERE id = ?1",
            params![id],
        )?;

        Ok(affected > 0)
    }

    pub fn delete_by_name(&self, name: &str) -> Result<bool> {
        let conn = self.get_connection()?;
        
        let affected = conn.execute(
            "DELETE FROM mcp_servers WHERE name = ?1",
            params![name],
        )?;

        Ok(affected > 0)
    }

    pub fn count(&self) -> Result<u32> {
        let conn = self.get_connection()?;
        
        let count: u32 = conn.query_row(
            "SELECT COUNT(*) FROM mcp_servers",
            [],
            |row| row.get(0),
        )?;

        Ok(count)
    }

    pub fn count_by_status(&self, status: MCPServerStatus) -> Result<u32> {
        let conn = self.get_connection()?;
        let status_str = match status {
            MCPServerStatus::NotStarted => "not_started",
            MCPServerStatus::Running => "running",
            MCPServerStatus::Stopped => "stopped",
            MCPServerStatus::Error => "error",
        };
        
        let count: u32 = conn.query_row(
            "SELECT COUNT(*) FROM mcp_servers WHERE status = ?1",
            params![status_str],
            |row| row.get(0),
        )?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_store() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_mcp_servers.db");
        
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }

        let store = MCPServerStore::new(db_path.clone());
        store.init().unwrap();

        let created = store.create(MCPServerCreateRequest {
            name: "test-server".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "test-mcp-server".to_string()],
            env: [("API_KEY".to_string(), "test123".to_string())].into(),
            disabled: false,
        }).unwrap();

        assert_eq!(created.name, "test-server");
        assert_eq!(created.command, "npx");
        assert_eq!(created.args.len(), 2);
        assert!(created.env.contains_key("API_KEY"));

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);

        let updated = store.update(&created.id, MCPServerUpdateRequest {
            disabled: Some(true),
            ..Default::default()
        }).unwrap();
        assert!(updated.disabled);

        store.update_status(&created.id, MCPServerStatus::Running, None).unwrap();
        let running = store.get_by_id(&created.id).unwrap();
        assert_eq!(running.status, MCPServerStatus::Running);

        store.update_counts(&created.id, 5, 3, 2).unwrap();
        let with_counts = store.get_by_id(&created.id).unwrap();
        assert_eq!(with_counts.tools_count, 5);

        let deleted = store.delete(&created.id).unwrap();
        assert!(deleted);

        let empty = store.list().unwrap();
        assert!(empty.is_empty());

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn test_mcp_server_status_serde() {
        let status = MCPServerStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"running\"");
        
        let parsed: MCPServerStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MCPServerStatus::Running);
    }
}
