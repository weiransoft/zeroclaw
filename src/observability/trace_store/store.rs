//! TraceStore trait - 统一存储抽象
//!
//! 定义轨迹存储的核心接口，支持多种后端实现

use crate::observability::trace_store::types::*;
use anyhow::Result;
use async_trait::async_trait;

/// 轨迹存储接口
///
/// 定义统一的存储抽象，支持 SQLite、DuckDB 等多种后端实现。
/// 所有方法都是异步的，支持高并发场景。
#[async_trait]
pub trait TraceStore: Send + Sync {
    // ============ 基础 CRUD ============
    
    /// 存储单条轨迹
    async fn store_trace(&self, trace: &AgentTrace) -> Result<()>;
    
    /// 批量存储轨迹（高性能）
    async fn store_traces_batch(&self, traces: &[AgentTrace]) -> Result<()>;
    
    /// 获取轨迹
    async fn get_trace(&self, id: &str) -> Result<Option<AgentTrace>>;
    
    /// 删除轨迹
    async fn delete_trace(&self, id: &str) -> Result<()>;
    
    /// 列出轨迹
    async fn list_traces(&self, query: &TraceQuery) -> Result<Vec<AgentTrace>>;
    
    /// 统计轨迹数量
    async fn count_traces(&self, query: &TraceQuery) -> Result<u64>;
    
    // ============ 推理链存储 ============
    
    /// 存储推理链
    async fn store_reasoning(&self, trace_id: &str, reasoning: &ReasoningChain) -> Result<()>;
    
    /// 获取推理链
    async fn get_reasoning(&self, trace_id: &str) -> Result<Option<ReasoningChain>>;
    
    // ============ 决策点存储 ============
    
    /// 存储决策点
    async fn store_decision(&self, trace_id: &str, decision: &DecisionPoint) -> Result<()>;
    
    /// 获取轨迹的所有决策点
    async fn get_decisions(&self, trace_id: &str) -> Result<Vec<DecisionPoint>>;
    
    // ============ 评估结果存储 ============
    
    /// 存储评估结果
    async fn store_evaluation(&self, trace_id: &str, evaluation: &EvaluationResult) -> Result<()>;
    
    /// 获取评估结果
    async fn get_evaluation(&self, trace_id: &str) -> Result<Option<EvaluationResult>>;
    
    // ============ 聚合查询 ============
    
    /// 聚合统计
    async fn aggregate(&self, query: AggregationQuery) -> Result<AggregationResult>;
    
    // ============ 维护操作 ============
    
    /// 清理过期数据
    async fn cleanup_expired(&self, retention_days: u32) -> Result<u64>;
    
    /// 获取存储统计
    async fn storage_stats(&self) -> Result<StorageStats>;
    
    /// 刷新缓冲区
    async fn flush(&self) -> Result<()>;
    
    /// 后端名称
    fn backend_name(&self) -> &'static str;
}

/// 同步版本的存储接口（用于不需要异步的场景）
pub trait TraceStoreSync: Send + Sync {
    /// 存储单条轨迹
    fn store_trace(&self, trace: &AgentTrace) -> Result<()>;
    
    /// 获取轨迹
    fn get_trace(&self, id: &str) -> Result<Option<AgentTrace>>;
    
    /// 列出轨迹
    fn list_traces(&self, query: &TraceQuery) -> Result<Vec<AgentTrace>>;
    
    /// 后端名称
    fn backend_name(&self) -> &'static str;
}

/// 测试用的内存存储
/// 仅用于测试场景，不适用于生产环境
#[cfg(test)]
pub struct InMemoryTraceStore {
    traces: std::sync::RwLock<std::collections::HashMap<String, AgentTrace>>,
}

#[cfg(test)]
impl InMemoryTraceStore {
    /// 创建新的内存存储
    pub fn new() -> Self {
        Self {
            traces: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl TraceStore for InMemoryTraceStore {
    async fn store_trace(&self, trace: &AgentTrace) -> Result<()> {
        let mut traces = self.traces.write().unwrap();
        traces.insert(trace.id.clone(), trace.clone());
        Ok(())
    }
    
    async fn store_traces_batch(&self, traces: &[AgentTrace]) -> Result<()> {
        let mut store = self.traces.write().unwrap();
        for trace in traces {
            store.insert(trace.id.clone(), trace.clone());
        }
        Ok(())
    }
    
    async fn get_trace(&self, id: &str) -> Result<Option<AgentTrace>> {
        let traces = self.traces.read().unwrap();
        Ok(traces.get(id).cloned())
    }
    
    async fn delete_trace(&self, id: &str) -> Result<()> {
        let mut traces = self.traces.write().unwrap();
        traces.remove(id);
        Ok(())
    }
    
    async fn list_traces(&self, _query: &TraceQuery) -> Result<Vec<AgentTrace>> {
        let traces = self.traces.read().unwrap();
        Ok(traces.values().cloned().collect())
    }
    
    async fn count_traces(&self, _query: &TraceQuery) -> Result<u64> {
        let traces = self.traces.read().unwrap();
        Ok(traces.len() as u64)
    }
    
    async fn store_reasoning(&self, _trace_id: &str, _reasoning: &ReasoningChain) -> Result<()> {
        Ok(())
    }
    
    async fn get_reasoning(&self, _trace_id: &str) -> Result<Option<ReasoningChain>> {
        Ok(None)
    }
    
    async fn store_decision(&self, _trace_id: &str, _decision: &DecisionPoint) -> Result<()> {
        Ok(())
    }
    
    async fn get_decisions(&self, _trace_id: &str) -> Result<Vec<DecisionPoint>> {
        Ok(vec![])
    }
    
    async fn store_evaluation(&self, _trace_id: &str, _evaluation: &EvaluationResult) -> Result<()> {
        Ok(())
    }
    
    async fn get_evaluation(&self, _trace_id: &str) -> Result<Option<EvaluationResult>> {
        Ok(None)
    }
    
    async fn aggregate(&self, _query: AggregationQuery) -> Result<AggregationResult> {
        Ok(AggregationResult::Unknown)
    }
    
    async fn cleanup_expired(&self, _retention_days: u32) -> Result<u64> {
        Ok(0)
    }
    
    async fn storage_stats(&self) -> Result<StorageStats> {
        let traces = self.traces.read().unwrap();
        Ok(StorageStats {
            total_traces: traces.len() as u64,
            total_reasoning_chains: 0,
            total_decisions: 0,
            total_evaluations: 0,
            db_size_bytes: 0,
            oldest_trace_timestamp: None,
            newest_trace_timestamp: None,
        })
    }
    
    async fn flush(&self) -> Result<()> {
        Ok(())
    }
    
    fn backend_name(&self) -> &'static str {
        "memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_in_memory_store() {
        let store = InMemoryTraceStore::new();
        
        let trace = AgentTrace {
            id: "test-1".to_string(),
            run_id: "run-1".to_string(),
            parent_id: None,
            timestamp: 1000,
            duration_ms: 500,
            trace_type: TraceType::UserMessage,
            input: TraceInput {
                content: "Hello".to_string(),
                content_type: InputContentType::Text,
                params: std::collections::HashMap::new(),
            },
            output: TraceOutput {
                content: "Hi".to_string(),
                success: true,
                error: None,
                tokens_used: None,
                cost_usd: None,
            },
            metadata: serde_json::json!({}),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        store.store_trace(&trace).await.unwrap();
        
        let retrieved = store.get_trace("test-1").await.unwrap();
        assert!(retrieved.is_some());
        
        let count = store.count_traces(&TraceQuery::default()).await.unwrap();
        assert_eq!(count, 1);
    }
}
