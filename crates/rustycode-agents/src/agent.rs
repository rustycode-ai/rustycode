//! Common agent trait and types
//!
//! This module defines the core `Agent` trait that all agents must implement,
//! along with common configuration and result types.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Model to use for this agent
    pub model: String,

    /// System prompt for the agent
    pub system_prompt: Option<String>,

    /// Maximum tokens for responses
    pub max_tokens: Option<u32>,

    /// Temperature for response generation
    pub temperature: Option<f32>,

    /// Additional configuration options
    pub options: HashMap<String, serde_json::Value>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-4-6".to_string(),
            system_prompt: None,
            max_tokens: Some(8192),
            temperature: Some(0.7),
            options: HashMap::new(),
        }
    }
}

/// Result from an agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// The output content from the agent
    pub content: String,

    /// Tool calls made by the agent (if any)
    pub tool_calls: Vec<ToolCall>,

    /// Token usage statistics
    pub token_usage: TokenUsage,

    /// Whether the agent completed successfully
    pub success: bool,

    /// Error message if not successful
    pub error: Option<String>,
}

/// A tool call made by an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub id: String,

    /// Name of the tool being called
    pub name: String,

    /// Arguments passed to the tool
    pub arguments: serde_json::Value,

    /// Result of the tool call (if completed)
    pub result: Option<serde_json::Value>,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Common trait for all agents
#[async_trait]
pub trait Agent: Send + Sync {
    /// Get the name of this agent
    fn name(&self) -> &'static str;

    /// Get the description of this agent
    fn description(&self) -> &'static str;

    /// Execute the agent with the given prompt and context
    async fn execute(
        &self,
        prompt: &str,
        context: Option<&str>,
        config: &AgentConfig,
    ) -> Result<AgentResult>;

    /// Check if this agent can handle the given task
    fn can_handle(&self, task_description: &str) -> bool;
}

impl AgentResult {
    /// Create a successful result
    pub fn success(content: String) -> Self {
        Self {
            content,
            tool_calls: Vec::new(),
            token_usage: TokenUsage::default(),
            success: true,
            error: None,
        }
    }

    /// Create a failed result
    pub fn failure(error: String) -> Self {
        Self {
            content: String::new(),
            tool_calls: Vec::new(),
            token_usage: TokenUsage::default(),
            success: false,
            error: Some(error),
        }
    }

    /// Add token usage to the result
    pub const fn with_token_usage(mut self, input: u64, output: u64) -> Self {
        self.token_usage = TokenUsage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
        };
        self
    }

    /// Add tool calls to the result
    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = tool_calls;
        self
    }
}
