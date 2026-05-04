//! Types for the OpenAI Responses API (`POST /v1/responses`).
//!
//! The Responses API replaces Chat Completions' `messages`/`choices` with
//! typed `input`/`output` arrays. It supports tool calling, streaming via
//! typed SSE events, and server-side conversation state (`previous_response_id`).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// Responses API request body.
#[derive(Serialize, Debug, Clone)]
pub struct ResponsesApiRequest {
    pub model: String,
    pub input: Vec<ResponsesApiInputItem>,
    /// System-level instructions (replaces the `system` message role).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Flat tool definitions (NOT nested under a `function` key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponsesApiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Server-side conversation state (OpenAI only — OpenRouter is stateless).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

/// Tagged input item for the Responses API.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ResponsesApiInputItem {
    /// A user or assistant message.
    #[serde(rename = "message")]
    Message {
        role: String,
        content: ResponsesApiContent,
    },
    /// A previously made function call (round-trip in conversation).
    #[serde(rename = "function_call")]
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    /// The output of a function call.
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
}

/// Content can be a plain string or an array of typed content parts.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ResponsesApiContent {
    /// Plain text content.
    Text(String),
    /// Array of typed content parts.
    Parts(Vec<ResponsesApiContentPart>),
}

impl ResponsesApiContent {
    /// Convenience constructor for plain text.
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }
}

/// Typed content part within a message.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ResponsesApiContentPart {
    /// Text content part.
    #[serde(rename = "input_text")]
    InputText { text: String },
    /// Image content part (URL).
    #[serde(rename = "input_image")]
    InputImage { image_url: String },
}

/// Flat tool definition for the Responses API.
///
/// Unlike Chat Completions which nests under `{type, function: {name, ...}}`,
/// the Responses API uses a flat format: `{type, name, description, parameters}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResponsesApiTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

/// Responses API response body.
#[derive(Deserialize, Debug, Clone)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub model: String,
    pub output: Vec<ResponsesApiOutputItem>,
    #[serde(default)]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponsesApiUsage>,
}

/// Tagged output item from the Responses API.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ResponsesApiOutputItem {
    /// An assistant message with content parts.
    #[serde(rename = "message")]
    Message {
        #[serde(default)]
        id: String,
        #[serde(default)]
        role: String,
        #[serde(default)]
        content: Vec<ResponsesApiOutputContent>,
    },
    /// A function call made by the model.
    #[serde(rename = "function_call")]
    FunctionCall {
        #[serde(default)]
        id: String,
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        arguments: String,
    },
}

/// Content part within an output message.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ResponsesApiOutputContent {
    /// Text output.
    #[serde(rename = "output_text")]
    OutputText { #[serde(default)] text: String },
}

/// Token usage from the Responses API.
#[derive(Deserialize, Debug, Clone)]
pub struct ResponsesApiUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

use crate::provider::{CompletionResponse, Usage, normalize_stop_reason, ProviderError};

/// Build a `CompletionResponse` from a Responses API response.
///
/// Extracts text content, tool calls, and usage from the typed `output` array.
pub fn build_responses_completion_response(
    resp: &ResponsesApiResponse,
) -> Result<CompletionResponse, ProviderError> {
    use rustycode_protocol::message::ContentBlock as ProtoContentBlock;

    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<ProtoContentBlock> = Vec::new();

    for item in &resp.output {
        match item {
            ResponsesApiOutputItem::Message { content, .. } => {
                for part in content {
                    match part {
                        ResponsesApiOutputContent::OutputText { text } => {
                            if !text.is_empty() {
                                text_parts.push(text.clone());
                            }
                        }
                    }
                }
            }
            ResponsesApiOutputItem::FunctionCall {
                id,
                call_id,
                name,
                arguments,
            } => {
                let input: serde_json::Value =
                    serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
                tool_calls.push(ProtoContentBlock::ToolUse {
                    id: if call_id.is_empty() {
                        id.clone()
                    } else {
                        call_id.clone()
                    },
                    name: name.clone(),
                    input,
                });
            }
        }
    }

    let mut content_text = text_parts.join("");

    if !tool_calls.is_empty() {
        let tool_calls_json: Vec<serde_json::Value> = tool_calls
            .iter()
            .map(|tc| match tc {
                ProtoContentBlock::ToolUse {
                    id,
                    name,
                    input,
                } => {
                    let mut m = serde_json::Map::new();
                    m.insert("id".into(), serde_json::Value::String(id.clone()));
                    m.insert(
                        "type".into(),
                        serde_json::Value::String("function".into()),
                    );
                    let mut func = serde_json::Map::new();
                    func.insert("name".into(), serde_json::Value::String(name.clone()));
                    func.insert("arguments".into(), input.clone());
                    m.insert("function".into(), serde_json::Value::Object(func));
                    serde_json::Value::Object(m)
                }
                _ => serde_json::Value::Null,
            })
            .collect();

        if let Ok(formatted) = serde_json::to_string_pretty(&tool_calls_json) {
            if !content_text.is_empty() {
                content_text.push('\n');
            }
            content_text.push_str(&formatted);
        }
    }

    let usage = resp.usage.as_ref().map(|u| Usage {
        input_tokens: u.input_tokens,
        output_tokens: u.output_tokens,
        total_tokens: u.total_tokens,
        cache_read_input_tokens: 0,
        cache_creation_input_tokens: 0,
        reasoning_tokens: None,
    });

    let stop_reason = if !tool_calls.is_empty() {
        Some("tool_use")
    } else {
        match resp.status.as_str() {
            "completed" => Some("stop"),
            "incomplete" => Some("max_tokens"),
            "failed" => Some("error"),
            _ => None,
        }
    };

    Ok(CompletionResponse {
        content: content_text,
        model: resp.model.clone(),
        usage,
        stop_reason: normalize_stop_reason(stop_reason),
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_request_serializes_with_skip_none() {
        let req = ResponsesApiRequest {
            model: "gpt-4o".to_string(),
            input: vec![ResponsesApiInputItem::Message {
                role: "user".to_string(),
                content: ResponsesApiContent::text("hello"),
            }],
            instructions: None,
            tools: None,
            temperature: None,
            max_output_tokens: None,
            stream: None,
            previous_response_id: None,
            tool_choice: None,
            parallel_tool_calls: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("instructions"));
        assert!(!json.contains("tools"));
        assert!(!json.contains("stream"));
        assert!(json.contains("\"model\":\"gpt-4o\""));
        assert!(json.contains("\"type\":\"message\""));
    }

    #[test]
    fn responses_input_item_function_call_roundtrip() {
        let item = ResponsesApiInputItem::FunctionCall {
            id: "fc_123".to_string(),
            call_id: "call_abc".to_string(),
            name: "get_weather".to_string(),
            arguments: r#"{"city":"SF"}"#.to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"function_call\""));
        let parsed: ResponsesApiInputItem = serde_json::from_str(&json).unwrap();
        if let ResponsesApiInputItem::FunctionCall { name, .. } = parsed {
            assert_eq!(name, "get_weather");
        } else {
            panic!("expected FunctionCall variant");
        }
    }

    #[test]
    fn responses_output_deserialize_message() {
        let json = r#"{
            "id": "resp_001",
            "model": "gpt-4o",
            "status": "completed",
            "output": [
                {
                    "type": "message",
                    "id": "msg_001",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "Hello!"}
                    ]
                }
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15}
        }"#;
        let resp: ResponsesApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "resp_001");
        assert_eq!(resp.output.len(), 1);
        assert_eq!(resp.usage.as_ref().unwrap().input_tokens, 10);
    }

    #[test]
    fn responses_output_deserialize_function_call() {
        let json = r#"{
            "id": "resp_002",
            "model": "gpt-4o",
            "status": "completed",
            "output": [
                {
                    "type": "function_call",
                    "id": "fc_1",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"/tmp/x\"}"
                }
            ]
        }"#;
        let resp: ResponsesApiResponse = serde_json::from_str(json).unwrap();
        if let ResponsesApiOutputItem::FunctionCall { name, .. } = &resp.output[0] {
            assert_eq!(name, "read_file");
        } else {
            panic!("expected FunctionCall");
        }
    }

    #[test]
    fn build_responses_completion_response_extracts_text_and_usage() {
        let resp = ResponsesApiResponse {
            id: "resp_003".to_string(),
            model: "gpt-4o".to_string(),
            output: vec![ResponsesApiOutputItem::Message {
                id: "msg_003".to_string(),
                role: "assistant".to_string(),
                content: vec![ResponsesApiOutputContent::OutputText {
                    text: "Hi there".to_string(),
                }],
            }],
            status: "completed".to_string(),
            usage: Some(ResponsesApiUsage {
                input_tokens: 8,
                output_tokens: 3,
                total_tokens: 11,
            }),
        };
        let result = build_responses_completion_response(&resp).unwrap();
        assert_eq!(result.content, "Hi there");
        assert_eq!(result.usage.as_ref().unwrap().input_tokens, 8);
        assert_eq!(result.stop_reason, Some("end_turn".to_string()));
    }

    #[test]
    fn build_responses_completion_response_extracts_tool_calls() {
        let resp = ResponsesApiResponse {
            id: "resp_004".to_string(),
            model: "gpt-4o".to_string(),
            output: vec![
                ResponsesApiOutputItem::FunctionCall {
                    id: "fc_1".to_string(),
                    call_id: "call_1".to_string(),
                    name: "bash".to_string(),
                    arguments: r#"{"command":"ls"}"#.to_string(),
                },
            ],
            status: "completed".to_string(),
            usage: None,
        };
        let result = build_responses_completion_response(&resp).unwrap();
        // Tool calls are serialized into content as JSON (matching Chat Completions behavior)
        assert!(result.content.contains("call_1"));
        assert!(result.content.contains("bash"));
        assert_eq!(result.stop_reason.as_deref(), Some("tool_use"));
    }

    #[test]
    fn flat_tool_serializes_without_function_wrapper() {
        let tool = ResponsesApiTool {
            tool_type: "function".to_string(),
            name: "read_file".to_string(),
            description: Some("Read a file".to_string()),
            parameters: Some(serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}})),
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"name\":\"read_file\""));
        assert!(!json.contains("\"function\":{")); // flat, not nested
    }

    #[test]
    fn content_part_input_text() {
        let part = ResponsesApiContentPart::InputText {
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"input_text\""));
    }
}
