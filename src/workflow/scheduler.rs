//! 工作流调度器
//!
//! 负责工作流的周期性执行和调度管理

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, MissedTickBehavior, Duration as TokioDuration};
use tracing::{debug, error, info, warn};

use crate::store::workflow::WorkflowStore;
use crate::workflow::WorkflowEngine;

/// 周期性工作流配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScheduledWorkflow {
    /// 工作流ID
    pub workflow_id: String,
    /// 调度表达式（cron 表达式或间隔时间）
    pub schedule: String,
    /// 调度类型：cron 或 interval
    pub schedule_type: ScheduleType,
    /// 是否启用
    pub enabled: bool,
    /// 下次执行时间
    pub next_run: DateTime<Utc>,
    /// 上次执行时间
    pub last_run: Option<DateTime<Utc>>,
    /// 执行次数
    pub run_count: i32,
    /// 最大执行次数（0 表示无限制）
    pub max_runs: i32,
    /// 执行失败次数
    pub failure_count: i32,
    /// 最大失败次数（超过后自动禁用）
    pub max_failures: i32,
}

/// 调度类型
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum ScheduleType {
    /// Cron 表达式
    Cron,
    /// 固定间隔（秒）
    Interval,
}

/// 工作流调度器
///
/// 负责管理工作流的周期性执行
#[derive(Debug)]
pub struct WorkflowScheduler {
    /// 工作流引擎
    workflow_engine: Arc<WorkflowEngine>,
    /// 工作流存储
    workflow_store: Arc<WorkflowStore>,
    /// 调度的工作流列表
    scheduled_workflows: Arc<RwLock<HashMap<String, ScheduledWorkflow>>>,
    /// 是否正在运行
    is_running: Arc<RwLock<bool>>,
}

impl WorkflowScheduler {
    /// 创建新的工作流调度器
    ///
    /// # Arguments
    /// * `workflow_engine` - 工作流引擎
    /// * `workflow_store` - 工作流存储
    ///
    /// # Returns
    /// 工作流调度器实例
    pub fn new(
        workflow_engine: Arc<WorkflowEngine>,
        workflow_store: Arc<WorkflowStore>,
    ) -> Self {
        Self {
            workflow_engine,
            workflow_store,
            scheduled_workflows: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// 添加周期性工作流
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    /// * `schedule` - 调度表达式（cron 表达式或间隔时间）
    /// * `schedule_type` - 调度类型
    ///
    /// # Returns
    /// 操作结果
    pub async fn add_scheduled_workflow(
        &self,
        workflow_id: &str,
        schedule: &str,
        schedule_type: ScheduleType,
    ) -> Result<()> {
        debug!(
            "Adding scheduled workflow: {} with schedule: {}",
            workflow_id, schedule
        );

        // 验证工作流是否存在
        let _workflow = self
            .workflow_store
            .get_workflow(workflow_id)
            .context("Failed to get workflow")?
            .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

        // 计算下次执行时间
        let next_run = Self::calculate_next_run(schedule, &schedule_type)?;

        let scheduled_workflow = ScheduledWorkflow {
            workflow_id: workflow_id.to_string(),
            schedule: schedule.to_string(),
            schedule_type,
            enabled: true,
            next_run,
            last_run: None,
            run_count: 0,
            max_runs: 0,
            failure_count: 0,
            max_failures: 3,
        };

        // 添加到调度列表
        let mut scheduled = self.scheduled_workflows.write().await;
        scheduled.insert(workflow_id.to_string(), scheduled_workflow);

        info!(
            "Scheduled workflow added: {} with schedule: {}, next run: {}",
            workflow_id, schedule, next_run
        );

        Ok(())
    }

    /// 移除周期性工作流
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    ///
    /// # Returns
    /// 操作结果
    pub async fn remove_scheduled_workflow(&self, workflow_id: &str) -> Result<()> {
        debug!("Removing scheduled workflow: {}", workflow_id);

        let mut scheduled = self.scheduled_workflows.write().await;
        scheduled.remove(workflow_id);

        info!("Scheduled workflow removed: {}", workflow_id);

        Ok(())
    }

    /// 更新周期性工作流配置
    ///
    /// # Arguments
    /// * `workflow_id` - 工作流ID
    /// * `enabled` - 是否启用
    /// * `max_runs` - 最大执行次数
    /// * `max_failures` - 最大失败次数
    ///
    /// # Returns
    /// 操作结果
    pub async fn update_scheduled_workflow(
        &self,
        workflow_id: &str,
        enabled: Option<bool>,
        max_runs: Option<i32>,
        max_failures: Option<i32>,
    ) -> Result<()> {
        debug!("Updating scheduled workflow: {}", workflow_id);

        let mut scheduled = self.scheduled_workflows.write().await;

        if let Some(workflow) = scheduled.get_mut(workflow_id) {
            if let Some(enabled) = enabled {
                workflow.enabled = enabled;
            }
            if let Some(max_runs) = max_runs {
                workflow.max_runs = max_runs;
            }
            if let Some(max_failures) = max_failures {
                workflow.max_failures = max_failures;
            }

            info!("Scheduled workflow updated: {}", workflow_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Scheduled workflow not found: {}",
                workflow_id
            ))
        }
    }

    /// 获取周期性工作流列表
    ///
    /// # Returns
    /// 周期性工作流列表
    pub async fn list_scheduled_workflows(&self) -> Vec<ScheduledWorkflow> {
        let scheduled = self.scheduled_workflows.read().await;
        scheduled.values().cloned().collect()
    }

    /// 启动调度器
    ///
    /// # Returns
    /// 操作结果
    pub async fn start(&self) -> Result<()> {
        debug!("Starting workflow scheduler");

        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Err(anyhow::anyhow!("Scheduler is already running"));
        }
        *is_running = true;
        drop(is_running);

        info!("Workflow scheduler started");

        // 启动调度循环
        let scheduled_workflows = self.scheduled_workflows.clone();
        let workflow_engine = self.workflow_engine.clone();
        let is_running_flag = self.is_running.clone();

        tokio::spawn(async move {
            let mut ticker = interval(TokioDuration::from_secs(5));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;

                // 检查是否应该继续运行
                {
                    let running = is_running_flag.read().await;
                    if !*running {
                        debug!("Scheduler stopped");
                        break;
                    }
                }

                // 检查并执行到期的工作流
                let mut scheduled = scheduled_workflows.write().await;
                let now = Utc::now();

                for (workflow_id, scheduled_workflow) in scheduled.iter_mut() {
                    if !scheduled_workflow.enabled {
                        continue;
                    }

                    // 检查是否达到最大执行次数
                    if scheduled_workflow.max_runs > 0
                        && scheduled_workflow.run_count >= scheduled_workflow.max_runs
                    {
                        info!(
                            "Workflow {} reached max runs ({}), disabling",
                            workflow_id, scheduled_workflow.max_runs
                        );
                        scheduled_workflow.enabled = false;
                        continue;
                    }

                    // 检查是否达到最大失败次数
                    if scheduled_workflow.failure_count >= scheduled_workflow.max_failures {
                        warn!(
                            "Workflow {} reached max failures ({}), disabling",
                            workflow_id, scheduled_workflow.max_failures
                        );
                        scheduled_workflow.enabled = false;
                        continue;
                    }

                    // 检查是否到达执行时间
                    if now >= scheduled_workflow.next_run {
                        info!(
                            "Executing scheduled workflow: {} (run #{})",
                            workflow_id, scheduled_workflow.run_count + 1
                        );

                        // 启动工作流
                        match workflow_engine.start_workflow(workflow_id).await {
                            Ok(_) => {
                                scheduled_workflow.last_run = Some(now);
                                scheduled_workflow.run_count += 1;
                                scheduled_workflow.failure_count = 0;

                                // 计算下次执行时间
                                match Self::calculate_next_run(
                                    &scheduled_workflow.schedule,
                                    &scheduled_workflow.schedule_type,
                                ) {
                                    Ok(next_run) => {
                                        scheduled_workflow.next_run = next_run;
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to calculate next run for workflow {}: {}",
                                            workflow_id, e
                                        );
                                        scheduled_workflow.enabled = false;
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Failed to execute scheduled workflow {}: {}",
                                    workflow_id, e
                                );
                                scheduled_workflow.failure_count += 1;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// 停止调度器
    ///
    /// # Returns
    /// 操作结果
    pub async fn stop(&self) -> Result<()> {
        debug!("Stopping workflow scheduler");

        let mut is_running = self.is_running.write().await;
        *is_running = false;

        info!("Workflow scheduler stopped");

        Ok(())
    }

    /// 计算下次执行时间
    ///
    /// # Arguments
    /// * `schedule` - 调度表达式
    /// * `schedule_type` - 调度类型
    ///
    /// # Returns
    /// 下次执行时间
    fn calculate_next_run(
        schedule: &str,
        schedule_type: &ScheduleType,
    ) -> Result<DateTime<Utc>> {
        let now = Utc::now();

        match schedule_type {
            ScheduleType::Interval => {
                // 解析间隔时间（秒）
                let seconds: i64 = schedule
                    .parse()
                    .context("Invalid interval schedule, must be number of seconds")?;
                let duration = Duration::seconds(seconds);
                Ok(now + duration)
            }
            ScheduleType::Cron => {
                // 解析 cron 表达式
                let cron_expr = cron::Schedule::try_from(schedule)
                    .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", schedule, e))?;
                
                // 获取下一个执行时间
                if let Some(next_time) = cron_expr.after(&now).next() {
                    Ok(next_time.into())
                } else {
                    // 如果无法计算出下一个执行时间，返回 1 小时后
                    Ok(now + Duration::hours(1))
                }
            }
        }
    }
}
