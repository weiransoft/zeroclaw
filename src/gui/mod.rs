/// GUI Agent 模块
/// 
/// 本模块提供 GUI Agent 的核心功能，包括屏幕捕获、窗口管理、自动化控制等。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::screen::capture::ScreenCapture;
/// use zeroclaw::gui::automation::executor::AutomationExecutor;
/// use zeroclaw::gui::perceptor::MultimodalPerceptor;
/// use zeroclaw::gui::planner::GuiTaskPlanner;
/// use zeroclaw::gui::launcher::ApplicationLauncher;
/// ```

/// 屏幕捕获模块
pub mod screen;
/// 自动化控制模块
pub mod automation;
/// GUI Agent 核心模块
pub mod agent;
/// 上下文管理模块
pub mod context;
/// 错误恢复模块
pub mod recovery;
// /// 网关模块（未完成）
// pub mod gateway;
// /// 集成模块（未完成）
// pub mod integration;
/// 多模态感知器模块
pub mod perceptor;
/// 任务规划器模块
pub mod planner;
/// 应用启动器模块
pub mod launcher;
// /// 使用示例模块
// #[cfg(test)]
// pub mod examples;
