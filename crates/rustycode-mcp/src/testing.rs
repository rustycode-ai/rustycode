//! Testing utilities for MCP implementation

use crate::server::McpServer;
use crate::types::*;
use crate::{McpError, McpResult};
use rustycode_tools::ToolExecutor;
use serde_json::json;
use std::path::PathBuf;
use tokio::task::JoinHandle;

/// Test context for MCP integration tests
pub struct McpTestContext {
    pub server_handle: Option<JoinHandle<()>>,
    pub server_name: String,
    pub test_dir: PathBuf,
}

impl McpTestContext {
    /// Create a new test context
    pub async fn new() -> McpResult<Self> {
        // Use system temp directory
        let test_dir = std::env::temp_dir();
        Ok(Self {
            server_handle: None,
            server_name: "test-server".to_string(),
            test_dir,
        })
    }

    /// Spawn a test server
    pub async fn spawn_server(&mut self, _server: McpServer) -> McpResult<()> {
        // For now, we'll skip actual server spawning in tests
        // In real tests, you'd spawn a subprocess and communicate via stdio
        Ok(())
    }

    /// Create a test server instance
    pub fn create_test_server(&self) -> McpServer {
        let executor = ToolExecutor::from_cwd(self.test_dir.clone());
        let mut server = McpServer::default_config("test-server");
        server.register_tool_executor(executor);
        server
    }
}

/// Mock MCP server for testing
pub struct MockMcpServer {
    tools: Vec<McpTool>,
    resources: Vec<McpResource>,
}

impl MockMcpServer {
    /// Create a new mock server
    pub const fn new() -> Self {
        Self {
            tools: Vec::new(),
            resources: Vec::new(),
        }
    }

    /// Add a tool
    pub fn add_tool(mut self, tool: McpTool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Add a resource
    pub fn add_resource(mut self, resource: McpResource) -> Self {
        self.resources.push(resource);
        self
    }

    /// Handle a mock request
    pub fn handle_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        match method {
            "initialize" => Ok(json!({
                "protocolVersion": crate::MCP_VERSION,
                "capabilities": {
                    "tools": {},
                    "resources": {},
                    "prompts": {}
                },
                "serverInfo": {
                    "name": "mock-server",
                    "version": "0.1.0"
                }
            })),
            "tools/list" => Ok(json!({ "tools": self.tools })),
            "resources/list" => Ok(json!({ "resources": self.resources })),
            "tools/call" => {
                let params_value = params.ok_or_else(|| {
                    McpError::InvalidRequest("Tool call requires params".to_string())
                })?;

                let name = params_value
                    .get("name")
                    .and_then(|n| n.as_str())
                    .ok_or_else(|| McpError::InvalidRequest("Tool name required".to_string()))?;

                // Return mock result
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Mock result from tool '{}'", name)
                    }]
                }))
            }
            "ping" => Ok(json!({})),
            _ => Err(McpError::MethodNotFound(method.to_string())),
        }
    }
}

impl Default for MockMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Test helpers
pub mod helpers {
    use super::*;

    /// Create a test tool
    pub fn create_test_tool(name: &str, description: &str) -> McpTool {
        McpTool {
            name: name.to_string(),
            title: None,
            description: description.to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "test": {"type": "string"}
                }
            }),
            output_schema: None,
            annotations: None,
            category: Some("test".to_string()),
        }
    }

    /// Create a test resource
    pub fn create_test_resource(uri: &str, name: &str) -> McpResource {
        McpResource {
            uri: uri.to_string(),
            title: None,
            name: name.to_string(),
            description: format!("Test resource: {name}"),
            mime_type: Some("text/plain".to_string()),
            annotations: None,
            size: None,
        }
    }

    /// Create test prompt
    pub fn create_test_prompt(name: &str, description: &str) -> McpPrompt {
        McpPrompt {
            name: name.to_string(),
            description: description.to_string(),
            arguments: None,
            title: None,
            annotations: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server() {
        let server = MockMcpServer::default()
            .add_tool(helpers::create_test_tool("test_tool", "A test tool"))
            .add_resource(helpers::create_test_resource(
                "test://resource",
                "Test Resource",
            ));

        // Test initialize
        let result = server.handle_request("initialize", None).unwrap();
        assert!(result["serverInfo"]["name"] == "mock-server");

        // Test list tools
        let result = server.handle_request("tools/list", None).unwrap();
        assert!(result["tools"].as_array().unwrap().len() == 1);

        // Test call tool
        let result = server
            .handle_request("tools/call", Some(json!({"name": "test_tool"})))
            .unwrap();
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("test_tool"));
    }

    #[test]
    fn test_helpers() {
        let tool = helpers::create_test_tool("my_tool", "My tool");
        assert_eq!(tool.name, "my_tool");

        let resource = helpers::create_test_resource("test://uri", "My resource");
        assert_eq!(resource.uri, "test://uri");

        let prompt = helpers::create_test_prompt("my_prompt", "My prompt");
        assert_eq!(prompt.name, "my_prompt");
    }

    #[test]
    fn test_mock_server_default() {
        let server = MockMcpServer::default();
        // Should have no tools or resources
        let result = server.handle_request("tools/list", None).unwrap();
        assert_eq!(result["tools"].as_array().unwrap().len(), 0);

        let result = server.handle_request("resources/list", None).unwrap();
        assert_eq!(result["resources"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_mock_server_ping() {
        let server = MockMcpServer::new();
        let result = server.handle_request("ping", None).unwrap();
        assert!(result.is_object());
    }

    #[test]
    fn test_mock_server_unknown_method() {
        let server = MockMcpServer::new();
        let result = server.handle_request("unknown/method", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_mock_server_tool_call_no_params() {
        let server = MockMcpServer::new();
        let result = server.handle_request("tools/call", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_server_tool_call_no_name() {
        let server = MockMcpServer::new();
        let result = server.handle_request("tools/call", Some(json!({})));
        assert!(result.is_err());
    }

    #[test]
    fn test_helpers_tool_has_category() {
        let tool = helpers::create_test_tool("t", "desc");
        assert_eq!(tool.category, Some("test".to_string()));
    }

    #[test]
    fn test_helpers_resource_description() {
        let resource = helpers::create_test_resource("file://x", "X");
        assert!(resource.description.contains("X"));
        assert_eq!(resource.mime_type, Some("text/plain".to_string()));
    }

    #[test]
    fn test_helpers_prompt_no_arguments() {
        let prompt = helpers::create_test_prompt("p", "desc");
        assert!(prompt.arguments.is_none());
    }

    #[tokio::test]
    async fn test_mcp_test_context_new() {
        let ctx = McpTestContext::new().await.unwrap();
        assert_eq!(ctx.server_name, "test-server");
        assert!(ctx.server_handle.is_none());
    }

    #[test]
    fn test_create_test_tool_has_schema() {
        let tool = helpers::create_test_tool("t", "d");
        assert!(tool.input_schema.is_object());
        assert!(tool.input_schema["properties"]["test"].is_object());
    }
}
