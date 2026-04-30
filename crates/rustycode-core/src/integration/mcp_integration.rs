//! MCP (Model Context Protocol) integration for Runtime.
//!
//! This module provides integration between the Runtime and MCP servers,
//! enabling automatic tool discovery and execution from external MCP servers.

use anyhow::{Context, Result};
use rustycode_config::{Config, MCPServerConfig};
use rustycode_mcp::{ManagerConfig, McpClient, McpServerManager, ServerConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{error, info, warn};

/// MCP tool information
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    /// Namespaced tool name (`mcp_<server>_<tool>`)
    pub name: String,
    /// Server name
    pub server_name: String,
    /// Actual tool name (without namespace)
    pub tool_name: String,
    /// Tool description
    pub description: String,
    /// Input schema
    pub input_schema: serde_json::Value,
}

/// MCP integration manager
pub struct McpIntegration {
    /// MCP server manager
    manager: Arc<RwLock<McpServerManager>>,
    /// MCP client for making tool calls
    client: Arc<RwLock<McpClient>>,
    /// Server configurations loaded from config
    server_configs: HashMap<String, MCPServerConfig>,
    /// Map of tool names to their server IDs
    tool_servers: HashMap<String, String>,
    /// Discovered MCP tools
    mcp_tools: HashMap<String, McpToolInfo>,
}

impl McpIntegration {
    /// Create a new MCP integration from config
    pub async fn new(config: &Config) -> Result<Self> {
        info!("Initializing MCP integration");

        // Create MCP server manager with default config
        let manager_config = ManagerConfig::default();
        let manager = McpServerManager::new(manager_config);
        let manager = Arc::new(RwLock::new(manager));

        // Create MCP client with default config
        use rustycode_mcp::McpClientConfig;
        let client_config = McpClientConfig::default();
        let client = McpClient::new(client_config);
        let client = Arc::new(RwLock::new(client));

        // Load server configurations from config
        let mut server_configs = HashMap::new();

        // Load from advanced config (detailed configuration)
        for (name, server_config) in &config.advanced.mcp_servers_map {
            if server_config.enabled {
                server_configs.insert(name.clone(), server_config.clone());
                info!(
                    "Loaded MCP server config: {} (command: {})",
                    name,
                    server_config.command.as_deref().unwrap_or("<none>")
                );
            }
        }

        // Legacy support: load from features.mcp_servers (simple list)
        for server_name in &config.features.mcp_servers {
            if !server_configs.contains_key(server_name) {
                // Create a default config for legacy entries
                warn!("Legacy MCP server entry '{}': needs detailed configuration in advanced.mcp_servers_map", server_name);
            }
        }

        info!(
            "MCP integration initialized with {} server configuration(s)",
            server_configs.len()
        );

        Ok(Self {
            manager,
            client,
            server_configs,
            tool_servers: HashMap::new(),
            mcp_tools: HashMap::new(),
        })
    }

    /// Start all configured MCP servers
    pub async fn start_servers(&mut self) -> Result<()> {
        info!("Starting {} MCP server(s)", self.server_configs.len());

        let mut manager = self.manager.write().await;
        let mut client = self.client.write().await;

        for (name, server_config) in &self.server_configs {
            info!("Starting MCP server: {}", name);

            // Convert config to ServerConfig
            let config = ServerConfig {
                server_id: name.clone(),
                name: name.clone(),
                command: server_config.command.clone(),
                args: server_config.args.clone(),
                transport_type: None,
                url: server_config.url.clone(),
                headers: server_config.headers.clone(),
                headers_helper: server_config.headers_helper.clone(),
                description: server_config.description.clone(),
                oauth: None,
                enable_tools: true,
                enable_resources: false,
                enable_prompts: false,
                enabled: true,
                tools_allowlist: vec![],
                tools_denylist: vec![],
                tags: vec![],
            };

            // Start server
            match manager.start_server(config).await {
                Ok(_server) => {
                    info!("MCP server '{}' started successfully", name);

                    // Connect client to server (stdio only for now)
                    if let Some(ref cmd) = server_config.command {
                        let args_str: Vec<&str> =
                            server_config.args.iter().map(|s| s.as_str()).collect();
                        match client.connect_stdio(name.clone(), cmd, &args_str).await {
                            Ok(_) => {
                                info!("Connected to MCP server '{}'", name);

                                // Discover tools from this server
                                match client.list_tools(name).await {
                                    Ok(tools) => {
                                        info!(
                                            "Discovered {} tools from MCP server '{}'",
                                            tools.len(),
                                            name
                                        );

                                        // Store tool information
                                        for tool in tools {
                                            let tool_name = format!("mcp_{}_{}", name, tool.name);

                                            let tool_info = McpToolInfo {
                                                name: tool_name.clone(),
                                                server_name: name.clone(),
                                                tool_name: tool.name.clone(),
                                                description: tool.description.clone(),
                                                input_schema: tool.input_schema.clone(),
                                            };

                                            // Store mapping of tool to server
                                            self.tool_servers
                                                .insert(tool_name.clone(), name.clone());

                                            // Store tool info
                                            self.mcp_tools.insert(tool_name.clone(), tool_info);

                                            info!(
                                                "Registered MCP tool: {} from server '{}'",
                                                tool_name, name
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Failed to discover tools from MCP server '{}': {}",
                                            name, e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to connect to MCP server '{}': {}", name, e);
                            }
                        }
                    } else {
                        info!(
                            "MCP server '{}' is remote (no stdio command), skipping stdio connect",
                            name
                        );
                    }
                }
                Err(e) => {
                    error!("Failed to start MCP server '{}': {}", name, e);
                }
            }
        }

        info!("MCP server startup complete");
        Ok(())
    }

    /// Execute an MCP tool call with retry logic
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String> {
        if !tool_name.starts_with("mcp_") {
            return Err(anyhow::anyhow!("Not an MCP tool: {}", tool_name));
        }

        let server_name = self
            .tool_servers
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown MCP tool: {}", tool_name))?;

        let tool_info = self
            .mcp_tools
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool info not found: {}", tool_name))?;

        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..3 {
            if attempt > 0 {
                let delay = time::Duration::from_millis(500 * 2u64.pow(attempt));
                time::sleep(delay).await;
            }

            let client = self.client.read().await;

            match client
                .call_tool(server_name, &tool_info.tool_name, arguments.clone())
                .await
            {
                Ok(result) => {
                    return serde_json::to_string_pretty(&result)
                        .context("Failed to serialize MCP tool result");
                }
                Err(e) => {
                    let err = anyhow::anyhow!("MCP error: {}", e);
                    warn!("MCP tool call attempt {} failed: {}", attempt + 1, err);
                    last_error = Some(err);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("MCP tool call failed after 3 attempts"))
            .context(format!(
                "Failed to execute MCP tool {} on server {} after retries",
                tool_info.tool_name, server_name
            )))
    }

    /// Check if a tool name is an MCP tool
    pub fn is_mcp_tool(tool_name: &str) -> bool {
        tool_name.starts_with("mcp_")
    }

    /// Get all MCP tools for tool listing
    pub fn get_mcp_tools(&self) -> Vec<McpToolInfo> {
        self.mcp_tools.values().cloned().collect()
    }

    /// Get tool info by name
    pub fn get_tool_info(&self, tool_name: &str) -> Option<&McpToolInfo> {
        self.mcp_tools.get(tool_name)
    }

    /// Get the MCP manager (for advanced usage)
    pub fn manager(&self) -> &Arc<RwLock<McpServerManager>> {
        &self.manager
    }

    /// Get the MCP client (for advanced usage)
    pub fn client(&self) -> &Arc<RwLock<McpClient>> {
        &self.client
    }

    /// Stop all MCP servers
    pub async fn stop_servers(&self) -> Result<()> {
        info!("Stopping all MCP servers");

        let mut manager = self.manager.write().await;
        let mut client = self.client.write().await;

        for server_name in self.server_configs.keys() {
            // Disconnect client
            if let Err(e) = client.disconnect(server_name).await {
                warn!(
                    "Failed to disconnect from MCP server '{}': {}",
                    server_name, e
                );
            }

            // Stop server
            match manager.stop_server(server_name).await {
                Ok(_) => {
                    info!("Stopped MCP server '{}'", server_name);
                }
                Err(e) => {
                    warn!("Failed to stop MCP server '{}': {}", server_name, e);
                }
            }
        }

        info!("All MCP servers stopped");
        Ok(())
    }

    /// Get server health status
    pub async fn get_server_health(&self) -> Vec<(String, String)> {
        let manager = self.manager.read().await;
        let mut health_status = Vec::new();

        for server_name in self.server_configs.keys() {
            let health = manager.health_check(server_name).await;
            let status_str = format!("{:?}", health);
            health_status.push((server_name.clone(), status_str));
        }

        health_status
    }

    /// Reconnect to a failed server
    pub async fn reconnect_server(&self, server_name: &str) -> Result<()> {
        info!("Reconnecting to MCP server '{}'", server_name);

        let mut manager = self.manager.write().await;

        manager
            .restart_server(server_name)
            .await
            .context(format!("Failed to reconnect to server '{}'", server_name))?;

        info!("Successfully reconnected to MCP server '{}'", server_name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_mcp_tool() {
        assert!(McpIntegration::is_mcp_tool("mcp_read_file"));
        assert!(McpIntegration::is_mcp_tool("mcp_github_list_issues"));
        assert!(!McpIntegration::is_mcp_tool("read_file"));
        assert!(!McpIntegration::is_mcp_tool("bash"));
    }
}
