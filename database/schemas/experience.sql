-- Experience 数据库初始化脚本
-- 经验存储表结构

-- 性能优化 PRAGMA 设置
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA mmap_size    = 8388608;
PRAGMA cache_size   = -2000;
PRAGMA temp_store   = MEMORY;

-- 经验主表
CREATE TABLE IF NOT EXISTS experiences (
    id              TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    content         TEXT NOT NULL,
    tags            TEXT NOT NULL DEFAULT '[]',
    category        TEXT,
    difficulty_level TEXT,
    success_rate    REAL,
    author_id       TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    used_count      INTEGER NOT NULL DEFAULT 0,
    last_used_at    INTEGER,
    rating          REAL,
    rating_count    INTEGER NOT NULL DEFAULT 0,
    metadata        TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_experiences_category ON experiences(category);
CREATE INDEX IF NOT EXISTS idx_experiences_created ON experiences(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_experiences_used ON experiences(used_count DESC);
CREATE INDEX IF NOT EXISTS idx_experiences_rating ON experiences(rating DESC);

-- 经验关联表
CREATE TABLE IF NOT EXISTS experience_relations (
    id                  TEXT PRIMARY KEY,
    experience_id       TEXT NOT NULL,
    related_experience_id TEXT NOT NULL,
    relation_type       TEXT NOT NULL,
    strength            REAL NOT NULL DEFAULT 1.0,
    created_at          INTEGER NOT NULL,
    metadata            TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (experience_id) REFERENCES experiences(id) ON DELETE CASCADE,
    FOREIGN KEY (related_experience_id) REFERENCES experiences(id) ON DELETE CASCADE,
    UNIQUE(experience_id, related_experience_id)
);
CREATE INDEX IF NOT EXISTS idx_experience_relations_source ON experience_relations(experience_id);
CREATE INDEX IF NOT EXISTS idx_experience_relations_target ON experience_relations(related_experience_id);

-- 全文搜索虚拟表
CREATE VIRTUAL TABLE IF NOT EXISTS experiences_fts USING fts5(
    title, description, content, content=experiences, content_rowid=rowid
);
