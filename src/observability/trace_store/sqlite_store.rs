//! SQLite 轨迹存储实现
//!
//! 使用 SQLite 作为存储后端，支持 WAL 模式高性能写入

use crate::observability::trace_store::types::*;
use crate::observability::trace_store::store::TraceStore;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::TraceStoreConfig;

// 使用统一的数据库连接池管理系统
use crate::db::{DbPool, SqlLoader, DatabaseType, SqliteConfig};

/// SQLite 轨迹存储
pub struct SqliteTraceStore {
    /// 数据库路径
    db_path: std::path::PathBuf,
    /// 连接池
    pool: DbPool,
    /// 配置
    config: TraceStoreConfig,
    /// 写入缓冲区
    buffer: Arc<RwLock<Vec<AgentTrace>>>,
}

impl SqliteTraceStore {
    /// 创建新的 SQLite 轨迹存储
    pub fn new(workspace_dir: &Path, config: TraceStoreConfig) -> Self {
        let db_path = workspace_dir.join(".zeroclaw").join("traces.db");
        
        // 确保目录存在
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        
        // 使用统一的数据库配置
        let sqlite_config = SqliteConfig {
            path: db_path.clone(),
            max_size: config.max_connections,
            min_size: 1, // 最小连接数（预热）
            connection_timeout: 5, // 连接超时（秒）
            wal_mode: true,
            foreign_keys: true,
            synchronous: true,
        };
        
        // 初始化统一的连接池
        let pool = DbPool::new(sqlite_config).expect("Failed to create trace database pool");
        
        let store = Self {
            db_path,
            pool,
            config: config.clone(),
            buffer: Arc::new(RwLock::new(Vec::with_capacity(config.batch_size))),
        };
        
        // 初始化 Schema
        store.init_schema();
        
        store
    }
    
    /// 使用现有连接池创建（复用 swarm.db）
    pub fn with_pool(pool: DbPool, config: TraceStoreConfig) -> Self {
        let store = Self {
            db_path: std::path::PathBuf::from("shared"),
            pool,
            config: config.clone(),
            buffer: Arc::new(RwLock::new(Vec::with_capacity(config.batch_size))),
        };
        
        store.init_schema();
        store
    }
    
    /// 使用数据库连接执行操作
    fn with_connection<T>(&self, f: impl FnOnce(&rusqlite::Connection) -> Result<T>) -> Result<T> {
        self.pool.with_connection(f)
    }
    
    /// 初始化数据库 Schema
    fn init_schema(&self) {
        // 使用 SQL 加载器加载 traces 数据库 schema
        match SqlLoader::default() {
            Ok(sql_loader) => {
                match sql_loader.load_schema(DatabaseType::Traces) {
                    Ok(schema) => {
                        let _ = self.with_connection(|conn| {
                            conn.execute_batch(&schema)
                                .context("Failed to initialize traces schema")?;
                            Ok(())
                        });
                    }
                    Err(e) => {
                        tracing::error!("[SqliteTraceStore] Failed to load traces schema: {:?}", e);
                        // 如果加载失败，尝试使用内置的最小 schema
                        let _ = self.with_connection(|conn| {
                            // 性能优化 PRAGMA
                            conn.execute_batch(
                                "PRAGMA journal_mode = WAL;
                                 PRAGMA synchronous = NORMAL;
                                 PRAGMA mmap_size = 8388608;
                                 PRAGMA cache_size = -2000;
                                 PRAGMA temp_store = MEMORY;
                                 PRAGMA foreign_keys = ON;"
                            )?;
                            
                            // 创建表
                            conn.execute_batch(&Self::get_schema_sql())?;
                            Ok(())
                        });
                    }
                }
            }
            Err(e) => {
                tracing::error!("[SqliteTraceStore] Failed to create SQL loader: {:?}", e);
                // 如果创建 SQL 加载器失败，尝试使用内置的最小 schema
                let _ = self.with_connection(|conn| {
                    // 性能优化 PRAGMA
                    conn.execute_batch(
                        "PRAGMA journal_mode = WAL;
                         PRAGMA synchronous = NORMAL;
                         PRAGMA mmap_size = 8388608;
                         PRAGMA cache_size = -2000;
                         PRAGMA temp_store = MEMORY;
                         PRAGMA foreign_keys = ON;"
                    )?;
                    
                    // 创建表
                    conn.execute_batch(&Self::get_schema_sql())?;
                    Ok(())
                });
            }
        }
    }
    
    /// 获取 Schema SQL
    fn get_schema_sql() -> String {
        r#"
        -- 主轨迹表
        CREATE TABLE IF NOT EXISTS agent_traces (
            id              TEXT PRIMARY KEY,
            run_id          TEXT NOT NULL,
            parent_id       TEXT,
            timestamp       INTEGER NOT NULL,
            duration_ms     INTEGER,
            trace_type      TEXT NOT NULL,
            input           TEXT NOT NULL,
            output          TEXT NOT NULL,
            metadata        TEXT NOT NULL DEFAULT '{}',
            created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
        );

        -- 轨迹索引
        CREATE INDEX IF NOT EXISTS idx_traces_run_id ON agent_traces(run_id);
        CREATE INDEX IF NOT EXISTS idx_traces_parent ON agent_traces(parent_id);
        CREATE INDEX IF NOT EXISTS idx_traces_timestamp ON agent_traces(timestamp);
        CREATE INDEX IF NOT EXISTS idx_traces_type ON agent_traces(trace_type);
        CREATE INDEX IF NOT EXISTS idx_traces_duration ON agent_traces(duration_ms);

        -- 推理链表
        CREATE TABLE IF NOT EXISTS reasoning_chains (
            id              TEXT PRIMARY KEY,
            trace_id        TEXT NOT NULL UNIQUE,
            steps           TEXT NOT NULL,
            conclusion      TEXT,
            confidence      REAL,
            quality_score   REAL,
            created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
            FOREIGN KEY (trace_id) REFERENCES agent_traces(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_reasoning_trace ON reasoning_chains(trace_id);

        -- 决策点表
        CREATE TABLE IF NOT EXISTS decision_points (
            id                  TEXT PRIMARY KEY,
            trace_id            TEXT NOT NULL,
            decision_type       TEXT NOT NULL,
            description         TEXT NOT NULL,
            alternatives        TEXT NOT NULL,
            chosen_alternative_id TEXT NOT NULL,
            rationale           TEXT,
            quality_score       REAL,
            is_optimal          INTEGER NOT NULL DEFAULT 0,
            timestamp           INTEGER NOT NULL,
            FOREIGN KEY (trace_id) REFERENCES agent_traces(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_decisions_trace ON decision_points(trace_id);
        CREATE INDEX IF NOT EXISTS idx_decisions_type ON decision_points(decision_type);

        -- 评估结果表
        CREATE TABLE IF NOT EXISTS evaluation_results (
            id                  TEXT PRIMARY KEY,
            trace_id            TEXT NOT NULL UNIQUE,
            overall_score       REAL NOT NULL,
            decision_scores     TEXT,
            reasoning_quality   REAL,
            efficiency_score    REAL,
            error_rate          REAL,
            suggestions         TEXT,
            evaluated_at        INTEGER NOT NULL,
            FOREIGN KEY (trace_id) REFERENCES agent_traces(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_eval_trace ON evaluation_results(trace_id);
        "#.to_string()
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
    fn parse_trace(row: &rusqlite::Row) -> Result<AgentTrace, rusqlite::Error> {
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
    fn build_query_sql(query: &TraceQuery) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut sql = String::from(
            "SELECT id, run_id, parent_id, timestamp, duration_ms,
                    trace_type, input, output, metadata
             FROM agent_traces WHERE 1=1"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        if let Some(ref text) = query.text {
            sql.push_str(" AND (input LIKE ? OR output LIKE ?)");
            let pattern = format!("%{}%", text);
            params.push(Box::new(pattern.clone()));
            params.push(Box::new(pattern));
        }
        
        if let Some(ref run_id) = query.run_id {
            sql.push_str(" AND run_id = ?");
            params.push(Box::new(run_id.clone()));
        }
        
        if let Some((start, end)) = query.time_range {
            sql.push_str(" AND timestamp BETWEEN ? AND ?");
            params.push(Box::new(start as i64));
            params.push(Box::new(end as i64));
        }
        
        if let Some(success) = query.success {
            sql.push_str(" AND json_extract(output, '$.success') = ?");
            params.push(Box::new(if success { 1i32 } else { 0i32 }));
        }
        
        if let Some(min_duration) = query.min_duration_ms {
            sql.push_str(" AND duration_ms >= ?");
            params.push(Box::new(min_duration as i64));
        }
        
        if let Some(max_duration) = query.max_duration_ms {
            sql.push_str(" AND duration_ms <= ?");
            params.push(Box::new(max_duration as i64));
        }
        
        if let Some(has_errors) = query.has_errors {
            if has_errors {
                sql.push_str(" AND json_extract(output, '$.success') = 0");
            } else {
                sql.push_str(" AND json_extract(output, '$.success') = 1");
            }
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
impl TraceStore for SqliteTraceStore {
    async fn store_trace(&self, trace: &AgentTrace) -> Result<()> {
        let (trace_type, input, output, metadata) = Self::serialize_trace(trace)?;
        
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO agent_traces (
                    id, run_id, parent_id, timestamp, duration_ms,
                    trace_type, input, output, metadata
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(id) DO UPDATE SET
                    duration_ms = excluded.duration_ms,
                    output = excluded.output,
                    metadata = excluded.metadata",
                params![
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
        })
    }
    
    async fn store_traces_batch(&self, traces: &[AgentTrace]) -> Result<()> {
        if traces.is_empty() {
            return Ok(());
        }
        
        self.with_connection(|conn| {
            let tx = conn.unchecked_transaction()
                .context("Failed to begin transaction")?;
            
            {
                let mut stmt = tx.prepare_cached(
                    "INSERT OR REPLACE INTO agent_traces (
                        id, run_id, parent_id, timestamp, duration_ms,
                        trace_type, input, output, metadata
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
                ).context("Failed to prepare statement")?;
                
                for trace in traces {
                    let (trace_type, input, output, metadata) = Self::serialize_trace(trace)?;
                    
                    stmt.execute(params![
                        trace.id,
                        trace.run_id,
                        trace.parent_id,
                        trace.timestamp as i64,
                        trace.duration_ms as i64,
                        trace_type,
                        input,
                        output,
                        metadata,
                    ]).context("Failed to insert trace")?;
                }
            }
            
            tx.commit().context("Failed to commit transaction")?;
            Ok(())
        })
    }
    
    async fn get_trace(&self, id: &str) -> Result<Option<AgentTrace>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, run_id, parent_id, timestamp, duration_ms,
                        trace_type, input, output, metadata
                 FROM agent_traces WHERE id = ?1"
            ).context("Failed to prepare statement")?;
            
            let trace = stmt.query_row(params![id], |row| {
                Self::parse_trace(row)
            }).optional().context("Failed to get trace")?;
            
            Ok(trace)
        })
    }
    
    async fn delete_trace(&self, id: &str) -> Result<()> {
        self.with_connection(|conn| {
            conn.execute("DELETE FROM agent_traces WHERE id = ?1", params![id])
                .context("Failed to delete trace")?;
            Ok(())
        })
    }
    
    async fn list_traces(&self, query: &TraceQuery) -> Result<Vec<AgentTrace>> {
        let (sql, sql_params) = Self::build_query_sql(query);
        
        self.with_connection(|conn| {
            let mut stmt = conn.prepare_cached(&sql)
                .context("Failed to prepare statement")?;
            
            let params_refs: Vec<&dyn rusqlite::ToSql> = sql_params.iter()
                .map(|p| p.as_ref())
                .collect();
            
            let traces = stmt.query_map(params_refs.as_slice(), |row| {
                Self::parse_trace(row)
            }).context("Failed to query traces")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect traces")?;
            
            Ok(traces)
        })
    }
    
    async fn count_traces(&self, query: &TraceQuery) -> Result<u64> {
        self.with_connection(|conn| {
            let mut sql = String::from("SELECT COUNT(*) FROM agent_traces WHERE 1=1");
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            
            if let Some(ref run_id) = query.run_id {
                sql.push_str(" AND run_id = ?");
                params.push(Box::new(run_id.clone()));
            }
            
            if let Some((start, end)) = query.time_range {
                sql.push_str(" AND timestamp BETWEEN ? AND ?");
                params.push(Box::new(start as i64));
                params.push(Box::new(end as i64));
            }
            
            let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter()
                .map(|p| p.as_ref())
                .collect();
            
            let count: i64 = conn.query_row(&sql, params_refs.as_slice(), |row| row.get(0))
                .context("Failed to count traces")?;
            
            Ok(count as u64)
        })
    }
    
    async fn store_reasoning(&self, trace_id: &str, reasoning: &ReasoningChain) -> Result<()> {
        self.with_connection(|conn| {
            let steps = serde_json::to_string(&reasoning.steps)
                .context("Failed to serialize reasoning steps")?;
            
            conn.execute(
                "INSERT INTO reasoning_chains (
                    id, trace_id, steps, conclusion, confidence, quality_score
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(trace_id) DO UPDATE SET
                    steps = excluded.steps,
                    conclusion = excluded.conclusion,
                    confidence = excluded.confidence,
                    quality_score = excluded.quality_score",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    trace_id,
                    steps,
                    reasoning.conclusion,
                    reasoning.confidence,
                    reasoning.quality_score,
                ],
            ).context("Failed to store reasoning")?;
            
            Ok(())
        })
    }
    
    async fn get_reasoning(&self, trace_id: &str) -> Result<Option<ReasoningChain>> {
        self.with_connection(|conn| {
            let result = conn.query_row(
                "SELECT steps, conclusion, confidence, quality_score
                 FROM reasoning_chains WHERE trace_id = ?1",
                params![trace_id],
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
        })
    }
    
    async fn store_decision(&self, trace_id: &str, decision: &DecisionPoint) -> Result<()> {
        self.with_connection(|conn| {
            let alternatives = serde_json::to_string(&decision.alternatives)
                .context("Failed to serialize alternatives")?;
            
            conn.execute(
                "INSERT INTO decision_points (
                    id, trace_id, decision_type, description, alternatives,
                    chosen_alternative_id, rationale, quality_score, is_optimal, timestamp
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    decision.id,
                    trace_id,
                    serde_json::to_string(&decision.decision_type)?,
                    decision.description,
                    alternatives,
                    decision.chosen_alternative_id,
                    decision.rationale,
                    decision.quality.score,
                    if decision.quality.is_optimal { 1i32 } else { 0i32 },
                    decision.timestamp as i64,
                ],
            ).context("Failed to store decision")?;
            
            Ok(())
        })
    }
    
    async fn get_decisions(&self, trace_id: &str) -> Result<Vec<DecisionPoint>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, decision_type, description, alternatives,
                        chosen_alternative_id, rationale, quality_score, is_optimal, timestamp
                 FROM decision_points WHERE trace_id = ?1
                 ORDER BY timestamp"
            ).context("Failed to prepare statement")?;
            
            let decisions = stmt.query_map(params![trace_id], |row| {
                let decision_type_str: String = row.get(1)?;
                let alternatives_str: String = row.get(3)?;
                let quality_score: Option<f64> = row.get(6)?;
                let is_optimal: i32 = row.get(7)?;
                
                Ok(DecisionPoint {
                    id: row.get(0)?,
                    decision_type: serde_json::from_str(&decision_type_str).unwrap_or(DecisionType::Other),
                    description: row.get(2)?,
                    alternatives: serde_json::from_str(&alternatives_str).unwrap_or_default(),
                    chosen_alternative_id: row.get(4)?,
                    rationale: row.get(5)?,
                    quality: DecisionQuality {
                        is_optimal: is_optimal != 0,
                        score: quality_score.unwrap_or(0.0),
                        improvement_suggestions: vec![],
                    },
                    timestamp: row.get::<_, i64>(8)? as u64,
                })
            }).context("Failed to query decisions")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect decisions")?;
            
            Ok(decisions)
        })
    }
    
    async fn store_evaluation(&self, trace_id: &str, evaluation: &EvaluationResult) -> Result<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO evaluation_results (
                    id, trace_id, overall_score, decision_scores,
                    reasoning_quality, efficiency_score, error_rate, suggestions, evaluated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(trace_id) DO UPDATE SET
                    overall_score = excluded.overall_score,
                    decision_scores = excluded.decision_scores,
                    reasoning_quality = excluded.reasoning_quality,
                    efficiency_score = excluded.efficiency_score,
                    error_rate = excluded.error_rate,
                    suggestions = excluded.suggestions,
                    evaluated_at = excluded.evaluated_at",
                params![
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
        })
    }
    
    async fn get_evaluation(&self, trace_id: &str) -> Result<Option<EvaluationResult>> {
        self.with_connection(|conn| {
            let result = conn.query_row(
                "SELECT overall_score, decision_scores, reasoning_quality,
                        efficiency_score, error_rate, suggestions, evaluated_at
                 FROM evaluation_results WHERE trace_id = ?1",
                params![trace_id],
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
        })
    }
    
    async fn aggregate(&self, query: AggregationQuery) -> Result<AggregationResult> {
        self.with_connection(|conn| {
            match query {
                AggregationQuery::SuccessRate { time_range } => {
                    let (start, end) = time_range.unwrap_or((0, i64::MAX as u64));
                    
                    let total: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM agent_traces WHERE timestamp BETWEEN ?1 AND ?2",
                        params![start as i64, end as i64],
                        |row| row.get(0),
                    ).context("Failed to count total traces")?;
                    
                    // SQLite json_extract 返回 JSON 布尔值 true/false
                    // 直接比较 JSON 提取结果
                    let success: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM agent_traces WHERE timestamp BETWEEN ?1 AND ?2 
                         AND json_extract(output, '$.success') = 1",
                        params![start as i64, end as i64],
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
                    let (start, end) = time_range.unwrap_or((0, i64::MAX as u64));
                    
                    let result = conn.query_row(
                        "SELECT AVG(duration_ms), MIN(duration_ms), MAX(duration_ms)
                         FROM agent_traces WHERE timestamp BETWEEN ?1 AND ?2",
                        params![start as i64, end as i64],
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
                    let (start, end) = time_range.unwrap_or((0, i64::MAX as u64));
                    
                    let mut stmt = conn.prepare_cached(
                        "SELECT trace_type, COUNT(*) as count 
                         FROM agent_traces WHERE timestamp BETWEEN ?1 AND ?2 
                         GROUP BY trace_type"
                    ).context("Failed to prepare statement")?;
                    
                    let distribution = stmt.query_map(params![start as i64, end as i64], |row| {
                        let trace_type_str: String = row.get(0)?;
                        let trace_type: TraceType = serde_json::from_str(&trace_type_str).unwrap_or(TraceType::UserMessage);
                        Ok((trace_type.type_name().to_string(), row.get::<_, i64>(1)? as u64))
                    }).context("Failed to query distribution")?
                    .collect::<Result<Vec<_>, _>>()
                    .context("Failed to collect distribution")?;
                    
                    Ok(AggregationResult::TraceTypeDistribution { distribution })
                }
                AggregationQuery::TokenUsage { time_range } => {
                    let (start, end) = time_range.unwrap_or((0, i64::MAX as u64));
                    
                    let result = conn.query_row(
                        "SELECT 
                            SUM(json_extract(output, '$.tokens_used.prompt_tokens')),
                            SUM(json_extract(output, '$.tokens_used.completion_tokens')),
                            SUM(json_extract(output, '$.tokens_used.total_tokens'))
                         FROM agent_traces WHERE timestamp BETWEEN ?1 AND ?2",
                        params![start as i64, end as i64],
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
                    let (start, end) = time_range.unwrap_or((0, i64::MAX as u64));
                    
                    let result = conn.query_row(
                        "SELECT 
                            SUM(json_extract(output, '$.cost_usd')),
                            AVG(json_extract(output, '$.cost_usd'))
                         FROM agent_traces WHERE timestamp BETWEEN ?1 AND ?2",
                        params![start as i64, end as i64],
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
        })
    }
    
    async fn cleanup_expired(&self, retention_days: u32) -> Result<u64> {
        self.with_connection(|conn| {
            let cutoff = (chrono::Utc::now() - chrono::Duration::days(retention_days as i64))
                .timestamp() as u64;
            
            let deleted = conn.execute(
                "DELETE FROM agent_traces WHERE timestamp < ?1",
                params![cutoff as i64],
            ).context("Failed to cleanup expired traces")?;
            
            Ok(deleted as u64)
        })
    }
    
    async fn storage_stats(&self) -> Result<StorageStats> {
        self.with_connection(|conn| {
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
        "sqlite"
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
        let store = SqliteTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
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
        let store = SqliteTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        let traces: Vec<AgentTrace> = (0..10)
            .map(|i| create_test_trace(&format!("batch-{}", i)))
            .collect();
        
        store.store_traces_batch(&traces).await.unwrap();
        
        let count = store.count_traces(&TraceQuery::default()).await.unwrap();
        assert_eq!(count, 10);
    }
    
    #[tokio::test]
    async fn test_query_traces() {
        let tmp = TempDir::new().unwrap();
        let store = SqliteTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        for i in 0..5 {
            let mut trace = create_test_trace(&format!("query-{}", i));
            trace.timestamp = 1000 + i * 100;
            store.store_trace(&trace).await.unwrap();
        }
        
        let query = TraceQuery {
            time_range: Some((1000, 1300)),
            limit: Some(3),
            ..Default::default()
        };
        
        let results = store.list_traces(&query).await.unwrap();
        assert_eq!(results.len(), 3);
    }
    
    #[tokio::test]
    async fn test_store_reasoning() {
        let tmp = TempDir::new().unwrap();
        let store = SqliteTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        let trace = create_test_trace("reasoning-test");
        store.store_trace(&trace).await.unwrap();
        
        let reasoning = ReasoningChain {
            steps: vec![ReasoningStep {
                step: 1,
                reasoning_type: ReasoningType::ProblemUnderstanding,
                content: "Understanding the problem".to_string(),
                evidence: vec!["User input".to_string()],
                hypotheses: vec![],
                timestamp: Some(1000),
            }],
            conclusion: Some("Problem understood".to_string()),
            confidence: Some(0.9),
            quality_score: Some(0.85),
        };
        
        store.store_reasoning("reasoning-test", &reasoning).await.unwrap();
        
        let retrieved = store.get_reasoning("reasoning-test").await.unwrap();
        assert!(retrieved.is_some());
        
        let r = retrieved.unwrap();
        assert_eq!(r.steps.len(), 1);
        assert_eq!(r.confidence.unwrap(), 0.9);
    }
    
    #[tokio::test]
    async fn test_aggregation() {
        let tmp = TempDir::new().unwrap();
        let store = SqliteTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        for i in 0..10 {
            let mut trace = create_test_trace(&format!("agg-{}", i));
            trace.timestamp = 1000 + i;
            trace.output.success = i < 8; // 8 success, 2 failure
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
        let store = SqliteTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        for i in 0..5 {
            let trace = create_test_trace(&format!("stats-{}", i));
            store.store_trace(&trace).await.unwrap();
        }
        
        let stats = store.storage_stats().await.unwrap();
        assert_eq!(stats.total_traces, 5);
        assert!(stats.db_size_bytes > 0);
    }
    
    #[tokio::test]
    async fn test_cleanup_expired() {
        let tmp = TempDir::new().unwrap();
        let store = SqliteTraceStore::new(tmp.path(), TraceStoreConfig::default());
        
        let now = chrono::Utc::now().timestamp() as u64;
        
        // 创建旧数据（35天前）
        let mut old_trace = create_test_trace("old-trace");
        old_trace.timestamp = now - 35 * 24 * 60 * 60;
        store.store_trace(&old_trace).await.unwrap();
        
        // 创建新数据（当前时间）
        let mut new_trace = create_test_trace("new-trace");
        new_trace.timestamp = now;
        store.store_trace(&new_trace).await.unwrap();
        
        // 清理 30 天前的数据
        let deleted = store.cleanup_expired(30).await.unwrap();
        assert_eq!(deleted, 1);
        
        // 验证新数据还在
        let count = store.count_traces(&TraceQuery::default()).await.unwrap();
        assert_eq!(count, 1);
    }
}
