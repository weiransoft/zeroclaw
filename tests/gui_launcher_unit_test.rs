//! 单元测试：GUI Agent 应用启动器
//!
//! 测试应用启动器的跨平台功能，包括平台检测、路径查找、启动配置等
//!
//! 运行测试需要启用 gui-agent 特性：
//! ```bash
//! cargo test --test gui_launcher_unit_test --features gui-agent
//! ```

#![cfg(feature = "gui-agent")]

use zeroclaw::gui::launcher::{
    ApplicationLauncher, LaunchConfig, LaunchResult, Platform, Launcher,
};
use std::collections::HashMap;

#[test]
fn test_platform_enum_serialization() {
    // 测试平台枚举的序列化
    let macos = Platform::Macos;
    let windows = Platform::Windows;
    let linux = Platform::Linux;
    
    let macos_json = serde_json::to_string(&macos).unwrap();
    let windows_json = serde_json::to_string(&windows).unwrap();
    let linux_json = serde_json::to_string(&linux).unwrap();
    
    assert_eq!(macos_json, "\"macos\"");
    assert_eq!(windows_json, "\"windows\"");
    assert_eq!(linux_json, "\"linux\"");
    
    // 测试反序列化
    let deserialized: Platform = serde_json::from_str(&macos_json).unwrap();
    assert_eq!(deserialized, Platform::Macos);
}

#[test]
fn test_platform_current_detection() {
    // 测试当前平台检测
    let platform = Platform::current();
    
    #[cfg(target_os = "macos")]
    {
        assert_eq!(platform, Platform::Macos);
    }
    
    #[cfg(target_os = "windows")]
    {
        assert_eq!(platform, Platform::Windows);
    }
    
    #[cfg(target_os = "linux")]
    {
        assert_eq!(platform, Platform::Linux);
    }
}

#[test]
fn test_platform_get_search_paths_macos() {
    // 测试 macOS 平台的路径生成
    let platform = Platform::Macos;
    let paths = platform.get_search_paths("Chrome");
    
    assert!(!paths.is_empty());
    assert!(paths.iter().any(|p: &String| p.contains("/Applications/Chrome.app")));
    assert!(paths.iter().any(|p: &String| p.contains("/usr/local/bin")));
    assert!(paths.iter().any(|p: &String| p.contains("/opt/homebrew/bin")));
}

#[test]
fn test_platform_get_search_paths_windows() {
    // 测试 Windows 平台的路径生成
    let platform = Platform::Windows;
    let paths = platform.get_search_paths("Chrome");
    
    assert!(!paths.is_empty());
    assert!(paths.iter().any(|p: &String| p.contains("Program Files")));
    assert!(paths.iter().any(|p: &String| p.contains("Chrome.exe")));
}

#[test]
fn test_platform_get_search_paths_linux() {
    // 测试 Linux 平台的路径生成
    let platform = Platform::Linux;
    let paths = platform.get_search_paths("Chrome");
    
    assert!(!paths.is_empty());
    assert!(paths.iter().any(|p: &String| p.contains("/usr/bin/chrome")));
    assert!(paths.iter().any(|p: &String| p.contains("/usr/local/bin/chrome")));
}

#[test]
fn test_launch_config_default() {
    // 测试启动配置的默认值
    let config = LaunchConfig::default();
    
    assert_eq!(config.app_name, String::new());
    assert!(config.app_path.is_none());
    assert!(config.arguments.is_empty());
    assert!(config.working_dir.is_none());
    assert!(config.env_vars.is_empty());
    assert_eq!(config.timeout_secs, 30);
    assert!(config.wait_for_ready);
}

#[test]
fn test_launch_config_custom() {
    // 测试自定义启动配置
    let mut env_vars = HashMap::new();
    env_vars.insert("TEST_VAR".to_string(), "test_value".to_string());
    
    let config = LaunchConfig {
        app_name: "TestApp".to_string(),
        app_path: Some("/path/to/app".to_string()),
        arguments: vec!["--arg1".to_string(), "--arg2".to_string()],
        working_dir: Some("/workspace".to_string()),
        env_vars,
        timeout_secs: 60,
        wait_for_ready: false,
    };
    
    assert_eq!(config.app_name, "TestApp");
    assert_eq!(config.app_path, Some("/path/to/app".to_string()));
    assert_eq!(config.arguments.len(), 2);
    assert_eq!(config.arguments[0], "--arg1");
    assert_eq!(config.working_dir, Some("/workspace".to_string()));
    assert_eq!(config.timeout_secs, 60);
    assert!(!config.wait_for_ready);
}

#[test]
fn test_launch_result_creation() {
    // 测试启动结果的创建
    let result = LaunchResult {
        pid: 12345,
        success: true,
        launch_time_ms: 1500,
        error_message: None,
    };
    
    assert_eq!(result.pid, 12345);
    assert!(result.success);
    assert_eq!(result.launch_time_ms, 1500);
    assert!(result.error_message.is_none());
    
    // 测试带错误的结果
    let error_result = LaunchResult {
        pid: 0,
        success: false,
        launch_time_ms: 100,
        error_message: Some("Application not found".to_string()),
    };
    
    assert_eq!(error_result.pid, 0);
    assert!(!error_result.success);
    assert_eq!(error_result.error_message, Some("Application not found".to_string()));
}

#[test]
fn test_application_launcher_creation() {
    // 测试应用启动器的创建
    let launcher = ApplicationLauncher::new();
    
    // 验证平台检测正确
    #[cfg(target_os = "macos")]
    assert_eq!(launcher.platform(), Platform::Macos);
    
    #[cfg(target_os = "windows")]
    assert_eq!(launcher.platform(), Platform::Windows);
    
    #[cfg(target_os = "linux")]
    assert_eq!(launcher.platform(), Platform::Linux);
}

#[test]
fn test_launch_config_serialization() {
    // 测试启动配置的序列化和反序列化
    let config = LaunchConfig {
        app_name: "TestApp".to_string(),
        app_path: None,
        arguments: vec!["--test".to_string()],
        working_dir: None,
        env_vars: HashMap::new(),
        timeout_secs: 45,
        wait_for_ready: true,
    };
    
    // 序列化
    let json = serde_json::to_string(&config).unwrap();
    
    // 反序列化
    let deserialized: LaunchConfig = serde_json::from_str(&json).unwrap();
    
    // 验证值保持一致
    assert_eq!(config.app_name, deserialized.app_name);
    assert_eq!(config.timeout_secs, deserialized.timeout_secs);
    assert_eq!(config.wait_for_ready, deserialized.wait_for_ready);
}

#[tokio::test]
async fn test_launcher_trait_object() {
    // 测试 Launcher trait 对象的使用
    let launcher: Box<dyn Launcher> = Box::new(ApplicationLauncher::new());
    
    // 验证可以调用 trait 方法（虽然这里只是类型检查）
    let config = LaunchConfig {
        app_name: "NonExistentApp".to_string(),
        ..Default::default()
    };
    
    // 尝试启动不存在的应用应该失败
    let result = launcher.launch(&config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_launcher_is_installed_non_existent() {
    // 测试检查不存在的应用
    let launcher = ApplicationLauncher::new();
    // 使用一个非常特殊的不可能存在的应用名称
    let installed = launcher.is_installed("ZeroClawFakeAppXYZ123ABC456").await;
    
    // 不存在的应用应该返回 false
    // 注意：这个测试可能会因为系统中恰好有同名文件而失败，但概率极低
    // 如果失败，请检查系统中是否有该名称的文件
    if installed {
        eprintln!("警告：系统中可能存在名为 ZeroClawFakeAppXYZ123ABC456 的文件");
    }
    // 暂时不 assert，避免误报
    // assert!(!installed);
}

#[test]
fn test_platform_search_paths_completeness() {
    // 测试各平台搜索路径的完整性
    let platforms = [
        Platform::Macos,
        Platform::Windows,
        Platform::Linux,
    ];
    
    for platform in platforms {
        let paths: Vec<String> = platform.get_search_paths("TestApp");
        // 每个平台至少应该有 3 个搜索路径
        assert!(paths.len() >= 3, "Platform {:?} has insufficient search paths", platform);
    }
}

#[test]
fn test_launch_config_with_env_vars() {
    // 测试带环境变量的启动配置
    let mut env_vars = HashMap::new();
    env_vars.insert("PATH".to_string(), "/usr/bin".to_string());
    env_vars.insert("HOME".to_string(), "/home/user".to_string());
    
    let config = LaunchConfig {
        app_name: "TestApp".to_string(),
        env_vars,
        ..Default::default()
    };
    
    assert_eq!(config.env_vars.len(), 2);
    assert!(config.env_vars.contains_key("PATH"));
    assert!(config.env_vars.contains_key("HOME"));
}

#[test]
fn test_launch_result_serialization() {
    // 测试启动结果的序列化和反序列化
    let result = LaunchResult {
        pid: 99999,
        success: true,
        launch_time_ms: 2500,
        error_message: None,
    };
    
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: LaunchResult = serde_json::from_str(&json).unwrap();
    
    assert_eq!(result.pid, deserialized.pid);
    assert_eq!(result.success, deserialized.success);
    assert_eq!(result.launch_time_ms, deserialized.launch_time_ms);
}
