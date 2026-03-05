//! 工作流引擎
//!
//! 负责工作流的自动执行、步骤转换和状态管理

use anyhow::{Context, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::store::workflow::WorkflowStore;

/// 工作流引擎
///
/// 负责工作流的自动执行、步骤转换和状态管理
#[derive(Debug)]
pub struct WorkflowEngine {
    /// 工作流存储
    workflow_store: Arc<WorkflowStore>,
    /// 工作流执行状态缓存
    execution_state: Arc<RwLock<ExecutionState>>,
}

/// 工作流执行状态
#[derive(Debug, Clone)]
struct ExecutionState {
    /// 正在执行的工作流ID列表
    running_workflows: Vec<String>,
    /// 暂停的工作流ID列表
    paused_workflows: Vec<String>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            running_workflows: Vec::new(),
            paused_workflows: Vec::new(),
        }
    }
}

impl WorkflowEngine {
    /// 创建新的工作流引擎
    ///
    /// # Arguments
    /// * `workflow_store` - 工作流存储
    ///
    /// # Returns
    /// 工作流引擎实例
    pub fn new(workflow_store: Arc<WorkflowStore>) -> Self {
        Self {
            workflow_store,
            execution_state: Arc::new(RwLock::new(ExecutionState::default())),
        }
    }

    /// 启动工作流
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    ///
    /// # Returns
    /// 操作结果
    pub async fn start_workflow(&self, workflow_id: &str) -> Result<()> {
        debug!("Starting workflow: {}", workflow_id);

        // 获取工作流
        let workflow = self
            .workflow_store
            .get_workflow(workflow_id)
            .context("Failed to get workflow")?
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        // 检查工作流状态
        if workflow.status != "created" && workflow.status != "paused" {
            return Err(anyhow::anyhow!(
                "Workflow cannot be started from status: {}",
                workflow.status
            ));
        }

        // 更新工作流状态为运行中
        self.workflow_store
            .update_workflow(workflow_id, None, None, Some("running"), None)
            .context("Failed to update workflow status")?;

        // 添加到运行状态
        let mut state = self.execution_state.write().await;
        state.running_workflows.push(workflow_id.to_string());
        state.paused_workflows.retain(|id| id != workflow_id);

        info!("Workflow started: {}", workflow_id);

        // 自动启动第一个步骤
        self.auto_start_next_step(workflow_id).await?;

        Ok(())
    }

    /// 暂停工作流
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    ///
    /// # Returns
    /// 操作结果
    pub async fn pause_workflow(&self, workflow_id: &str) -> Result<()> {
        debug!("Pausing workflow: {}", workflow_id);

        // 获取工作流
        let workflow = self
            .workflow_store
            .get_workflow(workflow_id)
            .context("Failed to get workflow")?
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        // 检查工作流状态
        if workflow.status != "running" {
            return Err(anyhow::anyhow!(
                "Workflow cannot be paused from status: {}",
                workflow.status
            ));
        }

        // 更新工作流状态为暂停
        self.workflow_store
            .update_workflow(workflow_id, None, None, Some("paused"), None)
            .context("Failed to update workflow status")?;

        // 更新执行状态
        let mut state = self.execution_state.write().await;
        state.running_workflows.retain(|id| id != workflow_id);
        state.paused_workflows.push(workflow_id.to_string());

        info!("Workflow paused: {}", workflow_id);

        Ok(())
    }

    /// 恢复工作流
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    ///
    /// # Returns
    /// 操作结果
    pub async fn resume_workflow(&self, workflow_id: &str) -> Result<()> {
        debug!("Resuming workflow: {}", workflow_id);

        // 获取工作流
        let workflow = self
            .workflow_store
            .get_workflow(workflow_id)
            .context("Failed to get workflow")?
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        // 检查工作流状态
        if workflow.status != "paused" {
            return Err(anyhow::anyhow!(
                "Workflow cannot be resumed from status: {}",
                workflow.status
            ));
        }

        // 更新工作流状态为运行中
        self.workflow_store
            .update_workflow(workflow_id, None, None, Some("running"), None)
            .context("Failed to update workflow status")?;

        // 更新执行状态
        let mut state = self.execution_state.write().await;
        state.running_workflows.push(workflow_id.to_string());
        state.paused_workflows.retain(|id| id != workflow_id);

        info!("Workflow resumed: {}", workflow_id);

        // 自动启动下一个步骤
        self.auto_start_next_step(workflow_id).await?;

        Ok(())
    }

    /// 停止工作流
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    ///
    /// # Returns
    /// 操作结果
    pub async fn stop_workflow(&self, workflow_id: &str) -> Result<()> {
        debug!("Stopping workflow: {}", workflow_id);

        // 获取工作流
        let workflow = self
            .workflow_store
            .get_workflow(workflow_id)
            .context("Failed to get workflow")?
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        // 检查工作流状态
        if workflow.status != "running" && workflow.status != "paused" {
            return Err(anyhow::anyhow!(
                "Workflow cannot be stopped from status: {}",
                workflow.status
            ));
        }

        // 更新工作流状态为已取消
        self.workflow_store
            .update_workflow(workflow_id, None, None, Some("cancelled"), None)
            .context("Failed to update workflow status")?;

        // 从执行状态中移除
        let mut state = self.execution_state.write().await;
        state.running_workflows.retain(|id| id != workflow_id);
        state.paused_workflows.retain(|id| id != workflow_id);

        info!("Workflow stopped: {}", workflow_id);

        Ok(())
    }

    /// 完成步骤
    ///
    /// # Arguments
    /// * `step_id` - 步骤ID
    ///
    /// # Returns
    /// 操作结果
    pub async fn complete_step(&self, step_id: &str) -> Result<()> {
        debug!("Completing step: {}", step_id);

        // 获取步骤所属的工作流ID
        let workflow_id = self.get_workflow_id_by_step(step_id)?;

        // 更新步骤状态为完成
        let _now = Utc::now().timestamp_millis();
        self.workflow_store
            .update_step(step_id, None, None, Some("completed"), None, None)
            .context("Failed to update step status")?;

        info!("Step completed: {}", step_id);

        // 自动启动下一个步骤
        self.auto_start_next_step(&workflow_id).await?;

        Ok(())
    }

    /// 失败步骤
    ///
    /// # Arguments
    /// * `step_id` - 步骤ID
    /// * `error_message` - 错误信息
    ///
    /// # Returns
    /// 操作结果
    pub async fn fail_step(&self, step_id: &str, error_message: &str) -> Result<()> {
        debug!("Failing step: {}", step_id);

        // 获取步骤所属的工作流ID
        let workflow_id = self.get_workflow_id_by_step(step_id)?;

        // 创建包含错误信息的元数据
        let error_metadata = serde_json::json!({
            "error": error_message,
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "status": "failed"
        });
        let metadata_str = serde_json::to_string(&error_metadata)?;

        // 更新步骤状态为失败并记录错误信息
        self.workflow_store
            .update_step(step_id, None, None, Some("failed"), None, Some(&metadata_str))
            .context("Failed to update step status")?;

        error!("Step failed: {} - {}", step_id, error_message);

        // 停止工作流
        self.stop_workflow(&workflow_id).await?;

        Ok(())
    }

    /// 自动启动下一个步骤
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    ///
    /// # Returns
    /// 操作结果
    async fn auto_start_next_step(&self, workflow_id: &str) -> Result<()> {
        debug!("Auto-starting next step for workflow: {}", workflow_id);

        // 获取工作流
        let workflow = self
            .workflow_store
            .get_workflow(workflow_id)
            .context("Failed to get workflow")?
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        // 检查工作流状态
        if workflow.status != "running" {
            debug!("Workflow is not running, skipping auto-start: {}", workflow_id);
            return Ok(());
        }

        // 查找下一个待执行的步骤
        let next_step = workflow
            .steps
            .iter()
            .find(|step| step.status == "pending");

        if let Some(step) = next_step {
            // 启动下一个步骤
            let _now = Utc::now().timestamp_millis();
            self.workflow_store
                    .update_step(&step.id, None, None, Some("running"), None, None)
                .context("Failed to update step status")?;

            info!(
                "Auto-started step: {} for workflow: {}",
                step.name, workflow_id
            );
        } else {
            // 检查是否所有步骤都已完成
            let all_completed = workflow
                .steps
                .iter()
                .all(|step| step.status == "completed");

            if all_completed {
                // 标记工作流为完成
                self.workflow_store
                    .update_workflow(workflow_id, None, None, Some("completed"), None)
                    .context("Failed to update workflow status")?;

                // 从执行状态中移除
                let mut state = self.execution_state.write().await;
                state.running_workflows.retain(|id| id != workflow_id);

                info!("Workflow completed: {}", workflow_id);
            }
        }

        Ok(())
    }

    /// 根据步骤ID获取工作流ID
    ///
    /// # Arguments
    /// * `step_id` - 步骤ID
    ///
    /// # Returns
    /// 工作流ID
    fn get_workflow_id_by_step(&self, step_id: &str) -> Result<String> {
        self.workflow_store.get_workflow_id_by_step(step_id)
    }

    /// 获取正在运行的工作流列表
    ///
    /// # Returns
    /// 正在运行的工作流ID列表
    pub async fn get_running_workflows(&self) -> Vec<String> {
        let state = self.execution_state.read().await;
        state.running_workflows.clone()
    }

    /// 获取暂停的工作流列表
    ///
    /// # Returns
    /// 暂停的工作流ID列表
    pub async fn get_paused_workflows(&self) -> Vec<String> {
        let state = self.execution_state.read().await;
        state.paused_workflows.clone()
    }
}
