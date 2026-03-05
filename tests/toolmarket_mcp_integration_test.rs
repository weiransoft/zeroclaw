//! 集成测试：工具市场 MCP 集成
//! 
//! 测试工具市场与 MCP 服务器的集成功能，包括服务器管理、工具列表获取等

use zeroclaw::toolmarket::{ToolMarketplaceManager, ToolMarketplaceConfig, ToolSearchOptions, ToolSource};
use zeroclaw::security::SecurityPolicy;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_tool_marketplace_mcp_integration() {
    // 创建临时目录作为工作空间
    let temp_dir = tempdir().unwrap();
    let security = Arc::new(SecurityPolicy::default());
    
    // 创建工具市场管理器，只启用 MCP，禁用 ClawHub
    let mut manager = ToolMarketplaceManager::new(
        temp_dir.path().to_path_buf(),
        ToolMarketplaceConfig {
            clawhub: false, // 禁用 ClawHub 以避免证书问题
            ..ToolMarketplaceConfig::default()
        },
        security,
    );
    
    // 初始化工具市场
    let result = manager.initialize().await;
    assert!(result.is_ok(), "工具市场初始化失败: {:?}", result);
    
    // 测试获取统计信息
    let stats = manager.get_stats().await.unwrap();
    assert_eq!(stats.total_tools, 0);
    assert_eq!(stats.installed_tools, 0);
    
    // 测试搜索工具（MCP 源）
    let search_options = ToolSearchOptions {
        query: None,
        source: Some(ToolSource::MCP),
        category: None,
        tags: None,
        limit: Some(10),
        offset: Some(0),
    };
    
    let results = manager.search_tools(search_options).await.unwrap();
    // 由于没有运行的 MCP 服务器，结果应该为空
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_tool_marketplace_mcp_server_management() {
    // 创建临时目录作为工作空间
    let temp_dir = tempdir().unwrap();
    let security = Arc::new(SecurityPolicy::default());
    
    // 创建工具市场管理器，只启用 MCP，禁用 ClawHub
    let mut manager = ToolMarketplaceManager::new(
        temp_dir.path().to_path_buf(),
        ToolMarketplaceConfig {
            clawhub: false, // 禁用 ClawHub 以避免证书问题
            ..ToolMarketplaceConfig::default()
        },
        security,
    );
    
    // 初始化工具市场
    let result = manager.initialize().await;
    assert!(result.is_ok(), "工具市场初始化失败: {:?}", result);
    
    // 测试获取运行中的 MCP 服务器
    let running_servers = manager.get_running_mcp_servers().await.unwrap();
    // 初始状态应该没有运行的服务器
    assert!(running_servers.is_empty());
}

#[tokio::test]
async fn test_tool_marketplace_mcp_tool_installation() {
    // 创建临时目录作为工作空间
    let temp_dir = tempdir().unwrap();
    let security = Arc::new(SecurityPolicy::default());
    
    // 创建工具市场管理器，只启用 MCP，禁用 ClawHub
    let mut manager = ToolMarketplaceManager::new(
        temp_dir.path().to_path_buf(),
        ToolMarketplaceConfig {
            clawhub: false, // 禁用 ClawHub 以避免证书问题
            ..ToolMarketplaceConfig::default()
        },
        security,
    );
    
    // 初始化工具市场
    let result = manager.initialize().await;
    assert!(result.is_ok(), "工具市场初始化失败: {:?}", result);
    
    // 测试安装 MCP 工具（模拟）
    // 注意：由于没有实际运行的 MCP 服务器，这里会失败，这是预期的行为
    let install_result = manager.install_tool("mcp:test-server:test-tool").await;
    assert!(install_result.is_err(), "安装不存在的 MCP 工具应该失败");
}

#[tokio::test]
async fn test_tool_marketplace_config() {
    // 测试工具市场配置
    let config = ToolMarketplaceConfig::default();
    
    // 验证默认配置
    assert!(config.enabled);
    assert!(config.clawhub);
    assert!(config.mcp);
    assert!(config.auto_discover);
    assert!(!config.auto_update);
    assert_eq!(config.skills_dir, ".clawhub");
    assert_eq!(config.mcp_servers_dir, ".mcp");
}

#[tokio::test]
async fn test_tool_marketplace_search_options() {
    // 测试搜索选项
    let options = ToolSearchOptions {
        query: Some("test".to_string()),
        source: Some(ToolSource::MCP),
        category: Some("Development".to_string()),
        tags: Some(vec!["api".to_string(), "tool".to_string()]),
        limit: Some(20),
        offset: Some(0),
    };
    
    assert_eq!(options.query, Some("test".to_string()));
    assert_eq!(options.source, Some(ToolSource::MCP));
    assert_eq!(options.category, Some("Development".to_string()));
    assert_eq!(options.tags, Some(vec!["api".to_string(), "tool".to_string()]));
    assert_eq!(options.limit, Some(20));
    assert_eq!(options.offset, Some(0));
}
