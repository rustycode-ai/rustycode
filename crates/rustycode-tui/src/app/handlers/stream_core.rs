//! Main stream chunk dispatcher.

use crate::app::async_::StreamChunk;
use crate::app::TUI;

use super::helpers::mark_dirty_and_scroll;
use rustycode_protocol::{
    CommandEvent, EventErrorKind, EventMsg, MilestoneProgress as EventMilestoneProgress,
    PlanStepInfo, QuestionOption, ToolOutput as EventToolOutput, WorkspaceEvent,
};

use super::stream_approval::{
    handle_approval_approved_chunk, handle_approval_rejected_chunk, handle_approval_request_chunk,
};
use super::stream_data::{
    handle_execution_trace_chunk, handle_extract_tasks_chunk, handle_file_snapshot_chunk,
    handle_milestone_progress_chunk, handle_question_answered_chunk, handle_question_request_chunk,
    handle_system_message_chunk, handle_tasks_extracted_chunk, handle_token_usage_chunk,
};
use super::stream_done::handle_done_chunk;
use super::stream_error::handle_error_chunk;
use super::stream_stopped::handle_stopped_chunk;
use super::stream_tools::{
    handle_tool_complete_chunk, handle_tool_progress_chunk, handle_tool_start_chunk,
};
use crate::tool_approval::risk;
use rustycode_protocol::permission_modes::OperationClass;

/// Emit an `EventMsg` equivalent for a `StreamChunk` to the unified event channel.
///
/// This is the additive migration path: existing typed channels continue to
/// function, and consumers can switch to `EventMsg` at their own pace.
fn emit_event_for_chunk(tui: &mut TUI, chunk: &StreamChunk) {
    use rustycode_llm::provider::ProviderError;

    let event = match chunk {
        // ── Text / Thinking ──────────────────────────────────────────────
        StreamChunk::Text(text) => EventMsg::TextDelta {
            delta: text.clone(),
        },
        StreamChunk::Thinking(text) => EventMsg::ThinkingDelta {
            delta: text.clone(),
        },

        // ── Lifecycle ────────────────────────────────────────────────────
        StreamChunk::Done => EventMsg::Done,
        StreamChunk::Stopped { stop_reason } => EventMsg::Stopped {
            stop_reason: stop_reason.clone(),
        },
        StreamChunk::Error(err) => {
            let kind = match err {
                crate::app::async_::StreamError::Provider(p) => match p {
                    ProviderError::Auth(_) => EventErrorKind::Provider,
                    ProviderError::RateLimited { .. } => EventErrorKind::Provider,
                    ProviderError::Network(_) => EventErrorKind::Provider,
                    ProviderError::ContextLengthExceeded(_) => {
                        EventErrorKind::ContextBudgetExceeded
                    }
                    ProviderError::CreditsExhausted { .. } => EventErrorKind::Provider,
                    ProviderError::InvalidModel(_) => EventErrorKind::Provider,
                    ProviderError::Timeout(_) => EventErrorKind::Provider,
                    ProviderError::Api(_) => EventErrorKind::Provider,
                    _ => EventErrorKind::Provider,
                },
                crate::app::async_::StreamError::NoApiKey { .. } => EventErrorKind::NoApiKey,
                crate::app::async_::StreamError::InvalidApiKey { .. } => {
                    EventErrorKind::InvalidApiKey
                }
                crate::app::async_::StreamError::MaxToolTurns { .. } => {
                    EventErrorKind::MaxToolTurns
                }
                crate::app::async_::StreamError::StreamDurationExceeded => {
                    EventErrorKind::StreamDurationExceeded
                }
                crate::app::async_::StreamError::StreamIdleTimeout { .. } => {
                    EventErrorKind::StreamIdleTimeout
                }
                crate::app::async_::StreamError::ContextBudgetExceeded => {
                    EventErrorKind::ContextBudgetExceeded
                }
                crate::app::async_::StreamError::OrchestrationStepFailed { .. } => {
                    EventErrorKind::OrchestrationStepFailed
                }
                crate::app::async_::StreamError::PipelineFailed { .. } => {
                    EventErrorKind::PipelineFailed
                }
                crate::app::async_::StreamError::RuntimeError { .. } => {
                    EventErrorKind::RuntimeError
                }
                crate::app::async_::StreamError::InternalError { .. } => {
                    EventErrorKind::InternalError
                }
                crate::app::async_::StreamError::ApprovalChannelUnavailable => {
                    EventErrorKind::ApprovalChannelUnavailable
                }
                crate::app::async_::StreamError::QuestionChannelUnavailable => {
                    EventErrorKind::QuestionChannelUnavailable
                }
            };
            EventMsg::Error {
                kind,
                message: err.to_string(),
                retryable: err.is_retryable(),
            }
        }

        // ── Tool execution ───────────────────────────────────────────────
        StreamChunk::ToolStart {
            tool_name,
            tool_id,
            input_json,
        } => {
            let input: serde_json::Value =
                serde_json::from_str(input_json).unwrap_or(serde_json::Value::Null);
            EventMsg::ToolCallStarted {
                tool_name: tool_name.clone(),
                tool_id: tool_id.clone(),
                input,
            }
        }
        StreamChunk::ToolProgress {
            tool_id,
            tool_name: _,
            stage,
            elapsed_ms,
            output_preview,
        } => EventMsg::ToolExecProgress {
            tool_id: tool_id.clone().unwrap_or_default(),
            stage: stage.clone(),
            elapsed_ms: *elapsed_ms,
            preview: output_preview.clone(),
        },
        StreamChunk::ToolComplete {
            tool_name,
            tool_id,
            duration_ms,
            success,
            output_size,
            output,
        } => EventMsg::ToolExecCompleted {
            tool_id: tool_id.clone(),
            tool_name: tool_name.clone(),
            success: *success,
            output: output.clone().unwrap_or_default(),
            output_size: *output_size,
            duration_ms: *duration_ms,
            exit_code: None,
        },

        // ── Approval ─────────────────────────────────────────────────────
        StreamChunk::ApprovalRequest {
            tool_name,
            tool_id,
            description,
            diff,
        } => EventMsg::ApprovalRequired {
            operation_class: {
                let tool_type = risk::classify_tool_type(tool_name);
                let command = diff.as_deref().unwrap_or(description.as_str());
                match risk::classify_tool_risk(&tool_type, command) {
                    risk::RiskLevel::Safe => OperationClass::ReadOnly,
                    risk::RiskLevel::Medium => OperationClass::Write,
                    risk::RiskLevel::High | risk::RiskLevel::Dangerous => {
                        OperationClass::Destructive
                    }
                }
            },
            tool_name: tool_name.clone(),
            tool_id: tool_id.clone(),
            description: description.clone(),
            diff: diff.clone(),
        },
        StreamChunk::ApprovalApproved { tool_id } => EventMsg::ApprovalApproved {
            tool_id: tool_id.clone(),
        },
        StreamChunk::ApprovalRejected { tool_id } => EventMsg::ApprovalRejected {
            tool_id: tool_id.clone(),
        },

        // ── Questions ────────────────────────────────────────────────────
        StreamChunk::QuestionRequest {
            question_id,
            question_text,
            header,
            options,
            multi_select,
        } => EventMsg::QuestionRequired {
            question_id: question_id.clone(),
            question_text: question_text.clone(),
            header: header.clone(),
            options: options
                .iter()
                .map(|o| QuestionOption {
                    label: o.label.clone(),
                    description: o.description.clone(),
                })
                .collect(),
            multi_select: *multi_select,
        },
        StreamChunk::QuestionAnswered {
            question_id,
            answer,
        } => EventMsg::QuestionAnswered {
            question_id: question_id.clone(),
            answer: answer.clone(),
        },

        // ── Tasks ────────────────────────────────────────────────────────
        StreamChunk::ExtractTasks { text } => EventMsg::ExtractTasks { text: text.clone() },
        StreamChunk::TasksExtracted {
            todos_count,
            tasks_count,
        } => EventMsg::TasksExtracted {
            todos_count: *todos_count,
            tasks_count: *tasks_count,
        },

        // ── File snapshots ───────────────────────────────────────────────
        StreamChunk::FileSnapshot { batch } => EventMsg::FileSnapshot {
            batch: batch.clone(),
        },

        // ── Token usage ──────────────────────────────────────────────────
        StreamChunk::TokenUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        } => EventMsg::TokenUsage {
            input_tokens: *input_tokens as u64,
            output_tokens: *output_tokens as u64,
            cache_read_tokens: *cache_read_tokens as u64,
            cache_creation_tokens: *cache_creation_tokens as u64,
        },
        StreamChunk::CacheUsage { .. } => {
            // CacheUsage is a subset of TokenUsage — we skip emitting a separate
            // EventMsg; TokenUsage emission already covers the main telemetry.
            return;
        }

        // ── Execution trace ──────────────────────────────────────────────
        StreamChunk::ExecutionTrace(trace) => EventMsg::ExecutionTrace(trace.clone()),

        // ── System messages ──────────────────────────────────────────────
        StreamChunk::SystemMessage(msg) => EventMsg::SystemMessage(msg.clone()),

        // ── Milestone progress ───────────────────────────────────────────
        StreamChunk::MilestoneProgress {
            milestone_id,
            milestone_title,
            status,
            plans_total,
            plans_completed,
            current_plan_summary,
            action_hint,
            ..
        } => EventMsg::MilestoneProgress(EventMilestoneProgress {
            milestone_id: milestone_id.clone(),
            milestone_title: milestone_title.clone(),
            status: format!("{:?}", status),
            plans_total: *plans_total,
            plans_completed: *plans_completed,
            current_plan_summary: current_plan_summary.clone(),
            action_hint: action_hint.clone(),
        }),
    };

    tui.integration.services.send_event(event);
}

pub fn handle_text_chunk(tui: &mut TUI, text: String) {
    // Capture stream start time on first chunk (response timing)
    if tui.session.streaming.stream_start_time.is_none() {
        tui.session.streaming.stream_start_time = Some(std::time::Instant::now());
    }

    // Feed through the streaming render buffer for safe markdown boundaries.
    // The buffer holds back incomplete markdown (unclosed bold, code blocks, etc.)
    // and returns complete segments safe for rendering.
    let safe_text = tui.session.streaming.streaming_render_buffer.push(&text);

    if let Some(renderable) = safe_text {
        // Append safe content to current stream content only.
        // The assistant message's .content is set atomically in
        // StreamChunk::Done to avoid text duplication.
        tui.session
            .streaming
            .current_stream_content
            .reserve(renderable.len());
        tui.session
            .streaming
            .current_stream_content
            .push_str(&renderable);

        tui.session.streaming.is_streaming = true;
        tui.session.streaming.chunks_received += 1;
        // Update terminal title on first chunk (state transition to "thinking")
        if tui.session.streaming.chunks_received == 1 {
            tui.update_terminal_title();
        }
        if tui.sys.renderer_mode.is_brutalist() {
            mark_dirty_and_scroll(tui);
        }
    } else {
        // Buffer is holding incomplete markdown — still mark streaming
        // so the UI shows the spinner, but don't dirty (no render change).
        tui.session.streaming.is_streaming = true;
    }
    // NOTE: Do NOT clear stream_cancelled here!
    // The user may have pressed Esc/Ctrl+D to cancel while chunks
    // are still in-flight. If we clear the flag on every Text chunk,
    // a late-arriving chunk would un-cancel the stream, causing the
    // Done handler to treat it as a successful completion and trigger
    // auto-continue or queued message send. The flag is properly
    // reset in the Done/Error handlers.
}

pub fn handle_thinking_chunk(tui: &mut TUI, mut thinking: String) {
    const MAX_THINKING_BYTES: usize = 50 * 1024;
    tui.session.streaming.thinking_chunks_received += 1;
    let assistant_msg = tui.last_assistant_message_mut();
    if let Some(last_msg) = assistant_msg {
        if let Some(existing) = &mut last_msg.thinking {
            if existing.len() + thinking.len() > MAX_THINKING_BYTES {
                let limit = existing.floor_char_boundary(MAX_THINKING_BYTES.saturating_sub(3));
                existing.truncate(limit);
                existing.push_str("...");
            } else {
                existing.push_str(&thinking);
            }
        } else {
            if thinking.len() > MAX_THINKING_BYTES {
                let limit = thinking.floor_char_boundary(MAX_THINKING_BYTES.saturating_sub(3));
                thinking.truncate(limit);
                thinking.push_str("...");
            }
            last_msg.thinking = Some(thinking);
        }
    }

    tui.session.streaming.is_streaming = true;

    // Take a turn snapshot on first streaming chunk so we can
    // verify file changes when the turn completes.
    if tui.session.turn_snapshot.is_none() {
        let cwd = std::env::current_dir().unwrap_or_default();
        tui.session.turn_snapshot = Some(crate::app::turn_snapshot::TurnSnapshot::take(&cwd));
    }

    mark_dirty_and_scroll(tui);
}

pub fn handle_stream_chunk(tui: &mut TUI, chunk: StreamChunk) {
    // NOTE: We do NOT call emit_event_for_chunk() here. Producers should
    // emit to ONE channel (legacy StreamChunk OR unified EventMsg), not both.
    // The EventMsg polling path in service_polling.rs handles events from
    // producers that have migrated to the unified channel.

    match chunk {
        StreamChunk::Text(text) => handle_text_chunk(tui, text),
        StreamChunk::Thinking(thinking) => handle_thinking_chunk(tui, thinking),
        StreamChunk::Done => handle_done_chunk(tui),
        StreamChunk::Error(err) => handle_error_chunk(tui, err),
        StreamChunk::ToolStart {
            tool_name,
            tool_id,
            input_json: input_json_str,
        } => handle_tool_start_chunk(tui, tool_name, tool_id, input_json_str),
        StreamChunk::ToolProgress {
            tool_id,
            tool_name,
            stage,
            elapsed_ms,
            output_preview,
        } => handle_tool_progress_chunk(tui, tool_id, tool_name, stage, elapsed_ms, output_preview),
        StreamChunk::ToolComplete {
            tool_name,
            tool_id,
            duration_ms,
            success,
            output_size,
            output,
        } => handle_tool_complete_chunk(
            tui,
            tool_name,
            tool_id,
            duration_ms,
            success,
            output_size,
            output,
        ),
        StreamChunk::ExtractTasks { text } => handle_extract_tasks_chunk(tui, text),
        StreamChunk::TasksExtracted {
            todos_count,
            tasks_count,
        } => handle_tasks_extracted_chunk(tui, todos_count, tasks_count),
        StreamChunk::ApprovalRequest {
            tool_name,
            tool_id,
            description,
            diff,
        } => handle_approval_request_chunk(tui, tool_name, tool_id, description, diff),
        StreamChunk::ApprovalApproved { tool_id } => handle_approval_approved_chunk(tui, tool_id),
        StreamChunk::ApprovalRejected { tool_id } => handle_approval_rejected_chunk(tui, tool_id),
        StreamChunk::QuestionRequest {
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
            options,
            multi_select,
        ),
        StreamChunk::QuestionAnswered {
            question_id,
            answer,
        } => handle_question_answered_chunk(tui, question_id, answer),
        StreamChunk::FileSnapshot { batch } => handle_file_snapshot_chunk(tui, batch),
        StreamChunk::TokenUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        } => handle_token_usage_chunk(
            tui,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        ),
        StreamChunk::CacheUsage {
            cache_read_tokens,
            cache_creation_tokens,
        } => {
            tui.model.token_budget.session_cache_read_tokens += cache_read_tokens;
            tui.model.token_budget.session_cache_creation_tokens += cache_creation_tokens;
        }
        StreamChunk::ExecutionTrace(trace) => handle_execution_trace_chunk(tui, trace),
        StreamChunk::SystemMessage(msg) => handle_system_message_chunk(tui, msg),
        StreamChunk::MilestoneProgress {
            milestone_id,
            milestone_title,
            status,
            plans_total,
            plans_completed,
            current_plan_summary,
            action_hint,
            plan_rows,
        } => handle_milestone_progress_chunk(
            tui,
            milestone_id,
            milestone_title,
            status,
            plans_total,
            plans_completed,
            current_plan_summary,
            action_hint,
            plan_rows,
        ),
        StreamChunk::Stopped { stop_reason } => handle_stopped_chunk(tui, stop_reason),
    }
}
