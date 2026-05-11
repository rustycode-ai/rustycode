//! Enhanced message types with rich content support
//!
//! This module provides message types that support:
//! - Multiple content parts (text, images, tool calls, etc.)
//! - Rich metadata (tokens, costs, model info)
//! - Efficient token estimation
//! - Serialization-friendly structure

use chrono::{DateTime, Utc};
use rustycode_protocol::estimate_tokens;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique message identifier
pub type MessageId = String;

/// Enhanced message with rich content types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: MessageId,

    /// Message role (user, assistant, system, tool)
    pub role: MessageRole,

    /// Content parts (can be multiple parts per message)
    pub parts: Vec<MessagePart>,

    /// Message timestamp
    pub timestamp: DateTime<Utc>,

    /// Message metadata
    pub metadata: MessageMetadata,
}

impl Message {
    /// Create a new user message with text content
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                content: content.into(),
            }],
            timestamp: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    /// Create a new assistant message with text content
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Text {
                content: content.into(),
            }],
            timestamp: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    /// Create a new system message with text content
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::System,
            parts: vec![MessagePart::Text {
                content: content.into(),
            }],
            timestamp: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    /// Create a new tool result message
    pub fn tool_result(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Tool,
            parts: vec![MessagePart::ToolResult {
                tool_call_id: tool_call_id.into(),
                content: content.into(),
                is_error,
            }],
            timestamp: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    /// Add a content part to the message
    pub fn add_part(&mut self, part: MessagePart) {
        self.parts.push(part);
    }

    /// Get the primary text content of the message
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| {
                if let MessagePart::Text { content } = part {
                    Some(content.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Estimate token count for this message
    ///
    /// This is a rough approximation based on character count.
    /// Actual token counts depend on the model used.
    pub fn estimate_tokens(&self) -> usize {
        self.parts
            .iter()
            .map(|part| match part {
                MessagePart::Text { content } => estimate_tokens(content),
                MessagePart::ToolCall { input, .. } => estimate_tokens(&input.to_string()),
                MessagePart::ToolResult { content, .. } => estimate_tokens(content),
                MessagePart::Reasoning { content } => estimate_tokens(content),
                MessagePart::Image { .. } => 85,
                MessagePart::File { .. } => 50,
                MessagePart::Code { code, .. } => estimate_tokens(code),
                MessagePart::Diff {
                    old_string,
                    new_string,
                    ..
                } => estimate_tokens(old_string) + estimate_tokens(new_string),
                #[allow(unreachable_patterns)]
                _ => 0,
            })
            .sum()
    }

    /// Get the character count of all text content
    pub fn char_count(&self) -> usize {
        self.parts
            .iter()
            .map(|part| match part {
                MessagePart::Text { content } => content.len(),
                MessagePart::ToolCall { input, .. } => input.to_string().len(),
                MessagePart::ToolResult { content, .. } => content.len(),
                MessagePart::Reasoning { content } => content.len(),
                MessagePart::Image { .. } => 0,
                MessagePart::File { .. } => 0,
                MessagePart::Code { code, .. } => code.len(),
                MessagePart::Diff {
                    old_string,
                    new_string,
                    ..
                } => old_string.len() + new_string.len(),
                #[allow(unreachable_patterns)]
                _ => 0,
            })
            .sum()
    }

    /// Check if message contains tool calls
    pub fn has_tool_calls(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, MessagePart::ToolCall { .. }))
    }

    /// Check if message contains images
    pub fn has_images(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, MessagePart::Image { .. }))
    }

    /// Check if message contains code blocks
    pub fn has_code(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, MessagePart::Code { .. }))
    }

    /// Set token count metadata
    pub const fn with_tokens(mut self, tokens: usize) -> Self {
        self.metadata.tokens = Some(tokens);
        self
    }

    /// Set cost metadata
    pub const fn with_cost(mut self, cost: f64) -> Self {
        self.metadata.cost = Some(cost);
        self
    }

    /// Set model metadata
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.metadata.model = Some(model.into());
        self
    }

    /// Set provider metadata
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.metadata.provider = Some(provider.into());
        self
    }

    /// Mark as cached
    pub const fn with_cached(mut self, cached: bool) -> Self {
        self.metadata.cached = cached;
        self
    }
}

/// Message role
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

impl MessageRole {
    /// Get role as string
    pub const fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Tool => "tool",
            #[allow(unreachable_patterns)]
            _ => "unknown",
        }
    }

    /// Parse from string
    pub fn from_role_str(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            "system" => Some(Self::System),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }
}

/// Rich content parts for messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MessagePart {
    /// Plain text content
    Text { content: String },

    /// Tool call
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool result
    ToolResult {
        tool_call_id: String,
        content: String,
        is_error: bool,
    },

    /// Reasoning/thinking content
    Reasoning { content: String },

    /// Image content
    Image {
        url: String,
        alt_text: Option<String>,
    },

    /// File reference
    File {
        url: String,
        filename: String,
        mime_type: String,
    },

    /// Code block
    Code { language: String, code: String },

    /// Diff/change
    Diff {
        filepath: String,
        old_string: String,
        new_string: String,
    },
}

/// Message metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    /// Token count (if known)
    pub tokens: Option<usize>,

    /// Cost in USD (if known)
    pub cost: Option<f64>,

    /// Model used to generate this message
    pub model: Option<String>,

    /// Provider used to generate this message
    pub provider: Option<String>,

    /// Whether this message was cached
    #[serde(default)]
    pub cached: bool,

    /// Additional custom metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, String>,
}

impl MessageMetadata {
    /// Create new metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Add custom metadata
    pub fn with_custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello, world!");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.text(), "Hello, world!");
        assert!(!msg.has_tool_calls());
        assert!(!msg.has_images());
    }

    #[test]
    fn test_message_with_multiple_parts() {
        let mut msg = Message::user("Check this code:");
        msg.add_part(MessagePart::Code {
            language: "rust".to_string(),
            code: "fn main() {}".to_string(),
        });

        assert_eq!(msg.parts.len(), 2);
        assert!(msg.has_code());
    }

    #[test]
    fn test_token_estimation() {
        let msg = Message::user("Hello, world!");
        let tokens = msg.estimate_tokens();
        assert!(tokens > 0 && tokens < 20);
    }

    #[test]
    fn test_message_metadata() {
        let msg = Message::assistant("Response")
            .with_tokens(10)
            .with_cost(0.001)
            .with_model("claude-3")
            .with_provider("anthropic")
            .with_cached(true);

        assert_eq!(msg.metadata.tokens, Some(10));
        assert_eq!(msg.metadata.cost, Some(0.001));
        assert_eq!(msg.metadata.model, Some("claude-3".to_string()));
        assert_eq!(msg.metadata.provider, Some("anthropic".to_string()));
        assert!(msg.metadata.cached);
    }

    #[test]
    fn test_tool_call_message() {
        let msg = Message::tool_result("call_123", "Result", false);
        assert_eq!(msg.role, MessageRole::Tool);
    }

    #[test]
    fn test_message_role_from_str() {
        assert_eq!(MessageRole::from_role_str("user"), Some(MessageRole::User));
        assert_eq!(
            MessageRole::from_role_str("assistant"),
            Some(MessageRole::Assistant)
        );
        assert_eq!(MessageRole::from_role_str("invalid"), None);
    }

    // --- Serialization roundtrip tests ---

    #[test]
    fn test_message_role_serde_roundtrip() {
        for role in [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::System,
            MessageRole::Tool,
        ] {
            let json = serde_json::to_string(&role).unwrap();
            let de: MessageRole = serde_json::from_str(&json).unwrap();
            assert_eq!(role, de);
        }
    }

    #[test]
    fn test_message_role_json_values() {
        let json = serde_json::to_string(&MessageRole::User).unwrap();
        assert_eq!(json, "\"User\"");
        let json = serde_json::to_string(&MessageRole::Tool).unwrap();
        assert_eq!(json, "\"Tool\"");
    }

    #[test]
    fn test_message_part_text_serde_roundtrip() {
        let part = MessagePart::Text {
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_tool_call_serde_roundtrip() {
        let part = MessagePart::ToolCall {
            id: "call_1".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({"cmd": "ls"}),
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_tool_result_serde_roundtrip() {
        let part = MessagePart::ToolResult {
            tool_call_id: "call_1".to_string(),
            content: "output".to_string(),
            is_error: false,
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_reasoning_serde_roundtrip() {
        let part = MessagePart::Reasoning {
            content: "thinking...".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_image_serde_roundtrip() {
        let part = MessagePart::Image {
            url: "https://example.com/img.png".to_string(),
            alt_text: Some("a diagram".to_string()),
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_image_no_alt_text_serde_roundtrip() {
        let part = MessagePart::Image {
            url: "https://example.com/img.png".to_string(),
            alt_text: None,
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_file_serde_roundtrip() {
        let part = MessagePart::File {
            url: "file:///tmp/a.rs".to_string(),
            filename: "a.rs".to_string(),
            mime_type: "text/rust".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_code_serde_roundtrip() {
        let part = MessagePart::Code {
            language: "rust".to_string(),
            code: "fn main() {}".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_part_diff_serde_roundtrip() {
        let part = MessagePart::Diff {
            filepath: "src/main.rs".to_string(),
            old_string: "old".to_string(),
            new_string: "new".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        let de: MessagePart = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&de).unwrap());
    }

    #[test]
    fn test_message_user_serde_roundtrip() {
        let msg = Message::user("Hello, world!");
        let json = serde_json::to_string(&msg).unwrap();
        let de: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(de.role, MessageRole::User);
        assert_eq!(de.text(), "Hello, world!");
    }

    #[test]
    fn test_message_assistant_serde_roundtrip() {
        let msg = Message::assistant("Hi there!");
        let json = serde_json::to_string(&msg).unwrap();
        let de: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(de.role, MessageRole::Assistant);
        assert_eq!(de.text(), "Hi there!");
    }

    #[test]
    fn test_message_system_serde_roundtrip() {
        let msg = Message::system("You are a helpful assistant.");
        let json = serde_json::to_string(&msg).unwrap();
        let de: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(de.role, MessageRole::System);
    }

    #[test]
    fn test_message_tool_result_serde_roundtrip() {
        let msg = Message::tool_result("call_42", "done", false);
        let json = serde_json::to_string(&msg).unwrap();
        let de: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(de.role, MessageRole::Tool);
    }

    #[test]
    fn test_message_with_full_metadata_serde_roundtrip() {
        let msg = Message::assistant("Response")
            .with_tokens(42)
            .with_cost(0.005)
            .with_model("test-model")
            .with_provider("test-provider")
            .with_cached(true);
        let json = serde_json::to_string(&msg).unwrap();
        let de: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(de.metadata.tokens, Some(42));
        assert_eq!(de.metadata.cost, Some(0.005));
        assert_eq!(de.metadata.model, Some("test-model".to_string()));
        assert_eq!(de.metadata.provider, Some("test-provider".to_string()));
        assert!(de.metadata.cached);
    }

    // --- Builder/setter pattern tests ---

    #[test]
    fn test_with_tokens_zero() {
        let msg = Message::user("hi").with_tokens(0);
        assert_eq!(msg.metadata.tokens, Some(0));
    }

    #[test]
    fn test_with_cost_zero() {
        let msg = Message::user("hi").with_cost(0.0);
        assert_eq!(msg.metadata.cost, Some(0.0));
    }

    #[test]
    fn test_with_model_empty_string() {
        let msg = Message::user("hi").with_model("");
        assert_eq!(msg.metadata.model, Some(String::new()));
    }

    #[test]
    fn test_with_provider_empty_string() {
        let msg = Message::user("hi").with_provider("");
        assert_eq!(msg.metadata.provider, Some(String::new()));
    }

    #[test]
    fn test_with_cached_false() {
        let msg = Message::user("hi").with_cached(false);
        assert!(!msg.metadata.cached);
    }

    #[test]
    fn test_message_metadata_new() {
        let meta = MessageMetadata::new();
        assert_eq!(meta.tokens, None);
        assert_eq!(meta.cost, None);
        assert_eq!(meta.model, None);
        assert_eq!(meta.provider, None);
        assert!(!meta.cached);
        assert!(meta.custom.is_empty());
    }

    #[test]
    fn test_message_metadata_with_custom() {
        let meta = MessageMetadata::new()
            .with_custom("key1", "val1")
            .with_custom("key2", "val2");
        assert_eq!(meta.custom.get("key1"), Some(&"val1".to_string()));
        assert_eq!(meta.custom.get("key2"), Some(&"val2".to_string()));
    }

    #[test]
    fn test_message_metadata_with_custom_overwrite() {
        let meta = MessageMetadata::new()
            .with_custom("k", "v1")
            .with_custom("k", "v2");
        assert_eq!(meta.custom.get("k"), Some(&"v2".to_string()));
        assert_eq!(meta.custom.len(), 1);
    }

    #[test]
    fn test_message_metadata_custom_skipped_when_empty() {
        let meta = MessageMetadata::new();
        let json = serde_json::to_string(&meta).unwrap();
        // The "custom" field should be omitted when empty
        assert!(!json.contains("custom"));
    }

    // --- Role as_str / from_role_str tests ---

    #[test]
    fn test_message_role_as_str() {
        assert_eq!(MessageRole::User.as_str(), "user");
        assert_eq!(MessageRole::Assistant.as_str(), "assistant");
        assert_eq!(MessageRole::System.as_str(), "system");
        assert_eq!(MessageRole::Tool.as_str(), "tool");
    }

    #[test]
    fn test_message_role_from_role_str_all() {
        assert_eq!(MessageRole::from_role_str("user"), Some(MessageRole::User));
        assert_eq!(
            MessageRole::from_role_str("assistant"),
            Some(MessageRole::Assistant)
        );
        assert_eq!(
            MessageRole::from_role_str("system"),
            Some(MessageRole::System)
        );
        assert_eq!(MessageRole::from_role_str("tool"), Some(MessageRole::Tool));
    }

    #[test]
    fn test_message_role_from_role_str_empty_and_unknown() {
        assert_eq!(MessageRole::from_role_str(""), None);
        assert_eq!(MessageRole::from_role_str("USER"), None);
        assert_eq!(MessageRole::from_role_str("User "), None);
    }

    // --- Edge case tests ---

    #[test]
    fn test_message_empty_parts() {
        let mut msg = Message::user("Hello");
        msg.parts.clear();
        assert_eq!(msg.text(), "");
        assert_eq!(msg.estimate_tokens(), 0);
        assert_eq!(msg.char_count(), 0);
    }

    #[test]
    fn test_message_empty_text_content() {
        let msg = Message::user("");
        assert_eq!(msg.text(), "");
        assert_eq!(msg.estimate_tokens(), 0);
    }

    #[test]
    fn test_message_multiple_text_parts() {
        let mut msg = Message::user("Part 1");
        msg.add_part(MessagePart::Text {
            content: "Part 2".to_string(),
        });
        msg.add_part(MessagePart::Text {
            content: "Part 3".to_string(),
        });
        assert_eq!(msg.text(), "Part 1\nPart 2\nPart 3");
    }

    #[test]
    fn test_message_mixed_parts_get_text_only_returns_text() {
        let mut msg = Message::user("Hello");
        msg.add_part(MessagePart::ToolCall {
            id: "1".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({}),
        });
        msg.add_part(MessagePart::Text {
            content: "World".to_string(),
        });
        // get_text should only return text parts
        assert_eq!(msg.text(), "Hello\nWorld");
    }

    #[test]
    fn test_message_has_tool_calls_negative() {
        let msg = Message::user("Hello");
        assert!(!msg.has_tool_calls());
    }

    #[test]
    fn test_message_has_tool_calls_positive() {
        let mut msg = Message::assistant("Using tool");
        msg.add_part(MessagePart::ToolCall {
            id: "1".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({}),
        });
        assert!(msg.has_tool_calls());
    }

    #[test]
    fn test_message_has_images_negative() {
        let msg = Message::user("Hello");
        assert!(!msg.has_images());
    }

    #[test]
    fn test_message_has_images_positive() {
        let mut msg = Message::user("Look at this");
        msg.add_part(MessagePart::Image {
            url: "https://example.com/img.png".to_string(),
            alt_text: None,
        });
        assert!(msg.has_images());
    }

    #[test]
    fn test_message_has_code_negative() {
        let msg = Message::user("Hello");
        assert!(!msg.has_code());
    }

    #[test]
    fn test_message_has_code_positive() {
        let mut msg = Message::assistant("Here is code:");
        msg.add_part(MessagePart::Code {
            language: "rust".to_string(),
            code: "fn main() {}".to_string(),
        });
        assert!(msg.has_code());
    }

    #[test]
    fn test_estimate_tokens_image() {
        let mut msg = Message::user("Analyze this");
        msg.add_part(MessagePart::Image {
            url: "https://example.com/img.png".to_string(),
            alt_text: None,
        });
        let tokens = msg.estimate_tokens();
        // image contributes ~85 tokens
        assert!(tokens >= 85);
    }

    #[test]
    fn test_estimate_tokens_file() {
        let mut msg = Message::user("Read this file");
        msg.add_part(MessagePart::File {
            url: "file:///tmp/a.rs".to_string(),
            filename: "a.rs".to_string(),
            mime_type: "text/rust".to_string(),
        });
        let tokens = msg.estimate_tokens();
        // file reference contributes ~50 tokens
        assert!(tokens >= 50);
    }

    #[test]
    fn test_estimate_tokens_diff() {
        let mut msg = Message::assistant("Here is a change:");
        msg.add_part(MessagePart::Diff {
            filepath: "a.rs".to_string(),
            old_string: "old code here".to_string(),
            new_string: "new code here".to_string(),
        });
        let tokens = msg.estimate_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_char_count_text() {
        let msg = Message::user("Hello, world!");
        assert_eq!(msg.char_count(), 13);
    }

    #[test]
    fn test_char_count_tool_call() {
        let mut msg = Message::assistant("Running tool");
        msg.add_part(MessagePart::ToolCall {
            id: "1".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        });
        assert!(msg.char_count() > 0);
    }

    #[test]
    fn test_char_count_image_zero() {
        let mut msg = Message::user("Check image");
        msg.add_part(MessagePart::Image {
            url: "https://example.com/img.png".to_string(),
            alt_text: Some("alt".to_string()),
        });
        // image contributes 0 to char_count, only text parts count
        assert_eq!(msg.char_count(), "Check image".len());
    }

    #[test]
    fn test_char_count_file_zero() {
        let mut msg = Message::user("Read file");
        msg.add_part(MessagePart::File {
            url: "file:///tmp/a.rs".to_string(),
            filename: "a.rs".to_string(),
            mime_type: "text/rust".to_string(),
        });
        // file contributes 0 to char_count
        assert_eq!(msg.char_count(), 9); // "Read file" length
    }

    #[test]
    fn test_tool_result_is_error_true() {
        let msg = Message::tool_result("call_1", "Error occurred", true);
        assert_eq!(msg.role, MessageRole::Tool);
        assert!(msg
            .parts
            .iter()
            .any(|p| matches!(p, MessagePart::ToolResult { is_error: true, .. })));
    }

    #[test]
    fn test_message_id_is_uuid() {
        let msg = Message::user("test");
        // Verify it parses as a UUID
        assert!(uuid::Uuid::parse_str(&msg.id).is_ok());
    }

    #[test]
    fn test_add_part_returns_unit() {
        let mut msg = Message::user("test");
        msg.add_part(MessagePart::Text {
            content: "extra".to_string(),
        });
        assert_eq!(msg.parts.len(), 2);
    }
}
