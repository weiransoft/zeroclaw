use crate::swarm::{RunStatus, SubagentRun};
use crate::swarm::chat::{ChatMessage, ChatMessageType};
use crate::observability::progress::{ProgressEntry, ProgressStatus, TraceEntry, ExportFilter};
use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

// 使用统一的数据库连接池管理系统
use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmEvent {
    pub id: i64,
    pub ts_unix: u64,
    pub run_id: Option<Uuid>,
    pub kind: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmChatMessage {
    pub id: i64,
    pub ts_unix: u64,
    pub run_id: Option<Uuid>,
    pub author: String,
    pub lang: String,
    pub content: String,
    pub meta: serde_json::Value,
}

#[derive(Clone)]
pub struct SwarmSqliteStore {
    db_path: PathBuf,
    pool: Arc<DbPool>,
}

impl SwarmSqliteStore {
    pub fn new(workspace_dir: &Path) -> Self {
        let db_path = workspace_dir.join(".zeroclaw").join("swarm.db");
        
        tracing::debug!("[SwarmStore] Initializing database at: {:?}", db_path);
        
        // 确保数据库目录存在
        if let Some(parent) = db_path.parent() {
            tracing::debug!("[SwarmStore] Creating database directory: {:?}", parent);
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("[SwarmStore] Failed to create database directory: {:?}", e);
            }
        }
        
        // 使用统一的数据库配置
        let config = SqliteConfig {
            path: db_path.clone(),
            max_size: 5, // 最大连接数
            connection_timeout: 5, // 连接超时（秒）
            wal_mode: true,
            foreign_keys: true,
            synchronous: true,
        };
        
        // 初始化统一的连接池
        tracing::debug!("[SwarmStore] Building database connection pool");
        let pool = DbPool::new(config).expect("Failed to create database connection pool");
        tracing::debug!("[SwarmStore] Database connection pool created successfully");
        
        let store = Self {
            db_path,
            pool: Arc::new(pool),
        };
        
        // 初始化数据库 schema
        store.init_schema();
        
        store
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn with_connection<T>(&self, f: impl FnOnce(&Connection) -> anyhow::Result<T>) -> anyhow::Result<T> {
        let start = std::time::Instant::now();
        
        // 使用统一的连接池方法
        let result = self.pool.with_connection(|conn| {
            let pool_wait_ms = start.elapsed().as_millis();
            if pool_wait_ms > 10 {
                tracing::warn!(
                    "[SwarmStore] Long pool wait - wait_ms: {}",
                    pool_wait_ms
                );
            }
            
            let result = f(conn);
            let total_ms = start.elapsed().as_millis();
            if total_ms > 50 {
                tracing::debug!(
                    "[SwarmStore] Slow operation - total_ms: {}",
                    total_ms
                );
            }
            
            result
        });
        
        result
    }

    fn init_schema(&self) {
        // 使用 SQL 加载器加载 swarm 数据库 schema
        match SqlLoader::default() {
            Ok(sql_loader) => {
                match sql_loader.load_schema(DatabaseType::Swarm) {
                    Ok(schema) => {
                        self.with_connection(|conn| {
                            conn.execute_batch(&schema)
                                .context("Failed to initialize swarm schema")?;
                            Ok(())
                        }).unwrap_or_default();
                    }
                    Err(e) => {
                        tracing::error!("[SwarmStore] Failed to load swarm schema: {:?}", e);
                        // 如果加载失败，尝试使用内置的最小 schema
                        self.with_connection(|conn| {
                            conn.execute_batch(
                                "CREATE TABLE IF NOT EXISTS subagent_runs (
                                    run_id              TEXT PRIMARY KEY,
                                    parent_run_id       TEXT,
                                    agent_name          TEXT NOT NULL,
                                    label               TEXT,
                                    task                TEXT NOT NULL,
                                    orchestrator        INTEGER NOT NULL DEFAULT 0,
                                    status              TEXT NOT NULL,
                                    depth               INTEGER NOT NULL,
                                    started_at_unix     INTEGER NOT NULL,
                                    ended_at_unix       INTEGER,
                                    output              TEXT,
                                    error               TEXT,
                                    children_json       TEXT NOT NULL DEFAULT '[]',
                                    cleanup             INTEGER NOT NULL DEFAULT 0,
                                    owner_instance      TEXT NOT NULL,
                                    last_heartbeat_unix INTEGER
                                );
                                CREATE INDEX IF NOT EXISTS idx_subagent_runs_parent ON subagent_runs(parent_run_id);
                                CREATE INDEX IF NOT EXISTS idx_subagent_runs_status ON subagent_runs(status);
                                CREATE INDEX IF NOT EXISTS idx_subagent_runs_heartbeat ON subagent_runs(last_heartbeat_unix);

                                CREATE TABLE IF NOT EXISTS swarm_events (
                                    id      INTEGER PRIMARY KEY AUTOINCREMENT,
                                    ts_unix INTEGER NOT NULL,
                                    run_id  TEXT,
                                    kind    TEXT NOT NULL,
                                    payload TEXT NOT NULL
                                );
                                CREATE INDEX IF NOT EXISTS idx_swarm_events_run ON swarm_events(run_id);

                                CREATE TABLE IF NOT EXISTS swarm_chat (
                                    id      INTEGER PRIMARY KEY AUTOINCREMENT,
                                    ts_unix INTEGER NOT NULL,
                                    run_id  TEXT,
                                    author  TEXT NOT NULL,
                                    lang    TEXT NOT NULL,
                                    content TEXT NOT NULL,
                                    meta    TEXT NOT NULL
                                );
                                CREATE INDEX IF NOT EXISTS idx_swarm_chat_run ON swarm_chat(run_id);

                                CREATE TABLE IF NOT EXISTS swarm_chat_extended (
                                    id          TEXT PRIMARY KEY,
                                    ts_unix     INTEGER NOT NULL,
                                    run_id      TEXT,
                                    task_id     TEXT,
                                    author      TEXT NOT NULL,
                                    author_type TEXT NOT NULL,
                                    message_type TEXT NOT NULL,
                                    lang        TEXT NOT NULL,
                                    content     TEXT NOT NULL,
                                    parent_id   TEXT,
                                    metadata    TEXT NOT NULL DEFAULT '{}'
                                );
                                CREATE INDEX IF NOT EXISTS idx_swarm_chat_extended_run ON swarm_chat_extended(run_id);
                                CREATE INDEX IF NOT EXISTS idx_swarm_chat_extended_task ON swarm_chat_extended(task_id);
                                CREATE INDEX IF NOT EXISTS idx_swarm_chat_extended_parent ON swarm_chat_extended(parent_id);

                                CREATE TABLE IF NOT EXISTS progress_entries (
                                    id              TEXT PRIMARY KEY,
                                    run_id          TEXT,
                                    task_id         TEXT,
                                    status          TEXT NOT NULL,
                                    title           TEXT NOT NULL,
                                    description     TEXT,
                                    progress        REAL NOT NULL DEFAULT 0.0,
                                    total           REAL,
                                    unit            TEXT,
                                    started_at      INTEGER,
                                    updated_at      INTEGER NOT NULL,
                                    completed_at    INTEGER,
                                    error           TEXT,
                                    metadata        TEXT NOT NULL DEFAULT '{}'
                                );
                                CREATE INDEX IF NOT EXISTS idx_progress_run ON progress_entries(run_id);
                                CREATE INDEX IF NOT EXISTS idx_progress_task ON progress_entries(task_id);
                                CREATE INDEX IF NOT EXISTS idx_progress_status ON progress_entries(status);

                                CREATE TABLE IF NOT EXISTS trace_entries (
                                    id          TEXT PRIMARY KEY,
                                    run_id      TEXT,
                                    task_id     TEXT,
                                    parent_id   TEXT,
                                    timestamp   INTEGER NOT NULL,
                                    level       TEXT NOT NULL,
                                    message     TEXT NOT NULL,
                                    lang        TEXT NOT NULL DEFAULT 'en',
                                    metadata    TEXT NOT NULL DEFAULT '{}'
                                );
                                CREATE INDEX IF NOT EXISTS idx_trace_run ON trace_entries(run_id);
                                CREATE INDEX IF NOT EXISTS idx_trace_task ON trace_entries(task_id);
                                CREATE INDEX IF NOT EXISTS idx_trace_parent ON trace_entries(parent_id);
                                CREATE INDEX IF NOT EXISTS idx_trace_timestamp ON trace_entries(timestamp);

                                CREATE TABLE IF NOT EXISTS intelligent_tasks (
                                    id                  TEXT PRIMARY KEY,
                                    title               TEXT NOT NULL,
                                    description         TEXT,
                                    status              TEXT NOT NULL,
                                    priority            TEXT NOT NULL DEFAULT 'medium',
                                    assignee_type       TEXT NOT NULL DEFAULT 'unassigned',
                                    assigned_by         TEXT NOT NULL,
                                    parent_task_id      TEXT,
                                    created_at          INTEGER NOT NULL,
                                    updated_at          INTEGER NOT NULL,
                                    due_date            INTEGER,
                                    progress            REAL NOT NULL DEFAULT 0.0,
                                    metadata            TEXT NOT NULL DEFAULT '{}',
                                    FOREIGN KEY (parent_task_id) REFERENCES intelligent_tasks(id)
                                );
                                CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_status ON intelligent_tasks(status);
                                CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_priority ON intelligent_tasks(priority);
                                CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_assignee ON intelligent_tasks(assignee_type);
                                CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_created ON intelligent_tasks(created_at DESC);

                                CREATE TABLE IF NOT EXISTS task_assignees (
                                    id              TEXT PRIMARY KEY,
                                    task_id         TEXT NOT NULL,
                                    assignee_name   TEXT NOT NULL,
                                    assigned_at     INTEGER NOT NULL,
                                    FOREIGN KEY (task_id) REFERENCES intelligent_tasks(id) ON DELETE CASCADE
                                );
                                CREATE INDEX IF NOT EXISTS idx_task_assignees_task ON task_assignees(task_id);
                                CREATE INDEX IF NOT EXISTS idx_task_assignees_name ON task_assignees(assignee_name);
                            ")
                            .context("Failed to initialize swarm schema with fallback")?;
                            Ok(())
                        }).unwrap_or_default();
                    }
                }
            }
            Err(e) => {
                tracing::error!("[SwarmStore] Failed to create SQL loader: {:?}", e);
                // 如果创建 SQL 加载器失败，尝试使用内置的最小 schema
                self.with_connection(|conn| {
                    conn.execute_batch(
                        "CREATE TABLE IF NOT EXISTS subagent_runs (
                            run_id              TEXT PRIMARY KEY,
                            parent_run_id       TEXT,
                            agent_name          TEXT NOT NULL,
                            label               TEXT,
                            task                TEXT NOT NULL,
                            orchestrator        INTEGER NOT NULL DEFAULT 0,
                            status              TEXT NOT NULL,
                            depth               INTEGER NOT NULL,
                            started_at_unix     INTEGER NOT NULL,
                            ended_at_unix       INTEGER,
                            output              TEXT,
                            error               TEXT,
                            children_json       TEXT NOT NULL DEFAULT '[]',
                            cleanup             INTEGER NOT NULL DEFAULT 0,
                            owner_instance      TEXT NOT NULL,
                            last_heartbeat_unix INTEGER
                        );
                        CREATE INDEX IF NOT EXISTS idx_subagent_runs_parent ON subagent_runs(parent_run_id);
                        CREATE INDEX IF NOT EXISTS idx_subagent_runs_status ON subagent_runs(status);
                        CREATE INDEX IF NOT EXISTS idx_subagent_runs_heartbeat ON subagent_runs(last_heartbeat_unix);

                        CREATE TABLE IF NOT EXISTS swarm_events (
                            id      INTEGER PRIMARY KEY AUTOINCREMENT,
                            ts_unix INTEGER NOT NULL,
                            run_id  TEXT,
                            kind    TEXT NOT NULL,
                            payload TEXT NOT NULL
                        );
                        CREATE INDEX IF NOT EXISTS idx_swarm_events_run ON swarm_events(run_id);

                        CREATE TABLE IF NOT EXISTS swarm_chat (
                            id      INTEGER PRIMARY KEY AUTOINCREMENT,
                            ts_unix INTEGER NOT NULL,
                            run_id  TEXT,
                            author  TEXT NOT NULL,
                            lang    TEXT NOT NULL,
                            content TEXT NOT NULL,
                            meta    TEXT NOT NULL
                        );
                        CREATE INDEX IF NOT EXISTS idx_swarm_chat_run ON swarm_chat(run_id);

                        CREATE TABLE IF NOT EXISTS swarm_chat_extended (
                            id          TEXT PRIMARY KEY,
                            ts_unix     INTEGER NOT NULL,
                            run_id      TEXT,
                            task_id     TEXT,
                            author      TEXT NOT NULL,
                            author_type TEXT NOT NULL,
                            message_type TEXT NOT NULL,
                            lang        TEXT NOT NULL,
                            content     TEXT NOT NULL,
                            parent_id   TEXT,
                            metadata    TEXT NOT NULL DEFAULT '{}'
                        );
                        CREATE INDEX IF NOT EXISTS idx_swarm_chat_extended_run ON swarm_chat_extended(run_id);
                        CREATE INDEX IF NOT EXISTS idx_swarm_chat_extended_task ON swarm_chat_extended(task_id);
                        CREATE INDEX IF NOT EXISTS idx_swarm_chat_extended_parent ON swarm_chat_extended(parent_id);

                        CREATE TABLE IF NOT EXISTS progress_entries (
                            id              TEXT PRIMARY KEY,
                            run_id          TEXT,
                            task_id         TEXT,
                            status          TEXT NOT NULL,
                            title           TEXT NOT NULL,
                            description     TEXT,
                            progress        REAL NOT NULL DEFAULT 0.0,
                            total           REAL,
                            unit            TEXT,
                            started_at      INTEGER,
                            updated_at      INTEGER NOT NULL,
                            completed_at    INTEGER,
                            error           TEXT,
                            metadata        TEXT NOT NULL DEFAULT '{}'
                        );
                        CREATE INDEX IF NOT EXISTS idx_progress_run ON progress_entries(run_id);
                        CREATE INDEX IF NOT EXISTS idx_progress_task ON progress_entries(task_id);
                        CREATE INDEX IF NOT EXISTS idx_progress_status ON progress_entries(status);

                        CREATE TABLE IF NOT EXISTS trace_entries (
                            id          TEXT PRIMARY KEY,
                            run_id      TEXT,
                            task_id     TEXT,
                            parent_id   TEXT,
                            timestamp   INTEGER NOT NULL,
                            level       TEXT NOT NULL,
                            message     TEXT NOT NULL,
                            lang        TEXT NOT NULL DEFAULT 'en',
                            metadata    TEXT NOT NULL DEFAULT '{}'
                        );
                        CREATE INDEX IF NOT EXISTS idx_trace_run ON trace_entries(run_id);
                        CREATE INDEX IF NOT EXISTS idx_trace_task ON trace_entries(task_id);
                        CREATE INDEX IF NOT EXISTS idx_trace_parent ON trace_entries(parent_id);
                        CREATE INDEX IF NOT EXISTS idx_trace_timestamp ON trace_entries(timestamp);

                        CREATE TABLE IF NOT EXISTS intelligent_tasks (
                            id                  TEXT PRIMARY KEY,
                            title               TEXT NOT NULL,
                            description         TEXT,
                            status              TEXT NOT NULL,
                            priority            TEXT NOT NULL DEFAULT 'medium',
                            assignee_type       TEXT NOT NULL DEFAULT 'unassigned',
                            assigned_by         TEXT NOT NULL,
                            parent_task_id      TEXT,
                            created_at          INTEGER NOT NULL,
                            updated_at          INTEGER NOT NULL,
                            due_date            INTEGER,
                            progress            REAL NOT NULL DEFAULT 0.0,
                            metadata            TEXT NOT NULL DEFAULT '{}',
                            FOREIGN KEY (parent_task_id) REFERENCES intelligent_tasks(id)
                        );
                        CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_status ON intelligent_tasks(status);
                        CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_priority ON intelligent_tasks(priority);
                        CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_assignee ON intelligent_tasks(assignee_type);
                        CREATE INDEX IF NOT EXISTS idx_intelligent_tasks_created ON intelligent_tasks(created_at DESC);

                        CREATE TABLE IF NOT EXISTS task_assignees (
                            id              TEXT PRIMARY KEY,
                            task_id         TEXT NOT NULL,
                            assignee_name   TEXT NOT NULL,
                            assigned_at     INTEGER NOT NULL,
                            FOREIGN KEY (task_id) REFERENCES intelligent_tasks(id) ON DELETE CASCADE
                        );
                        CREATE INDEX IF NOT EXISTS idx_task_assignees_task ON task_assignees(task_id);
                        CREATE INDEX IF NOT EXISTS idx_task_assignees_name ON task_assignees(assignee_name);
                    ")
                    .context("Failed to initialize swarm schema with fallback")?;
                    Ok(())
                }).unwrap_or_default();
            }
        }
    }



    pub fn sweep_stale_inflight(&self, stale_after_secs: u64, now_unix: u64) -> anyhow::Result<u64> {
        let cutoff = now_unix.saturating_sub(stale_after_secs) as i64;
        self.with_connection(|conn| {
            let changed = conn.execute(
                "UPDATE subagent_runs
                 SET status = 'terminated',
                     ended_at_unix = ?1,
                     error = COALESCE(error, 'terminated: stale heartbeat')
                 WHERE status IN ('pending','running')
                   AND last_heartbeat_unix IS NOT NULL
                   AND last_heartbeat_unix < ?2",
                params![now_unix as i64, cutoff],
            )?;
            Ok(changed as u64)
        })
    }

    pub fn count_runs(&self) -> anyhow::Result<u64> {
        self.with_connection(|conn| {
            let n: i64 = conn.query_row("SELECT COUNT(1) FROM subagent_runs", [], |row| row.get(0))?;
            Ok(n.max(0) as u64)
        })
    }

    pub fn upsert_run(&self, run: &SubagentRun, owner_instance: &str, last_heartbeat_unix: Option<u64>) -> anyhow::Result<()> {
        let children_json = serde_json::to_string(&run.children).context("Failed to encode children")?;
        let parent_run_id = run.parent_run_id.map(|u| u.to_string());
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO subagent_runs (
                    run_id, parent_run_id, agent_name, label, task, orchestrator, status, depth,
                    started_at_unix, ended_at_unix, output, error, children_json, cleanup, owner_instance, last_heartbeat_unix
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
                 ON CONFLICT(run_id) DO UPDATE SET
                    parent_run_id=excluded.parent_run_id,
                    agent_name=excluded.agent_name,
                    label=excluded.label,
                    task=excluded.task,
                    orchestrator=excluded.orchestrator,
                    status=excluded.status,
                    depth=excluded.depth,
                    started_at_unix=excluded.started_at_unix,
                    ended_at_unix=excluded.ended_at_unix,
                    output=excluded.output,
                    error=excluded.error,
                    children_json=excluded.children_json,
                    cleanup=excluded.cleanup,
                    owner_instance=excluded.owner_instance,
                    last_heartbeat_unix=excluded.last_heartbeat_unix",
                params![
                    run.run_id.to_string(),
                    parent_run_id,
                    run.agent_name,
                    run.label,
                    run.task,
                    if run.orchestrator { 1 } else { 0 },
                    serde_json::to_string(&run.status)?,
                    run.depth as i64,
                    run.started_at_unix as i64,
                    run.ended_at_unix.map(|v| v as i64),
                    run.output,
                    run.error,
                    children_json,
                    if run.cleanup { 1 } else { 0 },
                    owner_instance,
                    last_heartbeat_unix.map(|v| v as i64),
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete_run(&self, run_id: Uuid) -> anyhow::Result<()> {
        self.with_connection(|conn| {
            conn.execute(
                "DELETE FROM subagent_runs WHERE run_id = ?1",
                params![run_id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn update_children(&self, run_id: Uuid, children: &[Uuid]) -> anyhow::Result<()> {
        let children_json = serde_json::to_string(children).context("Failed to encode children")?;
        self.with_connection(|conn| {
            conn.execute(
                "UPDATE subagent_runs SET children_json = ?1 WHERE run_id = ?2",
                params![children_json, run_id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn update_heartbeat(&self, run_id: Uuid, ts_unix: u64) -> anyhow::Result<()> {
        self.with_connection(|conn| {
            conn.execute(
                "UPDATE subagent_runs SET last_heartbeat_unix = ?1 WHERE run_id = ?2",
                params![ts_unix as i64, run_id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn list_runs(&self) -> anyhow::Result<Vec<SubagentRun>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT run_id, parent_run_id, agent_name, label, task, orchestrator, status, depth,
                        started_at_unix, ended_at_unix, output, error, children_json, cleanup
                 FROM subagent_runs
                 ORDER BY started_at_unix ASC",
            )?;
            let mut rows = stmt.query([])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(parse_run_row(row)?);
            }
            Ok(out)
        })
    }

    pub fn get_run(&self, run_id: Uuid) -> anyhow::Result<Option<SubagentRun>> {
        self.with_connection(|conn| {
            conn.query_row(
                "SELECT run_id, parent_run_id, agent_name, label, task, orchestrator, status, depth,
                        started_at_unix, ended_at_unix, output, error, children_json, cleanup
                 FROM subagent_runs
                 WHERE run_id = ?1",
                params![run_id.to_string()],
                |row| parse_run_row(row),
            )
            .optional()
            .map_err(|e| anyhow::anyhow!(e))
        })
    }

    pub fn append_event(
        &self,
        ts_unix: u64,
        run_id: Option<Uuid>,
        kind: &str,
        payload: &serde_json::Value,
    ) -> anyhow::Result<i64> {
        let payload = serde_json::to_string(payload).context("Failed to encode event payload")?;
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO swarm_events(ts_unix, run_id, kind, payload) VALUES (?1, ?2, ?3, ?4)",
                params![
                    ts_unix as i64,
                    run_id.map(|u| u.to_string()),
                    kind,
                    payload
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn list_events(
        &self,
        run_id: Option<Uuid>,
        limit: usize,
    ) -> anyhow::Result<Vec<SwarmEvent>> {
        let limit = limit.clamp(1, 500) as i64;
        self.with_connection(|conn| {
            let mut out = Vec::new();
            if let Some(run_id) = run_id {
                let mut stmt = conn.prepare(
                    "SELECT id, ts_unix, run_id, kind, payload
                     FROM swarm_events
                     WHERE run_id = ?1
                     ORDER BY id DESC
                     LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![run_id.to_string(), limit])?;
                while let Some(row) = rows.next()? {
                    out.push(parse_event_row(row)?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, ts_unix, run_id, kind, payload
                     FROM swarm_events
                     ORDER BY id DESC
                     LIMIT ?1",
                )?;
                let mut rows = stmt.query(params![limit])?;
                while let Some(row) = rows.next()? {
                    out.push(parse_event_row(row)?);
                }
            }
            Ok(out)
        })
    }

    pub fn append_chat(
        &self,
        ts_unix: u64,
        run_id: Option<Uuid>,
        author: &str,
        lang: &str,
        content: &str,
        meta: &serde_json::Value,
    ) -> anyhow::Result<i64> {
        let meta = serde_json::to_string(meta).context("Failed to encode chat meta")?;
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO swarm_chat(ts_unix, run_id, author, lang, content, meta)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    ts_unix as i64,
                    run_id.map(|u| u.to_string()),
                    author,
                    lang,
                    content,
                    meta
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn append_chat_extended(&self, message: &ChatMessage) -> anyhow::Result<i64> {
        let message_type = serde_json::to_string(&message.message_type)
            .context("Failed to encode message type")?;
        let meta = serde_json::to_string(&message.metadata)
            .context("Failed to encode message metadata")?;
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO swarm_chat_extended(id, ts_unix, run_id, task_id, author, author_type, message_type, lang, content, parent_id, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    message.id,
                    message.timestamp as i64,
                    message.run_id.as_deref(),
                    message.task_id.as_deref(),
                    message.author,
                    message.author_type,
                    message_type,
                    message.lang,
                    message.content,
                    message.parent_id.as_deref(),
                    meta,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn list_chat(
        &self,
        run_id: Option<Uuid>,
        limit: usize,
    ) -> anyhow::Result<Vec<SwarmChatMessage>> {
        let limit = limit.clamp(1, 500) as i64;
        self.with_connection(|conn| {
            let mut out = Vec::new();
            if let Some(run_id) = run_id {
                let mut stmt = conn.prepare(
                    "SELECT id, ts_unix, run_id, author, lang, content, meta
                     FROM swarm_chat
                     WHERE run_id = ?1
                     ORDER BY id DESC
                     LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![run_id.to_string(), limit])?;
                while let Some(row) = rows.next()? {
                    out.push(parse_chat_row(row)?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, ts_unix, run_id, author, lang, content, meta
                     FROM swarm_chat
                     ORDER BY id DESC
                     LIMIT ?1",
                )?;
                let mut rows = stmt.query(params![limit])?;
                while let Some(row) = rows.next()? {
                    out.push(parse_chat_row(row)?);
                }
            }
            Ok(out)
        })
    }

    pub fn list_chat_extended(
        &self,
        run_id: Option<Uuid>,
        task_id: Option<String>,
        limit: usize,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let limit = limit.clamp(1, 500) as i64;
        self.with_connection(|conn| {
            let mut conditions = Vec::new();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if run_id.is_some() {
                conditions.push("run_id = ?");
                params.push(Box::new(run_id.unwrap().to_string()));
            }

            if task_id.is_some() {
                conditions.push("task_id = ?");
                params.push(Box::new(task_id.unwrap()));
            }

            let where_clause = if conditions.is_empty() {
                "".to_string()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            let query = format!(
                "SELECT id, ts_unix, run_id, task_id, author, author_type, message_type, lang, content, parent_id, metadata
                 FROM swarm_chat_extended
                 {}
                 ORDER BY ts_unix DESC LIMIT ?",
                where_clause
            );

            params.push(Box::new(limit));

            let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&query)?;
            let mut rows = stmt.query(params_refs.as_slice())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(parse_chat_extended_row(row)?);
            }
            Ok(out)
        })
    }
}

fn parse_chat_extended_row(row: &rusqlite::Row<'_>) -> anyhow::Result<ChatMessage> {
    let id: String = row.get(0).map_err(|e| anyhow::anyhow!(e))?;
    let ts_unix: i64 = row.get(1).map_err(|e| anyhow::anyhow!(e))?;
    let run_id: Option<String> = row.get(2).map_err(|e| anyhow::anyhow!(e))?;
    let task_id: Option<String> = row.get(3).map_err(|e| anyhow::anyhow!(e))?;
    let author: String = row.get(4).map_err(|e| anyhow::anyhow!(e))?;
    let author_type: String = row.get(5).map_err(|e| anyhow::anyhow!(e))?;
    let message_type_raw: String = row.get(6).map_err(|e| anyhow::anyhow!(e))?;
    let lang: String = row.get(7).map_err(|e| anyhow::anyhow!(e))?;
    let content: String = row.get(8).map_err(|e| anyhow::anyhow!(e))?;
    let parent_id: Option<String> = row.get(9).map_err(|e| anyhow::anyhow!(e))?;
    let metadata_raw: String = row.get(10).map_err(|e| anyhow::anyhow!(e))?;

    let message_type: ChatMessageType = serde_json::from_str(&message_type_raw)
        .context("Invalid message type in swarm DB")?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));

    Ok(ChatMessage {
        id,
        run_id,
        task_id,
        author,
        author_type,
        message_type,
        content,
        lang,
        timestamp: ts_unix as u64,
        parent_id,
        metadata,
    })
}

fn parse_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubagentRun> {
    let run_id: String = row.get(0)?;
    let parent_run_id: Option<String> = row.get(1)?;
    let agent_name: String = row.get(2)?;
    let label: Option<String> = row.get(3)?;
    let task: String = row.get(4)?;
    let orchestrator_raw: i64 = row.get(5)?;
    let status_raw: String = row.get(6)?;
    let depth: i64 = row.get(7)?;
    let started_at_unix: i64 = row.get(8)?;
    let ended_at_unix: Option<i64> = row.get(9)?;
    let output: Option<String> = row.get(10)?;
    let error: Option<String> = row.get(11)?;
    let children_json: String = row.get(12)?;
    let cleanup_raw: i64 = row.get(13)?;

    let status: RunStatus = serde_json::from_str(&status_raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let children: Vec<Uuid> = serde_json::from_str(&children_json).unwrap_or_default();

    Ok(SubagentRun {
        run_id: Uuid::parse_str(&run_id).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        parent_run_id: parent_run_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        agent_name,
        label,
        task,
        orchestrator: orchestrator_raw != 0,
        status,
        depth: depth as u32,
        started_at_unix: started_at_unix as u64,
        ended_at_unix: ended_at_unix.map(|v| v as u64),
        output,
        error,
        children,
        cleanup: cleanup_raw != 0,
    })
}

fn parse_event_row(row: &rusqlite::Row<'_>) -> anyhow::Result<SwarmEvent> {
    let id: i64 = row.get(0)?;
    let ts_unix: i64 = row.get(1)?;
    let run_id: Option<String> = row.get(2)?;
    let kind: String = row.get(3)?;
    let payload_raw: String = row.get(4)?;
    let payload: serde_json::Value = serde_json::from_str(&payload_raw).unwrap_or(serde_json::json!({}));
    Ok(SwarmEvent {
        id,
        ts_unix: ts_unix as u64,
        run_id: run_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        kind,
        payload,
    })
}

fn parse_chat_row(row: &rusqlite::Row<'_>) -> anyhow::Result<SwarmChatMessage> {
    let id: i64 = row.get(0)?;
    let ts_unix: i64 = row.get(1)?;
    let run_id: Option<String> = row.get(2)?;
    let author: String = row.get(3)?;
    let lang: String = row.get(4)?;
    let content: String = row.get(5)?;
    let meta_raw: String = row.get(6)?;
    let meta: serde_json::Value = serde_json::from_str(&meta_raw).unwrap_or(serde_json::json!({}));
    Ok(SwarmChatMessage {
        id,
        ts_unix: ts_unix as u64,
        run_id: run_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        author,
        lang,
        content,
        meta,
    })
}

impl SwarmSqliteStore {
    pub fn upsert_progress(&self, entry: &ProgressEntry) -> anyhow::Result<()> {
        let metadata = serde_json::to_string(&entry.metadata).context("Failed to encode progress metadata")?;
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO progress_entries (
                    id, run_id, task_id, status, title, description, progress, total, unit,
                    started_at, updated_at, completed_at, error, metadata
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
                 ON CONFLICT(id) DO UPDATE SET
                    run_id=excluded.run_id,
                    task_id=excluded.task_id,
                    status=excluded.status,
                    title=excluded.title,
                    description=excluded.description,
                    progress=excluded.progress,
                    total=excluded.total,
                    unit=excluded.unit,
                    started_at=excluded.started_at,
                    updated_at=excluded.updated_at,
                    completed_at=excluded.completed_at,
                    error=excluded.error,
                    metadata=excluded.metadata",
                params![
                    entry.id,
                    entry.run_id.as_deref(),
                    entry.task_id.as_deref(),
                    serde_json::to_string(&entry.status)?,
                    entry.title,
                    entry.description.as_deref(),
                    entry.progress,
                    entry.total,
                    entry.unit.as_deref(),
                    entry.started_at.map(|v| v as i64),
                    entry.updated_at as i64,
                    entry.completed_at.map(|v| v as i64),
                    entry.error.as_deref(),
                    metadata,
                ],
            )?;
            Ok(())
        })
    }

    pub fn update_progress(
        &self,
        id: &str,
        progress: f64,
        description: Option<&str>,
        metadata: Option<&serde_json::Value>,
        updated_at: Option<u64>,
    ) -> anyhow::Result<()> {
        let metadata_json = if let Some(meta) = metadata {
            Some(serde_json::to_string(meta).context("Failed to encode metadata")?)
        } else {
            None
        };
        let updated_at = updated_at.unwrap_or_else(|| now_unix());
        self.with_connection(|conn| {
            let mut query = "UPDATE progress_entries SET progress = ?1, updated_at = ?2".to_string();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(progress),
                Box::new(updated_at as i64),
            ];
            let mut _param_idx = 3;

            if let Some(desc) = description {
                query.push_str(&format!(", description = ?{_param_idx}"));
                params.push(Box::new(desc.to_string()));
                _param_idx += 1;
            }

            if let Some(meta) = metadata_json {
                query.push_str(&format!(", metadata = ?{_param_idx}"));
                params.push(Box::new(meta));
                _param_idx += 1;
            }

            query.push_str(" WHERE id = ?");
            params.push(Box::new(id.to_string()));

            let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            conn.execute(&query, params_refs.as_slice())?;
            Ok(())
        })
    }

    pub fn update_progress_status(
        &self,
        id: &str,
        status: &ProgressStatus,
        started_at: Option<u64>,
        completed_at: Option<u64>,
    ) -> anyhow::Result<()> {
        let status_json = serde_json::to_string(status)?;
        self.with_connection(|conn| {
            if let (Some(start), Some(end)) = (started_at, completed_at) {
                conn.execute(
                    "UPDATE progress_entries SET status = ?1, started_at = ?2, completed_at = ?3, updated_at = ?4 WHERE id = ?5",
                    params![status_json, start as i64, end as i64, now_unix() as i64, id],
                )?;
            } else if let Some(start) = started_at {
                conn.execute(
                    "UPDATE progress_entries SET status = ?1, started_at = ?2, updated_at = ?3 WHERE id = ?4",
                    params![status_json, start as i64, now_unix() as i64, id],
                )?;
            } else if let Some(end) = completed_at {
                conn.execute(
                    "UPDATE progress_entries SET status = ?1, completed_at = ?2, updated_at = ?3 WHERE id = ?4",
                    params![status_json, end as i64, now_unix() as i64, id],
                )?;
            } else {
                conn.execute(
                    "UPDATE progress_entries SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![status_json, now_unix() as i64, id],
                )?;
            }
            Ok(())
        })
    }

    pub fn update_progress_error(&self, id: &str, error: &str, updated_at: Option<u64>) -> anyhow::Result<()> {
        let updated_at = updated_at.unwrap_or_else(|| now_unix());
        self.with_connection(|conn| {
            conn.execute(
                "UPDATE progress_entries SET error = ?1, updated_at = ?2 WHERE id = ?3",
                params![error, updated_at as i64, id],
            )?;
            Ok(())
        })
    }

    pub fn get_progress(&self, id: &str) -> anyhow::Result<Option<ProgressEntry>> {
        self.with_connection(|conn| {
            conn.query_row(
                "SELECT id, run_id, task_id, status, title, description, progress, total, unit,
                        started_at, updated_at, completed_at, error, metadata
                 FROM progress_entries
                 WHERE id = ?1",
                params![id],
                |row| parse_progress_row(row),
            )
            .optional()
            .map_err(|e| anyhow::anyhow!(e))
        })
    }

    pub fn list_progress(&self, filter: &ExportFilter) -> anyhow::Result<Vec<ProgressEntry>> {
        let limit = filter.limit.unwrap_or(100).clamp(1, 500) as i64;
        self.with_connection(|conn| {
            let mut conditions = Vec::new();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(run_id) = &filter.run_id {
                conditions.push("run_id = ?");
                params.push(Box::new(run_id.as_str()));
            }

            if let Some(task_id) = &filter.task_id {
                conditions.push("task_id = ?");
                params.push(Box::new(task_id.as_str()));
            }

            if let Some(status) = &filter.status {
                let status_json = serde_json::to_string(status)?;
                conditions.push("status = ?");
                params.push(Box::new(status_json));
            }

            if let Some(start) = filter.start_time {
                conditions.push("updated_at >= ?");
                params.push(Box::new(start as i64));
            }

            if let Some(end) = filter.end_time {
                conditions.push("updated_at <= ?");
                params.push(Box::new(end as i64));
            }

            let where_clause = if conditions.is_empty() {
                "".to_string()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            let query = format!(
                "SELECT id, run_id, task_id, status, title, description, progress, total, unit,
                        started_at, updated_at, completed_at, error, metadata
                 FROM progress_entries
                 {}
                 ORDER BY updated_at DESC LIMIT ?",
                where_clause
            );

            params.push(Box::new(limit));

            let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&query)?;
            let mut rows = stmt.query(params_refs.as_slice())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(parse_progress_row(row)?);
            }
            Ok(out)
        })
    }

    pub fn upsert_trace(&self, entry: &TraceEntry) -> anyhow::Result<()> {
        let metadata = serde_json::to_string(&entry.metadata).context("Failed to encode trace metadata")?;
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO trace_entries (
                    id, run_id, task_id, parent_id, timestamp, level, message, lang, metadata
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(id) DO UPDATE SET
                    run_id=excluded.run_id,
                    task_id=excluded.task_id,
                    parent_id=excluded.parent_id,
                    timestamp=excluded.timestamp,
                    level=excluded.level,
                    message=excluded.message,
                    lang=excluded.lang,
                    metadata=excluded.metadata",
                params![
                    entry.id,
                    entry.run_id.as_deref(),
                    entry.task_id.as_deref(),
                    entry.parent_id.as_deref(),
                    entry.timestamp as i64,
                    entry.level,
                    entry.message,
                    entry.lang,
                    metadata,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_trace(&self, id: &str) -> anyhow::Result<Option<TraceEntry>> {
        self.with_connection(|conn| {
            conn.query_row(
                "SELECT id, run_id, task_id, parent_id, timestamp, level, message, lang, metadata
                 FROM trace_entries
                 WHERE id = ?1",
                params![id],
                |row| parse_trace_row(row),
            )
            .optional()
            .map_err(|e| anyhow::anyhow!(e))
        })
    }

    pub fn list_traces(&self, filter: &ExportFilter) -> anyhow::Result<Vec<TraceEntry>> {
        let limit = filter.limit.unwrap_or(100).clamp(1, 500) as i64;
        self.with_connection(|conn| {
            let mut conditions = Vec::new();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(run_id) = &filter.run_id {
                conditions.push("run_id = ?");
                params.push(Box::new(run_id.as_str()));
            }

            if let Some(task_id) = &filter.task_id {
                conditions.push("task_id = ?");
                params.push(Box::new(task_id.as_str()));
            }

            if let Some(level) = &filter.level {
                conditions.push("level = ?");
                params.push(Box::new(level.as_str()));
            }

            if let Some(start) = filter.start_time {
                conditions.push("timestamp >= ?");
                params.push(Box::new(start as i64));
            }

            if let Some(end) = filter.end_time {
                conditions.push("timestamp <= ?");
                params.push(Box::new(end as i64));
            }

            let where_clause = if conditions.is_empty() {
                "".to_string()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            let query = format!(
                "SELECT id, run_id, task_id, parent_id, timestamp, level, message, lang, metadata
                 FROM trace_entries
                 {}
                 ORDER BY timestamp DESC LIMIT ?",
                where_clause
            );

            params.push(Box::new(limit));

            let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&query)?;
            let mut rows = stmt.query(params_refs.as_slice())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(parse_trace_row(row)?);
            }
            Ok(out)
        })
    }
}

fn parse_progress_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProgressEntry> {
    let id: String = row.get(0)?;
    let run_id: Option<String> = row.get(1)?;
    let task_id: Option<String> = row.get(2)?;
    let status_raw: String = row.get(3)?;
    let title: String = row.get(4)?;
    let description: Option<String> = row.get(5)?;
    let progress: f64 = row.get(6)?;
    let total: Option<f64> = row.get(7)?;
    let unit: Option<String> = row.get(8)?;
    let started_at: Option<i64> = row.get(9)?;
    let updated_at: i64 = row.get(10)?;
    let completed_at: Option<i64> = row.get(11)?;
    let error: Option<String> = row.get(12)?;
    let metadata_raw: String = row.get(13)?;

    let status: ProgressStatus = serde_json::from_str(&status_raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));

    Ok(ProgressEntry {
        id,
        run_id,
        task_id,
        status,
        title,
        description,
        progress,
        total,
        unit,
        started_at: started_at.map(|v| v as u64),
        updated_at: updated_at as u64,
        completed_at: completed_at.map(|v| v as u64),
        error,
        metadata,
    })
}

fn parse_trace_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TraceEntry> {
    let id: String = row.get(0)?;
    let run_id: Option<String> = row.get(1)?;
    let task_id: Option<String> = row.get(2)?;
    let parent_id: Option<String> = row.get(3)?;
    let timestamp: i64 = row.get(4)?;
    let level: String = row.get(5)?;
    let message: String = row.get(6)?;
    let lang: String = row.get(7)?;
    let metadata_raw: String = row.get(8)?;

    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));

    Ok(TraceEntry {
        id,
        run_id,
        task_id,
        parent_id,
        timestamp: timestamp as u64,
        level,
        message,
        lang,
        metadata,
    })
}

impl SwarmSqliteStore {
    pub fn create_intelligent_task(
        &self,
        title: String,
        description: String,
        priority: crate::swarm::TaskPriority,
        assignee_type: crate::swarm::AssigneeType,
        assigned_by: String,
        parent_task_id: Option<String>,
        due_date: Option<u64>,
        metadata: serde_json::Value,
    ) -> anyhow::Result<String> {
        use crate::swarm::TaskStatus;
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_unix();
        let task = crate::swarm::IntelligentTask {
            id: id.clone(),
            title,
            description,
            status: TaskStatus::Pending,
            priority,
            assignee_type,
            assigned_by,
            parent_task_id,
            created_at: now,
            updated_at: now,
            due_date,
            progress: 0.0,
            metadata,
        };
        self.store_intelligent_task(&task)?;
        Ok(id)
    }

    pub fn store_intelligent_task(&self, task: &crate::swarm::IntelligentTask) -> anyhow::Result<()> {
        let metadata = serde_json::to_string(&task.metadata).context("Failed to encode task metadata")?;
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO intelligent_tasks (
                    id, title, description, status, priority, assignee_type,
                    assigned_by, parent_task_id, created_at, updated_at, due_date, progress, metadata
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                 ON CONFLICT(id) DO UPDATE SET
                    title=excluded.title,
                    description=excluded.description,
                    status=excluded.status,
                    priority=excluded.priority,
                    assignee_type=excluded.assignee_type,
                    assigned_by=excluded.assigned_by,
                    parent_task_id=excluded.parent_task_id,
                    updated_at=excluded.updated_at,
                    due_date=excluded.due_date,
                    progress=excluded.progress,
                    metadata=excluded.metadata",
                params![
                    task.id,
                    task.title,
                    task.description,
                    serde_json::to_string(&task.status)?,
                    serde_json::to_string(&task.priority)?,
                    serde_json::to_string(&task.assignee_type)?,
                    task.assigned_by,
                    task.parent_task_id.as_deref(),
                    task.created_at as i64,
                    task.updated_at as i64,
                    task.due_date.map(|v| v as i64),
                    task.progress,
                    metadata,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_intelligent_task(&self, id: &str) -> anyhow::Result<Option<crate::swarm::IntelligentTask>> {
        self.with_connection(|conn| {
            conn.query_row(
                "SELECT id, title, description, status, priority, assignee_type,
                        assigned_by, parent_task_id, created_at, updated_at, due_date, progress, metadata
                 FROM intelligent_tasks WHERE id = ?1",
                params![id],
                |row| parse_intelligent_task_row(row),
            )
            .optional()
            .map_err(|e| anyhow::anyhow!(e))
        })
    }

    pub fn list_intelligent_tasks(&self, limit: Option<usize>) -> anyhow::Result<Vec<crate::swarm::IntelligentTask>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, description, status, priority, assignee_type,
                        assigned_by, parent_task_id, created_at, updated_at, due_date, progress, metadata
                 FROM intelligent_tasks ORDER BY created_at DESC",
            )?;
            
            let mut rows = stmt.query([])?;
            let mut tasks = Vec::new();
            
            while let Some(row) = rows.next()? {
                tasks.push(parse_intelligent_task_row(row)?);
            }
            
            if let Some(limit) = limit {
                tasks.truncate(limit);
            }
            
            Ok(tasks)
        })
    }

    pub fn update_intelligent_task_status(
        &self,
        id: &str,
        status: crate::swarm::TaskStatus,
    ) -> anyhow::Result<()> {
        let now = now_unix();
        self.with_connection(|conn| {
            conn.execute(
                "UPDATE intelligent_tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![serde_json::to_string(&status)?, now as i64, id],
            )?;
            Ok(())
        })
    }

    pub fn add_task_assignee(
        &self,
        task_id: String,
        assignee_name: String,
    ) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_unix();
        let assignee = crate::swarm::TaskAssignee {
            id: id.clone(),
            task_id,
            assignee_name,
            assigned_at: now,
        };
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO task_assignees (id, task_id, assignee_name, assigned_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![assignee.id, assignee.task_id, assignee.assignee_name, assignee.assigned_at as i64],
            )?;
            Ok(())
        })?;
        Ok(id)
    }

    pub fn get_task_assignees(&self, task_id: &str) -> anyhow::Result<Vec<crate::swarm::TaskAssignee>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, task_id, assignee_name, assigned_at FROM task_assignees WHERE task_id = ?1",
            )?;
            let mut rows = stmt.query(params![task_id])?;
            let mut assignees = Vec::new();
            while let Some(row) = rows.next()? {
                assignees.push(crate::swarm::TaskAssignee {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    assignee_name: row.get(2)?,
                    assigned_at: row.get::<_, i64>(3)? as u64,
                });
            }
            Ok(assignees)
        })
    }
}

fn parse_intelligent_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::swarm::IntelligentTask> {
    let id: String = row.get(0)?;
    let title: String = row.get(1)?;
    let description: String = row.get(2)?;
    let status_raw: String = row.get(3)?;
    let priority_raw: String = row.get(4)?;
    let assignee_type_raw: String = row.get(5)?;
    let assigned_by: String = row.get(6)?;
    let parent_task_id: Option<String> = row.get(7)?;
    let created_at: i64 = row.get(8)?;
    let updated_at: i64 = row.get(9)?;
    let due_date: Option<i64> = row.get(10)?;
    let progress: f64 = row.get(11)?;
    let metadata_raw: String = row.get(12)?;

    let status: crate::swarm::TaskStatus = serde_json::from_str(&status_raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let priority: crate::swarm::TaskPriority = serde_json::from_str(&priority_raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let assignee_type: crate::swarm::AssigneeType = serde_json::from_str(&assignee_type_raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));

    Ok(crate::swarm::IntelligentTask {
        id,
        title,
        description,
        status,
        priority,
        assignee_type,
        assigned_by,
        parent_task_id,
        created_at: created_at as u64,
        updated_at: updated_at as u64,
        due_date: due_date.map(|v| v as u64),
        progress,
        metadata,
    })
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
