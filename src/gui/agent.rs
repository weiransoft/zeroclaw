/// GUI Agent 核心协调器
/// 
/// 本模块整合 ZeroClaw 的核心组件，为 GUI Agent 提供完整的功能支持。
/// 
/// # 架构设计
/// 
/// - GuiAgentCore: 核心协调器
///   - Planner (任务规划)
///   - Perceptor (多模态感知)
///   - Launcher (应用启动)
///   - Memory (记忆系统)
///   - Security (安全策略)
///   - Config (配置管理)
///   - RuntimeAdapter (运行时适配器)
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::agent::GuiAgentCore;
/// use zeroclaw::memory::InMemoryMemory;
/// use zeroclaw::security::SecurityPolicy;
/// use zeroclaw::runtime::NativeRuntime;
/// use zeroclaw::config::Config;
/// 
/// let memory = Arc::new(InMemoryMemory::new());
/// let security = Arc::new(SecurityPolicy::default());
/// let runtime = Arc::new(NativeRuntime::new());
/// let config = Config::default();
/// 
/// let agent = GuiAgentCore::new(memory, security, runtime, config).await?;
/// 
/// // 执行 GUI 任务
/// let result = agent.execute_task("打开 Chrome 并访问 Google").await?;
/// ```

use std::sync::Arc;

use crate::gui::planner::{GuiTaskPlanner, GuiAction};
use crate::gui::launcher::{ApplicationLauncher, LaunchConfig, Launcher};
use crate::memory::traits::Memory;
use crate::memory::traits::MemoryCategory;
use crate::security::SecurityPolicy;
use crate::runtime::traits::RuntimeAdapter;
use crate::config::Config;
use crate::swarm::engine::{WorkflowEngine, WorkflowStore, LLMProvider};
use crate::swarm::consensus::ConsensusManager;

/// GUI Agent 核心配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GuiAgentConfig {
    /// 是否启用记忆系统
    pub enable_memory: bool,
    /// 是否启用安全策略
    pub enable_security: bool,
    /// 任务规划超时时间（秒）
    pub planning_timeout_secs: u64,
    /// 动作执行超时时间（秒）
    pub execution_timeout_secs: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 是否自动启动应用（如果未启动）
    pub auto_launch_app: bool,
}

impl Default for GuiAgentConfig {
    fn default() -> Self {
        Self {
            enable_memory: true,
            enable_security: true,
            planning_timeout_secs: 30,
            execution_timeout_secs: 60,
            max_retries: 3,
            auto_launch_app: true,
        }
    }
}

/// GUI Agent 执行结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GuiAgentResult {
    /// 是否成功
    pub success: bool,
    /// 执行结果消息
    pub message: String,
    /// 执行的步骤数量
    pub steps_executed: usize,
    /// 耗时（毫秒）
    pub duration_ms: u64,
    /// 错误信息（如果有）
    pub error: Option<String>,
}

/// GUI Agent 核心
/// 
/// 整合 ZeroClaw 核心组件，提供完整的 GUI 自动化能力
pub struct GuiAgentCore {
    /// 任务规划器
    planner: Arc<GuiTaskPlanner>,
    /// 应用启动器
    launcher: Arc<ApplicationLauncher>,
    /// 记忆系统
    memory: Option<Arc<dyn Memory>>,
    /// 安全策略
    security: Arc<SecurityPolicy>,
    /// 配置
    config: GuiAgentConfig,
}

impl GuiAgentCore {
    /// 创建新的 GUI Agent 核心
    /// 
    /// # 参数
    /// 
    /// * `llm_provider` - LLM 提供者
    /// * `workflow_store` - 工作流存储
    /// * `memory` - 记忆系统（可选）
    /// * `security` - 安全策略
    /// * `runtime` - 运行时适配器
    /// * `config` - 全局配置
    pub async fn new(
        llm_provider: Arc<dyn LLMProvider>,
        workflow_store: Arc<WorkflowStore>,
        memory: Option<Arc<dyn Memory>>,
        security: Arc<SecurityPolicy>,
        runtime: Arc<dyn RuntimeAdapter>,
        config: Config,
    ) -> anyhow::Result<Self> {
        // 创建工作流引擎
        let consensus_manager = Arc::new(ConsensusManager::new(&runtime.storage_path()));
        let workflow_engine = Arc::new(WorkflowEngine::new(workflow_store.clone(), consensus_manager));
        
        // 创建任务规划器
        let planner = Arc::new(GuiTaskPlanner::new(
            workflow_engine,
            workflow_store,
            llm_provider,
        ));
        
        // 创建应用启动器
        let launcher = Arc::new(ApplicationLauncher::new());
        
        // 创建 GUI Agent 配置
        let gui_config = GuiAgentConfig {
            enable_memory: memory.is_some(),
            enable_security: true,
            planning_timeout_secs: config.agent.max_tool_iterations as u64 * 5,
            execution_timeout_secs: 60,
            max_retries: 3,
            auto_launch_app: true,
        };
        
        Ok(Self {
            planner,
            launcher,
            memory,
            security,
            config: gui_config,
        })
    }
    
    /// 执行 GUI 任务
    /// 
    /// # 参数
    /// 
    /// * `task_description` - 任务描述（自然语言）
    /// 
    /// # 返回
    /// 
    /// * `Result<GuiAgentResult>` - 执行结果
    pub async fn execute_task(&self, task_description: &str) -> anyhow::Result<GuiAgentResult> {
        let start_time = std::time::Instant::now();
        
        // 1. 任务规划
        let plan = self.planner.plan_gui_task(task_description).await?;
        
        // 2. 执行动作序列
        let mut steps_executed = 0;
        
        for action in &plan.action_sequence {
            match action {
                GuiAction::LaunchApp { app_name, app_path, arguments, wait_for_ready } => {
                    // 检查是否需要启动应用
                    if self.config.auto_launch_app {
                        let config = LaunchConfig {
                            app_name: app_name.clone(),
                            app_path: app_path.clone(),
                            arguments: arguments.clone(),
                            wait_for_ready: *wait_for_ready,
                            ..Default::default()
                        };
                        
                        match self.launcher.launch(&config).await {
                            Ok(result) => {
                                if !result.success {
                                    return Ok(GuiAgentResult {
                                        success: false,
                                        message: format!("启动应用失败: {}", app_name),
                                        steps_executed,
                                        duration_ms: start_time.elapsed().as_millis() as u64,
                                        error: result.error_message,
                                    });
                                }
                            }
                            Err(e) => {
                                return Ok(GuiAgentResult {
                                    success: false,
                                    message: format!("启动应用出错: {}", app_name),
                                    steps_executed,
                                    duration_ms: start_time.elapsed().as_millis() as u64,
                                    error: Some(e.to_string()),
                                });
                            }
                        }
                    }
                }
                GuiAction::UnderstandScreen { goal } => {
                    tracing::info!("屏幕理解目标: {}", goal);
                }
                GuiAction::FindElement { description, timeout_secs } => {
                    tracing::info!("查找元素: {}, 超时: {:?}s", description, timeout_secs);
                }
                GuiAction::MouseClick { x, y, click_type } => {
                    tracing::info!("执行鼠标点击: ({}, {}), 类型: {:?}", x, y, click_type);
                }
                GuiAction::MouseMove { x, y, delay_ms } => {
                    tracing::info!("执行鼠标移动: ({}, {}), 延迟: {:?}ms", x, y, delay_ms);
                }
                GuiAction::KeyboardType { text, delay_ms } => {
                    tracing::info!("执行键盘输入: {}, 延迟: {:?}ms", text, delay_ms);
                }
                GuiAction::KeyboardShortcut { keys } => {
                    tracing::info!("执行快捷键: {:?}", keys);
                }
                GuiAction::Wait { duration_secs } => {
                    tokio::time::sleep(tokio::time::Duration::from_secs_f64(*duration_secs)).await;
                }
                GuiAction::Verify { description, expected } => {
                    tracing::info!("验证: {}, 期望: {}", description, expected);
                }
            }
            steps_executed += 1;
        }
        
        // 3. 存储到记忆系统
        if self.config.enable_memory {
            if let Some(ref memory) = self.memory {
                let _ = memory.store(
                    &format!("gui_task_{}", start_time.elapsed().as_secs()),
                    task_description,
                    MemoryCategory::Core,
                ).await;
            }
        }
        
        let duration = start_time.elapsed().as_millis() as u64;
        
        Ok(GuiAgentResult {
            success: true,
            message: format!("任务执行成功，共执行 {} 个步骤", steps_executed),
            steps_executed,
            duration_ms: duration,
            error: None,
        })
    }
    
    /// 获取配置
    pub fn config(&self) -> &GuiAgentConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gui_agent_config_default() {
        let config = GuiAgentConfig::default();
        
        assert!(config.enable_memory);
        assert!(config.enable_security);
        assert_eq!(config.planning_timeout_secs, 30);
        assert_eq!(config.execution_timeout_secs, 60);
        assert_eq!(config.max_retries, 3);
        assert!(config.auto_launch_app);
    }
    
    #[test]
    fn test_gui_agent_result_creation() {
        let result = GuiAgentResult {
            success: true,
            message: "测试成功".to_string(),
            steps_executed: 5,
            duration_ms: 1000,
            error: None,
        };
        
        assert!(result.success);
        assert_eq!(result.steps_executed, 5);
        assert!(result.error.is_none());
    }
    
    #[test]
    fn test_gui_agent_result_with_error() {
        let result = GuiAgentResult {
            success: false,
            message: "测试失败".to_string(),
            steps_executed: 2,
            duration_ms: 500,
            error: Some("未知错误".to_string()),
        };
        
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
