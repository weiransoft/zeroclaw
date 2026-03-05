//! 失败模式分析模块
//! 
//! 提供失败记录和模式分析功能
//! 支持自动识别错误模式并生成修复建议

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 失败模式结构体
/// 表示识别出的一个失败模式，包含模式信息和统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    /// 模式唯一标识符
    pub id: String,
    /// 错误类型
    pub pattern_type: String,
    /// 模式描述
    pub description: String,
    /// 出现频率
    pub frequency: u32,
    /// 首次出现时间戳
    pub first_seen: i64,
    /// 最后出现时间戳
    pub last_seen: i64,
    /// 受影响的智能体列表
    pub affected_agents: Vec<String>,
    /// 错误消息列表
    pub error_messages: Vec<String>,
    /// 建议的修复方案
    pub suggested_fix: Option<String>,
}

/// 失败记录结构体
/// 记录每次失败事件的详细信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    /// 记录唯一标识符
    pub id: String,
    /// 时间戳（毫秒）
    pub timestamp: i64,
    /// 智能体名称
    pub agent_name: String,
    /// 错误类型
    pub error_type: String,
    /// 错误消息
    pub error_message: String,
    /// 上下文信息
    pub context: serde_json::Value,
}

/// 失败分析器
/// 负责记录失败事件、分析失败模式、生成修复建议
pub struct FailureAnalyzer {
    /// 失败记录列表
    records: Arc<RwLock<Vec<FailureRecord>>>,
    /// 识别出的失败模式列表
    patterns: Arc<RwLock<Vec<FailurePattern>>>,
}

impl FailureAnalyzer {
    /// 创建新的失败分析器实例
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            patterns: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 记录一次失败事件
    /// 记录失败后会自动分析是否形成新的模式
    /// # 参数
    /// - `agent_name`: 智能体名称
    /// - `error_type`: 错误类型
    /// - `error_message`: 错误消息
    /// - `context`: 上下文信息
    pub async fn record_failure(&self, agent_name: &str, error_type: &str, error_message: &str, context: serde_json::Value) {
        let record = FailureRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            agent_name: agent_name.to_string(),
            error_type: error_type.to_string(),
            error_message: error_message.to_string(),
            context,
        };
        
        let mut records = self.records.write().await;
        records.push(record);
        
        drop(records);
        self.analyze_patterns().await;
    }

    /// 获取失败模式列表
    /// # 参数
    /// - `limit`: 返回结果数量限制，默认为 20
    /// # 返回
    /// 返回识别出的失败模式列表
    pub async fn get_patterns(&self, limit: Option<usize>) -> Vec<FailurePattern> {
        let patterns = self.patterns.read().await;
        let limit = limit.unwrap_or(20);
        patterns.iter().take(limit).cloned().collect()
    }

    /// 分析失败模式
    /// 内部方法，根据已记录的失败事件分析并更新失败模式
    /// 会将相同智能体和错误类型的失败聚合为模式
    async fn analyze_patterns(&self) {
        let records = self.records.read().await;
        
        let mut error_groups: HashMap<String, Vec<&FailureRecord>> = HashMap::new();
        for record in records.iter() {
            let key = format!("{}:{}", record.agent_name, record.error_type);
            error_groups.entry(key).or_insert_with(Vec::new).push(record);
        }
        
        let mut new_patterns = Vec::new();
        
        // 遍历错误分组，分析每个错误类型是否形成模式
        // 至少需要出现2次以上才认为是模式
        for (_key, group) in error_groups {
            if group.len() < 2 {
                continue;
            }
            
            let first_record = group.first().unwrap();
            let last_record = group.last().unwrap();
            
            let error_messages: Vec<String> = group.iter()
                .map(|r| r.error_message.clone())
                .collect();
            
            let suggested_fix = self.generate_suggestion(&first_record.error_type, &error_messages);
            
            let pattern = FailurePattern {
                id: uuid::Uuid::new_v4().to_string(),
                pattern_type: first_record.error_type.clone(),
                description: format!("Occured {} times in agent {}", group.len(), first_record.agent_name),
                frequency: group.len() as u32,
                first_seen: first_record.timestamp,
                last_seen: last_record.timestamp,
                affected_agents: vec![first_record.agent_name.clone()],
                error_messages,
                suggested_fix,
            };
            
            new_patterns.push(pattern);
        }
        
        let mut patterns = self.patterns.write().await;
        *patterns = new_patterns;
    }

    /// 根据错误类型生成修复建议
    /// 根据错误类型和错误消息内容，生成针对性的修复建议
    /// # 参数
    /// - `error_type`: 错误类型
    /// - `error_messages`: 错误消息列表
    /// # 返回
    /// 可能的修复建议，如果没有则返回 None
    fn generate_suggestion(&self, error_type: &str, error_messages: &[String]) -> Option<String> {
        match error_type {
            "timeout" => Some("Consider increasing timeout duration or optimizing the operation".to_string()),
            "rate_limit" => Some("Implement exponential backoff and respect rate limits".to_string()),
            "authentication" => Some("Check credentials and ensure they are valid".to_string()),
            "network" => Some("Check network connectivity and retry logic".to_string()),
            "validation" => Some("Review input data and validate before processing".to_string()),
            "resource_exhausted" => Some("Consider scaling resources or optimizing resource usage".to_string()),
            _ => {
                if error_messages.iter().any(|m| m.contains("tool")) {
                    Some("Review tool implementation and error handling".to_string())
                } else if error_messages.iter().any(|m| m.contains("llm") || m.contains("model")) {
                    Some("Check LLM provider status and model availability".to_string())
                } else {
                    None
                }
            }
        }
    }

    /// 获取失败统计信息
    /// 统计失败事件的整体情况，包括按智能体和按错误类型分类
    /// # 返回
    /// 包含统计信息的 JSON 对象
    pub async fn get_statistics(&self) -> serde_json::Value {
        let records = self.records.read().await;
        
        let total_failures = records.len();
        let mut by_agent: HashMap<String, u32> = HashMap::new();
        let mut by_type: HashMap<String, u32> = HashMap::new();
        
        for record in records.iter() {
            *by_agent.entry(record.agent_name.clone()).or_insert(0) += 1;
            *by_type.entry(record.error_type.clone()).or_insert(0) += 1;
        }
        
        serde_json::json!({
            "totalFailures": total_failures,
            "byAgent": by_agent,
            "byType": by_type,
            "uniquePatterns": self.patterns.read().await.len()
        })
    }
}

impl Default for FailureAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
