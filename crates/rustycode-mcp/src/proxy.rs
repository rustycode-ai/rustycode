//! Tool proxy for delegating calls to MCP servers

use crate::allowlist::{AllowlistEntry, AllowlistManager, AllowlistStatus};
use crate::client::McpClient;
use crate::types::{McpTool, McpToolResult};
use crate::McpResult;
use rustycode_shared_runtime;
use rustycode_thread_guard;
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Proxy configuration
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Server name
    pub server_name: String,
    /// Command to spawn
    pub command: String,
    /// Arguments for command
    pub args: Vec<String>,
    /// Optional prefix for tool names
    pub tool_prefix: Option<String>,
    /// Cache tool definitions
    pub cache_tools: bool,
}

/// Tool proxy for MCP servers
pub struct ToolProxy {
    config: ProxyConfig,
    client: Arc<RwLock<McpClient>>,
    tool_cache: Arc<RwLock<HashMap<String, McpTool>>>,
    /// Allowlist manager for auto-approval
    allowlist: Arc<RwLock<AllowlistManager>>,
}

impl ToolProxy {
    pub async fn new(config: ProxyConfig) -> McpResult<Self> {
        // Connect to the server
        let args: Vec<&str> = config.args.iter().map(String::as_str).collect();
        let mut client = McpClient::default_config();
        client
            .connect_stdio(&config.server_name, &config.command, &args)
            .await?;

        Ok(Self {
            config,
            client: Arc::new(RwLock::new(client)),
            tool_cache: Arc::new(RwLock::new(HashMap::new())),
            allowlist: Arc::new(RwLock::new(AllowlistManager::new().unwrap_or_else(|e| {
                tracing::warn!("Failed to load allowlist, using empty: {}", e);
                AllowlistManager::with_file_path(std::path::PathBuf::from("/dev/null"))
                    .unwrap_or_else(|_| AllowlistManager::empty())
            }))),
        })
    }

    /// Create a proxy and discover tools
    pub async fn with_discovery(config: ProxyConfig) -> McpResult<Self> {
        let proxy = Self::new(config).await?;
        proxy.refresh_tools().await?;
        Ok(proxy)
    }

    /// Create a proxy with a custom allowlist manager
    pub async fn with_allowlist(
        config: ProxyConfig,
        allowlist: AllowlistManager,
    ) -> McpResult<Self> {
        let proxy = Self::new(config).await?;
        *proxy.allowlist.write().await = allowlist;
        Ok(proxy)
    }

    /// Refresh tool cache from server
    pub async fn refresh_tools(&self) -> McpResult<()> {
        let client = self.client.read().await;
        let tools = client.list_tools(&self.config.server_name).await?;

        let mut cache = self.tool_cache.write().await;
        cache.clear();

        for tool in tools {
            let name = if let Some(prefix) = &self.config.tool_prefix {
                format!("{}{}", prefix, tool.name)
            } else {
                tool.name.clone()
            };

            cache.insert(name, tool);
        }

        Ok(())
    }

    /// Get cached tools
    pub async fn tools(&self) -> Vec<ProxiedTool> {
        let cache = self.tool_cache.read().await;

        cache
            .iter()
            .map(|(name, tool)| ProxiedTool {
                name: name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                proxy: self.clone(),
            })
            .collect()
    }

    /// Execute a tool call
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<McpToolResult> {
        // Strip prefix if present
        let original_name = if let Some(prefix) = &self.config.tool_prefix {
            tool_name.strip_prefix(prefix).unwrap_or(tool_name)
        } else {
            tool_name
        };

        let client = self.client.read().await;
        client
            .call_tool(&self.config.server_name, original_name, arguments)
            .await
    }

    /// Check if the proxy is connected to the server
    pub async fn is_connected(&self) -> bool {
        let client = self.client.read().await;
        client.is_connected(&self.config.server_name).await
    }

    /// Disconnect this proxy from its MCP server.
    ///
    /// Tool invocations are still stateless, but the underlying MCP client
    /// connection can live for the whole session and should be closed
    /// explicitly when the session ends.
    pub async fn disconnect(&self) -> McpResult<()> {
        let mut client = self.client.write().await;
        client.disconnect(&self.config.server_name).await
    }

    /// List resources from the server
    pub async fn list_resources(
        &self,
        server_id: &str,
    ) -> McpResult<Vec<crate::types::McpResource>> {
        let client = self.client.read().await;
        client.list_resources(server_id).await
    }

    /// Check if a tool call requires confirmation
    pub async fn requires_confirmation(&self, tool_name: &str) -> bool {
        let allowlist = self.allowlist.read().await;
        let server_name = &self.config.server_name;

        // Strip prefix if present for allowlist check
        let actual_tool_name = if let Some(prefix) = &self.config.tool_prefix {
            tool_name.strip_prefix(prefix).unwrap_or(tool_name)
        } else {
            tool_name
        };

        allowlist
            .check_allowlist_status(server_name, actual_tool_name)
            .requires_confirmation()
    }

    /// Get the allowlist status for a tool
    pub async fn allowlist_status(&self, tool_name: &str) -> AllowlistStatus {
        let allowlist = self.allowlist.read().await;
        let server_name = &self.config.server_name;

        // Strip prefix if present for allowlist check
        let actual_tool_name = if let Some(prefix) = &self.config.tool_prefix {
            tool_name.strip_prefix(prefix).unwrap_or(tool_name)
        } else {
            tool_name
        };

        allowlist.check_allowlist_status(server_name, actual_tool_name)
    }

    /// Allow a tool for this session only
    pub async fn allow_session(&self, tool_name: Option<&str>) {
        let mut allowlist = self.allowlist.write().await;
        let server_name = &self.config.server_name;

        if let Some(tool) = tool_name {
            allowlist.add_session(AllowlistEntry::Tool {
                server: server_name.clone(),
                tool: tool.to_string(),
            });
        } else {
            // Allow all tools from this server
            allowlist.add_session(AllowlistEntry::Server(server_name.clone()));
        }
    }

    /// Allow a tool persistently (across sessions)
    pub async fn allow_persistent(&self, tool_name: Option<&str>) -> Result<(), std::io::Error> {
        let mut allowlist = self.allowlist.write().await;
        let server_name = &self.config.server_name;

        if let Some(tool) = tool_name {
            allowlist.add_persistent(AllowlistEntry::Tool {
                server: server_name.clone(),
                tool: tool.to_string(),
            })?;
        } else {
            // Allow all tools from this server
            allowlist.add_persistent(AllowlistEntry::Server(server_name.clone()))?;
        }
        Ok(())
    }

    /// Remove a tool from the allowlist
    pub async fn remove_from_allowlist(
        &self,
        tool_name: Option<&str>,
    ) -> Result<(), std::io::Error> {
        let mut allowlist = self.allowlist.write().await;
        let server_name = &self.config.server_name;

        if let Some(tool) = tool_name {
            allowlist.remove(&AllowlistEntry::Tool {
                server: server_name.clone(),
                tool: tool.to_string(),
            })?;
        } else {
            allowlist.clear_server(server_name)?;
        }
        Ok(())
    }

    /// Get the allowlist manager (for advanced usage)
    pub const fn allowlist(&self) -> &Arc<RwLock<AllowlistManager>> {
        &self.allowlist
    }
}

impl Clone for ToolProxy {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            client: self.client.clone(),
            tool_cache: self.tool_cache.clone(),
            allowlist: self.allowlist.clone(),
        }
    }
}

/// A proxied tool from an MCP server
#[derive(Clone)]
pub struct ProxiedTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    proxy: ToolProxy,
}

impl Tool for ProxiedTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn permission(&self) -> ToolPermission {
        // Proxy tools default to execute permission
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        // This is a synchronous wrapper around async call. Prefer using the
        // process-wide shared runtime when no current runtime is available to
        // avoid creating short-lived runtimes in production.
        let proxy = self.proxy.clone();
        let tool_name = self.name.clone();

        // Defensive check: calling network/proxy operations from the terminal
        // thread is not allowed. Panic in debug to catch regressions.
        if rustycode_thread_guard::is_terminal_thread() {
            rustycode_thread_guard::assert_not_terminal_thread(
                "mcp::ToolProxy::execute (proxy call)",
            );
        }

        let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // Check if we're already on the runtime - block_on would deadlock
            if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread {
                // Single-threaded runtime or already on the right thread: use shared runtime
                rustycode_shared_runtime::block_on_shared_send(async move {
                    proxy.call_tool(&tool_name, params).await
                })
                .map_err(|e| anyhow::anyhow!("Tool call failed: {e}"))?
            } else {
                // Multi-threaded runtime: use block_in_place to avoid deadlock
                tokio::task::block_in_place(|| handle.block_on(proxy.call_tool(&tool_name, params)))
                    .map_err(|e| anyhow::anyhow!("Tool call failed: {e}"))?
            }
        } else {
            // Use the shared runtime to block on the async call. Move owned
            // copies into the async block so the future is 'static and
            // therefore Send.
            rustycode_shared_runtime::block_on_shared_send(async move {
                proxy.call_tool(&tool_name, params).await
            })
            .map_err(|e| anyhow::anyhow!("Tool call failed: {e}"))?
        };

        // Convert MCP content to tool output
        let text = result
            .content
            .iter()
            .filter_map(|c| match c {
                crate::types::McpContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolOutput::text(text))
    }
}

/// Multi-server proxy manager
pub struct ProxyManager {
    proxies: Arc<RwLock<HashMap<String, ToolProxy>>>,
}

impl ProxyManager {
    pub fn new() -> Self {
        Self {
            proxies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a proxy configuration
    pub async fn add_proxy(&self, config: ProxyConfig) -> McpResult<()> {
        let proxy = ToolProxy::with_discovery(config).await?;
        let mut proxies = self.proxies.write().await;
        let server_name = proxy.config.server_name.clone();
        proxies.insert(server_name, proxy);
        Ok(())
    }

    /// Get all tools from all proxies
    pub async fn all_tools(&self) -> Vec<ProxiedTool> {
        let proxies = self.proxies.read().await;
        let mut all_tools = Vec::new();

        for proxy in proxies.values() {
            all_tools.extend(proxy.tools().await);
        }

        all_tools
    }

    /// Remove a proxy
    pub async fn remove_proxy(&self, server_name: &str) -> McpResult<()> {
        let mut proxies = self.proxies.write().await;
        proxies.remove(server_name);
        Ok(())
    }
}

impl Default for ProxyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config() {
        let config = ProxyConfig {
            server_name: "test".to_string(),
            command: "/bin/echo".to_string(),
            args: vec!["hello".to_string()],
            tool_prefix: Some("mcp_".to_string()),
            cache_tools: true,
        };

        assert_eq!(config.server_name, "test");
        assert!(config.tool_prefix.is_some());
    }

    #[tokio::test]
    async fn test_proxy_manager() {
        let manager = ProxyManager::new();
        let tools = manager.all_tools().await;
        assert_eq!(tools.len(), 0);
    }

    #[test]
    fn test_proxy_config_no_prefix() {
        let config = ProxyConfig {
            server_name: "server1".to_string(),
            command: "/usr/bin/cat".to_string(),
            args: vec![],
            tool_prefix: None,
            cache_tools: false,
        };
        assert!(config.tool_prefix.is_none());
        assert!(!config.cache_tools);
    }

    #[tokio::test]
    async fn test_proxy_manager_get_all_tools_empty() {
        let manager = ProxyManager::new();
        let tools = manager.all_tools().await;
        assert!(tools.is_empty());
    }

    #[test]
    fn test_proxy_config_with_args() {
        let config = ProxyConfig {
            server_name: "test".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@mcp/server".to_string()],
            tool_prefix: Some("test_".to_string()),
            cache_tools: true,
        };
        assert_eq!(config.args.len(), 2);
    }
}
