/// GUI Agent 错误恢复模块
/// 
/// 本模块提供错误检测和自动恢复功能，用于处理任务执行过程中的异常情况。

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use crate::gui::planner::GuiAction;

/// 错误类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorType {
    /// 应用未启动
    AppNotStarted,
    /// 应用启动失败
    AppLaunchFailed,
    /// 窗口未找到
    WindowNotFound,
    /// 元素未找到
    ElementNotFound,
    /// 操作超时
    OperationTimeout,
    /// 操作失败
    OperationFailed,
    /// 权限拒绝
    PermissionDenied,
    /// 截图失败
    ScreenshotFailed,
    /// OCR 识别失败
    OcrFailed,
    /// LLM 调用失败
    LlmFailed,
    /// 未知错误
    Unknown,
}

/// 错误严重级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorSeverity {
    /// 低 - 可以忽略
    Low,
    /// 中 - 需要重试
    Medium,
    /// 高 - 需要回退策略
    High,
    /// 严重 - 任务必须终止
    Critical,
}

/// 错误记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    /// 错误 ID
    pub error_id: String,
    /// 错误类型
    pub error_type: ErrorType,
    /// 错误消息
    pub message: String,
    /// 严重级别
    pub severity: ErrorSeverity,
    /// 发生时间戳（毫秒）
    pub timestamp_ms: u64,
    /// 发生时的步骤 ID
    pub step_id: String,
    /// 相关上下文数据
    pub context: Option<String>,
}

impl ErrorRecord {
    /// 创建新的错误记录
    pub fn new(error_type: ErrorType, message: &str, severity: ErrorSeverity, step_id: &str) -> Self {
        Self {
            error_id: format!("err_{}", current_timestamp_ms()),
            error_type,
            message: message.to_string(),
            severity,
            timestamp_ms: current_timestamp_ms(),
            step_id: step_id.to_string(),
            context: None,
        }
    }
    
    /// 设置上下文数据
    pub fn with_context(mut self, context: &str) -> Self {
        self.context = Some(context.to_string());
        self
    }
    
    /// 检查是否需要重试
    pub fn should_retry(&self) -> bool {
        matches!(self.severity, ErrorSeverity::Low | ErrorSeverity::Medium)
    }
    
    /// 检查是否需要回退
    pub fn should_fallback(&self) -> bool {
        matches!(self.severity, ErrorSeverity::High)
    }
    
    /// 检查是否必须终止
    pub fn must_terminate(&self) -> bool {
        matches!(self.severity, ErrorSeverity::Critical)
    }
}

/// 恢复策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RecoveryStrategy {
    /// 重试当前操作
    Retry,
    /// 等待后重试
    RetryWithDelay(u64),
    /// 回退到上一个检查点
    Rollback,
    /// 使用备用方案
    UseAlternative,
    /// 跳过当前步骤
    Skip,
    /// 终止任务
    Terminate,
}

/// 恢复动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    /// 策略
    pub strategy: RecoveryStrategy,
    /// 描述
    pub description: String,
    /// 建议的新动作（如果适用）
    pub suggested_actions: Vec<GuiAction>,
}

impl RecoveryAction {
    /// 创建重试策略
    pub fn retry(description: &str) -> Self {
        Self {
            strategy: RecoveryStrategy::Retry,
            description: description.to_string(),
            suggested_actions: vec![],
        }
    }
    
    /// 创建延迟重试策略
    pub fn retry_with_delay(description: &str, delay_ms: u64) -> Self {
        Self {
            strategy: RecoveryStrategy::RetryWithDelay(delay_ms),
            description: description.to_string(),
            suggested_actions: vec![],
        }
    }
    
    /// 创建回退策略
    pub fn rollback(description: &str) -> Self {
        Self {
            strategy: RecoveryStrategy::Rollback,
            description: description.to_string(),
            suggested_actions: vec![],
        }
    }
    
    /// 创建备用方案策略
    pub fn use_alternative(description: &str, actions: Vec<GuiAction>) -> Self {
        Self {
            strategy: RecoveryStrategy::UseAlternative,
            description: description.to_string(),
            suggested_actions: actions,
        }
    }
    
    /// 创建跳过策略
    pub fn skip(description: &str) -> Self {
        Self {
            strategy: RecoveryStrategy::Skip,
            description: description.to_string(),
            suggested_actions: vec![],
        }
    }
    
    /// 创建终止策略
    pub fn terminate(description: &str) -> Self {
        Self {
            strategy: RecoveryStrategy::Terminate,
            description: description.to_string(),
            suggested_actions: vec![],
        }
    }
}

/// 错误恢复器
/// 
/// 根据错误类型和历史经验，推荐恢复策略
pub struct ErrorRecover {
    /// 错误历史记录
    error_history: Arc<RwLock<VecDeque<ErrorRecord>>>,
    /// 最大历史记录数
    max_history_size: usize,
    /// 连续错误计数
    consecutive_errors: Arc<RwLock<u32>>,
    /// 最大连续错误数
    max_consecutive_errors: u32,
}

impl ErrorRecover {
    /// 创建新的错误恢复器
    pub fn new(max_history_size: usize, max_consecutive_errors: u32) -> Self {
        Self {
            error_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history_size,
            consecutive_errors: Arc::new(RwLock::new(0)),
            max_consecutive_errors,
        }
    }
    
    /// 记录错误
    pub async fn record_error(&self, error: ErrorRecord) {
        let mut history = self.error_history.write().await;
        
        // 添加到历史记录
        if history.len() >= self.max_history_size {
            history.pop_front();
        }
        history.push_back(error.clone());
        
        // 更新连续错误计数
        let mut count = self.consecutive_errors.write().await;
        *count += 1;
    }
    
    /// 记录成功（清除连续错误计数）
    pub async fn record_success(&self) {
        let mut count = self.consecutive_errors.write().await;
        *count = 0;
    }
    
    /// 获取连续错误计数
    pub async fn get_consecutive_error_count(&self) -> u32 {
        let count = self.consecutive_errors.read().await;
        *count
    }
    
    /// 检查是否超过最大连续错误数
    pub async fn should_terminate(&self) -> bool {
        self.get_consecutive_error_count().await >= self.max_consecutive_errors
    }
    
    /// 推荐恢复策略
    pub async fn recommend_recovery(&self, error: &ErrorRecord) -> RecoveryAction {
        // 检查是否超过最大连续错误数
        if self.should_terminate().await {
            return RecoveryAction::terminate("超过最大连续错误数，任务必须终止");
        }
        
        // 根据错误类型推荐策略
        match error.error_type {
            ErrorType::AppNotStarted => {
                RecoveryAction::retry_with_delay("尝试重新启动应用", 2000)
            }
            ErrorType::AppLaunchFailed => {
                RecoveryAction::use_alternative(
                    "尝试使用备用应用或启动方式",
                    vec![],
                )
            }
            ErrorType::WindowNotFound => {
                RecoveryAction::retry_with_delay("等待窗口出现", 1000)
            }
            ErrorType::ElementNotFound => {
                // 检查历史，如果多次找不到元素，建议回退或跳过
                let history = self.error_history.read().await;
                let element_not_found_count = history.iter()
                    .filter(|e| e.error_type == ErrorType::ElementNotFound)
                    .count();
                
                if element_not_found_count >= 3 {
                    RecoveryAction::skip("元素多次未找到，跳过当前步骤")
                } else {
                    RecoveryAction::retry_with_delay("重新尝试查找元素", 500)
                }
            }
            ErrorType::OperationTimeout => {
                RecoveryAction::retry_with_delay("等待后重试操作", 2000)
            }
            ErrorType::OperationFailed => {
                RecoveryAction::retry("重试操作")
            }
            ErrorType::PermissionDenied => {
                RecoveryAction::terminate("权限不足，无法继续执行")
            }
            ErrorType::ScreenshotFailed => {
                RecoveryAction::retry_with_delay("等待后重新截图", 1000)
            }
            ErrorType::OcrFailed => {
                RecoveryAction::use_alternative(
                    "OCR 失败，尝试使用 LLM 直接理解屏幕",
                    vec![],
                )
            }
            ErrorType::LlmFailed => {
                RecoveryAction::retry_with_delay("等待后重试 LLM 调用", 3000)
            }
            ErrorType::Unknown => {
                if error.severity == ErrorSeverity::Critical {
                    RecoveryAction::terminate("未知严重错误")
                } else {
                    RecoveryAction::retry("重试未知操作")
                }
            }
        }
    }
    
    /// 获取错误历史
    pub async fn get_error_history(&self) -> Vec<ErrorRecord> {
        let history = self.error_history.read().await;
        history.iter().cloned().collect()
    }
    
    /// 清理历史记录
    pub async fn clear_history(&self) {
        let mut history = self.error_history.write().await;
        history.clear();
        
        let mut count = self.consecutive_errors.write().await;
        *count = 0;
    }
    
    /// 分析错误模式
    pub async fn analyze_error_patterns(&self) -> ErrorPatternAnalysis {
        let history = self.error_history.read().await;
        
        let mut error_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let mut severity_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        
        for error in history.iter() {
            let type_name = format!("{:?}", error.error_type);
            *error_counts.entry(type_name).or_insert(0) += 1;
            
            let severity_name = format!("{:?}", error.severity);
            *severity_counts.entry(severity_name).or_insert(0) += 1;
        }
        
        ErrorPatternAnalysis {
            total_errors: history.len() as u32,
            error_counts,
            severity_counts,
            consecutive_errors: *self.consecutive_errors.read().await,
        }
    }
}

/// 错误模式分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPatternAnalysis {
    /// 总错误数
    pub total_errors: u32,
    /// 各类型错误数量
    pub error_counts: std::collections::HashMap<String, u32>,
    /// 各严重级别错误数量
    pub severity_counts: std::collections::HashMap<String, u32>,
    /// 连续错误数
    pub consecutive_errors: u32,
}

/// 获取当前时间戳（毫秒）
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_record_creation() {
        let error = ErrorRecord::new(
            ErrorType::ElementNotFound,
            "按钮未找到",
            ErrorSeverity::Medium,
            "step_1",
        );
        
        assert_eq!(error.error_type, ErrorType::ElementNotFound);
        assert!(error.should_retry());
        assert!(!error.should_fallback());
        assert!(!error.must_terminate());
    }
    
    #[test]
    fn test_error_severity() {
        let low_error = ErrorRecord::new(ErrorType::Unknown, "低", ErrorSeverity::Low, "s1");
        let medium_error = ErrorRecord::new(ErrorType::Unknown, "中", ErrorSeverity::Medium, "s1");
        let high_error = ErrorRecord::new(ErrorType::Unknown, "高", ErrorSeverity::High, "s1");
        let critical_error = ErrorRecord::new(ErrorType::Unknown, "严重", ErrorSeverity::Critical, "s1");
        
        assert!(low_error.should_retry());
        assert!(medium_error.should_retry());
        assert!(high_error.should_fallback());
        assert!(critical_error.must_terminate());
    }
    
    #[test]
    fn test_recovery_action() {
        let action = RecoveryAction::retry("重试");
        assert_eq!(action.strategy, RecoveryStrategy::Retry);
        
        let action2 = RecoveryAction::retry_with_delay("延迟重试", 1000);
        assert_eq!(action2.strategy, RecoveryStrategy::RetryWithDelay(1000));
        
        let action3 = RecoveryAction::terminate("终止");
        assert_eq!(action3.strategy, RecoveryStrategy::Terminate);
    }
    
    #[tokio::test]
    async fn test_error_recover() {
        let recover = ErrorRecover::new(10, 3);
        
        // 记录一些错误
        let error1 = ErrorRecord::new(ErrorType::ElementNotFound, "未找到", ErrorSeverity::Medium, "s1");
        recover.record_error(error1).await;
        
        let error2 = ErrorRecord::new(ErrorType::OperationTimeout, "超时", ErrorSeverity::Medium, "s2");
        recover.record_error(error2).await;
        
        // 检查连续错误计数
        let count = recover.get_consecutive_error_count().await;
        assert_eq!(count, 2);
        
        // 记录成功后清除计数
        recover.record_success().await;
        let count = recover.get_consecutive_error_count().await;
        assert_eq!(count, 0);
        
        // 测试终止条件
        for _ in 0..3 {
            let error = ErrorRecord::new(ErrorType::Unknown, "错", ErrorSeverity::Medium, "s");
            recover.record_error(error).await;
        }
        
        let should_term = recover.should_terminate().await;
        assert!(should_term);
    }
    
    #[tokio::test]
    async fn test_recommend_recovery() {
        let recover = ErrorRecover::new(10, 3);
        
        let error = ErrorRecord::new(ErrorType::AppNotStarted, "应用未启动", ErrorSeverity::Medium, "s1");
        let action = recover.recommend_recovery(&error).await;
        
        // 应该是延迟重试
        match action.strategy {
            RecoveryStrategy::RetryWithDelay(_) => {}
            _ => panic!("应该推荐延迟重试策略"),
        }
    }
}
