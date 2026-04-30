//! MCP protocol message types (JSON-RPC)

use crate::types::{JsonRpcId, RequestId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (must be "2.0")
    pub jsonrpc: String,
    /// Request ID
    pub id: JsonRpcId,
    /// Method name
    pub method: String,
    /// Method parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version (must be "2.0")
    pub jsonrpc: String,
    /// Request ID
    pub id: JsonRpcId,
    /// Result (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC notification (no response expected)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// JSON-RPC version (must be "2.0")
    pub jsonrpc: String,
    /// Method name
    pub method: String,
    /// Notification parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// MCP Event for notification dispatching
#[derive(Debug, Clone)]
pub enum McpEvent {
    ToolsListChanged {
        server_id: String,
    },
    ProgressNotification {
        progress: f64,
        message: Option<String>,
    },
    LogNotification {
        level: String,
        message: String,
    },
    TaskStatusChanged {
        task_id: String,
        status: String,
    },
    ResourcesListChanged {
        server_id: String,
    },
    UnknownNotification {
        method: String,
        params: Option<Value>,
    },
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request
    pub fn new(id: impl Into<JsonRpcId>, method: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            method: method.into(),
            params: None,
        }
    }

    /// Add parameters to the request
    pub fn with_params(mut self, params: Value) -> Self {
        self.params = Some(params);
        self
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl JsonRpcResponse {
    /// Create a successful response
    pub fn success(id: impl Into<JsonRpcId>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: impl Into<JsonRpcId>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Create an error response with data
    pub fn error_with_data(
        id: impl Into<JsonRpcId>,
        code: i32,
        message: impl Into<String>,
        data: Value,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: Some(data),
            }),
        }
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Check if response is successful
    pub const fn is_success(&self) -> bool {
        self.result.is_some()
    }
}

impl JsonRpcNotification {
    /// Create a new notification
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params: None,
        }
    }

    /// Add parameters to the notification
    pub fn with_params(mut self, params: Value) -> Self {
        self.params = Some(params);
        self
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// JSON-RPC error codes
pub mod error_codes {
    /// Invalid JSON
    pub const PARSE_ERROR: i32 = -32700;
    /// JSON-RPC version mismatch
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid parameters
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error
    pub const INTERNAL_ERROR: i32 = -32603;
}

/// MCP-specific request wrapper
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum McpRequest {
    /// Initialize connection
    Initialize {
        id: RequestId,
        protocol_version: String,
        capabilities: Value,
        client_info: Value,
    },
    /// List available tools
    ListTools { id: RequestId },
    /// Call a tool
    CallTool {
        id: RequestId,
        name: String,
        arguments: Value,
    },
    /// List available resources
    ListResources { id: RequestId },
    /// Read a resource
    ReadResource { id: RequestId, uri: String },
    /// List available prompts
    ListPrompts { id: RequestId },
    /// Get a prompt
    GetPrompt {
        id: RequestId,
        name: String,
        arguments: Option<Value>,
    },
    /// Ping/health check
    Ping { id: RequestId },
    /// Get a task by ID (2025-11-25)
    GetTask { id: RequestId, task_id: String },
    /// List tasks (2025-11-25)
    ListTasks { id: RequestId },
    /// Cancel a task (2025-11-25)
    CancelTask {
        id: RequestId,
        task_id: String,
        reason: Option<String>,
    },
    /// Complete an argument completion request (2025-11-25)
    Complete {
        id: RequestId,
        ref_type: String,
        uri: String,
        argument_name: String,
    },
}

/// MCP-specific response wrapper
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum McpResponse {
    /// Initialize response
    Initialize {
        id: RequestId,
        protocol_version: String,
        capabilities: Value,
        server_info: Value,
    },
    /// Tool list
    Tools { id: RequestId, tools: Vec<Value> },
    /// Tool call result
    ToolResult {
        id: RequestId,
        content: Vec<Value>,
        is_error: Option<bool>,
    },
    /// Resource list
    Resources {
        id: RequestId,
        resources: Vec<Value>,
    },
    /// Resource contents
    ResourceContents { id: RequestId, contents: Vec<Value> },
    /// Prompt list
    Prompts { id: RequestId, prompts: Vec<Value> },
    /// Prompt content
    Prompt { id: RequestId, messages: Vec<Value> },
    /// Pong response
    Pong { id: RequestId },
    /// Task result (2025-11-25)
    Task { id: RequestId, task: Value },
    /// Task list (2025-11-25)
    TaskList { id: RequestId, tasks: Vec<Value> },
    /// Completion result (2025-11-25)
    Completion {
        id: RequestId,
        values: Vec<String>,
        total: usize,
        has_more: bool,
    },
    /// Error response
    Error { id: RequestId, error: String },
}

/// MCP notification wrapper
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum McpNotification {
    /// Tool list changed
    ToolsChanged,
    /// Resource list changed
    ResourcesChanged,
    /// Prompt list changed
    PromptsChanged,
    /// Cancel request
    Cancel {
        request_id: RequestId,
        reason: String,
    },
    /// Progress update
    Progress {
        token: String,
        progress: f64,
        message: Option<String>,
    },
    /// Task status changed (2025-11-25)
    TaskStatus { task_id: String, status: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_roundtrip() {
        let req = JsonRpcRequest::new("test-123", "test_method")
            .with_params(serde_json::json!({"key": "value"}));

        let json = req.to_json().unwrap();
        let parsed = JsonRpcRequest::from_json(&json).unwrap();

        assert_eq!(parsed.method, "test_method");
        assert_eq!(parsed.id, req.id);
        assert!(parsed.params.is_some());
    }

    #[test]
    fn test_json_rpc_response_success() {
        let resp = JsonRpcResponse::success("test-123", serde_json::json!({"result": "success"}));

        assert!(resp.is_success());
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let resp = JsonRpcResponse::error("test-123", -32601, "Method not found");

        assert!(!resp.is_success());
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_json_rpc_notification() {
        let notif =
            JsonRpcNotification::new("test_event").with_params(serde_json::json!({"data": 123}));

        let json = notif.to_json().unwrap();
        let parsed = JsonRpcNotification::from_json(&json).unwrap();

        assert_eq!(parsed.method, "test_event");
        assert!(parsed.params.is_some());
    }

    #[test]
    fn test_json_rpc_request_without_params() {
        let req = JsonRpcRequest::new(42i64, "ping");
        assert!(req.params.is_none());

        let json = req.to_json().unwrap();
        assert!(!json.contains("params")); // skip_serializing_if works
    }

    #[test]
    fn test_json_rpc_response_roundtrip() {
        let resp = JsonRpcResponse::success("id-1", serde_json::json!({"tools": []}));
        let json = resp.to_json().unwrap();
        let parsed = JsonRpcResponse::from_json(&json).unwrap();
        assert!(parsed.is_success());
        assert_eq!(parsed.id, JsonRpcId::String("id-1".to_string()));
    }

    #[test]
    fn test_json_rpc_error_with_data() {
        let resp = JsonRpcResponse::error_with_data(
            1i64,
            -32602,
            "Invalid params",
            serde_json::json!({"expected": "string"}),
        );
        assert!(!resp.is_success());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.data.is_some());
    }

    #[test]
    fn test_json_rpc_notification_without_params() {
        let notif = JsonRpcNotification::new("cancelled");
        let json = notif.to_json().unwrap();
        assert!(!json.contains("params"));
    }

    #[test]
    fn test_json_rpc_request_parse_valid() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let req = JsonRpcRequest::from_json(json).unwrap();
        assert_eq!(req.method, "initialize");
    }

    #[test]
    fn test_json_rpc_request_parse_invalid() {
        let json = r#"{"not":"valid"}"#;
        assert!(JsonRpcRequest::from_json(json).is_err());
    }

    #[test]
    fn test_error_codes_constants() {
        assert_eq!(error_codes::PARSE_ERROR, -32700);
        assert_eq!(error_codes::INVALID_REQUEST, -32600);
        assert_eq!(error_codes::METHOD_NOT_FOUND, -32601);
        assert_eq!(error_codes::INVALID_PARAMS, -32602);
        assert_eq!(error_codes::INTERNAL_ERROR, -32603);
    }

    #[test]
    fn test_json_rpc_id_number_roundtrip() {
        let req = JsonRpcRequest::new(99i64, "test");
        let json = req.to_json().unwrap();
        let parsed = JsonRpcRequest::from_json(&json).unwrap();
        assert_eq!(parsed.id, JsonRpcId::Number(99));
    }
}
