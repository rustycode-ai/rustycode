//! Stream stopped handler for non-normal stop reasons.

use crate::app::TUI;
use tracing;

use super::helpers::{complete_stream_cleanup, mark_dirty_and_scroll, reset_streaming_buffer};

pub(super) fn handle_stopped_chunk(tui: &mut TUI, stop_reason: String) {
    let was_cancelled = tui.session.streaming.stream_cancelled;
    complete_stream_cleanup(tui);
    // Flush buffered content from the render buffer BEFORE replacing it
    let remaining = tui.session.streaming.streaming_render_buffer.flush();
    if !remaining.is_empty() {
        tui.session.streaming
            .current_stream_content
            .reserve(remaining.len());
        tui.session.streaming.current_stream_content.push_str(&remaining);
    }

    reset_streaming_buffer(tui);

    if !was_cancelled {
        let user_message = match stop_reason.as_str() {
            "content_filter" | "safety" | "SAFETY" | "RECITATION" => {
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

    tui.update_context_and_compact();
    tui.mark_session_dirty();
    mark_dirty_and_scroll(tui);
}
