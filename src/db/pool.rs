//! 数据库连接池管理模块
//! 
//! 提供统一的 SQLite 数据库连接池管理，支持连接获取、释放和 Schema 初始化

use crate::db::config::{SqliteConfig, DatabaseConfig};
use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// 数据库连接池错误类型
/// 
/// 用于区分不同类型的数据库错误，便于生产环境诊断和监控
#[derive(Debug, Error)]
pub enum DbPoolError {
    /// 连接池耗尽错误
    #[error("Database connection pool exhausted (max_size={max_size}) - consider increasing pool size or optimizing query performance")]
    PoolExhausted { 
        /// 连接池最大大小
        max_size: u32 
    },
    
    /// 连接超时错误
    #[error("Database connection timeout after {timeout_secs}s - check for long-running queries or deadlocks")]
    Timeout { 
        /// 超时时间（秒）
        timeout_secs: u64 
    },
    
    /// 数据库损坏错误
    #[error("Database corrupted: {reason} - database file may need recovery or restoration from backup")]
    Corrupted { 
        /// 损坏原因描述
        reason: String 
    },
    
    /// Schema 初始化错误
    #[error("Database schema initialization failed: {reason}")]
    SchemaError { 
        /// 错误原因
        reason: String 
    },
    
    /// 其他数据库错误（包装 rusqlite 错误）
    #[error("Database error: {0}")]
    Other(#[from] rusqlite::Error),
    
    /// 连接池构建错误
    #[error("Failed to create database connection pool: {0}")]
    PoolBuildError(#[from] r2d2::Error),
}

/// 单个数据库连接池包装
#[derive(Debug)]
pub struct DbPool {
    /// 连接池
    pool: Pool<SqliteConnectionManager>,
    /// 数据库配置
    config: SqliteConfig,
}

impl DbPool {
    /// 创建新的数据库连接池
    /// 
    /// # Arguments
    /// * `config` - SQLite 数据库配置
    /// 
    /// # Returns
    /// 初始化好的 DbPool
    /// 
    /// # Notes
    /// - 设置 min_size > 0 可以预热连接池，避免冷启动延迟
    /// - min_size 不能超过 max_size
    pub fn new(config: SqliteConfig) -> Result<Self> {
        // 确保数据库目录存在
        if let Some(parent) = config.path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create database directory: {:?}", parent))?;
            }
        }
        
        // 确保 min_size 不超过 max_size
        let min_size = config.min_size.min(config.max_size);
        
        // 创建连接池
        let manager = SqliteConnectionManager::file(config.path.clone());
        let pool = Pool::builder()
            .max_size(config.max_size)
            .min_idle(Some(min_size))
            .connection_timeout(Duration::from_secs(config.connection_timeout))
            .build(manager)
            .context("Failed to create database connection pool")?;
        
        // 记录连接池初始化信息
        tracing::debug!(
            "[DbPool] Initialized pool: path={:?}, max_size={}, min_idle={}",
            config.path, config.max_size, min_size
        );
        
        Ok(Self { pool, config })
    }
    
    /// 获取数据库连接
    /// 
    /// # Returns
    /// 从连接池获取的连接
    /// 
    /// # Errors
    /// 返回 `DbPoolError` 类型错误，包含以下可能：
    /// - `DbPoolError::PoolExhausted`: 连接池耗尽
    /// - `DbPoolError::Timeout`: 连接超时
    /// - `DbPoolError::Other`: 其他数据库错误
    pub fn get_connection(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        let start = std::time::Instant::now();
        
        // 使用带超时的连接获取，以便区分超时和池耗尽
        let conn = match self.pool.get_timeout(Duration::from_secs(self.config.connection_timeout)) {
            Ok(c) => c,
            Err(e) => {
                let error_msg = e.to_string();
                // 根据错误信息分类
                if error_msg.contains("timed out") || error_msg.contains("timeout") {
                    tracing::warn!(
                        "[DbPool] Connection timeout after {}s",
                        self.config.connection_timeout
                    );
                    return Err(DbPoolError::Timeout { 
                        timeout_secs: self.config.connection_timeout 
                    }.into());
                }
                // 默认视为连接池耗尽
                tracing::warn!(
                    "[DbPool] Connection pool exhausted (max_size={})",
                    self.config.max_size
                );
                return Err(DbPoolError::PoolExhausted { 
                    max_size: self.config.max_size 
                }.into());
            }
        };
        
        let pool_wait_ms = start.elapsed().as_millis();
        if pool_wait_ms > 10 {
            tracing::trace!("[DbPool] Long pool wait - wait_ms: {}", pool_wait_ms);
        }
        
        // 设置连接参数
        conn.busy_timeout(Duration::from_secs(self.config.connection_timeout))
            .context("Failed to set DB busy_timeout")?;
        
        Ok(conn)
    }
    
    /// 执行数据库操作（带连接获取）
    /// 
    /// # Arguments
    /// * `f` - 数据库操作闭包
    /// 
    /// # Returns
    /// 操作结果
    pub fn with_connection<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let start = std::time::Instant::now();
        let conn = self.get_connection()?;
        
        let result = f(&conn);
        let total_ms = start.elapsed().as_millis();
        if total_ms > 50 {
            tracing::trace!("[DbPool] Slow operation - total_ms: {}", total_ms);
        }
        
        result
    }
    
    /// 初始化数据库 Schema
    /// 
    /// # Arguments
    /// * `schema_sql` - Schema SQL 语句
    /// 
    /// # Returns
    /// 操作结果
    pub fn init_schema(&self, schema_sql: &str) -> Result<()> {
        self.with_connection(|conn| {
            // 设置性能优化 PRAGMA
            let mut pragma_sql = String::new();
            if self.config.wal_mode {
                pragma_sql.push_str("PRAGMA journal_mode = WAL;");
            }
            if self.config.synchronous {
                pragma_sql.push_str("PRAGMA synchronous = NORMAL;");
            }
            pragma_sql.push_str("PRAGMA mmap_size = 8388608;");
            pragma_sql.push_str("PRAGMA cache_size = -2000;");
            pragma_sql.push_str("PRAGMA temp_store = MEMORY;");
            if self.config.foreign_keys {
                pragma_sql.push_str("PRAGMA foreign_keys = ON;");
            }
            
            conn.execute_batch(&pragma_sql)?;
            
            // 执行 Schema 初始化
            conn.execute_batch(schema_sql)?;
            
            Ok(())
        })
    }
}

/// 数据库连接池管理器，管理多个数据库的连接池
pub struct DbPoolManager {
    /// 各数据库的连接池
    pools: HashMap<String, Arc<DbPool>>,
}

impl DbPoolManager {
    /// 创建新的连接池管理器
    /// 
    /// # Arguments
    /// * `config` - 数据库全局配置
    /// 
    /// # Returns
    /// 初始化好的 DbPoolManager
    pub fn new(config: &DatabaseConfig) -> Result<Self> {
        let mut pools = HashMap::new();
        
        // 创建各个数据库的连接池
        pools.insert("brain".to_string(), Arc::new(DbPool::new(config.brain.clone())?));
        pools.insert("traces".to_string(), Arc::new(DbPool::new(config.traces.clone())?));
        pools.insert("swarm".to_string(), Arc::new(DbPool::new(config.swarm.clone())?));
        pools.insert("workflow".to_string(), Arc::new(DbPool::new(config.workflow.clone())?));
        pools.insert("soul".to_string(), Arc::new(DbPool::new(config.soul.clone())?));
        pools.insert("experience".to_string(), Arc::new(DbPool::new(config.experience.clone())?));
        pools.insert("knowledge".to_string(), Arc::new(DbPool::new(config.knowledge.clone())?));
        pools.insert("team".to_string(), Arc::new(DbPool::new(config.team.clone())?));
        pools.insert("chat".to_string(), Arc::new(DbPool::new(config.chat.clone())?));
        
        Ok(Self { pools })
    }
    
    /// 获取指定数据库的连接池
    /// 
    /// # Arguments
    /// * `name` - 数据库名称
    /// 
    /// # Returns
    /// 数据库连接池
    pub fn get_pool(&self, name: &str) -> Result<Arc<DbPool>> {
        self.pools.get(name)
            .cloned()
            .with_context(|| format!("Database pool not found: {}", name))
    }
    
    /// 执行指定数据库的操作
    /// 
    /// # Arguments
    /// * `name` - 数据库名称
    /// * `f` - 数据库操作闭包
    /// 
    /// # Returns
    /// 操作结果
    pub fn with_connection<F, T>(&self, name: &str, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let pool = self.get_pool(name)?;
        pool.with_connection(f)
    }
}
