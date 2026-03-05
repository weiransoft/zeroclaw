-- Workflow 数据库初始化脚本
-- 工作流存储表结构

-- 性能优化 PRAGMA 设置
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA mmap_size    = 8388608;
PRAGMA cache_size   = -2000;
PRAGMA temp_store   = MEMORY;

-- 工作流主表
CREATE TABLE IF NOT EXISTS workflows (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'created',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    created_by  TEXT,
    roles       TEXT NOT NULL DEFAULT '[]',
    metadata    TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_workflows_status ON workflows(status);
CREATE INDEX IF NOT EXISTS idx_workflows_updated ON workflows(updated_at DESC);

-- 工作流步骤表
CREATE TABLE IF NOT EXISTS workflow_steps (
    id          TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'pending',
    step_order  INTEGER NOT NULL,
    assigned_to TEXT,
    started_at  INTEGER,
    completed_at INTEGER,
    metadata    TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_workflow ON workflow_steps(workflow_id);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_order ON workflow_steps(step_order);

-- 工作流模板表
CREATE TABLE IF NOT EXISTS workflow_templates (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    categories          TEXT NOT NULL DEFAULT '[]',
    applicable_scenarios TEXT NOT NULL DEFAULT '[]',
    content             TEXT NOT NULL DEFAULT '',
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    metadata            TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_workflow_templates_name ON workflow_templates(name);
