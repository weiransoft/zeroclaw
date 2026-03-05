-- Swarm 数据库初始化脚本
-- Swarm 模块存储表结构

-- 性能优化 PRAGMA 设置
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA mmap_size    = 8388608;
PRAGMA cache_size   = -2000;
PRAGMA temp_store   = MEMORY;

-- 子代理运行表
CREATE TABLE IF NOT EXISTS subagent_runs (
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

-- Swarm 事件表
CREATE TABLE IF NOT EXISTS swarm_events (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_unix INTEGER NOT NULL,
    run_id  TEXT,
    kind    TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_swarm_events_run ON swarm_events(run_id);

-- Swarm 聊天表
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

-- Swarm 扩展聊天表
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

-- 进度条目表
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

-- 轨迹条目表
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

-- 智能任务表
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

-- 任务分配表
CREATE TABLE IF NOT EXISTS task_assignees (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL,
    assignee_name   TEXT NOT NULL,
    assigned_at     INTEGER NOT NULL,
    FOREIGN KEY (task_id) REFERENCES intelligent_tasks(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_task_assignees_task ON task_assignees(task_id);
CREATE INDEX IF NOT EXISTS idx_task_assignees_name ON task_assignees(assignee_name);
