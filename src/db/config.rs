//! 数据库配置模块
//! 
//! 提供统一的 SQLite 数据库配置结构，支持从配置文件读取和默认配置

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// SQLite 数据库单个配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// 数据库文件路径
    pub path: PathBuf,
    /// 连接池最大连接数，默认 5
    pub max_size: u32,
    /// 连接池最小连接数（预热连接），默认 1
    /// 
    /// 设置 min_size > 0 可以避免冷启动延迟，连接池会在初始化时预热指定数量的连接
    pub min_size: u32,
    /// 连接超时时间（秒），默认 5
    pub connection_timeout: u64,
    /// 是否启用 WAL 模式，默认 true
    pub wal_mode: bool,
    /// 是否启用外键约束，默认 true
    pub foreign_keys: bool,
    /// 是否启用同步，默认 true
    pub synchronous: bool,
}

impl Default for SqliteConfig {
    /// 创建默认的 SQLite 配置
    fn default() -> Self {
        Self {
            path: PathBuf::from(":memory:"),
            max_size: 5,
            min_size: 1,
            connection_timeout: 5,
            wal_mode: true,
            foreign_keys: true,
            synchronous: true,
        }
    }
}

impl SqliteConfig {
    /// 从工作区目录创建 SQLite 配置
    /// 
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    /// * `db_filename` - 数据库文件名（例如 "workflow.db"）
    /// 
    /// # Returns
    /// 配置好的 SqliteConfig
    pub fn from_workspace(workspace_dir: &Path, db_filename: &str) -> Self {
        let db_path = workspace_dir.join(".zeroclaw").join(db_filename);
        
        Self {
            path: db_path,
            ..Default::default()
        }
    }
}

/// 数据库全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// 大脑记忆数据库
    pub brain: SqliteConfig,
    /// 追踪数据库
    pub traces: SqliteConfig,
    /// Swarm 数据库
    pub swarm: SqliteConfig,
    /// 工作流数据库
    pub workflow: SqliteConfig,
    /// 灵魂数据库
    pub soul: SqliteConfig,
    /// 经验数据库
    pub experience: SqliteConfig,
    /// 知识数据库
    pub knowledge: SqliteConfig,
    /// 团队数据库
    pub team: SqliteConfig,
    /// 聊天数据库
    pub chat: SqliteConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            brain: SqliteConfig::default(),
            traces: SqliteConfig::default(),
            swarm: SqliteConfig::default(),
            workflow: SqliteConfig::default(),
            soul: SqliteConfig::default(),
            experience: SqliteConfig::default(),
            knowledge: SqliteConfig::default(),
            team: SqliteConfig::default(),
            chat: SqliteConfig::default(),
        }
    }
}

impl DatabaseConfig {
    /// 从工作区目录创建默认的数据库配置
    /// 
    /// # Arguments
    /// * `workspace_dir` - 工作区目录路径
    /// 
    /// # Returns
    /// 配置好的 DatabaseConfig
    pub fn from_workspace(workspace_dir: &Path) -> Self {
        Self {
            brain: SqliteConfig {
                path: workspace_dir.join("memory").join("brain.db"),
                ..Default::default()
            },
            traces: SqliteConfig::from_workspace(workspace_dir, "traces.db"),
            swarm: SqliteConfig::from_workspace(workspace_dir, "swarm.db"),
            workflow: SqliteConfig::from_workspace(workspace_dir, "workflow.db"),
            soul: SqliteConfig::from_workspace(workspace_dir, "soul.db"),
            experience: SqliteConfig::from_workspace(workspace_dir, "experience.db"),
            knowledge: SqliteConfig::from_workspace(workspace_dir, "knowledge.db"),
            team: SqliteConfig::from_workspace(workspace_dir, "team.db"),
            chat: SqliteConfig::from_workspace(workspace_dir, "chat.db"),
        }
    }
}
