//! Stream completion handler — decomposed into focused helpers.
//!
//! Handles flushing buffered content, transferring stream content to messages,
//! auto-continue logic, doom loop detection, toast notifications, and queued messages.

use crate::app::TUI;
use crate::ui::message::{Message, MessageRole};
use tracing;

use super::helpers::check_and_trigger_auto_continue;

pub(super) fn handle_done_chunk(tui: &mut TUI) {
    let had_stream_content = flush_and_transfer_stream_content(tui);

    let was_cancelled = tui.streaming.stream_cancelled;
    tui.streaming.is_streaming = false;
    tui.streaming.stream_cancelled = false;
    tui.update_terminal_title();

    if let Some(start) = tui.streaming.stream_start_time.take() {
        tui.streaming.last_response_duration = Some(start.elapsed());
    }
    tui.services.complete_query();
    tui.rate_limit.retry_count = 0;
    tui.rate_limit.auto_retry_cancelled = false;
    tui.active_tools.clear();

    if tui.doom_loop.is_doom_loop() {
        if let Some(reason) = tui.doom_loop.doom_loop_reason() {
            tui.add_system_message(format!("Warning: {}", reason));
        }
    }
    tui.doom_loop.reset();

    if let Some(snap) = tui.turn_snapshot.take() {
        let cwd = std::env::current_dir().unwrap_or_default();
        let diff = snap.diff(&cwd);
        if !diff.is_empty() {
            tui.toast_manager.info(diff.summary());
        }
    }

    if !had_stream_content && !was_cancelled {
        handle_empty_stream_response(tui);
    }
    tui.dirty = true;
    if !tui.view.user_scrolled {
        tui.auto_scroll();
    }

    tui.compaction.context_monitor.update(&tui.messages);
    tui.maybe_auto_compact();

    // Mark session dirty so the 30-second auto-save persists this turn
    if let Some(ref mut recovery) = tui.session_recovery {
        recovery.mark_dirty();
    }

    tui.auto_continue.auto_continue_pending = false;
    if !was_cancelled && tui.auto_continue.auto_continue_enabled {
        check_and_trigger_auto_continue(tui);
    }

    if !was_cancelled
        && !tui.auto_continue.auto_continue_enabled
        && tui.plan_mode.current_phase() == "planning"
        && !tui.is_awaiting_approval()
    {
        let convoy_id = tui
            .plan_mode
            .current_plan()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "Manual".to_string());
        tui.show_approval_banner(&convoy_id, "Planning complete.");
    }

    tracing::debug!("Stream completed");

    ring_completion_bell(tui, was_cancelled);
    send_queued_message(tui, was_cancelled);
}

/// Flush render buffer and transfer accumulated stream content into the
/// assistant message. Returns whether any stream content was transferred.
fn flush_and_transfer_stream_content(tui: &mut TUI) -> bool {
    let remaining = tui.streaming.streaming_render_buffer.flush();
    if !remaining.is_empty() {
        tui.streaming.current_stream_content.reserve(remaining.len());
        tui.streaming.current_stream_content.push_str(&remaining);
    }

    if !tui.streaming.current_stream_content.is_empty() {
        let needs_message = tui
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
            .is_none();
        if needs_message {
            tui.messages.push(Message::assistant(String::new()));
        }
    }

    let final_content = std::mem::take(&mut tui.streaming.current_stream_content);
    let had_stream_content = !final_content.is_empty();
    if had_stream_content {
        if let Some(msg) = tui
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
        {
            msg.content = final_content;
        }
    }
    had_stream_content
}

/// When the stream produced no content, clean up the empty assistant message
/// or log a diagnostic if tool executions are present.
pub(super) fn handle_empty_stream_response(tui: &mut TUI) {
    let assistant_info = tui
        .messages
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::Assistant)
        .map(|m| {
            (
                m.id.clone(),
                m.content.is_empty() && m.thinking.is_none(),
                m.tool_executions.as_ref().is_none_or(|t| t.is_empty()),
            )
        });

    if let Some((msg_id, is_empty, no_tools)) = assistant_info {
        if is_empty {
            if no_tools {
                if let Some(pos) = tui.messages.iter().position(|m| m.id == msg_id) {
                    tui.messages.remove(pos);
                    if pos < tui.view.selected_message {
                        tui.view.selected_message = tui.view.selected_message.saturating_sub(1);
                    } else if pos == tui.view.selected_message && !tui.messages.is_empty() {
                        tui.view.selected_message = tui.view.selected_message.min(tui.messages.len() - 1);
                    }
                }
                tracing::warn!(
                    chunks_received = tui.streaming.chunks_received,
                    thinking_chunks = tui.streaming.thinking_chunks_received,
                    "Empty response from model — no text, no thinking, no tool executions"
                );
                tui.add_system_message(
                    "Received empty response from model. This may indicate:\n\
                     • The model couldn't generate a response (try rephrasing)\n\
                     • The API returned null content (check model/provider compatibility)\n\
                     • The response was filtered (check debug log for details)"
                        .to_string(),
                );
            } else if let Some(last_msg) = tui.messages.iter_mut().rev().find(|m| m.id == msg_id) {
                tracing::info!(
                    tool_count = last_msg
                        .tool_executions
                        .as_ref()
                        .map(|t| t.len())
                        .unwrap_or(0),
                    "Assistant turn has tool executions but no text content"
                );
                let _ = last_msg;
            }
        }
    }
}

/// Ring terminal bell and show toast if the response took at least 3 seconds.
fn ring_completion_bell(tui: &mut TUI, was_cancelled: bool) {
    if was_cancelled {
        return;
    }
    let should_bell = tui.streaming.last_response_duration.is_some_and(|d| d.as_secs() >= 3);
    if !should_bell {
        return;
    }
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::cursor::SetCursorStyle::DefaultUserShape
    );
    let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x07");
    let duration_str = tui
        .streaming
        .last_response_duration
        .map(|d| {
            let s = d.as_secs();
            if s < 60 {
                format!("{}s", s)
            } else {
                format!("{}m{}s", s / 60, s % 60)
            }
        })
        .unwrap_or_default();
    tui.toast_manager
        .success(format!("Response complete ({})", duration_str));
}

/// Auto-send a queued message if one was waiting, or preserve it on cancellation.
fn send_queued_message(tui: &mut TUI, was_cancelled: bool) {
    if was_cancelled {
        if tui.streaming.queued_message.is_some() {
            tui.add_system_message(
                "Queued message preserved — it will be sent when ready".to_string(),
            );
        }
        return;
    }
    let Some(queued) = tui.streaming.queued_message.take() else {
        return;
    };
    let auto_send_start = std::time::Instant::now();
    let message_to_send = tui.prepare_message_for_send(&queued);

    let user_msg = Message::user(queued.clone());
    tui.messages.push(user_msg);
    tui.view.selected_message = tui.messages.len() - 1;
    tui.view.scroll_offset_line = 0;
    tui.view.user_scrolled = false;

    let prepare_elapsed = auto_send_start.elapsed();
    let history_start = std::time::Instant::now();
    let history = tui.build_conversation_history();
    let history_elapsed = history_start.elapsed();
    if crate::logging::is_debug_enabled() {
        tracing::debug!(
            "Queued auto-send history built: elapsed_ms={} history_len={} messages={} user_turns={}",
            history_elapsed.as_millis(),
            history.len(),
            tui.messages.len(),
            tui.messages
                .iter()
            .filter(|m| matches!(m.role, MessageRole::User))
            .count()
        );
    }
    tui.rate_limit.last_message = Some(queued);
    tui.streaming.begin_streaming();
    let send_start = std::time::Instant::now();
    if let Err(e) = tui
        .services
        .send_message_with_history(message_to_send, Some(history), None)
    {
        tracing::error!("Failed to send queued message: {}", e);
        tui.reset_streaming_state();
        tui.active_tools.clear();
        tui.add_system_message(format!("Queued message failed: {}", e));
    } else {
        let assistant_msg = Message::assistant(String::new());
        tui.messages.push(assistant_msg);
    }
    if crate::logging::is_debug_enabled() {
        tracing::debug!(
            "Queued auto-send timing: prepare_us={} history_ms={} send_ms={} total_ms={} messages={}",
            prepare_elapsed.as_micros(),
            history_elapsed.as_millis(),
            send_start.elapsed().as_millis(),
            auto_send_start.elapsed().as_millis(),
            tui.messages.len()
        );
    }
    tui.dirty = true;
    tui.auto_scroll();
}
