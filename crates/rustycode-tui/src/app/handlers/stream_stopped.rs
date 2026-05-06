//! Stream stopped handler for non-normal stop reasons.

use crate::app::TUI;
use tracing;

pub(super) fn handle_stopped_chunk(tui: &mut TUI, stop_reason: String) {
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
    // Flush buffered content from the render buffer BEFORE replacing it
    let remaining = tui.streaming.streaming_render_buffer.flush();
    if !remaining.is_empty() {
        tui.streaming.current_stream_content.reserve(remaining.len());
        tui.streaming.current_stream_content.push_str(&remaining);
    }

    tui.streaming.streaming_render_buffer = crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
    tui.streaming.chunks_received = 0;
    tui.streaming.thinking_chunks_received = 0;

    if !was_cancelled {
        let user_message = match stop_reason.as_str() {
            "content_filter" | "SAFETY" | "RECITATION" => {
                "Response filtered by provider's safety policy. \
                 Try rephrasing your request or using a different model."
                    .to_string()
            }
            "refusal" => "Model declined to respond. \
                 Try rephrasing or simplifying your request."
                .to_string(),
            _ => {
                format!(
                    "Stream stopped (reason: {}). Try rephrasing or retrying.",
                    stop_reason
                )
            }
        };
        tracing::warn!(
            stop_reason = %stop_reason,
            "Stream stopped with non-normal stop reason"
        );
        tui.add_system_message(user_message);
    }

    tui.compaction.context_monitor.update(&tui.messages);
    tui.maybe_auto_compact();
    if let Some(ref mut recovery) = tui.session_recovery {
        recovery.mark_dirty();
    }
    tui.dirty = true;
    if !tui.view.user_scrolled {
        tui.auto_scroll();
    }
}
