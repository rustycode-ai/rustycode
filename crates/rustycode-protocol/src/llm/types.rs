//! LLM operation types
//!
//! Shared types for LLM provider operations, including request/response structs,
//! model information, and cost tracking.

use serde::{Deserialize, Serialize};

/// Information about an available model from an LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-3-opus", "gpt-4", "llama2")
    pub name: String,
    /// Provider name (e.g., "anthropic", "openai")
    pub provider: String,
    /// Context window size in tokens
    pub context_window: usize,
    /// Whether this model supports streaming responses
    pub supports_streaming: bool,
    /// Cost per 1K input tokens in dollars
    pub cost_per_1k_input_tokens: f64,
    /// Cost per 1K output tokens in dollars
    pub cost_per_1k_output_tokens: f64,
}

/// Request for LLM completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model to use for completion
    pub model: String,
    /// Input prompt/message
    pub prompt: String,
    /// Maximum tokens to generate (optional)
    pub max_tokens: Option<usize>,
    /// Temperature for generation (0.0 - 1.0, optional)
    pub temperature: Option<f32>,
    /// Top-p (nucleus sampling) parameter (optional)
    pub top_p: Option<f32>,
    /// System prompt to set model behavior (optional)
    pub system: Option<String>,
}

/// Token usage for a completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCount {
    /// Number of tokens in the input
    pub input_tokens: usize,
    /// Number of tokens in the output
    pub output_tokens: usize,
    /// Total tokens (input + output)
    pub total_tokens: usize,
}

/// Cost breakdown for a completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cost {
    /// Cost for input tokens in dollars
    pub input_cost: f64,
    /// Cost for output tokens in dollars
    pub output_cost: f64,
    /// Total cost (input_cost + output_cost) in dollars
    pub total_cost: f64,
}

/// Response from an LLM completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Generated text
    pub text: String,
    /// Token usage information
    pub tokens_used: TokenCount,
    /// Cost information
    pub cost: Cost,
    /// Reason for completion finish (e.g., "stop", "length", "error")
    pub finish_reason: String,
}
