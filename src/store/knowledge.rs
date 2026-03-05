//! 知识存储

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

/// 知识分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeCategory {
    pub id: String,
    pub name: String,
    pub description: String,
    pub parent_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub metadata: serde_json::Value,
}

/// 知识条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeItem {
    pub id: String,
    pub title: String,
    pub content: String,
    pub summary: Option<String>,
    pub category_id: Option<String>,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub author: Option<String>,
    pub status: String,  // 状态字段: active, archived, draft, deleted
    pub created_at: i64,
    pub updated_at: i64,
    pub accessed_at: i64,
    pub access_count: i64,
    pub metadata: serde_json::Value,
}

/// 知识存储
///
/// 使用统一的数据库连接池管理知识相关的持久化数据
pub struct KnowledgeStore {
    /// 数据库连接池
    pool: Arc<DbPool>,
}

impl KnowledgeStore {
    /// 创建新的知识存储
    ///
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    ///
    /// # Returns
    /// 初始化好的 KnowledgeStore 实例
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        // 使用统一配置创建连接池
        let config = SqliteConfig::from_workspace(workspace_dir, "knowledge.db");
        let db_path = config.path.clone();
        let pool = Arc::new(DbPool::new(config.clone())?);
        
        tracing::debug!("[KnowledgeStore] Initializing database at: {:?}", db_path);
        
        let store = Self {
            pool,
        };
        
        // 使用 SQL 文件初始化 schema
        store.init_schema()?;
        
        Ok(store)
    }
    
    /// 从现有连接池创建知识存储
    ///
    /// # Arguments
    /// * `pool` - 数据库连接池
    ///
    /// # Returns
    /// KnowledgeStore 实例
    pub fn from_pool(pool: Arc<DbPool>) -> Result<Self> {
        let store = Self {
            pool,
        };
        store.init_schema()?;
        Ok(store)
    }
    

    
    /// 初始化数据库 Schema
    ///
    /// 从 SQL 文件加载并执行数据库初始化脚本
    fn init_schema(&self) -> Result<()> {
        let loader = SqlLoader::default()?;
        let schema_sql = loader.load_schema(DatabaseType::Knowledge)
            .context("Failed to load knowledge schema")?;
        
        self.pool.init_schema(&schema_sql)
            .context("Failed to initialize knowledge schema")?;
        
        Ok(())
    }
    
    /// 创建分类
    ///
    /// # Arguments
    /// * `name` - 分类名称
    /// * `description` - 分类描述
    /// * `parent_id` - 父分类ID（可选）
    ///
    /// # Returns
    /// 创建的分类实例
    pub fn create_category(&self, name: &str, description: &str, parent_id: Option<String>) -> Result<KnowledgeCategory> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let category = KnowledgeCategory {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            parent_id,
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO knowledge_categories (id, name, description, parent_id, created_at, updated_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    category.id,
                    category.name,
                    category.description,
                    category.parent_id,
                    category.created_at,
                    category.updated_at,
                    serde_json::to_string(&category.metadata)?,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(category)
    }
    
    /// 获取分类
    ///
    /// # Arguments
    /// * `id` - 分类ID
    ///
    /// # Returns
    /// 分类实例，如果不存在则返回 None
    pub fn get_category(&self, id: &str) -> Result<Option<KnowledgeCategory>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, parent_id, created_at, updated_at, metadata
                 FROM knowledge_categories WHERE id = ?1",
                params![id],
                |row| parse_category_row(row),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 列出所有分类
    ///
    /// # Arguments
    /// * `parent_id` - 父分类ID过滤（可选）
    ///
    /// # Returns
    /// 分类列表
    pub fn list_categories(&self, parent_id: Option<&str>) -> Result<Vec<KnowledgeCategory>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut categories = Vec::new();
            
            if let Some(parent_id_val) = parent_id {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, parent_id, created_at, updated_at, metadata
                     FROM knowledge_categories WHERE parent_id = ?1 ORDER BY name",
                )?;
                
                let mut rows = stmt.query(params![parent_id_val])?;
                while let Some(row) = rows.next()? {
                    categories.push(parse_category_row(row)?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, parent_id, created_at, updated_at, metadata
                     FROM knowledge_categories ORDER BY name",
                )?;
                
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    categories.push(parse_category_row(row)?);
                }
            }
            
            Ok(categories)
        })
    }
    
    /// 更新分类
    ///
    /// # Arguments
    /// * `id` - 分类ID
    /// * `name` - 新名称（可选）
    /// * `description` - 新描述（可选）
    /// * `parent_id` - 新父分类ID（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_category(&self, id: &str, name: Option<&str>, description: Option<&str>, parent_id: Option<String>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE knowledge_categories SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE knowledge_categories SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            if let Some(parent_id) = parent_id {
                conn.execute(
                    "UPDATE knowledge_categories SET parent_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![parent_id, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除分类
    ///
    /// # Arguments
    /// * `id` - 分类ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_category(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM knowledge_categories WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 创建知识条目
    ///
    /// # Arguments
    /// * `title` - 条目标题
    /// * `content` - 条目内容
    /// * `summary` - 条目摘要（可选）
    /// * `category_id` - 分类ID（可选）
    /// * `tags` - 标签列表
    /// * `source` - 来源（可选）
    /// * `author` - 作者（可选）
    ///
    /// # Returns
    /// 创建的知识条目实例
    pub fn create_item(&self, title: &str, content: &str, summary: Option<String>, category_id: Option<String>, tags: Vec<String>, source: Option<String>, author: Option<String>) -> Result<KnowledgeItem> {
        // 输入验证
        if title.trim().is_empty() {
            return Err(anyhow::anyhow!("Knowledge item title cannot be empty"));
        }
        
        if content.trim().is_empty() {
            return Err(anyhow::anyhow!("Knowledge item content cannot be empty"));
        }
        
        // 检查重复性 - 验证相同标题的知识条目是否已存在
        if self.get_item_by_title(title)?.is_some() {
            return Err(anyhow::anyhow!("Knowledge item with title '{}' already exists", title));
        }
        
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let item = KnowledgeItem {
            id: id.clone(),
            title: title.to_string(),
            content: content.to_string(),
            summary,
            category_id,
            tags,
            source,
            author,
            status: "active".to_string(),  // 默认状态为活跃
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO knowledge_items (id, title, content, summary, category_id, tags, source, author, status, created_at, updated_at, accessed_at, access_count, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    item.id,
                    item.title,
                    item.content,
                    item.summary,
                    item.category_id,
                    serde_json::to_string(&item.tags)?,
                    item.source,
                    item.author,
                    item.status,
                    item.created_at,
                    item.updated_at,
                    item.accessed_at,
                    item.access_count,
                    serde_json::to_string(&item.metadata)?,
                ],
            )?;
            
            conn.execute(
                "INSERT INTO knowledge_items_fts (rowid, title, content, summary)
                 SELECT rowid, title, content, summary FROM knowledge_items WHERE id = ?1",
                params![item.id],
            )?;
            
            Ok(())
        })?;
        
        Ok(item)
    }
    
    /// 根据标题获取知识条目
    ///
    /// # Arguments
    /// * `title` - 条目标题
    ///
    /// # Returns
    /// 条目实例，如果不存在则返回 None
    pub fn get_item_by_title(&self, title: &str) -> Result<Option<KnowledgeItem>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, title, content, summary, category_id, tags, source, author, status, created_at, updated_at, accessed_at, access_count, metadata
                 FROM knowledge_items WHERE title = ?1",
                params![title],
                |row| parse_item_row(row),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 获取知识条目
    ///
    /// # Arguments
    /// * `id` - 条目ID
    ///
    /// # Returns
    /// 条目实例，如果不存在则返回 None
    pub fn get_item(&self, id: &str) -> Result<Option<KnowledgeItem>> {
        self.pool.with_connection(|conn: &Connection| {
            let result = conn.query_row(
                "SELECT id, title, content, summary, category_id, tags, source, author, status, created_at, updated_at, accessed_at, access_count, metadata
                 FROM knowledge_items WHERE id = ?1",
                params![id],
                |row| parse_item_row(row),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))?;
            
            if let Some(mut item) = result {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute(
                    "UPDATE knowledge_items SET accessed_at = ?1, access_count = access_count + 1 WHERE id = ?2",
                    params![now, id],
                )?;
                item.accessed_at = now;
                item.access_count += 1;
                Ok(Some(item))
            } else {
                Ok(None)
            }
        })
    }
    
    /// 列出知识条目
    ///
    /// # Arguments
    /// * `category_id` - 分类过滤（可选）
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 知识条目列表
    pub fn list_items(&self, category_id: Option<&str>, limit: Option<usize>) -> Result<Vec<KnowledgeItem>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut items = Vec::new();
            
            if let Some(category_id_val) = category_id {
                let mut stmt = conn.prepare(
                    "SELECT id, title, content, summary, category_id, tags, source, author, status, created_at, updated_at, accessed_at, access_count, metadata
                     FROM knowledge_items WHERE category_id = ?1 ORDER BY updated_at DESC",
                )?;
                
                let mut rows = stmt.query(params![category_id_val])?;
                while let Some(row) = rows.next()? {
                    items.push(parse_item_row(row)?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, title, content, summary, category_id, tags, source, author, status, created_at, updated_at, accessed_at, access_count, metadata
                     FROM knowledge_items ORDER BY updated_at DESC",
                )?;
                
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    items.push(parse_item_row(row)?);
                }
            }
            
            if let Some(limit) = limit {
                items.truncate(limit);
            }
            
            Ok(items)
        })
    }
    
    /// 搜索知识条目
    ///
    /// # Arguments
    /// * `query` - 搜索关键词
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 搜索结果知识条目列表
    pub fn search_items(&self, query: &str, limit: Option<usize>) -> Result<Vec<KnowledgeItem>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT k.id, k.title, k.content, k.summary, k.category_id, k.tags, k.source, k.author, k.status, k.created_at, k.updated_at, k.accessed_at, k.access_count, k.metadata
                 FROM knowledge_items k
                 INNER JOIN knowledge_items_fts fts ON k.rowid = fts.rowid
                 WHERE knowledge_items_fts MATCH ?1
                 ORDER BY rank DESC",
            )?;
            
            let mut rows = stmt.query(params![query])?;
            let mut items = Vec::new();
            
            while let Some(row) = rows.next()? {
                items.push(parse_item_row(row)?);
            }
            
            if let Some(limit) = limit {
                items.truncate(limit);
            }
            
            Ok(items)
        })
    }
    
    /// 更新知识条目
    ///
    /// # Arguments
    /// * `id` - 条目ID
    /// * `title` - 新标题（可选）
    /// * `content` - 新内容（可选）
    /// * `summary` - 新摘要（可选）
    /// * `category_id` - 新分类ID（可选）
    /// * `tags` - 新标签（可选）
    /// * `status` - 新状态（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_item(&self, id: &str, title: Option<&str>, content: Option<&str>, summary: Option<String>, category_id: Option<String>, tags: Option<Vec<String>>, status: Option<&str>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(title) = title {
                conn.execute(
                    "UPDATE knowledge_items SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, id],
                )?;
            }
            
            if let Some(content) = content {
                conn.execute(
                    "UPDATE knowledge_items SET content = ?1, updated_at = ?2 WHERE id = ?3",
                    params![content, now, id],
                )?;
            }
            
            if let Some(summary) = summary {
                conn.execute(
                    "UPDATE knowledge_items SET summary = ?1, updated_at = ?2 WHERE id = ?3",
                    params![summary, now, id],
                )?;
            }
            
            if let Some(category_id) = category_id {
                conn.execute(
                    "UPDATE knowledge_items SET category_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![category_id, now, id],
                )?;
            }
            
            if let Some(tags) = tags {
                conn.execute(
                    "UPDATE knowledge_items SET tags = ?1, updated_at = ?2 WHERE id = ?3",
                    params![serde_json::to_string(&tags)?, now, id],
                )?;
            }
            
            if let Some(status) = status {
                conn.execute(
                    "UPDATE knowledge_items SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![status, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除知识条目
    ///
    /// # Arguments
    /// * `id` - 条目ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_item(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM knowledge_items WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 按状态列出知识条目
    ///
    /// # Arguments
    /// * `status` - 状态筛选条件
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 符合状态条件的知识条目列表
    pub fn list_items_by_status(&self, status: &str, limit: Option<usize>) -> Result<Vec<KnowledgeItem>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut items = Vec::new();
            
            let mut stmt = conn.prepare(
                "SELECT id, title, content, summary, category_id, tags, source, author, status, created_at, updated_at, accessed_at, access_count, metadata
                 FROM knowledge_items WHERE status = ?1 ORDER BY updated_at DESC",
            )?;
            
            let mut rows = stmt.query(params![status])?;
            while let Some(row) = rows.next()? {
                items.push(parse_item_row(row)?);
            }
            
            if let Some(limit) = limit {
                items.truncate(limit);
            }
            
            Ok(items)
        })
    }
}

fn parse_category_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeCategory> {
    let metadata_raw: String = row.get(6)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(KnowledgeCategory {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        parent_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        metadata,
    })
}

fn parse_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeItem> {
    let tags_raw: String = row.get(5)?;
    let tags: Vec<String> = serde_json::from_str(&tags_raw)
        .unwrap_or(vec![]);
    
    let metadata_raw: String = row.get(13)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(KnowledgeItem {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        summary: row.get(3)?,
        category_id: row.get(4)?,
        tags,
        source: row.get(6)?,
        author: row.get(7)?,
        status: row.get(8)?,  // 状态字段
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        accessed_at: row.get(11)?,
        access_count: row.get(12)?,
        metadata,
    })
}
