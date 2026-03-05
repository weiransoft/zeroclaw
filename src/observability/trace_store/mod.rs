//! Trace Store - 统一轨迹存储抽象
//!
//! 提供低侵入性的轨迹存储方案，支持 SQLite 和 DuckDB 两种后端。
//! 设计原则：
//! 1. 最小侵入性 - 通过 Observer trait 集成
//! 2. 高性能 - 批量写入、异步刷新
//! 3. 可扩展 - 支持多种存储后端

mod types;
mod store;
mod sqlite_store;
mod collector;

#[cfg(feature = "duckdb")]
mod duckdb_store;

pub use types::*;
pub use store::TraceStore;
pub use collector::{TraceCollector, TraceCollectorConfig};

#[cfg(feature = "duckdb")]
pub use duckdb_store::DuckdbTraceStore;

use std::sync::Arc;
use std::path::Path;

/// 创建默认的轨迹存储（SQLite）
pub fn create_trace_store(
    workspace_dir: &Path,
    config: TraceStoreConfig,
) -> Arc<dyn TraceStore> {
    Arc::new(sqlite_store::SqliteTraceStore::new(workspace_dir, config))
}

/// 存储配置
#[derive(Debug, Clone)]
pub struct TraceStoreConfig {
    /// 最大连接数
    pub max_connections: u32,
    /// 数据保留天数
    pub retention_days: u32,
    /// 批量写入阈值
    pub batch_size: usize,
    /// 是否启用压缩
    pub enable_compression: bool,
}

impl Default for TraceStoreConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
            retention_days: 30,
            batch_size: 100,
            enable_compression: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_trace_store() {
        let tmp = TempDir::new().unwrap();
        let store = create_trace_store(tmp.path(), TraceStoreConfig::default());
        assert_eq!(store.backend_name(), "sqlite");
    }
}
