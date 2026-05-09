//! Core type definitions for MCP protocol

use serde::{Deserialize, Serialize};

/// Unique request identifier
pub type RequestId = String;

/// JSON-RPC request ID (can be string, number, or null)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
#[non_exhaustive]
pub enum JsonRpcId {
    String(String),
    Number(i64),
    Null,
}

impl From<String> for JsonRpcId {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for JsonRpcId {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<i64> for JsonRpcId {
    fn from(n: i64) -> Self {
        Self::Number(n)
    }
}

/// MCP tool definition (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name
    pub name: String,
    /// Human-readable title (for display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for input parameters
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    /// JSON Schema for structured output (2025-11-25)
    #[serde(rename = "outputSchema", skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Tool annotations hinting behavior (2025-11-25)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpToolAnnotations>,
    /// Optional category (non-standard extension)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Tool annotations (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpToolAnnotations {
    /// Human-readable title for display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// If true, the tool does not modify its environment
    #[serde(
        rename = "readOnlyHint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub read_only_hint: Option<bool>,
    /// If true, the tool may perform destructive operations
    #[serde(
        rename = "destructiveHint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub destructive_hint: Option<bool>,
    /// If true, the tool may interact with an open-ended set of entities
    #[serde(
        rename = "openWorldHint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub open_world_hint: Option<bool>,
}

/// MCP resource definition (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// Resource URI
    pub uri: String,
    /// Human-readable title (for display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Resource name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// MIME type
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Resource annotations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpAnnotations>,
    /// Total size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// Generic annotations for resources/prompts (2025-11-25)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpAnnotations {
    /// Human-readable title for display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Hint about how noisy/chatty the item is (0.0-1.0)
    #[serde(
        rename = "audienceHint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub audience_hint: Option<serde_json::Value>,
    /// Last modification timestamp
    #[serde(
        rename = "lastModifiedHint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_modified_hint: Option<String>,
}

/// MCP resource template (for dynamic resource access, 2025-11-25)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceTemplate {
    /// URI template (e.g., `"file://{path}"`, `"mcp://files/{filename}"`)
    #[serde(rename = "uriTemplate")]
    pub uri_template: String,
    /// Human-readable title (for display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Template name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// MIME type
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Resource template annotations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpAnnotations>,
}

/// MCP prompt template (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// Prompt name/ID
    pub name: String,
    /// Human-readable title (for display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Human-readable description
    pub description: String,
    /// Optional arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<McpPromptArgument>>,
    /// Prompt annotations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpAnnotations>,
}

/// Prompt argument definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    /// Argument name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Whether this argument is required
    #[serde(default)]
    pub required: bool,
}

/// Prompt message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptMessage {
    /// Content role (user, assistant, system)
    pub role: String,
    /// Text content
    pub content: McpPromptContent,
}

/// Prompt content (can be text or multimodal)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum McpPromptContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

/// Tool call result (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Result content
    pub content: Vec<McpContent>,
    /// Structured output matching outputSchema (2025-11-25)
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
    /// Whether this is an error result
    #[serde(default, rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Protocol metadata
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// MCP content block (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "audio")]
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    #[serde(rename = "resource_link")]
    ResourceLink {
        uri: String,
        name: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
}

/// Resource contents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceContents {
    /// Resource URI
    pub uri: String,
    /// Content blocks
    pub contents: Vec<McpContent>,
}

/// Server capabilities (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerCapabilities {
    /// Available tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<McpToolsCapability>,
    /// Available resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<McpResourcesCapability>,
    /// Available prompts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<McpPromptsCapability>,
    /// Logging capability (2025-11-25)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<serde_json::Value>,
    /// Completions capability (2025-11-25)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<serde_json::Value>,
    /// Tasks capability (2025-11-25)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<McpTasksCapability>,
    /// Optional extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Tools capability
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpToolsCapability {
    /// Whether tools can be listed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Resources capability
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpResourcesCapability {
    /// Whether resources can be listed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    /// Whether resource lists can change
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Prompts capability
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPromptsCapability {
    /// Whether prompts can be listed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Task status lifecycle (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTaskStatus {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

/// Task definition (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTask {
    /// Task ID
    pub id: String,
    pub status: McpTaskStatus,
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Task result (when completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Task error (when failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    /// Protocol metadata
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Tasks capability (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpTasksCapability {
    /// Whether the server supports task list change notifications
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Initialize request from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeRequest {
    /// Protocol version
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Client capabilities
    pub capabilities: McpClientCapabilities,
    /// Client information
    pub client_info: McpClientInfo,
}

/// Initialize response from server (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResponse {
    /// Protocol version
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Server capabilities
    pub capabilities: McpServerCapabilities,
    /// Server information
    #[serde(rename = "serverInfo")]
    pub server_info: McpServerInfo,
    /// Human-readable server instructions (2025-11-25)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// Client capabilities (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpClientCapabilities {
    /// Sampling/reporting capability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<McpSamplingCapability>,
    /// Roots/listing capability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<McpRootsCapability>,
    /// Elicitation capability — client can present forms/URLs to user (2025-11-25)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<serde_json::Value>,
    /// Optional extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Sampling capability
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpSamplingCapability {
    /// Sampling threshold (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// Roots capability
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpRootsCapability {
    /// Whether roots can be listed
    #[serde(default)]
    pub list_changed: bool,
}

/// Client information (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpClientInfo {
    /// Client name
    pub name: String,
    /// Client version
    pub version: String,
    /// Human-readable title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Server information (2025-11-25 spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Server name
    pub name: String,
    /// Server version
    pub version: String,
    /// Human-readable title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Implementation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::JsonRpcRequest;

    #[test]
    fn test_mcp_tool_serialization() {
        let tool = McpTool {
            name: "test_tool".to_string(),
            title: None,
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "param": {"type": "string"}
                }
            }),
            output_schema: None,
            annotations: None,
            category: Some("test".to_string()),
        };

        let json = serde_json::to_string(&tool).unwrap();
        let parsed: McpTool = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, tool.name);
        assert_eq!(parsed.description, tool.description);
    }

    #[test]
    fn test_json_rpc_id() {
        let id_str: JsonRpcId = "test-123".to_string().into();
        let id_num: JsonRpcId = 42i64.into();

        assert!(matches!(id_str, JsonRpcId::String(_)));
        assert!(matches!(id_num, JsonRpcId::Number(_)));
    }

    #[test]
    fn test_mcp_tool_without_category() {
        let tool = McpTool {
            name: "Read".to_string(),
            title: None,
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(!json.contains("category")); // skip_serializing_if works
        let parsed: McpTool = serde_json::from_str(&json).unwrap();
        assert!(parsed.category.is_none());
    }

    #[test]
    fn test_mcp_resource_serialization() {
        let resource = McpResource {
            uri: "file:///test.txt".to_string(),
            title: None,
            name: "test.txt".to_string(),
            description: "A test file".to_string(),
            mime_type: Some("text/plain".to_string()),
            annotations: None,
            size: None,
        };
        let json = serde_json::to_string(&resource).unwrap();
        assert!(json.contains("mimeType")); // camelCase rename works
        let parsed: McpResource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.uri, resource.uri);
    }

    #[test]
    fn test_mcp_resource_template_serialization() {
        let tmpl = McpResourceTemplate {
            uri_template: "file://{path}".to_string(),
            name: "file".to_string(),
            description: "File template".to_string(),
            mime_type: None,
            title: None,
            annotations: None,
        };
        let json = serde_json::to_string(&tmpl).unwrap();
        assert!(json.contains("uriTemplate")); // camelCase rename
        assert!(!json.contains("mimeType")); // skip_serializing_if
    }

    #[test]
    fn test_mcp_prompt_with_arguments() {
        let prompt = McpPrompt {
            name: "code_review".to_string(),
            description: "Review code".to_string(),
            arguments: Some(vec![McpPromptArgument {
                name: "language".to_string(),
                description: "Programming language".to_string(),
                required: true,
            }]),
            title: None,
            annotations: None,
        };
        let json = serde_json::to_string(&prompt).unwrap();
        let parsed: McpPrompt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.arguments.unwrap().len(), 1);
    }

    #[test]
    fn test_mcp_prompt_without_arguments() {
        let prompt = McpPrompt {
            name: "hello".to_string(),
            description: "Say hello".to_string(),
            arguments: None,
            title: None,
            annotations: None,
        };
        let json = serde_json::to_string(&prompt).unwrap();
        assert!(!json.contains("arguments"));
    }

    #[test]
    fn test_mcp_prompt_content_text() {
        let content = McpPromptContent::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"text\""));
    }

    #[test]
    fn test_mcp_prompt_content_image() {
        let content = McpPromptContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"image\""));
        let parsed: McpPromptContent = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, McpPromptContent::Image { .. }));
    }

    #[test]
    fn test_mcp_tool_result_serialization() {
        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "Done".to_string(),
            }],
            is_error: Some(false),
            structured_content: None,
            meta: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: McpToolResult = serde_json::from_str(&json).unwrap();
        assert!(!parsed.is_error.unwrap());
    }

    #[test]
    fn test_mcp_tool_result_error() {
        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "Tool failed".to_string(),
            }],
            is_error: Some(true),
            structured_content: None,
            meta: None,
        };
        assert!(result.is_error.unwrap());
    }

    #[test]
    fn test_mcp_content_text_equality() {
        let a = McpContent::Text {
            text: "hello".to_string(),
        };
        let b = McpContent::Text {
            text: "hello".to_string(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_mcp_server_capabilities_default() {
        let caps = McpServerCapabilities::default();
        assert!(caps.tools.is_none());
        assert!(caps.resources.is_none());
        assert!(caps.prompts.is_none());
        assert!(caps.extensions.is_none());
    }

    #[test]
    fn test_mcp_server_capabilities_with_tools() {
        let caps = McpServerCapabilities {
            tools: Some(McpToolsCapability {
                list_changed: Some(true),
            }),
            resources: None,
            prompts: None,
            extensions: None,
            logging: None,
            completions: None,
            tasks: None,
        };
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("tools"));
        assert!(!json.contains("resources"));
    }

    #[test]
    fn test_initialize_response_roundtrip() {
        let resp = InitializeResponse {
            protocol_version: "2024-11-05".to_string(),
            capabilities: McpServerCapabilities {
                tools: Some(McpToolsCapability {
                    list_changed: Some(true),
                }),
                resources: Some(McpResourcesCapability {
                    subscribe: Some(true),
                    list_changed: Some(true),
                }),
                prompts: None,
                extensions: None,
                logging: None,
                completions: None,
                tasks: None,
            },
            server_info: McpServerInfo {
                name: "test-server".to_string(),
                version: "1.0.0".to_string(),
                title: None,
            },
            instructions: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("protocolVersion")); // camelCase rename
        assert!(json.contains("serverInfo"));
        let parsed: InitializeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.protocol_version, "2024-11-05");
    }

    #[test]
    fn test_mcp_client_info_serialization() {
        let info = McpClientInfo {
            name: "rustycode".to_string(),
            version: "0.1.0".to_string(),
            title: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: McpClientInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "rustycode");
    }

    #[test]
    fn test_json_rpc_id_null() {
        let json = r#"{"jsonrpc":"2.0","id":null,"method":"test"}"#;
        let req = serde_json::from_str::<JsonRpcRequest>(json).unwrap();
        assert!(matches!(req.id, JsonRpcId::Null));
    }

    #[test]
    fn test_implementation_serialization() {
        let imp = Implementation {
            name: "test".to_string(),
            version: "1.0".to_string(),
        };
        let json = serde_json::to_string(&imp).unwrap();
        let parsed: Implementation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
    }

    #[test]
    fn test_mcp_prompt_message_serialization() {
        let msg = McpPromptMessage {
            role: "user".to_string(),
            content: McpPromptContent::Text {
                text: "Review this code".to_string(),
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: McpPromptMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, "user");
    }

    #[test]
    fn test_mcp_resource_contents_serialization() {
        let rc = McpResourceContents {
            uri: "file:///test.rs".to_string(),
            contents: vec![McpContent::Text {
                text: "fn main() {}".to_string(),
            }],
        };
        let json = serde_json::to_string(&rc).unwrap();
        let parsed: McpResourceContents = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.contents.len(), 1);
    }

    #[test]
    fn test_mcp_content_image_roundtrip() {
        let content = McpContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"image\""));
        let parsed: McpContent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, content);
    }

    #[test]
    fn test_mcp_content_resource_roundtrip() {
        let content = McpContent::Resource {
            uri: "file:///test.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"resource\""));
        let parsed: McpContent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, content);
    }

    #[test]
    fn test_mcp_prompt_content_resource() {
        let content = McpPromptContent::Resource {
            uri: "file:///doc".to_string(),
            mime_type: "application/pdf".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"type\":\"resource\""));
        let parsed: McpPromptContent = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, McpPromptContent::Resource { .. }));
    }

    #[test]
    fn test_initialize_request_serialization() {
        let req = InitializeRequest {
            protocol_version: "2024-11-05".to_string(),
            capabilities: McpClientCapabilities::default(),
            client_info: McpClientInfo {
                name: "test".to_string(),
                version: "1.0".to_string(),
                title: None,
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"protocolVersion\":\"2024-11-05\""));
        let parsed: InitializeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.protocol_version, "2024-11-05");
        assert_eq!(parsed.client_info.name, "test");
    }

    #[test]
    fn test_mcp_server_info_serialization() {
        let info = McpServerInfo {
            name: "my-server".to_string(),
            version: "2.0.0".to_string(),
            title: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: McpServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "my-server");
        assert_eq!(parsed.version, "2.0.0");
    }

    #[test]
    fn test_mcp_client_capabilities_default() {
        let caps = McpClientCapabilities::default();
        assert!(caps.sampling.is_none());
        assert!(caps.roots.is_none());
    }

    #[test]
    fn test_mcp_client_capabilities_with_fields() {
        let caps = McpClientCapabilities {
            sampling: Some(McpSamplingCapability {
                threshold: Some(0.5),
            }),
            roots: Some(McpRootsCapability { list_changed: true }),
            extensions: None,
            elicitation: None,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let parsed: McpClientCapabilities = serde_json::from_str(&json).unwrap();
        assert!(parsed.sampling.is_some());
        assert!(parsed.roots.is_some());
        assert!(parsed.roots.unwrap().list_changed);
    }

    #[test]
    fn test_mcp_tools_capability_default() {
        let cap = McpToolsCapability::default();
        assert!(cap.list_changed.is_none());
    }

    #[test]
    fn test_mcp_resources_capability_default() {
        let cap = McpResourcesCapability::default();
        assert!(cap.subscribe.is_none());
        assert!(cap.list_changed.is_none());
    }

    #[test]
    fn test_mcp_prompts_capability_default() {
        let cap = McpPromptsCapability::default();
        assert!(cap.list_changed.is_none());
    }

    #[test]
    fn test_mcp_tool_result_no_error() {
        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "done".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("isError"));
        let parsed: McpToolResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_error.is_none());
    }

    #[test]
    fn test_json_rpc_id_from_str_ref() {
        let id: JsonRpcId = "hello".into();
        assert_eq!(id, JsonRpcId::String("hello".to_string()));
    }

    #[test]
    fn test_json_rpc_id_from_i64() {
        let id: JsonRpcId = 42i64.into();
        assert_eq!(id, JsonRpcId::Number(42));
    }

    #[test]
    fn test_json_rpc_id_equality() {
        assert_eq!(JsonRpcId::String("a".into()), JsonRpcId::String("a".into()));
        assert_eq!(JsonRpcId::Number(1), JsonRpcId::Number(1));
        assert_eq!(JsonRpcId::Null, JsonRpcId::Null);
        assert_ne!(JsonRpcId::Null, JsonRpcId::Number(0));
    }

    #[test]
    fn test_mcp_prompt_argument_required_field() {
        let arg = McpPromptArgument {
            name: "lang".to_string(),
            description: "Language".to_string(),
            required: true,
        };
        let json = serde_json::to_string(&arg).unwrap();
        let parsed: McpPromptArgument = serde_json::from_str(&json).unwrap();
        assert!(parsed.required);

        let arg_not_required = McpPromptArgument {
            name: "x".to_string(),
            description: "X".to_string(),
            required: false,
        };
        assert!(!arg_not_required.required);
    }

    #[test]
    fn test_mcp_resource_template_with_mime_type() {
        let tmpl = McpResourceTemplate {
            uri_template: "file://{path}".to_string(),
            name: "f".to_string(),
            description: "File".to_string(),
            mime_type: Some("text/plain".to_string()),
            title: None,
            annotations: None,
        };
        let json = serde_json::to_string(&tmpl).unwrap();
        assert!(json.contains("mimeType"));
        let parsed: McpResourceTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mime_type, Some("text/plain".to_string()));
    }
}
