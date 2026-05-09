//! Simplified MCP Stdio Client
//!
//! Connects to MCP servers via stdio subprocess transport.
//! Implements JSON-RPC 2.0 for tool discovery and execution.
//!
//! This is a simplified synchronous client that doesn't require async/await.
//! For the full-featured async client, see [`crate::client::McpClient`].

use crate::types::{McpContent, McpTool, McpToolResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use tracing::{debug, info, warn};

/// MCP server configuration for stdio transport
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransportType {
    Stdio,
    Http,
    Sse,
}

/// OAuth configuration for remote MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthConfig {
    pub client_id: String,
    #[serde(default)]
    pub scopes: Option<String>,
    #[serde(default)]
    pub callback_port: Option<u16>,
}

/// MCP server configuration for stdio transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name/identifier
    pub name: String,

    /// Optional transport type
    #[serde(default, rename = "type", alias = "transport_type")]
    pub transport_type: Option<McpTransportType>,

    /// Command to spawn the MCP server
    #[serde(default)]
    pub command: String,

    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to set for the server process
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Remote server URL
    #[serde(default)]
    pub url: Option<String>,

    /// HTTP headers for remote servers
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,

    /// Path to script that outputs dynamic headers as JSON
    #[serde(default, rename = "headersHelper", alias = "headers_helper")]
    pub headers_helper: Option<String>,

    /// Server description
    #[serde(default)]
    pub description: Option<String>,

    /// OAuth configuration for remote servers
    #[serde(default)]
    pub oauth: Option<McpOAuthConfig>,

    /// Whether the server is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Legacy transport name
    #[serde(default)]
    pub transport: Option<String>,
}

const fn default_enabled() -> bool {
    true
}

impl McpServerConfig {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport_type: None,
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
            url: None,
            headers: None,
            headers_helper: None,
            description: None,
            oauth: None,
            enabled: true,
            transport: None,
        }
    }

    /// Add arguments to the command
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Add environment variables
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Set enabled state
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// JSON-RPC 2.0 request
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Kept for future use
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<u64>,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[allow(dead_code)] // Kept for future use
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// Initialize result from MCP server
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Kept for future use
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: Option<String>,
    #[serde(default)]
    capabilities: serde_json::Value,
    #[serde(rename = "serverInfo")]
    server_info: Option<serde_json::Value>,
}

/// List tools result
#[derive(Debug, Deserialize)]
struct ListToolsResult {
    tools: Vec<McpTool>,
}

/// Errors that can occur during MCP client operations
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpClientError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    JsonSerialization(#[from] serde_json::Error),

    #[error("Process error: {0}")]
    Process(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("RPC error: {code} - {message}")]
    Rpc { code: i64, message: String },

    #[error("Not connected to server")]
    NotConnected,

    #[error("Tool '{0}' not found")]
    ToolNotFound(String),

    #[error("Timeout waiting for response")]
    Timeout,
}

/// Result type for MCP client operations
pub type McpClientResult<T> = Result<T, McpClientError>;

/// Stdio-based MCP client
///
/// This client connects to MCP servers via stdio (standard input/output).
/// It spawns the server as a subprocess and communicates via JSON-RPC 2.0.
///
/// # Example
///
/// ```no_run
/// use rustycode_mcp::stdio_client::{McpStdioClient, McpServerConfig};
///
/// let config = McpServerConfig::new("filesystem", "npx")
///     .with_args(vec!["-y".to_string(), "@anthropic/mcp-server-filesystem".to_string(), "/tmp".to_string()]);
///
/// let mut client = McpStdioClient::new(config);
/// client.connect().unwrap();
///
/// // List available tools
/// let tools = client.tools();
/// println!("Available tools: {:?}", tools);
///
/// // Call a tool
/// let result = client.call_tool("Read", serde_json::json!({
///     "path": "/tmp/test.txt"
/// })).unwrap();
///
/// println!("Result: {:?}", result);
/// ```
pub struct McpStdioClient {
    config: McpServerConfig,
    child: Option<Child>,
    next_id: u64,
    tools: Vec<McpTool>,
}

impl McpStdioClient {
    pub const fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            child: None,
            next_id: 1,
            tools: Vec::new(),
        }
    }

    /// Connect to the MCP server and initialize
    ///
    /// This spawns the server process and performs the MCP initialization handshake.
    pub fn connect(&mut self) -> McpClientResult<()> {
        if !self.config.enabled {
            return Err(McpClientError::Protocol(format!(
                "Server '{}' is disabled",
                self.config.name
            )));
        }

        info!(
            "Connecting to MCP server '{}' via: {} {}",
            self.config.name,
            self.config.command,
            self.config.args.join(" ")
        );

        // Build the command
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args);
        cmd.envs(&self.config.env);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn the process
        let child = cmd.spawn().map_err(|e| {
            McpClientError::Process(format!("Failed to spawn '{}': {}", self.config.command, e))
        })?;

        self.child = Some(child);

        // Send initialize request
        let init_params = serde_json::json!({
            "protocolVersion": crate::MCP_VERSION,
            "capabilities": {
                "roots": {
                    "listChanged": true
                }
            },
            "clientInfo": {
                "name": "rustycode",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        debug!("Sending initialize request to '{}'", self.config.name);
        let response = self.send_request("initialize", Some(init_params))?;

        if let Some(error) = response.error {
            return Err(McpClientError::Rpc {
                code: error.code,
                message: error.message,
            });
        }

        // Send initialized notification
        debug!("Sending initialized notification to '{}'", self.config.name);
        self.send_notification("notifications/initialized", None)?;

        // Discover tools
        self.discover_tools()?;

        info!(
            "Connected to MCP server '{}' with {} tools",
            self.config.name,
            self.tools.len()
        );

        Ok(())
    }

    /// Discover available tools from the server
    pub fn discover_tools(&mut self) -> McpClientResult<()> {
        debug!("Discovering tools from '{}'", self.config.name);

        let response = self.send_request("tools/list", None)?;

        if let Some(error) = response.error {
            return Err(McpClientError::Rpc {
                code: error.code,
                message: error.message,
            });
        }

        if let Some(result) = response.result {
            let list_result: ListToolsResult = serde_json::from_value(result)?;
            self.tools = list_result.tools;
            debug!(
                "Discovered {} tools from '{}'",
                self.tools.len(),
                self.config.name
            );
        }

        Ok(())
    }

    /// Get available tools
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// Find a tool by name
    pub fn find_tool(&self, name: &str) -> Option<&McpTool> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Call a tool on the server
    ///
    pub fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> McpClientResult<McpToolResult> {
        debug!("Calling tool '{}' on server '{}'", name, self.config.name);

        // Check if tool exists
        if !self.tools.iter().any(|t| t.name == name) {
            warn!(
                "Tool '{}' not found in discovered tools from '{}'",
                name, self.config.name
            );
        }

        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let response = self.send_request("tools/call", Some(params))?;

        if let Some(error) = response.error {
            return Ok(McpToolResult {
                content: vec![McpContent::Text {
                    text: format!("Error: {}", error.message),
                }],
                is_error: Some(true),
                structured_content: None,
                meta: None,
            });
        }

        if let Some(result) = response.result {
            let tool_result: McpToolResult = serde_json::from_value(result)?;
            return Ok(tool_result);
        }

        Ok(McpToolResult {
            content: vec![McpContent::Text {
                text: "No result returned".to_string(),
            }],
            is_error: Some(false),
            structured_content: None,
            meta: None,
        })
    }

    /// Check if connected to the server
    pub const fn is_connected(&self) -> bool {
        self.child.is_some()
    }

    /// Get the server name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get the server configuration
    pub const fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Send a JSON-RPC request and wait for the response
    fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpClientResult<JsonRpcResponse> {
        let id = self.next_id;
        self.next_id += 1;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let message = serde_json::to_string(&request)? + "\n";

        // Send request
        if let Some(child) = &mut self.child {
            let stdin = child.stdin.as_mut().ok_or(McpClientError::NotConnected)?;
            stdin.write_all(message.as_bytes())?;
            stdin.flush()?;

            // Read response
            let stdout = child.stdout.as_mut().ok_or(McpClientError::NotConnected)?;
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            reader.read_line(&mut line)?;

            debug!("Response from '{}': {}", self.config.name, line.trim());

            let response: JsonRpcResponse = serde_json::from_str(line.trim())?;
            return Ok(response);
        }

        Err(McpClientError::NotConnected)
    }

    /// Send a JSON-RPC notification (no response expected)
    fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpClientResult<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let message = serde_json::to_string(&notification)? + "\n";

        if let Some(child) = &mut self.child {
            let stdin = child.stdin.as_mut().ok_or(McpClientError::NotConnected)?;
            stdin.write_all(message.as_bytes())?;
            stdin.flush()?;
            return Ok(());
        }

        Err(McpClientError::NotConnected)
    }

    /// Drain stderr from the child process (useful for debugging)
    pub fn drain_stderr(&mut self) -> McpClientResult<String> {
        if let Some(child) = &mut self.child {
            if let Some(stderr) = child.stderr.as_mut() {
                let mut reader = BufReader::new(stderr);
                let mut output = String::new();
                reader.read_to_string(&mut output)?;
                return Ok(output);
            }
        }
        Ok(String::new())
    }
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            debug!(
                "Dropping MCP client '{}', terminating process",
                self.config.name
            );
            if let Err(e) = child.kill() {
                warn!(
                    "Failed to kill MCP client '{}' process: {}",
                    self.config.name, e
                );
            }
            if let Err(e) = child.wait() {
                warn!(
                    "Failed to wait for MCP client '{}' process exit: {}",
                    self.config.name, e
                );
            }
        }
    }
}

/// Manager for multiple MCP clients
///
/// This manager handles connections to multiple MCP servers,
/// allowing you to interact with all of them through a single interface.
///
/// # Example
///
/// ```no_run
/// use rustycode_mcp::stdio_client::{McpClientManager, McpServerConfig};
///
/// let mut manager = McpClientManager::new();
///
/// // Add servers
/// manager.add_server(McpServerConfig::new("filesystem", "npx")
///     .with_args(vec!["-y".to_string(), "@anthropic/mcp-server-filesystem".to_string(), "/tmp".to_string()]))
///     .unwrap();
///
/// // List all tools from all servers
/// let all_tools = manager.all_tools();
/// for (server, tool) in all_tools {
///     println!("{}.{} - {}", server, tool.name, tool.description);
/// }
///
/// // Call a tool on a specific server
/// let result = manager.call_tool("filesystem", "Read", serde_json::json!({
///     "path": "/tmp/test.txt"
/// })).unwrap();
/// ```
pub struct McpClientManager {
    clients: HashMap<String, McpStdioClient>,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    /// Add and connect to a server
    pub fn add_server(&mut self, config: McpServerConfig) -> McpClientResult<()> {
        let name = config.name.clone();
        let mut client = McpStdioClient::new(config);
        client.connect()?;
        self.clients.insert(name, client);
        Ok(())
    }

    /// Get a client by name
    pub fn client(&self, name: &str) -> Option<&McpStdioClient> {
        self.clients.get(name)
    }

    /// Get a mutable client by name
    pub fn client_mut(&mut self, name: &str) -> Option<&mut McpStdioClient> {
        self.clients.get_mut(name)
    }

    /// List all connected server names
    pub fn connected_servers(&self) -> Vec<&str> {
        self.clients.keys().map(String::as_str).collect()
    }

    /// Get all available tools from all servers
    ///
    /// Returns a vector of tuples containing (`server_name`, tool)
    pub fn all_tools(&self) -> Vec<(&str, &McpTool)> {
        let mut tools = Vec::new();
        for (server_name, client) in &self.clients {
            for tool in &client.tools {
                tools.push((server_name.as_str(), tool));
            }
        }
        tools
    }

    /// Find a tool across all servers by name
    ///
    /// Returns the first matching tool found
    pub fn find_tool(&self, name: &str) -> Option<(&str, &McpTool)> {
        for (server_name, client) in &self.clients {
            if let Some(tool) = client.find_tool(name) {
                return Some((server_name.as_str(), tool));
            }
        }
        None
    }

    /// Call a tool on a specific server
    pub fn call_tool(
        &mut self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> McpClientResult<McpToolResult> {
        let client = self
            .clients
            .get_mut(server_name)
            .ok_or_else(|| McpClientError::ToolNotFound(server_name.to_string()))?;

        client.call_tool(tool_name, arguments)
    }

    /// Disconnect and remove a server
    pub fn remove_server(&mut self, name: &str) -> McpClientResult<()> {
        if let Some(client) = self.clients.remove(name) {
            drop(client); // This will call Drop and terminate the process
        }
        Ok(())
    }

    /// Disconnect all servers
    pub fn disconnect_all(&mut self) -> McpClientResult<()> {
        self.clients.clear();
        Ok(())
    }

    /// Call a tool on any server that has it
    ///
    /// This convenience method searches all connected servers for the first
    /// one that has the requested tool and executes it. This is useful when
    /// you don't care which server provides the tool, you just want it to run.
    ///
    pub fn call_tool_any(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> McpClientResult<(String, McpToolResult)> {
        for (server_name, client) in &mut self.clients {
            if client.tools.iter().any(|t| t.name == tool_name) {
                let result = client.call_tool(tool_name, arguments)?;
                return Ok((server_name.clone(), result));
            }
        }
        Err(McpClientError::ToolNotFound(tool_name.to_string()))
    }
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_creation() {
        let config = McpServerConfig::new("test", "node");
        assert_eq!(config.name, "test");
        assert_eq!(config.command, "node");
        assert!(config.args.is_empty());
        assert!(config.enabled);
    }

    #[test]
    fn test_mcp_server_config_with_args() {
        let config = McpServerConfig::new("test", "node")
            .with_args(vec!["server.js".to_string()])
            .with_enabled(false);

        assert_eq!(config.args.len(), 1);
        assert!(!config.enabled);
    }

    #[test]
    fn test_mcp_server_config_deserialization() {
        let json = r#"{
            "name": "test-server",
            "command": "node",
            "args": ["server.js"],
            "env": {"API_KEY": "test"},
            "enabled": true
        }"#;

        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "test-server");
        assert_eq!(config.command, "node");
        assert_eq!(config.args.len(), 1);
        assert_eq!(config.env.get("API_KEY"), Some(&"test".to_string()));
        assert!(config.enabled);
    }

    #[test]
    fn test_mcp_tool_result() {
        let json = r#"{
            "content": [{"type": "text", "text": "file contents"}],
            "isError": false
        }"#;

        let result: McpToolResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.content.len(), 1);
        // is_error can be Some(false) or None due to skip_serializing_if
        assert!(!result.is_error.unwrap_or(false) || result.is_error.is_none());
    }

    #[test]
    fn test_client_manager() {
        let manager = McpClientManager::new();
        assert!(manager.connected_servers().is_empty());
        assert!(manager.all_tools().is_empty());
        assert!(manager.find_tool("nonexistent").is_none());
    }

    #[test]
    fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "test".to_string(),
            params: Some(serde_json::json!({"key": "value"})),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"test\""));
    }

    #[test]
    fn test_json_rpc_response_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"tools": []}
        }"#;

        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some(1));
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_json_rpc_error_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32601,
                "message": "Method not found"
            }
        }"#;

        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601);
        assert_eq!(error.message, "Method not found");
    }

    #[test]
    fn test_mcp_client_error_display() {
        let io_err = McpClientError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "pipe broke",
        ));
        assert!(io_err.to_string().contains("IO error"));

        let json_err =
            McpClientError::JsonSerialization(serde_json::from_str::<i64>("bad").unwrap_err());
        assert!(json_err.to_string().contains("JSON"));

        let process_err = McpClientError::Process("spawn failed".to_string());
        assert!(process_err.to_string().contains("spawn failed"));

        let protocol_err = McpClientError::Protocol("disabled".to_string());
        assert!(protocol_err.to_string().contains("Protocol"));

        let rpc_err = McpClientError::Rpc {
            code: -32601,
            message: "not found".to_string(),
        };
        assert!(rpc_err.to_string().contains("-32601"));
        assert!(rpc_err.to_string().contains("not found"));

        let not_connected = McpClientError::NotConnected;
        assert!(not_connected.to_string().contains("Not connected"));

        let tool_not_found = McpClientError::ToolNotFound("Bash".to_string());
        assert!(tool_not_found.to_string().contains("Bash"));

        let timeout = McpClientError::Timeout;
        assert!(timeout.to_string().contains("Timeout"));
    }

    #[test]
    fn test_mcp_server_config_default_enabled() {
        let json = r#"{"name": "t", "command": "c"}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
    }

    #[test]
    fn test_mcp_server_config_with_env() {
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "VALUE".to_string());
        let config = McpServerConfig::new("test", "cmd").with_env(env);
        assert_eq!(config.env.get("KEY"), Some(&"VALUE".to_string()));
    }

    #[test]
    fn test_mcp_stdio_client_new() {
        let config = McpServerConfig::new("test", "echo");
        let client = McpStdioClient::new(config);
        assert!(!client.is_connected());
        assert_eq!(client.name(), "test");
        assert!(client.tools().is_empty());
    }

    #[test]
    fn test_mcp_stdio_client_find_tool_empty() {
        let config = McpServerConfig::new("test", "echo");
        let client = McpStdioClient::new(config);
        assert!(client.find_tool("anything").is_none());
    }

    #[test]
    fn test_mcp_stdio_client_config_accessor() {
        let config = McpServerConfig::new("test", "echo");
        let client = McpStdioClient::new(config);
        assert_eq!(client.config().name, "test");
        assert_eq!(client.config().command, "echo");
    }

    #[test]
    fn test_client_manager_default() {
        let manager = McpClientManager::default();
        assert!(manager.connected_servers().is_empty());
    }

    #[test]
    fn test_client_manager_get_client_not_found() {
        let mut manager = McpClientManager::new();
        assert!(manager.client("nonexistent").is_none());
        assert!(manager.client_mut("nonexistent").is_none());
    }

    #[test]
    fn test_client_manager_call_tool_not_found() {
        let mut manager = McpClientManager::new();
        let result = manager.call_tool("nonexistent", "tool", serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_client_manager_remove_server_nonexistent() {
        let mut manager = McpClientManager::new();
        let result = manager.remove_server("nonexistent");
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_manager_disconnect_all() {
        let mut manager = McpClientManager::new();
        let result = manager.disconnect_all();
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_manager_call_tool_any_not_found() {
        let mut manager = McpClientManager::new();
        let result = manager.call_tool_any("tool", serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_mcp_server_config_disabled() {
        let config = McpServerConfig::new("test", "echo").with_enabled(false);
        let mut client = McpStdioClient::new(config);
        let result = client.connect();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[test]
    fn test_json_rpc_response_null_id() {
        let json = r#"{"jsonrpc": "2.0", "id": null, "result": {}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.id.is_none());
        assert!(response.result.is_some());
    }
}
