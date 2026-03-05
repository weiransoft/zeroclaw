-- Knowledge 数据库初始化脚本
-- 知识存储表结构

-- 性能优化 PRAGMA 设置
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA mmap_size    = 8388608;
PRAGMA cache_size   = -2000;
PRAGMA temp_store   = MEMORY;

-- 知识分类表
CREATE TABLE IF NOT EXISTS knowledge_categories (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    parent_id   TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    metadata    TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (parent_id) REFERENCES knowledge_categories(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_knowledge_categories_parent ON knowledge_categories(parent_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_categories_name ON knowledge_categories(name);

-- 知识条目表
CREATE TABLE IF NOT EXISTS knowledge_items (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    content     TEXT NOT NULL,
    summary     TEXT,
    category_id TEXT,
    tags        TEXT NOT NULL DEFAULT '[]',
    source      TEXT,
    author      TEXT,
    status      TEXT NOT NULL DEFAULT 'active',  -- 状态字段: active, archived, draft, deleted
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL,
    access_count INTEGER NOT NULL DEFAULT 0,
    metadata    TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (category_id) REFERENCES knowledge_categories(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_knowledge_items_category ON knowledge_items(category_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_items_created ON knowledge_items(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_items_accessed ON knowledge_items(accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_items_status ON knowledge_items(status);
CREATE INDEX IF NOT EXISTS idx_knowledge_items_title ON knowledge_items(title);

-- 全文搜索虚拟表
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_items_fts USING fts5(
    title, content, summary, content=knowledge_items, content_rowid=rowid
);
