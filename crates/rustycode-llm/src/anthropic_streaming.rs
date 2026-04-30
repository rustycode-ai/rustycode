// Anthropic streaming event types according to official spec
// https://platform.claude.com/docs/en/build-with-claude/streaming

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStart {
    pub message: MessageStartMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStartMessage {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub content: Vec<serde_json::Value>, // Content blocks
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: Option<MessageUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockStart {
    pub content_block_start: ContentBlockStartInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockStartInfo {
    pub index: u32,
    pub r#type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockDelta {
    pub delta: ContentBlockDeltaInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockDeltaInfo {
    pub index: u32,
    pub r#type: String,
    /// Text delta (for text content blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Partial JSON delta (for tool_use content blocks with eager streaming)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_json: Option<String>,
    /// Citation metadata for search results (when applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<SearchResultLocation>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockStop {
    pub content_block_stop: ContentBlockStopInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockStopInfo {
    pub index: u32,
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDelta {
    pub delta: MessageDeltaInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeltaInfo {
    pub r#type: String,
    pub stop_reason: Option<String>,
    pub usage: Option<MessageUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStop {
    pub message_stop: MessageStopInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStopInfo {
    pub r#type: String,
    pub stop_reason: Option<String>,
    pub usage: Option<MessageUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub error: ErrorInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub r#type: String,
    pub message: String,
}

/// Ping event for keep-alive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingEvent {
    #[serde(rename = "type")]
    pub ping_type: String, // "ping"
}

/// Citation metadata for search results
/// When Claude cites sources in its response, it provides location information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultLocation {
    #[serde(rename = "type")]
    pub location_type: String, // "search_result_location"
    pub source: String,           // The source URL
    pub title: String,            // The title of the source
    pub cited_text: String,       // Exact text being cited
    pub search_result_index: u32, // 0-based index of the search result
    pub start_block_index: u32,   // Position in content array (start)
    pub end_block_index: u32,     // Position in content array (end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_start_serde() {
        let msg = MessageStart {
            message: MessageStartMessage {
                id: "msg_123".to_string(),
                r#type: "message_start".to_string(),
                role: "assistant".to_string(),
                content: vec![],
                model: "claude-sonnet-4-6".to_string(),
                stop_reason: None,
                usage: Some(MessageUsage {
                    input_tokens: 100,
                    output_tokens: 0,
                }),
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: MessageStart = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message.id, "msg_123");
        assert_eq!(back.message.model, "claude-sonnet-4-6");
        assert!(back.message.usage.is_some());
    }

    #[test]
    fn test_message_usage() {
        let usage = MessageUsage {
            input_tokens: 500,
            output_tokens: 150,
        };
        let json = serde_json::to_string(&usage).unwrap();
        let back: MessageUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.input_tokens, 500);
        assert_eq!(back.output_tokens, 150);
    }

    #[test]
    fn test_content_block_delta_text() {
        let delta = ContentBlockDelta {
            delta: ContentBlockDeltaInfo {
                index: 0,
                r#type: "text_delta".to_string(),
                text: Some("Hello".to_string()),
                partial_json: None,
                citations: None,
            },
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("Hello"));
        assert!(!json.contains("partial_json"));
        let back: ContentBlockDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.delta.text, Some("Hello".to_string()));
        assert!(back.delta.partial_json.is_none());
    }

    #[test]
    fn test_content_block_delta_partial_json() {
        let delta = ContentBlockDelta {
            delta: ContentBlockDeltaInfo {
                index: 1,
                r#type: "input_json_delta".to_string(),
                text: None,
                partial_json: Some("{\"key\": \"val".to_string()),
                citations: None,
            },
        };
        let json = serde_json::to_string(&delta).unwrap();
        let back: ContentBlockDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.delta.partial_json.unwrap(), "{\"key\": \"val");
        assert!(back.delta.text.is_none());
    }

    #[test]
    fn test_message_delta_stop_reason() {
        let delta = MessageDelta {
            delta: MessageDeltaInfo {
                r#type: "message_delta".to_string(),
                stop_reason: Some("end_turn".to_string()),
                usage: Some(MessageUsage {
                    input_tokens: 0,
                    output_tokens: 50,
                }),
            },
        };
        let json = serde_json::to_string(&delta).unwrap();
        let back: MessageDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.delta.stop_reason, Some("end_turn".to_string()));
    }

    #[test]
    fn test_error_event() {
        let err = ErrorEvent {
            error: ErrorInfo {
                r#type: "overloaded_error".to_string(),
                message: "Server is busy".to_string(),
            },
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: ErrorEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.error.r#type, "overloaded_error");
        assert_eq!(back.error.message, "Server is busy");
    }

    #[test]
    fn test_ping_event() {
        let ping = PingEvent {
            ping_type: "ping".to_string(),
        };
        let json = serde_json::to_string(&ping).unwrap();
        assert!(json.contains("\"type\":\"ping\""));
        let back: PingEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ping_type, "ping");
    }

    #[test]
    fn test_search_result_location() {
        let loc = SearchResultLocation {
            location_type: "search_result_location".to_string(),
            source: "https://example.com".to_string(),
            title: "Example".to_string(),
            cited_text: "some text".to_string(),
            search_result_index: 0,
            start_block_index: 1,
            end_block_index: 3,
        };
        let json = serde_json::to_string(&loc).unwrap();
        assert!(json.contains("\"type\":\"search_result_location\""));
        let back: SearchResultLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source, "https://example.com");
        assert_eq!(back.search_result_index, 0);
    }

    #[test]
    fn test_content_block_start() {
        let block = ContentBlockStart {
            content_block_start: ContentBlockStartInfo {
                index: 0,
                r#type: "text".to_string(),
                text: String::new(),
            },
        };
        let json = serde_json::to_string(&block).unwrap();
        let back: ContentBlockStart = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content_block_start.r#type, "text");
    }

    #[test]
    fn test_content_block_stop() {
        let block = ContentBlockStop {
            content_block_stop: ContentBlockStopInfo {
                index: 2,
                r#type: "text".to_string(),
            },
        };
        let json = serde_json::to_string(&block).unwrap();
        let back: ContentBlockStop = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content_block_stop.index, 2);
    }

    #[test]
    fn test_message_stop() {
        let stop = MessageStop {
            message_stop: MessageStopInfo {
                r#type: "message_stop".to_string(),
                stop_reason: Some("end_turn".to_string()),
                usage: None,
            },
        };
        let json = serde_json::to_string(&stop).unwrap();
        let back: MessageStop = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message_stop.r#type, "message_stop");
    }

    #[test]
    fn test_message_start_without_usage() {
        let msg = MessageStart {
            message: MessageStartMessage {
                id: "msg_456".to_string(),
                r#type: "message_start".to_string(),
                role: "assistant".to_string(),
                content: vec![serde_json::json!("hello")],
                model: "claude-haiku-4-5-20251001".to_string(),
                stop_reason: None,
                usage: None,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: MessageStart = serde_json::from_str(&json).unwrap();
        assert!(back.message.usage.is_none());
        assert_eq!(back.message.content.len(), 1);
    }
}
