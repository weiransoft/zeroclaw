//! DuckDB 轨迹存储实现
//!
//! 使用 DuckDB 作为存储后端，适合大规模轨迹分析场景。
//! DuckDB 是嵌入式列式数据库，提供优秀的分析查询性能。
//! 
//! 特点：
//! 1. 列式存储 - 分析查询性能优异
//! 2. 向量化执行 - 批量操作高效
//! 3. SQL 兼容 - 支持复杂聚合查询
//! 4. 嵌入式 - 无需独立服务进程

use crate::observability::trace_store::types::*;
use crate::observability::trace_store::store::TraceStore;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::TraceStoreConfig;

/// DuckDB 轨迹存储
/// 
/// 使用 DuckDB 作为后端，适合大规模轨迹分析场景。
/// 注意：DuckDB 不支持高并发写入，建议用于分析场景或低频写入场景。
pub struct DuckdbTraceStore {
    /// 数据库路径
    db_path: std::path::PathBuf,
    /// DuckDB 连接（使用互斥锁保护）
    conn: Arc<RwLock<Option<duckdb::Connection>>>,
    /// 配置
    config: TraceStoreConfig,
    /// 写入缓冲区
    buffer: Arc<RwLock<Vec<AgentTrace>>>,
}

impl DuckdbTraceStore {
    /// 创建新的 DuckDB 轨迹存储
    pub fn new(workspace_dir: &Path, config: TraceStoreConfig) -> Self {
        let db_path = workspace_dir.join(".zeroclaw").join("traces.duckdb");
        
        // 确保目录存在
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        
        let store = Self {
            db_path: db_path.clone(),
            conn: Arc::new(RwLock::new(None)),
            config: config.clone(),
            buffer: Arc::new(RwLock::new(Vec::with_capacity(config.batch_size))),
        };
        
        // 初始化连接和 Schema
        if let Err(e) = store.init_connection() {
            tracing::error!("Failed to initialize DuckDB connection: {}", e);
        }
        
        store
    }
    
    /// 初始化连接
    fn init_connection(&self) -> Result<()> {
        let conn = duckdb::Connection::open(&self.db_path)
            .context("Failed to open DuckDB database")?;
        
        // 性能优化配置
        conn.execute_batch(
            "SET threads = 4;
             SET memory_limit = '1GB';
             SET max_memory = '1GB';"
        ).context("Failed to set DuckDB options")?;
        
        // 创建 Schema
        conn.execute_batch(&Self::get_schema_sql())
            .context("Failed to create DuckDB schema")?;
        
        // 存储连接
        let mut conn_guard = self.conn.blocking_write();
        *conn_guard = Some(conn);
        
        Ok(())
    }
    
    /// 获取 Schema SQL
    fn get_schema_sql() -> String {
        r#"
        -- 主轨迹表
        CREATE TABLE IF NOT EXISTS agent_traces (
            id              VARCHAR PRIMARY KEY,
            run_id          VARCHAR NOT NULL,
            parent_id       VARCHAR,
            timestamp       BIGINT NOT NULL,
            duration_ms     BIGINT,
            trace_type      VARCHAR NOT NULL,
            input           VARCHAR NOT NULL,
            output          VARCHAR NOT NULL,
            metadata        VARCHAR NOT NULL DEFAULT '{}',
            created_at      TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        -- 轨迹索引
        CREATE INDEX IF NOT EXISTS idx_traces_run_id ON agent_traces(run_id);
        CREATE INDEX IF NOT EXISTS idx_traces_timestamp ON agent_traces(timestamp);
        CREATE INDEX IF NOT EXISTS idx_traces_type ON agent_traces(trace_type);

        -- 推理链表
        CREATE TABLE IF NOT EXISTS reasoning_chains (
            id              VARCHAR PRIMARY KEY,
            trace_id        VARCHAR NOT NULL UNIQUE,
            steps           VARCHAR NOT NULL,
            conclusion      VARCHAR,
            confidence      DOUBLE,
            quality_score   DOUBLE,
            created_at      TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        -- 决策点表
        CREATE TABLE IF NOT EXISTS decision_points (
            id                  VARCHAR PRIMARY KEY,
            trace_id            VARCHAR NOT NULL,
            decision_type       VARCHAR NOT NULL,
            description         VARCHAR NOT NULL,
            alternatives        VARCHAR NOT NULL,
            chosen_alternative_id VARCHAR NOT NULL,
            rationale           VARCHAR,
            quality_score       DOUBLE,
            is_optimal          BOOLEAN NOT NULL DEFAULT false,
            timestamp           BIGINT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_decisions_trace ON decision_points(trace_id);

        -- 评估结果表
        CREATE TABLE IF NOT EXISTS evaluation_results (
            id                  VARCHAR PRIMARY KEY,
            trace_id            VARCHAR NOT NULL UNIQUE,
            overall_score       DOUBLE NOT NULL,
            decision_scores     VARCHAR,
            reasoning_quality   DOUBLE,
            efficiency_score    DOUBLE,
            error_rate          DOUBLE,
            suggestions         VARCHAR,
            evaluated_at        BIGINT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_eval_trace ON evaluation_results(trace_id);

        -- 聚合视图：每日统计
        CREATE OR REPLACE VIEW daily_stats AS
        SELECT 
            DATE_TRUNC('day', TO_TIMESTAMP(timestamp)) as day,
            trace_type,
            COUNT(*) as total_count,
            SUM(CASE WHEN json_extract_string(output, '$.success') = 'true' THEN 1 ELSE 0 END) as success_count,
            AVG(duration_ms) as avg_duration_ms,
            SUM(CAST(json_extract_string(output, '$.tokens_used.total_tokens') AS BIGINT)) as total_tokens,
            SUM(CAST(json_extract_string(output, '$.cost_usd') AS DOUBLE)) as total_cost
        FROM agent_traces
        GROUP BY day, trace_type;

        -- 聚合视图：运行统计
        CREATE OR REPLACE VIEW run_stats AS
        SELECT 
            run_id,
            COUNT(*) as trace_count,
            MIN(timestamp) as start_time,
            MAX(timestamp) as end_time,
            SUM(CASE WHEN json_extract_string(output, '$.success') = 'true' THEN 1 ELSE 0 END) as success_count,
            SUM(CASE WHEN json_extract_string(output, '$.success') = 'false' THEN 1 ELSE 0 END) as error_count,
            SUM(duration_ms) as total_duration_ms,
            SUM(CAST(json_extract_string(output, '$.tokens_used.total_tokens') AS BIGINT)) as total_tokens,
            SUM(CAST(json_extract_string(output, '$.cost_usd') AS DOUBLE)) as total_cost
        FROM agent_traces
        GROUP BY run_id;
        "#.to_string()
    }
    
    /// 获取连接（阻塞版本）
    fn get_connection(&self) -> Result<duckdb::Connection> {
        let conn_guard = self.conn.blocking_read();
        if conn_guard.is_none() {
            return Err(anyhow::anyhow!("DuckDB connection not initialized"));
        }
        
        // DuckDB Connection 不支持 Clone，需要重新打开
        drop(conn_guard);
        duckdb::Connection::open(&self.db_path)
            .context("Failed to open DuckDB connection")
    }
    
    /// 将轨迹序列化为 JSON
    fn serialize_trace(trace: &AgentTrace) -> Result<(String, String, String, String)> {
        let trace_type = serde_json::to_string(&trace.trace_type)
            .context("Failed to serialize trace_type")?;
        let input = serde_json::to_string(&trace.input)
            .context("Failed to serialize input")?;
        let output = serde_json::to_string(&trace.output)
            .context("Failed to serialize output")?;
        let metadata = serde_json::to_string(&trace.metadata)
            .context("Failed to serialize metadata")?;
        Ok((trace_type, input, output, metadata))
    }
    
    /// 从数据库行解析轨迹
    fn parse_trace(row: &duckdb::Row) -> Result<AgentTrace, duckdb::Error> {
        let id: String = row.get(0)?;
        let run_id: String = row.get(1)?;
        let parent_id: Option<String> = row.get(2)?;
        let timestamp: i64 = row.get(3)?;
        let duration_ms: i64 = row.get(4)?;
        let trace_type_str: String = row.get(5)?;
        let input_str: String = row.get(6)?;
        let output_str: String = row.get(7)?;
        let metadata_str: String = row.get(8)?;
        
        let trace_type: TraceType = serde_json::from_str(&trace_type_str).unwrap_or(TraceType::UserMessage);
        let input: TraceInput = serde_json::from_str(&input_str).unwrap_or_else(|_| TraceInput {
            content: String::new(),
            content_type: InputContentType::Text,
            params: std::collections::HashMap::new(),
        });
        let output: TraceOutput = serde_json::from_str(&output_str).unwrap_or_else(|_| TraceOutput {
            content: String::new(),
            success: false,
            error: None,
            tokens_used: None,
            cost_usd: None,
        });
        let metadata: serde_json::Value = serde_json::from_str(&metadata_str).unwrap_or(serde_json::json!({}));
        
        Ok(AgentTrace {
            id,
            run_id,
            parent_id,
            timestamp: timestamp as u64,
            duration_ms: duration_ms as u64,
            trace_type,
            input,
            output,
            metadata,
            reasoning: None,
            decision: None,
            evaluation: None,
        })
    }
    
    /// 构建查询 SQL
    fn build_query_sql(query: &TraceQuery) -> (String, Vec<String>) {
        let mut sql = String::from(
            "SELECT id, run_id, parent_id, timestamp, duration_ms,
                    trace_type, input, output, metadata
             FROM agent_traces WHERE 1=1"
        );
        let mut params: Vec<String> = Vec::new();
        
        if let Some(ref text) = query.text {
            sql.push_str(" AND (input LIKE ? OR output LIKE ?)");
            let pattern = format!("%{}%", text);
            params.push(pattern.clone());
            params.push(pattern);
        }
        
        if let Some(ref run_id) = query.run_id {
            sql.push_str(" AND run_id = ?");
            params.push(run_id.clone());
        }
        
        if let Some((start, end)) = query.time_range {
            sql.push_str(" AND timestamp BETWEEN ? AND ?");
            params.push(start.to_string());
            params.push(end.to_string());
        }
        
        if let Some(min_duration) = query.min_duration_ms {
            sql.push_str(" AND duration_ms >= ?");
            params.push(min_duration.to_string());
        }
        
        if let Some(max_duration) = query.max_duration_ms {
            sql.push_str(" AND duration_ms <= ?");
            params.push(max_duration.to_string());
        }
        
        sql.push_str(" ORDER BY timestamp DESC");
        
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }
        
        (sql, params)
    }
}

#[async_trait]
impl TraceStore for DuckdbTraceStore {
    async fn store_trace(&self, trace: &AgentTrace) -> Result<()> {
        let conn = self.get_connection()?;
        let (trace_type, input, output, metadata) = Self::serialize_trace(trace)?;
        
        conn.execute(
            "INSERT INTO agent_traces (
                id, run_id, parent_id, timestamp, duration_ms,
                trace_type, input, output, metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                duration_ms = excluded.duration_ms,
                output = excluded.output,
                metadata = excluded.metadata",
            duckdb::params![
                trace.id,
                trace.run_id,
                trace.parent_id,
                trace.timestamp as i64,
                trace.duration_ms as i64,
                trace_type,
                input,
                output,
                metadata,
            ],
        ).context("Failed to store trace")?;
        
        Ok(())
    }
    
    async fn store_traces_batch(&self, traces: &[AgentTrace]) -> Result<()> {
        if traces.is_empty() {
            return Ok(());
        }
        
        let conn = self.get_connection()?;
        
        // DuckDB 使用单个 INSERT 语句批量插入
        let mut sql = String::from(
            "INSERT INTO agent_traces (
                id, run_id, parent_id, timestamp, duration_ms,
                trace_type, input, output, metadata
            ) VALUES "
        );
        
        let mut params: Vec<Box<dyn duckdb::ToSql>> = Vec::new();
        
        for (i, trace) in traces.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str("(?, ?, ?, ?, ?, ?, ?, ?, ?)");
            
            let (trace_type, input, output, metadata) = Self::serialize_trace(trace)?;
            params.push(Box::new(trace.id.clone()));
            params.push(Box::new(trace.run_id.clone()));
            params.push(Box::new(trace.parent_id.clone()));
            params.push(Box::new(trace.timestamp as i64));
            params.push(Box::new(trace.duration_ms as i64));
            params.push(Box::new(trace_type));
            params.push(Box::new(input));
            params.push(Box::new(output));
            params.push(Box::new(metadata));
        }
        
        sql.push_str(" ON CONFLICT(id) DO UPDATE SET duration_ms = excluded.duration_ms");
        
        let params_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        
        conn.execute(&sql, params_refs.as_slice())
            .context("Failed to batch insert traces")?;
        
        Ok(())
    }
    
    async fn get_trace(&self, id: &str) -> Result<Option<AgentTrace>> {
        let conn = self.get_connection()?;
        
        let mut stmt = conn.prepare(
            "SELECT id, run_id, parent_id, timestamp, duration_ms,
                    trace_type, input, output, metadata
             FROM agent_traces WHERE id = ?"
        ).context("Failed to prepare statement")?;
        
        let trace = stmt.query_row(duckdb::params![id], |row| {
            Self::parse_trace(row)
        }).optional().context("Failed to get trace")?;
        
        Ok(trace)
    }
    
    async fn delete_trace(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM agent_traces WHERE id = ?", duckdb::params![id])
            .context("Failed to delete trace")?;
        Ok(())
    }
    
    async fn list_traces(&self, query: &TraceQuery) -> Result<Vec<AgentTrace>> {
        let conn = self.get_connection()?;
        let (sql, sql_params) = Self::build_query_sql(query);
        
        let mut stmt = conn.prepare(&sql)
            .context("Failed to prepare statement")?;
        
        let params: Vec<Box<dyn duckdb::ToSql>> = sql_params.iter()
            .map(|p| Box::new(p.clone()) as Box<dyn duckdb::ToSql>)
            .collect();
        let params_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        
        let traces = stmt.query_map(params_refs.as_slice(), |row| {
            Self::parse_trace(row)
        }).context("Failed to query traces")?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to collect traces")?;
        
        Ok(traces)
    }
    
    async fn count_traces(&self, query: &TraceQuery) -> Result<u64> {
        let conn = self.get_connection()?;
        
        let mut sql = String::from("SELECT COUNT(*) FROM agent_traces WHERE 1=1");
        let mut params: Vec<String> = Vec::new();
        
        if let Some(ref run_id) = query.run_id {
            sql.push_str(" AND run_id = ?");
            params.push(run_id.clone());
        }
        
        if let Some((start, end)) = query.time_range {
            sql.push_str(" AND timestamp BETWEEN ? AND ?");
            params.push(start.to_string());
            params.push(end.to_string());
        }
        
        let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|p| p as &dyn duckdb::ToSql).collect();
        
        let count: i64 = conn.query_row(&sql, param_refs.as_slice(), |row| row.get(0))
            .context("Failed to count traces")?;
        
        Ok(count as u64)
    }
    
    async fn store_reasoning(&self, trace_id: &str, reasoning: &ReasoningChain) -> Result<()> {
        let conn = self.get_connection()?;
        
        let steps = serde_json::to_string(&reasoning.steps)
            .context("Failed to serialize reasoning steps")?;
        
        conn.execute(
            "INSERT INTO reasoning_chains (
                id, trace_id, steps, conclusion, confidence, quality_score
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(trace_id) DO UPDATE SET
                steps = excluded.steps,
                conclusion = excluded.conclusion,
                confidence = excluded.confidence,
                quality_score = excluded.quality_score",
            duckdb::params![
                uuid::Uuid::new_v4().to_string(),
                trace_id,
                steps,
                reasoning.conclusion,
                reasoning.confidence,
                reasoning.quality_score,
            ],
        ).context("Failed to store reasoning")?;
        
        Ok(())
    }
    
    async fn get_reasoning(&self, trace_id: &str) -> Result<Option<ReasoningChain>> {
        let conn = self.get_connection()?;
        
        let result = conn.query_row(
            "SELECT steps, conclusion, confidence, quality_score
             FROM reasoning_chains WHERE trace_id = ?",
            duckdb::params![trace_id],
            |row| {
                let steps_str: String = row.get(0)?;
                let steps: Vec<ReasoningStep> = serde_json::from_str(&steps_str).unwrap_or_default();
                Ok(ReasoningChain {
                    steps,
                    conclusion: row.get(1)?,
                    confidence: row.get(2)?,
                    quality_score: row.get(3)?,
                })
            }
        ).optional().context("Failed to get reasoning")?;
        
        Ok(result)
    }
    
    async fn store_decision(&self, trace_id: &str, decision: &DecisionPoint) -> Result<()> {
        let conn = self.get_connection()?;
        
        let alternatives = serde_json::to_string(&decision.alternatives)
            .context("Failed to serialize alternatives")?;
        
        conn.execute(
            "INSERT INTO decision_points (
                id, trace_id, decision_type, description, alternatives,
                chosen_alternative_id, rationale, quality_score, is_optimal, timestamp
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                decision.id,
                trace_id,
                serde_json::to_string(&decision.decision_type)?,
                decision.description,
                alternatives,
                decision.chosen_alternative_id,
                decision.rationale,
                decision.quality.score,
                decision.quality.is_optimal,
                decision.timestamp as i64,
            ],
        ).context("Failed to store decision")?;
        
        Ok(())
    }
    
    async fn get_decisions(&self, trace_id: &str) -> Result<Vec<DecisionPoint>> {
        let conn = self.get_connection()?;
        
        let mut stmt = conn.prepare(
            "SELECT id, decision_type, description, alternatives,
                    chosen_alternative_id, rationale, quality_score, is_optimal, timestamp
             FROM decision_points WHERE trace_id = ?
             ORDER BY timestamp"
        ).context("Failed to prepare statement")?;
        
        let decisions = stmt.query_map(duckdb::params![trace_id], |row| {
            let decision_type_str: String = row.get(1)?;
            let alternatives_str: String = row.get(3)?;
            let quality_score: Option<f64> = row.get(6)?;
            let is_optimal: bool = row.get(7)?;
            
            Ok(DecisionPoint {
                id: row.get(0)?,
                decision_type: serde_json::from_str(&decision_type_str).unwrap_or(DecisionType::Other),
                description: row.get(2)?,
                alternatives: serde_json::from_str(&alternatives_str).unwrap_or_default(),
                chosen_alternative_id: row.get(4)?,
                rationale: row.get(5)?,
                quality: DecisionQuality {
                    is_optimal,
                    score: quality_score.unwrap_or(0.0),
                    improvement_suggestions: vec![],
                },
                timestamp: row.get::<_, i64>(8)? as u64,
            })
        }).context("Failed to query decisions")?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to collect decisions")?;
        
        Ok(decisions)
    }
    
    async fn store_evaluation(&self, trace_id: &str, evaluation: &EvaluationResult) -> Result<()> {
        let conn = self.get_connection()?;
        
        conn.execute(
            "INSERT INTO evaluation_results (
                id, trace_id, overall_score, decision_scores,
                reasoning_quality, efficiency_score, error_rate, suggestions, evaluated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(trace_id) DO UPDATE SET
                overall_score = excluded.overall_score,
                decision_scores = excluded.decision_scores,
                reasoning_quality = excluded.reasoning_quality,
                efficiency_score = excluded.efficiency_score,
                error_rate = excluded.error_rate,
                suggestions = excluded.suggestions,
                evaluated_at = excluded.evaluated_at",
            duckdb::params![
                uuid::Uuid::new_v4().to_string(),
                trace_id,
                evaluation.overall_score,
                serde_json::to_string(&evaluation.decision_scores)?,
                evaluation.reasoning_quality,
                evaluation.efficiency_score,
                evaluation.error_rate,
                serde_json::to_string(&evaluation.suggestions)?,
                evaluation.evaluated_at as i64,
            ],
        ).context("Failed to store evaluation")?;
        
        Ok(())
    }
    
    async fn get_evaluation(&self, trace_id: &str) -> Result<Option<EvaluationResult>> {
        let conn = self.get_connection()?;
        
        let result = conn.query_row(
            "SELECT overall_score, decision_scores, reasoning_quality,
                    efficiency_score, error_rate, suggestions, evaluated_at
             FROM evaluation_results WHERE trace_id = ?",
            duckdb::params![trace_id],
            |row| {
                let decision_scores_str: Option<String> = row.get(1)?;
                let suggestions_str: Option<String> = row.get(5)?;
                
                Ok(EvaluationResult {
                    trace_id: trace_id.to_string(),
                    overall_score: row.get(0)?,
                    decision_scores: decision_scores_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default(),
                    reasoning_quality: row.get(2)?,
                    efficiency_score: row.get(3)?,
                    error_rate: row.get(4)?,
                    suggestions: suggestions_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default(),
                    evaluated_at: row.get::<_, i64>(6)? as u64,
                })
            }
        ).optional().context("Failed to get evaluation")?;
        
        Ok(result)
    }
    
    async fn aggregate(&self, query: AggregationQuery) -> Result<AggregationResult> {
        let conn = self.get_connection()?;
        
        match query {
            AggregationQuery::SuccessRate { time_range } => {
                let (start, end) = time_range.unwrap_or((0, u64::MAX));
                
                let total: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM agent_traces WHERE timestamp BETWEEN ? AND ?",
                    duckdb::params![start as i64, end as i64],
                    |row| row.get(0),
                ).context("Failed to count total traces")?;
                
                let success: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM agent_traces WHERE timestamp BETWEEN ? AND ? 
                     AND json_extract_string(output, '$.success') = 'true'",
                    duckdb::params![start as i64, end as i64],
                    |row| row.get(0),
                ).context("Failed to count successful traces")?;
                
                let rate = if total > 0 { success as f64 / total as f64 } else { 0.0 };
                
                Ok(AggregationResult::SuccessRate {
                    total: total as u64,
                    success: success as u64,
                    rate,
                })
            }
            AggregationQuery::AverageDuration { time_range } => {
                let (start, end) = time_range.unwrap_or((0, u64::MAX));
                
                let result = conn.query_row(
                    "SELECT AVG(duration_ms), MIN(duration_ms), MAX(duration_ms)
                     FROM agent_traces WHERE timestamp BETWEEN ? AND ?",
                    duckdb::params![start as i64, end as i64],
                    |row| Ok((
                        row.get::<_, Option<f64>>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<i64>>(2)?
                    )),
                ).context("Failed to get duration stats")?;
                
                Ok(AggregationResult::AverageDuration {
                    avg_ms: result.0.unwrap_or(0.0),
                    min_ms: result.1.unwrap_or(0) as u64,
                    max_ms: result.2.unwrap_or(0) as u64,
                })
            }
            AggregationQuery::TraceTypeDistribution { time_range } => {
                let (start, end) = time_range.unwrap_or((0, u64::MAX));
                
                let mut stmt = conn.prepare(
                    "SELECT trace_type, COUNT(*) as count 
                     FROM agent_traces WHERE timestamp BETWEEN ? AND ? 
                     GROUP BY trace_type"
                ).context("Failed to prepare statement")?;
                
                let distribution = stmt.query_map(duckdb::params![start as i64, end as i64], |row| {
                    let trace_type_str: String = row.get(0)?;
                    let trace_type: TraceType = serde_json::from_str(&trace_type_str).unwrap_or(TraceType::UserMessage);
                    Ok((trace_type.type_name().to_string(), row.get::<_, i64>(1)? as u64))
                }).context("Failed to query distribution")?
                .collect::<Result<Vec<_>, _>>()
                .context("Failed to collect distribution")?;
                
                Ok(AggregationResult::TraceTypeDistribution { distribution })
            }
            AggregationQuery::TokenUsage { time_range } => {
                let (start, end) = time_range.unwrap_or((0, u64::MAX));
                
                let result = conn.query_row(
                    "SELECT 
                        SUM(CAST(json_extract_string(output, '$.tokens_used.prompt_tokens') AS BIGINT)),
                        SUM(CAST(json_extract_string(output, '$.tokens_used.completion_tokens') AS BIGINT)),
                        SUM(CAST(json_extract_string(output, '$.tokens_used.total_tokens') AS BIGINT))
                     FROM agent_traces WHERE timestamp BETWEEN ? AND ?",
                    duckdb::params![start as i64, end as i64],
                    |row| Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<i64>>(2)?
                    )),
                ).context("Failed to get token stats")?;
                
                Ok(AggregationResult::TokenUsage {
                    total_prompt: result.0.unwrap_or(0) as u64,
                    total_completion: result.1.unwrap_or(0) as u64,
                    total: result.2.unwrap_or(0) as u64,
                })
            }
            AggregationQuery::CostStats { time_range } => {
                let (start, end) = time_range.unwrap_or((0, u64::MAX));
                
                let result = conn.query_row(
                    "SELECT 
                        SUM(CAST(json_extract_string(output, '$.cost_usd') AS DOUBLE)),
                        AVG(CAST(json_extract_string(output, '$.cost_usd') AS DOUBLE))
                     FROM agent_traces WHERE timestamp BETWEEN ? AND ?",
                    duckdb::params![start as i64, end as i64],
                    |row| Ok((
                        row.get::<_, Option<f64>>(0)?,
                        row.get::<_, Option<f64>>(1)?
                    )),
                ).context("Failed to get cost stats")?;
                
                Ok(AggregationResult::CostStats {
                    total_cost: result.0.unwrap_or(0.0),
                    avg_cost_per_trace: result.1.unwrap_or(0.0),
                })
            }
        }
    }
    
    async fn cleanup_expired(&self, retention_days: u32) -> Result<u64> {
        let conn = self.get_connection()?;
        
        let cutoff = chrono::Utc::now()
            .saturating_sub(chrono::Duration::days(retention_days as i64))
            .timestamp() as u64;
        
        let deleted = conn.execute(
            "DELETE FROM agent_traces WHERE timestamp < ?",
            duckdb::params![cutoff as i64],
        ).context("Failed to cleanup expired traces")?;
        
        Ok(deleted as u64)
    }
    
    async fn storage_stats(&self) -> Result<StorageStats> {
        let conn = self.get_connection()?;
        
        let total_traces: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_traces", [],
            |row| row.get(0),
        ).context("Failed to count traces")?;
        
        let total_reasoning: i64 = conn.query_row(
            "SELECT COUNT(*) FROM reasoning_chains", [],
            |row| row.get(0),
        ).context("Failed to count reasoning chains")?;
        
        let total_decisions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM decision_points", [],
            |row| row.get(0),
        ).context("Failed to count decisions")?;
        
        let total_evaluations: i64 = conn.query_row(
            "SELECT COUNT(*) FROM evaluation_results", [],
            |row| row.get(0),
        ).context("Failed to count evaluations")?;
        
        let oldest: Option<i64> = conn.query_row(
            "SELECT MIN(timestamp) FROM agent_traces", [],
            |row| row.get(0),
        ).ok().flatten();
        
        let newest: Option<i64> = conn.query_row(
            "SELECT MAX(timestamp) FROM agent_traces", [],
            |row| row.get(0),
        ).ok().flatten();
        
        let db_size = std::fs::metadata(&self.db_path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        Ok(StorageStats {
            total_traces: total_traces as u64,
            total_reasoning_chains: total_reasoning as u64,
            total_decisions: total_decisions as u64,
            total_evaluations: total_evaluations as u64,
            db_size_bytes: db_size,
            oldest_trace_timestamp: oldest.map(|t| t as u64),
            newest_trace_timestamp: newest.map(|t| t as u64),
        })
    }
    
    async fn flush(&self) -> Result<()> {
        let mut buffer = self.buffer.write().await;
        if !buffer.is_empty() {
            let traces: Vec<AgentTrace> = buffer.drain(..).collect();
            drop(buffer);
            self.store_traces_batch(&traces).await?;
        }
        Ok(())
    }
    
    fn backend_name(&self) -> &'static str {
        "duckdb"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn create_test_trace(id: &str) -> AgentTrace {
        AgentTrace {
            id: id.to_string(),
            run_id: "test-run".to_string(),
            parent_id: None,
            timestamp: 1000,
            duration_ms: 500,
            trace_type: TraceType::LlmCall {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
            },
            input: TraceInput {
                content: "Hello".to_string(),
                content_type: InputContentType::Text,
                params: std::collections::HashMap::new(),
            },
            output: TraceOutput {
                content: "Hi there!".to_string(),
                success: true,
                error: None,
                tokens_used: Some(TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                }),
                cost_usd: Some(0.001),
            },
            metadata: serde_json::json!({"key": "value"}),
            reasoning: None,
            decision: None,
            evaluation: None,
        }
    }
    
    #[tokio::test]
    async fn test_store_and_get_trace() {
        let tmp = TempDir::new().unwrap();
        let store = DuckdbTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        let trace = create_test_trace("test-1");
        store.store_trace(&trace).await.unwrap();
        
        let retrieved = store.get_trace("test-1").await.unwrap();
        assert!(retrieved.is_some());
        
        let t = retrieved.unwrap();
        assert_eq!(t.id, "test-1");
        assert_eq!(t.run_id, "test-run");
        assert!(t.output.success);
    }
    
    #[tokio::test]
    async fn test_batch_store() {
        let tmp = TempDir::new().unwrap();
        let store = DuckdbTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        let traces: Vec<AgentTrace> = (0..10)
            .map(|i| create_test_trace(&format!("batch-{}", i)))
            .collect();
        
        store.store_traces_batch(&traces).await.unwrap();
        
        let count = store.count_traces(&TraceQuery::default()).await.unwrap();
        assert_eq!(count, 10);
    }
    
    #[tokio::test]
    async fn test_aggregation() {
        let tmp = TempDir::new().unwrap();
        let store = DuckdbTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        for i in 0..10 {
            let mut trace = create_test_trace(&format!("agg-{}", i));
            trace.timestamp = 1000 + i;
            trace.output.success = i < 8;
            store.store_trace(&trace).await.unwrap();
        }
        
        let result = store.aggregate(AggregationQuery::SuccessRate { time_range: None }).await.unwrap();
        
        if let AggregationResult::SuccessRate { total, success, rate } = result {
            assert_eq!(total, 10);
            assert_eq!(success, 8);
            assert!((rate - 0.8).abs() < 0.01);
        } else {
            panic!("Expected SuccessRate result");
        }
    }
    
    #[tokio::test]
    async fn test_storage_stats() {
        let tmp = TempDir::new().unwrap();
        let store = DuckdbTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        for i in 0..5 {
            let trace = create_test_trace(&format!("stats-{}", i));
            store.store_trace(&trace).await.unwrap();
        }
        
        let stats = store.storage_stats().await.unwrap();
        assert_eq!(stats.total_traces, 5);
        assert!(stats.db_size_bytes > 0);
    }
}
