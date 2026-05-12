//! EventMsg dispatcher — maps `rustycode_protocol::EventMsg` to existing handlers.
//!
//! This is the unified dispatch path for the Codex-style protocol boundary.
//! Each `EventMsg` variant is translated to the same underlying handler functions
//! used by the legacy `StreamChunk`/`ToolResult`/`WorkspaceUpdate` channels.
//! Once all producers emit `EventMsg`, the legacy paths can be retired.

use crate::app::TUI;
use rustycode_protocol::{EventErrorKind, EventMsg};

use super::stream_approval::{
    handle_approval_approved_chunk, handle_approval_rejected_chunk, handle_approval_request_chunk,
};
use super::stream_core::{handle_text_chunk, handle_thinking_chunk};
use super::stream_data::{
    handle_execution_trace_chunk, handle_extract_tasks_chunk, handle_file_snapshot_chunk,
    handle_question_answered_chunk, handle_question_request_chunk, handle_system_message_chunk,
    handle_tasks_extracted_chunk, handle_token_usage_chunk,
};
use super::stream_done::handle_done_chunk;
use super::stream_error::handle_error_chunk;
use super::stream_stopped::handle_stopped_chunk;
use super::stream_tools::{
    handle_tool_complete_chunk, handle_tool_progress_chunk, handle_tool_start_chunk,
};

use super::handle_slash_command_result;
use super::handle_workspace_update;

/// Convert `EventErrorKind` + message into a `StreamError`.
fn event_error_to_stream(
    kind: &EventErrorKind,
    message: String,
    _retryable: bool,
) -> crate::app::async_::StreamError {
    match kind {
        EventErrorKind::Provider => crate::app::async_::StreamError::Provider(
            rustycode_llm::ProviderError::Unknown(message),
        ),
        EventErrorKind::NoApiKey => crate::app::async_::StreamError::NoApiKey { provider: message },
        EventErrorKind::InvalidApiKey => {
            crate::app::async_::StreamError::InvalidApiKey { details: message }
        }
        EventErrorKind::MaxToolTurns => crate::app::async_::StreamError::MaxToolTurns {
            limit: message.parse().unwrap_or_else(|_| {
                tracing::debug!("Failed to parse MaxToolTurns limit from: {}", message);
                0
            }),
        },
        EventErrorKind::StreamDurationExceeded => {
            crate::app::async_::StreamError::StreamDurationExceeded
        }
        EventErrorKind::StreamIdleTimeout => crate::app::async_::StreamError::StreamIdleTimeout {
            seconds: message.parse().unwrap_or_else(|_| {
                tracing::debug!(
                    "Failed to parse StreamIdleTimeout seconds from: {}",
                    message
                );
                0
            }),
        },
        EventErrorKind::ContextBudgetExceeded => {
            crate::app::async_::StreamError::ContextBudgetExceeded
        }
        EventErrorKind::OrchestrationStepFailed
        | EventErrorKind::PipelineFailed
        | EventErrorKind::RuntimeError
        | EventErrorKind::InternalError
        | EventErrorKind::ApprovalChannelUnavailable
        | EventErrorKind::QuestionChannelUnavailable => {
            crate::app::async_::StreamError::PipelineFailed { reason: message }
        }
    }
}

/// Convert `WorkspaceEvent` (protocol) to `WorkspaceUpdate` (TUI).
fn workspace_event_to_update(
    event: rustycode_protocol::WorkspaceEvent,
) -> crate::app::async_::WorkspaceUpdate {
    use rustycode_protocol::WorkspaceEvent as WE;
    match event {
        WE::ScanProgress { scanned, total } => {
            crate::app::async_::WorkspaceUpdate::ScanProgress { scanned, total }
        }
        WE::ScanComplete {
            file_count,
            dir_count,
        } => crate::app::async_::WorkspaceUpdate::ScanComplete {
            file_count,
            dir_count,
        },
        WE::ContextLoaded(s) => crate::app::async_::WorkspaceUpdate::ContextLoaded(s),
        WE::Notice(s) => crate::app::async_::WorkspaceUpdate::Notice(s),
        WE::Error(s) => crate::app::async_::WorkspaceUpdate::Error(s),
    }
}

/// Convert `CommandEvent` (protocol) to `SlashCommandResult` (TUI).
fn command_event_to_result(
    event: rustycode_protocol::CommandEvent,
) -> crate::app::async_::SlashCommandResult {
    use rustycode_protocol::CommandEvent as CE;
    match event {
        CE::Success(s) => crate::app::async_::SlashCommandResult::Success(s),
        CE::Error(s) => crate::app::async_::SlashCommandResult::Error(s),
    }
}

/// Convert protocol `QuestionOption` to TUI `QuestionOption`.
fn question_options_to_tui(
    options: Vec<rustycode_protocol::QuestionOption>,
) -> Vec<crate::app::async_::QuestionOption> {
    options
        .into_iter()
        .map(|o| crate::app::async_::QuestionOption {
            label: o.label,
            description: o.description,
        })
        .collect()
}

/// Main dispatch: map an `EventMsg` to existing TUI handler functions.
pub fn handle_event_msg(tui: &mut TUI, msg: EventMsg) {
    match msg {
        // ── Streaming deltas ─────────────────────────────────────────────
        EventMsg::TextDelta { delta } => handle_text_chunk(tui, delta),
        EventMsg::ThinkingDelta { delta } => handle_thinking_chunk(tui, delta),
        EventMsg::ThinkingBlockCompleted { .. } => {
            // No existing handler — metadata-only, not displayed in TUI
        }

        // ── Turn lifecycle ───────────────────────────────────────────────
        EventMsg::TurnStarted { .. } => {
            // No existing handler — metadata for future use
        }
        EventMsg::TurnCompleted { .. } => {
            // Handled through Done/Stopped path
        }

        // ── Tool execution ───────────────────────────────────────────────
        EventMsg::ToolCallStarted {
            tool_name,
            tool_id,
            input,
        } => handle_tool_start_chunk(tui, tool_name, tool_id, input.to_string()),
        EventMsg::ToolInputDelta { .. } => {
            // No existing handler — input delta is displayed inline with ToolCallStarted
        }
        EventMsg::ToolExecStarted { tool_name, tool_id } => {
            // Reuse tool_start with empty input — the TUI shows tool started state
            handle_tool_start_chunk(tui, tool_name, tool_id, String::new());
        }
        EventMsg::ToolExecProgress {
            tool_id,
            stage,
            elapsed_ms,
            preview,
        } => handle_tool_progress_chunk(
            tui,
            Some(tool_id),
            String::new(),
            stage,
            elapsed_ms,
            preview,
        ),
        EventMsg::ToolExecCompleted {
            tool_id,
            tool_name,
            success,
            output,
            output_size,
            duration_ms,
            ..
        } => handle_tool_complete_chunk(
            tui,
            tool_name,
            tool_id,
            duration_ms,
            success,
            output_size,
            Some(output),
        ),
        EventMsg::FileSnapshot { batch } => handle_file_snapshot_chunk(tui, batch),

        // ── Token usage ──────────────────────────────────────────────────
        EventMsg::TokenUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        } => handle_token_usage_chunk(
            tui,
            input_tokens as usize,
            output_tokens as usize,
            cache_read_tokens as usize,
            cache_creation_tokens as usize,
        ),

        // ── Session lifecycle ────────────────────────────────────────────
        EventMsg::Done => handle_done_chunk(tui),
        EventMsg::Stopped { stop_reason } => handle_stopped_chunk(tui, stop_reason),
        EventMsg::Error {
            kind,
            message,
            retryable,
        } => {
            let err = event_error_to_stream(&kind, message, retryable);
            handle_error_chunk(tui, err);
        }
        EventMsg::ExecutionTrace(trace) => handle_execution_trace_chunk(tui, trace),
        EventMsg::SystemMessage(msg) => handle_system_message_chunk(tui, msg),

        // ── Tool approval ────────────────────────────────────────────────
        EventMsg::ApprovalRequired {
            tool_name,
            tool_id,
            description,
            diff,
            ..
        } => handle_approval_request_chunk(tui, tool_name, tool_id, description, diff),
        EventMsg::ApprovalApproved { tool_id } => handle_approval_approved_chunk(tui, tool_id),
        EventMsg::ApprovalRejected { tool_id } => handle_approval_rejected_chunk(tui, tool_id),

        // ── User questions ───────────────────────────────────────────────
        EventMsg::QuestionRequired {
            question_id,
            question_text,
            header,
            options,
            multi_select,
        } => handle_question_request_chunk(
            tui,
            question_id,
            question_text,
            header,
            question_options_to_tui(options),
            multi_select,
        ),
        EventMsg::QuestionAnswered {
            question_id,
            answer,
        } => handle_question_answered_chunk(tui, question_id, answer),

        // ── Task extraction ──────────────────────────────────────────────
        EventMsg::ExtractTasks { text } => handle_extract_tasks_chunk(tui, text),
        EventMsg::TasksExtracted {
            todos_count,
            tasks_count,
        } => handle_tasks_extracted_chunk(tui, todos_count, tasks_count),

        // ── Plan events ──────────────────────────────────────────────────
        EventMsg::PlanCreated { .. }
        | EventMsg::PlanStepStarted { .. }
        | EventMsg::PlanStepCompleted { .. }
        | EventMsg::PlanCompleted { .. }
        | EventMsg::PlanApprovalRequested { .. } => {
            // Plan UI not yet wired — plan events are logged for future use
            tracing::debug!("Plan event received (not yet displayed): {:?}", msg);
        }

        // ── Workspace ────────────────────────────────────────────────────
        EventMsg::Workspace(event) => {
            let update = workspace_event_to_update(event);
            handle_workspace_update(tui, update);
        }

        // ── Slash commands ───────────────────────────────────────────────
        EventMsg::Command(event) => {
            let result = command_event_to_result(event);
            handle_slash_command_result(tui, result);
        }

        // ── Milestone progress ───────────────────────────────────────────
        EventMsg::MilestoneProgress(mp) => {
            use rustycode_protocol::MilestoneStatus;
            let status = match mp.status.to_lowercase().as_str() {
                "draft" => MilestoneStatus::Draft,
                "planning" => MilestoneStatus::Planning,
                "ready" => MilestoneStatus::Ready,
                "active" => MilestoneStatus::Active,
                "validating" => MilestoneStatus::Validating,
                "completed" => MilestoneStatus::Completed,
                "paused" => MilestoneStatus::Paused,
                "failed" => MilestoneStatus::Failed,
                other => {
                    tracing::debug!("Unknown MilestoneStatus '{}', defaulting to Active", other);
                    MilestoneStatus::Active
                }
            };
            super::stream_data::handle_milestone_progress_chunk(
                tui,
                mp.milestone_id,
                mp.milestone_title,
                status,
                mp.plans_total,
                mp.plans_completed,
                mp.current_plan_summary,
                mp.action_hint,
                Vec::new(), // plan_rows not available in EventMsg path
            );
        }

        // ── Future variants (#[non_exhaustive]) ─────────────────────────
        _ => {
            tracing::debug!("Unknown EventMsg variant ignored: {:?}", msg);
        }
    }
}
