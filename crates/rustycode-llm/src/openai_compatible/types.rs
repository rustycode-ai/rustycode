//! Shared types for OpenAI-compatible providers

use serde::{Deserialize, Serialize};

/// Generic OpenAI-compatible request.
///
/// All fields use `#[serde(skip_serializing_if = "Option::is_none")]` so
/// providers can set only the fields they need.
#[derive(Serialize, Debug, Clone)]
pub struct OpenAiCompatibleRequest<M> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub messages: Vec<M>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
}

/// Standard OpenAI-compatible message for simple text-based providers.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenAiCompatibleMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// OpenAI-compatible tool call.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiFunction,
}

/// Function details within a tool call.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OpenAiFunction {
    pub name: String,
    pub arguments: String,
}

/// Standard OpenAI-compatible response.
#[derive(Deserialize, Debug, Clone)]
pub struct OpenAiCompatibleResponse {
    pub choices: Vec<OpenAiCompatibleChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAiCompatibleUsage>,
    pub model: String,
}

/// A single choice in the response.
#[derive(Deserialize, Debug, Clone)]
pub struct OpenAiCompatibleChoice {
    pub message: OpenAiCompatibleResponseMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

/// The message within a choice.
#[derive(Deserialize, Debug, Clone)]
pub struct OpenAiCompatibleResponseMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
}

/// Token usage information.
#[derive(Deserialize, Debug, Clone)]
pub struct OpenAiCompatibleUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
}

/// Details about prompt token breakdown.
#[derive(Deserialize, Debug, Clone)]
pub struct PromptTokensDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

/// Model list response from OpenAI-compatible endpoints.
#[derive(Deserialize, Debug, Clone)]
pub struct OpenAiModelListResponse {
    pub data: Vec<OpenAiModelEntry>,
}

/// Single model entry in the list.
#[derive(Deserialize, Debug, Clone)]
pub struct OpenAiModelEntry {
    pub id: String,
}
