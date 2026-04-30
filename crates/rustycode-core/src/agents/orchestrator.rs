//! Orchestrator for coordinating subagents
//!
//! The orchestrator pattern (also called "Chief of Staff") delegates tasks
//! to specialized subagents based on the nature of the work.

use crate::agents::{subagent::SubagentRegistry, AgentResult};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Configuration for the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Model to use for the orchestrator itself
    pub model: String,

    /// System prompt for the orchestrator
    pub system_prompt: Option<String>,

    /// Maximum tokens for orchestrator responses
    pub max_tokens: Option<u32>,

    /// Whether to enable automatic delegation
    pub auto_delegate: bool,

    /// Maximum number of delegation hops (prevent infinite loops)
    pub max_delegation_depth: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-4-6".to_string(),
            system_prompt: None,
            max_tokens: Some(8192),
            auto_delegate: true,
            max_delegation_depth: 5,
        }
    }
}

impl OrchestratorConfig {
    /// Create a new orchestrator configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the model
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Enable/disable auto delegation
    pub fn with_auto_delegate(mut self, enabled: bool) -> Self {
        self.auto_delegate = enabled;
        self
    }
}

/// The orchestrator agent that coordinates subagents
#[derive(Debug, Clone)]
pub struct Orchestrator {
    config: OrchestratorConfig,
    subagents: SubagentRegistry,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub fn new(config: OrchestratorConfig) -> Self {
        Self {
            config,
            subagents: SubagentRegistry::with_defaults(),
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(OrchestratorConfig::default())
    }

    /// Get the subagent registry
    pub fn subagents(&self) -> &SubagentRegistry {
        &self.subagents
    }

    /// Get a mutable reference to the subagent registry
    pub fn subagents_mut(&mut self) -> &mut SubagentRegistry {
        &mut self.subagents
    }

    /// Load subagents from a directory
    pub fn load_subagents_from_directory(&mut self, dir: &std::path::Path) -> Result<usize> {
        self.subagents.load_from_directory(dir)
    }

    /// Generate the default system prompt for the orchestrator
    fn default_system_prompt(&self) -> String {
        let available_subagents: Vec<_> = self
            .subagents
            .list_ids()
            .into_iter()
            .filter_map(|id| self.subagents.get(&id))
            .map(|s| format!("- **{}**: {}", s.name(), s.config().description))
            .collect();

        format!(
            "You are a Chief of Staff orchestrator responsible for coordinating work \
            across specialized subagents. Your role is to:\n\n\
            1. Understand the user's request\n\
            2. Determine which subagent (if any) should handle it\n\
            3. Delegate appropriately and integrate results\n\
            4. Handle tasks yourself when delegation isn't needed\n\
            5. Ensure all work is completed successfully\n\n\
            Available subagents:\n{}\n\n\
            When delegating:\n\
            - Provide clear context and requirements\n\
            - Include relevant background information\n\
            - Specify expected output format\n\
            - Follow up on delegated work to ensure quality\n\n\
            When handling tasks yourself:\n\
            - Be thorough and systematic\n\
            - Ask clarifying questions when needed\n\
            - Provide clear, actionable responses",
            available_subagents.join("\n")
        )
    }

    /// Process a request through the orchestrator
    pub async fn process(&self, request: &str) -> Result<AgentResult> {
        let system_prompt = self
            .config
            .system_prompt
            .clone()
            .unwrap_or_else(|| self.default_system_prompt());

        // For now, return a simple result
        // In a full implementation, this would call the LLM and potentially delegate
        Ok(AgentResult::success(format!(
            "Processed: {}\n\nSystem prompt: {}",
            request,
            system_prompt.chars().take(100).collect::<String>()
        )))
    }

    /// Delegate a task to a specific subagent
    pub async fn delegate_to(&self, subagent_id: &str, task: &str) -> Result<AgentResult> {
        let subagent = self
            .subagents
            .get(subagent_id)
            .ok_or_else(|| anyhow::anyhow!("Subagent '{}' not found", subagent_id))?;

        // For now, return a simulated result
        // In a full implementation, this would call the subagent's LLM
        Ok(AgentResult::success(format!(
            "[Delegated to {}]: {}",
            subagent.name(),
            task
        )))
    }

    /// Determine which subagent should handle a request
    pub fn route_request(&self, request: &str) -> Option<String> {
        let request_lower = request.to_lowercase();

        // Simple keyword-based routing
        // In a full implementation, this would use embeddings or an LLM
        for subagent_id in self.subagents.list_ids() {
            if let Some(subagent) = self.subagents.get(&subagent_id) {
                let keywords = match subagent.id() {
                    "coder" => vec![
                        "write",
                        "implement",
                        "code",
                        "function",
                        "class",
                        "refactor",
                    ],
                    "debugger" => vec!["bug", "error", "fix", "debug", "broken", "not working"],
                    "reviewer" => vec!["review", "check", "improve", "suggest", "optimize"],
                    _ => continue,
                };

                if keywords.iter().any(|kw| request_lower.contains(kw)) {
                    return Some(subagent_id.clone());
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_config() {
        let config = OrchestratorConfig::new()
            .with_model("claude-haiku-4-5".to_string())
            .with_auto_delegate(false);

        assert_eq!(config.model, "claude-haiku-4-5");
        assert!(!config.auto_delegate);
    }

    #[test]
    fn test_orchestrator_creation() {
        let orchestrator = Orchestrator::with_defaults();
        assert!(orchestrator.subagents().get("coder").is_some());
        assert!(orchestrator.subagents().get("debugger").is_some());
        assert!(orchestrator.subagents().get("reviewer").is_some());
    }

    #[test]
    fn test_route_request() {
        let orchestrator = Orchestrator::with_defaults();

        // Should route to coder (uses "write" keyword)
        assert_eq!(
            orchestrator.route_request("Write a function"),
            Some("coder".to_string())
        );

        // Should route to debugger (uses "bug" keyword)
        assert_eq!(
            orchestrator.route_request("Fix this bug"),
            Some("debugger".to_string())
        );

        // Should route to reviewer (uses "review" keyword, not "code")
        assert_eq!(
            orchestrator.route_request("Please review this"),
            Some("reviewer".to_string())
        );
    }

    #[tokio::test]
    async fn test_delegate_to() {
        let orchestrator = Orchestrator::with_defaults();
        let result = orchestrator
            .delegate_to("coder", "Write hello world")
            .await
            .unwrap();
        assert!(result.success);
        // The result format is "[Delegated to {name}]: {task}"
        assert!(result.content.contains("Coder") || result.content.contains("coder"));
    }
}
