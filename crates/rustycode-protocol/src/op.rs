//! Submission Queue (Op) types for the TUI → Core protocol boundary.
//!
//! The `Op` enum represents all commands that a frontend (TUI, IDE, etc.)
//! can submit to the core session. Inspired by Codex's SQ pattern:
//! frontends send `Op` through a channel; the core processes them and
//! emits [`crate::stream_event::StreamEvent`] back.

use serde::{Deserialize, Serialize};

/// Commands submitted by the frontend to the core session.
///
/// Each variant corresponds to a user action or UI event that the core
/// must process. The core matches on these in its event loop and dispatches
/// to the appropriate service method.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Op {
    /// Send a simple user message (no history override).
    SendMessage { content: String },

    /// Send a user message with full conversation history and optional images.
    ///
    /// The `history` and `images` fields contain serialized LLM-specific types
    /// (`ChatMessage`, `ContentBlock`). The core deserializes them at dispatch time.
    SendMessageFull {
        content: String,
        /// Serialized `Vec<ChatMessage>` conversation history.
        history: Option<serde_json::Value>,
        /// Serialized `Vec<ContentBlock>` image blocks (e.g. clipboard paste).
        images: Option<serde_json::Value>,
    },

    /// Request cooperative cancellation of the active stream.
    StopStream,

    /// Respond to a tool approval request with optional overrides.
    ///
    /// `approved` = true: execute tool.
    /// `approved` = false: reject execution.
    ApproveTool {
        tool_id: String,
        approved: bool,
        /// Optional modified input for the tool (e.g. user-edited file content).
        modified_input: Option<serde_json::Value>,
        /// Optional timeout override in seconds.
        timeout_override: Option<u64>,
    },

    /// Set the permission mode for tool approval decisions.
    SetPermissionMode {
        mode: crate::permission_modes::PermissionMode,
    },

    /// Answer a question from the agent (ask_user tool).
    AnswerQuestion { answer: String },

    /// Switch the active LLM model.
    SwitchModel { model_id: String },

    /// Set the agent mode directly (code, architect, etc.).
    SetAgentMode { mode: String },

    /// Cycle to the next or previous agent mode.
    CycleAgentMode {
        /// `true` = next mode, `false` = previous mode.
        forward: bool,
    },

    /// Set the reasoning effort level (low/medium/high/xhigh/max).
    SetEffort { effort: String },

    // ── Checkpoints & Step Management ────────────────────────────────────────
    /// Resume from a specific checkpoint.
    ResumeFromCheckpoint { checkpoint_id: String },

    /// Retry a failed plan step.
    RetryStep { step_id: String },

    /// Skip a plan step.
    SkipStep { step_id: String },

    // ── Orchestration & Strategy ─────────────────────────────────────────────
    /// Set the orchestration strategy (e.g. "fast", "architect", "ensemble").
    SetStrategy {
        strategy: String,
        config: Option<serde_json::Value>,
    },

    /// Query progress for a specific milestone.
    QueryMilestoneProgress { milestone_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_send_message_serialization() {
        let op = Op::SendMessage {
            content: "hello".into(),
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: Op = serde_json::from_str(&json).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn op_send_message_full_with_history() {
        let op = Op::SendMessageFull {
            content: "fix the bug".into(),
            history: Some(serde_json::json!([{"role": "user", "content": "hi"}])),
            images: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: Op = serde_json::from_str(&json).unwrap();
        assert_eq!(op, back);
    }

    #[test]
    fn op_all_variants_roundtrip() {
        let ops = vec![
            Op::SendMessage {
                content: "test".into(),
            },
            Op::SendMessageFull {
                content: "test".into(),
                history: None,
                images: None,
            },
            Op::StopStream,
            Op::ApproveTool {
                tool_id: "tool_1".into(),
                approved: true,
                modified_input: None,
                timeout_override: None,
            },
            Op::ApproveTool {
                tool_id: "tool_1".into(),
                approved: false,
                modified_input: None,
                timeout_override: None,
            },
            Op::SetPermissionMode {
                mode: crate::permission_modes::PermissionMode::Bypass,
            },
            Op::AnswerQuestion {
                answer: "yes".into(),
            },
            Op::SwitchModel {
                model_id: "gpt-4".into(),
            },
            Op::SetAgentMode {
                mode: "architect".into(),
            },
            Op::CycleAgentMode { forward: true },
            Op::CycleAgentMode { forward: false },
            Op::SetEffort {
                effort: "high".into(),
            },
        ];
        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            let back: Op = serde_json::from_str(&json).unwrap();
            assert_eq!(op, back);
        }
    }
}
