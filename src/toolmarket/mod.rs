use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod clawhub_integration;
pub mod mcp_integration;
pub mod commands;

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum ToolMarketCommands {
    /// List available tools from ClawHub and MCP
    Search {
        /// Search query
        #[arg(required = false, default_value = "")]
        query: String,
        /// Filter by source (clawhub, mcp)
        #[arg(long, required = false)]
        source: Option<String>,
        /// Filter by category
        #[arg(long, required = false)]
        category: Option<String>,
        /// Limit results
        #[arg(long, default_value = "20")]
        limit: u32,
    },

    /// List installed tools
    List,

    /// Install a tool
    Install {
        /// Tool ID
        tool_id: String,
    },

    /// Uninstall a tool
    Uninstall {
        /// Tool ID
        tool_id: String,
    },

    /// Enable a tool
    Enable {
        /// Tool ID
        tool_id: String,
    },

    /// Disable a tool
    Disable {
        /// Tool ID
        tool_id: String,
    },

    /// Get tool information
    Info {
        /// Tool ID
        tool_id: String,
    },

    /// Get marketplace stats
    Stats,

    /// Manage MCP servers
    MCP {
        #[command(subcommand)]
        mcp_command: MCPMarketCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum MCPMarketCommands {
    /// List MCP servers
    List,

    /// Add a new MCP server
    Add {
        /// Server name
        name: String,
        /// Command to run
        command: String,
        /// Arguments
        #[arg(required = false, num_args = 0..)]
        args: Vec<String>,
    },

    /// Start an MCP server
    Start {
        /// Server name
        name: String,
    },

    /// Stop an MCP server
    Stop {
        /// Server name
        name: String,
    },

    /// List tools from an MCP server
    Tools {
        /// Server name
        name: String,
    },
}

use crate::config::Config;
use crate::mcp::{MCPManager, MCPServerConfig};
use crate::skills::clawhub::ClawHubClient;
use crate::tools::traits::Tool;
use crate::security::SecurityPolicy;
use crate::runtime::RuntimeAdapter;
use crate::memory::Memory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMarketplaceConfig {
    pub enabled: bool,
    pub clawhub: bool,
    pub mcp: bool,
    pub auto_discover: bool,
    pub auto_update: bool,
    pub skills_dir: String,
    pub mcp_servers_dir: String,
}

impl Default for ToolMarketplaceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            clawhub: true,
            mcp: true,
            auto_discover: true,
            auto_update: false,
            skills_dir: ".clawhub".to_string(),
            mcp_servers_dir: ".mcp".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: ToolSource,
    pub category: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub installed: bool,
    pub enabled: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolSource {
    ClawHub,
    MCP,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceStats {
    pub total_tools: usize,
    pub installed_tools: usize,
    pub by_source: HashMap<ToolSource, usize>,
    pub by_category: HashMap<String, usize>,
    pub last_sync: Option<i64>,
    pub pending_operations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchOptions {
    pub query: Option<String>,
    pub source: Option<ToolSource>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub struct ToolMarketplaceManager {
    config: ToolMarketplaceConfig,
    workspace_dir: PathBuf,
    clawhub_client: Option<ClawHubClient>,
    mcp_manager: MCPManager,
    installed_tools: Arc<RwLock<HashMap<String, MarketplaceToolInfo>>>,
    security: Arc<SecurityPolicy>,
}

impl ToolMarketplaceManager {
    pub fn new(
        workspace_dir: PathBuf,
        config: ToolMarketplaceConfig,
        security: Arc<SecurityPolicy>,
    ) -> Self {
        let clawhub_client = if config.clawhub {
            Some(ClawHubClient::with_default_config(workspace_dir.clone()))
        } else {
            None
        };

        Self {
            config,
            workspace_dir,
            clawhub_client,
            mcp_manager: MCPManager::new(),
            installed_tools: Arc::new(RwLock::new(HashMap::new())),
            security,
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        // 初始化 ClawHub 客户端
        if let Some(client) = &self.clawhub_client {
            // 加载已安装的技能
            let installed_skills = client.get_installed_skills()?;
            for skill in installed_skills {
                let tool_info = MarketplaceToolInfo {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: format!("ClawHub 技能: {}", skill.name),
                    source: ToolSource::ClawHub,
                    category: None,
                    version: Some(skill.version),
                    author: None,
                    installed: true,
                    enabled: skill.enabled,
                    path: Some(skill.path),
                };
                self.installed_tools.write().await.insert(skill.id, tool_info);
            }
        }

        // 初始化 MCP 服务器
        if self.config.mcp {
            self.load_mcp_servers().await?;
        }

        Ok(())
    }

    pub async fn search_tools(&self, options: ToolSearchOptions) -> Result<Vec<MarketplaceToolInfo>> {
        let mut results = Vec::new();

        // 从 ClawHub 搜索
        if self.config.clawhub && self.clawhub_client.is_some() {
            let client = self.clawhub_client.as_ref().unwrap();
            let clawhub_results = client.search_skills(
                options.query.as_deref().unwrap_or(""),
                None,
            ).await?;

            for skill in clawhub_results.skills {
                let installed = self.installed_tools.read().await.contains_key(&skill.id);
                let tool_info = MarketplaceToolInfo {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    source: ToolSource::ClawHub,
                    category: Some(skill.category.to_string()),
                    version: Some(skill.version),
                    author: Some(skill.author),
                    installed,
                    enabled: installed,
                    path: None,
                };
                results.push(tool_info);
            }
        }

        // 从 MCP 搜索
        if self.config.mcp {
            let mcp_tools = self.mcp_manager.get_all_tools().await;
            for (server_name, tools) in mcp_tools {
                for tool in tools {
                    let id = format!("mcp:{server_name}:{}", tool.name);
                    let installed = self.installed_tools.read().await.contains_key(&id);
                    let tool_info = MarketplaceToolInfo {
                        id: id.clone(),
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        source: ToolSource::MCP,
                        category: None,
                        version: None,
                        author: Some(server_name.clone()),
                        installed,
                        enabled: installed,
                        path: None,
                    };
                    results.push(tool_info);
                }
            }
        }

        // 应用搜索过滤
        if let Some(query) = &options.query {
            results.retain(|tool| {
                tool.name.to_lowercase().contains(&query.to_lowercase()) ||
                tool.description.to_lowercase().contains(&query.to_lowercase())
            });
        }

        if let Some(source) = &options.source {
            results.retain(|tool| tool.source == *source);
        }

        if let Some(category) = &options.category {
            results.retain(|tool| {
                if let Some(tool_category) = &tool.category {
                    tool_category.to_lowercase() == category.to_lowercase()
                } else {
                    false
                }
            });
        }

        // 应用分页
        let limit = options.limit.unwrap_or(20);
        let offset = options.offset.unwrap_or(0);
        let start = offset as usize;
        let _end = (offset + limit) as usize;
        results = results.into_iter().skip(start).take(limit as usize).collect();

        Ok(results)
    }

    pub async fn list_installed_tools(&self) -> Result<Vec<MarketplaceToolInfo>> {
        let tools = self.installed_tools.read().await;
        Ok(tools.values().cloned().collect())
    }

    pub async fn install_tool(&mut self, tool_id: &str) -> Result<MarketplaceToolInfo> {
        // 检查工具来源
        if tool_id.starts_with("mcp:") {
            // MCP 工具不需要安装，只需要启用
            let parts: Vec<&str> = tool_id.split(":").collect();
            if parts.len() < 3 {
                return Err(anyhow::anyhow!("Invalid MCP tool ID format"));
            }
            let server_name = parts[1];
            let tool_name = parts[2];

            // 确保 MCP 服务器正在运行
            let running_servers = self.mcp_manager.get_running_servers().await;
            if !running_servers.contains(&server_name.to_string()) {
                return Err(anyhow::anyhow!("MCP server {} is not running", server_name));
            }

            let tool_info = MarketplaceToolInfo {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                description: format!("MCP tool: {}", tool_name),
                source: ToolSource::MCP,
                category: None,
                version: None,
                author: Some(server_name.to_string()),
                installed: true,
                enabled: true,
                path: None,
            };

            self.installed_tools.write().await.insert(tool_id.to_string(), tool_info.clone());
            Ok(tool_info)
        } else {
            // ClawHub 工具
            if let Some(_client) = &self.clawhub_client {
                // 这里需要实现下载和安装逻辑
                // 暂时返回模拟数据
                let tool_info = MarketplaceToolInfo {
                    id: tool_id.to_string(),
                    name: tool_id.to_string(),
                    description: format!("Installed ClawHub tool: {}", tool_id),
                    source: ToolSource::ClawHub,
                    category: None,
                    version: Some("1.0.0".to_string()),
                    author: Some("unknown".to_string()),
                    installed: true,
                    enabled: true,
                    path: Some(self.workspace_dir.join(&self.config.skills_dir).join(tool_id).to_string_lossy().to_string()),
                };

                self.installed_tools.write().await.insert(tool_id.to_string(), tool_info.clone());
                Ok(tool_info)
            } else {
                Err(anyhow::anyhow!("ClawHub is disabled"))
            }
        }
    }

    pub async fn uninstall_tool(&mut self, tool_id: &str) -> Result<()> {
        self.installed_tools.write().await.remove(tool_id);
        Ok(())
    }

    pub async fn enable_tool(&mut self, tool_id: &str) -> Result<()> {
        if let Some(tool) = self.installed_tools.write().await.get_mut(tool_id) {
            tool.enabled = true;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Tool not found"))
        }
    }

    pub async fn disable_tool(&mut self, tool_id: &str) -> Result<()> {
        if let Some(tool) = self.installed_tools.write().await.get_mut(tool_id) {
            tool.enabled = false;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Tool not found"))
        }
    }

    pub async fn get_tool_info(&self, tool_id: &str) -> Result<Option<MarketplaceToolInfo>> {
        let tools = self.installed_tools.read().await;
        Ok(tools.get(tool_id).cloned())
    }

    pub async fn get_stats(&self) -> Result<MarketplaceStats> {
        let tools = self.installed_tools.read().await;
        let mut by_source = HashMap::new();
        let mut by_category = HashMap::new();

        for tool in tools.values() {
            *by_source.entry(tool.source.clone()).or_insert(0) += 1;
            if let Some(category) = &tool.category {
                *by_category.entry(category.clone()).or_insert(0) += 1;
            }
        }

        Ok(MarketplaceStats {
            total_tools: tools.len(),
            installed_tools: tools.len(),
            by_source,
            by_category,
            last_sync: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64),
            pending_operations: 0,
        })
    }

    pub async fn load_mcp_servers(&self) -> Result<()> {
        let mcp_dir = self.workspace_dir.join(&self.config.mcp_servers_dir);
        if !mcp_dir.exists() {
            return Ok(());
        }

        // 这里可以实现从配置文件加载 MCP 服务器
        // 暂时返回
        Ok(())
    }

    pub async fn add_mcp_server(&self, config: MCPServerConfig) -> Result<()> {
        self.mcp_manager.register_server(config).await
    }

    pub async fn start_mcp_server(&self, name: &str) -> Result<()> {
        self.mcp_manager.start_server(name).await
    }

    pub async fn stop_mcp_server(&self, name: &str) -> Result<()> {
        self.mcp_manager.stop_server(name).await
    }

    pub async fn get_running_mcp_servers(&self) -> Result<Vec<String>> {
        Ok(self.mcp_manager.get_running_servers().await)
    }

    pub async fn build_tool_list(
        &self,
        _runtime: Arc<dyn RuntimeAdapter>,
        _memory: Arc<dyn Memory>,
        _config: Arc<Config>,
    ) -> Result<Vec<Box<dyn Tool>>> {
        let tools: Vec<Box<dyn Tool>> = vec![];

        // 添加已启用的工具
        let installed_tools = self.installed_tools.read().await;
        for tool in installed_tools.values() {
            if tool.enabled {
                // 这里需要根据工具类型创建相应的 Tool 实例
                // 暂时跳过，后续实现
            }
        }

        Ok(tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_tool_marketplace_config_default() {
        let config = ToolMarketplaceConfig::default();
        assert!(config.enabled);
        assert!(config.clawhub);
        assert!(config.mcp);
        assert_eq!(config.skills_dir, ".clawhub");
        assert_eq!(config.mcp_servers_dir, ".mcp");
    }

    #[test]
    fn test_marketplace_tool_info() {
        let tool_info = MarketplaceToolInfo {
            id: "test-tool".to_string(),
            name: "Test Tool".to_string(),
            description: "A test tool".to_string(),
            source: ToolSource::ClawHub,
            category: Some("Development".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("test".to_string()),
            installed: true,
            enabled: true,
            path: Some("/path/to/tool".to_string()),
        };

        assert_eq!(tool_info.id, "test-tool");
        assert!(tool_info.installed);
        assert!(tool_info.enabled);
    }

    #[tokio::test]
    async fn test_tool_marketplace_manager() {
        let temp_dir = tempdir().unwrap();
        let security = Arc::new(SecurityPolicy::default());
        let mut manager = ToolMarketplaceManager::new(
            temp_dir.path().to_path_buf(),
            ToolMarketplaceConfig::default(),
            security,
        );

        let result = manager.initialize().await;
        assert!(result.is_ok());

        let stats = manager.get_stats().await.unwrap();
        assert_eq!(stats.total_tools, 0);
        assert_eq!(stats.installed_tools, 0);
    }

    #[test]
    fn test_tool_source_serde() {
        let source = ToolSource::ClawHub;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, "\"ClawHub\"");

        let parsed: ToolSource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ToolSource::ClawHub);
    }
}
