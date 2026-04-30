#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::needless_collect
    )
)]

use std::thread;

/// Returns true if the current thread appears to be the terminal (UI) thread.
///
/// Heuristic: the terminal UI runs on the main thread (where `enable_raw_mode()` and
/// `event::poll/read` are invoked). Tools and background tasks should not run on
/// that thread. This helper uses `thread::current().name()` and OS-specific
/// checks if needed. For now we treat the thread named "main" as the terminal
/// thread when running the TUI.
pub fn is_terminal_thread() -> bool {
    matches!(thread::current().name(), Some(name) if name == "main")
}

/// Assert that the current operation is not running on the terminal thread.
/// Panics with a helpful message when violated.
pub fn assert_not_terminal_thread(op: &str) {
    assert!(
        !is_terminal_thread(),
        "Operation '{op}' must not run on the terminal/UI thread"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_terminal_thread_on_named_main() {
        // In test context, thread name varies by test runner.
        // This test documents the behavior: "main" thread is terminal.
        let thread_name = std::thread::current().name().map(ToString::to_string);
        // Test threads are NOT named "main", so this should be false
        // unless running in a special context
        if thread_name.as_deref() == Some("main") {
            assert!(is_terminal_thread());
        } else {
            assert!(!is_terminal_thread());
        }
    }

    #[test]
    fn test_assert_not_terminal_thread_does_not_panic() {
        // Test threads are not named "main", so this should not panic
        assert_not_terminal_thread("test_operation");
    }

    #[test]
    fn test_is_terminal_thread_returns_bool() {
        // Just verify it returns a bool without panicking
        let _result: bool = is_terminal_thread();
    }

    // --- Named thread behavior ---

    #[test]
    fn test_named_main_thread_is_terminal() {
        let child = thread::Builder::new()
            .name("main".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(child.join().expect("thread panicked"));
    }

    #[test]
    fn test_named_worker_thread_is_not_terminal() {
        let child = thread::Builder::new()
            .name("worker".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(!child.join().expect("thread panicked"));
    }

    #[test]
    fn test_unnamed_thread_is_not_terminal() {
        let child = thread::spawn(is_terminal_thread);
        assert!(!child.join().expect("thread panicked"));
    }

    // --- assert_not_terminal_thread ---

    #[test]
    fn test_assert_not_terminal_worker_thread() {
        let child = thread::Builder::new()
            .name("tool-exec".to_string())
            .spawn(|| {
                assert_not_terminal_thread("tool_execution");
            })
            .expect("spawn failed");
        child.join().expect("thread should not panic");
    }

    #[test]
    fn test_assert_not_terminal_panics_on_main_thread() {
        let child = thread::Builder::new()
            .name("main".to_string())
            .spawn(|| {
                assert_not_terminal_thread("dangerous_op");
            })
            .expect("spawn failed");
        let result = child.join();
        // The thread named "main" should panic because assert_not_terminal_thread
        // detects it as the terminal thread
        assert!(result.is_err(), "should have panicked on main-named thread");
    }

    #[test]
    fn test_assert_not_terminal_various_ops() {
        // Test with different operation names — none should panic on worker threads
        for op in &[
            "file_read",
            "bash_exec",
            "llm_call",
            "git_operation",
            "mcp_tool",
        ] {
            let op_str = *op;
            let child = thread::spawn(move || {
                assert_not_terminal_thread(op_str);
            });
            child.join().expect("should not panic for worker thread");
        }
    }

    // --- Thread name inspection ---

    #[test]
    fn test_current_thread_name_accessible() {
        // Verify that thread::current().name() is accessible in test context
        let current = std::thread::current();
        let name = current.name();
        // Test runner threads typically have names like "test::test_name" or similar
        // Just verify we can call it without panic
        let _ = name;
    }

    #[test]
    fn test_multiple_threads_different_names() {
        let handles: Vec<_> = (0..5)
            .map(|i| {
                thread::Builder::new()
                    .name(format!("worker-{i}"))
                    .spawn(move || {
                        let is_term = is_terminal_thread();
                        let name = std::thread::current().name().map(ToString::to_string);
                        (is_term, name)
                    })
                    .expect("spawn failed")
            })
            .collect();

        for (i, handle) in handles.into_iter().enumerate() {
            let (is_term, name) = handle.join().expect("thread panicked");
            assert!(!is_term, "worker-{i} should not be terminal thread");
            assert_eq!(name, Some(format!("worker-{i}")));
        }
    }

    // --- Concurrent access ---

    #[test]
    fn test_concurrent_is_terminal_thread_calls() {
        let handles: Vec<_> = (0..20)
            .map(|i| {
                thread::Builder::new()
                    .name(format!("conc-{i}"))
                    .spawn(move || {
                        // Each thread calls is_terminal_thread many times
                        let mut results = Vec::with_capacity(100);
                        for _ in 0..100 {
                            results.push(is_terminal_thread());
                        }
                        results
                    })
                    .expect("spawn failed")
            })
            .collect();

        for (i, handle) in handles.into_iter().enumerate() {
            let results = handle.join().expect("thread panicked");
            assert_eq!(results.len(), 100, "conc-{i} should have 100 results");
            assert!(
                results.iter().all(|&r| !r),
                "conc-{i}: all results should be false (not terminal)"
            );
        }
    }

    #[test]
    fn test_concurrent_assert_not_terminal_thread() {
        let handles: Vec<_> = (0..20)
            .map(|_| {
                thread::spawn(|| {
                    for _ in 0..50 {
                        assert_not_terminal_thread("concurrent_op");
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("no thread should panic");
        }
    }

    // --- Edge cases: thread names ---

    #[test]
    fn test_empty_string_thread_name_is_not_terminal() {
        let child = thread::Builder::new()
            .name(String::new())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(
            !child.join().expect("thread panicked"),
            "empty-named thread should not be terminal"
        );
    }

    #[test]
    fn test_main_substring_thread_name_is_not_terminal() {
        // "main-worker" contains "main" but is NOT the terminal thread
        let child = thread::Builder::new()
            .name("main-worker".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(
            !child.join().expect("thread panicked"),
            "\"main-worker\" should not be terminal thread (exact match required)"
        );
    }

    #[test]
    fn test_my_main_thread_name_is_not_terminal() {
        let child = thread::Builder::new()
            .name("my_main".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(
            !child.join().expect("thread panicked"),
            "\"my_main\" should not be terminal thread (exact match required)"
        );
    }

    #[test]
    fn test_main_case_sensitive_uppercase_is_not_terminal() {
        let child = thread::Builder::new()
            .name("MAIN".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(
            !child.join().expect("thread panicked"),
            "\"MAIN\" should not be terminal thread (case-sensitive)"
        );
    }

    #[test]
    fn test_main_case_sensitive_mixed_case_is_not_terminal() {
        let child = thread::Builder::new()
            .name("Main".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(
            !child.join().expect("thread panicked"),
            "\"Main\" should not be terminal thread (case-sensitive)"
        );
    }

    #[test]
    fn test_main_with_whitespace_is_not_terminal() {
        let child = thread::Builder::new()
            .name("main ".to_string())
            .spawn(is_terminal_thread)
            .expect("spawn failed");
        assert!(
            !child.join().expect("thread panicked"),
            "\"main \" (trailing space) should not be terminal thread"
        );
    }

    // --- Panic message verification ---

    #[test]
    fn test_assert_panic_message_contains_operation_name() {
        let child = thread::Builder::new()
            .name("main".to_string())
            .spawn(|| {
                assert_not_terminal_thread("my_special_operation_123");
            })
            .expect("spawn failed");

        let result = child.join();
        assert!(result.is_err(), "should have panicked");

        // Extract panic message and verify it contains the op name
        let panic_msg = result
            .unwrap_err()
            .downcast::<String>()
            .expect("panic payload should be a String");
        assert!(
            panic_msg.contains("my_special_operation_123"),
            "panic message should contain the operation name: {panic_msg}"
        );
        assert!(
            panic_msg.contains("must not run on the terminal"),
            "panic message should mention terminal thread: {panic_msg}"
        );
    }

    // --- assert_not_terminal_thread on unnamed thread ---

    #[test]
    fn test_assert_not_terminal_on_unnamed_thread() {
        // Unnamed (default) threads are not "main", so should not panic
        let child = thread::spawn(|| {
            assert_not_terminal_thread("unnamed_thread_op");
        });
        child.join().expect("unnamed thread should not panic");
    }

    // --- Special characters in operation names ---

    #[test]
    fn test_assert_with_special_characters_in_op_name() {
        let child = thread::spawn(|| {
            assert_not_terminal_thread("op::with::colons/and/slashes");
        });
        child
            .join()
            .expect("should not panic with special chars in op name");
    }

    #[test]
    fn test_assert_with_unicode_op_name() {
        let child = thread::spawn(|| {
            assert_not_terminal_thread("操作");
        });
        child.join().expect("should not panic with unicode op name");
    }

    #[test]
    fn test_assert_with_empty_op_name() {
        let child = thread::spawn(|| {
            assert_not_terminal_thread("");
        });
        child.join().expect("should not panic with empty op name");
    }

    // --- Consistency across repeated calls ---

    #[test]
    fn test_is_terminal_thread_is_consistent() {
        // On a single thread, repeated calls should always return the same value
        let results: Vec<bool> = (0..50).map(|_| is_terminal_thread()).collect();
        let first = results[0];
        assert!(
            results.iter().all(|&r| r == first),
            "is_terminal_thread should return consistent results on the same thread"
        );
    }

    #[test]
    fn test_consistency_across_named_main_thread() {
        let child = thread::Builder::new()
            .name("main".to_string())
            .spawn(|| {
                let results: Vec<bool> = (0..50).map(|_| is_terminal_thread()).collect();
                let first = results[0];
                (first, results)
            })
            .expect("spawn failed");

        let (first, results) = child.join().expect("thread panicked");
        assert!(first, "main-named thread should be terminal");
        assert!(
            results.iter().all(|&r| r == first),
            "is_terminal_thread should be consistent on main-named thread"
        );
    }

    // --- Rapid spawn/join ---

    #[test]
    fn test_rapid_thread_lifecycle() {
        for i in 0..50 {
            let result = thread::Builder::new()
                .name(format!("rapid-{i}"))
                .spawn(is_terminal_thread)
                .expect("spawn failed")
                .join()
                .expect("thread panicked");
            assert!(!result, "rapid-{i} should not be terminal");
        }
    }

    // --- Many threads asserting simultaneously ---

    #[test]
    fn test_many_threads_assert_not_terminal() {
        let handles: Vec<_> = (0..100)
            .map(|i| {
                thread::Builder::new()
                    .name(format!("pool-{i}"))
                    .spawn(move || {
                        assert_not_terminal_thread("pool_task");
                        is_terminal_thread()
                    })
                    .expect("spawn failed")
            })
            .collect();

        let results: Vec<bool> = handles
            .into_iter()
            .map(|h| h.join().expect("thread panicked"))
            .collect();

        assert!(
            results.iter().all(|&r| !r),
            "no pool thread should be terminal"
        );
    }

    // --- Thread names similar to "main" ---

    #[test]
    fn test_thread_name_main_with_suffix_not_terminal() {
        for name in &["main2", "mains", "main_thread", "0main", ".main"] {
            let child = thread::Builder::new()
                .name(name.to_string())
                .spawn(is_terminal_thread)
                .expect("spawn failed");
            assert!(
                !child.join().expect("thread panicked"),
                "\"{name}\" should not be terminal thread"
            );
        }
    }

    // --- Panic message on main-named thread includes operation context ---

    #[test]
    fn test_multiple_panics_have_different_messages() {
        let op_names = ["alpha_op", "beta_op", "gamma_op"];

        for op in op_names {
            let op_owned = op.to_string();
            let child = thread::Builder::new()
                .name("main".to_string())
                .spawn(move || {
                    assert_not_terminal_thread(&op_owned);
                })
                .expect("spawn failed");

            let err = child.join().unwrap_err();
            let msg = err.downcast::<String>().expect("should be String panic");
            assert!(
                msg.contains(op),
                "panic for \"{op}\" should mention it: {msg}"
            );
        }
    }
}
