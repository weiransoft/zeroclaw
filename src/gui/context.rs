/// GUI Agent 上下文管理模块
/// 
/// 本模块提供任务执行过程中的上下文跟踪和管理功能，
/// 用于维护长期任务的执行状态和历史记录。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// 等待中
    Pending,
    /// 运行中
    Running,
    /// 已暂停
    Paused,
    /// 成功完成
    Completed,
    /// 执行失败
    Failed,
    /// 已取消
    Cancelled,
}

/// 执行步骤记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    /// 步骤 ID
    pub step_id: String,
    /// 步骤描述
    pub description: String,
    /// 开始时间戳
    pub start_timestamp_ms: u64,
    /// 结束时间戳
    pub end_timestamp_ms: Option<u64>,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error_message: Option<String>,
    /// 执行的屏幕截图 ID
    pub screen_capture_id: Option<String>,
}

impl ExecutionStep {
    /// 创建新的执行步骤
    pub fn new(step_id: &str, description: &str) -> Self {
        Self {
            step_id: step_id.to_string(),
            description: description.to_string(),
            start_timestamp_ms: current_timestamp_ms(),
            end_timestamp_ms: None,
            success: false,
            error_message: None,
            screen_capture_id: None,
        }
    }
    
    /// 标记为成功完成
    pub fn complete(&mut self) {
        self.end_timestamp_ms = Some(current_timestamp_ms());
        self.success = true;
    }
    
    /// 标记为失败
    pub fn fail(&mut self, error: &str) {
        self.end_timestamp_ms = Some(current_timestamp_ms());
        self.success = false;
        self.error_message = Some(error.to_string());
    }
    
    /// 获取执行耗时（毫秒）
    pub fn duration_ms(&self) -> Option<u64> {
        self.end_timestamp_ms.map(|end| end - self.start_timestamp_ms)
    }
}

/// GUI 任务上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiTaskContext {
    /// 任务 ID
    pub task_id: String,
    /// 任务描述
    pub task_description: String,
    /// 当前状态
    pub status: TaskStatus,
    /// 计划的动作序列
    pub planned_actions: Vec<String>,
    /// 已执行的步骤
    pub executed_steps: Vec<ExecutionStep>,
    /// 当前步骤索引
    pub current_step_index: usize,
    /// 任务开始时间戳
    pub start_timestamp_ms: u64,
    /// 任务结束时间戳
    pub end_timestamp_ms: Option<u64>,
    /// 上下文数据（用于存储自定义数据）
    pub context_data: HashMap<String, String>,
    /// 重试计数
    pub retry_count: u32,
    /// 最大重试次数
    pub max_retries: u32,
}

impl GuiTaskContext {
    /// 创建新的任务上下文
    pub fn new(task_id: &str, task_description: &str, planned_actions: Vec<String>, max_retries: u32) -> Self {
        Self {
            task_id: task_id.to_string(),
            task_description: task_description.to_string(),
            status: TaskStatus::Pending,
            planned_actions,
            executed_steps: Vec::new(),
            current_step_index: 0,
            start_timestamp_ms: current_timestamp_ms(),
            end_timestamp_ms: None,
            context_data: HashMap::new(),
            retry_count: 0,
            max_retries,
        }
    }
    
    /// 开始任务执行
    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
    }
    
    /// 添加执行步骤
    pub fn add_step(&mut self, step: ExecutionStep) {
        self.executed_steps.push(step);
        self.current_step_index = self.executed_steps.len();
    }
    
    /// 完成当前步骤
    pub fn complete_current_step(&mut self, success: bool, error: Option<String>) {
        if let Some(step) = self.executed_steps.last_mut() {
            if success {
                step.complete();
            } else {
                step.fail(error.as_deref().unwrap_or("未知错误"));
            }
        }
    }
    
    /// 检查是否可以重试
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }
    
    /// 增加重试计数
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
    
    /// 重置任务以进行重试
    pub fn reset_for_retry(&mut self) {
        self.status = TaskStatus::Pending;
        self.current_step_index = 0;
        self.executed_steps.clear();
    }
    
    /// 完成任务
    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
        self.end_timestamp_ms = Some(current_timestamp_ms());
    }
    
    /// 标记任务失败
    pub fn fail(&mut self, error: &str) {
        self.status = TaskStatus::Failed;
        self.end_timestamp_ms = Some(current_timestamp_ms());
        // 添加失败步骤
        let mut step = ExecutionStep::new("final_error", "任务执行失败");
        step.fail(error);
        self.executed_steps.push(step);
    }
    
    /// 取消任务
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.end_timestamp_ms = Some(current_timestamp_ms());
    }
    
    /// 获取任务总耗时（毫秒）
    pub fn total_duration_ms(&self) -> Option<u64> {
        self.end_timestamp_ms.map(|end| end - self.start_timestamp_ms)
    }
    
    /// 获取上下文数据
    pub fn get_data(&self, key: &str) -> Option<&String> {
        self.context_data.get(key)
    }
    
    /// 设置上下文数据
    pub fn set_data(&mut self, key: &str, value: &str) {
        self.context_data.insert(key.to_string(), value.to_string());
    }
}

/// 上下文管理器
/// 
/// 管理多个 GUI 任务的执行上下文
pub struct ContextManager {
    /// 任务上下文映射
    contexts: Arc<RwLock<HashMap<String, GuiTaskContext>>>,
    /// 当前活跃任务 ID
    active_task_id: Arc<RwLock<Option<String>>>,
}

impl ContextManager {
    /// 创建新的上下文管理器
    pub fn new() -> Self {
        Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
            active_task_id: Arc::new(RwLock::new(None)),
        }
    }
    
    /// 创建新任务上下文
    pub async fn create_context(
        &self,
        task_id: &str,
        task_description: &str,
        planned_actions: Vec<String>,
        max_retries: u32,
    ) -> GuiTaskContext {
        let context = GuiTaskContext::new(task_id, task_description, planned_actions, max_retries);
        
        let mut contexts = self.contexts.write().await;
        contexts.insert(task_id.to_string(), context.clone());
        
        context
    }
    
    /// 获取任务上下文
    pub async fn get_context(&self, task_id: &str) -> Option<GuiTaskContext> {
        let contexts = self.contexts.read().await;
        contexts.get(task_id).cloned()
    }
    
    /// 更新任务上下文
    pub async fn update_context(&self, context: &GuiTaskContext) {
        let mut contexts = self.contexts.write().await;
        contexts.insert(context.task_id.clone(), context.clone());
    }
    
    /// 删除任务上下文
    pub async fn remove_context(&self, task_id: &str) {
        let mut contexts = self.contexts.write().await;
        contexts.remove(task_id);
    }
    
    /// 设置活跃任务
    pub async fn set_active_task(&self, task_id: &str) {
        let mut active = self.active_task_id.write().await;
        *active = Some(task_id.to_string());
    }
    
    /// 获取活跃任务 ID
    pub async fn get_active_task_id(&self) -> Option<String> {
        let active = self.active_task_id.read().await;
        active.clone()
    }
    
    /// 获取活跃任务上下文
    pub async fn get_active_context(&self) -> Option<GuiTaskContext> {
        let task_id = self.get_active_task_id().await?;
        self.get_context(&task_id).await
    }
    
    /// 列出所有任务
    pub async fn list_tasks(&self) -> Vec<GuiTaskContext> {
        let contexts = self.contexts.read().await;
        contexts.values().cloned().collect()
    }
    
    /// 清理已完成的任务上下文
    pub async fn cleanup_completed(&self, max_age_ms: u64) {
        let now = current_timestamp_ms();
        let mut contexts = self.contexts.write().await;
        
        contexts.retain(|_, context| {
            if let Some(end_time) = context.end_timestamp_ms {
                // 保留未完成的任务和最近完成的任务
                now - end_time < max_age_ms || context.status != TaskStatus::Completed
            } else {
                true
            }
        });
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
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
    fn test_execution_step() {
        let mut step = ExecutionStep::new("step1", "测试步骤");
        
        assert!(!step.success);
        assert!(step.end_timestamp_ms.is_none());
        
        // 完成步骤
        step.complete();
        assert!(step.success);
        assert!(step.end_timestamp_ms.is_some());
        assert!(step.duration_ms().is_some());
    }
    
    #[test]
    fn test_execution_step_fail() {
        let mut step = ExecutionStep::new("step1", "测试步骤");
        
        step.fail("测试错误");
        
        assert!(!step.success);
        assert_eq!(step.error_message, Some("测试错误".to_string()));
    }
    
    #[test]
    fn test_task_context_creation() {
        let context = GuiTaskContext::new(
            "task1",
            "测试任务",
            vec!["action1".to_string(), "action2".to_string()],
            3,
        );
        
        assert_eq!(context.task_id, "task1");
        assert_eq!(context.status, TaskStatus::Pending);
        assert_eq!(context.planned_actions.len(), 2);
        assert_eq!(context.retry_count, 0);
    }
    
    #[test]
    fn test_task_context_retry() {
        let mut context = GuiTaskContext::new("task1", "测试任务", vec![], 3);
        
        assert!(context.can_retry());
        
        context.increment_retry();
        assert_eq!(context.retry_count, 1);
        
        context.increment_retry();
        context.increment_retry();
        assert!(!context.can_retry());
    }
    
    #[tokio::test]
    async fn test_context_manager() {
        let manager = ContextManager::new();
        
        // 创建上下文
        let context = manager.create_context(
            "task1",
            "测试任务",
            vec!["action1".to_string()],
            3,
        ).await;
        
        assert_eq!(context.task_id, "task1");
        
        // 获取上下文
        let retrieved = manager.get_context("task1").await;
        assert!(retrieved.is_some());
        
        // 设置活跃任务
        manager.set_active_task("task1").await;
        let active_id = manager.get_active_task_id().await;
        assert_eq!(active_id, Some("task1".to_string()));
    }
}
