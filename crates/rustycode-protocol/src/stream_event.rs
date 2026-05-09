//! Raw streaming events from the agent LLM↔tool loop.
//!
//! These events are emitted by `AgentSession` during execution and consumed
//! by `SessionProcessor`. They represent the finest-grained observations
//! the agent makes.
//!
//! The event set is intentionally minimal. New variants can be added later
//! without breaking consumers (they ignore unknown variants).

use serde::{Deserialize, Serialize};

/// Approval decision for a tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    Reject(String),
    AutoApproved,
}

/// A raw streaming event from the agent loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum StreamEvent {
    /// LLM text content arriving incrementally.
    TextDelta { content: String },
    /// LLM thinking/reasoning arriving incrementally.
    ThinkingDelta { content: String },
    /// A thinking or redacted_thinking block completed with round-trip metadata.
    ThinkingBlockCompleted {
        block_type: String,
        signature: String,
        data: String,
    },
    /// A tool call block has started.
    ToolCallStarted { id: String, name: String },
    /// Tool input JSON arriving incrementally.
    ToolInputDelta { id: String, chunk: String },
    /// Tool execution has begun.
    ToolExecStarted { id: String, name: String },
    /// Tool execution finished.
    ToolExecCompleted {
        id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    /// A new agent turn has started.
    TurnStarted { turn: usize },
    /// Token usage report.
    TokenUsage {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// LLM turn ended with a stop reason ("end_turn", "tool_use", "max_tokens").
    TurnCompleted { stop_reason: String },
    /// Cache token accounting (prompt caching).
    CacheUsage {
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
    },
    /// Session completed normally.
    Done,
    /// A plan has been created with steps.
    PlanCreated {
        id: String,
        title: String,
        steps: Vec<StreamPlanStep>,
    },
    /// A plan step has started executing.
    PlanStepStarted { plan_id: String, step_index: usize },
    /// A plan step has finished.
    PlanStepCompleted {
        plan_id: String,
        step_index: usize,
        success: bool,
        message: String,
    },
    /// The entire plan has finished.
    PlanCompleted {
        plan_id: String,
        success: bool,
        summary: String,
    },
    /// Plan is awaiting user approval.
    PlanApprovalRequested {
        plan_id: String,
        title: String,
        steps: Vec<StreamPlanStep>,
    },
}

/// Lightweight plan step for streaming events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamPlanStep {
    pub name: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_roundtrip() {
        let events = vec![
            StreamEvent::TextDelta {
                content: "Hello".into(),
            },
            StreamEvent::ThinkingDelta {
                content: "thinking".into(),
            },
            StreamEvent::ToolCallStarted {
                id: "t1".into(),
                name: "Bash".into(),
            },
            StreamEvent::ToolInputDelta {
                id: "t1".into(),
                chunk: r#"{"cmd":"echo"#.into(),
            },
            StreamEvent::ToolInputDelta {
                id: "t1".into(),
                chunk: r#""}"#.into(),
            },
            StreamEvent::ToolExecStarted {
                id: "t1".into(),
                name: "Bash".into(),
            },
            StreamEvent::ToolExecCompleted {
                id: "t1".into(),
                name: "Bash".into(),
                output: "ok".into(),
                is_error: false,
            },
            StreamEvent::TurnStarted { turn: 1 },
            StreamEvent::TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
            StreamEvent::TurnCompleted {
                stop_reason: "tool_use".into(),
            },
            StreamEvent::TurnCompleted {
                stop_reason: "end_turn".into(),
            },
            StreamEvent::CacheUsage {
                cache_read_tokens: 1024,
                cache_creation_tokens: 512,
            },
            StreamEvent::Done,
        ];

        for ev in events {
            let json = serde_json::to_string(&ev).unwrap();
            let decoded: StreamEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, ev);
        }
    }
}
