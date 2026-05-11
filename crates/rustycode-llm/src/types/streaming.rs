pub use crate::types::error::ProviderError;
pub use crate::types::response::Citation;
pub use rustycode_protocol::llm::Usage;
use serde::{Deserialize, Serialize};

/// Stream chunk result — provider-agnostic streaming event or error.
///
/// Providers translate their native wire format into `StreamEvent` before
/// returning from `complete_stream()`.
pub use rustycode_protocol::stream_event::StreamEvent;
pub type StreamChunk = Result<StreamEvent, ProviderError>;

/// Server-Sent Events (SSE) for streaming responses
///
/// These event types mirror Claude's SSE event format as documented in:
/// <https://platform.claude.com/docs/en/build-with-claude/streaming>
#[doc(hidden)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
#[non_exhaustive]
#[allow(dead_code)]
pub(crate) enum SSEEvent {
    /// Plain text content (for providers without full SSE support)
    Text { text: String },

    /// message_start - Initial event with message metadata
    MessageStart {
        message_id: String,
        #[serde(rename = "type")]
        message_type: String,
        role: String,
    },

    /// content_block_start - Start of a content block
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockType,
    },

    /// content_block_delta - Incremental update to a content block
    ContentBlockDelta { index: usize, delta: ContentDelta },

    /// content_block_stop - End of a content block
    ContentBlockStop { index: usize },

    /// message_delta - Final message metadata (stop_reason, usage)
    MessageDelta {
        stop_reason: Option<String>,
        usage: Option<Usage>,
    },

    /// message_stop - End of message
    MessageStop,

    /// ping - Keep-alive event
    Ping,

    /// error - Error event
    Error { error_type: String, message: String },

    /// thinking_delta - Extended thinking content (Claude's thinking feature)
    ThinkingDelta { thinking: String },

    /// signature_delta - Extended thinking signature (for verification)
    SignatureDelta { signature: String },
}

/// Content block types in streaming responses
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub(crate) enum ContentBlockType {
    /// Text content block
    #[serde(rename = "text")]
    Text { text: String },

    /// Tool use content block
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Unique identifier for this tool use block
        id: String,
        /// Name of the tool being called
        name: String,
        /// Partial JSON input (streamed incrementally with eager streaming)
        input: Option<serde_json::Value>,
    },

    /// Thinking content block (extended thinking)
    #[serde(rename = "thinking")]
    Thinking { thinking: String },

    /// Tool result content block (for multi-turn conversations)
    #[serde(rename = "tool_result")]
    ToolResult {
        /// ID of the tool_use this result corresponds to
        tool_use_id: String,
        /// Result content
        content: Option<String>,
        /// Is this an error result?
        is_error: Option<bool>,
    },
}

/// Delta types for content block updates
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub(crate) enum ContentDelta {
    /// Text delta
    Text { text: String },

    /// Partial JSON delta (for tool_use with eager streaming)
    PartialJson { partial_json: String },

    /// Thinking delta (extended thinking)
    Thinking { thinking: String },

    /// Signature delta (for extended thinking verification)
    Signature { signature: String },

    /// Citation metadata delta
    Citations { citations: Vec<Citation> },
}

#[allow(dead_code)]
impl SSEEvent {
    /// Check if this event represents content that should be displayed to the user
    pub fn is_content(&self) -> bool {
        matches!(
            self,
            Self::Text { .. } | Self::ContentBlockDelta { .. } | Self::ThinkingDelta { .. }
        )
    }

    /// Extract text content from this event, if any
    #[allow(clippy::collapsible_match)]
    pub fn as_text(&self) -> Option<String> {
        match self {
            Self::Text { text } => Some(text.clone()),
            Self::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::Text { text } => Some(text.clone()),
                _ => None,
            },
            Self::ThinkingDelta { thinking } => Some(thinking.clone()),
            _ => None,
        }
    }

    /// Check if this is a final event (end of stream)
    pub fn is_final(&self) -> bool {
        matches!(self, Self::MessageStop | Self::Error { .. })
    }

    /// Check if this event represents thinking/reasoning content
    pub fn is_thinking(&self) -> bool {
        match self {
            Self::ThinkingDelta { .. } => true,
            Self::ContentBlockDelta { delta, .. } => matches!(delta, ContentDelta::Thinking { .. }),
            _ => false,
        }
    }

    /// Check if this event represents a tool use
    pub fn is_tool_use(&self) -> bool {
        matches!(self, Self::ContentBlockStart { content_block, .. }
            if matches!(content_block, ContentBlockType::ToolUse { .. }))
    }

    /// Extract thinking content, if any
    #[allow(clippy::collapsible_match)]
    pub fn as_thinking(&self) -> Option<String> {
        match self {
            Self::ThinkingDelta { thinking } => Some(thinking.clone()),
            Self::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::Thinking { thinking } => Some(thinking.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Extract tool use info, if any
    #[allow(clippy::collapsible_match)]
    pub fn as_tool_use(&self) -> Option<(String, String)> {
        match self {
            Self::ContentBlockStart { content_block, .. } => match content_block {
                ContentBlockType::ToolUse { id, name, .. } => Some((id.clone(), name.clone())),
                _ => None,
            },
            _ => None,
        }
    }

    /// Create a simple text event (for backward compatibility)
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
}
