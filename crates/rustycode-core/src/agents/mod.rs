//! Agent orchestration and workflow patterns
//!
//! This module implements agent orchestration patterns inspired by Anthropic's
//! Claude Cookbooks, including:
//!
//! - **Orchestrator-Subagents**: Main orchestrator delegates to specialized agents
//! - **Evaluator-Optimizer**: One agent evaluates work, another improves it
//! - **Prompt Chaining**: Chain multiple LLM calls where each builds on the previous
//! - **Routing**: Classify requests and route to appropriate handler
//!
//! ## Architecture
//!
//! ```text
//!     ┌─────────────────────────────────────────┐
//!     │         Orchestrator Agent              │
//!     │  (Chief of Staff / Main Coordinator)    │
//!     └──────────────────┬──────────────────────┘
//!                        │
//!         ┌──────────────┼──────────────┐
//!         │              │              │
//!    ┌────▼────┐   ┌────▼────┐   ┌────▼────┐
//!    │ Coder   │   │Debugger │   │Reviewer │
//!    │ Subagent│   │ Subagent│   │ Subagent│
//!    └────┬────┘   └────┬────┘   └────┬────┘
//!         │             │             │
//!         └──────────────┼──────────────┘
//!                        │
//!                   ┌────▼────┐
//!                   │  Tools  │
//!                   └─────────┘
//! ```

pub mod orchestrator;
pub mod patterns;
pub mod subagent;

pub use orchestrator::{Orchestrator, OrchestratorConfig};
pub use patterns::{EvaluatorOptimizer, EvaluatorOptimizerConfig, PromptChain, PromptChainConfig};
pub use subagent::{Subagent, SubagentConfig, SubagentRegistry};

use serde::{Deserialize, Serialize};

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_result_success_construction() {
        let result = AgentResult::success("done".into());
        assert!(result.success);
        assert_eq!(result.content, "done");
        assert!(result.tool_calls.is_empty());
        assert!(result.error.is_none());
        assert_eq!(result.token_usage.input_tokens, 0);
    }

    #[test]
    fn agent_result_failure_construction() {
        let result = AgentResult::failure("crashed".into());
        assert!(!result.success);
        assert!(result.content.is_empty());
        assert_eq!(result.error, Some("crashed".into()));
    }

    #[test]
    fn agent_result_serde_roundtrip() {
        let result = AgentResult {
            content: "output".into(),
            tool_calls: vec![ToolCall {
                id: "tc-1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({"cmd": "ls"}),
                result: Some(serde_json::json!("files")),
            }],
            token_usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                total_tokens: 150,
            },
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: AgentResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.success);
        assert_eq!(decoded.tool_calls.len(), 1);
        assert_eq!(decoded.tool_calls[0].name, "bash");
        assert_eq!(decoded.token_usage.total_tokens, 150);
    }

    #[test]
    fn tool_call_serde_roundtrip() {
        let tc = ToolCall {
            id: "tc-2".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "/tmp/file"}),
            result: None,
        };
        let json = serde_json::to_string(&tc).unwrap();
        let decoded: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "tc-2");
        assert!(decoded.result.is_none());
    }

    #[test]
    fn token_usage_serde_roundtrip() {
        let usage = TokenUsage {
            input_tokens: 500,
            output_tokens: 200,
            total_tokens: 700,
        };
        let json = serde_json::to_string(&usage).unwrap();
        let decoded: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_tokens, 700);
    }

    #[test]
    fn token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }
}
