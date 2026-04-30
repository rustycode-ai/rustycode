#![allow(clippy::unwrap_used)]
//! Tests for ACP types serialization and deserialization

use rustycode_acp::{
    AgentCapabilities, ClientInfo, ContentBlock, ContentPart, InitializeRequest,
    InitializeResponse, JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpServerConfig, Model,
    ModelConfig, NewSessionRequest, NewSessionResponse, PromptMessage, PromptRequest,
    PromptResponse, RequestId, ServerInfo, SessionState, StopReason, ToolCapabilities,
};
use std::collections::HashMap;

#[test]
fn request_id_serialization() {
    // Numeric ID
    let num_id = RequestId::Num(42);
    let json = serde_json::to_string(&num_id).unwrap();
    assert_eq!(json, "42");
    let parsed: RequestId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, num_id);

    // String ID
    let str_id = RequestId::Str("abc-123".to_string());
    let json = serde_json::to_string(&str_id).unwrap();
    assert_eq!(json, "\"abc-123\"");
    let parsed: RequestId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, str_id);
}

#[test]
fn json_rpc_request_serialization() {
    let req: JsonRpcRequest<InitializeRequest> = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: RequestId::Num(1),
        method: "initialize".to_string(),
        params: None,
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"initialize\""));
    assert!(json.contains("\"id\":1"));
}

#[test]
fn json_rpc_response_serialization() {
    let resp: JsonRpcResponse<String> = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: RequestId::Num(1),
        result: Some("success".to_string()),
        error: None,
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"result\":\"success\""));
    assert!(!json.contains("\"error\""));
}

#[test]
fn json_rpc_error_serialization() {
    let error = JsonRpcError {
        code: -32600,
        message: "Invalid Request".to_string(),
        data: None,
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"code\":-32600"));
    assert!(json.contains("\"message\":\"Invalid Request\""));
}

#[test]
fn initialize_request_serialization() {
    let req = InitializeRequest {
        protocol_version: 1,
        capabilities: None,
        client: None,
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"protocol_version\":1"));
}

#[test]
fn initialize_request_with_capabilities() {
    let caps = AgentCapabilities {
        tools: ToolCapabilities {
            list: true,
            call: true,
        },
        modes: None,
        models: None,
        features: None,
    };

    let req = InitializeRequest {
        protocol_version: 1,
        capabilities: Some(caps),
        client: Some(ClientInfo {
            name: "test-client".to_string(),
            version: "1.0.0".to_string(),
            description: None,
        }),
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"protocol_version\":1"));
    assert!(json.contains("\"tools\""));
    assert!(json.contains("\"list\":true"));
    assert!(json.contains("\"call\":true"));
}

#[test]
fn initialize_response_serialization() {
    let resp = InitializeResponse {
        protocol_version: 1,
        capabilities: AgentCapabilities {
            tools: ToolCapabilities {
                list: true,
                call: true,
            },
            modes: None,
            models: None,
            features: None,
        },
        server: ServerInfo {
            name: "rustycode".to_string(),
            version: "0.1.0".to_string(),
            description: Some("Token-optimized CLI proxy".to_string()),
        },
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"name\":\"rustycode\""));
    assert!(json.contains("\"version\":\"0.1.0\""));
}

#[test]
fn session_state_creation() {
    let state = SessionState::new("/tmp/test".to_string());
    assert!(!state.id.is_empty());
    assert_eq!(state.cwd, "/tmp/test");
    assert!(state.model.is_none());
    assert!(state.mode.is_none());
    assert!(state.mcp_servers.is_empty());
}

#[test]
fn new_session_request_serialization() {
    let req = NewSessionRequest {
        cwd: Some("/home/user/project".to_string()),
        model: Some(ModelConfig {
            provider_id: "anthropic".to_string(),
            model_id: "claude-sonnet-4-5".to_string(),
        }),
        mode: Some("default".to_string()),
        mcp_servers: None,
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"cwd\":\"/home/user/project\""));
    assert!(json.contains("\"provider_id\":\"anthropic\""));
}

#[test]
fn new_session_response_serialization() {
    let resp = NewSessionResponse {
        session_id: "sess-123".to_string(),
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert_eq!(json, r#"{"session_id":"sess-123"}"#);
}

#[test]
fn mcp_server_config_serialization() {
    let mut env = HashMap::new();
    env.insert("API_KEY".to_string(), "secret".to_string());

    let config = McpServerConfig {
        name: "filesystem".to_string(),
        command: "npx".to_string(),
        args: Some(vec!["@modelcontextprotocol/server-fs".to_string()]),
        env: Some(env),
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"name\":\"filesystem\""));
    assert!(json.contains("\"command\":\"npx\""));
}

#[test]
fn prompt_message_serialization() {
    let user_msg = PromptMessage::User {
        parts: vec![ContentPart::Text {
            text: "Hello".to_string(),
        }],
    };

    let json = serde_json::to_string(&user_msg).unwrap();
    assert!(json.contains("\"role\":\"user\""));
    assert!(json.contains("\"text\":\"Hello\""));

    let assistant_msg = PromptMessage::Assistant {
        parts: vec![ContentPart::Text {
            text: "Hi there".to_string(),
        }],
    };

    let json = serde_json::to_string(&assistant_msg).unwrap();
    assert!(json.contains("\"role\":\"assistant\""));
}

#[test]
fn content_block_serialization() {
    let block = ContentBlock {
        role: "user".to_string(),
        content: "Fix the bug".to_string(),
    };

    let json = serde_json::to_string(&block).unwrap();
    assert_eq!(json, r#"{"role":"user","content":"Fix the bug"}"#);
}

#[test]
fn prompt_request_serialization() {
    let req = PromptRequest {
        session_id: "sess-123".to_string(),
        messages: vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "Test".to_string(),
            }],
        }],
        user_message: Some(ContentBlock {
            role: "user".to_string(),
            content: "Hello".to_string(),
        }),
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"session_id\":\"sess-123\""));
    assert!(json.contains("\"role\":\"user\""));
}

#[test]
fn stop_reason_serialization() {
    // End turn
    let stop = StopReason::EndTurn;
    let json = serde_json::to_string(&stop).unwrap();
    assert_eq!(json, r#"{"reason":"end_turn"}"#);

    // Max tokens
    let stop = StopReason::MaxTokens;
    let json = serde_json::to_string(&stop).unwrap();
    assert_eq!(json, r#"{"reason":"max_tokens"}"#);

    // Tool call
    let stop = StopReason::ToolCall;
    let json = serde_json::to_string(&stop).unwrap();
    assert_eq!(json, r#"{"reason":"tool_call"}"#);

    // Error
    let stop = StopReason::Error {
        message: "Rate limited".to_string(),
    };
    let json = serde_json::to_string(&stop).unwrap();
    assert!(json.contains("\"reason\":\"error\""));
    assert!(json.contains("\"message\":\"Rate limited\""));
}

#[test]
fn prompt_response_serialization() {
    let resp = PromptResponse {
        message: None,
        stop_reason: StopReason::EndTurn,
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"reason\":\"end_turn\""));
}

#[test]
fn model_serialization() {
    let model = Model {
        provider_id: "anthropic".to_string(),
        model_id: "claude-sonnet-4-5".to_string(),
        name: "Claude Sonnet 4.5".to_string(),
    };

    let json = serde_json::to_string(&model).unwrap();
    assert!(json.contains("\"provider_id\":\"anthropic\""));
    assert!(json.contains("\"model_id\":\"claude-sonnet-4-5\""));
}
