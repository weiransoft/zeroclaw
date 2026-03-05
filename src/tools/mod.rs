pub mod browser;
pub mod browser_open;
pub mod composio;
pub mod delegate;
pub mod file_read;
pub mod file_write;
pub mod git_operations;
pub mod group_chat;
pub mod hardware_board_info;
pub mod hardware_memory_map;
pub mod hardware_memory_read;
pub mod http_request;
pub mod image_info;
pub mod memory_forget;
pub mod memory_recall;
pub mod memory_store;
pub mod schedule;
pub mod sessions_spawn;
pub mod screenshot;
pub mod shell;
pub mod subagents;
pub mod task_manager;
pub mod traits;
pub mod workflow;

// pub use browser::{BrowserTool, ComputerUseConfig};
// pub use browser_open::BrowserOpenTool;
// pub use composio::ComposioTool;
// pub use delegate::DelegateTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
// pub use git_operations::GitOperationsTool;
pub use group_chat::GroupChatTool;
pub use hardware_board_info::HardwareBoardInfoTool;
pub use hardware_memory_map::HardwareMemoryMapTool;
pub use hardware_memory_read::HardwareMemoryReadTool;
pub use http_request::HttpRequestTool;
// pub use image_info::ImageInfoTool;
pub use memory_forget::MemoryForgetTool;
pub use memory_recall::MemoryRecallTool;
pub use memory_store::MemoryStoreTool;
// pub use schedule::ScheduleTool;
// pub use sessions_spawn::SessionsSpawnTool;
// pub use screenshot::ScreenshotTool;
pub use shell::ShellTool;
pub use subagents::SubagentsTool;
pub use task_manager::TaskManagerTool;
pub use traits::Tool;
pub use workflow::WorkflowTool;
#[allow(unused_imports)]
pub use traits::{ToolResult, ToolSpec};

use crate::config::DelegateAgentConfig;
use crate::memory::Memory;
use crate::providers::Provider;
use crate::runtime::RuntimeAdapter;
use crate::security::SecurityPolicy;
use crate::swarm::{SwarmContext, SwarmManager};
use std::sync::Arc;

pub fn default_tools(security: Arc<SecurityPolicy>) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![];
    tools.push(Box::new(FileReadTool::new(security.clone())));
    tools.push(Box::new(FileWriteTool::new(security.clone())));
    tools
}

pub fn all_tools_with_runtime(
    security: &Arc<SecurityPolicy>,
    runtime: Arc<dyn RuntimeAdapter>,
    mem: Arc<dyn Memory>,
    _composio_key: Option<&str>,
    _composio_entity_id: Option<&str>,
    _browser_config: &crate::config::schema::BrowserConfig,
    _http_config: &crate::config::schema::HttpRequestConfig,
    workspace_dir: &std::path::Path,
    _agents: &std::collections::HashMap<String, DelegateAgentConfig>,
    _api_key: Option<&str>,
    config: Arc<crate::config::Config>,
) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![];
    
    // Add existing tools
    tools.push(Box::new(FileReadTool::new(security.clone())));
    tools.push(Box::new(FileWriteTool::new(security.clone())));
    tools.push(Box::new(ShellTool::new(security.clone(), runtime.clone())));
    tools.push(Box::new(HttpRequestTool::new(security.clone(), vec![], 1024 * 1024, 30)));
    tools.push(Box::new(MemoryStoreTool::new(mem.clone())));
    tools.push(Box::new(MemoryRecallTool::new(mem.clone())));
    tools.push(Box::new(MemoryForgetTool::new(mem.clone())));
    
    // Add swarm-related tools
    let swarm_manager = SwarmManager::new(workspace_dir.to_path_buf(), 5);
    let ctx = SwarmContext::root();
    tools.push(Box::new(GroupChatTool::new(security.clone(), config.clone(), ctx.clone())));
    tools.push(Box::new(SubagentsTool::new(security.clone(), config.clone(), swarm_manager.clone(), ctx.clone())));
    tools.push(Box::new(WorkflowTool::new(security.clone(), config.clone(), swarm_manager, ctx.clone())));
    tools.push(Box::new(TaskManagerTool::new(security.clone(), config.clone())));
    
    tools
}

pub fn all_tools_with_runtime_swarm_context(
    security: &Arc<SecurityPolicy>,
    runtime: Arc<dyn RuntimeAdapter>,
    mem: Arc<dyn Memory>,
    _composio_key: Option<&str>,
    _composio_entity_id: Option<&str>,
    _browser_config: &crate::config::schema::BrowserConfig,
    _http_config: &crate::config::schema::HttpRequestConfig,
    workspace_dir: &std::path::Path,
    _agents: &std::collections::HashMap<String, DelegateAgentConfig>,
    _api_key: Option<&str>,
    config: Arc<crate::config::Config>,
    ctx: SwarmContext,
) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![];
    
    // Add existing tools
    tools.push(Box::new(FileReadTool::new(security.clone())));
    tools.push(Box::new(FileWriteTool::new(security.clone())));
    tools.push(Box::new(ShellTool::new(security.clone(), runtime.clone())));
    tools.push(Box::new(HttpRequestTool::new(security.clone(), vec![], 1024 * 1024, 30)));
    tools.push(Box::new(MemoryStoreTool::new(mem.clone())));
    tools.push(Box::new(MemoryRecallTool::new(mem.clone())));
    tools.push(Box::new(MemoryForgetTool::new(mem.clone())));
    
    // Add swarm-related tools
    let swarm_manager = SwarmManager::new(workspace_dir.to_path_buf(), 5);
    tools.push(Box::new(GroupChatTool::new(security.clone(), config.clone(), ctx.clone())));
    tools.push(Box::new(SubagentsTool::new(security.clone(), config.clone(), swarm_manager.clone(), ctx.clone())));
    tools.push(Box::new(WorkflowTool::new(security.clone(), config.clone(), swarm_manager, ctx.clone())));
    tools.push(Box::new(TaskManagerTool::new(security.clone(), config.clone())));
    
    tools
}

pub fn all_tools_with_provider(
    security: &Arc<SecurityPolicy>,
    runtime: Arc<dyn RuntimeAdapter>,
    mem: Arc<dyn Memory>,
    _composio_key: Option<&str>,
    _composio_entity_id: Option<&str>,
    _browser_config: &crate::config::schema::BrowserConfig,
    _http_config: &crate::config::schema::HttpRequestConfig,
    workspace_dir: &std::path::Path,
    _agents: &std::collections::HashMap<String, DelegateAgentConfig>,
    _api_key: Option<&str>,
    config: Arc<crate::config::Config>,
    provider: Arc<dyn Provider>,
) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![];
    
    // Add existing tools
    tools.push(Box::new(FileReadTool::new(security.clone())));
    tools.push(Box::new(FileWriteTool::new(security.clone())));
    tools.push(Box::new(ShellTool::new(security.clone(), runtime.clone())));
    tools.push(Box::new(HttpRequestTool::new(security.clone(), vec![], 1024 * 1024, 30)));
    tools.push(Box::new(MemoryStoreTool::new(mem.clone())));
    tools.push(Box::new(MemoryRecallTool::new(mem.clone())));
    tools.push(Box::new(MemoryForgetTool::new(mem.clone())));
    
    // Add swarm-related tools
    let swarm_manager = SwarmManager::new(workspace_dir.to_path_buf(), 5);
    let ctx = SwarmContext::root();
    tools.push(Box::new(GroupChatTool::new(security.clone(), config.clone(), ctx.clone())));
    tools.push(Box::new(SubagentsTool::new(security.clone(), config.clone(), swarm_manager.clone(), ctx.clone())));
    tools.push(Box::new(
        WorkflowTool::new(security.clone(), config.clone(), swarm_manager, ctx.clone())
            .with_provider(provider)
    ));
    tools.push(Box::new(TaskManagerTool::new(security.clone(), config.clone())));
    
    tools
}