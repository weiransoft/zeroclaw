//! 工作流事件驱动模块
//! 
//! 提供基于事件的工作流触发和管理功能

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tracing::{info, debug, warn, error};
use serde::{Deserialize, Serialize};

use crate::workflow::{WorkflowEngine, WorkflowScheduler};

/// 事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventType {
    /// 系统事件
    System,
    /// Webhook 事件
    Webhook,
    /// 工作流状态变更事件
    WorkflowStatusChange,
    /// 步骤状态变更事件
    StepStatusChange,
    /// 自定义事件
    Custom,
    /// 工作流开始事件
    WorkflowStarted,
    /// 工作流完成事件
    WorkflowCompleted,
    /// 工作流失败事件
    WorkflowFailed,
    /// 步骤开始事件
    StepStarted,
    /// 步骤完成事件
    StepCompleted,
    /// 步骤失败事件
    StepFailed,
}

/// 事件结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEvent {
    /// 事件 ID
    pub id: String,
    /// 事件类型
    pub event_type: EventType,
    /// 事件来源
    pub source: String,
    /// 事件数据
    pub data: serde_json::Value,
    /// 事件时间
    pub timestamp: DateTime<Utc>,
    /// 相关工作流 ID（可选）
    pub workflow_id: Option<String>,
    /// 相关步骤 ID（可选）
    pub step_id: Option<String>,
}

/// 事件监听器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventListener {
    /// 监听器 ID
    pub id: String,
    /// 事件类型
    pub event_type: EventType,
    /// 监听条件
    pub condition: Option<String>,
    /// 触发的工作流 ID
    pub workflow_id: String,
    /// 是否启用
    pub enabled: bool,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后修改时间
    pub updated_at: DateTime<Utc>,
}

/// 事件总线
#[derive(Debug, Clone)]
pub struct EventBus {
    /// 事件监听器
    listeners: Arc<RwLock<Vec<EventListener>>>,
    /// 事件发送通道
    event_tx: broadcast::Sender<WorkflowEvent>,
    /// 工作流引擎
    workflow_engine: Arc<WorkflowEngine>,
    /// 工作流调度器
    workflow_scheduler: Arc<WorkflowScheduler>,
}

impl EventBus {
    /// 创建事件总线
    ///
    /// # Arguments
    /// * `workflow_engine` - 工作流引擎
    /// * `workflow_scheduler` - 工作流调度器
    ///
    /// # Returns
    /// 事件总线实例
    pub fn new(
        workflow_engine: Arc<WorkflowEngine>,
        workflow_scheduler: Arc<WorkflowScheduler>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        
        Self {
            listeners: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            workflow_engine,
            workflow_scheduler,
        }
    }

    /// 启动事件总线
    pub async fn start(&self) {
        info!("Starting event bus");
        
        let mut rx = self.event_tx.subscribe();
        let listeners = self.listeners.clone();
        let workflow_engine = self.workflow_engine.clone();
        
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                Self::process_event(event, listeners.clone(), workflow_engine.clone()).await;
            }
        });
    }

    /// 处理事件
    ///
    /// # Arguments
    /// * `event` - 工作流事件
    /// * `listeners` - 事件监听器
    /// * `workflow_engine` - 工作流引擎
    async fn process_event(
        event: WorkflowEvent,
        listeners: Arc<RwLock<Vec<EventListener>>>,
        workflow_engine: Arc<WorkflowEngine>,
    ) {
        debug!("Processing event: {:?}", event);
        
        // 获取监听器
        let current_listeners = listeners.read().await;
        
        // 查找匹配的监听器
        for listener in current_listeners.iter() {
            if !listener.enabled {
                continue;
            }
            
            // 检查事件类型是否匹配
            if listener.event_type != event.event_type {
                continue;
            }
            
            // 检查条件是否匹配（如果有）
            if let Some(condition) = &listener.condition {
                if !Self::evaluate_condition(condition, &event).await {
                    continue;
                }
            }
            
            // 触发工作流
            info!(
                "Event {} triggered workflow {} via listener {}",
                event.id,
                listener.workflow_id,
                listener.id
            );
            
            // 启动工作流
            if let Err(e) = workflow_engine.start_workflow(&listener.workflow_id).await {
                error!(
                    "Failed to start workflow {} for event {}: {}",
                    listener.workflow_id,
                    event.id,
                    e
                );
            }
        }
    }

    /// 评估条件
    ///
    /// # Arguments
    /// * `condition` - 条件表达式
    /// * `event` - 工作流事件
    ///
    /// # Returns
    /// 条件是否满足
    async fn evaluate_condition(condition: &str, event: &WorkflowEvent) -> bool {
        debug!("Evaluating condition: {} for event: {:?}", condition, event);
        
        // 解析条件表达式，支持基本的比较操作
        // 格式: "field op value"，例如 "event_type == workflow_completed"
        let parts: Vec<&str> = condition.trim().split_whitespace().collect();
        if parts.len() != 3 {
            warn!("Invalid condition format: {}. Expected 'field op value'", condition);
            return false;
        }
        
        let field = parts[0];
        let op = parts[1];
        let value = parts[2];
        
        match field {
            "event_type" => {
                let event_type_str = match event.event_type {
                    EventType::System => "system",
                    EventType::Webhook => "webhook",
                    EventType::WorkflowStatusChange => "workflow_status_change",
                    EventType::StepStatusChange => "step_status_change",
                    EventType::WorkflowStarted => "workflow_started",
                    EventType::WorkflowCompleted => "workflow_completed",
                    EventType::WorkflowFailed => "workflow_failed",
                    EventType::StepStarted => "step_started",
                    EventType::StepCompleted => "step_completed",
                    EventType::StepFailed => "step_failed",
                    EventType::Custom => "custom",
                };
                
                match op {
                    "==" => event_type_str == value,
                    "!=" => event_type_str != value,
                    _ => {
                        warn!("Unsupported operator for event_type: {}", op);
                        false
                    }
                }
            },
            "workflow_id" => {
                match op {
                    "==" => event.workflow_id.as_deref().unwrap_or("") == value,
                    "!=" => event.workflow_id.as_deref().unwrap_or("") != value,
                    _ => {
                        warn!("Unsupported operator for workflow_id: {}", op);
                        false
                    }
                }
            },
            "step_id" => {
                match op {
                    "==" => event.step_id.as_deref().unwrap_or("") == value,
                    "!=" => event.step_id.as_deref().unwrap_or("") != value,
                    _ => {
                        warn!("Unsupported operator for step_id: {}", op);
                        false
                    }
                }
            },
            _ => {
                warn!("Unknown field in condition: {}", field);
                false
            }
        }
    }

    /// 发布事件
    ///
    /// # Arguments
    /// * `event` - 工作流事件
    ///
    /// # Returns
    /// 操作结果
    pub async fn publish_event(&self, event: WorkflowEvent) -> Result<()> {
        debug!("Publishing event: {:?}", event);
        
        match self.event_tx.send(event) {
            Ok(_) => {
                info!("Event published successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to publish event: {}", e);
                Err(anyhow!("Failed to publish event: {}", e))
            }
        }
    }

    /// 添加事件监听器
    ///
    /// # Arguments
    /// * `listener` - 事件监听器
    ///
    /// # Returns
    /// 操作结果
    pub async fn add_listener(&self, listener: EventListener) -> Result<()> {
        info!("Adding event listener: {:?}", listener);
        
        let mut listeners = self.listeners.write().await;
        listeners.push(listener);
        
        Ok(())
    }

    /// 移除事件监听器
    ///
    /// # Arguments
    /// * `listener_id` - 监听器 ID
    ///
    /// # Returns
    /// 操作结果
    pub async fn remove_listener(&self, listener_id: &str) -> Result<()> {
        info!("Removing event listener: {}", listener_id);
        
        let mut listeners = self.listeners.write().await;
        let initial_len = listeners.len();
        
        listeners.retain(|l| l.id != listener_id);
        
        if listeners.len() == initial_len {
            return Err(anyhow!("Listener not found: {}", listener_id));
        }
        
        Ok(())
    }

    /// 更新事件监听器
    ///
    /// # Arguments
    /// * `listener` - 事件监听器
    ///
    /// # Returns
    /// 操作结果
    pub async fn update_listener(&self, listener: EventListener) -> Result<()> {
        info!("Updating event listener: {:?}", listener);
        
        let mut listeners = self.listeners.write().await;
        
        for l in listeners.iter_mut() {
            if l.id == listener.id {
                *l = listener;
                return Ok(());
            }
        }
        
        Err(anyhow!("Listener not found: {}", listener.id))
    }

    /// 获取所有事件监听器
    ///
    /// # Returns
    /// 事件监听器列表
    pub async fn get_listeners(&self) -> Result<Vec<EventListener>> {
        let listeners = self.listeners.read().await;
        Ok(listeners.clone())
    }

    /// 获取指定事件类型的监听器
    ///
    /// # Arguments
    /// * `event_type` - 事件类型
    ///
    /// # Returns
    /// 事件监听器列表
    pub async fn get_listeners_by_type(&self, event_type: EventType) -> Result<Vec<EventListener>> {
        let listeners = self.listeners.read().await;
        let filtered = listeners
            .iter()
            .filter(|l| l.event_type == event_type)
            .cloned()
            .collect();
        Ok(filtered)
    }

    /// 获取指定工作流的监听器
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流 ID
    ///
    /// # Returns
    /// 事件监听器列表
    pub async fn get_listeners_by_workflow(&self, workflow_id: &str) -> Result<Vec<EventListener>> {
        let listeners = self.listeners.read().await;
        let filtered = listeners
            .iter()
            .filter(|l| l.workflow_id == workflow_id)
            .cloned()
            .collect();
        Ok(filtered)
    }
}

/// 事件工具函数
pub mod utils {
    use super::*;
    use uuid::Uuid;

    /// 创建系统事件
    ///
    /// # Arguments
    /// * `source` - 事件来源
    /// * `data` - 事件数据
    ///
    /// # Returns
    /// 工作流事件
    pub fn create_system_event(source: &str, data: serde_json::Value) -> WorkflowEvent {
        WorkflowEvent {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::System,
            source: source.to_string(),
            data,
            timestamp: Utc::now(),
            workflow_id: None,
            step_id: None,
        }
    }

    /// 创建 Webhook 事件
    ///
    /// # Arguments
    /// * `source` - 事件来源
    /// * `data` - 事件数据
    ///
    /// # Returns
    /// 工作流事件
    pub fn create_webhook_event(source: &str, data: serde_json::Value) -> WorkflowEvent {
        WorkflowEvent {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::Webhook,
            source: source.to_string(),
            data,
            timestamp: Utc::now(),
            workflow_id: None,
            step_id: None,
        }
    }

    /// 创建工作流状态变更事件
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流 ID
    /// * `old_status` - 旧状态
    /// * `new_status` - 新状态
    ///
    /// # Returns
    /// 工作流事件
    pub fn create_workflow_status_event(
        workflow_id: &str,
        old_status: &str,
        new_status: &str,
    ) -> WorkflowEvent {
        let data = serde_json::json!({
            "old_status": old_status,
            "new_status": new_status
        });
        
        WorkflowEvent {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::WorkflowStatusChange,
            source: "workflow_engine".to_string(),
            data,
            timestamp: Utc::now(),
            workflow_id: Some(workflow_id.to_string()),
            step_id: None,
        }
    }

    /// 创建步骤状态变更事件
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流 ID
    /// * `step_id` - 步骤 ID
    /// * `old_status` - 旧状态
    /// * `new_status` - 新状态
    ///
    /// # Returns
    /// 工作流事件
    pub fn create_step_status_event(
        workflow_id: &str,
        step_id: &str,
        old_status: &str,
        new_status: &str,
    ) -> WorkflowEvent {
        let data = serde_json::json!({
            "old_status": old_status,
            "new_status": new_status
        });
        
        WorkflowEvent {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::StepStatusChange,
            source: "workflow_engine".to_string(),
            data,
            timestamp: Utc::now(),
            workflow_id: Some(workflow_id.to_string()),
            step_id: Some(step_id.to_string()),
        }
    }

    /// 创建自定义事件
    ///
    /// # Arguments
    /// * `source` - 事件来源
    /// * `data` - 事件数据
    /// * `workflow_id` - 工作流 ID（可选）
    /// * `step_id` - 步骤 ID（可选）
    ///
    /// # Returns
    /// 工作流事件
    pub fn create_custom_event(
        source: &str,
        data: serde_json::Value,
        workflow_id: Option<&str>,
        step_id: Option<&str>,
    ) -> WorkflowEvent {
        WorkflowEvent {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::Custom,
            source: source.to_string(),
            data,
            timestamp: Utc::now(),
            workflow_id: workflow_id.map(|s| s.to_string()),
            step_id: step_id.map(|s| s.to_string()),
        }
    }
}
