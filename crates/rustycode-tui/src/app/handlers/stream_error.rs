//! Stream error handler with exponential backoff and retry logic.

use crate::app::async_::StreamError;
use crate::app::TUI;
use crate::ui::message::{Message, MessageRole};
use anyhow;
use std::time::{Duration, SystemTime};
use tracing;

pub(super) fn handle_error_chunk(tui: &mut TUI, err: StreamError) {
    // Streaming encountered an error — release query guard
    tui.streaming.is_streaming = false;
    tui.streaming.stream_cancelled = false; // Reset for next stream

    // Update terminal title back to "ready"
    tui.update_terminal_title();

    tui.services.complete_query();
    // Clear stale active tools on error
    tui.active_tools.clear();
    // Reset streaming buffer state so next stream starts clean
    tui.streaming.streaming_render_buffer = crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
    tui.streaming.chunks_received = 0;
    tui.streaming.thinking_chunks_received = 0;

    // Preserve partial response content so the user doesn't lose
    // what the AI already wrote before the error. If there's partial
    // content that hasn't been committed as a message, commit it now.
    // Use iter().rev().find() (not .last()) because system messages
    // (auto-approve notifications, doom loop warnings) may have been
    // pushed during streaming, making .last() point to the wrong message.
    if !tui.streaming.current_stream_content.is_empty() {
        let content = std::mem::take(&mut tui.streaming.current_stream_content);
        let preserved_len = content.len();
        let assistant_msg = tui
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.role == MessageRole::Assistant);
        if let Some(msg) = assistant_msg {
            if msg.content.is_empty() {
                msg.content = content;
            }
        } else {
            tui.messages.push(Message::assistant(content));
        }
        tui.add_system_message(format!(
            "Partial response preserved ({} chars)",
            preserved_len
        ));
    }

    tui.context_monitor.update(&tui.messages);
    tui.maybe_auto_compact();

    // On cancellation, keep the queued message so the user can retry.
    // On retryable errors (rate limit, network), preserve it for auto-retry.
    // On non-retryable errors (auth, context), clear it — retrying won't help.
    if !err.should_preserve_queued_message() {
        tui.streaming.queued_message = None;
    }

    tui.auto_continue.auto_continue_pending = false;
    let is_retryable = err.is_retryable();
    if !is_retryable {
        tui.auto_continue.auto_continue_enabled = false;
        tui.show_error(anyhow::anyhow!("{}", err));
        tui.dirty = true;
        tui.auto_scroll();
        return;
    }

    // Calculate exponential backoff with jitter using RateLimitHandler constants
    let base_delay_secs = tui.rate_limit.backoff_delay_secs();
    let jitter = (base_delay_secs as f64 * 0.25) as isize;
    let random_jitter = if jitter > 0 {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as isize;
        (nanos % (2 * jitter)) - jitter
    } else {
        0
    };
    let delay_secs = (base_delay_secs as isize + random_jitter).max(1) as u64;

    // Set rate limit countdown with exponential backoff
    tui.rate_limit.until = Some(std::time::Instant::now() + Duration::from_secs(delay_secs));

    // Remove previous retry message if exists
    if let Some(prev_idx) = tui.rate_limit.message_index.take() {
        if prev_idx < tui.messages.len() {
            if let Some(msg) = tui.messages.get(prev_idx) {
                if msg.content.contains("Retrying now") || msg.content.contains("Auto-retrying") {
                    tui.messages.remove(prev_idx);
                    if prev_idx < tui.selected_message {
                        tui.selected_message = tui.selected_message.saturating_sub(1);
                    }
                }
            }
        }
    }

    let error_type = err.display_category();

    // Show retry attempt number (if > 1)
    let retry_info = if tui.rate_limit.retry_count > 0 {
        format!(" (retry {})", tui.rate_limit.retry_count + 1)
    } else {
        String::new()
    };

    tui.add_system_message(format!(
        "◯ {} - Auto-retrying in {}s...{} (Esc or Ctrl+C to cancel)",
        error_type, delay_secs, retry_info
    ));

    // Store the message index for updating countdown
    tui.rate_limit.message_index = Some(tui.messages.len() - 1);

    // Reset auto-retry cancellation flag for new error
    tui.rate_limit.auto_retry_cancelled = false;

    // Increment retry count for next exponential backoff
    tui.rate_limit.retry_count = tui.rate_limit.retry_count.saturating_add(1);

    tui.auto_scroll();

    tracing::debug!(
        "Stream error: {} (retry {} in {}s)",
        err,
        tui.rate_limit.retry_count,
        delay_secs
    );
}
