//! Main stream chunk dispatcher.

use crate::app::async_::StreamChunk;
use crate::app::TUI;

use super::stream_approval::{
    handle_approval_approved_chunk, handle_approval_rejected_chunk, handle_approval_request_chunk,
};
use super::stream_data::{
    handle_execution_trace_chunk, handle_extract_tasks_chunk, handle_file_snapshot_chunk,
    handle_milestone_progress_chunk, handle_question_answered_chunk, handle_question_request_chunk,
    handle_system_message_chunk, handle_tasks_extracted_chunk, handle_todo_sync_chunk,
    handle_token_usage_chunk,
};
use super::stream_done::handle_done_chunk;
use super::stream_error::handle_error_chunk;
use super::stream_stopped::handle_stopped_chunk;
use super::stream_tools::{
    handle_tool_complete_chunk, handle_tool_progress_chunk, handle_tool_start_chunk,
};

fn handle_text_chunk(tui: &mut TUI, text: String) {
    // Capture stream start time on first chunk (Goose pattern: response timing)
    if tui.streaming.stream_start_time.is_none() {
        tui.streaming.stream_start_time = Some(std::time::Instant::now());
    }

    // Feed through the streaming render buffer for safe markdown boundaries.
    // The buffer holds back incomplete markdown (unclosed bold, code blocks, etc.)
    // and returns complete segments safe for rendering.
    let safe_text = tui.streaming.streaming_render_buffer.push(&text);

    if let Some(renderable) = safe_text {
        // Append safe content to current stream content only.
        // The assistant message's .content is set atomically in
        // StreamChunk::Done to avoid text duplication.
        tui.streaming
            .current_stream_content
            .reserve(renderable.len());
        tui.streaming.current_stream_content.push_str(&renderable);

        tui.streaming.is_streaming = true;
        tui.streaming.chunks_received += 1;
        // Update terminal title on first chunk (state transition to "thinking")
        if tui.streaming.chunks_received == 1 {
            tui.update_terminal_title();
        }
        if tui.renderer_mode.is_brutalist() {
            if !tui.view.user_scrolled {
                tui.auto_scroll();
            }
            tui.dirty = true;
        }
    } else {
        // Buffer is holding incomplete markdown — still mark streaming
        // so the UI shows the spinner, but don't dirty (no render change).
        tui.streaming.is_streaming = true;
    }
    // NOTE: Do NOT clear stream_cancelled here!
    // The user may have pressed Esc/Ctrl+D to cancel while chunks
    // are still in-flight. If we clear the flag on every Text chunk,
    // a late-arriving chunk would un-cancel the stream, causing the
    // Done handler to treat it as a successful completion and trigger
    // auto-continue or queued message send. The flag is properly
    // reset in the Done/Error handlers.
}

fn handle_thinking_chunk(tui: &mut TUI, mut thinking: String) {
    const MAX_THINKING_BYTES: usize = 50 * 1024;
    tui.streaming.thinking_chunks_received += 1;
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

    tui.streaming.is_streaming = true;

    // Take a turn snapshot on first streaming chunk so we can
    // verify file changes when the turn completes.
    if tui.turn_snapshot.is_none() {
        let cwd = std::env::current_dir().unwrap_or_default();
        tui.turn_snapshot = Some(crate::app::turn_snapshot::TurnSnapshot::take(&cwd));
    }

    if !tui.view.user_scrolled {
        tui.auto_scroll();
    }
    tui.dirty = true;
}

pub fn handle_stream_chunk(tui: &mut TUI, chunk: StreamChunk) {
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
        StreamChunk::TodoSync => handle_todo_sync_chunk(tui),
    }
}
