//! 数据库 SQL 加载器模块
//! 
//! 提供从项目 database/schemas 目录加载 SQL 初始化脚本的功能

use anyhow::{Context, Result};
use std::path::PathBuf;

/// 数据库类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    /// 大脑记忆数据库
    Brain,
    /// 追踪数据库
    Traces,
    /// Swarm 数据库
    Swarm,
    /// 工作流数据库
    Workflow,
    /// 灵魂数据库
    Soul,
    /// 经验数据库
    Experience,
    /// 知识数据库
    Knowledge,
    /// 团队数据库
    Team,
    /// 聊天数据库
    Chat,
}

impl DatabaseType {
    /// 获取数据库对应的 SQL 文件名
    /// 
    /// # Returns
    /// SQL 文件名（例如 "workflow.sql"）
    pub fn filename(&self) -> &'static str {
        match self {
            DatabaseType::Brain => "brain.sql",
            DatabaseType::Traces => "traces.sql",
            DatabaseType::Swarm => "swarm.sql",
            DatabaseType::Workflow => "workflow.sql",
            DatabaseType::Soul => "soul.sql",
            DatabaseType::Experience => "experience.sql",
            DatabaseType::Knowledge => "knowledge.sql",
            DatabaseType::Team => "team.sql",
            DatabaseType::Chat => "chat.sql",
        }
    }
    
    /// 获取数据库对应的名称标识
    /// 
    /// # Returns
    /// 数据库名称标识（例如 "workflow"）
    pub fn name(&self) -> &'static str {
        match self {
            DatabaseType::Brain => "brain",
            DatabaseType::Traces => "traces",
            DatabaseType::Swarm => "swarm",
            DatabaseType::Workflow => "workflow",
            DatabaseType::Soul => "soul",
            DatabaseType::Experience => "experience",
            DatabaseType::Knowledge => "knowledge",
            DatabaseType::Team => "team",
            DatabaseType::Chat => "chat",
        }
    }
}

/// SQL 加载器，负责从项目目录加载 SQL 脚本
pub struct SqlLoader {
    /// schemas 目录路径
    schemas_dir: PathBuf,
}

impl SqlLoader {
    /// 创建新的 SQL 加载器
    /// 
    /// # Arguments
    /// * `project_root` - 项目根目录路径
    /// 
    /// # Returns
    /// 初始化好的 SqlLoader
    pub fn new(project_root: PathBuf) -> Self {
        let schemas_dir = project_root.join("database").join("schemas");
        Self { schemas_dir }
    }
    
    /// 获取项目根目录的默认 SQL 加载器
    /// 通过 CARGO_MANIFEST_DIR 环境变量自动检测项目根目录
    /// 
    /// # Returns
    /// 初始化好的 SqlLoader
    pub fn default() -> Result<Self> {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Ok(Self::new(project_root))
    }
    
    /// 加载指定数据库类型的 SQL 脚本
    /// 
    /// # Arguments
    /// * `db_type` - 数据库类型
    /// 
    /// # Returns
    /// SQL 脚本内容字符串
    pub fn load_schema(&self, db_type: DatabaseType) -> Result<String> {
        let file_path = self.schemas_dir.join(db_type.filename());
        std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read SQL schema file: {:?}", file_path))
    }
    
    /// 加载所有数据库的 SQL 脚本
    /// 
    /// # Returns
    /// 包含所有数据库 SQL 脚本的 HashMap，键为数据库名称
    pub fn load_all_schemas(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut schemas = std::collections::HashMap::new();
        
        let all_types = [
            DatabaseType::Brain,
            DatabaseType::Traces,
            DatabaseType::Swarm,
            DatabaseType::Workflow,
            DatabaseType::Soul,
            DatabaseType::Experience,
            DatabaseType::Knowledge,
            DatabaseType::Team,
            DatabaseType::Chat,
        ];
        
        for db_type in all_types.iter() {
            let schema = self.load_schema(*db_type)?;
            schemas.insert(db_type.name().to_string(), schema);
        }
        
        Ok(schemas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_database_type_filename() {
        assert_eq!(DatabaseType::Workflow.filename(), "workflow.sql");
        assert_eq!(DatabaseType::Brain.filename(), "brain.sql");
        assert_eq!(DatabaseType::Chat.filename(), "chat.sql");
    }
    
    #[test]
    fn test_database_type_name() {
        assert_eq!(DatabaseType::Workflow.name(), "workflow");
        assert_eq!(DatabaseType::Brain.name(), "brain");
        assert_eq!(DatabaseType::Chat.name(), "chat");
    }
    
    #[test]
    fn test_sql_loader_default() {
        let loader = SqlLoader::default();
        assert!(loader.is_ok());
    }
    
    #[test]
    fn test_load_workflow_schema() {
        let loader = SqlLoader::default().unwrap();
        let schema = loader.load_schema(DatabaseType::Workflow);
        assert!(schema.is_ok());
        let schema_content = schema.unwrap();
        assert!(schema_content.contains("CREATE TABLE"));
    }
}
