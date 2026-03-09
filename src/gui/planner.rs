/// GUI Agent 任务规划器
/// 
/// 本模块复用 ZeroClaw 现有的任务规划引擎，针对 GUI 操作进行优化。
/// 
/// # 架构设计
/// 
/// ```text
/// ┌─────────────────────────────────────────┐
/// │         GuiTaskPlanner                  │
/// ├─────────────────────────────────────────┤
/// │  ┌─────────────────────────────────┐   │
/// │  │   PlanningEngine (复用)         │   │
/// │  │   - 任务分解                     │   │
/// │  │   - 工作流生成                   │   │
/// │  │   - 资源分配                     │   │
/// │  └─────────────────────────────────┘   │
/// │  ┌─────────────────────────────────┐   │
/// │  │   GUI Task Decomposer           │   │
/// │  │   - GUI 操作分解                  │   │
/// │  │   - 屏幕理解集成                 │   │
/// │  │   - 动作序列生成                 │   │
/// │  └─────────────────────────────────┘   │
/// │  ┌─────────────────────────────────┐   │
/// │  │   Action Sequence Generator     │   │
/// │  │   - 鼠标操作序列                 │   │
/// │  │   - 键盘操作序列                 │   │
/// │  │   - 窗口操作序列                 │   │
/// │  └─────────────────────────────────┘   │
/// └─────────────────────────────────────────┘
/// ```
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::planner::GuiTaskPlanner;
/// 
/// let planner = GuiTaskPlanner::new(llm_provider, workflow_store);
/// 
/// // 规划 GUI 任务
/// let plan = planner.plan_gui_task("登录到系统并导出数据").await?;
/// 
/// // 执行计划
/// for action in plan.actions {
///     executor.execute(action).await?;
/// }
/// ```

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::swarm::engine::{WorkflowEngine, WorkflowStore, LLMProvider};
// use crate::gui::perceptor::{UiElement, ScreenUnderstanding};

/// GUI 任务规划结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiTaskPlan {
    /// 任务描述
    pub task_description: String,
    /// 分解的子任务列表
    pub subtasks: Vec<GuiSubtask>,
    /// 动作序列
    pub action_sequence: Vec<GuiAction>,
    /// 预计执行时间（秒）
    pub estimated_duration_secs: f64,
    /// 风险评估
    pub risk_assessment: Vec<GuiRisk>,
}

/// GUI 子任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiSubtask {
    /// 子任务 ID
    pub id: String,
    /// 子任务描述
    pub description: String,
    /// 子任务类型
    pub subtask_type: GuiSubtaskType,
    /// 依赖的子任务 ID 列表
    pub dependencies: Vec<String>,
    /// 预计执行时间（秒）
    pub estimated_duration_secs: f64,
    /// 所需 UI 元素
    pub required_elements: Vec<String>,
}

/// GUI 子任务类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuiSubtaskType {
    /// 启动应用
    AppLaunch,
    /// 屏幕理解
    ScreenUnderstanding,
    /// 元素查找
    ElementFinding,
    /// 鼠标操作
    MouseAction,
    /// 键盘操作
    KeyboardAction,
    /// 窗口操作
    WindowAction,
    /// 等待/验证
    WaitAndVerify,
}

/// GUI 动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum GuiAction {
    /// 启动应用
    LaunchApp {
        /// 应用名称
        app_name: String,
        /// 应用路径（可选）
        app_path: Option<String>,
        /// 启动参数
        arguments: Vec<String>,
        /// 等待应用启动完成
        wait_for_ready: bool,
    },
    /// 理解屏幕
    UnderstandScreen {
        /// 屏幕理解的目标
        goal: String,
    },
    /// 查找 UI 元素
    FindElement {
        /// 元素描述
        description: String,
        /// 超时时间（秒）
        timeout_secs: Option<u64>,
    },
    /// 鼠标点击
    MouseClick {
        /// X 坐标
        x: i32,
        /// Y 坐标
        y: i32,
        /// 点击类型
        click_type: MouseClickType,
    },
    /// 鼠标移动
    MouseMove {
        /// X 坐标
        x: i32,
        /// Y 坐标
        y: i32,
        /// 移动延迟（毫秒）
        delay_ms: Option<u64>,
    },
    /// 键盘输入
    KeyboardType {
        /// 输入文本
        text: String,
        /// 延迟（毫秒）
        delay_ms: Option<u64>,
    },
    /// 键盘快捷键
    KeyboardShortcut {
        /// 快捷键列表（如 ["ctrl", "s"]）
        keys: Vec<String>,
    },
    /// 等待
    Wait {
        /// 等待时间（秒）
        duration_secs: f64,
    },
    /// 验证
    Verify {
        /// 验证描述
        description: String,
        /// 期望结果
        expected: String,
    },
}

/// 鼠标点击类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseClickType {
    /// 左键点击
    Left,
    /// 右键点击
    Right,
    /// 双击
    Double,
}

/// GUI 风险
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiRisk {
    /// 风险描述
    pub description: String,
    /// 风险等级
    pub severity: RiskSeverity,
    /// 缓解措施
    pub mitigation: String,
}

/// 风险等级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
}

/// GUI 任务规划器
/// 
/// 复用 ZeroClaw 的 PlanningEngine，针对 GUI 操作进行优化。
pub struct GuiTaskPlanner {
    /// 工作流引擎（复用现有组件）
    workflow_engine: Arc<WorkflowEngine>,
    /// 工作流存储
    workflow_store: Arc<WorkflowStore>,
    /// LLM Provider（复用现有组件）
    llm_provider: Arc<dyn LLMProvider>,
}

impl GuiTaskPlanner {
    /// 创建新的 GUI 任务规划器
    /// 
    /// # 参数
    /// 
    /// * `workflow_engine` - 工作流引擎（复用现有组件）
    /// * `workflow_store` - 工作流存储
    /// * `llm_provider` - LLM Provider
    pub fn new(
        workflow_engine: Arc<WorkflowEngine>,
        workflow_store: Arc<WorkflowStore>,
        llm_provider: Arc<dyn LLMProvider>,
    ) -> Self {
        Self {
            workflow_engine,
            workflow_store,
            llm_provider,
        }
    }

    /// 规划 GUI 任务
    /// 
    /// # 参数
    /// 
    /// * `task_description` - 任务描述（自然语言）
    /// 
    /// # 返回
    /// 
    /// * `Result<GuiTaskPlan>` - 任务规划结果
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let plan = planner.plan_gui_task("登录到系统并导出数据").await?;
    /// ```
    pub async fn plan_gui_task(&self, task_description: &str) -> Result<GuiTaskPlan, anyhow::Error> {
        // 1. 使用 LLM 进行任务分解
        let decomposition = self.decompose_task(task_description).await?;
        
        // 2. 生成动作序列
        let action_sequence = self.generate_action_sequence(&decomposition).await?;
        
        // 3. 评估风险
        let risk_assessment = self.assess_risks(task_description, &action_sequence).await?;
        
        // 4. 计算预计时间
        let estimated_duration = self.estimate_duration(&action_sequence);
        
        Ok(GuiTaskPlan {
            task_description: task_description.to_string(),
            subtasks: decomposition,
            action_sequence,
            estimated_duration_secs: estimated_duration,
            risk_assessment,
        })
    }

    /// 使用 LLM 进行任务分解
    async fn decompose_task(&self, task_description: &str) -> Result<Vec<GuiSubtask>, anyhow::Error> {
        // 构建任务分解的 prompt
        let prompt = format!(
            r#"你是一个专业的 GUI Agent 任务规划专家。请将以下 GUI 任务分解为可执行的子任务。

任务：{}

请将任务分解为以下类型的子任务：
1. app_launch: 启动应用（如果应用未运行）
2. screen_understanding: 屏幕理解
3. element_finding: 查找 UI 元素
4. mouse_action: 鼠标操作（点击、移动等）
5. keyboard_action: 键盘输入
6. window_action: 窗口操作
7. wait_and_verify: 等待和验证

对于每个子任务，请提供：
- id: 唯一标识符（如 "step_1", "step_2"）
- description: 子任务描述
- subtask_type: 子任务类型
- dependencies: 依赖的子任务 ID 列表
- estimated_duration_secs: 预计执行时间（秒）
- required_elements: 需要的 UI 元素描述列表

请以 JSON 数组格式返回结果。

# 示例

任务："打开 Chrome 浏览器并访问 Google"

[
  {{
    "id": "step_1",
    "description": "启动 Google Chrome 浏览器",
    "subtask_type": "app_launch",
    "dependencies": [],
    "estimated_duration_secs": 3.0,
    "required_elements": []
  }},
  {{
    "id": "step_2",
    "description": "等待浏览器加载完成",
    "subtask_type": "wait_and_verify",
    "dependencies": ["step_1"],
    "estimated_duration_secs": 2.0,
    "required_elements": []
  }},
  {{
    "id": "step_3",
    "description": "在地址栏输入 https://www.google.com",
    "subtask_type": "keyboard_action",
    "dependencies": ["step_2"],
    "estimated_duration_secs": 1.0,
    "required_elements": ["地址栏输入框"]
  }}
]"#,
            task_description
        );
        
        // 调用 LLM
        let response = self.llm_provider.complete(&prompt).await?;
        
        // 解析响应
        let subtasks: Vec<GuiSubtask> = serde_json::from_str(&response)
            .unwrap_or_else(|_| Vec::new());
        
        Ok(subtasks)
    }

    /// 生成动作序列
    async fn generate_action_sequence(&self, subtasks: &[GuiSubtask]) -> Result<Vec<GuiAction>, anyhow::Error> {
        let mut actions = Vec::new();
        
        for subtask in subtasks {
            match subtask.subtask_type {
                GuiSubtaskType::AppLaunch => {
                    // 从描述中提取应用名称
                    let app_name = self.extract_app_name(&subtask.description);
                    actions.push(GuiAction::LaunchApp {
                        app_name,
                        app_path: None,
                        arguments: Vec::new(),
                        wait_for_ready: true,
                    });
                }
                GuiSubtaskType::ScreenUnderstanding => {
                    actions.push(GuiAction::UnderstandScreen {
                        goal: subtask.description.clone(),
                    });
                }
                GuiSubtaskType::ElementFinding => {
                    actions.push(GuiAction::FindElement {
                        description: subtask.description.clone(),
                        timeout_secs: Some(10),
                    });
                }
                GuiSubtaskType::MouseAction => {
                    // 从描述中提取坐标（简化实现，实际需要 LLM 解析）
                    actions.push(GuiAction::MouseClick {
                        x: 0, // 需要从描述中解析
                        y: 0,
                        click_type: MouseClickType::Left,
                    });
                }
                GuiSubtaskType::KeyboardAction => {
                    actions.push(GuiAction::KeyboardType {
                        text: subtask.description.clone(),
                        delay_ms: Some(50),
                    });
                }
                GuiSubtaskType::WindowAction => {
                    // 窗口操作暂不处理
                }
                GuiSubtaskType::WaitAndVerify => {
                    actions.push(GuiAction::Wait {
                        duration_secs: 1.0,
                    });
                }
            }
        }
        
        Ok(actions)
    }
    
    /// 从描述中提取应用名称（简化实现）
    fn extract_app_name(&self, description: &str) -> String {
        // 简单的关键词匹配，实际应该用 LLM 提取
        let app_keywords = [
            ("chrome", "Google Chrome"),
            ("firefox", "Firefox"),
            ("safari", "Safari"),
            ("edge", "Microsoft Edge"),
            ("vscode", "Visual Studio Code"),
            ("terminal", "Terminal"),
        ];
        
        let desc_lower = description.to_lowercase();
        for (keyword, app_name) in &app_keywords {
            if desc_lower.contains(keyword) {
                return app_name.to_string();
            }
        }
        
        // 默认返回描述本身
        description.to_string()
    }

    /// 评估风险
    async fn assess_risks(&self, _task_description: &str, actions: &[GuiAction]) -> Result<Vec<GuiRisk>, anyhow::Error> {
        // 简单的风险评估逻辑
        let mut risks = Vec::new();
        
        // 检查是否有危险操作
        for action in actions {
            match action {
                GuiAction::MouseClick { x, y, .. } => {
                    if *x < 0 || *y < 0 {
                        risks.push(GuiRisk {
                            description: "鼠标点击坐标无效".to_string(),
                            severity: RiskSeverity::Medium,
                            mitigation: "在执行前验证坐标有效性".to_string(),
                        });
                    }
                }
                GuiAction::KeyboardType { text, .. } => {
                    if text.contains("password") || text.contains("secret") {
                        risks.push(GuiRisk {
                            description: "可能输入敏感信息".to_string(),
                            severity: RiskSeverity::High,
                            mitigation: "确保敏感信息安全处理".to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
        
        Ok(risks)
    }

    /// 计算预计执行时间
    fn estimate_duration(&self, actions: &[GuiAction]) -> f64 {
        let mut total = 0.0;
        
        for action in actions {
            match action {
                GuiAction::UnderstandScreen { .. } => {
                    total += 2.0; // 屏幕理解约 2 秒
                }
                GuiAction::FindElement { .. } => {
                    total += 1.0; // 元素查找约 1 秒
                }
                GuiAction::MouseClick { .. } => {
                    total += 0.5; // 鼠标点击约 0.5 秒
                }
                GuiAction::MouseMove { delay_ms: Some(delay), .. } => {
                    total += *delay as f64 / 1000.0;
                }
                GuiAction::KeyboardType { text, delay_ms: Some(delay), .. } => {
                    total += text.len() as f64 * (*delay as f64 / 1000.0);
                }
                GuiAction::Wait { duration_secs } => {
                    total += duration_secs;
                }
                _ => {
                    total += 0.5; // 其他操作默认 0.5 秒
                }
            }
        }
        
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gui_action_serialization() {
        let action = GuiAction::MouseClick {
            x: 100,
            y: 200,
            click_type: MouseClickType::Left,
        };
        
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("mouse_click"));
        assert!(json.contains("100"));
        assert!(json.contains("200"));
    }

    #[test]
    fn test_gui_subtask_type_display() {
        assert_eq!(serde_json::to_string(&GuiSubtaskType::ScreenUnderstanding).unwrap(), "\"screen_understanding\"");
        assert_eq!(serde_json::to_string(&GuiSubtaskType::MouseAction).unwrap(), "\"mouse_action\"");
    }
}
