/// GUI Agent Tool 集成
/// 
/// 本模块提供 GUI Agent 的 Tool 实现，使 GUI 操作可以作为 Tool 暴露给 ZeroClaw。

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::gui::automation::executor::AutomationExecutor;
use crate::gui::screen::capture::ScreenCapture;
use crate::gui::screen::window::WindowManager;
use crate::tools::traits::{Tool, ToolResult};

/// GUI Agent Tool
/// 
/// 将 GUI Agent 的功能作为 Tool 暴露给 ZeroClaw，
/// 使 AI Agent 可以执行 GUI 操作。
pub struct GuiAgentTool {
    /// 自动化执行器
    executor: Arc<Mutex<AutomationExecutor>>,
    /// 屏幕捕获器
    capture: Arc<Mutex<ScreenCapture>>,
}

impl GuiAgentTool {
    /// 创建新的 GUI Agent Tool
    /// 
    /// # 参数
    /// 
    /// * `executor` - 自动化执行器
    /// * `capture` - 屏幕捕获器
    /// 
    /// # 返回
    /// 
    /// * `GuiAgentTool` - GUI Agent Tool 实例
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// use zeroclaw::gui::automation::executor::AutomationExecutor;
    /// use zeroclaw::gui::screen::capture::ScreenCapture;
    /// use zeroclaw::gui::integration::tools::GuiAgentTool;
    /// 
    /// let executor = AutomationExecutor::new();
    /// let capture = ScreenCapture::new();
    /// let tool = GuiAgentTool::new(executor, capture);
    /// ```
    pub fn new(executor: Arc<Mutex<AutomationExecutor>>, capture: Arc<Mutex<ScreenCapture>>) -> Self {
        GuiAgentTool { executor, capture }
    }
    
    /// 获取所有 GUI Agent Tools
    /// 
    /// # 返回
    /// 
    /// * `Vec<Box<dyn Tool>>` - GUI Agent Tools 列表
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let tool = GuiAgentTool::new(...);
    /// let tools = GuiAgentTool::get_all_tools();
    /// ```
    pub fn get_all_tools() -> Vec<Box<dyn Tool>> {
        let executor = Arc::new(Mutex::new(AutomationExecutor::new()));
        let capture = Arc::new(Mutex::new(ScreenCapture::new()));
        let tool = GuiAgentTool::new(executor, capture);
        
        vec![
            tool.launch_app_tool(),
            tool.click_screen_tool(),
            tool.type_text_tool(),
            tool.capture_screen_tool(),
            tool.list_windows_tool(),
            tool.find_window_tool(),
        ]
    }
    
    /// 获取启动应用 Tool
    /// 
    /// # 返回
    /// 
    /// * `Box<dyn Tool>` - 启动应用 Tool
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let tool = GuiAgentTool::new(...);
    /// let launch_app_tool = tool.launch_app_tool();
    /// ```
    pub fn launch_app_tool(&self) -> Box<dyn Tool> {
        Box::new(LaunchAppTool {
            inner: self.clone(),
        })
    }
    
    /// 获取点击屏幕 Tool
    /// 
    /// # 返回
    /// 
    /// * `Box<dyn Tool>` - 点击屏幕 Tool
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let tool = GuiAgentTool::new(...);
    /// let click_screen_tool = tool.click_screen_tool();
    /// ```
    pub fn click_screen_tool(&self) -> Box<dyn Tool> {
        Box::new(ClickScreenTool {
            inner: self.clone(),
        })
    }
    
    /// 获取输入文本 Tool
    /// 
    /// # 返回
    /// 
    /// * `Box<dyn Tool>` - 输入文本 Tool
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let tool = GuiAgentTool::new(...);
    /// let type_text_tool = tool.type_text_tool();
    /// ```
    pub fn type_text_tool(&self) -> Box<dyn Tool> {
        Box::new(TypeTextTool {
            inner: self.clone(),
        })
    }
    
    /// 获取截取屏幕 Tool
    /// 
    /// # 返回
    /// 
    /// * `Box<dyn Tool>` - 截取屏幕 Tool
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let tool = GuiAgentTool::new(...);
    /// let capture_screen_tool = tool.capture_screen_tool();
    /// ```
    pub fn capture_screen_tool(&self) -> Box<dyn Tool> {
        Box::new(CaptureScreenTool {
            inner: self.clone(),
        })
    }
    
    /// 获取列出窗口 Tool
    /// 
    /// # 返回
    /// 
    /// * `Box<dyn Tool>` - 列出窗口 Tool
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let tool = GuiAgentTool::new(...);
    /// let list_windows_tool = tool.list_windows_tool();
    /// ```
    pub fn list_windows_tool(&self) -> Box<dyn Tool> {
        Box::new(ListWindowsTool {
            inner: self.clone(),
        })
    }
    
    /// 获取查找窗口 Tool
    /// 
    /// # 返回
    /// 
    /// * `Box<dyn Tool>` - 查找窗口 Tool
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let tool = GuiAgentTool::new(...);
    /// let find_window_tool = tool.find_window_tool();
    /// ```
    pub fn find_window_tool(&self) -> Box<dyn Tool> {
        Box::new(FindWindowTool {
            inner: self.clone(),
        })
    }
}

/// 启动应用 Tool
pub struct LaunchAppTool {
    inner: GuiAgentTool,
}

#[async_trait]
impl Tool for LaunchAppTool {
    fn name(&self) -> &str {
        "launch_app"
    }
    
    fn description(&self) -> &str {
        "启动指定路径的应用程序"
    }
    
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "应用程序路径"
                }
            },
            "required": ["path"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let path = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("参数 path 缺失或不是字符串"))?;
        
        // 实现启动应用逻辑
        let _executor = self.inner.executor.lock().await;
        
        #[cfg(target_os = "macos")]
        {
            // 使用 AppleScript 启动应用
            let script = format!(
                "tell application \"System Events\"\n    open application file \"{}\"\nend tell",
                path
            );
            
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| anyhow::anyhow!("执行 AppleScript 失败: {}", e))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("启动应用失败: {}", stderr));
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            // 使用 PowerShell 启动应用
            let script = format!("Start-Process \"{}\"", path);
            
            let output = std::process::Command::new("powershell")
                .arg("-Command")
                .arg(script)
                .output()
                .map_err(|e| anyhow::anyhow!("执行 PowerShell 失败: {}", e))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("启动应用失败: {}", stderr));
            }
        }
        
        #[cfg(target_os = "linux")]
        {
            // 使用 xdg-open 启动应用
            let output = std::process::Command::new("xdg-open")
                .arg(path)
                .output()
                .map_err(|e| anyhow::anyhow!("执行 xdg-open 失败: {}", e))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("启动应用失败: {}", stderr));
            }
        }
        
        Ok(ToolResult {
            success: true,
            output: format!("启动应用: {}", path),
            error: None,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(LaunchAppTool {
            inner: self.inner.clone(),
        })
    }
}

/// 点击屏幕 Tool
pub struct ClickScreenTool {
    inner: GuiAgentTool,
}

#[async_trait]
impl Tool for ClickScreenTool {
    fn name(&self) -> &str {
        "click_screen"
    }
    
    fn description(&self) -> &str {
        "点击屏幕指定位置"
    }
    
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "x": {
                    "type": "integer",
                    "description": "X 坐标"
                },
                "y": {
                    "type": "integer",
                    "description": "Y 坐标"
                }
            },
            "required": ["x", "y"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let x = args["x"].as_i64().ok_or_else(|| anyhow::anyhow!("参数 x 缺失或不是整数"))? as i32;
        let y = args["y"].as_i64().ok_or_else(|| anyhow::anyhow!("参数 y 缺失或不是整数"))? as i32;
        
        // 实现点击屏幕逻辑
        let executor = self.inner.executor.lock().await;
        executor.mouse_click(x, y)?;
        
        Ok(ToolResult {
            success: true,
            output: format!("点击屏幕位置: ({}, {})", x, y),
            error: None,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(ClickScreenTool {
            inner: self.inner.clone(),
        })
    }
}

/// 输入文本 Tool
pub struct TypeTextTool {
    inner: GuiAgentTool,
}

#[async_trait]
impl Tool for TypeTextTool {
    fn name(&self) -> &str {
        "type_text"
    }
    
    fn description(&self) -> &str {
        "在当前焦点位置输入文本"
    }
    
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "要输入的文本"
                }
            },
            "required": ["text"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let text = args["text"].as_str().ok_or_else(|| anyhow::anyhow!("参数 text 缺失或不是字符串"))?;
        
        // 实现输入文本逻辑
        let executor = self.inner.executor.lock().await;
        executor.type_text(text)?;
        
        Ok(ToolResult {
            success: true,
            output: format!("输入文本: {}", text),
            error: None,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(TypeTextTool {
            inner: self.inner.clone(),
        })
    }
}

/// 截取屏幕 Tool
pub struct CaptureScreenTool {
    inner: GuiAgentTool,
}

#[async_trait]
impl Tool for CaptureScreenTool {
    fn name(&self) -> &str {
        "capture_screen"
    }
    
    fn description(&self) -> &str {
        "截取整个屏幕，返回 Base64 编码的 PNG 图片"
    }
    
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "region": {
                    "type": "object",
                    "description": "可选的区域参数",
                    "properties": {
                        "x": { "type": "integer" },
                        "y": { "type": "integer" },
                        "width": { "type": "integer" },
                        "height": { "type": "integer" }
                    }
                }
            }
        })
    }
    
    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let capture = self.inner.capture.lock().await;
        let data = capture.capture_screen()?;
        let base64_data = base64::Engine::encode(&base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD), data);
        
        Ok(ToolResult {
            success: true,
            output: base64_data,
            error: None,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(CaptureScreenTool {
            inner: self.inner.clone(),
        })
    }
}

/// 列出窗口 Tool
pub struct ListWindowsTool {
    inner: GuiAgentTool,
}

#[async_trait]
impl Tool for ListWindowsTool {
    fn name(&self) -> &str {
        "list_windows"
    }
    
    fn description(&self) -> &str {
        "列出所有窗口信息"
    }
    
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
    
    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        // 实现列出窗口逻辑
        let manager = WindowManager::new();
        let windows = manager.list_windows()?;
        let windows_json = serde_json::to_string(&windows)?;
        
        Ok(ToolResult {
            success: true,
            output: windows_json,
            error: None,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(ListWindowsTool {
            inner: self.inner.clone(),
        })
    }
}

/// 查找窗口 Tool
pub struct FindWindowTool {
    inner: GuiAgentTool,
}

#[async_trait]
impl Tool for FindWindowTool {
    fn name(&self) -> &str {
        "find_window"
    }
    
    fn description(&self) -> &str {
        "根据窗口标题查找窗口"
    }
    
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "窗口标题"
                }
            },
            "required": ["title"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let title = args["title"].as_str().ok_or_else(|| anyhow::anyhow!("参数 title 缺失或不是字符串"))?;
        
        // 实现查找窗口逻辑
        let manager = WindowManager::new();
        let windows = manager.list_windows()?;
        
        // 查找匹配标题的窗口
        let window = windows.iter()
            .find(|w| w.title.contains(title))
            .cloned();
        
        let window_json = serde_json::to_string(&window)?;
        
        Ok(ToolResult {
            success: true,
            output: window_json,
            error: None,
        })
    }
    
    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(FindWindowTool {
            inner: self.inner.clone(),
        })
    }
}

impl Clone for GuiAgentTool {
    fn clone(&self) -> Self {
        GuiAgentTool {
            executor: self.executor.clone(),
            capture: self.capture.clone(),
        }
    }
}
