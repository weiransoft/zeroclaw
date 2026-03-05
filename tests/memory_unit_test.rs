//! 单元测试：内存模块
//!
//! 测试内存相关功能，包括后端管理、文本分块、SQLite存储等

use zeroclaw::memory::{Memory, MemoryCategory, MemoryEntry, SqliteMemory, LucidMemory};
use tempfile::tempdir;
use std::path::Path;

#[tokio::test]
async fn test_memory_category_enum() {
    // 测试内存类别枚举
    let core_category = MemoryCategory::Core;
    let daily_category = MemoryCategory::Daily;
    let conversation_category = MemoryCategory::Conversation;
    let custom_category = MemoryCategory::Custom("test_category".to_string());
    
    assert!(matches!(core_category, MemoryCategory::Core));
    assert!(matches!(daily_category, MemoryCategory::Daily));
    assert!(matches!(conversation_category, MemoryCategory::Conversation));
    assert!(matches!(custom_category, MemoryCategory::Custom(_)));
}

#[tokio::test]
async fn test_memory_category_display() {
    // 测试内存类别的显示格式
    assert_eq!(MemoryCategory::Core.to_string(), "core");
    assert_eq!(MemoryCategory::Daily.to_string(), "daily");
    assert_eq!(MemoryCategory::Conversation.to_string(), "conversation");
    assert_eq!(MemoryCategory::Custom("project".to_string()).to_string(), "project");
}

#[tokio::test]
async fn test_memory_entry_creation() {
    // 测试内存条目的创建
    let entry = MemoryEntry {
        id: "test-id".to_string(),
        key: "test-key".to_string(),
        content: "test content".to_string(),
        category: MemoryCategory::Core,
        timestamp: chrono::Utc::now().to_rfc3339(),
        session_id: Some("session-123".to_string()),
        score: Some(0.85),
    };
    
    assert_eq!(entry.id, "test-id");
    assert_eq!(entry.key, "test-key");
    assert_eq!(entry.content, "test content");
    assert!(matches!(entry.category, MemoryCategory::Core));
    assert!(entry.session_id.is_some());
    assert_eq!(entry.score, Some(0.85));
}

#[tokio::test]
async fn test_sqlite_memory_creation() {
    // 测试SQLite内存的创建
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("memory.db");
    
    let memory = SqliteMemory::new(&db_path).expect("SQLite memory should be created");
    
    // 验证数据库文件被创建
    assert!(db_path.exists());
    
    // 验证初始状态
    let count = memory.count().await.expect("Count should succeed");
    assert_eq!(count, 0);
    
    // 验证健康检查
    assert!(memory.health_check().await);
}

#[tokio::test]
async fn test_sqlite_memory_store_and_recall() {
    // 测试SQLite内存的存储和召回功能
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("memory.db");
    let memory = SqliteMemory::new(&db_path).expect("SQLite memory should be created");
    
    // 存储一个条目
    let key = "test_key";
    let content = "This is test content for memory storage.";
    let category = MemoryCategory::Core;
    
    memory.store(key, content, category).await.expect("Store should succeed");
    
    // 验证计数增加
    let count = memory.count().await.expect("Count should succeed");
    assert_eq!(count, 1);
    
    // 召回条目
    let recalled = memory.recall("test", 10).await.expect("Recall should succeed");
    assert!(!recalled.is_empty());
    assert_eq!(recalled[0].key, key);
    assert_eq!(recalled[0].content, content);
}

#[tokio::test]
async fn test_sqlite_memory_get_specific() {
    // 测试获取特定键的内存
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("memory.db");
    let memory = SqliteMemory::new(&db_path).expect("SQLite memory should be created");
    
    // 存储一个条目
    let key = "specific_key";
    let content = "Content for specific key test.";
    let category = MemoryCategory::Daily;
    
    memory.store(key, content, category).await.expect("Store should succeed");
    
    // 获取特定键的条目
    let retrieved = memory.get(key).await.expect("Get should succeed");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.as_ref().unwrap().key, key);
    assert_eq!(retrieved.as_ref().unwrap().content, content);
    
    // 尝试获取不存在的键
    let non_existent = memory.get("nonexistent").await.expect("Get should succeed");
    assert!(non_existent.is_none());
}

#[tokio::test]
async fn test_sqlite_memory_list_by_category() {
    // 测试按类别列出内存
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("memory.db");
    let memory = SqliteMemory::new(&db_path).expect("SQLite memory should be created");
    
    // 存储不同类别的条目
    memory.store("core_key", "Core content", MemoryCategory::Core).await.expect("Store should succeed");
    memory.store("daily_key", "Daily content", MemoryCategory::Daily).await.expect("Store should succeed");
    memory.store("conv_key", "Conversation content", MemoryCategory::Conversation).await.expect("Store should succeed");
    
    // 列出特定类别的条目
    let core_entries = memory.list(Some(&MemoryCategory::Core)).await.expect("List should succeed");
    assert_eq!(core_entries.len(), 1);
    assert_eq!(core_entries[0].key, "core_key");
    
    let all_entries = memory.list(None).await.expect("List should succeed");
    assert_eq!(all_entries.len(), 3);
}

#[tokio::test]
async fn test_sqlite_memory_forget() {
    // 测试删除内存条目
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("memory.db");
    let memory = SqliteMemory::new(&db_path).expect("SQLite memory should be created");
    
    // 存储一个条目
    let key = "to_be_deleted";
    memory.store(key, "Content to delete", MemoryCategory::Core).await.expect("Store should succeed");
    
    // 验证条目存在
    let count_before = memory.count().await.expect("Count should succeed");
    assert_eq!(count_before, 1);
    
    let exists_before = memory.get(key).await.expect("Get should succeed").is_some();
    assert!(exists_before);
    
    // 删除条目
    let deleted = memory.forget(key).await.expect("Forget should succeed");
    assert!(deleted);
    
    // 验证条目已被删除
    let count_after = memory.count().await.expect("Count should succeed");
    assert_eq!(count_after, 0);
    
    let exists_after = memory.get(key).await.expect("Get should succeed").is_some();
    assert!(!exists_after);
}

#[tokio::test]
async fn test_lucid_memory_creation() {
    // 测试Lucid内存的创建
    let temp_dir = tempdir().unwrap();
    
    let memory = LucidMemory::new(temp_dir.path(), SqliteMemory::new(&temp_dir.path().join("local.db")).unwrap());
    
    // 验证基础目录结构被创建
    assert!(temp_dir.path().exists());
    
    // 验证健康检查
    assert!(memory.health_check().await);
}