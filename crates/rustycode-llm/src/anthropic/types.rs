//! Anthropic-specific request and response types.
//!
//! These types model the JSON wire format for Anthropic's Messages API.

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Thinking configuration (Opus 4.5+, Sonnet 4.5+)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
    /// Output configuration for structured outputs and effort control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<crate::provider::OutputConfig>,
    /// Skills container for Anthropic Agent Skills API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<serde_json::Value>,
    /// Tool choice: controls how the model selects tools.
    /// None = auto, {"type": "any"} = force tool use, {"type": "tool", "name": "..."} = specific tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    /// Enable parallel tool calls — model may emit multiple tool_use blocks per turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

#[derive(Deserialize)]
pub struct AnthropicResponse {
    pub content: Vec<AnthropicContent>,
    pub usage: AnthropicUsage,
    pub model: String,
    // Ignore extra fields that z.ai or other proxies might add
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    pub id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    pub response_type: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    pub role: Option<String>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    pub stop_sequence: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct AnthropicContent {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(default)]
    pub text: String,
    #[allow(dead_code)] // Kept for future use
    #[serde(default)]
    pub id: String,
    #[allow(dead_code)] // Kept for future use
    #[serde(default)]
    pub name: String,
    #[allow(dead_code)] // Kept for future use
    #[serde(default)]
    pub input: serde_json::Value,
    /// Citations returned by Anthropic API (web search results, etc.)
    #[serde(default)]
    pub citations: Option<Vec<AnthropicCitation>>,
    /// Thinking content from extended thinking blocks
    #[serde(default)]
    pub thinking: String,
    /// Signature for extended thinking blocks (encrypted, for round-tripping)
    #[serde(default)]
    #[allow(dead_code)]
    pub signature: String,
    /// Encrypted data for redacted_thinking blocks (for round-tripping)
    #[serde(default)]
    pub data: String,
}

/// Citation within an Anthropic content block.
#[derive(Deserialize, Debug)]
#[allow(dead_code)] // Kept for future use
pub struct AnthropicCitation {
    #[serde(rename = "type")]
    pub citation_type: String,
    #[serde(default)]
    pub cited_text: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub search_result_index: Option<u32>,
}

#[derive(Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,

    /// Cache-aware token tracking (from Anthropic API)
    /// Tokens read from cache (present when prompt caching is enabled)
    #[serde(default)]
    pub cache_read_input_tokens: usize,

    /// Tokens written to cache (present when prompt caching is enabled)
    #[serde(default)]
    pub cache_creation_input_tokens: usize,
}

#[derive(Serialize)]
pub struct AnthropicMessage {
    pub role: &'static str,
    pub content: AnthropicRequestContent,
}

#[derive(Serialize, Debug)]
#[serde(untagged)]
#[allow(dead_code)] // Kept for future use
pub enum AnthropicRequestContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Content for tool_result blocks: either a plain string or an array of typed blocks.
#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ToolResultBlock>),
}

impl PartialEq<&str> for ToolResultContent {
    fn eq(&self, other: &&str) -> bool {
        match self {
            Self::Text(s) => s == *other,
            _ => false,
        }
    }
}

impl PartialEq<str> for ToolResultContent {
    fn eq(&self, other: &str) -> bool {
        match self {
            Self::Text(s) => s == other,
            _ => false,
        }
    }
}

impl PartialEq<String> for ToolResultContent {
    fn eq(&self, other: &String) -> bool {
        match self {
            Self::Text(s) => s == other,
            _ => false,
        }
    }
}

/// A single block inside a tool_result content array.
#[derive(Serialize, Clone, Debug)]
pub struct ToolResultBlock {
    #[serde(rename = "type")]
    pub block_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum ContentBlock {
    Text {
        #[serde(rename = "type")]
        content_type: &'static str,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Image {
        #[serde(rename = "type")]
        content_type: &'static str,
        source: ImageSource,
    },
    ToolUse {
        #[serde(rename = "type")]
        content_type: &'static str,
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolResult {
        #[serde(rename = "type")]
        content_type: &'static str,
        tool_use_id: String,
        content: ToolResultContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    SearchResult {
        #[serde(rename = "type")]
        content_type: &'static str,
        source: String,
        title: String,
        content: Vec<SearchResultContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<CitationMetadata>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Thinking {
        #[serde(rename = "type")]
        content_type: &'static str,
        thinking: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        signature: String,
    },
    RedactedThinking {
        #[serde(rename = "type")]
        content_type: &'static str,
        #[serde(skip_serializing_if = "String::is_empty")]
        data: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

/// Content block for search results (RAG applications)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResultBlock {
    #[serde(rename = "type")]
    pub content_type: &'static str,
    pub source: String,
    pub title: String,
    pub content: Vec<SearchResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Text block within search result content
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResultContent {
    #[serde(rename = "type")]
    pub content_type: &'static str,
    pub text: String,
}

/// Citation configuration for search results
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CitationMetadata {
    pub enabled: bool,
}

/// Cache control for search results
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub cache_type: &'static str,
}

/// System prompt content block with optional cache control.
#[derive(Serialize, Clone, Debug)]
pub struct SystemContentBlock {
    #[serde(rename = "type")]
    pub block_type: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// System prompt: either a plain string or an array of content blocks.
#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum SystemPrompt {
    #[allow(dead_code)] // Used when system prompt doesn't need cache_control
    Text(String),
    Blocks(Vec<SystemContentBlock>),
}
