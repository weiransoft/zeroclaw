/// GUI Agent 窗口管理模块
/// 
/// 本模块提供窗口管理功能,包括窗口枚举、窗口信息获取等。

use std::result;
use serde::{Deserialize, Serialize};

/// 窗口信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// 窗口 ID
    pub id: u64,
    /// 窗口标题
    pub title: String,
    /// 窗口位置 X
    pub x: i32,
    /// 窗口位置 Y
    pub y: i32,
    /// 窗口宽度
    pub width: u32,
    /// 窗口高度
    pub height: u32,
}

impl WindowInfo {
    /// 解析 AppleScript 返回的窗口信息
    /// 
    /// # 参数
    /// 
    /// * `applescript_str` - AppleScript 返回的字符串，格式为: {id:1234, name:"Window Title", position:{100, 200}, size:{800, 600}}
    /// 
    /// # 返回
    /// 
    /// * `Result<WindowInfo>` - 窗口信息
    /// 
    /// # 错误
    /// 
    /// * `WindowManagerError::Other` - 解析失败
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let info = WindowInfo::parse_applescript_window("{id:1234, name:\"Test\", position:{100, 200}, size:{800, 600}}");
    /// ```
    pub fn parse_applescript_window(applescript_str: &str) -> Result<WindowInfo> {
        // 移除开头和结尾的花括号
        let content = applescript_str
            .trim()
            .strip_prefix("{")
            .and_then(|s| s.strip_suffix("}"))
            .ok_or_else(|| WindowManagerError::Other("无效的 AppleScript 窗口格式".to_string()))?;
        
        // 解析各个字段
        let mut id: u64 = 0;
        let mut title = String::new();
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        
        // 解析 id
        if let Some(id_str) = content.split(", ").find(|s| s.starts_with("id:")) {
            if let Some(value) = id_str.split(':').nth(1) {
                id = value.trim().parse().map_err(|_| {
                    WindowManagerError::Other("解析窗口 ID 失败".to_string())
                })?;
            }
        }
        
        // 解析 name
        if let Some(name_str) = content.split(", ").find(|s| s.starts_with("name:")) {
            // 提取引号之间的内容
            if let Some(start) = name_str.find('"') {
                if let Some(end) = name_str[start + 1..].find('"') {
                    title = name_str[start + 1..start + 1 + end].to_string();
                }
            }
        }
        
        // 解析 position
        if let Some(pos_str) = content.split(", ").find(|s| s.starts_with("position:")) {
            if let Some(start) = pos_str.find('{') {
                if let Some(end) = pos_str[start..].find('}') {
                    let pos_content = &pos_str[start + 1..start + end];
                    let coords: Vec<&str> = pos_content.split(',').collect();
                    if coords.len() >= 2 {
                        x = coords[0].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口 X 坐标失败".to_string())
                        })?;
                        y = coords[1].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口 Y 坐标失败".to_string())
                        })?;
                    }
                }
            }
        }
        
        // 解析 size
        if let Some(size_str) = content.split(", ").find(|s| s.starts_with("size:")) {
            if let Some(start) = size_str.find('{') {
                if let Some(end) = size_str[start..].find('}') {
                    let size_content = &size_str[start + 1..start + end];
                    let dims: Vec<&str> = size_content.split(',').collect();
                    if dims.len() >= 2 {
                        width = dims[0].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口宽度失败".to_string())
                        })?;
                        height = dims[1].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口高度失败".to_string())
                        })?;
                    }
                }
            }
        }
        
        Ok(WindowInfo {
            id,
            title,
            x,
            y,
            width,
            height,
        })
    }
}

/// 窗口管理结果类型
pub type Result<T> = result::Result<T, WindowManagerError>;

/// 窗口管理错误类型
#[derive(Debug)]
pub enum WindowManagerError {
    /// 窗口未找到
    WindowNotFound(String),
    /// 平台不支持
    PlatformNotSupported(String),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for WindowManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowManagerError::WindowNotFound(msg) => write!(f, "窗口未找到: {}", msg),
            WindowManagerError::PlatformNotSupported(msg) => write!(f, "平台不支持: {}", msg),
            WindowManagerError::Other(msg) => write!(f, "其他错误: {}", msg),
        }
    }
}

impl std::error::Error for WindowManagerError {}

/// 窗口管理器
/// 
/// 提供窗口管理功能,包括窗口枚举、窗口信息获取等。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::screen::window::WindowManager;
/// 
/// let manager = WindowManager::new();
/// let windows = manager.list_windows().unwrap();
/// for window in windows {
///     println!("窗口: {:?}, 标题: {}", window.id, window.title);
/// }
/// ```

pub struct WindowManager {}

impl WindowManager {
    /// 创建新的窗口管理器实例
    /// 
    /// # 返回
    /// 
    /// * `WindowManager` - 窗口管理器实例
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let manager = WindowManager::new();
    /// ```
    pub fn new() -> Self {
        WindowManager {}
    }
    
    /// 枚举所有窗口
    /// 
    /// # 返回
    /// 
    /// * `Result<Vec<WindowInfo>>` - 窗口列表
    /// 
    /// # 错误
    /// 
    /// * `WindowManagerError::PlatformNotSupported` - 当前平台不支持
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let manager = WindowManager::new();
    /// let windows = manager.list_windows().unwrap();
    /// for window in windows {
    ///     println!("窗口: {:?}, 标题: {}", window.id, window.title);
    /// }
    /// ```
    pub fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        #[cfg(target_os = "macos")]
        {
            self.list_windows_macos()
        }
        #[cfg(target_os = "windows")]
        {
            self.list_windows_windows()
        }
        #[cfg(target_os = "linux")]
        {
            self.list_windows_linux()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Err(WindowManagerError::PlatformNotSupported(
                "当前平台不支持窗口枚举".to_string()
            ))
        }
    }
    
    /// 获取前台窗口
    /// 
    /// # 返回
    /// 
    /// * `Result<Option<WindowInfo>>` - 前台窗口信息,如果不存在则返回 None
    /// 
    /// # 错误
    /// 
    /// * `WindowManagerError::PlatformNotSupported` - 当前平台不支持
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let manager = WindowManager::new();
    /// if let Some(window) = manager.get_foreground_window() {
    ///     println!("前台窗口: {:?}, 标题: {}", window.id, window.title);
    /// }
    /// ```
    pub fn get_foreground_window(&self) -> Result<Option<WindowInfo>> {
        #[cfg(target_os = "macos")]
        {
            self.get_foreground_window_macos()
        }
        #[cfg(target_os = "windows")]
        {
            self.get_foreground_window_windows()
        }
        #[cfg(target_os = "linux")]
        {
            self.get_foreground_window_linux()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Err(WindowManagerError::PlatformNotSupported(
                "当前平台不支持获取前台窗口".to_string()
            ))
        }
    }
    
    // macOS 平台实现
    #[cfg(target_os = "macos")]
    fn list_windows_macos(&self) -> Result<Vec<WindowInfo>> {
        // 使用 AppleScript 获取窗口列表，返回 JSON 格式
        let script = "use framework \"Foundation\"\nuse scripting additions\nset jsonData to \"{\\\"windows\\\":[]}\"\ntell application \"System Events\"\n    set windowList to {}\n    repeat with proc in every process\n        try\n            repeat with win in every window of proc\n                set end of windowList to {id:id of win, name:name of win, position:position of win, size:size of win}\n            end repeat\n        end try\n    end repeat\n    return windowList\nend tell";
        
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| WindowManagerError::Other(format!("执行 AppleScript 失败: {}", e)))?;
        
        // 检查执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WindowManagerError::Other(format!(
                "获取窗口列表失败: {}",
                stderr
            )));
        }
        
        // 解析 AppleScript 输出
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut windows = Vec::new();
        
        // 解析 AppleScript 返回的列表格式
        // 格式类似: {{id:1234, name:"Window Title", position:{100, 200}, size:{800, 600}}, ...}
        for line in stdout.lines() {
            if line.trim().starts_with("{") && line.trim().ends_with("}") {
                let window_info = Self::parse_applescript_window(line.trim());
                if let Ok(info) = window_info {
                    windows.push(info);
                }
            }
        }
        
        Ok(windows)
    }
    
    #[cfg(target_os = "macos")]
    fn get_foreground_window_macos(&self) -> Result<Option<WindowInfo>> {
        // 使用 AppleScript 获取前台窗口，返回 JSON 格式
        let script = "use framework \"Foundation\"\nuse scripting additions\ntell application \"System Events\"\n    set frontApp to name of first application process whose frontmost is true\n    tell process frontApp\n        set frontWindow to front window\n        return {id:id of frontWindow, name:name of frontWindow, position:position of frontWindow, size:size of frontWindow}\n    end tell\nend tell";
        
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| WindowManagerError::Other(format!("执行 AppleScript 失败: {}", e)))?;
        
        // 检查执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WindowManagerError::Other(format!(
                "获取前台窗口失败: {}",
                stderr
            )));
        }
        
        // 解析 AppleScript 输出
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // 解析 AppleScript 返回的窗口信息
        if stdout.trim().starts_with("{") && stdout.trim().ends_with("}") {
            match Self::parse_applescript_window(stdout.trim()) {
                Ok(info) => Ok(Some(info)),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
    
    /// 解析 AppleScript 返回的窗口信息
    /// 
    /// # 参数
    /// 
    /// * `applescript_str` - AppleScript 返回的字符串，格式为: {id:1234, name:"Window Title", position:{100, 200}, size:{800, 600}}
    /// 
    /// # 返回
    /// 
    /// * `Result<WindowInfo>` - 窗口信息
    /// 
    /// # 错误
    /// 
    /// * `WindowManagerError::Other` - 解析失败
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let info = WindowInfo::parse_applescript_window("{id:1234, name:\"Test\", position:{100, 200}, size:{800, 600}}");
    /// ```
    fn parse_applescript_window(applescript_str: &str) -> Result<WindowInfo> {
        // 移除开头和结尾的花括号
        let content = applescript_str
            .trim()
            .strip_prefix("{")
            .and_then(|s| s.strip_suffix("}"))
            .ok_or_else(|| WindowManagerError::Other("无效的 AppleScript 窗口格式".to_string()))?;
        
        // 解析各个字段
        let mut id: u64 = 0;
        let mut title = String::new();
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        
        // 解析 id
        if let Some(id_str) = content.split(", ").find(|s| s.starts_with("id:")) {
            if let Some(value) = id_str.split(':').nth(1) {
                id = value.trim().parse().map_err(|_| {
                    WindowManagerError::Other("解析窗口 ID 失败".to_string())
                })?;
            }
        }
        
        // 解析 name
        if let Some(name_str) = content.split(", ").find(|s| s.starts_with("name:")) {
            // 提取引号之间的内容
            if let Some(start) = name_str.find('"') {
                if let Some(end) = name_str[start + 1..].find('"') {
                    title = name_str[start + 1..start + 1 + end].to_string();
                }
            }
        }
        
        // 解析 position
        if let Some(pos_str) = content.split(", ").find(|s| s.starts_with("position:")) {
            if let Some(start) = pos_str.find('{') {
                if let Some(end) = pos_str[start..].find('}') {
                    let pos_content = &pos_str[start + 1..start + end];
                    let coords: Vec<&str> = pos_content.split(',').collect();
                    if coords.len() >= 2 {
                        x = coords[0].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口 X 坐标失败".to_string())
                        })?;
                        y = coords[1].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口 Y 坐标失败".to_string())
                        })?;
                    }
                }
            }
        }
        
        // 解析 size
        if let Some(size_str) = content.split(", ").find(|s| s.starts_with("size:")) {
            if let Some(start) = size_str.find('{') {
                if let Some(end) = size_str[start..].find('}') {
                    let size_content = &size_str[start + 1..start + end];
                    let dims: Vec<&str> = size_content.split(',').collect();
                    if dims.len() >= 2 {
                        width = dims[0].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口宽度失败".to_string())
                        })?;
                        height = dims[1].trim().parse().map_err(|_| {
                            WindowManagerError::Other("解析窗口高度失败".to_string())
                        })?;
                    }
                }
            }
        }
        
        Ok(WindowInfo {
            id,
            title,
            x,
            y,
            width,
            height,
        })
    }
    
    // Windows 平台实现
    #[cfg(target_os = "windows")]
    fn list_windows_windows(&self) -> Result<Vec<WindowInfo>> {
        // 使用 Windows API 枚举窗口
        // 通过 PowerShell 脚本获取窗口列表
        let script = r#"
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Window {
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc enumProc, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
'@;

$windows = @();
[Window]::EnumWindows({ $hWnd = $_; $text = New-Object System.Text.StringBuilder(256); [Window]::GetWindowText($hWnd, $text, 256) | Out-Null; $rect = New-Object RECT; [Window]::GetWindowRect($hWnd, [ref]$rect) | Out-Null; if ($text.ToString() -and (Test-Path variable:\IsWindowVisible) -and [Window]::IsWindowVisible($hWnd)) { $windows += @{ Id = $hWnd; Title = $text.ToString(); X = $rect.Left; Y = $rect.Top; Width = $rect.Right - $rect.Left; Height = $rect.Bottom - $rect.Top } }; return $true }, [IntPtr]::Zero) | Out-Null;
$windows | ConvertTo-Json
"#;
        
        let output = std::process::Command::new("powershell")
            .arg("-Command")
            .arg(script)
            .output()
            .map_err(|e| WindowManagerError::OperationFailed(format!("执行 PowerShell 脚本失败: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WindowManagerError::OperationFailed(format!(
                "窗口枚举失败: {}",
                stderr
            )));
        }
        
        // TODO: 解析 JSON 输出
        // 暂时返回空列表
        Ok(vec![])
    }
    
    #[cfg(target_os = "windows")]
    fn get_foreground_window_windows(&self) -> Result<Option<WindowInfo>> {
        // 使用 Windows API 获取前台窗口
        // 通过 PowerShell 脚本获取前台窗口
        let script = r#"
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Window {
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")]
    public static extern bool GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
'@;

$hWnd = [Window]::GetForegroundWindow();
if ($hWnd -ne [IntPtr]::Zero) {
    $text = New-Object System.Text.StringBuilder(256);
    [Window]::GetWindowText($hWnd, $text, 256) | Out-Null;
    $rect = New-Object RECT;
    [Window]::GetWindowRect($hWnd, [ref]$rect) | Out-Null;
    @{ Id = $hWnd; Title = $text.ToString(); X = $rect.Left; Y = $rect.Top; Width = $rect.Right - $rect.Left; Height = $rect.Bottom - $rect.Top } | ConvertTo-Json
} else {
    Write-Output "null"
}
"#;
        
        let output = std::process::Command::new("powershell")
            .arg("-Command")
            .arg(script)
            .output()
            .map_err(|e| WindowManagerError::OperationFailed(format!("执行 PowerShell 脚本失败: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WindowManagerError::OperationFailed(format!(
                "获取前台窗口失败: {}",
                stderr
            )));
        }
        
        // TODO: 解析 JSON 输出
        // 暂时返回 None
        Ok(None)
    }
    
    // Linux 平台实现
    #[cfg(target_os = "linux")]
    fn list_windows_linux(&self) -> Result<Vec<WindowInfo>> {
        // 使用 xwininfo 和 wmctrl 获取窗口列表
        // 通过 wmctrl 获取窗口列表
        let output = std::process::Command::new("wmctrl")
            .arg("-l")
            .output()
            .map_err(|e| WindowManagerError::OperationFailed(format!("执行 wmctrl 命令失败: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WindowManagerError::OperationFailed(format!(
                "窗口枚举失败: {}",
                stderr
            )));
        }
        
        // TODO: 解析 wmctrl 输出
        // 暂时返回空列表
        Ok(vec![])
    }
    
    #[cfg(target_os = "linux")]
    fn get_foreground_window_linux(&self) -> Result<Option<WindowInfo>> {
        // 使用 xprop 获取前台窗口
        // 通过 wmctrl 获取前台窗口
        let output = std::process::Command::new("wmctrl")
            .arg("-a")
            .arg(":ACTIVE:")
            .output()
            .map_err(|e| WindowManagerError::OperationFailed(format!("执行 wmctrl 命令失败: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WindowManagerError::OperationFailed(format!(
                "获取前台窗口失败: {}",
                stderr
            )));
        }
        
        // TODO: 解析 wmctrl 输出
        // 暂时返回 None
        Ok(None)
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}
