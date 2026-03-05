//! 告警管理模块
//! 
//! 提供完整的告警系统，包括告警定义、规则引擎、告警管理器等功能
//! 支持多种告警类型和严重级别，可根据指标条件自动触发告警

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 告警结构体
/// 表示一个具体的告警实例，包含告警的所有信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// 告警唯一标识符
    pub id: String,
    /// 告警类型
    pub alert_type: AlertType,
    /// 告警严重级别
    pub severity: AlertSeverity,
    /// 告警消息内容
    pub message: String,
    /// 告警来源
    pub source: String,
    /// 告警时间戳（毫秒）
    pub timestamp: i64,
    /// 告警元数据
    pub metadata: serde_json::Value,
    /// 是否已被忽略/关闭
    pub dismissed: bool,
}

/// 告警类型枚举
/// 定义不同类型的告警，用于分类和过滤
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertType {
    /// 错误类告警
    Error,
    /// 警告类告警
    Warning,
    /// 延迟类告警
    Latency,
    /// Token 限制类告警
    TokenLimit,
    /// 成本阈值类告警
    CostThreshold,
    /// 工作流卡住类告警
    WorkflowStuck,
    /// 智能体失败类告警
    AgentFailure,
    /// 系统错误类告警
    SystemError,
}

/// 告警严重级别枚举
/// 用于区分告警的重要程度，便于优先处理
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertSeverity {
    /// 信息级
    Info,
    /// 警告级
    Warning,
    /// 严重级
    Critical,
}

/// 告警规则结构体
/// 定义告警的触发条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// 规则唯一标识符
    pub id: String,
    /// 规则名称
    pub name: String,
    /// 告警类型
    pub alert_type: AlertType,
    /// 触发条件
    pub condition: AlertCondition,
    /// 阈值
    pub threshold: f64,
    /// 是否启用
    pub enabled: bool,
}

/// 告警条件结构体
/// 定义具体的指标和比较条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCondition {
    /// 指标名称
    pub metric: String,
    /// 操作符 (>, <, >=, <=, ==)
    pub operator: String,
    /// 阈值
    pub value: f64,
}

/// 告警管理器
/// 负责告警的存储、查询、触发和规则评估
pub struct AlertManager {
    /// 告警列表（使用 RwLock 支持并发读写）
    alerts: Arc<RwLock<Vec<Alert>>>,
    /// 告警规则列表
    rules: Arc<RwLock<Vec<AlertRule>>>,
}

impl AlertManager {
    /// 创建新的告警管理器实例
    pub fn new() -> Self {
        Self {
            alerts: Arc::new(RwLock::new(Vec::new())),
            rules: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 添加新的告警
    /// # 参数
    /// - `alert`: 要添加的告警实例
    pub async fn add_alert(&self, alert: Alert) {
        let mut alerts = self.alerts.write().await;
        alerts.push(alert);
    }

    /// 获取告警列表
    /// # 参数
    /// - `limit`: 返回结果数量限制，默认为 100
    /// # 返回
    /// 返回未关闭的告警列表，按时间倒序
    pub async fn get_alerts(&self, limit: Option<usize>) -> Vec<Alert> {
        let alerts = self.alerts.read().await;
        let limit = limit.unwrap_or(100);
        alerts.iter()
            .filter(|a| !a.dismissed)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// 忽略/关闭告警
    /// # 参数
    /// - `id`: 告警 ID
    /// # 返回
    /// 是否成功关闭告警
    pub async fn dismiss_alert(&self, id: &str) -> bool {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == id) {
            alert.dismissed = true;
            return true;
        }
        false
    }

    /// 添加告警规则
    /// # 参数
    /// - `rule`: 要添加的规则
    pub async fn add_rule(&self, rule: AlertRule) {
        let mut rules = self.rules.write().await;
        rules.push(rule);
    }

    /// 获取告警规则列表
    /// # 返回
    /// 返回所有告警规则
    pub async fn get_rules(&self) -> Vec<AlertRule> {
        let rules = self.rules.read().await;
        rules.clone()
    }

    /// 评估指标并触发告警
    /// 根据规则评估指标值，如果满足条件则创建告警
    /// # 参数
    /// - `metric_name`: 指标名称
    /// - `value`: 指标值
    pub async fn evaluate_and_alert(&self, metric_name: &str, value: f64) {
        let rules = self.rules.read().await;
        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }
            if rule.condition.metric != metric_name {
                continue;
            }
            
            let should_alert = match rule.condition.operator.as_str() {
                ">" => value > rule.threshold,
                "<" => value < rule.threshold,
                ">=" => value >= rule.threshold,
                "<=" => value <= rule.threshold,
                "==" => (value - rule.threshold).abs() < f64::EPSILON,
                _ => false,
            };
            
            if should_alert {
                let alert = Alert {
                    id: uuid::Uuid::new_v4().to_string(),
                    alert_type: rule.alert_type.clone(),
                    severity: AlertSeverity::Warning,
                    message: format!("Alert triggered: {} {} {}", metric_name, rule.condition.operator, rule.threshold),
                    source: rule.name.clone(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    metadata: serde_json::json!({
                        "metric": metric_name,
                        "value": value,
                        "ruleId": rule.id
                    }),
                    dismissed: false,
                };
                drop(rules);
                self.add_alert(alert).await;
                return;
            }
        }
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}
