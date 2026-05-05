//! MCP client implementation

use crate::protocol::{JsonRpcRequest, McpEvent};
use crate::transport::Transport;
use crate::types::*;
use crate::StdioTransport;
use crate::{McpError, McpResult};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

/// Wrapper for `tools/list` response (spec returns `{"tools": []}`)
#[derive(Debug, Deserialize)]
struct ListToolsResult {
    tools: Vec<McpTool>,
}

/// Wrapper for `resources/list` response (spec returns `{"resources": []}`)
#[derive(Debug, Deserialize)]
struct ListResourcesResult {
    resources: Vec<McpResource>,
}

/// Wrapper for `prompts/list` response (spec returns `{"prompts": []}`)
#[derive(Debug, Deserialize)]
struct ListPromptsResult {
    prompts: Vec<McpPrompt>,
}

/// Wrapper for `prompts/get` response (spec returns `{"messages": []}`)
#[derive(Debug, Deserialize)]
struct GetPromptResult {
    messages: Vec<McpPromptMessage>,
}

/// MCP client configuration
#[derive(Debug, Clone)]
pub struct McpClientConfig {
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum number of concurrent requests
    pub max_concurrent_requests: usize,
    /// Enable progress notifications
    pub enable_progress: bool,
    /// Client name for identification
    pub client_name: String,
    /// Client version
    pub client_version: String,
}

impl Default for McpClientConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_concurrent_requests: 10,
            enable_progress: true,
            client_name: "rustycode-mcp-client".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

pub struct McpClient {
    pub config: McpClientConfig,
    transports: Arc<RwLock<HashMap<String, Box<dyn Transport>>>>,
    server_capabilities: Arc<RwLock<HashMap<String, McpServerCapabilities>>>,
    #[allow(dead_code)]
    event_sender: broadcast::Sender<McpEvent>,
}

impl McpClient {
    pub fn new(config: McpClientConfig) -> Self {
        let (event_sender, _) = broadcast::channel(1024);
        Self {
            config,
            transports: Arc::new(RwLock::new(HashMap::new())),
            server_capabilities: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
        }
    }

    /// Process incoming notification and dispatch event
    #[allow(dead_code)]
    fn handle_notification(&self, server_id: &str, method: &str, params: Option<Value>) {
        let event = match method {
            "notifications/tools/list_changed" => McpEvent::ToolsListChanged {
                server_id: server_id.to_string(),
            },
            "notifications/progress" => {
                // Parse progress params
                let progress = params
                    .as_ref()
                    .and_then(|p| p.get("progress").and_then(Value::as_f64))
                    .unwrap_or(0.0);
                McpEvent::ProgressNotification {
                    progress,
                    message: params.and_then(|p| {
                        p.get("message")
                            .and_then(|v| v.as_str())
                            .map(ToString::to_string)
                    }),
                }
            }
            "notifications/message" => {
                let message = params
                    .as_ref()
                    .and_then(|p| p.get("message").and_then(|v| v.as_str()))
                    .unwrap_or_default();
                McpEvent::LogNotification {
                    level: "info".to_string(),
                    message: message.to_string(),
                }
            }
            _ => McpEvent::UnknownNotification {
                method: method.to_string(),
                params,
            },
        };

        if let Err(e) = self.event_sender.send(event) {
            warn!("Failed to dispatch MCP event: {}", e);
        }
    }

    /// Create a client with default configuration
    pub fn default_config() -> Self {
        Self::new(McpClientConfig::default())
    }

    /// Connect to an MCP server via stdio
    pub async fn connect_stdio(
        &mut self,
        server_name: impl Into<String>,
        command: &str,
        args: &[&str],
    ) -> McpResult<()> {
        let server_name = server_name.into();
        info!("Connecting to MCP server '{}' via stdio", server_name);

        let transport = StdioTransport::spawn(command, args)?;
        self.connect_with_transport(server_name, Box::new(transport))
            .await
    }

    /// Connect with a pre-constructed transport
    pub async fn connect_with_transport(
        &mut self,
        server_name: impl Into<String>,
        mut transport: Box<dyn Transport>,
    ) -> McpResult<()> {
        let server_name = server_name.into();
        info!(
            "Connecting to MCP server '{}' via custom transport",
            server_name
        );
        // Initialize the connection
        Self::initialize_connection_static(transport.as_mut(), &server_name, &self.config).await?;

        // Store the transport
        let mut transports = self.transports.write().await;
        transports.insert(server_name.clone(), transport);

        Ok(())
    }

    /// Initialize an MCP connection (static method to avoid borrow issues)
    async fn initialize_connection_static(
        transport: &mut dyn Transport,
        server_name: &str,
        config: &McpClientConfig,
    ) -> McpResult<()> {
        debug!("Initializing connection to '{}'", server_name);

        let init_req = JsonRpcRequest::new("init-1", "initialize").with_params(json!({
            "protocolVersion": crate::MCP_VERSION,
            "capabilities": {
                "roots": {
                    "listChanged": true
                }
            },
            "clientInfo": {
                "name": config.client_name,
                "version": config.client_version
            }
        }));

        let response = transport
            .send_request(init_req)
            .await
            .map_err(|e| McpError::ProtocolError(format!("Initialize failed: {e}")))?;

        if !response.is_success() {
            return Err(McpError::ProtocolError(format!(
                "Initialize failed: {:?}",
                response.error
            )));
        }

        // Extract server capabilities
        if let Some(result) = response.result {
            let init_response: InitializeResponse =
                serde_json::from_value(result).map_err(|e| {
                    McpError::ProtocolError(format!("Invalid initialize response: {e}"))
                })?;

            debug!(
                "Connected to server {} v{}",
                init_response.server_info.name, init_response.server_info.version
            );
        }

        // Send initialized notification
        transport
            .send_notification(
                crate::protocol::JsonRpcNotification::new("notifications/initialized")
                    .with_params(json!({})),
            )
            .await?;

        Ok(())
    }

    /// List available tools from a server
    pub async fn list_tools(&self, server_name: &str) -> McpResult<Vec<McpTool>> {
        debug!("Listing tools from server '{}'", server_name);

        let mut transports = self.transports.write().await;
        let transport = transports.get_mut(server_name).ok_or_else(|| {
            McpError::InvalidRequest(format!("Server '{server_name}' not connected"))
        })?;

        let req = JsonRpcRequest::new("tools-list-1", "tools/list");

        let response = transport
            .send_request(req)
            .await
            .map_err(|e| McpError::ProtocolError(format!("List tools failed: {e}")))?;

        if !response.is_success() {
            return Err(McpError::ProtocolError(format!(
                "List tools failed: {:?}",
                response.error
            )));
        }

        if let Some(result) = response.result {
            let list_result: ListToolsResult = serde_json::from_value(result)
                .map_err(|e| McpError::ProtocolError(format!("Invalid tools response: {e}")))?;

            Ok(list_result.tools)
        } else {
            Ok(Vec::new())
        }
    }

    /// Call a tool on a server
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> McpResult<McpToolResult> {
        debug!("Calling tool '{}' on server '{}'", tool_name, server_name);

        let mut transports = self.transports.write().await;
        let transport = transports.get_mut(server_name).ok_or_else(|| {
            McpError::InvalidRequest(format!("Server '{server_name}' not connected"))
        })?;

        let req = JsonRpcRequest::new("tool-call-1", "tools/call").with_params(json!({
            "name": tool_name,
            "arguments": arguments
        }));

        let response = transport
            .send_request(req)
            .await
            .map_err(|e| McpError::ProtocolError(format!("Tool call failed: {e}")))?;

        if !response.is_success() {
            return Err(McpError::ProtocolError(format!(
                "Tool call failed: {:?}",
                response.error
            )));
        }

        if let Some(result) = response.result {
            let tool_result: McpToolResult = serde_json::from_value(result)
                .map_err(|e| McpError::ProtocolError(format!("Invalid tool result: {e}")))?;

            Ok(tool_result)
        } else {
            Err(McpError::ProtocolError(
                "Tool call returned empty result".to_string(),
            ))
        }
    }

    /// List available resources from a server
    pub async fn list_resources(&self, server_name: &str) -> McpResult<Vec<McpResource>> {
        debug!("Listing resources from server '{}'", server_name);

        let mut transports = self.transports.write().await;
        let transport = transports.get_mut(server_name).ok_or_else(|| {
            McpError::InvalidRequest(format!("Server '{server_name}' not connected"))
        })?;

        let req = JsonRpcRequest::new("resources-list-1", "resources/list");

        let response = transport
            .send_request(req)
            .await
            .map_err(|e| McpError::ProtocolError(format!("List resources failed: {e}")))?;

        if !response.is_success() {
            return Err(McpError::ProtocolError(format!(
                "List resources failed: {:?}",
                response.error
            )));
        }

        if let Some(result) = response.result {
            let list_result: ListResourcesResult = serde_json::from_value(result)
                .map_err(|e| McpError::ProtocolError(format!("Invalid resources response: {e}")))?;

            Ok(list_result.resources)
        } else {
            Ok(Vec::new())
        }
    }

    /// Read a resource from a server
    pub async fn read_resource(
        &self,
        server_name: &str,
        uri: &str,
    ) -> McpResult<McpResourceContents> {
        debug!("Reading resource '{}' from server '{}'", uri, server_name);

        let mut transports = self.transports.write().await;
        let transport = transports.get_mut(server_name).ok_or_else(|| {
            McpError::InvalidRequest(format!("Server '{server_name}' not connected"))
        })?;

        let req = JsonRpcRequest::new("resource-read-1", "resources/read").with_params(json!({
            "uri": uri
        }));

        let response = transport
            .send_request(req)
            .await
            .map_err(|e| McpError::ProtocolError(format!("Read resource failed: {e}")))?;

        if !response.is_success() {
            return Err(McpError::ProtocolError(format!(
                "Read resource failed: {:?}",
                response.error
            )));
        }

        if let Some(result) = response.result {
            let contents: McpResourceContents = serde_json::from_value(result)
                .map_err(|e| McpError::ProtocolError(format!("Invalid resource contents: {e}")))?;

            Ok(contents)
        } else {
            Err(McpError::ProtocolError(
                "Read resource returned empty result".to_string(),
            ))
        }
    }

    /// List available prompts from a server
    pub async fn list_prompts(&self, server_name: &str) -> McpResult<Vec<McpPrompt>> {
        debug!("Listing prompts from server '{}'", server_name);

        let mut transports = self.transports.write().await;
        let transport = transports.get_mut(server_name).ok_or_else(|| {
            McpError::InvalidRequest(format!("Server '{server_name}' not connected"))
        })?;

        let req = JsonRpcRequest::new("prompts-list-1", "prompts/list");

        let response = transport
            .send_request(req)
            .await
            .map_err(|e| McpError::ProtocolError(format!("List prompts failed: {e}")))?;

        if !response.is_success() {
            return Err(McpError::ProtocolError(format!(
                "List prompts failed: {:?}",
                response.error
            )));
        }

        if let Some(result) = response.result {
            let list_result: ListPromptsResult = serde_json::from_value(result)
                .map_err(|e| McpError::ProtocolError(format!("Invalid prompts response: {e}")))?;

            Ok(list_result.prompts)
        } else {
            Ok(Vec::new())
        }
    }

    /// Get a prompt from a server
    pub async fn prompt(
        &self,
        server_name: &str,
        prompt_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> McpResult<Vec<McpPromptMessage>> {
        debug!(
            "Getting prompt '{}' from server '{}'",
            prompt_name, server_name
        );

        let mut transports = self.transports.write().await;
        let transport = transports.get_mut(server_name).ok_or_else(|| {
            McpError::InvalidRequest(format!("Server '{server_name}' not connected"))
        })?;

        let mut params = json!({ "name": prompt_name });
        if let Some(args) = arguments {
            params["arguments"] = args;
        }

        let req = JsonRpcRequest::new("prompt-get-1", "prompts/get").with_params(params);

        let response = transport
            .send_request(req)
            .await
            .map_err(|e| McpError::ProtocolError(format!("Get prompt failed: {e}")))?;

        if !response.is_success() {
            return Err(McpError::ProtocolError(format!(
                "Get prompt failed: {:?}",
                response.error
            )));
        }

        if let Some(result) = response.result {
            let prompt_result: GetPromptResult = serde_json::from_value(result)
                .map_err(|e| McpError::ProtocolError(format!("Invalid prompt response: {e}")))?;

            Ok(prompt_result.messages)
        } else {
            Err(McpError::ProtocolError(
                "Get prompt returned empty result".to_string(),
            ))
        }
    }

    /// Disconnect from a server
    pub async fn disconnect(&mut self, server_name: &str) -> McpResult<()> {
        info!("Disconnecting from server '{}'", server_name);

        let mut transports = self.transports.write().await;
        if let Some(mut transport) = transports.remove(server_name) {
            transport.close().await?;
        }

        let mut caps = self.server_capabilities.write().await;
        caps.remove(server_name);

        Ok(())
    }

    /// Check if connected to a server
    pub async fn is_connected(&self, server_name: &str) -> bool {
        let transports = self.transports.read().await;
        transports.get(server_name).is_some()
    }

    /// Get server capabilities
    pub async fn capabilities(&self, server_name: &str) -> Option<McpServerCapabilities> {
        let caps = self.server_capabilities.read().await;
        caps.get(server_name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = McpClientConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_concurrent_requests, 10);
        assert!(config.enable_progress);
        assert_eq!(config.client_name, "rustycode-mcp-client");
    }

    #[tokio::test]
    async fn test_client_creation() {
        let client = McpClient::default_config();
        assert!(!client.is_connected("test").await);
    }

    #[test]
    fn test_client_config_custom() {
        let config = McpClientConfig {
            timeout_secs: 60,
            max_concurrent_requests: 5,
            enable_progress: false,
            client_name: "custom".to_string(),
            client_version: "2.0".to_string(),
        };
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.max_concurrent_requests, 5);
        assert!(!config.enable_progress);
    }

    #[tokio::test]
    async fn test_client_not_connected_operations() {
        let client = McpClient::default_config();

        // All operations should fail gracefully when not connected
        assert!(client.list_tools("nonexistent").await.is_err());
        assert!(client.list_resources("nonexistent").await.is_err());
        assert!(client.list_prompts("nonexistent").await.is_err());
        assert!(client
            .read_resource("nonexistent", "file:///test")
            .await
            .is_err());
        assert!(client
            .prompt("nonexistent", "test", None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_client_disconnect_not_connected() {
        let mut client = McpClient::default_config();
        // Disconnecting from a non-existent server should not panic
        assert!(client.disconnect("nonexistent").await.is_ok());
    }

    #[tokio::test]
    async fn test_client_multiple_servers_not_connected() {
        let client = McpClient::default_config();
        assert!(!client.is_connected("server1").await);
        assert!(!client.is_connected("server2").await);
        assert!(!client.is_connected("server3").await);
    }

    #[test]
    fn test_client_config_timeout_override() {
        let config = McpClientConfig {
            timeout_secs: 120,
            ..Default::default()
        };
        let client = McpClient::new(config.clone());
        assert_eq!(client.config.timeout_secs, 120);
    }

    #[tokio::test]
    async fn test_client_call_tool_not_connected() {
        let client = McpClient::default_config();
        let result = client.call_tool("nonexistent", "tool", json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_get_capabilities_none() {
        let client = McpClient::default_config();
        assert!(client.capabilities("nonexistent").await.is_none());
    }
}
