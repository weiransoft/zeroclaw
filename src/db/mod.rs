//! 数据库统一管理模块
//! 
//! 提供统一的数据库配置、连接池管理和 Schema 初始化功能
//! 
//! # 使用示例
//! 
//! ```rust
//! use crate::db::{DatabaseConfig, DbPoolManager, SqlLoader, DatabaseType};
//! use std::path::Path;
//! 
//! // 从工作区创建配置
//! let config = DatabaseConfig::from_workspace(Path::new("/path/to/workspace"));
//! 
//! // 创建连接池管理器
//! let pool_manager = DbPoolManager::new(&config)?;
//! 
//! // 获取指定数据库的连接池
//! let workflow_pool = pool_manager.get_pool("workflow")?;
//! 
//! // 加载并初始化 SQL Schema
//! let sql_loader = SqlLoader::default()?;
//! let schema = sql_loader.load_schema(DatabaseType::Workflow)?;
//! workflow_pool.init_schema(&schema)?;
//! 
//! // 执行数据库操作
//! workflow_pool.with_connection(|conn| {
//!     // 使用连接进行操作
//!     Ok(())
//! })?;
//! ```

pub mod config;
pub mod pool;
pub mod sql_loader;

// 重新导出常用类型
pub use config::SqliteConfig;
pub use pool::DbPool;
pub use sql_loader::{DatabaseType, SqlLoader};
