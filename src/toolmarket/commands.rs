use anyhow::Result;
use std::sync::Arc;

use crate::config::Config;
use crate::toolmarket::{ToolMarketplaceManager, ToolMarketplaceConfig, ToolSearchOptions, ToolSource, ToolMarketCommands, MCPMarketCommands};
use crate::security::SecurityPolicy;

pub async fn handle_command(command: ToolMarketCommands, config: &Config) -> Result<()> {
    let security = Arc::new(SecurityPolicy::default());
    let toolmarket_config = ToolMarketplaceConfig::default();
    let mut manager = ToolMarketplaceManager::new(
        config.workspace_dir.clone(),
        toolmarket_config,
        security,
    );

    manager.initialize().await?;

    match command {
        ToolMarketCommands::Search { query, source, category, limit } => {
            let source = source.map(|s| {
                match s.to_lowercase().as_str() {
                    "clawhub" => ToolSource::ClawHub,
                    "mcp" => ToolSource::MCP,
                    _ => ToolSource::Local,
                }
            });

            let options = ToolSearchOptions {
                query: Some(query),
                source,
                category,
                tags: None,
                limit: Some(limit),
                offset: Some(0),
            };

            let results = manager.search_tools(options).await?;
            println!("🔍 Found {} tools:", results.len());
            println!();

            for tool in results {
                println!("{} ({})", tool.name, tool.id);
                println!("  Source: {:?}", tool.source);
                if let Some(category) = tool.category {
                    println!("  Category: {}", category);
                }
                if let Some(version) = tool.version {
                    println!("  Version: {}", version);
                }
                if let Some(author) = tool.author {
                    println!("  Author: {}", author);
                }
                println!("  Description: {}", tool.description);
                println!("  Status: {}", if tool.installed { "Installed" } else { "Available" });
                println!();
            }
        }

        ToolMarketCommands::List => {
            let tools = manager.list_installed_tools().await?;
            println!("📦 Installed tools:",);
            println!();

            if tools.is_empty() {
                println!("No tools installed yet.");
                println!();
                return Ok(());
            }

            for tool in tools {
                println!("{} ({})", tool.name, tool.id);
                println!("  Source: {:?}", tool.source);
                if let Some(category) = tool.category {
                    println!("  Category: {}", category);
                }
                if let Some(version) = tool.version {
                    println!("  Version: {}", version);
                }
                if let Some(author) = tool.author {
                    println!("  Author: {}", author);
                }
                println!("  Status: {}", if tool.enabled { "Enabled" } else { "Disabled" });
                if let Some(path) = tool.path {
                    println!("  Path: {}", path);
                }
                println!();
            }
        }

        ToolMarketCommands::Install { tool_id } => {
            println!("📥 Installing tool: {}", tool_id);
            let tool = manager.install_tool(&tool_id).await?;
            println!("✅ Tool installed successfully!");
            println!();
            println!("Name: {}", tool.name);
            println!("ID: {}", tool.id);
            println!("Source: {:?}", tool.source);
            println!("Status: Enabled");
        }

        ToolMarketCommands::Uninstall { tool_id } => {
            println!("📤 Uninstalling tool: {}", tool_id);
            manager.uninstall_tool(&tool_id).await?;
            println!("✅ Tool uninstalled successfully!");
        }

        ToolMarketCommands::Enable { tool_id } => {
            println!("🔄 Enabling tool: {}", tool_id);
            manager.enable_tool(&tool_id).await?;
            println!("✅ Tool enabled successfully!");
        }

        ToolMarketCommands::Disable { tool_id } => {
            println!("🔄 Disabling tool: {}", tool_id);
            manager.disable_tool(&tool_id).await?;
            println!("✅ Tool disabled successfully!");
        }

        ToolMarketCommands::Info { tool_id } => {
            println!("📋 Tool information for: {}", tool_id);
            println!();
            let tool = manager.get_tool_info(&tool_id).await?;
            match tool {
                Some(tool_info) => {
                    println!("Name: {}", tool_info.name);
                    println!("ID: {}", tool_info.id);
                    println!("Source: {:?}", tool_info.source);
                    if let Some(category) = tool_info.category {
                        println!("Category: {}", category);
                    }
                    if let Some(version) = tool_info.version {
                        println!("Version: {}", version);
                    }
                    if let Some(author) = tool_info.author {
                        println!("Author: {}", author);
                    }
                    println!("Description: {}", tool_info.description);
                    println!("Installed: {}", tool_info.installed);
                    println!("Enabled: {}", tool_info.enabled);
                    if let Some(path) = tool_info.path {
                        println!("Path: {}", path);
                    }
                }
                None => {
                    println!("Tool not found: {}", tool_id);
                }
            }
        }

        ToolMarketCommands::Stats => {
            let stats = manager.get_stats().await?;
            println!("📊 Tool Marketplace Stats");
            println!();
            println!("Total tools: {}", stats.total_tools);
            println!("Installed tools: {}", stats.installed_tools);
            println!();
            println!("Tools by source:");
            for (source, count) in stats.by_source {
                println!("  {:?}: {}", source, count);
            }
            println!();
            println!("Tools by category:");
            for (category, count) in stats.by_category {
                println!("  {}: {}", category, count);
            }
            if let Some(last_sync) = stats.last_sync {
                let datetime = chrono::DateTime::from_timestamp(last_sync, 0)
                    .unwrap_or_else(|| chrono::Utc::now());
                println!();
                println!("Last sync: {}", datetime);
            }
        }

        ToolMarketCommands::MCP { mcp_command } => {
            handle_mcp_command(mcp_command, &mut manager).await?;
        }
    }

    Ok(())
}

async fn handle_mcp_command(
    command: MCPMarketCommands,
    manager: &mut ToolMarketplaceManager,
) -> Result<()> {
    match command {
        MCPMarketCommands::List => {
            let servers = manager.get_running_mcp_servers().await?;
            println!("🖥️  Running MCP servers:");
            println!();

            if servers.is_empty() {
                println!("No MCP servers running.");
                println!();
                return Ok(());
            }

            for server in servers {
                println!("- {}", server);
            }
        }

        MCPMarketCommands::Add { name, command, args } => {
            println!("📡 Adding MCP server: {}", name);
            println!("Command: {}", command);
            if !args.is_empty() {
                println!("Arguments: {:?}", args);
            }

            let mcp_config = crate::mcp::MCPServerConfig {
                name,
                command,
                args,
                env: std::collections::HashMap::new(),
                disabled: false,
            };

            manager.add_mcp_server(mcp_config).await?;
            println!("✅ MCP server added successfully!");
        }

        MCPMarketCommands::Start { name } => {
            println!("🚀 Starting MCP server: {}", name);
            manager.start_mcp_server(&name).await?;
            println!("✅ MCP server started successfully!");
        }

        MCPMarketCommands::Stop { name } => {
            println!("🛑 Stopping MCP server: {}", name);
            manager.stop_mcp_server(&name).await?;
            println!("✅ MCP server stopped successfully!");
        }

        MCPMarketCommands::Tools { name } => {
            println!("🔧 Tools from MCP server: {}", name);
            println!();

            // 这里需要实现获取 MCP 服务器工具的逻辑
            // 暂时返回模拟数据
            println!("No tools found or server not running.");
        }
    }

    Ok(())
}
