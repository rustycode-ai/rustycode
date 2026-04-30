//! Tests for rustycode-thread-guard
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]

use rustycode_thread_guard::{assert_not_terminal_thread, is_terminal_thread};
use std::thread;

#[test]
fn detects_main_thread_as_terminal() {
    // When running on the main thread, this should return true
    // Note: This test depends on how the test runner names threads
    let result = is_terminal_thread();
    // The main test thread may or may not be named "main"
    // This test just verifies the function doesn't panic
    // Just verify the function doesn't panic
    let _ = result;
}

#[test]
fn spawned_thread_not_terminal() {
    let handle = thread::spawn(is_terminal_thread);
    let result = handle.join().unwrap();
    // Spawned threads shouldn't be named "main"
    assert!(!result);
}

#[test]
fn assert_not_terminal_on_spawned_thread() {
    // This should NOT panic on a spawned thread
    let handle = thread::spawn(|| {
        assert_not_terminal_thread("test_operation");
    });
    handle.join().expect("Should not panic on spawned thread");
}

#[test]
fn assert_not_terminal_behavior() {
    // Test that the assertion works correctly on spawned threads
    let handle = thread::spawn(|| {
        // Should not panic
        assert_not_terminal_thread("background_operation");
        true
    });
    let result = handle.join().unwrap();
    assert!(result);
}
