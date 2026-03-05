//! Trace 数据类型定义
//!
//! 定义轨迹、推理链、决策点等核心数据结构

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 唯一标识符类型
pub type TraceId = String;
pub type RunId = String;

/// 完整的智能体轨迹条目
/// 记录智能体执行的每一步，包括推理过程
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrace {
    /// 唯一标识
    pub id: TraceId,
    /// 运行 ID（一次对话可能有多轮）
    pub run_id: RunId,
    /// 父轨迹 ID（用于嵌套调用）
    pub parent_id: Option<String>,
    
    /// 时间戳（秒）
    pub timestamp: u64,
    /// 持续时间（毫秒）
    pub duration_ms: u64,
    
    /// 轨迹类型
    pub trace_type: TraceType,
    
    /// 输入内容
    pub input: TraceInput,
    /// 输出内容
    pub output: TraceOutput,
    
    /// 元数据
    pub metadata: serde_json::Value,
    
    /// 推理链（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningChain>,
    
    /// 决策点（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<DecisionPoint>,
    
    /// 评估结果（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<EvaluationResult>,
}

/// 轨迹类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceType {
    /// 用户消息
    UserMessage,
    /// LLM 调用
    LlmCall {
        provider: String,
        model: String,
    },
    /// 工具调用
    ToolCall {
        tool: String,
        action: String,
    },
    /// 子智能体调用
    SubAgentCall {
        agent_name: String,
        task: String,
    },
    /// 阶段转换
    PhaseTransition {
        from: String,
        to: String,
    },
    /// 审批请求
    ApprovalRequest {
        approval_type: String,
    },
    /// 错误
    Error {
        component: String,
        error_type: String,
    },
    /// 系统事件
    SystemEvent {
        event: String,
    },
}

impl TraceType {
    /// 获取类型名称
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::UserMessage { .. } => "user_message",
            Self::LlmCall { .. } => "llm_call",
            Self::ToolCall { .. } => "tool_call",
            Self::SubAgentCall { .. } => "sub_agent_call",
            Self::PhaseTransition { .. } => "phase_transition",
            Self::ApprovalRequest { .. } => "approval_request",
            Self::Error { .. } => "error",
            Self::SystemEvent { .. } => "system_event",
        }
    }
}

/// 轨迹输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceInput {
    /// 输入内容（文本或 JSON）
    pub content: String,
    /// 输入类型
    pub content_type: InputContentType,
    /// 附加参数
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub params: HashMap<String, serde_json::Value>,
}

/// 输入内容类型
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InputContentType {
    #[default]
    Text,
    Json,
    Base64,
    Multimodal,
}

/// 轨迹输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceOutput {
    /// 输出内容
    pub content: String,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Token 使用量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<TokenUsage>,
    /// 成本（美元）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// Token 使用量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// 推理链 - 记录 LLM 的思考过程
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    /// 推理步骤列表
    pub steps: Vec<ReasoningStep>,
    
    /// 最终结论
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<String>,
    
    /// 置信度 (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    
    /// 推理质量评分
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f64>,
}

/// 单个推理步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// 步骤序号
    pub step: u32,
    
    /// 推理类型
    pub reasoning_type: ReasoningType,
    
    /// 推理内容
    pub content: String,
    
    /// 相关证据/依据
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    
    /// 产生的假设
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hypotheses: Vec<String>,
    
    /// 时间戳
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

/// 推理类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningType {
    /// 问题理解
    ProblemUnderstanding,
    /// 信息收集
    InformationGathering,
    /// 假设生成
    HypothesisGeneration,
    /// 假设验证
    HypothesisValidation,
    /// 决策制定
    DecisionMaking,
    /// 计划制定
    Planning,
    /// 执行监控
    ExecutionMonitoring,
    /// 结果评估
    ResultEvaluation,
    /// 错误纠正
    ErrorCorrection,
    /// 其他
    Other,
}

/// 决策点 - 记录智能体的关键决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionPoint {
    /// 决策 ID
    pub id: String,
    
    /// 决策类型
    pub decision_type: DecisionType,
    
    /// 决策描述
    pub description: String,
    
    /// 可选方案
    pub alternatives: Vec<DecisionAlternative>,
    
    /// 选择的方案 ID
    pub chosen_alternative_id: String,
    
    /// 选择理由
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    
    /// 决策质量
    pub quality: DecisionQuality,
    
    /// 决策时间戳
    pub timestamp: u64,
}

/// 决策类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    /// 工具选择
    ToolSelection,
    /// 参数确定
    ParameterDetermination,
    /// 任务分解
    TaskDecomposition,
    /// 优先级排序
    Prioritization,
    /// 错误处理策略
    ErrorHandlingStrategy,
    /// 终止判断
    TerminationJudgment,
    /// 子任务委派
    SubtaskDelegation,
    /// 其他
    Other,
}

/// 决策备选方案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionAlternative {
    pub id: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pros: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_duration_ms: Option<u64>,
}

/// 决策质量评估
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionQuality {
    /// 是否最优
    pub is_optimal: bool,
    /// 质量分数 (0.0 - 1.0)
    pub score: f64,
    /// 改进建议
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub improvement_suggestions: Vec<String>,
}

/// 评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// 轨迹 ID
    pub trace_id: String,
    /// 总体评分
    pub overall_score: f64,
    /// 决策评分列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_scores: Vec<(String, f64)>,
    /// 推理质量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_quality: Option<f64>,
    /// 效率评分
    #[serde(skip_serializing_if = "Option::is_none")]
    pub efficiency_score: Option<f64>,
    /// 错误率
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_rate: Option<f64>,
    /// 改进建议
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    /// 评估时间
    pub evaluated_at: u64,
}

/// 轨迹查询条件
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceQuery {
    /// 文本搜索
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 轨迹类型（字符串形式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_type: Option<String>,
    /// 时间范围 (start, end)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<(u64, u64)>,
    /// 成功状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    /// 最小持续时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_duration_ms: Option<u64>,
    /// 最大持续时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration_ms: Option<u64>,
    /// 是否有错误
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_errors: Option<bool>,
    /// 运行 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// 限制数量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// 偏移量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// 聚合查询
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AggregationQuery {
    /// 成功率统计
    SuccessRate { 
        #[serde(skip_serializing_if = "Option::is_none")]
        time_range: Option<(u64, u64)> 
    },
    /// 平均耗时
    AverageDuration { 
        #[serde(skip_serializing_if = "Option::is_none")]
        time_range: Option<(u64, u64)> 
    },
    /// 轨迹类型分布
    TraceTypeDistribution { 
        #[serde(skip_serializing_if = "Option::is_none")]
        time_range: Option<(u64, u64)> 
    },
    /// Token 使用统计
    TokenUsage { 
        #[serde(skip_serializing_if = "Option::is_none")]
        time_range: Option<(u64, u64)> 
    },
    /// 成本统计
    CostStats { 
        #[serde(skip_serializing_if = "Option::is_none")]
        time_range: Option<(u64, u64)> 
    },
}

/// 聚合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AggregationResult {
    SuccessRate {
        total: u64,
        success: u64,
        rate: f64,
    },
    AverageDuration {
        avg_ms: f64,
        min_ms: u64,
        max_ms: u64,
    },
    TraceTypeDistribution {
        distribution: Vec<(String, u64)>,
    },
    TokenUsage {
        total_prompt: u64,
        total_completion: u64,
        total: u64,
    },
    CostStats {
        total_cost: f64,
        avg_cost_per_trace: f64,
    },
    Unknown,
}

/// 存储统计信息
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_traces: u64,
    pub total_reasoning_chains: u64,
    pub total_decisions: u64,
    pub total_evaluations: u64,
    pub db_size_bytes: u64,
    pub oldest_trace_timestamp: Option<u64>,
    pub newest_trace_timestamp: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_type_serialization() {
        let trace_type = TraceType::LlmCall {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
        };
        let json = serde_json::to_string(&trace_type).unwrap();
        assert!(json.contains("llm_call"));
        
        let decoded: TraceType = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.type_name(), "llm_call");
    }

    #[test]
    fn test_agent_trace_creation() {
        let trace = AgentTrace {
            id: "test-id".to_string(),
            run_id: "run-1".to_string(),
            parent_id: None,
            timestamp: 1000,
            duration_ms: 500,
            trace_type: TraceType::UserMessage,
            input: TraceInput {
                content: "Hello".to_string(),
                content_type: InputContentType::Text,
                params: HashMap::new(),
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
            metadata: serde_json::json!({}),
            reasoning: None,
            decision: None,
            evaluation: None,
        };
        
        assert_eq!(trace.id, "test-id");
        assert!(trace.output.success);
    }

    #[test]
    fn test_reasoning_chain() {
        let chain = ReasoningChain {
            steps: vec![
                ReasoningStep {
                    step: 1,
                    reasoning_type: ReasoningType::ProblemUnderstanding,
                    content: "Understanding the problem".to_string(),
                    evidence: vec!["User input".to_string()],
                    hypotheses: vec![],
                    timestamp: Some(1000),
                },
            ],
            conclusion: Some("Problem understood".to_string()),
            confidence: Some(0.9),
            quality_score: Some(0.85),
        };
        
        assert_eq!(chain.steps.len(), 1);
        assert_eq!(chain.confidence.unwrap(), 0.9);
    }
}
