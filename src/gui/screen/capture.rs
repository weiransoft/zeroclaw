/// 屏幕捕获模块
/// 
/// 本模块提供屏幕截图功能,支持全屏、区域、窗口截图。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::screen::capture::ScreenCapture;
/// 
/// // 创建屏幕捕获实例
/// let capture = ScreenCapture::new();
/// 
/// // 捕获全屏
/// let full_screen = capture.capture_screen().unwrap();
/// 
/// // 捕获指定区域
/// let region = capture.capture_region(0, 0, 100, 100).unwrap();
/// 
/// // 捕获指定窗口
/// let window = capture.capture_window(1234).unwrap();
/// ```

use std::result;
use std::process::Command;
use std::fs;

/// 屏幕捕获结果类型
pub type Result<T> = result::Result<T, ScreenCaptureError>;

/// 屏幕捕获错误类型
#[derive(Debug)]
pub enum ScreenCaptureError {
    /// 屏幕捕获失败
    CaptureFailed(String),
    /// 窗口未找到
    WindowNotFound(String),
    /// 参数无效
    InvalidParameter(String),
    /// 平台不支持
    PlatformNotSupported(String),
    /// 命令执行失败
    CommandFailed(String),
    /// 文件操作失败
    FileError(String),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for ScreenCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScreenCaptureError::CaptureFailed(msg) => write!(f, "屏幕捕获失败: {}", msg),
            ScreenCaptureError::WindowNotFound(msg) => write!(f, "窗口未找到: {}", msg),
            ScreenCaptureError::InvalidParameter(msg) => write!(f, "参数无效: {}", msg),
            ScreenCaptureError::PlatformNotSupported(msg) => write!(f, "平台不支持: {}", msg),
            ScreenCaptureError::CommandFailed(msg) => write!(f, "命令执行失败: {}", msg),
            ScreenCaptureError::FileError(msg) => write!(f, "文件操作失败: {}", msg),
            ScreenCaptureError::Other(msg) => write!(f, "其他错误: {}", msg),
        }
    }
}

impl std::error::Error for ScreenCaptureError {}

/// 屏幕捕获结构体
/// 
/// 提供屏幕截图功能,支持全屏、区域、窗口截图。
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::screen::capture::ScreenCapture;
/// 
/// let capture = ScreenCapture::new();
/// ```

pub struct ScreenCapture {
    /// 屏幕宽度
    width: u32,
    /// 屏幕高度
    height: u32,
}

impl ScreenCapture {
    /// 创建新的屏幕捕获实例
    /// 
    /// # 返回
    /// 
    /// * `ScreenCapture` - 屏幕捕获实例
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let capture = ScreenCapture::new();
    /// ```
    pub fn new() -> Self {
        // 获取屏幕分辨率
        let (width, height) = Self::get_screen_resolution();
        
        ScreenCapture { width, height }
    }
    
    /// 获取屏幕分辨率
    /// 
    /// # 返回
    /// 
    /// * `(u32, u32)` - 屏幕宽度和高度
    #[cfg(target_os = "macos")]
    fn get_screen_resolution() -> (u32, u32) {
        // 使用 AppleScript 获取屏幕分辨率
        let script = "tell application \"System Events\"\n    set displayCount to count of displays\n    if displayCount > 0 then\n        set mainDisplay to display 1\n        set {w, h} to dimensions of mainDisplay\n        return w & \"x\" & h\n    else\n        return \"1920x1080\"\n    end if\nend tell";
        
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split('x').collect();
                if parts.len() == 2 {
                    (parts[0].parse().unwrap_or(1920), parts[1].parse().unwrap_or(1080))
                } else {
                    (1920, 1080)
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("获取屏幕分辨率失败: {}", stderr);
                (1920, 1080)
            }
            Err(e) => {
                eprintln!("执行 AppleScript 失败: {}", e);
                (1920, 1080)
            }
        }
    }
    
    #[cfg(target_os = "linux")]
    fn get_screen_resolution() -> (u32, u32) {
        // 使用 xrandr 获取屏幕分辨率
        let output = Command::new("xrandr")
            .arg("--current")
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // 查找当前分辨率
                for line in stdout.lines() {
                    if line.contains('*') && line.contains('x') {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        for part in parts {
                            if part.contains('x') {
                                let dims: Vec<&str> = part.split('x').collect();
                                if dims.len() == 2 {
                                    return (dims[0].parse().unwrap_or(1920), dims[1].parse().unwrap_or(1080));
                                }
                            }
                        }
                    }
                }
                (1920, 1080)
            }
            _ => (1920, 1080),
        }
    }
    
    #[cfg(target_os = "windows")]
    fn get_screen_resolution() -> (u32, u32) {
        // TODO: 实现 Windows 屏幕分辨率获取
        (1920, 1080)
    }
    
    /// 获取屏幕宽度
    /// 
    /// # 返回
    /// 
    /// * `u32` - 屏幕宽度
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let capture = ScreenCapture::new();
    /// let width = capture.get_width();
    /// println!("屏幕宽度: {}", width);
    /// ```
    pub fn get_width(&self) -> u32 {
        self.width
    }
    
    /// 获取屏幕高度
    /// 
    /// # 返回
    /// 
    /// * `u32` - 屏幕高度
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let capture = ScreenCapture::new();
    /// let height = capture.get_height();
    /// println!("屏幕高度: {}", height);
    /// ```
    pub fn get_height(&self) -> u32 {
        self.height
    }
    
    /// 捕获全屏
    /// 
    /// # 返回
    /// 
    /// * `Result<Vec<u8>>` - 截图数据 (Base64 编码的 PNG 数据)
    /// 
    /// # 错误
    /// 
    /// * `ScreenCaptureError::CaptureFailed` - 屏幕捕获失败
    /// * `ScreenCaptureError::PlatformNotSupported` - 当前平台不支持
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let capture = ScreenCapture::new();
    /// let image = capture.capture_screen().unwrap();
    /// println!("截图大小: {} 字节", image.len());
    /// ```
    pub fn capture_screen(&self) -> Result<Vec<u8>> {
        #[cfg(target_os = "macos")]
        {
            self.capture_screen_macos()
        }
        #[cfg(target_os = "windows")]
        {
            self.capture_screen_windows()
        }
        #[cfg(target_os = "linux")]
        {
            self.capture_screen_linux()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Err(ScreenCaptureError::PlatformNotSupported(
                "当前平台不支持屏幕捕获".to_string()
            ))
        }
    }
    
    /// 捕获指定区域
    /// 
    /// # 参数
    /// 
    /// * `x` - 区域左上角 X 坐标
    /// * `y` - 区域左上角 Y 坐标
    /// * `width` - 区域宽度
    /// * `height` - 区域高度
    /// 
    /// # 返回
    /// 
    /// * `Result<Vec<u8>>` - 区域截图数据 (Base64 编码的 PNG 数据)
    /// 
    /// # 错误
    /// 
    /// * `ScreenCaptureError::InvalidParameter` - 参数无效
    /// * `ScreenCaptureError::CaptureFailed` - 屏幕捕获失败
    /// * `ScreenCaptureError::PlatformNotSupported` - 当前平台不支持
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let capture = ScreenCapture::new();
    /// let image = capture.capture_region(0, 0, 100, 100).unwrap();
    /// println!("区域截图大小: {} 字节", image.len());
    /// ```
    pub fn capture_region(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        // 验证参数
        if x >= self.width || y >= self.height {
            return Err(ScreenCaptureError::InvalidParameter(
                format!("坐标超出屏幕范围: ({}, {}), 屏幕分辨率: ({}, {})", x, y, self.width, self.height)
            ));
        }
        
        if width == 0 || height == 0 {
            return Err(ScreenCaptureError::InvalidParameter(
                "区域宽高不能为零".to_string()
            ));
        }
        
        if x + width > self.width || y + height > self.height {
            return Err(ScreenCaptureError::InvalidParameter(
                format!("区域超出屏幕范围: ({}, {}, {}, {})", x, y, width, height)
            ));
        }
        
        #[cfg(target_os = "macos")]
        {
            self.capture_region_macos(x, y, width, height)
        }
        #[cfg(target_os = "windows")]
        {
            self.capture_region_windows(x, y, width, height)
        }
        #[cfg(target_os = "linux")]
        {
            self.capture_region_linux(x, y, width, height)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Err(ScreenCaptureError::PlatformNotSupported(
                "当前平台不支持区域捕获".to_string()
            ))
        }
    }
    
    /// 捕获指定窗口
    /// 
    /// # 参数
    /// 
    /// * `window_id` - 窗口 ID
    /// 
    /// # 返回
    /// 
    /// * `Result<Vec<u8>>` - 窗口截图数据 (Base64 编码的 PNG 数据)
    /// 
    /// # 错误
    /// 
    /// * `ScreenCaptureError::WindowNotFound` - 窗口未找到
    /// * `ScreenCaptureError::CaptureFailed` - 屏幕捕获失败
    /// * `ScreenCaptureError::PlatformNotSupported` - 当前平台不支持
    /// 
    /// # 示例
    /// 
    /// ```rust
    /// let capture = ScreenCapture::new();
    /// let image = capture.capture_window(1234).unwrap();
    /// println!("窗口截图大小: {} 字节", image.len());
    /// ```
    pub fn capture_window(&self, window_id: u64) -> Result<Vec<u8>> {
        #[cfg(target_os = "macos")]
        {
            self.capture_window_macos(window_id)
        }
        #[cfg(target_os = "windows")]
        {
            self.capture_window_windows(window_id)
        }
        #[cfg(target_os = "linux")]
        {
            self.capture_window_linux(window_id)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Err(ScreenCaptureError::PlatformNotSupported(
                "当前平台不支持窗口捕获".to_string()
            ))
        }
    }
    
    // macOS 平台实现
    #[cfg(target_os = "macos")]
    fn capture_screen_macos(&self) -> Result<Vec<u8>> {
        // 使用 screencapture 命令行工具捕获屏幕
        // 创建临时文件
        let temp_path = "/tmp/zeroclaw_screenshot.png";
        
        // 执行 screencapture 命令
        let output = Command::new("screencapture")
            .arg("-x")  // 不播放声音
            .arg(temp_path)
            .output()
            .map_err(|e| ScreenCaptureError::CommandFailed(format!("执行 screencapture 命令失败: {}", e)))?;
        
        // 检查命令执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScreenCaptureError::CaptureFailed(format!(
                "screencapture 命令执行失败: {}",
                stderr
            )));
        }
        
        // 读取图片文件
        let data = fs::read(temp_path)
            .map_err(|e| ScreenCaptureError::FileError(format!("读取截图文件失败: {}", e)))?;
        
        // 删除临时文件
        let _ = fs::remove_file(temp_path);
        
        Ok(data)
    }
    
    #[cfg(target_os = "macos")]
    fn capture_region_macos(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        // 使用 screencapture 命令行工具捕获指定区域
        // screencapture -R x,y,width,height 截取指定区域
        
        let rect = format!("{},{},{},{}", x, y, width, height);
        let temp_path = "/tmp/zeroclaw_region_screenshot.png";
        
        // 执行 screencapture 命令
        let output = Command::new("screencapture")
            .arg("-x")  // 不播放声音
            .arg("-R")  // 指定区域
            .arg(&rect)
            .arg(temp_path)
            .output()
            .map_err(|e| ScreenCaptureError::CommandFailed(format!("执行 screencapture 命令失败: {}", e)))?;
        
        // 检查命令执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScreenCaptureError::CaptureFailed(format!(
                "screencapture 命令执行失败: {}",
                stderr
            )));
        }
        
        // 读取图片文件
        let data = fs::read(temp_path)
            .map_err(|e| ScreenCaptureError::FileError(format!("读取截图文件失败: {}", e)))?;
        
        // 删除临时文件
        let _ = fs::remove_file(temp_path);
        
        Ok(data)
    }
    
    #[cfg(target_os = "macos")]
    fn capture_window_macos(&self, window_id: u64) -> Result<Vec<u8>> {
        // 使用 macOS Accessibility API 捕获窗口
        // 通过 AppleScript 获取窗口信息,然后使用 screencapture 截取窗口区域
        
        // 首先获取窗口信息
        let window_info = self.get_window_info(window_id)?;
        
        // 使用 screencapture 捕获窗口区域
        // screencapture -R x,y,width,height 截取指定区域
        let rect = format!("{},{},{},{}", window_info.x, window_info.y, window_info.width, window_info.height);
        
        let temp_path = "/tmp/zeroclaw_window_screenshot.png";
        
        let output = Command::new("screencapture")
            .arg("-x")  // 不播放声音
            .arg("-R")  // 指定区域
            .arg(&rect)
            .arg(temp_path)
            .output()
            .map_err(|e| ScreenCaptureError::CommandFailed(format!("执行 screencapture 命令失败: {}", e)))?;
        
        // 检查命令执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScreenCaptureError::CaptureFailed(format!(
                "screencapture 命令执行失败: {}",
                stderr
            )));
        }
        
        // 读取图片文件
        let data = fs::read(temp_path)
            .map_err(|e| ScreenCaptureError::FileError(format!("读取窗口截图文件失败: {}", e)))?;
        
        // 删除临时文件
        let _ = fs::remove_file(temp_path);
        
        Ok(data)
    }
    
    /// 获取窗口信息
    /// 
    /// # 参数
    /// 
    /// * `window_id` - 窗口 ID
    /// 
    /// # 返回
    /// 
    /// * `Result<WindowInfo>` - 窗口信息
    /// 
    /// # 错误
    /// 
    /// * `ScreenCaptureError::WindowNotFound` - 窗口未找到
    /// * `ScreenCaptureError::Other` - 其他错误
    fn get_window_info(&self, window_id: u64) -> Result<WindowInfo> {
        // 使用 AppleScript 获取窗口信息
        let script = format!(
            "tell application \"System Events\"\n    repeat with proc in every process\n        try\n            repeat with win in every window of proc\n                if id of win is {} then\n                    return {{id:id of win, name:name of win, position:position of win, size:size of win}}\n                end if\n            end repeat\n        end try\n    end repeat\n    return {{id:0, name:\"\", position:{{0, 0}}, size:{{0, 0}}}}\nend tell",
            window_id
        );
        
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| ScreenCaptureError::CommandFailed(format!("执行 AppleScript 失败: {}", e)))?;
        
        // 检查命令执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScreenCaptureError::CaptureFailed(format!(
                "获取窗口信息失败: {}",
                stderr
            )));
        }
        
        // 解析 AppleScript 输出
        let stdout = String::from_utf8_lossy(&output.stdout);
        let info = WindowInfo::parse_applescript_window(stdout.trim())
            .map_err(|e| ScreenCaptureError::WindowNotFound(format!("解析窗口信息失败: {}", e)))?;
        
        if info.id == 0 {
            return Err(ScreenCaptureError::WindowNotFound(format!(
                "未找到 ID 为 {} 的窗口",
                window_id
            )));
        }
        
        Ok(info)
    }
    
    // Windows 平台实现
    #[cfg(target_os = "windows")]
    fn capture_screen_windows(&self) -> Result<Vec<u8>> {
        // 使用 Windows API 捕获屏幕
        // TODO: 实现 Windows 屏幕捕获
        unimplemented!("Windows 屏幕捕获待实现")
    }
    
    #[cfg(target_os = "windows")]
    fn capture_region_windows(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        // 使用 Windows API 捕获区域
        // TODO: 实现 Windows 区域捕获
        unimplemented!("Windows 区域捕获待实现")
    }
    
    #[cfg(target_os = "windows")]
    fn capture_window_windows(&self, window_id: u64) -> Result<Vec<u8>> {
        // 使用 Windows UI Automation 捕获窗口
        // TODO: 实现 Windows 窗口捕获
        unimplemented!("Windows 窗口捕获待实现")
    }
    
    // Linux 平台实现
    #[cfg(target_os = "linux")]
    fn capture_screen_linux(&self) -> Result<Vec<u8>> {
        // 使用 scrot 命令行工具捕获屏幕
        // 创建临时文件
        let temp_path = "/tmp/zeroclaw_screenshot.png";
        
        // 执行 scrot 命令
        let output = Command::new("scrot")
            .arg(temp_path)
            .output()
            .map_err(|e| ScreenCaptureError::CommandFailed(format!("执行 scrot 命令失败: {}", e)))?;
        
        // 检查命令执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScreenCaptureError::CaptureFailed(format!(
                "scrot 命令执行失败: {}",
                stderr
            )));
        }
        
        // 读取图片文件
        let data = fs::read(temp_path)
            .map_err(|e| ScreenCaptureError::FileError(format!("读取截图文件失败: {}", e)))?;
        
        // 删除临时文件
        let _ = fs::remove_file(temp_path);
        
        Ok(data)
    }
    
    #[cfg(target_os = "linux")]
    fn capture_region_linux(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Vec<u8>> {
        // 使用 scrot 命令行工具捕获指定区域
        // scrot 支持 -s 选项选择区域, -x -y -w -h 选项指定区域
        let temp_path = "/tmp/zeroclaw_region_screenshot.png";
        
        // 执行 scrot 命令
        let output = Command::new("scrot")
            .arg("-s")  // 选择区域
            .arg("-b")  // 包含边框
            .arg(temp_path)
            .output()
            .map_err(|e| ScreenCaptureError::CommandFailed(format!("执行 scrot 命令失败: {}", e)))?;
        
        // 检查命令执行结果
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScreenCaptureError::CaptureFailed(format!(
                "scrot 命令执行失败: {}",
                stderr
            )));
        }
        
        // 读取图片文件
        let data = fs::read(temp_path)
            .map_err(|e| ScreenCaptureError::FileError(format!("读取截图文件失败: {}", e)))?;
        
        // 删除临时文件
        let _ = fs::remove_file(temp_path);
        
        Ok(data)
    }
    
    #[cfg(target_os = "linux")]
    fn capture_window_linux(&self, _window_id: u64) -> Result<Vec<u8>> {
        // 使用 scrot 命令行工具捕获指定窗口
        // 由于 scrot 不直接支持窗口捕获,这里返回错误
        // 实际实现需要使用 xwininfo 和 scrot 组合
        Err(ScreenCaptureError::PlatformNotSupported(
            "Linux 窗口捕获需要额外实现".to_string()
        ))
    }
}

impl Default for ScreenCapture {
    fn default() -> Self {
        Self::new()
    }
}
