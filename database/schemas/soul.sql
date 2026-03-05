-- Soul 数据库初始化脚本
-- 灵魂存储表结构

-- 性能优化 PRAGMA 设置
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA mmap_size    = 8388608;
PRAGMA cache_size   = -2000;
PRAGMA temp_store   = MEMORY;

-- 灵魂主表
CREATE TABLE IF NOT EXISTS souls (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    personality TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    is_active   BOOLEAN NOT NULL DEFAULT 0,
    metadata    TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_souls_active ON souls(is_active);
CREATE INDEX IF NOT EXISTS idx_souls_created ON souls(created_at DESC);

-- 灵魂特质表
CREATE TABLE IF NOT EXISTS soul_traits (
    id          TEXT PRIMARY KEY,
    soul_id     TEXT NOT NULL,
    name        TEXT NOT NULL,
    value       REAL NOT NULL DEFAULT 0.0,
    description TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    metadata    TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (soul_id) REFERENCES souls(id) ON DELETE CASCADE,
    UNIQUE(soul_id, name)
);
CREATE INDEX IF NOT EXISTS idx_soul_traits_soul ON soul_traits(soul_id);

-- 灵魂记忆表
CREATE TABLE IF NOT EXISTS soul_memories (
    id              TEXT PRIMARY KEY,
    soul_id         TEXT NOT NULL,
    memory_type     TEXT NOT NULL,
    content         TEXT NOT NULL,
    importance      REAL NOT NULL DEFAULT 0.5,
    timestamp       INTEGER NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (soul_id) REFERENCES souls(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_soul_memories_soul ON soul_memories(soul_id);
CREATE INDEX IF NOT EXISTS idx_soul_memories_type ON soul_memories(memory_type);
CREATE INDEX IF NOT EXISTS idx_soul_memories_importance ON soul_memories(importance DESC);
CREATE INDEX IF NOT EXISTS idx_soul_memories_timestamp ON soul_memories(timestamp DESC);
