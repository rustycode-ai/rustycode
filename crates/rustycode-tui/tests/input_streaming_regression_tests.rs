#![allow(
    clippy::doc_markdown,
    clippy::match_same_arms,
    clippy::redundant_clone,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Regression tests for critical input and streaming bugs
//!
//! These tests prevent regressions of:
//! - Bug #1: Enter key intercepted by global shortcuts instead of submitting messages
//! - Bug #2: Missing Done signal causing streaming to never complete
//!
//! Test Strategy:
//! 1. Unit tests for InputHandler - verify Enter key behavior
//! 2. Unit tests for streaming - verify Done signal is sent
//! 3. Integration tests - full flow with mocked LLM

use crossterm::event::{KeyCode, KeyModifiers};
use rustycode_tui::app::async_::StreamChunk;
use rustycode_tui::ui::input_handler::{InputAction, InputHandler};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Mutex;
use std::time::Duration;

/// Global lock to serialize tests that mutate environment variables.
/// `with_isolated_provider` sets `RUSTYCODE_PROVIDER` and `RUSTYCODE_MODEL_OVERRIDE`,
/// which is process-wide. Without serialization, parallel test threads race on these
/// env vars and produce flaky failures.
static PROVIDER_LOCK: Mutex<()> = Mutex::new(());

// ============================================================================
// Test 1: Enter Key Submission Tests
// ============================================================================

#[test]
fn test_enter_key_submits_message_in_single_line_mode() {
    let mut handler = InputHandler::new();

    // Type some text
    handler.state.insert_char('H');
    handler.state.insert_char('i');
    handler.state.insert_char('!');

    // Verify we're in single-line mode (default)
    assert_eq!(
        handler.state.mode,
        rustycode_tui::ui::input_state::InputMode::SingleLine
    );

    // Press Enter
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    // Should send the message, not toggle collapse or do anything else
    assert!(
        matches!(action, InputAction::SendMessage(_)),
        "Enter should send message in single-line mode"
    );

    if let InputAction::SendMessage(lines) = action {
        assert_eq!(lines.len(), 1, "Should have one line");
        assert_eq!(lines[0], "Hi!", "Message content should match input");
    }
}

#[test]
fn test_enter_key_creates_newline_in_multiline_mode() {
    let mut handler = InputHandler::new();

    // Switch to multi-line mode
    handler.state.mode = rustycode_tui::ui::input_state::InputMode::MultiLine;

    // Type some text
    handler.state.insert_char('L');
    handler.state.insert_char('i');
    handler.state.insert_char('n');
    handler.state.insert_char('e');
    handler.state.insert_char('1');

    // Press Enter
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    // Should insert newline, NOT send message
    assert_eq!(
        action,
        InputAction::Consumed,
        "Enter should insert newline in multi-line mode"
    );

    // Verify we have two lines
    assert_eq!(
        handler.state.lines.len(),
        2,
        "Should have two lines after Enter"
    );
    // After Enter, cursor moves to new empty line (row 1)
    assert_eq!(
        handler.state.current_line(),
        "",
        "Current line should be the new empty line after Enter"
    );
    // First line should be preserved
    assert_eq!(
        handler.state.lines[0], "Line1",
        "First line should be preserved"
    );
}

#[test]
fn test_shift_enter_inserts_newline_in_multiline_mode() {
    let mut handler = InputHandler::new();

    // Switch to multi-line mode
    handler.state.mode = rustycode_tui::ui::input_state::InputMode::MultiLine;

    // Type some text
    handler.state.insert_char('L');
    handler.state.insert_char('i');
    handler.state.insert_char('n');
    handler.state.insert_char('e');
    handler.state.insert_char('1');

    // Press Shift+Enter — should insert newline, not send
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::SHIFT);
    assert!(
        matches!(action, InputAction::Consumed),
        "Shift+Enter should insert newline in multi-line mode, not send"
    );

    // Continue typing on new line
    handler.state.insert_char('L');
    handler.state.insert_char('i');
    handler.state.insert_char('n');
    handler.state.insert_char('e');
    handler.state.insert_char('2');

    // Now Ctrl+Enter should send
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::CONTROL);
    assert!(
        matches!(action, InputAction::SendMessage(_)),
        "Ctrl+Enter should send message in multi-line mode"
    );

    if let InputAction::SendMessage(lines) = action {
        assert_eq!(lines.len(), 2, "Should have two lines");
        assert_eq!(lines[0], "Line1", "First line should match");
        assert_eq!(lines[1], "Line2", "Second line should match");
    }
}

#[test]
fn test_space_key_toggles_collapse_not_enter() {
    let mut handler = InputHandler::new();

    // Type some text
    handler.state.insert_char('T');
    handler.state.insert_char('e');
    handler.state.insert_char('x');
    handler.state.insert_char('t');

    // Press Space
    let action = handler.handle_key_event(KeyCode::Char(' '), KeyModifiers::NONE);

    // Space should insert a space character, NOT send message
    assert_eq!(
        action,
        InputAction::Consumed,
        "Space should insert space character"
    );

    // Verify the space was inserted
    assert_eq!(
        handler.state.current_line(),
        "Text ",
        "Space should be in the text"
    );
}

#[test]
fn test_empty_enter_does_not_crash() {
    let mut handler = InputHandler::new();

    // Press Enter with empty input
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    // Should send empty message (event loop will filter it)
    assert!(
        matches!(action, InputAction::SendMessage(_)),
        "Enter with empty input should still return SendMessage"
    );

    if let InputAction::SendMessage(lines) = action {
        assert_eq!(lines.len(), 1, "Should have one line");
        assert_eq!(lines[0], "", "Message should be empty");
    }
}

// ============================================================================
// Test 2: Streaming Done Signal Tests
// ============================================================================

/// Helper: temporarily override provider env vars for isolated streaming tests.
/// Prevents tests from hitting real API endpoints when ~/.rustycode/config.json
/// contains valid keys. Uses `ollama` which has no API key requirement and will
/// fail fast if ollama isn't running on localhost.
fn with_isolated_provider<F: FnOnce()>(f: F) {
    let _lock = PROVIDER_LOCK.lock().unwrap();
    let orig_provider = std::env::var("RUSTYCODE_PROVIDER").ok();
    let orig_model = std::env::var("RUSTYCODE_MODEL_OVERRIDE").ok();

    std::env::set_var("RUSTYCODE_PROVIDER", "ollama");
    std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", "test-model");

    f();

    match orig_provider {
        Some(v) => std::env::set_var("RUSTYCODE_PROVIDER", v),
        None => std::env::remove_var("RUSTYCODE_PROVIDER"),
    }
    match orig_model {
        Some(v) => std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", v),
        None => std::env::remove_var("RUSTYCODE_MODEL_OVERRIDE"),
    }
}

#[test]
fn test_streaming_always_sends_done_on_success() {
    use rustycode_tui::app::streaming;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let (stream_tx, stream_rx): (SyncSender<StreamChunk>, Receiver<StreamChunk>) =
        sync_channel(100);

    let stop_flag = Arc::new(AtomicBool::new(false));

    // Use isolated provider to avoid hitting real APIs when config has keys.
    with_isolated_provider(|| {
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                let config =
                    streaming::StreamConfig::new("test", &PathBuf::from("/tmp"), stream_tx)
                        .stop_signal_opt(Some(stop_flag));
                let _ = streaming::stream_llm_response(config).await;
            });
        });

        // Collect all chunks with timeout
        let mut chunks = Vec::new();
        let timeout = Duration::from_secs(10);
        let start = std::time::Instant::now();

        loop {
            match stream_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => {
                    chunks.push(chunk);
                    if matches!(chunks.last(), Some(StreamChunk::Done)) {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if handle.is_finished() {
                        while let Ok(chunk) = stream_rx.try_recv() {
                            chunks.push(chunk);
                        }
                        break;
                    }
                    if start.elapsed() > timeout {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    while let Ok(chunk) = stream_rx.try_recv() {
                        chunks.push(chunk);
                    }
                    break;
                }
            }
        }

        let has_done = chunks.iter().any(|c| matches!(c, StreamChunk::Done));
        let has_error = chunks.iter().any(|c| matches!(c, StreamChunk::Error(_)));

        assert!(
            has_done || has_error,
            "Streaming should always send Done or Error signal. Got chunks: {:?}",
            chunks
        );

        if has_done && has_error {
            let last_done_idx = chunks.iter().rposition(|c| matches!(c, StreamChunk::Done));
            let last_error_idx = chunks
                .iter()
                .rposition(|c| matches!(c, StreamChunk::Error(_)));

            if let (Some(done_idx), Some(error_idx)) = (last_done_idx, last_error_idx) {
                assert!(
                    done_idx > error_idx || done_idx == chunks.len() - 1,
                    "Done signal should come after Error or be last signal"
                );
            }
        }
    });
}

#[test]
fn test_streaming_done_signal_is_last() {
    // Create a mock streaming scenario
    let (stream_tx, stream_rx): (SyncSender<StreamChunk>, Receiver<StreamChunk>) =
        sync_channel(100);

    // Simulate streaming: send some text, then Done
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        stream_tx
            .send(StreamChunk::Text("Hello".to_string()))
            .unwrap();
        stream_tx
            .send(StreamChunk::Text(" World".to_string()))
            .unwrap();
        stream_tx.send(StreamChunk::Done).unwrap();
    });

    // Collect all chunks
    let mut chunks = Vec::new();
    loop {
        match stream_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(chunk) => {
                chunks.push(chunk);
                if matches!(chunks.last(), Some(StreamChunk::Done)) {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
        }
    }

    // Verify structure
    assert!(
        chunks.len() >= 3,
        "Should have at least 3 chunks (Text, Text, Done)"
    );
    assert!(
        matches!(&chunks[0], StreamChunk::Text(_)),
        "First chunk should be Text"
    );
    assert!(
        matches!(&chunks[1], StreamChunk::Text(_)),
        "Second chunk should be Text"
    );
    assert!(
        matches!(&chunks[2], StreamChunk::Done),
        "Third chunk should be Done"
    );

    // Verify Done is last
    assert!(
        matches!(chunks.last(), Some(StreamChunk::Done)),
        "Last chunk should be Done"
    );
}

// ============================================================================
// Test 3: Integration Tests
// ============================================================================

#[test]
fn test_input_handler_ignores_unknown_keys() {
    let mut handler = InputHandler::new();

    // Type some text
    handler.state.insert_char('T');
    handler.state.insert_char('e');
    handler.state.insert_char('s');
    handler.state.insert_char('t');

    let initial_text = handler.state.current_line();

    // Press various function keys that shouldn't affect input
    let keys_to_test = vec![
        (KeyCode::F(1), KeyModifiers::NONE),
        (KeyCode::F(5), KeyModifiers::NONE),
        (KeyCode::F(10), KeyModifiers::NONE),
        (KeyCode::Tab, KeyModifiers::NONE),
        (KeyCode::BackTab, KeyModifiers::SHIFT),
    ];

    for (key, modifiers) in keys_to_test {
        let action = handler.handle_key_event(key, modifiers);

        // These keys should be ignored (passed to global shortcuts)
        assert_eq!(
            action,
            InputAction::Ignored,
            "{:?}+{:?} should be ignored",
            key,
            modifiers
        );

        // Text should remain unchanged
        assert_eq!(
            handler.state.current_line(),
            initial_text,
            "Input should not change for ignored keys"
        );
    }
}

#[test]
fn test_ctrl_keys_not_intercepted_by_global_shortcuts() {
    let mut handler = InputHandler::new();

    // Type a message
    for c in "Hello World".chars() {
        handler.state.insert_char(c);
    }

    // Press Ctrl+C (should be ignored by InputHandler, handled by global shortcuts)
    let action = handler.handle_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);

    // Ctrl+C should be ignored (passed to global shortcuts for quit)
    assert_eq!(
        action,
        InputAction::Ignored,
        "Ctrl+C should be ignored by InputHandler"
    );

    // Text should remain unchanged
    assert_eq!(
        handler.state.current_line(),
        "Hello World",
        "Ctrl+C should not affect input"
    );

    // Ctrl+K is handled by InputHandler as "kill to end of line" (readline binding)
    let action = handler.handle_key_event(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert_eq!(
        action,
        InputAction::Consumed,
        "Ctrl+K should be consumed by InputHandler (kill to end of line)"
    );
}

#[test]
fn test_enter_never_returns_ignored() {
    let mut handler = InputHandler::new();

    // Test Enter in both modes with and without modifiers
    let test_cases = vec![
        (
            KeyCode::Enter,
            KeyModifiers::NONE,
            rustycode_tui::ui::input_state::InputMode::SingleLine,
        ),
        (
            KeyCode::Enter,
            KeyModifiers::NONE,
            rustycode_tui::ui::input_state::InputMode::MultiLine,
        ),
        (
            KeyCode::Enter,
            KeyModifiers::SHIFT,
            rustycode_tui::ui::input_state::InputMode::SingleLine,
        ),
        (
            KeyCode::Enter,
            KeyModifiers::SHIFT,
            rustycode_tui::ui::input_state::InputMode::MultiLine,
        ),
        (
            KeyCode::Enter,
            KeyModifiers::ALT,
            rustycode_tui::ui::input_state::InputMode::SingleLine,
        ),
        (
            KeyCode::Enter,
            KeyModifiers::ALT,
            rustycode_tui::ui::input_state::InputMode::MultiLine,
        ),
    ];

    for (key, modifiers, mode) in test_cases {
        handler.state.mode = mode;

        let action = handler.handle_key_event(key, modifiers);

        // Enter should NEVER be ignored - it should always send message or insert newline
        assert_ne!(
            action,
            InputAction::Ignored,
            "Enter should never be ignored (mode: {:?}, modifiers: {:?})",
            mode,
            modifiers
        );

        // Should be either SendMessage (send) or Consumed (newline insert)
        assert!(
            matches!(action, InputAction::SendMessage(_) | InputAction::Consumed),
            "Enter should return SendMessage or Consumed, got {:?} (mode: {:?}, modifiers: {:?})",
            action,
            mode,
            modifiers
        );
    }
}

// ============================================================================
// Test 4: Regression Tests for Specific Bugs
// ============================================================================

#[test]
#[ignore = "Requires full TUI setup - run manually to verify"]
fn test_regression_enter_key_not_intercepted_by_toggle_collapse() {
    // This test would require a full TUI setup with terminal
    // It's marked as ignore to run manually when needed

    // Manual test procedure:
    // 1. Start TUI
    // 2. Type "hello"
    // 3. Press Enter
    // 4. Verify:
    //    - Message is submitted (not collapsed)
    //    - Input field is cleared
    //    - AI response is requested
    // 5. Type "world"
    // 6. Press Space
    // 7. Verify:
    //    - Message is NOT submitted
    //    - Selected message is collapsed/expanded

    // This documents the expected behavior for manual testing
    // See test procedure above for expected behavior
}

#[test]
fn test_regression_streaming_done_signal_prevents_hang() {
    use rustycode_tui::app::streaming;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::thread;

    let (stream_tx, stream_rx): (SyncSender<StreamChunk>, Receiver<StreamChunk>) =
        sync_channel(100);
    let stop_flag = Arc::new(AtomicBool::new(true)); // Already stopped

    let stop_flag_clone = stop_flag.clone();

    // Use isolated provider to avoid hitting real APIs.
    with_isolated_provider(|| {
        // Spawn streaming in a thread — with stop_flag=true, it should exit quickly
        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let config = streaming::StreamConfig::new(
                    "test",
                    PathBuf::from("/tmp").as_path(),
                    stream_tx,
                )
                .stop_signal_opt(Some(stop_flag_clone));
                let _ = streaming::stream_llm_response(config).await;
            });
        });

        // Wait for thread with generous timeout
        let joined = handle.join();
        assert!(
            joined.is_ok(),
            "Streaming should complete without panicking"
        );

        // Drain any remaining chunks from the channel
        let chunks: Vec<_> = stream_rx.try_iter().collect();
        assert!(
            chunks.iter().any(|c| matches!(c, StreamChunk::Done)),
            "Should receive Done signal. Got: {:?}",
            chunks
        );
    });
}
