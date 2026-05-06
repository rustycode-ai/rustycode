//! Unified LLM provider operation types
//!
//! Types specific to the UnifiedLLMProvider compatibility trait.
//! Shared data types (Usage, TokenCount, Cost) live in rustycode_protocol::llm.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub provider: String,
    pub context_window: usize,
    pub supports_streaming: bool,
    pub cost_per_1k_input_tokens: f64,
    pub cost_per_1k_output_tokens: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: String,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub system: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub tokens_used: TokenCount,
    pub cost: Cost,
    pub finish_reason: String,
}

// Re-export shared types from protocol for use in unified trait signatures
pub use rustycode_protocol::llm::{Cost, TokenCount};
