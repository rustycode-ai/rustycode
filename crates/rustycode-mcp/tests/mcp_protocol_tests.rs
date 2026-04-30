#![allow(
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::ignored_unit_patterns,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! MCP protocol integration tests

use rustycode_mcp::{
    client::McpClient,
    protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse},
    server::McpServer,
    testing::helpers,
    testing::MockMcpServer,
    types::*,
    McpError,
};
use serde_json::json;

#[tokio::test]
async fn test_json_rpc_request_serialization() {
    let req = JsonRpcRequest::new("test-123", "test_method").with_params(json!({"key": "value"}));

    let json = req.to_json().unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"test_method\""));
    assert!(json.contains("\"key\""));
}

#[tokio::test]
async fn test_json_rpc_response_success() {
    let resp = JsonRpcResponse::success("test-123", json!({"result": "success"}));

    let json = resp.to_json().unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"result\""));

    let parsed = JsonRpcResponse::from_json(&json).unwrap();
    assert!(parsed.is_success());
}

#[tokio::test]
async fn test_json_rpc_response_error() {
    let resp = JsonRpcResponse::error("test-123", -32601, "Method not found");

    let json = resp.to_json().unwrap();
    assert!(json.contains("\"error\""));
    assert!(json.contains("\"code\":-32601"));

    let parsed = JsonRpcResponse::from_json(&json).unwrap();
    assert!(!parsed.is_success());
    assert!(parsed.error.is_some());
}

#[tokio::test]
async fn test_json_rpc_notification() {
    let notif = JsonRpcNotification::new("test_event").with_params(json!({"data": 123}));

    let json = notif.to_json().unwrap();
    assert!(json.contains("\"method\":\"test_event\""));
    assert!(!json.contains("\"id\"")); // Notifications don't have IDs

    let parsed = JsonRpcNotification::from_json(&json).unwrap();
    assert_eq!(parsed.method, "test_event");
}

#[tokio::test]
async fn test_mcp_tool_serialization() {
    let tool = McpTool {
        name: "test_tool".to_string(),
        title: None,
        description: "A test tool".to_string(),
        input_schema: json!({
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
    assert_eq!(parsed.category, tool.category);
}

#[tokio::test]
async fn test_mcp_resource_serialization() {
    let resource = McpResource {
        uri: "test://resource".to_string(),
        title: None,
        name: "Test Resource".to_string(),
        description: "A test resource".to_string(),
        mime_type: Some("text/plain".to_string()),
        annotations: None,
        size: None,
    };

    let json = serde_json::to_string(&resource).unwrap();
    let parsed: McpResource = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.uri, resource.uri);
    assert_eq!(parsed.name, resource.name);
}

#[tokio::test]
async fn test_mcp_prompt_serialization() {
    let prompt = McpPrompt {
        name: "test_prompt".to_string(),
        description: "A test prompt".to_string(),
        arguments: None,
        title: None,
        annotations: None,
    };

    let json = serde_json::to_string(&prompt).unwrap();
    let parsed: McpPrompt = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.name, prompt.name);
    assert_eq!(parsed.description, prompt.description);
}

#[tokio::test]
async fn test_server_capabilities() {
    let caps = McpServerCapabilities {
        tools: Some(McpToolsCapability::default()),
        resources: Some(McpResourcesCapability::default()),
        prompts: Some(McpPromptsCapability::default()),
        extensions: None,
        logging: None,
        completions: None,
        tasks: None,
    };

    let json = serde_json::to_string(&caps).unwrap();
    let parsed: McpServerCapabilities = serde_json::from_str(&json).unwrap();

    assert!(parsed.tools.is_some());
    assert!(parsed.resources.is_some());
    assert!(parsed.prompts.is_some());
}

#[tokio::test]
async fn test_mcp_error_display() {
    let err = McpError::ToolNotFound("my_tool".to_string());
    assert!(err.to_string().contains("Tool not found"));
    assert!(err.to_string().contains("my_tool"));

    let err = McpError::InvalidRequest("bad params".to_string());
    assert!(err.to_string().contains("Invalid request"));
}

#[tokio::test]
async fn test_server_creation() {
    let server = McpServer::default_config("test-server");
    assert_eq!(server.config.server_name, "test-server");
    assert!(server.config.enable_tools);
}

#[tokio::test]
async fn test_server_registration() {
    let server = McpServer::default_config("test-server");

    // Register a resource - this should not panic
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

    // Register a prompt - this should not panic
    server
        .register_prompt("test-prompt", "A test prompt", |_args| {
            Ok(vec![McpPromptMessage {
                role: "user".to_string(),
                content: McpPromptContent::Text {
                    text: "Test".to_string(),
                },
            }])
        })
        .await;
}

#[tokio::test]
async fn test_mock_server() {
    let server = MockMcpServer::default()
        .add_tool(helpers::create_test_tool("test_tool", "A test tool"))
        .add_resource(helpers::create_test_resource("test://uri", "Test Resource"));

    // Test initialize
    let result = server.handle_request("initialize", None).unwrap();
    assert_eq!(result["serverInfo"]["name"], "mock-server");

    // Test list tools
    let result = server.handle_request("tools/list", None).unwrap();
    assert_eq!(result["tools"].as_array().unwrap().len(), 1);

    // Test list resources
    let result = server.handle_request("resources/list", None).unwrap();
    assert_eq!(result["resources"].as_array().unwrap().len(), 1);

    // Test call tool
    let result = server
        .handle_request("tools/call", Some(json!({"name": "test_tool"})))
        .unwrap();
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("test_tool"));

    // Test unknown method
    let result = server.handle_request("unknown_method", None);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), McpError::MethodNotFound(_)));
}

#[tokio::test]
async fn test_mcp_content_variants() {
    let text = McpContent::Text {
        text: "Hello".to_string(),
    };
    let json = serde_json::to_string(&text).unwrap();
    assert!(json.contains("\"type\":\"text\""));

    let image = McpContent::Image {
        data: "base64data".to_string(),
        mime_type: "image/png".to_string(),
    };
    let json = serde_json::to_string(&image).unwrap();
    assert!(json.contains("\"type\":\"image\""));

    let resource = McpContent::Resource {
        uri: "file:///test".to_string(),
        mime_type: Some("text/plain".to_string()),
    };
    let json = serde_json::to_string(&resource).unwrap();
    assert!(json.contains("\"type\":\"resource\""));
}

#[tokio::test]
async fn test_client_creation() {
    let client = McpClient::default_config();
    assert_eq!(client.config.client_name, "rustycode-mcp-client");
    assert_eq!(client.config.timeout_secs, 30);
    assert!(!client.is_connected("nonexistent").await);
}

#[tokio::test]
async fn test_prompt_content_variants() {
    let text_content = McpPromptContent::Text {
        text: "Hello, World!".to_string(),
    };
    let json = serde_json::to_string(&text_content).unwrap();
    assert!(json.contains("\"type\":\"text\""));

    let image_content = McpPromptContent::Image {
        data: "base64data".to_string(),
        mime_type: "image/png".to_string(),
    };
    let json = serde_json::to_string(&image_content).unwrap();
    assert!(json.contains("\"type\":\"image\""));

    let resource_content = McpPromptContent::Resource {
        uri: "file:///test".to_string(),
        mime_type: "text/plain".to_string(),
    };
    let json = serde_json::to_string(&resource_content).unwrap();
    assert!(json.contains("\"type\":\"resource\""));
}

#[tokio::test]
async fn test_tool_result() {
    let result = McpToolResult {
        content: vec![
            McpContent::Text {
                text: "Line 1".to_string(),
            },
            McpContent::Text {
                text: "Line 2".to_string(),
            },
        ],
        is_error: Some(false),
        structured_content: None,
        meta: None,
    };

    let json = serde_json::to_string(&result).unwrap();
    let parsed: McpToolResult = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.content.len(), 2);
    assert_eq!(parsed.is_error, Some(false));
}

#[tokio::test]
async fn test_resource_contents() {
    let contents = McpResourceContents {
        uri: "file:///test.txt".to_string(),
        contents: vec![McpContent::Text {
            text: "File contents".to_string(),
        }],
    };

    let json = serde_json::to_string(&contents).unwrap();
    let parsed: McpResourceContents = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.uri, "file:///test.txt");
    assert_eq!(parsed.contents.len(), 1);
}
