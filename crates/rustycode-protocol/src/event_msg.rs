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
    /// Tool execution was blocked by policy.
    ToolBlocked {
        tool_name: String,
        tool_id: String,
        reason: String,
    },
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
    /// A new core session has started.
    SessionStarted { session_id: String, task: String },
    /// A core session has stopped.
    SessionStopped { session_id: String, reason: String },
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

    // ── Messages ─────────────────────────────────────────────────────────────
    /// A new message was added to the conversation history.
    MessageAdded { role: String, content: String },

    // ── Slash commands ───────────────────────────────────────────────────────
    /// Slash command execution result.
    Command(CommandEvent),

    // ── Milestone progress ───────────────────────────────────────────────────
    /// Milestone progress update from autonomous sequencing.
    MilestoneProgress(MilestoneProgress),

    // ── Orchestration & Strategy ─────────────────────────────────────────────
    /// Orchestration phase transition.
    PhaseTransition {
        from: String,
        to: String,
        reason: Option<String>,
    },
    /// Strategy switch (e.g. from fast to deep reasoning).
    StrategySwitch {
        from: String,
        to: String,
        reason: String,
    },
    /// Quality gate result from an evaluation step.
    QualityGateResult {
        gate_name: String,
        passed: bool,
        score: Option<f64>,
        details: String,
    },

    // ── Memory Operations ────────────────────────────────────────────────────
    /// Memory operation result.
    MemoryOperation {
        op_type: String, // Created, Updated, Deleted, Listed
        memory_id: String,
        content: Option<String>,
        error: Option<String>,
    },
}

/// Lightweight plan step description for event messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanStepInfo {
    pub name: String,
    pub description: String,
}

/// Convert an `EventMsg` back to a `StreamEvent` for consumers that still
/// process `StreamEvent` internally (e.g. the TUI during Phase 1 migration).
///
/// Returns `None` for EventMsg-only variants that have no StreamEvent equivalent
/// (approval, question, workspace, orchestration, etc.).
///
/// Note: Some information is lost in the round-trip (e.g. `input` on ToolCallStarted,
/// `output_size`/`duration_ms` on ToolExecCompleted, cache tokens merged into TokenUsage).
pub fn event_msg_to_stream_event(msg: EventMsg) -> Option<crate::stream_event::StreamEvent> {
    use crate::stream_event::{StreamEvent, StreamPlanStep};

    match msg {
        EventMsg::TextDelta { delta } => Some(StreamEvent::TextDelta { content: delta }),
        EventMsg::ThinkingDelta { delta } => Some(StreamEvent::ThinkingDelta { content: delta }),
        EventMsg::ThinkingBlockCompleted {
            block_type,
            signature,
            data,
        } => Some(StreamEvent::ThinkingBlockCompleted {
            block_type,
            signature,
            data,
        }),
        EventMsg::TurnStarted { turn } => Some(StreamEvent::TurnStarted { turn }),
        EventMsg::TurnCompleted { stop_reason } => Some(StreamEvent::TurnCompleted { stop_reason }),
        EventMsg::ToolCallStarted {
            tool_id, tool_name, ..
        } => Some(StreamEvent::ToolCallStarted {
            id: tool_id,
            name: tool_name,
        }),
        EventMsg::ToolInputDelta { tool_id, delta } => Some(StreamEvent::ToolInputDelta {
            id: tool_id,
            chunk: delta,
        }),
        EventMsg::ToolExecStarted { tool_id, tool_name } => Some(StreamEvent::ToolExecStarted {
            id: tool_id,
            name: tool_name,
        }),
        EventMsg::ToolExecCompleted {
            tool_id,
            tool_name,
            success,
            output,
            ..
        } => Some(StreamEvent::ToolExecCompleted {
            id: tool_id,
            name: tool_name,
            output,
            is_error: !success,
        }),
        EventMsg::TokenUsage {
            input_tokens,
            output_tokens,
            ..
        } => Some(StreamEvent::TokenUsage {
            input_tokens,
            output_tokens,
        }),
        EventMsg::Done => Some(StreamEvent::Done),
        EventMsg::PlanCreated {
            plan_id,
            title,
            steps,
        } => Some(StreamEvent::PlanCreated {
            id: plan_id,
            title,
            steps: steps
                .into_iter()
                .map(|s| StreamPlanStep {
                    name: s.name,
                    description: s.description,
                })
                .collect(),
        }),
        EventMsg::PlanStepStarted {
            plan_id,
            step_index,
        } => Some(StreamEvent::PlanStepStarted {
            plan_id,
            step_index,
        }),
        EventMsg::PlanStepCompleted {
            plan_id,
            step_index,
            success,
            message,
        } => Some(StreamEvent::PlanStepCompleted {
            plan_id,
            step_index,
            success,
            message,
        }),
        EventMsg::PlanCompleted {
            plan_id,
            success,
            summary,
        } => Some(StreamEvent::PlanCompleted {
            plan_id,
            success,
            summary,
        }),
        EventMsg::PlanApprovalRequested {
            plan_id,
            title,
            steps,
        } => Some(StreamEvent::PlanApprovalRequested {
            plan_id,
            title,
            steps: steps
                .into_iter()
                .map(|s| StreamPlanStep {
                    name: s.name,
                    description: s.description,
                })
                .collect(),
        }),

        // EventMsg-only variants — no StreamEvent equivalent
        EventMsg::ToolBlocked { .. }
        | EventMsg::ToolExecProgress { .. }
        | EventMsg::FileSnapshot { .. }
        | EventMsg::SessionStarted { .. }
        | EventMsg::SessionStopped { .. }
        | EventMsg::Stopped { .. }
        | EventMsg::Error { .. }
        | EventMsg::ExecutionTrace(_)
        | EventMsg::SystemMessage(_)
        | EventMsg::ApprovalRequired { .. }
        | EventMsg::ApprovalApproved { .. }
        | EventMsg::ApprovalRejected { .. }
        | EventMsg::QuestionRequired { .. }
        | EventMsg::QuestionAnswered { .. }
        | EventMsg::ExtractTasks { .. }
        | EventMsg::TasksExtracted { .. }
        | EventMsg::Workspace(_)
        | EventMsg::MessageAdded { .. }
        | EventMsg::Command(_)
        | EventMsg::MilestoneProgress(_)
        | EventMsg::PhaseTransition { .. }
        | EventMsg::StrategySwitch { .. }
        | EventMsg::QualityGateResult { .. }
        | EventMsg::MemoryOperation { .. } => None,
    }
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

    #[test]
    fn event_msg_to_stream_event_converts_stream_equivalents() {
        use crate::stream_event::StreamEvent;

        let cases: Vec<(EventMsg, StreamEvent)> = vec![
            (
                EventMsg::TextDelta {
                    delta: "hello".into(),
                },
                StreamEvent::TextDelta {
                    content: "hello".into(),
                },
            ),
            (
                EventMsg::ThinkingDelta {
                    delta: "hmm".into(),
                },
                StreamEvent::ThinkingDelta {
                    content: "hmm".into(),
                },
            ),
            (
                EventMsg::ThinkingBlockCompleted {
                    block_type: "thinking".into(),
                    signature: "sig".into(),
                    data: "data".into(),
                },
                StreamEvent::ThinkingBlockCompleted {
                    block_type: "thinking".into(),
                    signature: "sig".into(),
                    data: "data".into(),
                },
            ),
            (
                EventMsg::TurnStarted { turn: 3 },
                StreamEvent::TurnStarted { turn: 3 },
            ),
            (
                EventMsg::TurnCompleted {
                    stop_reason: "tool_use".into(),
                },
                StreamEvent::TurnCompleted {
                    stop_reason: "tool_use".into(),
                },
            ),
            (
                EventMsg::ToolCallStarted {
                    tool_id: "t1".into(),
                    tool_name: "Bash".into(),
                    input: serde_json::json!({}),
                },
                StreamEvent::ToolCallStarted {
                    id: "t1".into(),
                    name: "Bash".into(),
                },
            ),
            (
                EventMsg::ToolInputDelta {
                    tool_id: "t1".into(),
                    delta: r#"{"cmd":"ls"}"#.into(),
                },
                StreamEvent::ToolInputDelta {
                    id: "t1".into(),
                    chunk: r#"{"cmd":"ls"}"#.into(),
                },
            ),
            (
                EventMsg::ToolExecStarted {
                    tool_id: "t1".into(),
                    tool_name: "Bash".into(),
                },
                StreamEvent::ToolExecStarted {
                    id: "t1".into(),
                    name: "Bash".into(),
                },
            ),
            (
                EventMsg::ToolExecCompleted {
                    tool_id: "t1".into(),
                    tool_name: "Bash".into(),
                    success: true,
                    output: "ok".into(),
                    output_size: 2,
                    duration_ms: 100,
                },
                StreamEvent::ToolExecCompleted {
                    id: "t1".into(),
                    name: "Bash".into(),
                    output: "ok".into(),
                    is_error: false,
                },
            ),
            (
                EventMsg::TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                },
                StreamEvent::TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                },
            ),
            (EventMsg::Done, StreamEvent::Done),
        ];

        for (msg, expected) in cases {
            let result = event_msg_to_stream_event(msg).expect("should convert");
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn event_msg_to_stream_event_returns_none_for_msg_only_variants() {
        let msg_only: Vec<EventMsg> = vec![
            EventMsg::Stopped {
                stop_reason: "end_turn".into(),
            },
            EventMsg::Error {
                kind: EventErrorKind::Provider,
                message: "err".into(),
                retryable: false,
            },
            EventMsg::ApprovalRequired {
                tool_name: "Write".into(),
                tool_id: "t1".into(),
                operation_class: crate::permission_modes::OperationClass::Write,
                description: "write file".into(),
                diff: None,
            },
            EventMsg::QuestionRequired {
                question_id: "q1".into(),
                question_text: "Continue?".into(),
                header: "Confirm".into(),
                options: vec![],
                multi_select: false,
            },
            EventMsg::Workspace(WorkspaceEvent::ScanProgress {
                scanned: 1,
                total: 10,
            }),
            EventMsg::Command(CommandEvent::Success("ok".into())),
            EventMsg::MilestoneProgress(MilestoneProgress {
                milestone_id: "m1".into(),
                milestone_title: "Phase 1".into(),
                status: "in_progress".into(),
                plans_total: 3,
                plans_completed: 1,
                current_plan_summary: "doing".into(),
                action_hint: "wait".into(),
            }),
        ];

        for msg in msg_only {
            assert!(
                event_msg_to_stream_event(msg).is_none(),
                "EventMsg-only variant should return None"
            );
        }
    }

    #[test]
    fn event_msg_to_stream_event_plan_roundtrip() {
        use crate::stream_event::StreamEvent;

        let msg = EventMsg::PlanCreated {
            plan_id: "p1".into(),
            title: "My Plan".into(),
            steps: vec![
                PlanStepInfo {
                    name: "Step 1".into(),
                    description: "Do thing".into(),
                },
                PlanStepInfo {
                    name: "Step 2".into(),
                    description: "Do other".into(),
                },
            ],
        };

        let result = event_msg_to_stream_event(msg).expect("should convert");
        match result {
            StreamEvent::PlanCreated { id, title, steps } => {
                assert_eq!(id, "p1");
                assert_eq!(title, "My Plan");
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].name, "Step 1");
            }
            other => panic!("Expected PlanCreated, got {:?}", other),
        }
    }
}
