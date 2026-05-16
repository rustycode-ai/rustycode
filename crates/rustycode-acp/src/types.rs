//! ACP (Agent Client Protocol) types
//!
//! This module defines the types used for ACP protocol communication.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Protocol version
pub const ACP_PROTOCOL_VERSION: u32 = 1;

/// JSON-RPC request wrapper
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<T>,
}

/// JSON-RPC response wrapper
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Tool execution annotations
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ToolAnnotations {
    #[serde(default)]
    pub read_only_hint: bool,
    #[serde(default = "default_true")]
    pub destructive_hint: bool,
    #[serde(default)]
    pub idempotent_hint: bool,
    #[serde(default = "default_true")]
    pub open_world_hint: bool,
}

use rustycode_protocol::default_true;

/// Tool execution result
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolResult {
    pub content: Vec<ContentPart>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none", rename = "structuredContent")]
    pub structured_content: Option<serde_json::Value>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Request identifier
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
#[non_exhaustive]
pub enum RequestId {
    Num(u64),
    Str(String),
}

/// Initialize request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitializeRequest {
    pub protocol_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<AgentCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientInfo>,
}

/// Initialize response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitializeResponse {
    pub protocol_version: u32,
    pub capabilities: AgentCapabilities,
    pub server: ServerInfo,
}

/// Agent capabilities
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentCapabilities {
    pub tools: ToolCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modes: Option<Vec<Mode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<Model>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
}

/// Tool capabilities
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCapabilities {
    pub list: bool,
    pub call: bool,
}

/// Mode option
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mode {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Session update notification
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionUpdateNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: SessionUpdateParams,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateParams {
    pub session_id: String,
    pub update: SessionUpdate,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionUpdate {
    AgentMessageChunk {
        content: ContentPart,
    },
    AgentThoughtChunk {
        content: ContentPart,
    },
    ToolCallStart {
        tool_call_id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolCallUpdate {
        tool_call_id: String,
        status: String,
        content: Vec<ContentPart>,
    },
    ToolCallFinished {
        tool_call_id: String,
        result: serde_json::Value,
    },
    Plan {
        steps: Vec<String>,
    },
    TurnEnd,
}

/// Model option
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Model {
    pub provider_id: String,
    pub model_id: String,
    pub name: String,
}

/// Server info
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Client info
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// New session request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpServerConfig>>,
}

/// Model configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    pub provider_id: String,
    pub model_id: String,
}

/// MCP server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// New session response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewSessionResponse {
    pub session_id: String,
}

/// Load session request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoadSessionRequest {
    pub session_id: String,
}

/// Load session response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoadSessionResponse {
    pub session_id: String,
}

/// Prompt request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptRequest {
    pub session_id: String,
    pub messages: Vec<PromptMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<ContentBlock>,
}

/// Prompt message
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "role")]
#[non_exhaustive]
pub enum PromptMessage {
    #[serde(rename = "user")]
    User { parts: Vec<ContentPart> },
    #[serde(rename = "assistant")]
    Assistant { parts: Vec<ContentPart> },
}

/// Content block
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContentBlock {
    pub role: String,
    pub content: String,
}

/// Content part
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        data: String,
        mime_type: String,
    },
    Audio {
        data: String,
        mime_type: String,
    },
    ResourceLink {
        uri: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        mime_type: String,
    },
    Resource {
        uri: String,
        mime_type: String,
        text: Option<String>,
        blob: Option<String>,
    },
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Prompt response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<PromptMessage>,
    pub stop_reason: StopReason,
}

/// Stop reason
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "reason")]
#[non_exhaustive]
pub enum StopReason {
    #[serde(rename = "end_turn")]
    EndTurn,
    #[serde(rename = "max_tokens")]
    MaxTokens,
    #[serde(rename = "stop_sequence")]
    StopSequence,
    #[serde(rename = "tool_call")]
    ToolCall,
    #[serde(rename = "error")]
    Error { message: String },
}

/// Session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: String,
    pub cwd: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub model: Option<ModelConfig>,
    pub mode: Option<String>,
    pub mcp_servers: Vec<McpServerConfig>,
}

impl SessionState {
    pub fn new(cwd: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            cwd,
            created_at: chrono::Utc::now(),
            model: None,
            mode: None,
            mcp_servers: Vec::new(),
        }
    }
}
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_session_state_new() {
        let state = SessionState::new("/tmp/project".to_string());
        assert_eq!(state.cwd, "/tmp/project");
        assert!(state.model.is_none());
        assert!(state.mode.is_none());
        assert!(state.mcp_servers.is_empty());
        assert!(!state.id.is_empty());
    }

    #[test]
    fn test_json_rpc_request_roundtrip() {
        let req: JsonRpcRequest<InitializeRequest> = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Num(1),
            method: "initialize".to_string(),
            params: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: JsonRpcRequest<InitializeRequest> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.jsonrpc, "2.0");
        assert_eq!(decoded.method, "initialize");
    }

    #[test]
    fn test_request_id_variants() {
        let num = serde_json::to_string(&RequestId::Num(42)).unwrap();
        assert_eq!(num, "42");
        let s = serde_json::to_string(&RequestId::Str("abc".to_string())).unwrap();
        assert_eq!(s, "\"abc\"");
    }

    #[test]
    fn test_json_rpc_error_serialization() {
        let err = JsonRpcError {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32600"));
        assert!(json.contains("Invalid Request"));
    }

    #[test]
    fn test_initialize_response_roundtrip() {
        let resp = InitializeResponse {
            protocol_version: ACP_PROTOCOL_VERSION,
            capabilities: AgentCapabilities {
                tools: ToolCapabilities {
                    list: true,
                    call: true,
                },
                modes: None,
                models: None,
                features: Some(vec!["streaming".to_string()]),
            },
            server: ServerInfo {
                name: "rustycode".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: InitializeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.protocol_version, 1);
        assert!(decoded.capabilities.tools.list);
        assert!(decoded.capabilities.tools.call);
    }

    #[test]
    fn test_prompt_message_roundtrip() {
        let msg = PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "Hello".to_string(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: PromptMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            PromptMessage::User { parts } => {
                assert_eq!(parts.len(), 1);
            }
            _ => panic!("Expected User variant"),
        }
    }

    #[test]
    fn test_stop_reason_variants() {
        let reasons = vec![
            serde_json::to_string(&StopReason::EndTurn).unwrap(),
            serde_json::to_string(&StopReason::MaxTokens).unwrap(),
            serde_json::to_string(&StopReason::ToolCall).unwrap(),
        ];
        for json in &reasons {
            let decoded: StopReason = serde_json::from_str(json).unwrap();
            let re_encoded = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, &re_encoded);
        }
    }

    #[test]
    fn test_content_part_tool_roundtrip() {
        let part = ContentPart::Tool {
            name: "Bash".to_string(),
            input: Some(serde_json::json!({"command": "ls"})),
            id: None,
        };
        let json = serde_json::to_string(&part).unwrap();
        let decoded: ContentPart = serde_json::from_str(&json).unwrap();
        match decoded {
            ContentPart::Tool { name, .. } => assert_eq!(name, "Bash"),
            _ => panic!("Expected Tool variant"),
        }
    }

    #[test]
    fn test_stop_reason_end_turn_roundtrip() {
        let resp = PromptResponse {
            message: None,
            stop_reason: StopReason::EndTurn,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: PromptResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded.stop_reason, StopReason::EndTurn));
    }

    #[test]
    fn test_stop_reason_error_variant() {
        let reason = StopReason::Error {
            message: "timeout".to_string(),
        };
        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("timeout"));
    }

    #[test]
    fn test_session_state_serialization() {
        let state = SessionState::new("/tmp".to_string());
        let json = serde_json::to_string(&state).unwrap();
        let decoded: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cwd, "/tmp");
        assert!(!decoded.id.is_empty());
    }

    #[test]
    fn test_mcp_server_config_roundtrip() {
        let config = McpServerConfig {
            name: "test-server".to_string(),
            command: "npx".to_string(),
            args: Some(vec!["-y".to_string(), "server".to_string()]),
            env: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "test-server");
        assert_eq!(decoded.args.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_new_session_request_roundtrip() {
        let req = NewSessionRequest {
            cwd: Some("/project".to_string()),
            model: Some(ModelConfig {
                provider_id: "anthropic".to_string(),
                model_id: "claude-3".to_string(),
            }),
            mode: Some("code".to_string()),
            mcp_servers: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: NewSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cwd, Some("/project".to_string()));
        assert!(decoded.model.is_some());
    }

    #[test]
    fn test_json_rpc_response_serialization() {
        let resp: JsonRpcResponse<serde_json::Value> = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Num(1),
            result: Some(serde_json::json!({"status": "ok"})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
    }

    #[test]
    fn test_request_id_equality() {
        assert_eq!(RequestId::Num(1), RequestId::Num(1));
        assert_eq!(RequestId::Str("a".into()), RequestId::Str("a".into()));
        assert_ne!(RequestId::Num(1), RequestId::Num(2));
        assert_ne!(RequestId::Num(1), RequestId::Str("1".into()));
    }

    #[test]
    fn test_request_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(RequestId::Num(1));
        set.insert(RequestId::Num(1));
        assert_eq!(set.len(), 1);
        set.insert(RequestId::Str("a".into()));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_json_rpc_error_with_data() {
        let err = JsonRpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: Some(serde_json::json!({"method": "unknown"})),
        };
        let json = serde_json::to_string(&err).unwrap();
        let decoded: JsonRpcError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.code, -32601);
        assert!(decoded.data.is_some());
    }

    #[test]
    fn test_initialize_request_roundtrip() {
        let req = InitializeRequest {
            protocol_version: 1,
            capabilities: Some(AgentCapabilities {
                tools: ToolCapabilities {
                    list: true,
                    call: false,
                },
                modes: None,
                models: None,
                features: None,
            }),
            client: Some(ClientInfo {
                name: "test-client".to_string(),
                version: "1.0".to_string(),
                description: Some("Test client".to_string()),
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: InitializeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.protocol_version, 1);
        assert!(decoded.client.is_some());
        assert!(decoded.capabilities.unwrap().tools.list);
    }

    #[test]
    fn test_agent_capabilities_with_models() {
        let caps = AgentCapabilities {
            tools: ToolCapabilities {
                list: true,
                call: true,
            },
            modes: Some(vec![Mode {
                id: "code".into(),
                name: "Code".into(),
                description: None,
            }]),
            models: Some(vec![Model {
                provider_id: "anthropic".into(),
                model_id: "claude-3".into(),
                name: "Claude 3".into(),
            }]),
            features: None,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: AgentCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.models.unwrap().len(), 1);
        assert_eq!(decoded.modes.unwrap().len(), 1);
    }

    #[test]
    fn test_content_part_resource_roundtrip() {
        let part = ContentPart::Resource {
            uri: "file:///test.txt".to_string(),
            mime_type: "text/plain".to_string(),
            text: Some("content".to_string()),
            blob: None,
        };

        let json = serde_json::to_string(&part).unwrap();
        let decoded: ContentPart = serde_json::from_str(&json).unwrap();
        match decoded {
            ContentPart::Resource { uri, .. } => assert_eq!(uri, "file:///test.txt"),
            _ => panic!("Expected Resource variant"),
        }
    }

    #[test]
    fn test_prompt_message_assistant_roundtrip() {
        let msg = PromptMessage::Assistant {
            parts: vec![
                ContentPart::Text {
                    text: "Hello".to_string(),
                },
                ContentPart::Tool {
                    name: "Read".to_string(),
                    input: None,
                    id: None,
                },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: PromptMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            PromptMessage::Assistant { parts } => assert_eq!(parts.len(), 2),
            _ => panic!("Expected Assistant variant"),
        }
    }

    #[test]
    fn test_content_block_roundtrip() {
        let block = ContentBlock {
            role: "user".to_string(),
            content: "Hello world".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.role, "user");
        assert_eq!(decoded.content, "Hello world");
    }

    #[test]
    fn test_prompt_request_roundtrip() {
        let req = PromptRequest {
            session_id: "sess-123".to_string(),
            messages: vec![PromptMessage::User {
                parts: vec![ContentPart::Text {
                    text: "hi".to_string(),
                }],
            }],
            user_message: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: PromptRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, "sess-123");
        assert_eq!(decoded.messages.len(), 1);
    }

    #[test]
    fn test_load_session_request_roundtrip() {
        let req = LoadSessionRequest {
            session_id: "sess-abc".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: LoadSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, "sess-abc");
    }

    #[test]
    fn test_load_session_response_roundtrip() {
        let resp = LoadSessionResponse {
            session_id: "sess-xyz".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: LoadSessionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, "sess-xyz");
    }

    #[test]
    fn test_new_session_response_roundtrip() {
        let resp = NewSessionResponse {
            session_id: "new-sess".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: NewSessionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, "new-sess");
    }

    #[test]
    fn test_session_state_unique_ids() {
        let s1 = SessionState::new("/a".to_string());
        let s2 = SessionState::new("/a".to_string());
        assert_ne!(s1.id, s2.id);
    }

    #[test]
    fn test_mcp_server_config_with_env() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "secret".to_string());
        let config = McpServerConfig {
            name: "server".to_string(),
            command: "node".to_string(),
            args: None,
            env: Some(env),
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.env.unwrap().len(), 1);
    }

    #[test]
    fn test_model_config_roundtrip() {
        let config = ModelConfig {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "openai");
        assert_eq!(decoded.model_id, "gpt-4");
    }

    #[test]
    fn test_json_rpc_response_with_error() {
        let resp: JsonRpcResponse<serde_json::Value> = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Str("err-1".into()),
            result: None,
            error: Some(JsonRpcError {
                code: -32700,
                message: "Parse error".to_string(),
                data: None,
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: JsonRpcResponse<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert!(decoded.result.is_none());
        assert!(decoded.error.unwrap().code == -32700);
    }

    #[test]
    fn test_new_session_request_minimal() {
        let req = NewSessionRequest {
            cwd: None,
            model: None,
            mode: None,
            mcp_servers: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: NewSessionRequest = serde_json::from_str(&json).unwrap();
        assert!(decoded.cwd.is_none());
        assert!(decoded.model.is_none());
    }

    #[test]
    fn test_stop_sequence_variant() {
        let reason = StopReason::StopSequence;
        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("stop_sequence"));
        let decoded: StopReason = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, StopReason::StopSequence));
    }

    #[test]
    fn test_server_info_with_description() {
        let info = ServerInfo {
            name: "rustycode".to_string(),
            version: "0.2.0".to_string(),
            description: Some("AI coding assistant".to_string()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.description.unwrap(), "AI coding assistant");
    }

    #[test]
    fn test_tool_capabilities_default_fields() {
        let caps = ToolCapabilities {
            list: false,
            call: false,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ToolCapabilities = serde_json::from_str(&json).unwrap();
        assert!(!decoded.list);
        assert!(!decoded.call);
    }

    #[test]
    fn test_agent_capabilities_no_optional_fields() {
        let caps = AgentCapabilities {
            tools: ToolCapabilities {
                list: true,
                call: true,
            },
            modes: None,
            models: None,
            features: None,
        };
        let json = serde_json::to_string(&caps).unwrap();
        assert!(!json.contains("modes"));
        assert!(!json.contains("models"));
        assert!(!json.contains("features"));
    }

    #[test]
    fn test_mode_without_description() {
        let mode = Mode {
            id: "ask".to_string(),
            name: "Ask".to_string(),
            description: None,
        };
        let json = serde_json::to_string(&mode).unwrap();
        assert!(!json.contains("description"));
    }

    #[test]
    fn test_client_info_serialization() {
        let info = ClientInfo {
            name: "test-client".to_string(),
            version: "1.0".to_string(),
            description: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: ClientInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "test-client");
    }

    #[test]
    fn test_content_part_tool_without_input() {
        let part = ContentPart::Tool {
            name: "read".to_string(),
            input: None,
            id: None,
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"tool\""));
        let decoded: ContentPart = serde_json::from_str(&json).unwrap();
        match decoded {
            ContentPart::Tool { name, input, .. } => {
                assert_eq!(name, "read");
                assert!(input.is_none());
            }
            _ => panic!("Expected Tool variant"),
        }
    }

    #[test]
    fn test_content_part_resource_without_data() {
        let part = ContentPart::Resource {
            uri: "file:///x.rs".to_string(),
            mime_type: "text/plain".to_string(),
            text: None,
            blob: None,
        };
        let json = serde_json::to_string(&part).unwrap();
        let decoded: ContentPart = serde_json::from_str(&json).unwrap();
        match decoded {
            ContentPart::Resource { uri, .. } => {
                assert_eq!(uri, "file:///x.rs");
            }
            _ => panic!("Expected Resource variant"),
        }
    }

    #[test]
    fn test_prompt_request_with_user_message() {
        let req = PromptRequest {
            session_id: "s1".to_string(),
            messages: vec![],
            user_message: Some(ContentBlock {
                role: "user".to_string(),
                content: "Fix this bug".to_string(),
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: PromptRequest = serde_json::from_str(&json).unwrap();
        assert!(decoded.user_message.is_some());
    }

    #[test]
    fn test_session_state_with_model_and_mode() {
        let mut state = SessionState::new("/project".to_string());
        state.model = Some(ModelConfig {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3".to_string(),
        });
        state.mode = Some("code".to_string());
        let json = serde_json::to_string(&state).unwrap();
        let decoded: SessionState = serde_json::from_str(&json).unwrap();
        assert!(decoded.model.is_some());
        assert_eq!(decoded.mode.unwrap(), "code");
    }

    #[test]
    fn test_session_state_with_mcp_servers() {
        let mut state = SessionState::new("/project".to_string());
        state.mcp_servers = vec![McpServerConfig {
            name: "fs".to_string(),
            command: "npx".to_string(),
            args: Some(vec!["server".to_string()]),
            env: None,
        }];
        let json = serde_json::to_string(&state).unwrap();
        let decoded: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.mcp_servers.len(), 1);
    }

    #[test]
    fn test_json_rpc_request_string_id() {
        let req: JsonRpcRequest<serde_json::Value> = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Str("req-abc".into()),
            method: "test".to_string(),
            params: Some(serde_json::json!({"key": "val"})),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: JsonRpcRequest<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, RequestId::Str("req-abc".into()));
        assert!(decoded.params.is_some());
    }

    #[test]
    fn test_prompt_response_with_tool_call() {
        let resp = PromptResponse {
            message: None,
            stop_reason: StopReason::ToolCall,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: PromptResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded.stop_reason, StopReason::ToolCall));
    }

    #[test]
    fn test_prompt_response_with_max_tokens() {
        let resp = PromptResponse {
            message: None,
            stop_reason: StopReason::MaxTokens,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("max_tokens"));
    }

    #[test]
    fn test_mcp_server_config_minimal() {
        let config = McpServerConfig {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: None,
            env: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("args"));
        assert!(!json.contains("env"));
    }

    #[test]
    fn test_acp_protocol_version() {
        assert_eq!(ACP_PROTOCOL_VERSION, 1);
    }

    #[test]
    fn test_stop_reason_error_roundtrip() {
        let reason = StopReason::Error {
            message: "model overloaded".to_string(),
        };
        let json = serde_json::to_string(&reason).unwrap();
        let decoded: StopReason = serde_json::from_str(&json).unwrap();
        match decoded {
            StopReason::Error { message } => assert_eq!(message, "model overloaded"),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_new_session_request_with_mcp_servers() {
        let req = NewSessionRequest {
            cwd: Some("/app".to_string()),
            model: None,
            mode: None,
            mcp_servers: Some(vec![McpServerConfig {
                name: "db".to_string(),
                command: "npx".to_string(),
                args: None,
                env: None,
            }]),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: NewSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.mcp_servers.unwrap().len(), 1);
    }

    #[test]
    fn test_model_config_provider_roundtrip() {
        let config = ModelConfig {
            provider_id: "google".to_string(),
            model_id: "gemini-pro".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "google");
        assert_eq!(decoded.model_id, "gemini-pro");
    }

    #[test]
    fn test_agent_capabilities_with_features() {
        let caps = AgentCapabilities {
            tools: ToolCapabilities {
                list: true,
                call: true,
            },
            modes: None,
            models: None,
            features: Some(vec!["streaming".to_string(), "tools".to_string()]),
        };
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: AgentCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.features.unwrap().len(), 2);
    }

    // --- Additional edge-case tests ---

    #[test]
    fn test_deserialize_prompt_message_user_from_raw_json() {
        let raw = r#"{"role":"user","parts":[{"type":"text","text":"Hello, world!"}]}"#;
        let msg: PromptMessage = serde_json::from_str(raw).unwrap();
        match msg {
            PromptMessage::User { parts } => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    ContentPart::Text { text } => assert_eq!(text, "Hello, world!"),
                    _ => panic!("Expected Text part"),
                }
            }
            _ => panic!("Expected User variant"),
        }
    }

    #[test]
    fn test_deserialize_prompt_message_assistant_from_raw_json() {
        let raw = r#"{"role":"assistant","parts":[{"type":"text","text":"Hi!"}]}"#;
        let msg: PromptMessage = serde_json::from_str(raw).unwrap();
        assert!(matches!(msg, PromptMessage::Assistant { .. }));
    }

    #[test]
    fn test_deserialize_content_part_tool_from_raw_json() {
        let raw = r#"{"type":"tool","name":"Bash","input":{"command":"ls -la"}}"#;
        let part: ContentPart = serde_json::from_str(raw).unwrap();
        match part {
            ContentPart::Tool { name, input, .. } => {
                assert_eq!(name, "Bash");
                assert!(input.is_some());
            }
            _ => panic!("Expected Tool variant"),
        }
    }

    #[test]
    fn test_deserialize_content_part_resource_from_raw_json() {
        let raw = r#"{"type":"resource","uri":"file:///src/main.rs","mime_type":"text/plain"}"#;
        let part: ContentPart = serde_json::from_str(raw).unwrap();
        match part {
            ContentPart::Resource { uri, .. } => {
                assert_eq!(uri, "file:///src/main.rs");
            }
            _ => panic!("Expected Resource variant"),
        }
    }

    #[test]
    fn test_deserialize_stop_reason_end_turn_from_raw() {
        let raw = r#"{"reason":"end_turn"}"#;
        let reason: StopReason = serde_json::from_str(raw).unwrap();
        assert!(matches!(reason, StopReason::EndTurn));
    }

    #[test]
    fn test_deserialize_stop_reason_error_from_raw() {
        let raw = r#"{"reason":"error","message":"context length exceeded"}"#;
        let reason: StopReason = serde_json::from_str(raw).unwrap();
        match reason {
            StopReason::Error { message } => assert_eq!(message, "context length exceeded"),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_session_state_created_at_is_recent() {
        let before = chrono::Utc::now();
        let state = SessionState::new("/test".to_string());
        let after = chrono::Utc::now();
        assert!(state.created_at >= before);
        assert!(state.created_at <= after);
    }

    #[test]
    fn test_session_state_created_at_is_utc() {
        let state = SessionState::new("/test".to_string());
        // Verify the timestamp has UTC offset (chrono::Utc always does)
        assert_eq!(state.created_at.timezone(), chrono::Utc);
    }

    #[test]
    fn test_prompt_request_with_multiple_mixed_messages() {
        let req = PromptRequest {
            session_id: "sess-multi".to_string(),
            messages: vec![
                PromptMessage::User {
                    parts: vec![ContentPart::Text {
                        text: "What is 2+2?".to_string(),
                    }],
                },
                PromptMessage::Assistant {
                    parts: vec![ContentPart::Text {
                        text: "4".to_string(),
                    }],
                },
                PromptMessage::User {
                    parts: vec![ContentPart::Text {
                        text: "And 3+3?".to_string(),
                    }],
                },
            ],
            user_message: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: PromptRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.messages.len(), 3);
        assert!(matches!(decoded.messages[0], PromptMessage::User { .. }));
        assert!(matches!(
            decoded.messages[1],
            PromptMessage::Assistant { .. }
        ));
        assert!(matches!(decoded.messages[2], PromptMessage::User { .. }));
    }

    #[test]
    fn test_deserialize_initialize_request_from_raw_json() {
        let raw = r#"{"protocol_version":1}"#;
        let req: InitializeRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.protocol_version, 1);
        assert!(req.capabilities.is_none());
        assert!(req.client.is_none());
    }

    #[test]
    fn test_deserialize_new_session_request_minimal_json() {
        // All optional fields omitted -- should deserialize with all None
        let raw = r#"{}"#;
        let req: NewSessionRequest = serde_json::from_str(raw).unwrap();
        assert!(req.cwd.is_none());
        assert!(req.model.is_none());
        assert!(req.mode.is_none());
        assert!(req.mcp_servers.is_none());
    }

    #[test]
    fn test_deserialize_json_rpc_request_from_raw() {
        let raw =
            r#"{"jsonrpc":"2.0","id":42,"method":"initialize","params":{"protocol_version":1}}"#;
        let req: JsonRpcRequest<InitializeRequest> = serde_json::from_str(raw).unwrap();
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.id, RequestId::Num(42));
        assert_eq!(req.method, "initialize");
        assert!(req.params.is_some());
    }

    #[test]
    fn test_deserialize_json_rpc_request_with_string_id() {
        let raw = r#"{"jsonrpc":"2.0","id":"abc-123","method":"session/new","params":null}"#;
        let req: JsonRpcRequest<serde_json::Value> = serde_json::from_str(raw).unwrap();
        assert_eq!(req.id, RequestId::Str("abc-123".to_string()));
    }

    #[test]
    fn test_deserialize_json_rpc_response_from_raw() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"result":{"session_id":"s1"}}"#;
        let resp: JsonRpcResponse<serde_json::Value> = serde_json::from_str(raw).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_deserialize_json_rpc_error_response_from_raw() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Bad request"}}"#;
        let resp: JsonRpcResponse<serde_json::Value> = serde_json::from_str(raw).unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Bad request");
    }

    #[test]
    fn test_deserialize_prompt_request_from_raw_json() {
        let raw = r#"{"session_id":"s1","messages":[{"role":"user","parts":[{"type":"text","text":"hi"}]}]}"#;
        let req: PromptRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.session_id, "s1");
        assert_eq!(req.messages.len(), 1);
        assert!(req.user_message.is_none());
    }

    #[test]
    fn test_deserialize_load_session_request_from_raw() {
        let raw = r#"{"session_id":"sess-abc"}"#;
        let req: LoadSessionRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.session_id, "sess-abc");
    }

    #[test]
    fn test_deserialize_model_from_raw_json() {
        let raw =
            r#"{"provider_id":"anthropic","model_id":"claude-3-opus","name":"Claude 3 Opus"}"#;
        let model: Model = serde_json::from_str(raw).unwrap();
        assert_eq!(model.provider_id, "anthropic");
        assert_eq!(model.model_id, "claude-3-opus");
        assert_eq!(model.name, "Claude 3 Opus");
    }

    #[test]
    fn test_deserialize_mcp_server_config_full() {
        let raw = r#"{"name":"test","command":"node","args":["--verbose"],"env":{"KEY":"val"}}"#;
        let config: McpServerConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.command, "node");
        assert_eq!(config.args.as_ref().unwrap().len(), 1);
        assert_eq!(config.env.as_ref().unwrap().get("KEY").unwrap(), "val");
    }

    #[test]
    fn test_json_rpc_error_negative_codes() {
        // Standard JSON-RPC error codes are negative
        let codes = [-32700, -32600, -32601, -32602, -32603];
        for &code in &codes {
            let err = JsonRpcError {
                code,
                message: "test".to_string(),
                data: None,
            };
            let json = serde_json::to_string(&err).unwrap();
            let decoded: JsonRpcError = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.code, code);
        }
    }

    #[test]
    fn test_json_rpc_error_with_complex_data() {
        let err = JsonRpcError {
            code: -32000,
            message: "Server error".to_string(),
            data: Some(serde_json::json!({
                "details": ["line 1", "line 2"],
                "nested": {"key": "value"}
            })),
        };
        let json = serde_json::to_string(&err).unwrap();
        let decoded: JsonRpcError = serde_json::from_str(&json).unwrap();
        assert!(decoded.data.is_some());
        let data = decoded.data.unwrap();
        assert!(data.get("details").is_some());
    }

    #[test]
    fn test_prompt_message_user_multiple_parts() {
        let msg = PromptMessage::User {
            parts: vec![
                ContentPart::Text {
                    text: "Read this file".to_string(),
                },
                ContentPart::Resource {
                    uri: "file:///test.txt".to_string(),
                    mime_type: "text/plain".to_string(),
                    text: Some("content".to_string()),
                    blob: None,
                },
                ContentPart::Tool {
                    name: "Bash".to_string(),
                    input: Some(serde_json::json!({"command": "cargo build"})),
                    id: None,
                },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: PromptMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            PromptMessage::User { parts } => assert_eq!(parts.len(), 3),
            _ => panic!("Expected User variant"),
        }
    }

    #[test]
    fn test_prompt_message_user_empty_parts() {
        let msg = PromptMessage::User { parts: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: PromptMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            PromptMessage::User { parts } => assert!(parts.is_empty()),
            _ => panic!("Expected User variant"),
        }
    }

    #[test]
    fn test_request_id_num_zero() {
        let id = RequestId::Num(0);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "0");
        let decoded: RequestId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn test_request_id_str_empty() {
        let id = RequestId::Str(String::new());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"\"");
        let decoded: RequestId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn test_request_id_large_number() {
        let id = RequestId::Num(u64::MAX);
        let json = serde_json::to_string(&id).unwrap();
        let decoded: RequestId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn test_server_info_without_description() {
        let info = ServerInfo {
            name: "test".to_string(),
            version: "1.0".to_string(),
            description: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("description"));
        let decoded: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "test");
    }

    #[test]
    fn test_client_info_with_description_roundtrip() {
        let info = ClientInfo {
            name: "zed".to_string(),
            version: "0.123.0".to_string(),
            description: Some("Zed editor".to_string()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: ClientInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.description.unwrap(), "Zed editor");
    }

    #[test]
    fn test_mode_with_description_roundtrip() {
        let mode = Mode {
            id: "code".to_string(),
            name: "Code".to_string(),
            description: Some("Full code editing mode".to_string()),
        };
        let json = serde_json::to_string(&mode).unwrap();
        let decoded: Mode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.description.unwrap(), "Full code editing mode");
    }

    #[test]
    fn test_mode_roundtrip() {
        let mode = Mode {
            id: "ask".to_string(),
            name: "Ask".to_string(),
            description: None,
        };
        let json = serde_json::to_string(&mode).unwrap();
        let decoded: Mode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "ask");
        assert_eq!(decoded.name, "Ask");
    }

    #[test]
    fn test_model_roundtrip() {
        let model = Model {
            provider_id: "openai".to_string(),
            model_id: "gpt-4-turbo".to_string(),
            name: "GPT-4 Turbo".to_string(),
        };
        let json = serde_json::to_string(&model).unwrap();
        let decoded: Model = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "openai");
        assert_eq!(decoded.model_id, "gpt-4-turbo");
        assert_eq!(decoded.name, "GPT-4 Turbo");
    }

    #[test]
    fn test_deserialize_agent_capabilities_from_raw() {
        let raw = r#"{"tools":{"list":true,"call":true},"modes":[{"id":"code","name":"Code"}],"features":["streaming"]}"#;
        let caps: AgentCapabilities = serde_json::from_str(raw).unwrap();
        assert!(caps.tools.list);
        assert!(caps.tools.call);
        assert_eq!(caps.modes.unwrap().len(), 1);
        assert_eq!(caps.features.unwrap().len(), 1);
        assert!(caps.models.is_none());
    }

    #[test]
    fn test_session_state_id_is_valid_uuid() {
        let state = SessionState::new("/test".to_string());
        // Verify the ID parses as a valid UUID
        let parsed = Uuid::parse_str(&state.id);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_content_block_with_special_characters() {
        let block = ContentBlock {
            role: "user".to_string(),
            content: "Fix \"quotes\" & <html> stuff\nwith newlines\tand tabs".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.content, block.content);
    }

    #[test]
    fn test_initialize_response_protocol_version_matches_constant() {
        let resp = InitializeResponse {
            protocol_version: ACP_PROTOCOL_VERSION,
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
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
        };
        assert_eq!(resp.protocol_version, 1);
    }

    #[test]
    fn test_new_session_request_all_fields() {
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "VAL".to_string());
        let req = NewSessionRequest {
            cwd: Some("/project".to_string()),
            model: Some(ModelConfig {
                provider_id: "anthropic".to_string(),
                model_id: "claude-3".to_string(),
            }),
            mode: Some("code".to_string()),
            mcp_servers: Some(vec![McpServerConfig {
                name: "fs".to_string(),
                command: "npx".to_string(),
                args: Some(vec!["server".to_string()]),
                env: Some(env),
            }]),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: NewSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cwd, Some("/project".to_string()));
        assert!(decoded.model.is_some());
        assert_eq!(decoded.mode, Some("code".to_string()));
        assert_eq!(decoded.mcp_servers.unwrap().len(), 1);
    }

    #[test]
    fn test_json_rpc_request_with_params_roundtrip() {
        let req: JsonRpcRequest<NewSessionRequest> = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Num(7),
            method: "session/new".to_string(),
            params: Some(NewSessionRequest {
                cwd: Some("/test".to_string()),
                model: None,
                mode: None,
                mcp_servers: None,
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: JsonRpcRequest<NewSessionRequest> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.method, "session/new");
        assert!(decoded.params.unwrap().cwd.is_some());
    }

    #[test]
    fn test_json_rpc_response_error_only_no_result() {
        let resp: JsonRpcResponse<serde_json::Value> = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Num(99),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found: foo/bar".to_string(),
                data: None,
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        // Verify result is not serialized when None
        assert!(!json.contains("\"result\""));
        // Verify error is present
        assert!(json.contains("\"error\""));
        assert!(json.contains("foo/bar"));
    }

    #[test]
    fn test_stop_reason_all_variants_from_raw() {
        let cases = vec![
            (r#"{"reason":"end_turn"}"#, "end_turn"),
            (r#"{"reason":"max_tokens"}"#, "max_tokens"),
            (r#"{"reason":"stop_sequence"}"#, "stop_sequence"),
            (r#"{"reason":"tool_call"}"#, "tool_call"),
        ];
        for (raw, expected_tag) in cases {
            let reason: StopReason = serde_json::from_str(raw).unwrap();
            let re_json = serde_json::to_string(&reason).unwrap();
            assert!(
                re_json.contains(expected_tag),
                "Failed for {}",
                expected_tag
            );
        }
    }

    #[test]
    fn test_prompt_request_empty_messages() {
        let req = PromptRequest {
            session_id: "sess-empty".to_string(),
            messages: vec![],
            user_message: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: PromptRequest = serde_json::from_str(&json).unwrap();
        assert!(decoded.messages.is_empty());
    }

    #[test]
    fn test_session_state_serialization_preserves_all_fields() {
        let mut state = SessionState::new("/deep/project".to_string());
        state.model = Some(ModelConfig {
            provider_id: "openai".to_string(),
            model_id: "gpt-4".to_string(),
        });
        state.mode = Some("ask".to_string());
        state.mcp_servers = vec![
            McpServerConfig {
                name: "git".to_string(),
                command: "npx".to_string(),
                args: None,
                env: None,
            },
            McpServerConfig {
                name: "fs".to_string(),
                command: "node".to_string(),
                args: Some(vec!["server.js".to_string()]),
                env: None,
            },
        ];
        let json = serde_json::to_string(&state).unwrap();
        let decoded: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cwd, "/deep/project");
        assert_eq!(decoded.model.unwrap().provider_id, "openai");
        assert_eq!(decoded.mode.unwrap(), "ask");
        assert_eq!(decoded.mcp_servers.len(), 2);
        assert!(!decoded.id.is_empty());
    }
}
