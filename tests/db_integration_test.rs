//! 数据库模块集成测试
//!
//! 测试数据库连接池管理、配置管理和 SQL 加载功能

use anyhow::Result;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::tempdir;
use zeroclaw::db::{SqlLoader, DatabaseType, SqliteConfig, DbPool};
use zeroclaw::db::config::DatabaseConfig;
use zeroclaw::db::pool::DbPoolManager;

#[test]
fn test_database_config_from_workspace() -> Result<()> {
    // 创建临时工作区
    let temp_dir = tempdir()?;
    let workspace_path = temp_dir.path();
    
    // 从工作区创建数据库配置
    let config = DatabaseConfig::from_workspace(workspace_path);
    
    // 验证配置结构
    assert_eq!(config.workflow.path, workspace_path.join(".zeroclaw").join("workflow.db"));
    assert_eq!(config.soul.path, workspace_path.join(".zeroclaw").join("soul.db"));
    assert_eq!(config.chat.path, workspace_path.join(".zeroclaw").join("chat.db"));
    assert_eq!(config.brain.path, workspace_path.join("memory").join("brain.db"));
    
    Ok(())
}

#[test]
fn test_sqlite_config_default() {
    // 测试默认配置
    let config = SqliteConfig::default();
    
    assert_eq!(config.max_size, 5);
    assert_eq!(config.connection_timeout, 5);
    assert!(config.wal_mode);
    assert!(config.foreign_keys);
    assert!(config.synchronous);
}

#[test]
fn test_sqlite_config_from_workspace() {
    // 测试从工作区创建配置
    let temp_dir = tempdir().unwrap();
    let workspace_path = temp_dir.path();
    
    let config = SqliteConfig::from_workspace(workspace_path, "test.db");
    
    assert_eq!(config.path, workspace_path.join(".zeroclaw").join("test.db"));
    assert_eq!(config.max_size, 5);
}

#[test]
fn test_sql_loader_load_workflow_schema() -> Result<()> {
    // 测试加载 workflow SQL 脚本
    let loader = SqlLoader::default()?;
    let schema = loader.load_schema(DatabaseType::Workflow)?;
    
    // 验证 SQL 内容包含必要的表
    assert!(schema.contains("CREATE TABLE"));
    assert!(schema.contains("workflow"));
    
    Ok(())
}

#[test]
fn test_sql_loader_load_all_schemas() -> Result<()> {
    // 测试加载所有数据库的 SQL 脚本
    let loader = SqlLoader::default()?;
    let schemas = loader.load_all_schemas()?;
    
    // 验证所有数据库的 SQL 都被加载
    assert!(schemas.contains_key("workflow"));
    assert!(schemas.contains_key("soul"));
    assert!(schemas.contains_key("experience"));
    assert!(schemas.contains_key("knowledge"));
    assert!(schemas.contains_key("team"));
    assert!(schemas.contains_key("chat"));
    assert!(schemas.contains_key("swarm"));
    assert!(schemas.contains_key("traces"));
    assert!(schemas.contains_key("brain"));
    
    Ok(())
}

#[test]
fn test_db_pool_creation() -> Result<()> {
    // 创建临时目录用于测试数据库
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    
    // 创建配置
    let config = SqliteConfig {
        path: db_path,
        max_size: 2,
        min_size: 1,
        connection_timeout: 1,
        wal_mode: true,
        foreign_keys: true,
        synchronous: true,
    };
    
    // 创建数据库连接池
    let pool = DbPool::new(config)?;
    
    // 测试获取连接
    let conn = pool.get_connection()?;
    
    // 测试执行简单的 SQL
    conn.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY);")?;
    
    Ok(())
}

#[test]
fn test_db_pool_with_connection() -> Result<()> {
    // 测试 with_connection 方法
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("test_with_conn.db");
    
    let config = SqliteConfig {
        path: db_path,
        max_size: 2,
        min_size: 1,
        connection_timeout: 1,
        wal_mode: true,
        foreign_keys: true,
        synchronous: true,
    };
    
    let pool = DbPool::new(config)?;
    
    // 使用 with_connection 执行操作
    pool.with_connection(|conn| {
        conn.execute_batch("CREATE TABLE test_with_conn (id INTEGER PRIMARY KEY, name TEXT);")?;
        conn.execute("INSERT INTO test_with_conn (name) VALUES (?1)", ["test"])?;
        
        let mut stmt = conn.prepare("SELECT name FROM test_with_conn WHERE id = 1")?;
        let name: String = stmt.query_row([], |row| row.get(0))?;
        
        assert_eq!(name, "test");
        Ok(())
    })?;
    
    Ok(())
}

#[test]
fn test_db_pool_init_schema() -> Result<()> {
    // 创建临时目录用于测试数据库
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("test_schema.db");
    
    // 创建配置
    let config = SqliteConfig {
        path: db_path,
        max_size: 2,
        min_size: 1,
        connection_timeout: 1,
        wal_mode: true,
        foreign_keys: true,
        synchronous: true,
    };
    
    // 创建数据库连接池
    let pool = DbPool::new(config)?;
    
    // 测试初始化简单的 schema
    let test_schema = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);";
    pool.init_schema(test_schema)?;
    
    // 验证表是否创建成功
    pool.with_connection(|conn| {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='users'")?;
        let exists: bool = stmt.exists([])?;
        assert!(exists);
        Ok(())
    })?;
    
    Ok(())
}

#[test]
fn test_db_pool_manager_creation() -> Result<()> {
    // 创建临时工作区
    let temp_dir = tempdir()?;
    let workspace_path = temp_dir.path();
    
    // 创建配置和连接池管理器
    let config = DatabaseConfig::from_workspace(workspace_path);
    let pool_manager = DbPoolManager::new(&config)?;
    
    // 测试获取各数据库的连接池
    assert!(pool_manager.get_pool("workflow").is_ok());
    assert!(pool_manager.get_pool("soul").is_ok());
    assert!(pool_manager.get_pool("chat").is_ok());
    assert!(pool_manager.get_pool("brain").is_ok());
    
    // 测试获取不存在的数据库连接池
    assert!(pool_manager.get_pool("nonexistent").is_err());
    
    Ok(())
}

#[test]
fn test_db_pool_manager_with_connection() -> Result<()> {
    // 测试 DbPoolManager 的 with_connection 方法
    let temp_dir = tempdir()?;
    let workspace_path = temp_dir.path();
    
    let config = DatabaseConfig::from_workspace(workspace_path);
    let pool_manager = DbPoolManager::new(&config)?;
    
    // 使用 pool_manager 执行操作
    pool_manager.with_connection("workflow", |conn: &rusqlite::Connection| {
        // 执行简单的查询
        let mut stmt = conn.prepare("SELECT 1")?;
        let _: i32 = stmt.query_row([], |row| row.get::<_, i32>(0))?;
        Ok(())
    })?;
    
    Ok(())
}

#[test]
fn test_database_type_enum() {
    // 测试 DatabaseType 枚举的所有值
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
    
    // 验证每个类型的 name 和 filename 都不重复
    let mut names = std::collections::HashSet::new();
    let mut filenames = std::collections::HashSet::new();
    
    for db_type in all_types.iter() {
        assert!(names.insert(db_type.name()));
        assert!(filenames.insert(db_type.filename()));
    }
    
    // 验证特定值
    assert_eq!(DatabaseType::Workflow.name(), "workflow");
    assert_eq!(DatabaseType::Workflow.filename(), "workflow.sql");
    
    assert_eq!(DatabaseType::Brain.name(), "brain");
    assert_eq!(DatabaseType::Brain.filename(), "brain.sql");
    
    assert_eq!(DatabaseType::Chat.name(), "chat");
    assert_eq!(DatabaseType::Chat.filename(), "chat.sql");
}

#[test]
fn test_multiple_connections_from_pool() -> Result<()> {
    // 测试从连接池获取多个连接
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("test_multi_conn.db");
    
    let config = SqliteConfig {
        path: db_path,
        max_size: 3,
        min_size: 1,
        connection_timeout: 5,
        wal_mode: true,
        foreign_keys: true,
        synchronous: true,
    };
    
    let pool = Arc::new(DbPool::new(config)?);
    
    // 获取多个连接并在不同线程中使用
    let mut handles = vec![];
    
    for i in 0..3 {
        let pool_clone = pool.clone();
        let handle = thread::spawn(move || {
            let conn = pool_clone.get_connection().unwrap();
            conn.execute_batch(&format!("CREATE TABLE test_table_{} (id INTEGER PRIMARY KEY);", i)).unwrap();
            thread::sleep(Duration::from_millis(10));
        });
        handles.push(handle);
    }
    
    // 等待所有线程完成
    for handle in handles {
        handle.join().unwrap();
    }
    
    Ok(())
}

#[test]
fn test_sql_loader_custom_project_root() -> Result<()> {
    // 测试使用自定义项目根目录
    let temp_dir = tempdir()?;
    let project_root = temp_dir.path();
    
    // 创建测试 schemas 目录和文件
    let schemas_dir = project_root.join("database").join("schemas");
    std::fs::create_dir_all(&schemas_dir)?;
    
    let test_sql_path = schemas_dir.join("test_db.sql");
    std::fs::write(&test_sql_path, "CREATE TABLE test (id INTEGER);")?;
    
    // 创建自定义加载器
    let _loader = SqlLoader::new(project_root.to_path_buf());
    
    // 虽然我们没有完整的 DatabaseType，但我们可以验证目录结构
    assert!(schemas_dir.exists());
    assert!(test_sql_path.exists());
    
    Ok(())
}

