//! Unified outbound event message type for the TUI ← Core protocol boundary.
//!
//! `EventMsg` is the single event type that the core sends to frontends.
//! It unifies the previously separate `StreamChunk`, `ToolResult`,
//! `WorkspaceUpdate`, and `SlashCommandResult` channels into a single
//! typed stream, matching the Codex `EventMsg` pattern.
//!
//! # Design
//!
//! - **Non-exhaustive** — new variants can be added without breaking consumers.
//! - **Serializable** — all variants derive `Serialize`/`Deserialize` for
//!   audit logging (`RolloutRecorder`).
//! - **Self-contained** — no dependencies on TUI-specific types.

use serde::{Deserialize, Serialize};

/// A single option in a multiple-choice question from the agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

/// Category label for a streaming error, used for display and retry logic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventErrorKind {
    /// Provider-level error (auth, rate limit, network, etc.).
    Provider,
    /// No API key configured.
    NoApiKey,
    /// Invalid API key format.
    InvalidApiKey,
    /// Maximum tool-use turns exceeded.
    MaxToolTurns,
    /// Stream exceeded maximum wall-clock duration.
    StreamDurationExceeded,
    /// No data received for too long.
    StreamIdleTimeout,
    /// Context / token budget exceeded.
    ContextBudgetExceeded,
    /// Orchestration pipeline step failed.
    OrchestrationStepFailed,
    /// Pipeline task failed.
    PipelineFailed,
    /// Async runtime creation failed.
    RuntimeError,
    /// Internal / unexpected error.
    InternalError,
    /// Approval channel unavailable.
    ApprovalChannelUnavailable,
    /// Question channel unavailable.
    QuestionChannelUnavailable,
}

/// Workspace context update event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceEvent {
    /// Workspace scan progress.
    ScanProgress { scanned: usize, total: usize },
    /// Workspace scan complete.
    ScanComplete { file_count: usize, dir_count: usize },
    /// Workspace context loaded.
    ContextLoaded(String),
    /// Workspace notice for the user.
    Notice(String),
    /// Workspace scan error.
    Error(String),
}

/// Tool execution output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolOutput {
    /// Successful execution with output text.
    Success(String),
    /// Execution failed with error text.
    Error(String),
    /// Tool execution timed out.
    Timeout,
}

/// Slash command execution result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandEvent {
    /// Command succeeded with message.
    Success(String),
    /// Command failed with error.
    Error(String),
}

/// A milestone or plan progress update from the orchestration layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MilestoneProgress {
    pub milestone_id: String,
    pub milestone_title: String,
    pub status: String,
    pub plans_total: usize,
    pub plans_completed: usize,
    pub current_plan_summary: String,
    pub action_hint: String,
}

/// Unified outbound event from the core to frontends.
///
/// Every variant corresponds to something the TUI (or another frontend)
/// needs to display or react to — text deltas, tool execution, approvals,
/// workspace updates, errors, lifecycle signals.
///
/// # Migration
///
/// This type is designed to be introduced alongside the existing
/// `StreamChunk` / `ToolResult` / `WorkspaceUpdate` / `SlashCommandResult`
/// channels. Producers can emit `EventMsg` alongside the old types;
/// consumers can switch to `EventMsg` at their own pace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum EventMsg {
    // ── Streaming (delta-based) ──────────────────────────────────────────────
    /// LLM text content arriving incrementally.
    TextDelta { delta: String },
    /// LLM thinking / reasoning arriving incrementally.
    ThinkingDelta { delta: String },
    /// A thinking block completed with round-trip metadata.
    ThinkingBlockCompleted {
        block_type: String,
        signature: String,
        data: String,
    },
    /// A new agent turn has started.
    TurnStarted { turn: usize },
    /// LLM turn ended with a stop reason.
    TurnCompleted { stop_reason: String },

    // ── Tool execution ───────────────────────────────────────────────────────
    /// A tool call block has started.
    ToolCallStarted {
        tool_name: String,
        tool_id: String,
        input: serde_json::Value,
    },
    /// Tool input JSON arriving incrementally.
    ToolInputDelta { tool_id: String, delta: String },
    /// Tool execution has begun.
    ToolExecStarted { tool_name: String, tool_id: String },
    /// Tool execution progress update.
    ToolExecProgress {
        tool_id: String,
        stage: String,
        elapsed_ms: u64,
        preview: Option<String>,
    },
    /// Tool execution completed.
    ToolExecCompleted {
        tool_id: String,
        tool_name: String,
        success: bool,
        output: String,
        output_size: usize,
        duration_ms: u64,
    },
    /// File snapshot before a write operation (for undo).
    FileSnapshot { batch: Vec<(String, String)> },

    // ── Token usage ──────────────────────────────────────────────────────────
    /// Token usage for the current turn.
    TokenUsage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
    },

    // ── Session lifecycle ────────────────────────────────────────────────────
    /// Session completed normally.
    Done,
    /// Streaming stopped with a non-normal reason (content filter, safety, etc.).
    Stopped { stop_reason: String },
    /// Streaming encountered an error.
    Error {
        kind: EventErrorKind,
        message: String,
        retryable: bool,
    },
    /// The final execution trace from the orchestration pipeline.
    ExecutionTrace(serde_json::Value),
    /// A system-level status message.
    SystemMessage(String),

    // ── Tool approval ────────────────────────────────────────────────────────
    /// Request user approval for a tool execution.
    ApprovalRequired {
        tool_name: String,
        tool_id: String,
        operation_class: crate::permission_modes::OperationClass,
        description: String,
        diff: Option<String>,
    },
    /// User approved tool execution.
    ApprovalApproved { tool_id: String },
    /// User rejected tool execution.
    ApprovalRejected { tool_id: String },

    // ── User questions ───────────────────────────────────────────────────────
    /// Request user answer to a question.
    QuestionRequired {
        question_id: String,
        question_text: String,
        header: String,
        options: Vec<QuestionOption>,
        multi_select: bool,
    },
    /// User answered a question.
    QuestionAnswered { question_id: String, answer: String },

    // ── Task extraction ──────────────────────────────────────────────────────
    /// Extract tasks/todos from text.
    ExtractTasks { text: String },
    /// Tasks/todos extracted from response.
    TasksExtracted {
        todos_count: usize,
        tasks_count: usize,
    },

    // ── Plan events ──────────────────────────────────────────────────────────
    /// A plan has been created with steps.
    PlanCreated {
        plan_id: String,
        title: String,
        steps: Vec<PlanStepInfo>,
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
        steps: Vec<PlanStepInfo>,
    },

    // ── Workspace ────────────────────────────────────────────────────────────
    /// Workspace context update.
    Workspace(WorkspaceEvent),

    // ── Slash commands ───────────────────────────────────────────────────────
    /// Slash command execution result.
    Command(CommandEvent),

    // ── Milestone progress ───────────────────────────────────────────────────
    /// Milestone progress update from autonomous sequencing.
    MilestoneProgress(MilestoneProgress),
}

/// Lightweight plan step description for event messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanStepInfo {
    pub name: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_msg_serialization_roundtrip() {
        let cases = vec![
            EventMsg::TextDelta {
                delta: "hello".into(),
            },
            EventMsg::Done,
            EventMsg::Stopped {
                stop_reason: "end_turn".into(),
            },
            EventMsg::Error {
                kind: EventErrorKind::Provider,
                message: "rate limited".into(),
                retryable: true,
            },
            EventMsg::ToolExecStarted {
                tool_name: "Bash".into(),
                tool_id: "tool_1".into(),
            },
            EventMsg::ApprovalRequired {
                tool_name: "Write".into(),
                tool_id: "tool_2".into(),
                operation_class: crate::permission_modes::OperationClass::Write,
                description: "write to file.rs".into(),
                diff: None,
            },
            EventMsg::Workspace(WorkspaceEvent::ScanProgress {
                scanned: 10,
                total: 100,
            }),
            EventMsg::Command(CommandEvent::Success("done".into())),
        ];

        for msg in cases {
            let json = serde_json::to_string(&msg).unwrap();
            let back: EventMsg = serde_json::from_str(&json).unwrap();
            assert_eq!(msg, back);
        }
    }

    #[test]
    fn plan_step_info_roundtrip() {
        let info = PlanStepInfo {
            name: "Step 1".into(),
            description: "Do the thing".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: PlanStepInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }

    #[test]
    fn question_option_roundtrip() {
        let opt = QuestionOption {
            label: "Yes".into(),
            description: "Approve the change".into(),
        };
        let json = serde_json::to_string(&opt).unwrap();
        let back: QuestionOption = serde_json::from_str(&json).unwrap();
        assert_eq!(opt, back);
    }

    #[test]
    fn event_error_kind_roundtrip() {
        let kinds = vec![
            EventErrorKind::Provider,
            EventErrorKind::NoApiKey,
            EventErrorKind::MaxToolTurns,
            EventErrorKind::StreamDurationExceeded,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: EventErrorKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn non_exhaustive_allows_unknown_variants() {
        // Consumers should handle unknown variants gracefully
        let unknown = r#"{"type":"unknown_future_variant","data":{"foo":"bar"}}"#;
        let result: Result<EventMsg, _> = serde_json::from_str(unknown);
        // Should fail to deserialize (unknown variant) — consumers match
        // exhaustively with a catch-all; the `#[non_exhaustive]` attribute
        // ensures new variants can be added without breaking consumers.
        assert!(result.is_err());
    }
}
