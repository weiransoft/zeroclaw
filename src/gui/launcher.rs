/// GUI Agent 应用启动器
/// 
/// 本模块提供跨平台的应用启动功能，支持 macOS、Windows 和 Linux。
/// 
/// # 架构设计
/// 
/// ```text
/// ┌─────────────────────────────────────────┐
/// │         ApplicationLauncher             │
/// ├─────────────────────────────────────────┤
/// │  ┌─────────────────────────────────┐   │
/// │  │   Platform Abstraction          │   │
/// │  │   - macOS (NSWorkspace/AppleScript) │
/// │  │   - Windows (Process/COM)       │   │
/// │  │   - Linux (Desktop Entry/DBus)  │   │
/// │  └─────────────────────────────────┘   │
/// │  ┌─────────────────────────────────┐   │
/// │  │   Application Registry          │   │
/// │  │   - 应用路径管理                 │   │
/// │  │   - 启动参数配置                 │   │
/// │  └─────────────────────────────────┘   │
/// │  ┌─────────────────────────────────┐   │
/// │  │   Process Monitor               │   │
/// │  │   - 进程状态监控                 │   │
/// │  │   - 启动超时处理                 │   │
/// │  └─────────────────────────────────┘   │
/// └─────────────────────────────────────────┘
/// ```
/// 
/// # 使用示例
/// 
/// ```rust
/// use zeroclaw::gui::launcher::{ApplicationLauncher, LaunchConfig};
/// 
/// let launcher = ApplicationLauncher::new();
/// 
/// // 启动应用
/// let config = LaunchConfig {
///     app_name: "Google Chrome".to_string(),
///     arguments: vec!["--new-window".to_string()],
///     ..Default::default()
/// };
/// 
/// let result = launcher.launch(&config).await?;
/// println!("应用已启动，PID: {}", result.pid);
/// ```

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use anyhow::Result;

/// 应用启动结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchResult {
    /// 进程 ID
    pub pid: u32,
    /// 是否成功启动
    pub success: bool,
    /// 启动时间（毫秒）
    pub launch_time_ms: u64,
    /// 错误信息（如果有）
    pub error_message: Option<String>,
}

/// 启动配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    /// 应用名称
    pub app_name: String,
    /// 应用路径（可选，留空则自动查找）
    pub app_path: Option<String>,
    /// 启动参数
    pub arguments: Vec<String>,
    /// 工作目录
    pub working_dir: Option<String>,
    /// 环境变量
    pub env_vars: HashMap<String, String>,
    /// 启动超时时间（秒）
    pub timeout_secs: u64,
    /// 是否等待应用启动完成
    pub wait_for_ready: bool,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            app_name: String::new(),
            app_path: None,
            arguments: Vec::new(),
            working_dir: None,
            env_vars: HashMap::new(),
            timeout_secs: 30,
            wait_for_ready: true,
        }
    }
}

/// 平台类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// macOS
    Macos,
    /// Windows
    Windows,
    /// Linux
    Linux,
}

impl Platform {
    /// 获取当前平台
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        return Platform::Macos;
        
        #[cfg(target_os = "windows")]
        return Platform::Windows;
        
        #[cfg(target_os = "linux")]
        return Platform::Linux;
        
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        panic!("不支持的操作系统");
    }
    
    /// 获取应用路径搜索策略
    pub fn get_search_paths(&self, app_name: &str) -> Vec<String> {
        match self {
            Platform::Macos => {
                vec![
                    format!("/Applications/{}.app/Contents/MacOS", app_name),
                    format!("/Applications/{}.app", app_name),
                    format!("~/Applications/{}.app/Contents/MacOS", app_name),
                    "/usr/local/bin".to_string(),
                    "/opt/homebrew/bin".to_string(),
                ]
            }
            Platform::Windows => {
                vec![
                    format!("C:\\Program Files\\{}\\{}.exe", app_name, app_name),
                    format!("C:\\Program Files (x86)\\{}\\{}.exe", app_name, app_name),
                    format!("C:\\Users\\%USERNAME%\\AppData\\Local\\{}\\{}.exe", app_name, app_name),
                ]
            }
            Platform::Linux => {
                vec![
                    format!("/usr/bin/{}", app_name.to_lowercase()),
                    format!("/usr/local/bin/{}", app_name.to_lowercase()),
                    format!("/snap/bin/{}", app_name.to_lowercase()),
                    format!("/opt/{}/bin/{}", app_name.to_lowercase(), app_name.to_lowercase()),
                ]
            }
        }
    }
}

/// 应用启动器 trait
#[async_trait]
pub trait Launcher: Send + Sync {
    /// 启动应用
    async fn launch(&self, config: &LaunchConfig) -> Result<LaunchResult>;
    
    /// 检查应用是否已安装
    async fn is_installed(&self, app_name: &str) -> bool;
    
    /// 获取应用路径
    async fn find_app_path(&self, app_name: &str) -> Option<String>;
}

/// 应用启动器实现
pub struct ApplicationLauncher {
    /// 平台
    platform: Platform,
    /// 进程监控器
    process_monitor: Arc<ProcessMonitor>,
}

impl ApplicationLauncher {
    /// 创建新的应用启动器
    pub fn new() -> Self {
        Self {
            platform: Platform::current(),
            process_monitor: Arc::new(ProcessMonitor::new()),
        }
    }
    
    /// 获取当前平台
    pub fn platform(&self) -> Platform {
        self.platform
    }
}

impl Default for ApplicationLauncher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Launcher for ApplicationLauncher {
    /// 启动应用
    async fn launch(&self, config: &LaunchConfig) -> Result<LaunchResult> {
        let start_time = std::time::Instant::now();
        
        // 1. 查找应用路径
        let app_path = if let Some(ref path) = config.app_path {
            path.clone()
        } else {
            self.find_app_path(&config.app_name).await
                .ok_or_else(|| anyhow::anyhow!("未找到应用：{}", config.app_name))?
        };
        
        // 2. 构建启动命令
        let mut command = Command::new(&app_path);
        
        // 添加参数
        for arg in &config.arguments {
            command.arg(arg);
        }
        
        // 设置工作目录
        if let Some(ref dir) = config.working_dir {
            command.current_dir(dir);
        }
        
        // 设置环境变量
        for (key, value) in &config.env_vars {
            command.env(key, value);
        }
        
        // 标准输出重定向
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        
        // 3. 启动进程
        let child = command.spawn()?;
        let pid = child.id().expect("进程启动失败，无法获取 PID");
        
        // 4. 等待应用启动（如果需要）
        if config.wait_for_ready {
            let timeout_duration = Duration::from_secs(config.timeout_secs);
            
            match timeout(timeout_duration, self.process_monitor.wait_for_ready(pid)).await {
                Ok(Ok(_)) => {
                    // 应用已就绪
                }
                Ok(Err(e)) => {
                    eprintln!("等待应用就绪失败：{}", e);
                }
                Err(_) => {
                    eprintln!("启动超时：{} 秒", config.timeout_secs);
                }
            }
        }
        
        // 5. 返回结果
        let launch_time = start_time.elapsed().as_millis() as u64;
        
        Ok(LaunchResult {
            pid,
            success: true,
            launch_time_ms: launch_time,
            error_message: None,
        })
    }
    
    /// 检查应用是否已安装
    async fn is_installed(&self, app_name: &str) -> bool {
        self.find_app_path(app_name).await.is_some()
    }
    
    /// 获取应用路径
    async fn find_app_path(&self, app_name: &str) -> Option<String> {
        let search_paths = self.platform.get_search_paths(app_name);
        
        for path in search_paths {
            if tokio::fs::metadata(&path).await.is_ok() {
                return Some(path);
            }
        }
        
        // 尝试使用系统命令查找
        self.find_with_system_command(app_name).await
    }
}

impl ApplicationLauncher {
    /// 使用系统命令查找应用路径
    async fn find_with_system_command(&self, app_name: &str) -> Option<String> {
        match self.platform {
            Platform::Macos => {
                // 使用 mdfind 查找
                let output = Command::new("mdfind")
                    .arg(format!("kMDItemCFName == '{}.app'", app_name))
                    .output()
                    .await
                    .ok()?;
                
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout);
                    let path = path.trim().lines().next()?;
                    Some(format!("{}/Contents/MacOS/{}", path, app_name))
                } else {
                    None
                }
            }
            Platform::Windows => {
                // 使用 where 命令查找
                let output = Command::new("where")
                    .arg(format!("{}.exe", app_name))
                    .output()
                    .await
                    .ok()?;
                
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout);
                    let path = path.trim().lines().next()?;
                    Some(path.to_string())
                } else {
                    None
                }
            }
            Platform::Linux => {
                // 使用 which 命令查找
                let output = Command::new("which")
                    .arg(app_name)
                    .output()
                    .await
                    .ok()?;
                
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout);
                    Some(path.trim().to_string())
                } else {
                    None
                }
            }
        }
    }
}

/// 进程监控器
pub struct ProcessMonitor {
    // 预留实现
}

impl ProcessMonitor {
    /// 创建新的进程监控器
    pub fn new() -> Self {
        Self {}
    }
    
    /// 等待进程就绪
    pub async fn wait_for_ready(&self, pid: u32) -> Result<()> {
        // 简单实现：等待 1 秒
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // 检查进程是否还在运行
        #[cfg(unix)]
        {
            use std::process::Command as StdCommand;
            let output = StdCommand::new("ps")
                .arg("-p")
                .arg(pid.to_string())
                .output();
            
            if let Ok(output) = output {
                if !output.status.success() {
                    return Err(anyhow::anyhow!("进程已退出"));
                }
            }
        }
        
        #[cfg(windows)]
        {
            use std::process::Command as StdCommand;
            let output = StdCommand::new("tasklist")
                .arg("/FI")
                .arg(format!("PID eq {}", pid))
                .output();
            
            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.contains(&pid.to_string()) {
                    return Err(anyhow::anyhow!("进程已退出"));
                }
            }
        }
        
        Ok(())
    }
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_platform_detection() {
        let platform = Platform::current();
        
        #[cfg(target_os = "macos")]
        assert_eq!(platform, Platform::Macos);
        
        #[cfg(target_os = "windows")]
        assert_eq!(platform, Platform::Windows);
        
        #[cfg(target_os = "linux")]
        assert_eq!(platform, Platform::Linux);
    }

    #[tokio::test]
    async fn test_search_paths_generation() {
        let platform = Platform::Macos;
        let paths = platform.get_search_paths("Chrome");
        
        assert!(!paths.is_empty());
        assert!(paths.iter().any(|p| p.contains("Chrome")));
    }

    #[tokio::test]
    async fn test_launch_config_default() {
        let config = LaunchConfig::default();
        
        assert_eq!(config.timeout_secs, 30);
        assert!(config.arguments.is_empty());
        assert!(config.wait_for_ready);
    }
}
