//! 工作流存储
//!
//! 提供工作流、工作流步骤和工作流模板的持久化存储功能

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

/// 工作流步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// 步骤唯一标识
    pub id: String,
    /// 步骤名称
    pub name: String,
    /// 步骤描述
    pub description: String,
    /// 步骤状态：pending, running, completed, failed
    pub status: String,
    /// 步骤顺序
    pub order: i32,
    /// 分配给的代理
    pub assigned_to: Option<String>,
    /// 开始时间戳
    pub started_at: Option<i64>,
    /// 完成时间戳
    pub completed_at: Option<i64>,
    /// 扩展元数据
    pub metadata: serde_json::Value,
}

/// 工作流
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// 工作流唯一标识
    pub id: String,
    /// 工作流名称
    pub name: String,
    /// 工作流描述
    pub description: String,
    /// 工作流状态：created, running, paused, completed, cancelled
    pub status: String,
    /// 关联角色列表
    pub roles: Vec<String>,
    /// 工作流步骤列表
    pub steps: Vec<WorkflowStep>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
    /// 创建者
    pub created_by: Option<String>,
    /// 扩展元数据
    pub metadata: serde_json::Value,
}

/// 工作流模板
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    /// 模板唯一标识
    pub id: String,
    /// 模板名称
    pub name: String,
    /// 模板描述
    pub description: String,
    /// 分类标签
    pub categories: Vec<String>,
    /// 适用场景
    pub applicable_scenarios: Vec<String>,
    /// 模板内容
    pub content: String,
    /// 模板步骤
    pub steps: Vec<WorkflowStep>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
    /// 扩展元数据
    pub metadata: serde_json::Value,
}

/// 工作流存储
///
/// 使用统一的数据库连接池管理工作流相关的持久化数据
#[derive(Debug)]
pub struct WorkflowStore {
    /// 数据库连接池
    pool: Arc<DbPool>,
}

impl WorkflowStore {
    /// 创建新的工作流存储
    ///
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    ///
    /// # Returns
    /// 初始化好的 WorkflowStore 实例
    pub fn new(workspace_dir: &Path) -> Result<Self> {
        // 使用统一配置创建连接池
        let config = SqliteConfig::from_workspace(workspace_dir, "workflow.db");
        let db_path = config.path.clone();
        let pool = Arc::new(DbPool::new(config.clone())?);
        
        tracing::debug!("[WorkflowStore] Initializing database at: {:?}", db_path);
        
        let store = Self { pool };
        
        // 使用 SQL 文件初始化 schema
        store.init_schema()?;
        
        Ok(store)
    }
    
    /// 从现有连接池创建工作流存储
    ///
    /// # Arguments
    /// * `pool` - 数据库连接池
    ///
    /// # Returns
    /// WorkflowStore 实例
    pub fn from_pool(pool: Arc<DbPool>) -> Result<Self> {
        let store = Self { pool };
        store.init_schema()?;
        Ok(store)
    }
    
    /// 初始化数据库 Schema
    ///
    /// 从 SQL 文件加载并执行数据库初始化脚本
    fn init_schema(&self) -> Result<()> {
        let loader = SqlLoader::default()?;
        let schema_sql = loader.load_schema(DatabaseType::Workflow)
            .context("Failed to load workflow schema")?;
        
        self.pool.init_schema(&schema_sql)
            .context("Failed to initialize workflow schema")?;
        
        Ok(())
    }
    
    /// 创建工作流
    ///
    /// # Arguments
    /// * `name` - 工作流名称
    /// * `description` - 工作流描述
    /// * `roles` - 关联角色列表
    /// * `created_by` - 创建者
    ///
    /// # Returns
    /// 创建的工作流实例
    pub fn create_workflow(&self, name: &str, description: &str, roles: Vec<String>, created_by: Option<String>) -> Result<Workflow> {
        // 输入验证
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Workflow name cannot be empty"));
        }
        
        // 检查重复性 - 验证名称是否已存在
        if self.get_workflow_by_name(name)?.is_some() {
            return Err(anyhow::anyhow!("Workflow with name '{}' already exists", name));
        }
        
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let workflow = Workflow {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            status: "created".to_string(),
            roles,
            steps: vec![],
            created_at: now,
            updated_at: now,
            created_by,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO workflows (id, name, description, status, created_at, updated_at, created_by, roles, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    workflow.id,
                    workflow.name,
                    workflow.description,
                    workflow.status,
                    workflow.created_at,
                    workflow.updated_at,
                    workflow.created_by,
                    serde_json::to_string(&workflow.roles)?,
                    serde_json::to_string(&workflow.metadata)?,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(workflow)
    }
    
    /// 获取工作流
    ///
    /// # Arguments
    /// * `id` - 工作流ID
    ///
    /// # Returns
    /// 工作流实例，如果不存在则返回 None
    pub fn get_workflow(&self, id: &str) -> Result<Option<Workflow>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, status, created_at, updated_at, created_by, roles, metadata
                 FROM workflows WHERE id = ?1",
                params![id],
                |row| parse_workflow_row(row, conn),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 根据名称获取工作流
    ///
    /// # Arguments
    /// * `name` - 工作流名称
    ///
    /// # Returns
    /// 工作流实例，如果不存在则返回 None
    pub fn get_workflow_by_name(&self, name: &str) -> Result<Option<Workflow>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, status, created_at, updated_at, created_by, roles, metadata
                 FROM workflows WHERE name = ?1",
                params![name],
                |row| parse_workflow_row(row, conn),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 列出所有工作流
    ///
    /// # Arguments
    /// * `limit` - 返回数量限制
    ///
    /// # Returns
    /// 工作流列表
    pub fn list_workflows(&self, limit: Option<usize>) -> Result<Vec<Workflow>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, status, created_at, updated_at, created_by, roles, metadata
                 FROM workflows ORDER BY updated_at DESC",
            )?;
            
            let mut rows = stmt.query([])?;
            let mut workflows = Vec::new();
            
            while let Some(row) = rows.next()? {
                workflows.push(parse_workflow_row(row, conn)?);
            }
            
            if let Some(limit) = limit {
                workflows.truncate(limit);
            }
            
            Ok(workflows)
        })
    }
    
    /// 更新工作流
    ///
    /// # Arguments
    /// * `id` - 工作流ID
    /// * `name` - 新名称（可选）
    /// * `description` - 新描述（可选）
    /// * `status` - 新状态（可选）
    /// * `roles` - 新角色列表（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_workflow(&self, id: &str, name: Option<&str>, description: Option<&str>, status: Option<&str>, roles: Option<Vec<String>>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE workflows SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE workflows SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            if let Some(status) = status {
                conn.execute(
                    "UPDATE workflows SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![status, now, id],
                )?;
            }
            
            if let Some(roles) = roles {
                conn.execute(
                    "UPDATE workflows SET roles = ?1, updated_at = ?2 WHERE id = ?3",
                    params![serde_json::to_string(&roles)?, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除工作流
    ///
    /// # Arguments
    /// * `id` - 工作流ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_workflow(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM workflows WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 添加步骤
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    /// * `name` - 步骤名称
    /// * `description` - 步骤描述
    /// * `order` - 步骤顺序
    ///
    /// # Returns
    /// 创建的步骤实例
    pub fn add_step(&self, workflow_id: &str, name: &str, description: &str, order: i32) -> Result<WorkflowStep> {
        let id = Uuid::new_v4().to_string();
        
        let step = WorkflowStep {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            status: "pending".to_string(),
            order,
            assigned_to: None,
            started_at: None,
            completed_at: None,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO workflow_steps (id, workflow_id, name, description, status, step_order, assigned_to, started_at, completed_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    step.id,
                    workflow_id,
                    step.name,
                    step.description,
                    step.status,
                    step.order,
                    step.assigned_to,
                    step.started_at,
                    step.completed_at,
                    serde_json::to_string(&step.metadata)?,
                ],
            )?;
            
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE workflows SET updated_at = ?1 WHERE id = ?2",
                params![now, workflow_id],
            )?;
            
            Ok(())
        })?;
        
        Ok(step)
    }
    
    /// 更新步骤
    ///
    /// # Arguments
    /// * `id` - 步骤ID
    /// * `name` - 新名称（可选）
    /// * `description` - 新描述（可选）
    /// * `status` - 新状态（可选）
    /// * `assigned_to` - 分配给的代理（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_step(&self, id: &str, name: Option<&str>, description: Option<&str>, status: Option<&str>, assigned_to: Option<&str>, metadata: Option<&str>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE workflow_steps SET name = ?1 WHERE id = ?2",
                    params![name, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE workflow_steps SET description = ?1 WHERE id = ?2",
                    params![description, id],
                )?;
            }
            
            if let Some(status) = status {
                let mut started_at: Option<i64> = None;
                let mut completed_at: Option<i64> = None;
                
                if status == "running" {
                    started_at = Some(now);
                } else if status == "completed" {
                    completed_at = Some(now);
                }
                
                conn.execute(
                    "UPDATE workflow_steps SET status = ?1, started_at = ?2, completed_at = ?3 WHERE id = ?4",
                    params![status, started_at, completed_at, id],
                )?;
            }
            
            if let Some(assigned_to) = assigned_to {
                conn.execute(
                    "UPDATE workflow_steps SET assigned_to = ?1 WHERE id = ?2",
                    params![assigned_to, id],
                )?;
            }
            
            if let Some(metadata) = metadata {
                conn.execute(
                    "UPDATE workflow_steps SET metadata = ?1 WHERE id = ?2",
                    params![metadata, id],
                )?;
            }
            
            if let Ok(workflow_id) = conn.query_row(
                "SELECT workflow_id FROM workflow_steps WHERE id = ?1",
                params![id],
                |row: &rusqlite::Row<'_>| row.get::<_, String>(0),
            ) {
                conn.execute(
                    "UPDATE workflows SET updated_at = ?1 WHERE id = ?2",
                    params![now, workflow_id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 根据步骤ID获取工作流ID
    /// 
    /// # Arguments
    /// * `step_id` - 步骤ID
    /// 
    /// # Returns
    /// 工作流ID
    pub fn get_workflow_id_by_step(&self, step_id: &str) -> Result<String> {
        self.pool.with_connection(|conn: &Connection| {
            let workflow_id: String = conn.query_row(
                "SELECT workflow_id FROM workflow_steps WHERE id = ?1",
                params![step_id],
                |row| row.get(0),
            )?;
            Ok(workflow_id)
        })
    }
    
    /// 创建模板
    ///
    /// # Arguments
    /// * `name` - 模板名称
    /// * `description` - 模板描述
    /// * `categories` - 分类标签
    /// * `applicable_scenarios` - 适用场景
    /// * `content` - 模板内容
    ///
    /// # Returns
    /// 创建的模板实例
    pub fn create_template(&self, name: &str, description: &str, categories: Vec<String>, applicable_scenarios: Vec<String>, content: &str) -> Result<WorkflowTemplate> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        
        let template = WorkflowTemplate {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            categories,
            applicable_scenarios,
            content: content.to_string(),
            steps: vec![],
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        };
        
        self.pool.with_connection(|conn: &Connection| {
            conn.execute(
                "INSERT INTO workflow_templates (id, name, description, categories, applicable_scenarios, content, created_at, updated_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    template.id,
                    template.name,
                    template.description,
                    serde_json::to_string(&template.categories)?,
                    serde_json::to_string(&template.applicable_scenarios)?,
                    template.content,
                    template.created_at,
                    template.updated_at,
                    serde_json::to_string(&template.metadata)?,
                ],
            )?;
            Ok(())
        })?;
        
        Ok(template)
    }
    
    /// 更新模板
    ///
    /// # Arguments
    /// * `id` - 模板ID
    /// * `name` - 新名称（可选）
    /// * `description` - 新描述（可选）
    /// * `categories` - 新分类标签（可选）
    /// * `applicable_scenarios` - 新适用场景（可选）
    /// * `content` - 新内容（可选）
    ///
    /// # Returns
    /// 操作结果
    pub fn update_template(&self, id: &str, name: Option<&str>, description: Option<&str>, categories: Option<Vec<String>>, applicable_scenarios: Option<Vec<String>>, content: Option<&str>) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            let now = chrono::Utc::now().timestamp_millis();
            
            if let Some(name) = name {
                conn.execute(
                    "UPDATE workflow_templates SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            
            if let Some(description) = description {
                conn.execute(
                    "UPDATE workflow_templates SET description = ?1, updated_at = ?2 WHERE id = ?3",
                    params![description, now, id],
                )?;
            }
            
            if let Some(categories) = categories {
                conn.execute(
                    "UPDATE workflow_templates SET categories = ?1, updated_at = ?2 WHERE id = ?3",
                    params![serde_json::to_string(&categories)?, now, id],
                )?;
            }
            
            if let Some(applicable_scenarios) = applicable_scenarios {
                conn.execute(
                    "UPDATE workflow_templates SET applicable_scenarios = ?1, updated_at = ?2 WHERE id = ?3",
                    params![serde_json::to_string(&applicable_scenarios)?, now, id],
                )?;
            }
            
            if let Some(content) = content {
                conn.execute(
                    "UPDATE workflow_templates SET content = ?1, updated_at = ?2 WHERE id = ?3",
                    params![content, now, id],
                )?;
            }
            
            Ok(())
        })
    }
    
    /// 删除模板
    ///
    /// # Arguments
    /// * `id` - 模板ID
    ///
    /// # Returns
    /// 操作结果
    pub fn delete_template(&self, id: &str) -> Result<()> {
        self.pool.with_connection(|conn: &Connection| {
            conn.execute("DELETE FROM workflow_templates WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
    
    /// 获取模板
    ///
    /// # Arguments
    /// * `id` - 模板ID
    ///
    /// # Returns
    /// 模板实例，如果不存在则返回 None
    pub fn get_template(&self, id: &str) -> Result<Option<WorkflowTemplate>> {
        self.pool.with_connection(|conn: &Connection| {
            conn.query_row(
                "SELECT id, name, description, categories, applicable_scenarios, content, created_at, updated_at, metadata
                 FROM workflow_templates WHERE id = ?1",
                params![id],
                |row| parse_template_row(row),
            )
            .optional()
            .map_err(|e: rusqlite::Error| anyhow::anyhow!(e))
        })
    }
    
    /// 列出所有模板
    ///
    /// # Returns
    /// 模板列表
    pub fn list_templates(&self) -> Result<Vec<WorkflowTemplate>> {
        self.pool.with_connection(|conn: &Connection| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, categories, applicable_scenarios, content, created_at, updated_at, metadata
                 FROM workflow_templates ORDER BY name",
            )?;
            
            let mut rows = stmt.query([])?;
            let mut templates = Vec::new();
            
            while let Some(row) = rows.next()? {
                templates.push(parse_template_row(row)?);
            }
            
            Ok(templates)
        })
    }
}

/// 解析工作流行数据
///
/// # Arguments
/// * `row` - 数据库行
/// * `conn` - 数据库连接
///
/// # Returns
/// 解析后的工作流实例
fn parse_workflow_row(row: &rusqlite::Row<'_>, conn: &Connection) -> rusqlite::Result<Workflow> {
    let id: String = row.get(0)?;
    
    // 查询关联的步骤
    let mut stmt = conn.prepare(
        "SELECT id, name, description, status, step_order, assigned_to, started_at, completed_at, metadata
         FROM workflow_steps WHERE workflow_id = ?1 ORDER BY step_order",
    )?;
    
    let mut steps = Vec::new();
    let mut step_rows = stmt.query(params![id])?;
    
    while let Some(step_row) = step_rows.next()? {
        steps.push(parse_step_row(step_row)?);
    }
    
    let roles_raw: String = row.get(7)?;
    let roles: Vec<String> = serde_json::from_str(&roles_raw)
        .unwrap_or(vec![]);
    
    let metadata_raw: String = row.get(8)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(Workflow {
        id,
        name: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        roles,
        steps,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        created_by: row.get(6)?,
        metadata,
    })
}

/// 解析步骤行数据
///
/// # Arguments
/// * `row` - 数据库行
///
/// # Returns
/// 解析后的步骤实例
fn parse_step_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowStep> {
    let metadata_raw: String = row.get(8)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(WorkflowStep {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        order: row.get(4)?,
        assigned_to: row.get(5)?,
        started_at: row.get(6)?,
        completed_at: row.get(7)?,
        metadata,
    })
}

/// 解析模板行数据
///
/// # Arguments
/// * `row` - 数据库行
///
/// # Returns
/// 解析后的模板实例
fn parse_template_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowTemplate> {
    let categories_raw: String = row.get(3)?;
    let categories: Vec<String> = serde_json::from_str(&categories_raw)
        .unwrap_or(vec![]);
    
    let scenarios_raw: String = row.get(4)?;
    let applicable_scenarios: Vec<String> = serde_json::from_str(&scenarios_raw)
        .unwrap_or(vec![]);
    
    let content: String = row.get(5)?;
    
    let metadata_raw: String = row.get(8)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_raw)
        .unwrap_or(serde_json::json!({}));
    
    Ok(WorkflowTemplate {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        categories,
        applicable_scenarios,
        content,
        steps: vec![],
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    /// 创建测试用的 WorkflowStore
    fn create_test_store() -> (TempDir, WorkflowStore) {
        let temp_dir = TempDir::new().unwrap();
        let store = WorkflowStore::new(temp_dir.path()).unwrap();
        (temp_dir, store)
    }
    
    #[test]
    fn test_create_workflow() {
        let (_temp_dir, store) = create_test_store();
        
        let workflow = store.create_workflow(
            "Test Workflow",
            "Test Description",
            vec!["role1".to_string(), "role2".to_string()],
            Some("user1".to_string()),
        ).unwrap();
        
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.description, "Test Description");
        assert_eq!(workflow.status, "created");
        assert_eq!(workflow.roles.len(), 2);
    }
    
    #[test]
    fn test_get_workflow() {
        let (_temp_dir, store) = create_test_store();
        
        let created = store.create_workflow(
            "Test Workflow",
            "Test Description",
            vec![],
            None,
        ).unwrap();
        
        let fetched = store.get_workflow(&created.id).unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "Test Workflow");
        
        let not_found = store.get_workflow("non-existent").unwrap();
        assert!(not_found.is_none());
    }
    
    #[test]
    fn test_update_workflow() {
        let (_temp_dir, store) = create_test_store();
        
        let workflow = store.create_workflow(
            "Original Name",
            "Original Description",
            vec![],
            None,
        ).unwrap();
        
        store.update_workflow(
            &workflow.id,
            Some("Updated Name"),
            Some("Updated Description"),
            Some("running"),
            None,
        ).unwrap();
        
        let updated = store.get_workflow(&workflow.id).unwrap().unwrap();
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.description, "Updated Description");
        assert_eq!(updated.status, "running");
    }
    
    #[test]
    fn test_delete_workflow() {
        let (_temp_dir, store) = create_test_store();
        
        let workflow = store.create_workflow(
            "Test Workflow",
            "Test Description",
            vec![],
            None,
        ).unwrap();
        
        store.delete_workflow(&workflow.id).unwrap();
        
        let deleted = store.get_workflow(&workflow.id).unwrap();
        assert!(deleted.is_none());
    }
    
    #[test]
    fn test_add_and_update_step() {
        let (_temp_dir, store) = create_test_store();
        
        let workflow = store.create_workflow(
            "Test Workflow",
            "Test Description",
            vec![],
            None,
        ).unwrap();
        
        let step = store.add_step(&workflow.id, "Step 1", "Step Description", 1).unwrap();
        assert_eq!(step.name, "Step 1");
        assert_eq!(step.status, "pending");
        
        store.update_step(&step.id, None, None, Some("running"), Some("agent1"), None).unwrap();
        
        let updated_workflow = store.get_workflow(&workflow.id).unwrap().unwrap();
        assert_eq!(updated_workflow.steps.len(), 1);
        assert_eq!(updated_workflow.steps[0].status, "running");
        assert_eq!(updated_workflow.steps[0].assigned_to, Some("agent1".to_string()));
    }
    
    #[test]
    fn test_template_crud() {
        let (_temp_dir, store) = create_test_store();
        
        // 创建模板
        let template = store.create_template(
            "Test Template",
            "Template Description",
            vec!["category1".to_string()],
            vec!["scenario1".to_string()],
            "Template Content",
        ).unwrap();
        
        assert_eq!(template.name, "Test Template");
        
        // 获取模板
        let fetched = store.get_template(&template.id).unwrap().unwrap();
        assert_eq!(fetched.name, "Test Template");
        
        // 更新模板
        store.update_template(
            &template.id,
            Some("Updated Template"),
            None,
            None,
            None,
            None,
        ).unwrap();
        
        let updated = store.get_template(&template.id).unwrap().unwrap();
        assert_eq!(updated.name, "Updated Template");
        
        // 列出模板
        let templates = store.list_templates().unwrap();
        assert_eq!(templates.len(), 1);
        
        // 删除模板
        store.delete_template(&template.id).unwrap();
        let deleted = store.get_template(&template.id).unwrap();
        assert!(deleted.is_none());
    }
}
