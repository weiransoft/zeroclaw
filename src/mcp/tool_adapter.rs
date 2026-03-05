use anyhow::{Result, Context};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::tools::traits::{Tool, ToolResult};
use super::{MCPManager, ToolDefinition, ContentBlock};

pub struct MCPTool {
    server_name: String,
    tool_definition: ToolDefinition,
    manager: Arc<MCPManager>,
}

impl MCPTool {
    pub fn new(
        server_name: String,
        tool_definition: ToolDefinition,
        manager: Arc<MCPManager>,
    ) -> Self {
        Self {
            server_name,
            tool_definition,
            manager,
        }
    }

    fn parse_content_to_result(content: &[ContentBlock]) -> String {
        content.iter()
            .filter_map(|c| c.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait]
impl Tool for MCPTool {
    fn name(&self) -> &str {
        &self.tool_definition.name
    }

    fn description(&self) -> &str {
        &self.tool_definition.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.tool_definition.input_schema.clone()
    }

    fn clone_box(&self) -> Box<dyn Tool> {
        Box::new(Self {
            server_name: self.server_name.clone(),
            tool_definition: self.tool_definition.clone(),
            manager: self.manager.clone(),
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult> {
        let result = self.manager
            .call_tool(&self.server_name, &self.tool_definition.name, Some(params))
            .await?;

        let output = Self::parse_content_to_result(&result.content);
        let error = if result.is_error {
            Some(output.clone())
        } else {
            None
        };

        Ok(ToolResult {
            success: !result.is_error,
            output,
            error,
        })
    }
}

pub struct MCPToolAdapter {
    manager: Arc<MCPManager>,
}

impl MCPToolAdapter {
    pub fn new(manager: Arc<MCPManager>) -> Self {
        Self { manager }
    }

    pub async fn get_all_tools_as_boxed(&self) -> Result<Vec<Box<dyn Tool>>> {
        let all_tools = self.manager.get_all_tools().await;
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();

        for (server_name, tool_definitions) in all_tools {
            for tool_def in tool_definitions {
                let tool = MCPTool::new(
                    server_name.clone(),
                    tool_def,
                    self.manager.clone(),
                );
                tools.push(Box::new(tool));
            }
        }

        Ok(tools)
    }

    pub async fn get_tools_for_server(&self, server_name: &str) -> Result<Vec<Box<dyn Tool>>> {
        let all_tools = self.manager.get_all_tools().await;
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();

        if let Some(tool_definitions) = all_tools.get(server_name) {
            for tool_def in tool_definitions {
                let tool = MCPTool::new(
                    server_name.to_string(),
                    tool_def.clone(),
                    self.manager.clone(),
                );
                tools.push(Box::new(tool));
            }
        }

        Ok(tools)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPConfig {
    pub servers: HashMap<String, MCPServerConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerConfigEntry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub disabled: bool,
}

impl Default for MCPConfig {
    fn default() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }
}

impl MCPConfig {
    pub fn from_toml(content: &str) -> Result<Self> {
        let config: Self = toml::from_str(content)
            .with_context(|| "Failed to parse MCP config")?;
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .with_context(|| "Failed to serialize MCP config")
    }

    pub fn add_server(&mut self, name: String, config: MCPServerConfigEntry) {
        self.servers.insert(name, config);
    }

    pub fn remove_server(&mut self, name: &str) -> Option<MCPServerConfigEntry> {
        self.servers.remove(name)
    }

    pub fn get_server(&self, name: &str) -> Option<&MCPServerConfigEntry> {
        self.servers.get(name)
    }

    pub fn list_servers(&self) -> Vec<&String> {
        self.servers.keys().collect()
    }
}

pub async fn load_mcp_config_from_file(path: &std::path::Path) -> Result<MCPConfig> {
    if !path.exists() {
        return Ok(MCPConfig::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read MCP config from {:?}", path))?;
    
    MCPConfig::from_toml(&content)
}

pub async fn save_mcp_config_to_file(config: &MCPConfig, path: &std::path::Path) -> Result<()> {
    let content = config.to_toml()?;
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    std::fs::write(path, content)
        .with_context(|| format!("Failed to write MCP config to {:?}", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_default() {
        let config = MCPConfig::default();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_mcp_config_toml() {
        let toml_str = r#"
[servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

[servers.git]
command = "uvx"
args = ["mcp-server-git"]
disabled = true
"#;

        let config: MCPConfig = toml::from_str(toml_str).unwrap();
        
        assert_eq!(config.servers.len(), 2);
        assert!(config.servers.contains_key("filesystem"));
        assert!(config.servers.contains_key("git"));
        
        let fs_config = config.servers.get("filesystem").unwrap();
        assert_eq!(fs_config.command, "npx");
        assert!(!fs_config.disabled);
        
        let git_config = config.servers.get("git").unwrap();
        assert!(git_config.disabled);
    }

    #[test]
    fn test_mcp_config_add_remove() {
        let mut config = MCPConfig::default();
        
        config.add_server("test".to_string(), MCPServerConfigEntry {
            command: "test-cmd".to_string(),
            args: vec![],
            env: HashMap::new(),
            disabled: false,
        });
        
        assert_eq!(config.servers.len(), 1);
        
        let removed = config.remove_server("test");
        assert!(removed.is_some());
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_mcp_server_config_entry() {
        let entry = MCPServerConfigEntry {
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: [("NODE_ENV".to_string(), "production".to_string())].into(),
            disabled: false,
        };
        
        assert_eq!(entry.command, "node");
        assert_eq!(entry.args.len(), 1);
        assert_eq!(entry.env.get("NODE_ENV").unwrap(), "production");
    }
}
