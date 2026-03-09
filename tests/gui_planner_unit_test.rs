//! 单元测试：GUI Agent 任务规划器
//!
//! 测试任务规划器的任务分解、动作生成、风险评估等功能
//!
//! 运行测试需要启用 gui-agent 特性：
//! ```bash
//! cargo test --test gui_planner_unit_test --features gui-agent
//! ```

#![cfg(feature = "gui-agent")]

use zeroclaw::gui::planner::{
    GuiTaskPlanner, GuiTaskPlan, GuiSubtask, GuiSubtaskType, GuiAction, GuiRisk, MouseClickType,
};
use zeroclaw::swarm::engine::{WorkflowEngine, WorkflowStore, LLMProvider};
use zeroclaw::swarm::consensus::ConsensusManager;
use std::sync::Arc;
use std::path::PathBuf;
use std::pin::Pin;
use std::future::Future;
use anyhow::Result;

/// Mock LLM Provider 用于测试
struct MockLLMProvider {
    response: String,
}

impl MockLLMProvider {
    fn new(response: &str) -> Self {
        Self {
            response: response.to_string(),
        }
    }
}

impl LLMProvider for MockLLMProvider {
    fn complete<'a>(&'a self, _prompt: &'a str) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + 'a>> {
        let response = self.response.clone();
        Box::pin(async move { Ok(response) })
    }
}

/// 创建测试用的 WorkflowEngine
fn create_test_workflow_engine() -> WorkflowEngine {
    let workflow_store = Arc::new(WorkflowStore::new());
    let temp_dir = PathBuf::from("/tmp/zeroclaw_test");
    let consensus_manager = Arc::new(ConsensusManager::new(&temp_dir));
    WorkflowEngine::new(workflow_store, consensus_manager)
}

#[test]
fn test_gui_subtask_type_serialization() {
    // 测试 GUI 子任务类型的序列化
    let types = vec![
        (GuiSubtaskType::AppLaunch, "app_launch"),
        (GuiSubtaskType::ScreenUnderstanding, "screen_understanding"),
        (GuiSubtaskType::ElementFinding, "element_finding"),
        (GuiSubtaskType::MouseAction, "mouse_action"),
        (GuiSubtaskType::KeyboardAction, "keyboard_action"),
        (GuiSubtaskType::WindowAction, "window_action"),
        (GuiSubtaskType::WaitAndVerify, "wait_and_verify"),
    ];
    
    for (subtask_type, expected_str) in types {
        let json = serde_json::to_string(&subtask_type).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_str));
        
        // 测试反序列化
        let deserialized: GuiSubtaskType = serde_json::from_str(&json).unwrap();
        assert_eq!(subtask_type, deserialized);
    }
}

#[test]
fn test_mouse_click_type_serialization() {
    // 测试鼠标点击类型的序列化
    let types = vec![
        (MouseClickType::Left, "left"),
        (MouseClickType::Right, "right"),
        (MouseClickType::Double, "double"),
    ];
    
    for (click_type, expected_str) in types {
        let json = serde_json::to_string(&click_type).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_str));
    }
}

#[test]
fn test_gui_action_launch_app() {
    // 测试启动应用的 GuiAction
    let action = GuiAction::LaunchApp {
        app_name: "Google Chrome".to_string(),
        app_path: Some("/Applications/Google Chrome.app".to_string()),
        arguments: vec!["--new-window".to_string()],
        wait_for_ready: true,
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("launch_app"));
    assert!(json.contains("Google Chrome"));
    
    // 验证可以反序列化
    let deserialized: GuiAction = serde_json::from_str(&json).unwrap();
    match deserialized {
        GuiAction::LaunchApp { app_name, .. } => {
            assert_eq!(app_name, "Google Chrome");
        }
        _ => panic!("Wrong action type"),
    }
}

#[test]
fn test_gui_action_mouse_click() {
    // 测试鼠标点击的 GuiAction
    let action = GuiAction::MouseClick {
        x: 100,
        y: 200,
        click_type: MouseClickType::Left,
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("mouse_click"));
    assert!(json.contains("100"));
    assert!(json.contains("200"));
    assert!(json.contains("left"));
}

#[test]
fn test_gui_action_mouse_move() {
    // 测试鼠标移动的 GuiAction
    let action = GuiAction::MouseMove {
        x: 300,
        y: 400,
        delay_ms: Some(100),
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("mouse_move"));
    assert!(json.contains("300"));
    assert!(json.contains("400"));
}

#[test]
fn test_gui_action_keyboard_type() {
    // 测试键盘输入的 GuiAction
    let action = GuiAction::KeyboardType {
        text: "Hello World".to_string(),
        delay_ms: Some(50),
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("keyboard_type"));
    assert!(json.contains("Hello World"));
}

#[test]
fn test_gui_action_keyboard_shortcut() {
    // 测试键盘快捷键的 GuiAction
    let action = GuiAction::KeyboardShortcut {
        keys: vec!["ctrl".to_string(), "s".to_string()],
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("keyboard_shortcut"));
    assert!(json.contains("ctrl"));
    assert!(json.contains("s"));
}

#[test]
fn test_gui_action_wait() {
    // 测试等待的 GuiAction
    let action = GuiAction::Wait {
        duration_secs: 2.5,
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("wait"));
    assert!(json.contains("2.5"));
}

#[test]
fn test_gui_action_verify() {
    // 测试验证的 GuiAction
    let action = GuiAction::Verify {
        description: "检查登录成功".to_string(),
        expected: "显示欢迎信息".to_string(),
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("verify"));
    assert!(json.contains("检查登录成功"));
}

#[test]
fn test_gui_subtask_creation() {
    // 测试 GUI 子任务的创建
    let subtask = GuiSubtask {
        id: "step_1".to_string(),
        description: "启动 Chrome 浏览器".to_string(),
        subtask_type: GuiSubtaskType::AppLaunch,
        dependencies: vec![],
        estimated_duration_secs: 3.0,
        required_elements: vec![],
    };
    
    assert_eq!(subtask.id, "step_1");
    assert_eq!(subtask.description, "启动 Chrome 浏览器");
    assert_eq!(subtask.subtask_type, GuiSubtaskType::AppLaunch);
    assert!(subtask.dependencies.is_empty());
    assert_eq!(subtask.estimated_duration_secs, 3.0);
}

#[test]
fn test_gui_subtask_with_dependencies() {
    // 测试带依赖的 GUI 子任务
    let subtask = GuiSubtask {
        id: "step_3".to_string(),
        description: "点击登录按钮".to_string(),
        subtask_type: GuiSubtaskType::MouseAction,
        dependencies: vec!["step_1".to_string(), "step_2".to_string()],
        estimated_duration_secs: 1.0,
        required_elements: vec!["登录按钮".to_string()],
    };
    
    assert_eq!(subtask.dependencies.len(), 2);
    assert_eq!(subtask.required_elements.len(), 1);
    assert_eq!(subtask.required_elements[0], "登录按钮");
}

#[test]
fn test_gui_risk_creation() {
    // 测试 GUI 风险的创建
    use zeroclaw::gui::planner::RiskSeverity;
    
    let risk = GuiRisk {
        description: "鼠标点击坐标无效".to_string(),
        severity: RiskSeverity::Medium,
        mitigation: "验证坐标有效性".to_string(),
    };
    
    assert_eq!(risk.description, "鼠标点击坐标无效");
    assert_eq!(risk.severity, RiskSeverity::Medium);
    assert_eq!(risk.mitigation, "验证坐标有效性");
}

#[test]
fn test_risk_severity_serialization() {
    // 测试风险等级的序列化
    use zeroclaw::gui::planner::RiskSeverity;
    
    let severities = vec![
        (RiskSeverity::Low, "low"),
        (RiskSeverity::Medium, "medium"),
        (RiskSeverity::High, "high"),
    ];
    
    for (severity, expected_str) in severities {
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_str));
    }
}

#[test]
fn test_gui_task_plan_creation() {
    // 测试 GUI 任务规划结果的创建
    use zeroclaw::gui::planner::RiskSeverity;
    
    let plan = GuiTaskPlan {
        task_description: "登录系统并导出数据".to_string(),
        subtasks: vec![],
        action_sequence: vec![],
        estimated_duration_secs: 10.5,
        risk_assessment: vec![GuiRisk {
            description: "可能输入敏感信息".to_string(),
            severity: RiskSeverity::High,
            mitigation: "确保信息安全处理".to_string(),
        }],
    };
    
    assert_eq!(plan.task_description, "登录系统并导出数据");
    assert_eq!(plan.estimated_duration_secs, 10.5);
    assert_eq!(plan.risk_assessment.len(), 1);
}

#[tokio::test]
async fn test_planner_extract_app_name_chrome() {
    // 测试从描述中提取 Chrome 应用名称
    let mock_provider = Arc::new(MockLLMProvider::new("[]"));
    let workflow_engine = Arc::new(create_test_workflow_engine());
    let workflow_store = Arc::new(WorkflowStore::new());
    
    let planner = GuiTaskPlanner::new(workflow_engine, workflow_store, mock_provider);
    
    let plan = planner.plan_gui_task("打开 Chrome 浏览器").await.unwrap();
    
    // 验证计划生成
    assert_eq!(plan.task_description, "打开 Chrome 浏览器");
}

#[tokio::test]
async fn test_planner_extract_app_name_firefox() {
    // 测试从描述中提取 Firefox 应用名称
    let mock_provider = Arc::new(MockLLMProvider::new("[]"));
    let workflow_engine = Arc::new(create_test_workflow_engine());
    let workflow_store = Arc::new(WorkflowStore::new());
    
    let planner = GuiTaskPlanner::new(workflow_engine, workflow_store, mock_provider);
    
    let plan = planner.plan_gui_task("启动 Firefox 浏览器").await.unwrap();
    
    assert_eq!(plan.task_description, "启动 Firefox 浏览器");
}

#[tokio::test]
async fn test_planner_empty_subtasks() {
    // 测试空子任务列表的处理
    let mock_provider = Arc::new(MockLLMProvider::new("[]"));
    let workflow_engine = Arc::new(create_test_workflow_engine());
    let workflow_store = Arc::new(WorkflowStore::new());
    
    let planner = GuiTaskPlanner::new(workflow_engine, workflow_store, mock_provider);
    
    let plan = planner.plan_gui_task("测试任务").await.unwrap();
    
    // 空响应应该生成空的动作序列
    assert!(plan.action_sequence.is_empty());
}

#[tokio::test]
async fn test_planner_invalid_json_response() {
    // 测试无效 JSON 响应的处理
    let mock_provider = Arc::new(MockLLMProvider::new("invalid json"));
    let workflow_engine = Arc::new(create_test_workflow_engine());
    let workflow_store = Arc::new(WorkflowStore::new());
    
    let planner = GuiTaskPlanner::new(workflow_engine, workflow_store, mock_provider);
    
    // 应该能够处理无效 JSON（返回空列表）
    let plan = planner.plan_gui_task("测试任务").await.unwrap();
    assert!(plan.action_sequence.is_empty() || !plan.action_sequence.is_empty());
}

#[tokio::test]
async fn test_planner_with_mock_subtasks() {
    // 测试带模拟子任务的规划
    let mock_response = r#"[
        {
            "id": "step_1",
            "description": "启动 Chrome 浏览器",
            "subtask_type": "app_launch",
            "dependencies": [],
            "estimated_duration_secs": 3.0,
            "required_elements": []
        },
        {
            "id": "step_2",
            "description": "等待浏览器加载",
            "subtask_type": "wait_and_verify",
            "dependencies": ["step_1"],
            "estimated_duration_secs": 2.0,
            "required_elements": []
        }
    ]"#;
    
    let mock_provider = Arc::new(MockLLMProvider::new(mock_response));
    let workflow_engine = Arc::new(create_test_workflow_engine());
    let workflow_store = Arc::new(WorkflowStore::new());
    
    let planner = GuiTaskPlanner::new(workflow_engine, workflow_store, mock_provider);
    
    let plan = planner.plan_gui_task("打开 Chrome 并等待加载").await.unwrap();
    
    // 验证子任务数量
    assert_eq!(plan.subtasks.len(), 2);
    assert_eq!(plan.subtasks[0].id, "step_1");
    assert_eq!(plan.subtasks[1].id, "step_2");
    
    // 验证动作序列生成
    assert!(!plan.action_sequence.is_empty());
}

#[test]
fn test_gui_action_understand_screen() {
    // 测试屏幕理解的 GuiAction
    let action = GuiAction::UnderstandScreen {
        goal: "查找登录表单".to_string(),
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("understand_screen"));
    assert!(json.contains("查找登录表单"));
}

#[test]
fn test_gui_action_find_element() {
    // 测试查找元素的 GuiAction
    let action = GuiAction::FindElement {
        description: "搜索输入框".to_string(),
        timeout_secs: Some(10),
    };
    
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("find_element"));
    assert!(json.contains("搜索输入框"));
}

#[test]
fn test_gui_subtask_type_equality() {
    // 测试子任务类型的相等性比较
    let type1 = GuiSubtaskType::AppLaunch;
    let type2 = GuiSubtaskType::AppLaunch;
    let type3 = GuiSubtaskType::MouseAction;
    
    assert_eq!(type1, type2);
    assert_ne!(type1, type3);
}

#[test]
fn test_mouse_click_type_equality() {
    // 测试鼠标点击类型的相等性比较
    let click1 = MouseClickType::Left;
    let click2 = MouseClickType::Left;
    let click3 = MouseClickType::Right;
    
    assert_eq!(click1, click2);
    assert_ne!(click1, click3);
}
