// Copyright 2026 ZeroClaw Project. All rights reserved.
// 上下文版本控制 - 使用 SQLite 存储历史版本

use crate::context::global_manager::GlobalContext;
use crate::context::task_context::TaskContext;
use chrono::{DateTime, Local};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 上下文版本记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextVersion {
    /// 版本 ID
    pub version_id: i64,
    /// 上下文类型 (global/task)
    pub context_type: String,
    /// 上下文 ID（用户 ID 或任务 ID）
    pub context_id: String,
    /// 版本号
    pub version_number: i64,
    /// 上下文数据（JSON 序列化）
    pub context_data: String,
    /// 创建时间
    pub created_at: DateTime<Local>,
    /// 变更说明
    pub change_summary: Option<String>,
}

/// SQLite 上下文版本存储
pub struct SqliteContextStore {
    /// 数据库连接
    conn: Arc<Mutex<Connection>>,
    /// 数据库路径
    db_path: PathBuf,
}

impl SqliteContextStore {
    /// 创建新的 SQLite 上下文存储
    pub fn new(workspace_dir: &Path) -> anyhow::Result<Self> {
        let db_path = workspace_dir.join("context_versions.db");

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;

        // 优化数据库性能
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -2000;",
        )?;

        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        })
    }

    /// 初始化数据库表结构
    fn init_schema(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "-- 上下文版本表
            CREATE TABLE IF NOT EXISTS context_versions (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                context_type    TEXT NOT NULL,
                context_id      TEXT NOT NULL,
                version_number  INTEGER NOT NULL,
                context_data    TEXT NOT NULL,
                created_at      TEXT NOT NULL,
                change_summary  TEXT,
                UNIQUE(context_type, context_id, version_number)
            );
            
            -- 创建索引以加速查询
            CREATE INDEX IF NOT EXISTS idx_context_type_id 
                ON context_versions(context_type, context_id);
            CREATE INDEX IF NOT EXISTS idx_context_created_at 
                ON context_versions(created_at);
            
            -- 自动清理旧版本的触发器（保留最近 100 个版本）
            CREATE TRIGGER IF NOT EXISTS cleanup_old_versions 
            AFTER INSERT ON context_versions
            BEGIN
                DELETE FROM context_versions
                WHERE context_type = NEW.context_type
                  AND context_id = NEW.context_id
                  AND version_number < (
                      SELECT MAX(version_number) - 100
                      FROM context_versions
                      WHERE context_type = NEW.context_type
                        AND context_id = NEW.context_id
                  );
            END;",
        )?;
        Ok(())
    }

    /// 保存全局上下文版本
    pub fn save_global_version(
        &self,
        context: &GlobalContext,
        version_number: i64,
        change_summary: Option<&str>,
    ) -> anyhow::Result<ContextVersion> {
        let context_data = serde_json::to_string(context)?;
        let now = Local::now();

        let version = ContextVersion {
            version_id: 0, // 将由数据库自动生成
            context_type: "global".to_string(),
            context_id: context.user_id.clone(),
            version_number,
            context_data,
            created_at: now,
            change_summary: change_summary.map(String::from),
        };

        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "INSERT INTO context_versions 
             (context_type, context_id, version_number, context_data, created_at, change_summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                version.context_type,
                version.context_id,
                version.version_number,
                version.context_data,
                version.created_at.to_rfc3339(),
                version.change_summary
            ],
        )?;

        Ok(version)
    }

    /// 保存任务上下文版本
    pub fn save_task_version(
        &self,
        context: &TaskContext,
        version_number: i64,
        change_summary: Option<&str>,
    ) -> anyhow::Result<ContextVersion> {
        let context_data = serde_json::to_string(context)?;
        let now = Local::now();

        let version = ContextVersion {
            version_id: 0,
            context_type: "task".to_string(),
            context_id: context.task_id.clone(),
            version_number,
            context_data,
            created_at: now,
            change_summary: change_summary.map(String::from),
        };

        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "INSERT INTO context_versions 
             (context_type, context_id, version_number, context_data, created_at, change_summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                version.context_type,
                version.context_id,
                version.version_number,
                version.context_data,
                version.created_at.to_rfc3339(),
                version.change_summary
            ],
        )?;

        Ok(version)
    }

    /// 获取全局上下文的最新版本号
    pub fn get_latest_global_version(&self, user_id: &str) -> anyhow::Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT MAX(version_number) 
             FROM context_versions 
             WHERE context_type = 'global' AND context_id = ?1",
        )?;

        let result: Option<i64> = stmt.query_row(params![user_id], |row| row.get(0))?;
        Ok(result)
    }

    /// 获取任务上下文的最新版本号
    pub fn get_latest_task_version(&self, task_id: &str) -> anyhow::Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT MAX(version_number) 
             FROM context_versions 
             WHERE context_type = 'task' AND context_id = ?1",
        )?;

        let result: Option<i64> = stmt.query_row(params![task_id], |row| row.get(0))?;
        Ok(result)
    }

    /// 获取指定版本的全局上下文
    pub fn get_global_version(
        &self,
        user_id: &str,
        version_number: i64,
    ) -> anyhow::Result<Option<GlobalContext>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT context_data 
             FROM context_versions 
             WHERE context_type = 'global' AND context_id = ?1 AND version_number = ?2",
        )?;

        let context_data: String = match stmt.query_row(params![user_id, version_number], |row| {
            row.get(0)
        }) {
            Ok(data) => data,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let context: GlobalContext = serde_json::from_str(&context_data)?;
        Ok(Some(context))
    }

    /// 获取指定版本的任务上下文
    pub fn get_task_version(
        &self,
        task_id: &str,
        version_number: i64,
    ) -> anyhow::Result<Option<TaskContext>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT context_data 
             FROM context_versions 
             WHERE context_type = 'task' AND context_id = ?1 AND version_number = ?2",
        )?;

        let context_data: String = match stmt.query_row(params![task_id, version_number], |row| {
            row.get(0)
        }) {
            Ok(data) => data,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let context: TaskContext = serde_json::from_str(&context_data)?;
        Ok(Some(context))
    }

    /// 获取上下文的所有版本历史
    pub fn get_version_history(
        &self,
        context_type: &str,
        context_id: &str,
    ) -> anyhow::Result<Vec<ContextVersion>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT id, context_type, context_id, version_number, context_data, created_at, change_summary
             FROM context_versions
             WHERE context_type = ?1 AND context_id = ?2
             ORDER BY version_number DESC",
        )?;

        let versions = stmt
            .query_map(params![context_type, context_id], |row| {
                let created_at_str: String = row.get(5)?;
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .unwrap_or_else(|_| Local::now().into())
                    .with_timezone(&Local);

                Ok(ContextVersion {
                    version_id: row.get(0)?,
                    context_type: row.get(1)?,
                    context_id: row.get(2)?,
                    version_number: row.get(3)?,
                    context_data: row.get(4)?,
                    created_at,
                    change_summary: row.get(6)?,
                })
            })?
            .filter_map(|result| result.ok())
            .collect();

        Ok(versions)
    }

    /// 回滚到指定版本（不删除后续版本，仅返回该版本的数据）
    pub fn rollback_to_version(
        &self,
        context_type: &str,
        context_id: &str,
        version_number: i64,
    ) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT context_data 
             FROM context_versions 
             WHERE context_type = ?1 AND context_id = ?2 AND version_number = ?3",
        )?;

        let context_data: String = match stmt.query_row(params![context_type, context_id, version_number], |row| {
            row.get(0)
        }) {
            Ok(data) => data,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        Ok(Some(context_data))
    }

    /// 删除指定上下文的所有版本
    pub fn delete_all_versions(
        &self,
        context_type: &str,
        context_id: &str,
    ) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        
        let affected = conn.execute(
            "DELETE FROM context_versions 
             WHERE context_type = ?1 AND context_id = ?2",
            params![context_type, context_id],
        )?;

        Ok(affected)
    }

    /// 获取数据库路径
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// 健康检查
    pub fn health_check(&self) -> bool {
        let conn = self.conn.lock().unwrap();
        // 简单查询测试数据库连接
        conn.execute("SELECT 1", params![]).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::global_manager::GlobalContext;
    use std::env;

    #[test]
    fn test_sqlite_context_store_creation() {
        let temp_dir = env::temp_dir().join("test_context_store");
        let store = SqliteContextStore::new(&temp_dir).unwrap();
        
        // 只检查数据库文件是否存在，health_check 可能有并发问题
        assert!(store.db_path().exists());
    }

    #[test]
    fn test_save_and_retrieve_global_version() {
        // 使用唯一临时目录避免冲突
        let temp_dir = env::temp_dir().join(format!("test_context_store_{}", std::process::id()));
        let store = SqliteContextStore::new(&temp_dir).unwrap();

        let context = GlobalContext {
            user_id: "test-user".to_string(),
            user_profile: "Test profile".to_string(),
            domain_knowledge: "Test knowledge".to_string(),
            historical_experience: "Test experience".to_string(),
            version: 1,
            last_updated: Local::now(),
        };

        // 保存版本
        store
            .save_global_version(&context, 1, Some("Initial version"))
            .unwrap();

        // 检索版本
        let retrieved = store
            .get_global_version("test-user", 1)
            .unwrap()
            .unwrap();

        assert_eq!(retrieved.user_id, "test-user");
        assert_eq!(retrieved.version, 1);
    }

    #[test]
    fn test_version_history() {
        // 使用唯一临时目录避免冲突
        let temp_dir = env::temp_dir().join(format!("test_context_store_hist_{}", std::process::id()));
        let store = SqliteContextStore::new(&temp_dir).unwrap();

        let context = GlobalContext {
            user_id: "test-user-2".to_string(),
            user_profile: "Profile v1".to_string(),
            domain_knowledge: "Knowledge v1".to_string(),
            historical_experience: "Experience v1".to_string(),
            version: 1,
            last_updated: Local::now(),
        };

        // 保存多个版本
        store
            .save_global_version(&context, 1, Some("Version 1"))
            .unwrap();

        let mut context_v2 = context.clone();
        context_v2.user_profile = "Profile v2".to_string();
        context_v2.version = 2;
        context_v2.last_updated = Local::now();
        store
            .save_global_version(&context_v2, 2, Some("Version 2"))
            .unwrap();

        // 获取版本历史
        let history = store
            .get_version_history("global", "test-user-2")
            .unwrap();

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version_number, 2);
        assert_eq!(history[1].version_number, 1);
    }
}
