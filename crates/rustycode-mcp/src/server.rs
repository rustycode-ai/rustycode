//! MCP server implementation

use crate::protocol::{error_codes, JsonRpcRequest, JsonRpcResponse};
use crate::types::*;
use crate::{McpError, McpResult};
use rustycode_tools::ToolExecutor;
use serde_json::json;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

type ToolHandler = Arc<dyn Fn(serde_json::Value) -> McpResult<McpToolResult> + Send + Sync>;

#[derive(Clone)]
struct RegisteredTool {
    tool: McpTool,
    handler: ToolHandler,
}

/// MCP server configuration
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub server_name: String,
    pub server_version: String,
    /// Enable tools capability
    pub enable_tools: bool,
    /// Enable resources capability
    pub enable_resources: bool,
    /// Enable prompts capability
    pub enable_prompts: bool,
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            server_name: "rustycode-mcp-server".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            enable_tools: true,
            enable_resources: false,
            enable_prompts: false,
            timeout_secs: 30,
        }
    }
}

/// MCP server for hosting tools and resources
pub struct McpServer {
    pub config: McpServerConfig,
    tool_executor: Option<ToolExecutor>,
    tools: Arc<RwLock<HashMap<String, RegisteredTool>>>,
    pub resources: Arc<RwLock<HashMap<String, McpResourceEntry>>>,
    pub prompts: Arc<RwLock<HashMap<String, McpPromptTemplate>>>,
    initialized: Arc<RwLock<bool>>,
}

/// Internal resource entry
pub struct McpResourceEntry {
    resource: McpResource,
    content_fn: Arc<dyn Fn() -> McpResult<Vec<McpContent>> + Send + Sync>,
}

/// Internal prompt template
pub struct McpPromptTemplate {
    prompt: McpPrompt,
    template_fn:
        Arc<dyn Fn(Option<serde_json::Value>) -> McpResult<Vec<McpPromptMessage>> + Send + Sync>,
}

impl McpServer {
    pub fn new(_name: impl Into<String>, config: McpServerConfig) -> Self {
        Self {
            config,
            tool_executor: None,
            tools: Arc::new(RwLock::new(HashMap::new())),
            resources: Arc::new(RwLock::new(HashMap::new())),
            prompts: Arc::new(RwLock::new(HashMap::new())),
            initialized: Arc::new(RwLock::new(false)),
        }
    }

    /// Create a server with default configuration
    pub fn default_config(name: impl Into<String>) -> Self {
        let config = McpServerConfig {
            server_name: name.into(),
            ..Default::default()
        };
        Self::new("", config)
    }

    /// Register a tool executor
    pub fn register_tool_executor(&mut self, executor: ToolExecutor) {
        self.tool_executor = Some(executor);
    }

    /// Register an MCP tool with a custom handler.
    pub async fn register_tool<F>(&self, tool: McpTool, handler: F)
    where
        F: Fn(serde_json::Value) -> McpResult<McpToolResult> + Send + Sync + 'static,
    {
        let entry = RegisteredTool {
            tool: tool.clone(),
            handler: Arc::new(handler),
        };

        let mut tools = self.tools.write().await;
        tools.insert(tool.name.clone(), entry);
    }

    /// Register a resource
    pub async fn register_resource<F>(
        &self,
        uri: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        mime_type: impl Into<String>,
        content_fn: F,
    ) where
        F: Fn() -> McpResult<Vec<McpContent>> + Send + Sync + 'static,
    {
        let entry = McpResourceEntry {
            resource: McpResource {
                uri: uri.into(),
                name: name.into(),
                description: description.into(),
                mime_type: Some(mime_type.into()),
                title: None,
                annotations: None,
                size: None,
            },
            content_fn: Arc::new(content_fn),
        };

        let mut resources = self.resources.write().await;
        resources.insert(entry.resource.uri.clone(), entry);
    }

    /// Remove a resource by URI.
    pub async fn remove_resource(&self, uri: &str) -> bool {
        let mut resources = self.resources.write().await;
        resources.remove(uri).is_some()
    }

    /// Register a prompt template
    pub async fn register_prompt<F>(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        template_fn: F,
    ) where
        F: Fn(Option<serde_json::Value>) -> McpResult<Vec<McpPromptMessage>>
            + Send
            + Sync
            + 'static,
    {
        let prompt = McpPrompt {
            name: name.into(),
            description: description.into(),
            arguments: None,
            title: None,
            annotations: None,
        };

        let template = McpPromptTemplate {
            prompt,
            template_fn: Arc::new(template_fn),
        };

        let mut prompts = self.prompts.write().await;
        prompts.insert(template.prompt.name.clone(), template);
    }

    /// Run the server on stdio (for use as a subprocess)
    pub async fn run_stdio(&mut self) -> McpResult<()> {
        info!("Starting MCP server '{}' on stdio", self.config.server_name);

        // Use stdin/stdout directly
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        let mut stdin_lines = stdin.lock().lines();

        // Process messages
        for line in &mut stdin_lines {
            let line =
                line.map_err(|e| McpError::TransportError(format!("Failed to read stdin: {e}")))?;

            debug!("Received: {}", line.trim());

            // Parse request
            let request = match JsonRpcRequest::from_json(&line) {
                Ok(req) => req,
                Err(e) => {
                    error!("Failed to parse request: {}", e);
                    let error_resp = JsonRpcResponse::error(
                        "unknown",
                        error_codes::PARSE_ERROR,
                        format!("Parse error: {e}"),
                    );
                    self.send_response(&mut stdout, &error_resp)?;
                    continue;
                }
            };

            // Handle request with per-request timeout
            let timeout = std::time::Duration::from_secs(self.config.timeout_secs);
            let response = match tokio::time::timeout(timeout, self.handle_request(request)).await {
                Ok(response) => response,
                Err(_) => JsonRpcResponse::error(
                    "timeout",
                    error_codes::INTERNAL_ERROR,
                    format!("Request timeout ({}s exceeded)", self.config.timeout_secs),
                ),
            };

            // Send response
            self.send_response(&mut stdout, &response)?;
        }

        Ok(())
    }

    /// Send a response to stdout
    fn send_response(&self, stdout: &mut io::Stdout, response: &JsonRpcResponse) -> McpResult<()> {
        let json = response
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Failed to serialize response: {e}")))?;

        writeln!(stdout, "{json}")
            .map_err(|e| McpError::TransportError(format!("Failed to write stdout: {e}")))?;

        stdout
            .flush()
            .map_err(|e| McpError::TransportError(format!("Failed to flush stdout: {e}")))?;

        Ok(())
    }

    /// Handle an incoming request
    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();

        // Handle method
        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "tools/list" => self.handle_list_tools().await,
            "tools/call" => self.handle_call_tool(request.params).await,
            "resources/list" => self.handle_list_resources().await,
            "resources/read" => self.handle_read_resource(request.params).await,
            "completion/complete" => self.handle_completion(request.params).await,
            "prompts/list" => self.handle_list_prompts().await,
            "prompts/get" => self.handle_get_prompt(request.params).await,
            "ping" => Ok(json!({})),
            _ => Err(McpError::MethodNotFound(request.method.clone())),
        };

        match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err(e) => {
                error!("Request error: {}", e);
                let (code, message) = self.error_to_code(&e);
                JsonRpcResponse::error(id, code, message)
            }
        }
    }

    /// Handle initialize request
    async fn handle_initialize(
        &self,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        // Guard against double-initialize
        {
            let initialized = self.initialized.read().await;
            if *initialized {
                return Err(McpError::InvalidRequest(
                    "Server already initialized".to_string(),
                ));
            }
        }

        let params = params
            .ok_or_else(|| McpError::InvalidRequest("Initialize requires params".to_string()))?;

        // Version negotiation: check client's requested version
        let client_version = params
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if client_version != crate::MCP_VERSION {
            tracing::warn!(
                "Client requested protocol version {}, server supports {}",
                client_version,
                crate::MCP_VERSION
            );
        }

        // Mark as initialized
        let mut initialized = self.initialized.write().await;
        *initialized = true;

        // Build capabilities
        let mut capabilities = json!({});

        if self.config.enable_tools {
            capabilities["tools"] = json!({});
        }

        if self.config.enable_resources {
            capabilities["resources"] = json!({
                "subscribe": false,
                "listChanged": false
            });
        }

        if self.config.enable_prompts {
            capabilities["prompts"] = json!({});
        }

        Ok(json!({
            "protocolVersion": crate::MCP_VERSION,
            "capabilities": capabilities,
            "serverInfo": {
                "name": self.config.server_name,
                "version": self.config.server_version
            }
        }))
    }

    /// Handle list tools request
    async fn handle_list_tools(&self) -> McpResult<serde_json::Value> {
        let mut tools_by_name: HashMap<String, serde_json::Value> = HashMap::new();

        if let Some(executor) = &self.tool_executor {
            let tool_infos = executor.list();

            for info in tool_infos {
                let tool = McpTool {
                    name: info.name.clone(),
                    description: info.description.clone(),
                    input_schema: info.parameters_schema.clone(),
                    category: None,
                    title: None,
                    output_schema: None,
                    annotations: None,
                };
                tools_by_name.insert(
                    tool.name.clone(),
                    serde_json::to_value(&tool).unwrap_or_else(|e| {
                        tracing::warn!("Failed to serialize tool {}: {}", tool.name, e);
                        json!({})
                    }),
                );
            }
        }

        let tools = self.tools.read().await;
        for entry in tools.values() {
            tools_by_name.insert(
                entry.tool.name.clone(),
                serde_json::to_value(&entry.tool).unwrap_or_else(|e| {
                    tracing::warn!("Failed to serialize tool {}: {}", entry.tool.name, e);
                    json!({})
                }),
            );
        }

        Ok(json!({ "tools": tools_by_name.into_values().collect::<Vec<_>>() }))
    }

    /// Handle call tool request
    async fn handle_call_tool(
        &self,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        let params = params
            .ok_or_else(|| McpError::InvalidRequest("Call tool requires params".to_string()))?;

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Tool name required".to_string()))?;

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        {
            let tools = self.tools.read().await;
            if let Some(entry) = tools.get(name) {
                let handler = entry.handler.clone();
                let result = tokio::task::spawn_blocking(move || handler(arguments))
                    .await
                    .map_err(|e| McpError::InternalError(format!("handler panicked: {e}")))??;
                let mut response = json!({
                    "content": result.content,
                });
                if result.is_error.unwrap_or(false) {
                    response["isError"] = json!(true);
                }
                return Ok(response);
            }
        }

        // Execute tool
        let executor = self
            .tool_executor
            .as_ref()
            .ok_or_else(|| McpError::InternalError("No tool executor registered".to_string()))?;

        let tool_call = rustycode_protocol::ToolCall {
            call_id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            arguments,
        };

        let result = executor.execute(&tool_call);

        if result.error.is_none() {
            let content = vec![json!({
                "type": "text",
                "text": result.output
            })];

            Ok(json!({ "content": content }))
        } else {
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": result.error.unwrap_or_default()
                }],
                "isError": true
            }))
        }
    }

    /// Handle list resources request
    async fn handle_list_resources(&self) -> McpResult<serde_json::Value> {
        let resources = self.resources.read().await;
        let resource_list: Vec<McpResource> =
            resources.values().map(|e| e.resource.clone()).collect();

        Ok(json!({ "resources": resource_list }))
    }

    /// Handle read resource request
    async fn handle_read_resource(
        &self,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        let params = params
            .ok_or_else(|| McpError::InvalidRequest("Read resource requires params".to_string()))?;

        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Resource URI required".to_string()))?;

        let resources = self.resources.read().await;
        let entry = resources
            .get(uri)
            .ok_or_else(|| McpError::ResourceNotFound(uri.to_string()))?;

        let contents = (entry.content_fn)()?;

        Ok(json!({ "contents": contents }))
    }

    /// Handle list prompts request
    async fn handle_list_prompts(&self) -> McpResult<serde_json::Value> {
        let prompts = self.prompts.read().await;
        let prompt_list: Vec<McpPrompt> = prompts.values().map(|p| p.prompt.clone()).collect();

        Ok(json!({ "prompts": prompt_list }))
    }

    /// Handle get prompt request
    async fn handle_get_prompt(
        &self,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        let params = params
            .ok_or_else(|| McpError::InvalidRequest("Get prompt requires params".to_string()))?;

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Prompt name required".to_string()))?;

        let arguments = params.get("arguments").cloned();

        let prompts = self.prompts.read().await;
        let template = prompts
            .get(name)
            .ok_or_else(|| McpError::InvalidRequest(format!("Prompt '{name}' not found")))?;

        let messages = (template.template_fn)(arguments)?;

        Ok(json!({ "messages": messages }))
    }

    /// Handle completion/complete request (2025-11-25)
    async fn handle_completion(
        &self,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        let _ = params;
        // Completion support requires handler registration; return empty by default
        Ok(json!({ "completion": { "values": [], "total": 0, "hasMore": false } }))
    }

    /// Convert error to JSON-RPC error code
    fn error_to_code(&self, error: &McpError) -> (i32, String) {
        match error {
            McpError::InvalidRequest(_) => (error_codes::INVALID_PARAMS, error.to_string()),
            McpError::MethodNotFound(_) => (error_codes::METHOD_NOT_FOUND, error.to_string()),
            McpError::ToolNotFound(_) => (error_codes::INVALID_PARAMS, error.to_string()),
            McpError::ResourceNotFound(_) => (error_codes::INVALID_PARAMS, error.to_string()),
            _ => (error_codes::INTERNAL_ERROR, error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_lsp_tool_executor;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_server_creation() {
        let config = McpServerConfig {
            server_name: "test-server".to_string(),
            ..Default::default()
        };
        let server = McpServer::new("test", config);
        assert_eq!(server.config.server_name, "test-server");
    }

    #[tokio::test]
    async fn test_register_resource() {
        let server = McpServer::default_config("test-server");

        server
            .register_resource(
                "test://resource",
                "Test Resource",
                "A test resource",
                "text/plain",
                || {
                    Ok(vec![McpContent::Text {
                        text: "Hello, World!".to_string(),
                    }])
                },
            )
            .await;

        let resources = server.resources.read().await;
        assert_eq!(resources.len(), 1);
        assert!(resources.contains_key("test://resource"));
    }

    #[tokio::test]
    async fn test_register_prompt() {
        let server = McpServer::default_config("test-server");

        server
            .register_prompt("test-prompt", "A test prompt", |_args| {
                Ok(vec![McpPromptMessage {
                    role: "user".to_string(),
                    content: McpPromptContent::Text {
                        text: "Test prompt".to_string(),
                    },
                }])
            })
            .await;

        let prompts = server.prompts.read().await;
        assert_eq!(prompts.len(), 1);
        assert!(prompts.contains_key("test-prompt"));
    }

    #[tokio::test]
    async fn test_register_tool() {
        let server = McpServer::default_config("test-server");

        server
            .register_tool(
                McpTool {
                    name: "echo".to_string(),
                    description: "Echo input".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "text": {"type": "string"}
                        }
                    }),
                    category: None,
                    title: None,
                    output_schema: None,
                    annotations: None,
                },
                |args| {
                    let text = args
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    Ok(McpToolResult {
                        content: vec![McpContent::Text {
                            text: text.to_string(),
                        }],
                        is_error: Some(false),
                        structured_content: None,
                        meta: None,
                    })
                },
            )
            .await;

        let request = JsonRpcRequest::new("1", "tools/call").with_params(json!({
            "name": "echo",
            "arguments": {"text": "hello"},
        }));
        let response = server.handle_request(request).await;
        let response_json = serde_json::to_value(response).unwrap();
        assert!(response_json.to_string().contains("hello"));
    }

    #[test]
    fn test_server_config_default() {
        let config = McpServerConfig::default();
        assert_eq!(config.server_name, "rustycode-mcp-server");
        assert!(config.enable_tools);
        assert!(!config.enable_resources);
        assert!(!config.enable_prompts);
        assert_eq!(config.timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_server_default_config_constructor() {
        let server = McpServer::default_config("test");
        // default_config sets server_name to the passed argument
        assert_eq!(server.config.server_name, "test");
    }

    #[tokio::test]
    async fn test_server_not_initialized() {
        let server = McpServer::default_config("test");
        let init = server.initialized.read().await;
        assert!(!*init);
    }

    #[tokio::test]
    async fn test_register_multiple_resources() {
        let server = McpServer::default_config("test");

        server
            .register_resource("test://r1", "R1", "First", "text/plain", || {
                Ok(vec![McpContent::Text {
                    text: "one".to_string(),
                }])
            })
            .await;
        server
            .register_resource("test://r2", "R2", "Second", "text/plain", || {
                Ok(vec![McpContent::Text {
                    text: "two".to_string(),
                }])
            })
            .await;

        let resources = server.resources.read().await;
        assert_eq!(resources.len(), 2);
    }

    #[tokio::test]
    async fn test_register_multiple_prompts() {
        let server = McpServer::default_config("test");

        server
            .register_prompt("p1", "Prompt 1", |_args| {
                Ok(vec![McpPromptMessage {
                    role: "user".to_string(),
                    content: McpPromptContent::Text {
                        text: "one".to_string(),
                    },
                }])
            })
            .await;
        server
            .register_prompt("p2", "Prompt 2", |_args| {
                Ok(vec![McpPromptMessage {
                    role: "user".to_string(),
                    content: McpPromptContent::Text {
                        text: "two".to_string(),
                    },
                }])
            })
            .await;

        let prompts = server.prompts.read().await;
        assert_eq!(prompts.len(), 2);
    }

    #[tokio::test]
    async fn test_server_config_custom() {
        let config = McpServerConfig {
            server_name: "custom-server".to_string(),
            server_version: "3.0.0".to_string(),
            enable_tools: false,
            enable_resources: true,
            enable_prompts: true,
            timeout_secs: 60,
        };
        let server = McpServer::new("custom", config);
        assert!(!server.config.enable_tools);
        assert!(server.config.enable_resources);
        assert!(server.config.enable_prompts);
        assert_eq!(server.config.timeout_secs, 60);
    }

    #[tokio::test]
    async fn test_server_error_to_code_mapping() {
        let server = McpServer::default_config("test");

        let (code, msg) = server.error_to_code(&McpError::InvalidRequest("bad".to_string()));
        assert_eq!(code, error_codes::INVALID_PARAMS);
        assert!(msg.contains("bad"));

        let (code, _) = server.error_to_code(&McpError::MethodNotFound("x".to_string()));
        assert_eq!(code, error_codes::METHOD_NOT_FOUND);

        let (code, _) = server.error_to_code(&McpError::ToolNotFound("Bash".to_string()));
        assert_eq!(code, error_codes::INVALID_PARAMS);

        let (code, _) = server.error_to_code(&McpError::ResourceNotFound("file://x".to_string()));
        assert_eq!(code, error_codes::INVALID_PARAMS);

        let (code, _) = server.error_to_code(&McpError::Timeout);
        assert_eq!(code, error_codes::INTERNAL_ERROR);

        let (code, _) = server.error_to_code(&McpError::TransportError("fail".to_string()));
        assert_eq!(code, error_codes::INTERNAL_ERROR);
    }

    #[tokio::test]
    async fn test_server_handle_request_unknown_method() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("test-id", "unknown/method").with_params(json!({}));
        let response = server.handle_request(request).await;
        assert!(!response.is_success());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, error_codes::METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_server_handle_ping() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("ping-1", "ping");
        let response = server.handle_request(request).await;
        assert!(response.is_success());
    }

    #[tokio::test]
    async fn test_server_handle_initialize_no_params() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("init-1", "initialize");
        let response = server.handle_request(request).await;
        assert!(!response.is_success());
    }

    #[tokio::test]
    async fn test_server_handle_initialize_with_params() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("init-1", "initialize").with_params(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {}
        }));
        let response = server.handle_request(request).await;
        assert!(response.is_success());
        let result = response.result.unwrap();
        assert_eq!(result["protocolVersion"], crate::MCP_VERSION);
        assert_eq!(result["serverInfo"]["name"], "test");
    }

    #[tokio::test]
    async fn test_server_handle_list_tools_no_executor() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("tools-1", "tools/list");
        let response = server.handle_request(request).await;
        // No executor registered but should return empty tools list
        assert!(response.is_success());
        assert_eq!(
            response.result.unwrap()["tools"].as_array().unwrap().len(),
            0
        );
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn test_server_handle_list_tools_includes_lsp_executor_tools() {
        let workspace = tempdir().expect("temp workspace");
        let mut server = McpServer::default_config("test");
        server.register_tool_executor(build_lsp_tool_executor(workspace.path().to_path_buf()));

        let request = JsonRpcRequest::new("tools-1", "tools/list");
        let response = server.handle_request(request).await;
        assert!(response.is_success());

        let result = response.result.unwrap();
        let tools = result["tools"].as_array().expect("tools array");
        let names: Vec<String> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|name| name.as_str()))
            .map(ToOwned::to_owned)
            .collect();

        assert!(names.contains(&"LspHover".to_string()));
        assert!(names.contains(&"LspDefinition".to_string()));
        assert!(names.contains(&"LspCompletion".to_string()));
        assert!(names.contains(&"LspDiagnostics".to_string()));
    }

    #[tokio::test]
    async fn test_server_handle_call_tool_no_params() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("call-1", "tools/call");
        let response = server.handle_request(request).await;
        assert!(!response.is_success());
    }

    #[tokio::test]
    async fn test_server_handle_call_tool_no_executor() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("call-1", "tools/call")
            .with_params(json!({"name": "test", "arguments": {}}));
        let response = server.handle_request(request).await;
        assert!(!response.is_success());
    }

    #[tokio::test]
    async fn test_server_handle_list_resources_empty() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("res-1", "resources/list");
        let response = server.handle_request(request).await;
        assert!(response.is_success());
        assert_eq!(
            response.result.unwrap()["resources"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn test_server_handle_list_prompts_empty() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("prompt-1", "prompts/list");
        let response = server.handle_request(request).await;
        assert!(response.is_success());
        assert_eq!(
            response.result.unwrap()["prompts"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn test_server_handle_read_resource_not_found() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("read-1", "resources/read")
            .with_params(json!({"uri": "file:///nonexistent"}));
        let response = server.handle_request(request).await;
        assert!(!response.is_success());
    }

    #[tokio::test]
    async fn test_server_handle_get_prompt_not_found() {
        let server = McpServer::default_config("test");
        let request = JsonRpcRequest::new("prompt-1", "prompts/get")
            .with_params(json!({"name": "nonexistent"}));
        let response = server.handle_request(request).await;
        assert!(!response.is_success());
    }

    #[tokio::test]
    async fn test_server_with_all_capabilities_enabled() {
        let config = McpServerConfig {
            server_name: "full".to_string(),
            server_version: "1.0".to_string(),
            enable_tools: true,
            enable_resources: true,
            enable_prompts: true,
            timeout_secs: 30,
        };
        let server = McpServer::new("full", config);
        let request = JsonRpcRequest::new("init-1", "initialize")
            .with_params(json!({"protocolVersion": "2024-11-05", "capabilities": {}}));
        let response = server.handle_request(request).await;
        let result = response.result.unwrap();
        assert!(result["capabilities"]["tools"].is_object());
        assert!(result["capabilities"]["resources"].is_object());
        assert!(result["capabilities"]["prompts"].is_object());
    }

    #[tokio::test]
    async fn test_server_double_initialize_rejected() {
        let server = McpServer::default_config("test");

        // First initialize should succeed
        let request = JsonRpcRequest::new("init-1", "initialize")
            .with_params(json!({"protocolVersion": "2024-11-05", "capabilities": {}}));
        let response = server.handle_request(request).await;
        assert!(response.is_success());

        // Second initialize should fail with INVALID_PARAMS (-32600 maps to -32602 in our code)
        let request2 = JsonRpcRequest::new("init-2", "initialize")
            .with_params(json!({"protocolVersion": "2024-11-05", "capabilities": {}}));
        let response2 = server.handle_request(request2).await;
        assert!(!response2.is_success());
        let err = response2.error.unwrap();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
        assert!(err.message.contains("already initialized"));
    }
}
