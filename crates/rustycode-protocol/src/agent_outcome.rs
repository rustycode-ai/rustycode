//! Outbound result from a completed agent execution.
//!
//! Shared across orchestration, agent-runtime, and team crates via the protocol layer.

use crate::agent_protocol::FileChange;
use crate::reasoning_summary::ReasoningSummary;
use crate::token_usage::TokenUsage;

/// Outbound result from a completed agent execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentOutcome {
    /// Identifier of the agent that produced this outcome.
    pub agent_id: String,
    /// Task this outcome belongs to.
    pub task_id: String,
    /// Whether the agent considers its work successful.
    pub success: bool,
    /// Final text output from the agent.
    pub output_text: String,
    /// Files modified during execution.
    pub files_changed: Vec<FileChange>,
    /// Cumulative token usage.
    pub usage: TokenUsage,
    /// Structured reasoning summary from the agent's thinking.
    pub reasoning_summary: ReasoningSummary,
}

impl AgentOutcome {
    /// Create a minimal failed outcome for error cases.
    pub fn failed(agent_id: impl Into<String>, task_id: impl Into<String>, reason: &str) -> Self {
        Self {
            agent_id: agent_id.into(),
            task_id: task_id.into(),
            success: false,
            output_text: reason.into(),
            files_changed: vec![],
            usage: TokenUsage::zero(),
            reasoning_summary: ReasoningSummary::empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_outcome() {
        let outcome = AgentOutcome::failed("agent_1", "task_1", "timeout exceeded");
        assert!(!outcome.success);
        assert_eq!(outcome.output_text, "timeout exceeded");
        assert!(outcome.files_changed.is_empty());
        assert_eq!(outcome.usage.total(), 0);
    }

    #[test]
    fn serialization_round_trip() {
        let outcome = AgentOutcome {
            agent_id: "agent_2".into(),
            task_id: "task_2".into(),
            success: true,
            output_text: "Done".into(),
            files_changed: vec![FileChange {
                path: "src/lib.rs".into(),
                summary: "Added new function".into(),
                diff_hunk: "+fn new() {}".into(),
                lines_added: 1,
                lines_removed: 0,
            }],
            usage: TokenUsage::zero(),
            reasoning_summary: ReasoningSummary::empty(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: AgentOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_id, outcome.agent_id);
        assert!(deserialized.success);
        assert_eq!(deserialized.files_changed.len(), 1);
    }
}
