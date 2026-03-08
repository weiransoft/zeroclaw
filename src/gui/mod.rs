/// GUI Agent 模块
/// 
/// 本模块提供 GUI Agent 的核心功能,包括屏幕捕获、窗口管理、自动化控制等。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::screen::capture::ScreenCapture;
/// use zeroclaw::gui::automation::executor::AutomationExecutor;
/// ```

/// 屏幕捕获模块
pub mod screen;
/// 自动化控制模块
pub mod automation;
/// 网关模块
pub mod gateway;
/// 集成模块
pub mod integration;
