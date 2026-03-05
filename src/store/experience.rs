//! 经验存储

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

/// 经验条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub id: String,
    pub title: String,
    pub description: String,
    pub content: String,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub difficulty_level: Option<String>,
    pub success_rate: Option<f64>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub used_count: i64,
    pub last_used_at: Option<i64>,
    pub rating: Option<f64>,
    pub rating_count: i64,
    pub metadata: serde_json::Value,
}

/// 经验关联
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceRelation {
    pub id: String,
    pub experience_id: String,
    pub related_experience_id: String,
    pub relation_type: String,
    pub strength: f64,
    pub created_at: i64,
    pub metadata: serde_json::Value,
}

/// 经验存储
///
/// 使用统一的数据库连接池管理经验相关的持久化数据
pub struct ExperienceStore {
    /// 数据库连接池
    pool: Arc<DbPool>,
}

impl ExperienceStore {
    /// 创建新的经验存储
    ///
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    ///
    /// # Returns
    /// 初始化好的 ExperienceStore 实例
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        // 使用统一配置创建连接池
        let config = SqliteConfig::from_workspace(workspace_dir, "experience.db");
        let db_path = config.path.clone();
        let pool = Arc::new(DbPool::new(config.clone())?);
        
        tracing::debug!("[ExperienceStore] Initializing database at: {:?}", db_path);
        
        let store = Self {
            pool,
        };
        
        // 使用 SQL 文件初始化 schema
        store.init_schema()?;
        
        Ok(store)
    }
    
    /// 从现有连接池创建经验存储
    ///
    /// # Arguments
    /// * `pool` - 数据库连接池
    ///
    /// # Returns
    /// ExperienceStore 实例
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
        let schema_sql = loader.load_schema(DatabaseType::Experience)
            .context("Failed to load experience schema")?;
        
        self.pool.init_schema(&schema_sql)
            .context("Failed to initialize experience schema")?;
        
        Ok(())
    }
    
    /// 创建经验
    ///
    /// # Arguments
    /// * `title` - 经验标题
    /// * `description` - 经验描述
    /// * `content` - 经验内容
    /// * `tags` - 标签列表
    /// * `category` - 分类（可选）
    /// * `difficulty_level` - 难度级别（可选）
    /// * `author_id` - 作者ID（可选）
    ///
    /// # Returns
    /// 创建的经验实例
    pub fn create_experience(&self, title: &str, description: &str, content: &str, tags: Vec<String>, category: Option<String>, difficulty_level: Option<String>, author_id: Option<String>) -> Result<Experience> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let experience = Experience {
            id: id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            content: content.to_string(),
            tags,
            category,
            difficulty_level,
            success_rate: None,
            author_id,
            created_at: now,
            updated_at: now,
            used_count: 0,
            last_used_at: None,
            rating: None,
            rating_count: 0,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO experiences (id, title, description, content, tags, category, difficulty_level, author_id, created_at, updated_at, used_count, rating_count, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    experience.id,
                    experience.title,
                    experience.description,
                    experience.content,
                    serde_json::to_string(&experience.tags)?,
                    experience.category,
                    experience.difficulty_level,
                    experience.author_id,
                    experience.created_at,
                    experience.updated_at,
                    experience.used_count,
                    experience.rating_count,
                    serde_json::to_string(&experience.metadata)?,
                ],
            )?;
            
            conn.execute(
                "INSERT INTO experiences_fts (rowid, title, description, content)
                 SELECT rowid, title, description, content FROM experiences WHERE id = ?1",
                params![experience.id],
            )?;
            
            Ok(())
        })?;
        
        Ok(experience)
    }
    
    /// 获取经验
    ///
    /// # Arguments
    /// * `id` - 经验ID
    ///
    /// # Returns
    /// 经验实例，如果不存在则返回 None
    pub fn get_experience(&self, id: &str) -> Result<Option<Experience>> {
        self.pool.with_connection(|conn: &Connection| {
            let result = conn.query_row(
                "SELECT id, title, description, content, tags, category, difficulty_level, success_rate, author_id, created_at, updated_at, used_count, last_used_at, rating, rating_count, metadata
                 FROM experiences WHERE id = ?1",
                params![id],
                |row| parse_experience_row(row),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))?;
            
            if let Some(mut experience) = result {
                let now = chrono::Utc::now().timestamp_millis();
                conn.execute(
                    "UPDATE experiences SET used_count = used_count + 1, last_used_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                experience.used_count += 1;
                experience.last_used_at = Some(now);
                Ok(Some(experience))
            } else {
                Ok(None)
            }
        })
    }
    
    /// 列出经验
    ///
    /// # Arguments
    /// * `category` - 分类过滤（可选）
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 经验列表
    pub fn list_experiences(&self, category: Option<&str>, limit: Option<usize>) -> Result<Vec<Experience>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut experiences = Vec::new();
            
            if let Some(category_val) = category {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, content, tags, category, difficulty_level, success_rate, author_id, created_at, updated_at, used_count, last_used_at, rating, rating_count, metadata
                     FROM experiences WHERE category = ?1 ORDER BY created_at DESC",
                )?;
                
                let mut rows = stmt.query(params![category_val])?;
                while let Some(row) = rows.next()? {
                    experiences.push(parse_experience_row(row)?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, content, tags, category, difficulty_level, success_rate, author_id, created_at, updated_at, used_count, last_used_at, rating, rating_count, metadata
                     FROM experiences ORDER BY created_at DESC",
                )?;
                
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    experiences.push(parse_experience_row(row)?);
                }
            }
            
            if let Some(limit) = limit {
                experiences.truncate(limit);
            }
            
            Ok(experiences)
        })
    }
    
    /// 搜索经验
    ///
    /// # Arguments
    /// * `query` - 搜索关键词
    /// * `limit` - 返回数量限制（可选）
    ///
    /// # Returns
    /// 搜索结果经验列表
    pub fn search_experiences(&self, query: &str, limit: Option<usize>) -> Result<Vec<Experience>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT e.id, e.title, e.description, e.content, e.tags, e.category, e.difficulty_level, e.success_rate, e.author_id, e.created_at, e.updated_at, e.used_count, e.last_used_at, e.rating, e.rating_count, e.metadata
                 FROM experiences e
                 INNER JOIN experiences_fts fts ON e.rowid = fts.rowid
                 WHERE experiences_fts MATCH ?1
                 ORDER BY rank DESC",
            )?;
            
            let mut rows = stmt.query(params![query])?;
            let mut experiences = Vec::new();
            
            while let Some(row) = rows.next()? {
                experiences.push(parse_experience_row(row)?);
            }
            
            if let Some(limit) = limit {
                experiences.truncate(limit);
            }
            
            Ok(experiences)
        })
    }
    
    /// 更新经验
    ///
    /// # Arguments
    /// * `id` - 经验ID
    /// * `title` - 新标题（可选）
    /// * `description` - 新描述（可选）
    /// * `content` - 新内容（可选）
    /// * `tags` - 新标签（可选）
    /// * `category` - 新分类（可选）
    /// * `difficulty_level` - 新难度级别（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_experience(&self, id: &str, title: Option<&str>, description: Option<&str>, content: Option<&str>, tags: Option<Vec<String>>, category: Option<String>, difficulty_level: Option<String>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(title) = title {
                conn.execute(
                    "UPDATE experiences SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE experiences SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            if let Some(content) = content {
                conn.execute(
                    "UPDATE experiences SET content = ?1, updated_at = ?2 WHERE id = ?3",
                    params![content, now, id],
                )?;
            }
            
            if let Some(tags) = tags {
                conn.execute(
                    "UPDATE experiences SET tags = ?1, updated_at = ?2 WHERE id = ?3",
                    params![serde_json::to_string(&tags)?, now, id],
                )?;
            }
            
            if let Some(category) = category {
                conn.execute(
                    "UPDATE experiences SET category = ?1, updated_at = ?2 WHERE id = ?3",
                    params![category, now, id],
                )?;
            }
            
            if let Some(difficulty_level) = difficulty_level {
                conn.execute(
                    "UPDATE experiences SET difficulty_level = ?1, updated_at = ?2 WHERE id = ?3",
                    params![difficulty_level, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 评分经验
    ///
    /// # Arguments
    /// * `id` - 经验ID
    /// * `rating` - 评分值
    ///
    /// # Returns
    /// 操作结果
    pub fn rate_experience(&self, id: &str, rating: f64) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let result = conn.query_row(
                "SELECT rating, rating_count FROM experiences WHERE id = ?1",
                params![id],
                |row| {
                    let current_rating: Option<f64> = row.get(0)?;
                    let current_count: i64 = row.get(1)?;
                    Ok((current_rating, current_count))
                },
            )
            .optional()?;
            
            if let Some((current_rating, current_count)) = result {
                let new_rating = if let Some(cr) = current_rating {
                    (cr * current_count as f64 + rating) / (current_count as f64 + 1.0)
                } else {
                    rating
                };
                
                let new_count = current_count + 1;
                
                conn.execute(
                    "UPDATE experiences SET rating = ?1, rating_count = ?2 WHERE id = ?3",
                    params![new_rating, new_count, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 更新成功率
    ///
    /// # Arguments
    /// * `id` - 经验ID
    /// * `success` - 是否成功
    ///
    /// # Returns
    /// 操作结果
    pub fn update_success_rate(&self, id: &str, success: bool) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let result = conn.query_row(
                "SELECT success_rate, used_count FROM experiences WHERE id = ?1",
                params![id],
                |row| {
                    let current_rate: Option<f64> = row.get(0)?;
                    let used_count: i64 = row.get(1)?;
                    Ok((current_rate, used_count))
                },
            )
            .optional()?;
            
            if let Some((current_rate, used_count)) = result {
                let new_rate = if let Some(cr) = current_rate {
                    let success_count = (cr * used_count as f64).round() as i64;
                    let new_success_count = if success { success_count + 1 } else { success_count };
                    new_success_count as f64 / (used_count as f64 + 1.0)
                } else {
                    if success { 1.0 } else { 0.0 }
                };
                
                conn.execute(
                    "UPDATE experiences SET success_rate = ?1 WHERE id = ?2",
                    params![new_rate, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除经验
    ///
    /// # Arguments
    /// * `id` - 经验ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_experience(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM experiences WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 添加经验关联
    ///
    /// # Arguments
    /// * `experience_id` - 经验ID
    /// * `related_experience_id` - 相关经验ID
    /// * `relation_type` - 关联类型
    /// * `strength` - 关联强度
    ///
    /// # Returns
    /// 创建的关联实例
    pub fn add_relation(&self, experience_id: &str, related_experience_id: &str, relation_type: &str, strength: f64) -> Result<ExperienceRelation> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let relation = ExperienceRelation {
            id: id.clone(),
            experience_id: experience_id.to_string(),
            related_experience_id: related_experience_id.to_string(),
            relation_type: relation_type.to_string(),
            strength,
            created_at: now,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO experience_relations (id, experience_id, related_experience_id, relation_type, strength, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    relation.id,
                    relation.experience_id,
                    relation.related_experience_id,
                    relation.relation_type,
                    relation.strength,
                    relation.created_at,
                    serde_json::to_string(&relation.metadata)?,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(relation)
    }
    
    /// 获取相关经验
    ///
    /// # Arguments
    /// * `experience_id` - 经验ID
    ///
    /// # Returns
    /// 关联经验列表，包含关联信息和经验详情
    pub fn get_related_experiences(&self, experience_id: &str) -> Result<Vec<(ExperienceRelation, Experience)>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT r.id, r.experience_id, r.related_experience_id, r.relation_type, r.strength, r.created_at, r.metadata,
                        e.id, e.title, e.description, e.content, e.tags, e.category, e.difficulty_level, e.success_rate,
                        e.author_id, e.created_at, e.updated_at, e.used_count, e.last_used_at, e.rating, e.rating_count, e.metadata
                 FROM experience_relations r
                 INNER JOIN experiences e ON r.related_experience_id = e.id
                 WHERE r.experience_id = ?1
                 ORDER BY r.strength DESC",
            )?;
            
            let mut rows = stmt.query(params![experience_id])?;
            let mut results = Vec::new();
            
            while let Some(row) = rows.next()? {
                let relation = parse_relation_row(&row, 0)?;
                let experience = parse_experience_row_from_offset(&row, 7)?;
                results.push((relation, experience));
            }
            
            Ok(results)
        })
    }
}

fn parse_experience_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Experience> {
    let tags_raw: String = row.get(4)?;
    let tags: Vec<String> = serde_json::from_str(&tags_raw)
        .unwrap_or(vec![]);
    
    let metadata_raw: String = row.get(15)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(Experience {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        content: row.get(3)?,
        tags,
        category: row.get(5)?,
        difficulty_level: row.get(6)?,
        success_rate: row.get(7)?,
        author_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        used_count: row.get(11)?,
        last_used_at: row.get(12)?,
        rating: row.get(13)?,
        rating_count: row.get(14)?,
        metadata,
    })
}

fn parse_experience_row_from_offset(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<Experience> {
    let tags_raw: String = row.get(offset + 4)?;
    let tags: Vec<String> = serde_json::from_str(&tags_raw)
        .unwrap_or(vec![]);
    
    let metadata_raw: String = row.get(offset + 15)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(Experience {
        id: row.get(offset + 0)?,
        title: row.get(offset + 1)?,
        description: row.get(offset + 2)?,
        content: row.get(offset + 3)?,
        tags,
        category: row.get(offset + 5)?,
        difficulty_level: row.get(offset + 6)?,
        success_rate: row.get(offset + 7)?,
        author_id: row.get(offset + 8)?,
        created_at: row.get(offset + 9)?,
        updated_at: row.get(offset + 10)?,
        used_count: row.get(offset + 11)?,
        last_used_at: row.get(offset + 12)?,
        rating: row.get(offset + 13)?,
        rating_count: row.get(offset + 14)?,
        metadata,
    })
}

fn parse_relation_row(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<ExperienceRelation> {
    let metadata_raw: String = row.get(offset + 6)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(ExperienceRelation {
        id: row.get(offset + 0)?,
        experience_id: row.get(offset + 1)?,
        related_experience_id: row.get(offset + 2)?,
        relation_type: row.get(offset + 3)?,
        strength: row.get(offset + 4)?,
        created_at: row.get(offset + 5)?,
        metadata,
    })
}
