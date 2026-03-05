use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::mcp::{MCPManager, MCPServerConfig, ToolDefinition};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPIntegrationConfig {
    pub enabled: bool,
    pub servers_dir: String,
    pub auto_start: bool,
    pub default_timeout: u64,
}

impl Default for MCPIntegrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            servers_dir: ".mcp".to_string(),
            auto_start: false,
            default_timeout: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerInfo {
    pub name: String,
    pub status: MCPServerStatus,
    pub tools_count: usize,
    pub resources_count: usize,
    pub prompts_count: usize,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MCPServerStatus {
    Running,
    Stopped,
    Error,
}

pub struct MCPIntegration {
    manager: MCPManager,
    config: MCPIntegrationConfig,
    workspace_dir: PathBuf,
    servers: Arc<RwLock<HashMap<String, MCPServerConfig>>>,
}

impl MCPIntegration {
    pub fn new(workspace_dir: PathBuf, config: MCPIntegrationConfig) -> Self {
        Self {
            manager: MCPManager::new(),
            config,
            workspace_dir,
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        self.load_servers().await?;
        
        if self.config.auto_start {
            self.start_all_servers().await?;
        }

        Ok(())
    }

    pub async fn load_servers(&mut self) -> Result<()> {
        let servers_dir = self.workspace_dir.join(&self.config.servers_dir);
        if !servers_dir.exists() {
            return Ok(());
        }

        // 这里可以实现从配置文件加载服务器
        // 暂时返回
        Ok(())
    }

    pub async fn add_server(&self, config: MCPServerConfig) -> Result<()> {
        self.manager.register_server(config.clone()).await?;
        self.servers.write().await.insert(config.name.clone(), config);
        Ok(())
    }

    pub async fn start_server(&self, name: &str) -> Result<()> {
        self.manager.start_server(name).await
    }

    pub async fn stop_server(&self, name: &str) -> Result<()> {
        self.manager.stop_server(name).await
    }

    pub async fn start_all_servers(&self) -> Result<()> {
        let servers = self.servers.read().await;
        for (name, _) in servers.iter() {
            let _ = self.manager.start_server(name).await;
        }
        Ok(())
    }

    pub async fn stop_all_servers(&self) -> Result<()> {
        self.manager.stop_all().await
    }

    pub async fn get_server_info(&self, name: &str) -> Result<Option<MCPServerInfo>> {
        let running_servers = self.manager.get_running_servers().await;
        let is_running = running_servers.contains(&name.to_string());

        if !is_running {
            return Ok(Some(MCPServerInfo {
                name: name.to_string(),
                status: MCPServerStatus::Stopped,
                tools_count: 0,
                resources_count: 0,
                prompts_count: 0,
                version: None,
            }));
        }

        // 这里需要获取服务器信息，暂时返回模拟数据
        Ok(Some(MCPServerInfo {
            name: name.to_string(),
            status: MCPServerStatus::Running,
            tools_count: 0,
            resources_count: 0,
            prompts_count: 0,
            version: Some("1.0.0".to_string()),
        }))
    }

    pub async fn list_servers(&self) -> Result<Vec<MCPServerInfo>> {
        let servers = self.servers.read().await;
        let running_servers: HashSet<String> = self.manager.get_running_servers().await.into_iter().collect();

        let mut server_infos = Vec::new();
        for (name, _) in servers.iter() {
            let status = if running_servers.contains(name) {
                MCPServerStatus::Running
            } else {
                MCPServerStatus::Stopped
            };

            server_infos.push(MCPServerInfo {
                name: name.clone(),
                status,
                tools_count: 0,
                resources_count: 0,
                prompts_count: 0,
                version: None,
            });
        }

        Ok(server_infos)
    }

    pub async fn get_all_tools(&self) -> Result<HashMap<String, Vec<ToolDefinition>>> {
        Ok(self.manager.get_all_tools().await)
    }

    pub async fn get_server_tools(&self, server_name: &str) -> Result<Vec<ToolDefinition>> {
        let all_tools = self.manager.get_all_tools().await;
        Ok(all_tools.get(server_name).cloned().unwrap_or_default())
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<crate::mcp::ToolCallResult> {
        self.manager.call_tool(server_name, tool_name, arguments).await
    }

    pub async fn read_resource(&self, server_name: &str, uri: &str) -> Result<crate::mcp::ContentBlock> {
        self.manager.read_resource(server_name, uri).await
    }

    pub async fn get_prompt(
        &self,
        server_name: &str,
        prompt_name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> Result<Vec<crate::mcp::PromptMessage>> {
        self.manager.get_prompt(server_name, prompt_name, arguments).await
    }

    pub fn config(&self) -> &MCPIntegrationConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: MCPIntegrationConfig) {
        self.config = config;
    }

    pub async fn get_running_servers(&self) -> Result<Vec<String>> {
        Ok(self.manager.get_running_servers().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_mcp_integration_config_default() {
        let config = MCPIntegrationConfig::default();
        assert!(config.enabled);
        assert_eq!(config.servers_dir, ".mcp");
        assert!(!config.auto_start);
        assert_eq!(config.default_timeout, 30);
    }

    #[test]
    fn test_mcp_server_info() {
        let server_info = MCPServerInfo {
            name: "test-server".to_string(),
            status: MCPServerStatus::Running,
            tools_count: 5,
            resources_count: 3,
            prompts_count: 2,
            version: Some("1.0.0".to_string()),
        };

        assert_eq!(server_info.name, "test-server");
        assert_eq!(server_info.status, MCPServerStatus::Running);
        assert_eq!(server_info.tools_count, 5);
    }

    #[tokio::test]
    async fn test_mcp_integration_creation() {
        let temp_dir = tempdir().unwrap();
        let config = MCPIntegrationConfig::default();
        let mut integration = MCPIntegration::new(temp_dir.path().to_path_buf(), config);

        let result = integration.initialize().await;
        assert!(result.is_ok());

        let servers = integration.list_servers().await.unwrap();
        assert!(servers.is_empty());
    }

    #[test]
    fn test_mcp_server_status_serde() {
        let status = MCPServerStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Running\"");

        let parsed: MCPServerStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MCPServerStatus::Running);
    }
}
