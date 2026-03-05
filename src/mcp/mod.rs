use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::io::BufReader;
use tokio::sync::{Mutex, RwLock};

pub mod store;
pub mod tool_adapter;

#[allow(unused_imports)]
pub use store::{
    MCPServerStore, MCPServerRecord, MCPServerStatus,
    MCPServerCreateRequest, MCPServerUpdateRequest,
};

#[allow(unused_imports)]
pub use tool_adapter::{
    MCPTool, MCPToolAdapter, MCPConfig, MCPServerConfigEntry,
    load_mcp_config_from_file, save_mcp_config_to_file,
};

const MCP_VERSION: &str = "2024-11-05";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<MCPError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    #[serde(default)]
    pub subscribe: bool,
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingCapability {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: MCPCapabilities,
    pub server_info: ImplementationInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReference {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: String,
    pub content: ContentBlock,
}

#[derive(Debug, Clone)]
pub struct MCPServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub disabled: bool,
}

impl Default for MCPServerConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            command: "".to_string(),
            args: vec![],
            env: HashMap::new(),
            disabled: false,
        }
    }
}

pub struct MCPClient {
    name: String,
    process: Option<Child>,
    request_id: Arc<Mutex<u64>>,
    initialized: Arc<RwLock<bool>>,
    capabilities: Arc<RwLock<Option<MCPCapabilities>>>,
    server_info: Arc<RwLock<Option<ImplementationInfo>>>,
    tools: Arc<RwLock<Vec<ToolDefinition>>>,
    resources: Arc<RwLock<Vec<Resource>>>,
    prompts: Arc<RwLock<Vec<PromptDefinition>>>,
}

impl MCPClient {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            process: None,
            request_id: Arc::new(Mutex::new(1)),
            initialized: Arc::new(RwLock::new(false)),
            capabilities: Arc::new(RwLock::new(None)),
            server_info: Arc::new(RwLock::new(None)),
            tools: Arc::new(RwLock::new(Vec::new())),
            resources: Arc::new(RwLock::new(Vec::new())),
            prompts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start(&mut self, config: &MCPServerConfig) -> Result<()> {
        if config.disabled {
            return Err(anyhow::anyhow!("MCP server {} is disabled", config.name));
        }

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        
        for (key, value) in &config.env {
            cmd.env(key, value);
        }
        
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let child = cmd.spawn()
            .with_context(|| format!("Failed to start MCP server: {}", config.command))?;
        
        self.process = Some(child);
        
        self.initialize().await?;
        
        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
        let result = self.send_request::<InitializeResult>(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": MCP_VERSION,
                "capabilities": {
                    "tools": {},
                    "resources": {},
                    "prompts": {}
                },
                "clientInfo": {
                    "name": "zeroclaw",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        ).await?;

        {
            let mut capabilities = self.capabilities.write().await;
            *capabilities = Some(result.capabilities.clone());
        }
        
        {
            let mut server_info = self.server_info.write().await;
            *server_info = Some(result.server_info.clone());
        }

        self.send_notification("notifications/initialized", None).await?;

        {
            let mut initialized = self.initialized.write().await;
            *initialized = true;
        }

        self.refresh_tools().await?;
        self.refresh_resources().await?;
        self.refresh_prompts().await?;

        Ok(())
    }

    async fn get_next_id(&self) -> u64 {
        let mut id = self.request_id.lock().await;
        let next = *id;
        *id += 1;
        next
    }

    async fn send_request<T: for<'de> Deserialize<'de>>(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<T> {
        let id = self.get_next_id().await;
        
        let request = MCPRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let request_str = serde_json::to_string(&request)?;
        
        self.write_to_stdin(&request_str).await?;
        
        let response_str = self.read_from_stdout().await?;
        let response: MCPResponse = serde_json::from_str(&response_str)?;

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!("MCP error {}: {}", error.code, error.message));
        }

        let result = response.result
            .ok_or_else(|| anyhow::anyhow!("No result in MCP response"))?;
        
        let typed_result: T = serde_json::from_value(result)?;
        
        Ok(typed_result)
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        let request = MCPRequest {
            jsonrpc: "2.0".to_string(),
            id: 0,
            method: method.to_string(),
            params,
        };

        let request_str = serde_json::to_string(&request)?;
        self.write_to_stdin(&request_str).await?;
        
        Ok(())
    }

    async fn write_to_stdin(&mut self, data: &str) -> Result<()> {
        if let Some(ref mut child) = self.process {
            let stdin = child.stdin.as_mut()
                .ok_or_else(|| anyhow::anyhow!("stdin not available"))?;
            
            let data_with_newline = format!("{}\n", data);
            use tokio::io::AsyncWriteExt;
            stdin.write_all(data_with_newline.as_bytes()).await?;
            stdin.flush().await?;
        }
        Ok(())
    }

    async fn read_from_stdout(&mut self) -> Result<String> {
        if let Some(ref mut child) = self.process {
            let stdout = child.stdout.as_mut()
                .ok_or_else(|| anyhow::anyhow!("stdout not available"))?;
            
            use tokio::io::AsyncBufReadExt;
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            
            Ok(line.trim().to_string())
        } else {
            Err(anyhow::anyhow!("Process not running"))
        }
    }

    pub async fn refresh_tools(&mut self) -> Result<()> {
        let result = self.send_request::<serde_json::Value>("tools/list", None).await?;
        
        let tools: Vec<ToolDefinition> = result
            .get("tools")
            .and_then(|t| serde_json::from_value(t.clone()).ok())
            .unwrap_or_default();
        
        let mut current_tools = self.tools.write().await;
        *current_tools = tools;
        
        Ok(())
    }

    pub async fn refresh_resources(&mut self) -> Result<()> {
        let result = self.send_request::<serde_json::Value>("resources/list", None).await?;
        
        let resources: Vec<Resource> = result
            .get("resources")
            .and_then(|r| serde_json::from_value(r.clone()).ok())
            .unwrap_or_default();
        
        let mut current_resources = self.resources.write().await;
        *current_resources = resources;
        
        Ok(())
    }

    pub async fn refresh_prompts(&mut self) -> Result<()> {
        let result = self.send_request::<serde_json::Value>("prompts/list", None).await?;
        
        let prompts: Vec<PromptDefinition> = result
            .get("prompts")
            .and_then(|p| serde_json::from_value(p.clone()).ok())
            .unwrap_or_default();
        
        let mut current_prompts = self.prompts.write().await;
        *current_prompts = prompts;
        
        Ok(())
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolCallResult> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments.unwrap_or(serde_json::json!({}))
        });
        
        self.send_request("tools/call", Some(params)).await
    }

    pub async fn read_resource(&mut self, uri: &str) -> Result<ContentBlock> {
        let params = serde_json::json!({
            "uri": uri
        });
        
        let result = self.send_request::<serde_json::Value>("resources/read", Some(params)).await?;
        
        let contents: Vec<ContentBlock> = result
            .get("contents")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_default();
        
        contents.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No content in resource"))
    }

    pub async fn get_prompt(
        &mut self,
        name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>> {
        let params = if let Some(args) = arguments {
            Some(serde_json::json!({
                "name": name,
                "arguments": args
            }))
        } else {
            Some(serde_json::json!({
                "name": name
            }))
        };
        
        let result = self.send_request::<serde_json::Value>("prompts/get", params).await?;
        
        let messages: Vec<PromptMessage> = result
            .get("messages")
            .and_then(|m| serde_json::from_value(m.clone()).ok())
            .unwrap_or_default();
        
        Ok(messages)
    }

    pub async fn get_tools(&self) -> Vec<ToolDefinition> {
        self.tools.read().await.clone()
    }

    pub async fn get_resources(&self) -> Vec<Resource> {
        self.resources.read().await.clone()
    }

    pub async fn get_prompts(&self) -> Vec<PromptDefinition> {
        self.prompts.read().await.clone()
    }

    pub async fn is_initialized(&self) -> bool {
        *self.initialized.read().await
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            child.kill().await?;
        }
        
        let mut initialized = self.initialized.write().await;
        *initialized = false;
        
        Ok(())
    }
}

impl Drop for MCPClient {
    fn drop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.start_kill();
        }
    }
}

pub struct MCPManager {
    clients: Arc<RwLock<HashMap<String, MCPClient>>>,
    configs: Arc<RwLock<HashMap<String, MCPServerConfig>>>,
}

impl MCPManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_server(&self, config: MCPServerConfig) -> Result<()> {
        let mut configs = self.configs.write().await;
        configs.insert(config.name.clone(), config);
        Ok(())
    }

    pub async fn start_server(&self, name: &str) -> Result<()> {
        let configs = self.configs.read().await;
        let config = configs.get(name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", name))?
            .clone();
        drop(configs);

        let mut client = MCPClient::new(name);
        client.start(&config).await?;

        let mut clients = self.clients.write().await;
        clients.insert(name.to_string(), client);

        Ok(())
    }

    pub async fn stop_server(&self, name: &str) -> Result<()> {
        let mut clients = self.clients.write().await;
        if let Some(mut client) = clients.remove(name) {
            client.stop().await?;
        }
        Ok(())
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<ToolCallResult> {
        let mut clients = self.clients.write().await;
        let client = clients.get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not running", server_name))?;
        
        client.call_tool(tool_name, arguments).await
    }

    pub async fn read_resource(
        &self,
        server_name: &str,
        uri: &str,
    ) -> Result<ContentBlock> {
        let mut clients = self.clients.write().await;
        let client = clients.get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not running", server_name))?;
        
        client.read_resource(uri).await
    }

    pub async fn get_prompt(
        &self,
        server_name: &str,
        prompt_name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>> {
        let mut clients = self.clients.write().await;
        let client = clients.get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not running", server_name))?;
        
        client.get_prompt(prompt_name, arguments).await
    }

    pub async fn get_all_tools(&self) -> HashMap<String, Vec<ToolDefinition>> {
        let clients = self.clients.read().await;
        let mut all_tools = HashMap::new();
        
        for (name, client) in clients.iter() {
            all_tools.insert(name.clone(), client.get_tools().await);
        }
        
        all_tools
    }

    pub async fn get_running_servers(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        clients.keys().cloned().collect()
    }

    pub async fn stop_all(&self) -> Result<()> {
        let mut clients = self.clients.write().await;
        for (_, mut client) in clients.drain() {
            let _ = client.stop().await;
        }
        Ok(())
    }
}

impl Default for MCPManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_request_serialization() {
        let request = MCPRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "initialize".to_string(),
            params: Some(serde_json::json!({"protocolVersion": "2024-11-05"})),
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"method\":\"initialize\""));
    }

    #[test]
    fn test_mcp_response_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05"}}"#;
        let response: MCPResponse = serde_json::from_str(json).unwrap();
        
        assert_eq!(response.id, 1);
        assert!(response.result.is_some());
    }

    #[test]
    fn test_tool_definition() {
        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
        };
        
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"name\":\"test_tool\""));
    }

    #[test]
    fn test_content_block() {
        let content = ContentBlock {
            content_type: "text".to_string(),
            text: Some("Hello, world!".to_string()),
            data: None,
            mime_type: None,
            resource: None,
        };
        
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"text\""));
    }

    #[test]
    fn test_mcp_server_config() {
        let config = MCPServerConfig {
            name: "test".to_string(),
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: HashMap::new(),
            disabled: false,
        };
        
        assert_eq!(config.name, "test");
        assert_eq!(config.command, "node");
        assert!(!config.disabled);
    }
}
