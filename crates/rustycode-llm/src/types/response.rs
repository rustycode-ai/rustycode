use serde::{Deserialize, Serialize};

use super::config::ThinkingDisplay;

// Re-export Usage from protocol
pub use rustycode_protocol::llm::Usage;

/// A thinking block from an extended thinking response.
/// Must be preserved unchanged for multi-turn conversations with tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// The block type: "thinking" or "redacted_thinking"
    #[serde(rename = "type")]
    pub block_type: String,
    /// The thinking content (empty for redacted_thinking or display: "omitted")
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thinking: String,
    /// The encrypted signature (for round-tripping)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    /// Encrypted data for redacted_thinking blocks
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data: String,
    /// Display mode: "summarized" or "omitted" (Claude adaptive thinking)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<ThinkingDisplay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    /// Model name used for this completion (for tracking and logging)
    pub model: String,
    pub usage: Option<Usage>,
    pub stop_reason: Option<String>,
    /// Citation metadata for search results (when applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<Citation>>,
    /// Thinking blocks for round-tripping in multi-turn conversations with tool use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_blocks: Option<Vec<ThinkingBlock>>,
    /// Parsed structured output when `output_config.format` was `JsonSchema`.
    /// Contains the validated JSON value returned by the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
}

/// Normalize provider-specific stop/finish reasons to Anthropic convention.
///
/// The agent loop checks for `"end_turn"` (model finished naturally) and
/// `"tool_use"` (model wants to call tools). Different providers use
/// different strings — this function maps them to the canonical values.
///
/// | Provider    | Raw value      | Normalized    |
/// |-------------|----------------|---------------|
/// | OpenAI      | `"stop"`       | `"end_turn"`  |
/// | OpenAI      | `"tool_calls"` | `"tool_use"`  |
/// | OpenAI      | `"length"`     | `"max_tokens"`|
/// | Cohere      | `"COMPLETE"`   | `"end_turn"`  |
/// | Cohere      | `"TOOL_CALL"`  | `"tool_use"`  |
/// | Cohere      | `"MAX_TOKENS"` | `"max_tokens"`|
/// | Anthropic   | `"end_turn"`   | unchanged     |
/// | Anthropic   | `"tool_use"`   | unchanged     |
pub fn normalize_stop_reason(reason: Option<&str>) -> Option<String> {
    reason
        .map(|r| match r {
            // OpenAI family
            "stop" => "end_turn",
            "tool_calls" => "tool_use",
            "length" => "max_tokens",
            "content_filter" => "end_turn",
            // Cohere
            "COMPLETE" => "end_turn",
            "TOOL_CALL" => "tool_use",
            "MAX_TOKENS" => "max_tokens",
            // Ollama
            "done" => "end_turn",
            // Gemini
            "STOP" => "end_turn",
            "SAFETY" => "end_turn",
            "RECITATION" => "end_turn",
            "FUNCTION_CALL" => "tool_use",
            "BLOCKLIST" => "end_turn",
            "PROHIBITED_CONTENT" => "end_turn",
            "SPII" => "end_turn",
            "MALFORMED_FUNCTION_CALL" => "end_turn",
            // Anthropic — already correct (end_turn, tool_use, max_tokens)
            other => other,
        })
        .map(|s| s.to_string())
}

/// Citation metadata for search results (RAG applications)
/// When a model cites sources in its response, it provides location information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub source: String,     // The source URL or identifier
    pub title: String,      // The title of the source
    pub cited_text: String, // Exact text being cited
    pub index: u32,         // Index of the cited search result (0-based)
}
