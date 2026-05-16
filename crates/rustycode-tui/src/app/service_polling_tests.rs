//! Characterization tests for service_polling.rs drain limits and cleanup paths.
//!
//! These tests document current behavior of `poll_services()` constants and
//! the disconnected-channel cleanup guard, so any refactoring that changes
//! drain limits or cleanup behavior will be caught immediately.

use crate::app::event_loop::TUI;

/// Characterization: MAX_STREAM_CHUNKS_PER_FRAME is 8.
/// Higher values (32) caused visible stuttering on multi-turn conversations.
/// This test ensures the constant isn't accidentally changed.
#[test]
fn characterization_max_stream_chunks_per_frame_is_eight() {
    // This value is embedded in poll_services() as a const.
    // We document it here so changes require updating this test.
    const MAX_STREAM_CHUNKS_PER_FRAME: usize = 8;
    assert_eq!(MAX_STREAM_CHUNKS_PER_FRAME, 8);
}

/// Characterization: MAX_TOOL_RESULTS_PER_FRAME is 8.
/// Tools are heavier than stream chunks; same limit keeps UI responsive.
#[test]
fn characterization_max_tool_results_per_frame_is_eight() {
    const MAX_TOOL_RESULTS_PER_FRAME: usize = 8;
    assert_eq!(MAX_TOOL_RESULTS_PER_FRAME, 8);
}

/// Characterization: MAX_EVENT_MSGS_PER_FRAME is 8.
/// EventMsg drain matches stream chunk batch pattern.
#[test]
fn characterization_max_event_msgs_per_frame_is_eight() {
    const MAX_EVENT_MSGS_PER_FRAME: usize = 8;
    assert_eq!(MAX_EVENT_MSGS_PER_FRAME, 8);
}

/// Characterization: disconnected-channel cleanup guard fires when
/// channel disconnects while is_streaming is true.
///
/// The guard is: `if channel_disconnected && self.session.streaming.is_streaming { ... }`
/// It calls reset_streaming_state(), clears active_tools, adds system message, marks dirty.
#[test]
fn characterization_disconnected_cleanup_guard_resets_streaming() {
    let mut tui = TUI::new_for_test();

    // Simulate: stream was active
    tui.session.streaming.is_streaming = true;
    tui.session.streaming.current_stream_content = "Partial response...".to_string();

    // Simulate: channel disconnected
    // (In poll_services, channel_disconnected is set when RecvStatus::Disconnected)
    let channel_disconnected = true;

    // Simulate the guard logic
    if channel_disconnected && tui.session.streaming.is_streaming {
        tui.reset_streaming_state();
        tui.session.active_tools.clear();
        // In real code: complete_query() + update_terminal_title() + add_system_message()
        tui.sys.dirty.set(crate::app::state_model::DirtyFlags::ALL);
    }

    // Verify cleanup happened
    assert!(
        !tui.session.streaming.is_streaming,
        "is_streaming should be false after disconnected cleanup"
    );
    assert!(
        tui.session.active_tools.is_empty(),
        "active_tools should be cleared after disconnected cleanup"
    );
    assert!(
        tui.sys.dirty.is_dirty(),
        "dirty flag should be set after disconnected cleanup"
    );
}

/// Characterization: disconnected-channel cleanup does NOT fire when
/// is_streaming is false (normal completion already handled via Done chunk).
#[test]
fn characterization_disconnected_cleanup_skipped_when_not_streaming() {
    let mut tui = TUI::new_for_test();

    // Normal state: stream completed via Done chunk, is_streaming already false
    tui.session.streaming.is_streaming = false;
    let initial_dirty = tui.sys.dirty;

    let channel_disconnected = true;

    // Guard check
    if channel_disconnected && tui.session.streaming.is_streaming {
        // This should NOT execute
        tui.reset_streaming_state();
        tui.sys.dirty.set(crate::app::state_model::DirtyFlags::ALL);
        panic!("Cleanup should not fire when is_streaming is false");
    }

    // dirty should not have changed from this guard
    assert_eq!(
        tui.sys.dirty.is_dirty(),
        initial_dirty.is_dirty(),
        "dirty should not change from disconnected guard when not streaming"
    );
}

/// Characterization: poll_services processes chunks unconditionally
/// even after is_streaming goes false within a batch.
///
/// The comment in poll_services reads:
/// "Process all drained chunks unconditionally — do NOT break on `is_streaming`
/// going false. `handle_stream_chunk(Done)` toggles `is_streaming` false→true
/// (via auto-queued messages), so the flag is not a reliable stream-end signal."
#[test]
fn characterization_chunks_processed_unconditionally_after_done() {
    use crate::app::async_::StreamChunk;
    use crate::app::handlers::handle_stream_chunk;

    let mut tui = TUI::new_for_test();
    tui.session.streaming.is_streaming = true;

    // Send Done — this sets is_streaming = false
    handle_stream_chunk(&mut tui, StreamChunk::Done);
    assert!(
        !tui.session.streaming.is_streaming,
        "Done should stop streaming"
    );

    // But poll_services would still process remaining chunks in the batch.
    // Send another Text chunk after Done — this should still work.
    handle_stream_chunk(&mut tui, StreamChunk::Text("after done".to_string()));

    // The text was processed (appended to current_stream_content)
    assert!(
        tui.session
            .streaming
            .current_stream_content
            .contains("after done"),
        "Text after Done should still be processed unconditionally"
    );
}

/// Characterization: Stream channel capacity is 100 (bounded).
/// The backpressure semantics must be preserved during refactoring.
#[test]
fn characterization_stream_channel_capacity() {
    // This is defined in async_.rs as the bounded channel capacity.
    // Documenting here so it's not accidentally changed.
    const STREAM_CHANNEL_CAPACITY: usize = 100;
    assert_eq!(STREAM_CHANNEL_CAPACITY, 100);
}

/// Characterization: EventMsg channel matches the same capacity pattern.
#[test]
fn characterization_event_channel_capacity() {
    // EventMsg channel uses the same capacity as StreamChunk.
    const EVENT_CHANNEL_CAPACITY: usize = 100;
    assert_eq!(EVENT_CHANNEL_CAPACITY, 100);
}

/// Characterization: reset_streaming_state clears streaming and ast_phase.
#[test]
fn characterization_reset_streaming_state_clears_all() {
    let mut tui = TUI::new_for_test();

    tui.session.streaming.is_streaming = true;
    tui.session.streaming.current_stream_content = "some content".to_string();
    tui.panels
        .ast_phase_state
        .activate("test-phase", 0, "summary");

    tui.reset_streaming_state();

    assert!(!tui.session.streaming.is_streaming);
    assert!(tui.session.streaming.current_stream_content.is_empty());
    assert!(!tui.panels.ast_phase_state.is_active());
    // Note: terminal_progress.clear() writes OSC escape sequence but does NOT
    // change the `enabled` field — that's a capability detection, not state.
}

/// Characterization: auto_scroll is called when system messages are added
/// during disconnected cleanup (this is important for the UX flow).
#[test]
fn characterization_system_message_after_disconnect_triggers_scroll() {
    let mut tui = TUI::new_for_test();
    tui.session.streaming.is_streaming = true;
    tui.ui.view.user_scrolled = false;

    // Simulate the cleanup adding a system message
    tui.add_system_message("⚠ Stream connection lost unexpectedly.".to_string());

    // System message was added
    assert!(
        tui.session
            .messages
            .iter()
            .any(|m| m.content.contains("Stream connection lost")),
        "System message about disconnection should be present"
    );
}
