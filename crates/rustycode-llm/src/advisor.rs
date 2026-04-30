//! Advisor tool support (Anthropic `advisor_20260301` tool type)
//!
//! Implements the advisor pattern from Anthropic's blog (2026-04-09):
//! - Sonnet or Haiku runs as **executor** (drives the task, calls tools)
//! - When stuck, executor invokes the **advisor** (Opus) for guidance
//! - Opus returns plan/correction/stop signal — never calls tools or produces output
//! - **Single `/v1/messages` request** — no extra round-trips
//!
//! # API Usage
//!
//! ```rust,ignore
//! let advisor = AdvisorTool::new("claude-opus-4-6")
//!     .with_max_uses(3);
//!
//! // Add to your tool list — it uses a special tool type, not the normal format
//! let tools = advisor.to_anthropic_tool();
//! ```
//!
//! # Architecture
//!
//! Traditional sub-agent pattern: big model orchestrates, delegates to small workers.
//! Advisor pattern: small model drives, escalates to big model only when needed.
//! Result: near-Opus intelligence at Sonnet-level cost.

use serde::{Deserialize, Serialize};
use serde_json::json;

/// Configuration for the advisor tool.
///
/// The advisor tool allows an executor model (e.g., Sonnet) to consult
/// a more capable advisor model (e.g., Opus) during task execution.
/// The advisor provides guidance but never executes tools or produces
/// user-facing output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorTool {
    /// The model to use as the advisor (e.g., "claude-opus-4-6")
    pub advisor_model: String,
    /// Maximum number of times the executor may consult the advisor per request.
    /// Controls cost — each advisor call is billed at advisor model rates.
    /// Default: 3
    pub max_uses: u32,
    /// Optional name for the tool (default: "advisor")
    pub name: String,
}

impl AdvisorTool {
    /// Create a new advisor tool with the given advisor model.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let advisor = AdvisorTool::new("claude-opus-4-6");
    /// ```
    pub fn new(advisor_model: impl Into<String>) -> Self {
        Self {
            advisor_model: advisor_model.into(),
            max_uses: 3,
            name: "advisor".to_string(),
        }
    }

    /// Set the maximum number of advisor consultations per request.
    ///
    /// Lower values reduce cost but may limit the executor's ability
    /// to get guidance on complex tasks.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let advisor = AdvisorTool::new("claude-opus-4-6")
    ///     .with_max_uses(5);  // Allow up to 5 consultations
    /// ```
    pub fn with_max_uses(mut self, max: u32) -> Self {
        self.max_uses = max;
        self
    }

    /// Set a custom name for the advisor tool.
    ///
    /// Useful when you have multiple advisors with different configurations.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Convert to the Anthropic API tool format.
    ///
    /// The advisor tool uses a special `type: "advisor_20260301"` format
    /// rather than the normal `type: "custom"` tool format.
    ///
    /// This must be sent alongside the `anthropic-beta: advisor-tool-2026-03-01`
    /// header in the API request.
    pub fn to_anthropic_tool(&self) -> serde_json::Value {
        json!({
            "type": "advisor_20260301",
            "name": self.name,
            "model": self.advisor_model,
            "max_uses": self.max_uses,
        })
    }

    /// Get the beta header required for the advisor tool.
    ///
    /// This must be included in the `anthropic-beta` header of the API request.
    pub fn beta_header() -> &'static str {
        "anthropic-beta: advisor-tool-2026-03-01"
    }
}

/// Response from the advisor when the executor consults it.
///
/// The advisor returns structured guidance that the executor uses
/// to continue the task. The advisor never calls tools or produces
/// user-facing output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorResponse {
    /// The advisor's recommended plan or correction
    pub guidance: String,
    /// Whether the executor should stop the current approach
    pub should_stop: bool,
    /// Optional corrected plan for the executor to follow
    pub corrected_plan: Option<String>,
}

/// Configuration for an advisor-enabled request.
///
/// Wraps the standard request configuration with advisor-specific settings.
#[derive(Debug, Clone)]
pub struct AdvisorConfig {
    /// The advisor tool configuration
    pub advisor: AdvisorTool,
    /// The model to use as the executor (e.g., "claude-sonnet-4-6")
    pub executor_model: String,
}

impl AdvisorConfig {
    /// Create a new advisor configuration with default settings.
    ///
    /// Uses Sonnet 4.6 as executor and Opus 4.6 as advisor.
    pub fn new() -> Self {
        Self {
            advisor: AdvisorTool::new("claude-opus-4-6"),
            executor_model: "claude-sonnet-4-6".to_string(),
        }
    }

    /// Create with custom models.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Haiku executor with Opus advisor for maximum cost savings
    /// let config = AdvisorConfig::with_models("claude-haiku-4-5-20251001", "claude-opus-4-6");
    /// ```
    pub fn with_models(executor: impl Into<String>, advisor: impl Into<String>) -> Self {
        Self {
            advisor: AdvisorTool::new(advisor),
            executor_model: executor.into(),
        }
    }

    /// Get the combined tool list for an advisor-enabled request.
    ///
    /// Merges the standard tools with the advisor tool.
    pub fn tool_list(&self, standard_tools: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        let mut tools = standard_tools;
        tools.push(self.advisor.to_anthropic_tool());
        tools
    }

    /// Get the beta headers required for advisor mode.
    pub fn beta_headers() -> Vec<String> {
        vec![AdvisorTool::beta_header().to_string()]
    }
}

impl Default for AdvisorConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advisor_tool_creation() {
        let advisor = AdvisorTool::new("claude-opus-4-6");
        assert_eq!(advisor.advisor_model, "claude-opus-4-6");
        assert_eq!(advisor.max_uses, 3);
        assert_eq!(advisor.name, "advisor");
    }

    #[test]
    fn test_advisor_tool_custom_config() {
        let advisor = AdvisorTool::new("claude-opus-4-6")
            .with_max_uses(5)
            .with_name("senior_advisor");

        assert_eq!(advisor.max_uses, 5);
        assert_eq!(advisor.name, "senior_advisor");
    }

    #[test]
    fn test_anthropic_tool_format() {
        let advisor = AdvisorTool::new("claude-opus-4-6").with_max_uses(3);
        let tool_json = advisor.to_anthropic_tool();

        assert_eq!(tool_json["type"], "advisor_20260301");
        assert_eq!(tool_json["name"], "advisor");
        assert_eq!(tool_json["model"], "claude-opus-4-6");
        assert_eq!(tool_json["max_uses"], 3);
    }

    #[test]
    fn test_beta_header() {
        let header = AdvisorTool::beta_header();
        assert_eq!(header, "anthropic-beta: advisor-tool-2026-03-01");
    }

    #[test]
    fn test_advisor_config_default() {
        let config = AdvisorConfig::new();
        assert_eq!(config.executor_model, "claude-sonnet-4-6");
        assert_eq!(config.advisor.advisor_model, "claude-opus-4-6");
    }

    #[test]
    fn test_advisor_config_custom_models() {
        let config = AdvisorConfig::with_models("claude-haiku-4-5-20251001", "claude-opus-4-6");
        assert_eq!(config.executor_model, "claude-haiku-4-5-20251001");
        assert_eq!(config.advisor.advisor_model, "claude-opus-4-6");
    }

    #[test]
    fn test_tool_list_merges_advisor() {
        let config = AdvisorConfig::new();
        let standard_tools = vec![
            json!({"type": "custom", "name": "bash"}),
            json!({"type": "custom", "name": "read_file"}),
        ];

        let merged = config.tool_list(standard_tools);
        assert_eq!(merged.len(), 3);

        let advisor_tool = merged.iter().find(|t| t["type"] == "advisor_20260301");
        assert!(
            advisor_tool.is_some(),
            "Advisor tool should be in merged list"
        );
    }

    #[test]
    fn test_advisor_response_deserialization() {
        let json = r#"{
            "guidance": "Consider using a HashMap instead of Vec for O(1) lookups",
            "should_stop": false,
            "corrected_plan": "1. Replace Vec with HashMap\n2. Update lookup calls"
        }"#;

        let response: AdvisorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.guidance,
            "Consider using a HashMap instead of Vec for O(1) lookups"
        );
        assert!(!response.should_stop);
        assert!(response.corrected_plan.is_some());
    }

    #[test]
    fn test_advisor_response_stop_signal() {
        let json = r#"{
            "guidance": "This approach won't work due to borrow checker rules",
            "should_stop": true,
            "corrected_plan": null
        }"#;

        let response: AdvisorResponse = serde_json::from_str(json).unwrap();
        assert!(response.should_stop);
        assert!(response.corrected_plan.is_none());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let advisor = AdvisorTool::new("claude-opus-4-6")
            .with_max_uses(7)
            .with_name("expert");

        let json = serde_json::to_string(&advisor).unwrap();
        let back: AdvisorTool = serde_json::from_str(&json).unwrap();

        assert_eq!(back.advisor_model, "claude-opus-4-6");
        assert_eq!(back.max_uses, 7);
        assert_eq!(back.name, "expert");
    }
}
