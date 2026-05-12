//! Stream error handler with exponential backoff and retry logic.

use crate::app::async_::StreamError;
use crate::app::TUI;
use crate::ui::message::Message;
use anyhow;
use std::time::{Duration, SystemTime};
use tracing;

use super::helpers::reset_streaming_buffer;

pub(super) fn handle_error_chunk(tui: &mut TUI, err: StreamError) {
    // Streaming encountered an error — release query guard
    tui.session.streaming.is_streaming = false;
    tui.session.streaming.stream_cancelled = false; // Reset for next stream

    // Update terminal title back to "ready"
    tui.update_terminal_title();

    tui.integration.services.complete_query();
    // Clear stale active tools on error
    tui.session.active_tools.clear();
    // Reset streaming buffer state so next stream starts clean
    reset_streaming_buffer(tui);

    // Preserve partial response content so the user doesn't lose
    // what the AI already wrote before the error. If there's partial
    // content that hasn't been committed as a message, commit it now.
    // Use iter().rev().find() (not .last()) because system messages
    // (auto-approve notifications, doom loop warnings) may have been
    // pushed during streaming, making .last() point to the wrong message.
    if !tui.session.streaming.current_stream_content.is_empty() {
        let content = std::mem::take(&mut tui.session.streaming.current_stream_content);
        let preserved_len = content.len();
        let assistant_msg = tui.last_assistant_message_mut();
        if let Some(msg) = assistant_msg {
            if msg.content.is_empty() {
                msg.content = content;
            }
        } else {
            tui.session.messages.push(Message::assistant(content));
        }
        tui.add_system_message(format!(
            "Partial response preserved ({} chars)",
            preserved_len
        ));
    }

    tui.update_context_and_compact();

    // On cancellation, keep the queued message so the user can retry.
    // On retryable errors (rate limit, network), preserve it for auto-retry.
    // On non-retryable errors (auth, context), clear it — retrying won't help.
    if !err.should_preserve_queued_message() {
        tui.session.streaming.queued_message = None;
    }

    tui.session.auto_continue.clear_pending();
    let is_retryable = err.is_retryable();
    if !is_retryable {
        tui.session.auto_continue.disable();
        tui.show_error(anyhow::anyhow!("{}", err));
        tui.sys.dirty = true;
        tui.auto_scroll();
        return;
    }

    // Calculate exponential backoff with jitter using RateLimitHandler constants
    let server_retry_after = if let StreamError::Provider(ref provider_err) = err {
        provider_err.retry_delay().map(|d| d.as_secs())
    } else {
        None
    };
    let base_delay_secs =
        server_retry_after.unwrap_or_else(|| tui.integration.rate_limit.backoff_delay_secs());
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
    tui.integration.rate_limit.until =
        Some(std::time::Instant::now() + Duration::from_secs(delay_secs));

    // Remove previous retry message if exists
    if let Some(prev_idx) = tui.integration.rate_limit.message_index.take() {
        if prev_idx < tui.session.messages.len() {
            if let Some(msg) = tui.session.messages.get(prev_idx) {
                if msg.content.contains("Retrying now") || msg.content.contains("Auto-retrying") {
                    tui.session.messages.remove(prev_idx);
                    if prev_idx < tui.ui.view.selected_message {
                        tui.ui.view.selected_message =
                            tui.ui.view.selected_message.saturating_sub(1);
                    }
                }
            }
        }
    }

    let error_type = err.display_category();

    // Show retry attempt number (if > 1)
    let retry_info = if tui.integration.rate_limit.retry_count > 0 {
        format!(" (retry {})", tui.integration.rate_limit.retry_count + 1)
    } else {
        String::new()
    };

    tui.add_system_message(format!(
        "◯ {} - Auto-retrying in {}s...{} (Esc or Ctrl+C to cancel)",
        error_type, delay_secs, retry_info
    ));

    // Store the message index for updating countdown
    tui.integration.rate_limit.message_index = Some(tui.session.messages.len() - 1);

    // Reset auto-retry cancellation flag for new error
    tui.integration.rate_limit.auto_retry_cancelled = false;

    // Increment retry count for next exponential backoff
    tui.integration.rate_limit.retry_count =
        tui.integration.rate_limit.retry_count.saturating_add(1);

    tui.auto_scroll();

    tracing::debug!(
        "Stream error: {} (retry {} in {}s)",
        err,
        tui.integration.rate_limit.retry_count,
        delay_secs
    );
}
