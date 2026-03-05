-- Brain 数据库初始化脚本
-- 大脑记忆存储表结构

-- 性能优化 PRAGMA 设置
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA mmap_size    = 8388608;
PRAGMA cache_size   = -2000;
PRAGMA temp_store   = MEMORY;

-- 核心记忆表
CREATE TABLE IF NOT EXISTS memories (
    id          TEXT PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    content     TEXT NOT NULL,
    category    TEXT NOT NULL DEFAULT 'core',
    embedding   BLOB,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
CREATE INDEX IF NOT EXISTS idx_memories_key ON memories(key);

-- FTS5 全文搜索虚拟表（BM25 评分）
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    key, content, content=memories, content_rowid=rowid
);

-- FTS5 触发器：与 memories 表保持同步
CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, key, content)
    VALUES (new.rowid, new.key, new.content);
END;
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, key, content)
    VALUES ('delete', old.rowid, old.key, old.content);
END;
CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, key, content)
    VALUES ('delete', old.rowid, old.key, old.content);
    INSERT INTO memories_fts(rowid, key, content)
    VALUES (new.rowid, new.key, new.content);
END;

-- 嵌入缓存表（带 LRU 淘汰）
CREATE TABLE IF NOT EXISTS embedding_cache (
    content_hash TEXT PRIMARY KEY,
    embedding    BLOB NOT NULL,
    created_at   TEXT NOT NULL,
    accessed_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cache_accessed ON embedding_cache(accessed_at);
