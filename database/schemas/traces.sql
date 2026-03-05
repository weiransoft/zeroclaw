-- Traces 数据库初始化脚本
-- 追踪数据存储表结构

-- 性能优化 PRAGMA 设置
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA mmap_size = 8388608;
PRAGMA cache_size = -2000;
PRAGMA temp_store = MEMORY;
PRAGMA foreign_keys = ON;

-- 主轨迹表
CREATE TABLE IF NOT EXISTS agent_traces (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL,
    parent_id       TEXT,
    timestamp       INTEGER NOT NULL,
    duration_ms     INTEGER,
    trace_type      TEXT NOT NULL,
    input           TEXT NOT NULL,
    output          TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- 轨迹索引
CREATE INDEX IF NOT EXISTS idx_traces_run_id ON agent_traces(run_id);
CREATE INDEX IF NOT EXISTS idx_traces_parent ON agent_traces(parent_id);
CREATE INDEX IF NOT EXISTS idx_traces_timestamp ON agent_traces(timestamp);
CREATE INDEX IF NOT EXISTS idx_traces_type ON agent_traces(trace_type);
CREATE INDEX IF NOT EXISTS idx_traces_duration ON agent_traces(duration_ms);

-- 推理链表
CREATE TABLE IF NOT EXISTS reasoning_chains (
    id              TEXT PRIMARY KEY,
    trace_id        TEXT NOT NULL UNIQUE,
    steps           TEXT NOT NULL,
    conclusion      TEXT,
    confidence      REAL,
    quality_score   REAL,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY (trace_id) REFERENCES agent_traces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reasoning_trace ON reasoning_chains(trace_id);

-- 决策点表
CREATE TABLE IF NOT EXISTS decision_points (
    id                  TEXT PRIMARY KEY,
    trace_id            TEXT NOT NULL,
    decision_type       TEXT NOT NULL,
    description         TEXT NOT NULL,
    alternatives        TEXT NOT NULL,
    chosen_alternative_id TEXT NOT NULL,
    rationale           TEXT,
    quality_score       REAL,
    is_optimal          INTEGER NOT NULL DEFAULT 0,
    timestamp           INTEGER NOT NULL,
    FOREIGN KEY (trace_id) REFERENCES agent_traces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_decisions_trace ON decision_points(trace_id);
CREATE INDEX IF NOT EXISTS idx_decisions_type ON decision_points(decision_type);

-- 评估结果表
CREATE TABLE IF NOT EXISTS evaluation_results (
    id                  TEXT PRIMARY KEY,
    trace_id            TEXT NOT NULL UNIQUE,
    overall_score       REAL NOT NULL,
    decision_scores     TEXT,
    reasoning_quality   REAL,
    efficiency_score    REAL,
    error_rate          REAL,
    suggestions         TEXT,
    evaluated_at        INTEGER NOT NULL,
    FOREIGN KEY (trace_id) REFERENCES agent_traces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_eval_trace ON evaluation_results(trace_id);
