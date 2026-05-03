//! Base agent trait and types

use async_trait::async_trait;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Result from agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

impl AgentResult {
    pub fn success(content: String) -> Self {
        Self {
            success: true,
            content,
            error: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            content: String::new(),
            error: Some(error),
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }
}

/// Configuration for agent behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Model to use
    pub model: String,
    /// Maximum number of iterations
    pub max_iterations: usize,
    /// System prompt
    pub system_prompt: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-5".to_string(),
            max_iterations: 10,
            system_prompt: None,
        }
    }
}

impl AgentConfig {
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }
}

/// Base trait for all agents
#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;

    async fn execute(
        &self,
        prompt: &str,
        context: Option<&str>,
        config: &AgentConfig,
    ) -> Result<AgentResult>;

    fn can_handle(&self, task_description: &str) -> bool {
        let _ = task_description;
        true
    }
}
