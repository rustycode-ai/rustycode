//! Sprint 6: Concurrent Session Isolation Tests
//!
//! Verifies that sessions remain fully isolated under concurrent load.
//! Tests CWD tracking, tool context isolation, and cross-session data integrity.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::manual_assert,
    clippy::single_match_else,
    unused_variables
)]

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rustycode_tools_api::worktree_session::{
    clear_session_original_cwd_for, in_worktree_session_for, session_original_cwd_for,
    set_session_original_cwd_for,
};

fn session_id(idx: usize, suffix: &str) -> String {
    format!("concurrent_test_{suffix}_sess_{idx}")
}

fn cleanup_sessions(count: usize, suffix: &str) {
    for i in 0..count {
        clear_session_original_cwd_for(Some(&session_id(i, suffix)));
    }
}

#[test]
fn concurrent_cwd_10_sessions_no_cross_contamination() {
    run_cwd_isolation_test(10, "cwd10");
}

#[test]
fn concurrent_cwd_25_sessions_no_cross_contamination() {
    run_cwd_isolation_test(25, "cwd25");
}

#[test]
fn concurrent_cwd_50_sessions_no_cross_contamination() {
    run_cwd_isolation_test(50, "cwd50");
}

fn run_cwd_isolation_test(session_count: usize, suffix: &'static str) {
    cleanup_sessions(session_count, suffix);

    let errors = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(session_count);

    for i in 0..session_count {
        let errors = errors.clone();
        let handle = std::thread::spawn(move || {
            let sid = session_id(i, suffix);
            let cwd = PathBuf::from(format!("/tmp/concurrent_test/{sid}"));
            set_session_original_cwd_for(Some(&sid), cwd.clone());

            let read_back = session_original_cwd_for(Some(&sid));
            if read_back != Some(cwd) {
                errors.fetch_add(1, Ordering::Relaxed);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }
    assert_eq!(errors.load(Ordering::SeqCst), 0, "CWD read-back mismatches");

    // Verify all sessions independently after concurrent writes
    for i in 0..session_count {
        let sid = session_id(i, suffix);
        let expected = PathBuf::from(format!("/tmp/concurrent_test/{sid}"));
        assert_eq!(
            session_original_cwd_for(Some(&sid)),
            Some(expected),
            "Session {sid} has wrong CWD after concurrent writes"
        );
    }

    // Clear half, verify other half unaffected
    let clear_count = session_count / 2;
    for i in 0..clear_count {
        clear_session_original_cwd_for(Some(&session_id(i, suffix)));
    }
    for i in clear_count..session_count {
        let sid = session_id(i, suffix);
        assert!(
            in_worktree_session_for(Some(&sid)),
            "Session {sid} was incorrectly cleared"
        );
    }

    cleanup_sessions(session_count, suffix);
}

#[test]
fn stress_rapid_cwd_cycles_no_corruption() {
    let session_count = 20;
    let iterations = 100;
    let suffix = "stress";
    let errors = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(session_count);
    for i in 0..session_count {
        let errors = errors.clone();
        let handle = std::thread::spawn(move || {
            let sid = session_id(i, suffix);
            for iter in 0..iterations {
                let cwd = PathBuf::from(format!("/tmp/stress/{sid}/iter_{iter}"));
                set_session_original_cwd_for(Some(&sid), cwd.clone());
                let read = session_original_cwd_for(Some(&sid));
                if read != Some(cwd) {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
                clear_session_original_cwd_for(Some(&sid));
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    cleanup_sessions(session_count, suffix);
    assert_eq!(
        errors.load(Ordering::SeqCst),
        0,
        "Corruption detected during rapid set/get/clear cycles"
    );
}

#[test]
fn concurrent_tool_contexts_isolated() {
    use rustycode_tools_api::ToolContext;

    let session_count = 20;
    let errors = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(session_count);

    for i in 0..session_count {
        let errors = errors.clone();
        let handle = std::thread::spawn(move || {
            let cwd = PathBuf::from(format!("/tmp/tool_ctx_test/sess_{i}"));
            let ctx = ToolContext::new(&cwd);

            if ctx.cwd != cwd {
                errors.fetch_add(1, Ordering::Relaxed);
            }

            let sid = format!("tool_ctx_{i}");
            set_session_original_cwd_for(Some(&sid), cwd.clone());

            let worktree_cwd = session_original_cwd_for(Some(&sid));
            if worktree_cwd != Some(cwd.clone()) {
                errors.fetch_add(1, Ordering::Relaxed);
            }

            // Context CWD should not be affected by worktree state
            if ctx.cwd != cwd {
                errors.fetch_add(1, Ordering::Relaxed);
            }

            clear_session_original_cwd_for(Some(&sid));
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    assert_eq!(
        errors.load(Ordering::SeqCst),
        0,
        "ToolContext isolation violated under concurrent access"
    );
}

#[test]
fn concurrent_readers_writers_no_deadlock() {
    let suffix = "deadlock";
    let count = 25;
    let iterations = 50;

    for i in 0..count {
        set_session_original_cwd_for(
            Some(&session_id(i, suffix)),
            PathBuf::from(format!("/tmp/deadlock/{i}")),
        );
    }

    let errors = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(count * 2);

    for i in 0..count {
        let errors = errors.clone();
        let handle = std::thread::spawn(move || {
            let sid = session_id(i, suffix);
            for iter in 0..iterations {
                let cwd = PathBuf::from(format!("/tmp/deadlock/{i}/v{iter}"));
                set_session_original_cwd_for(Some(&sid), cwd);
            }
        });
        handles.push(handle);
    }

    for i in 0..count {
        let errors = errors.clone();
        let handle = std::thread::spawn(move || {
            let sid = session_id(i, suffix);
            for _ in 0..iterations {
                let cwd = session_original_cwd_for(Some(&sid));
                if let Some(ref p) = cwd {
                    if !p.starts_with("/tmp/deadlock/") {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        handles.push(handle);
    }

    let start = std::time::Instant::now();
    for h in handles {
        while !h.is_finished() {
            if start.elapsed() > std::time::Duration::from_secs(10) {
                panic!("Deadlock detected: threads did not complete within 10s");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        h.join().expect("thread panicked");
    }

    cleanup_sessions(count, suffix);
    assert_eq!(
        errors.load(Ordering::SeqCst),
        0,
        "Readers observed corrupted data"
    );
}

#[test]
fn concurrent_writes_distinct_keys_preserve_values() {
    let count = 30;
    let suffix = "integrity";

    let barrier = Arc::new(std::sync::Barrier::new(count));
    let mut handles = Vec::with_capacity(count);

    let expected_cwds: HashSet<PathBuf> = (0..count)
        .map(|i| PathBuf::from(format!("/tmp/integrity/sess_{i}")))
        .collect();

    for i in 0..count {
        let barrier = barrier.clone();
        let handle = std::thread::spawn(move || {
            let sid = session_id(i, suffix);
            let cwd = PathBuf::from(format!("/tmp/integrity/sess_{i}"));
            barrier.wait();
            set_session_original_cwd_for(Some(&sid), cwd);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let mut actual_cwds = HashSet::new();
    for i in 0..count {
        let sid = session_id(i, suffix);
        let cwd = session_original_cwd_for(Some(&sid));
        assert!(cwd.is_some(), "Session {sid} lost its CWD");
        actual_cwds.insert(cwd.unwrap());
    }

    assert_eq!(
        actual_cwds, expected_cwds,
        "Data integrity lost: some CWDs were overwritten"
    );

    cleanup_sessions(count, suffix);
}

#[test]
fn stress_no_panics_under_high_concurrency() {
    let session_count = 100;
    let suffix = "panic";
    let iterations = 50;
    let panics = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(session_count);
    for i in 0..session_count {
        let panics = panics.clone();
        let handle = std::thread::spawn(move || {
            let sid = session_id(i, suffix);
            for iter in 0..iterations {
                let cwd = PathBuf::from(format!("/tmp/panic/{sid}/i{iter}"));
                set_session_original_cwd_for(Some(&sid), cwd);
                let _ = session_original_cwd_for(Some(&sid));
                let _ = in_worktree_session_for(Some(&sid));
                clear_session_original_cwd_for(Some(&sid));
            }
        });
        handles.push(handle);
    }

    for (idx, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(()) => {}
            Err(_) => {
                panics.fetch_add(1, Ordering::Relaxed);
                eprintln!("Thread {idx} panicked");
            }
        }
    }

    assert_eq!(
        panics.load(Ordering::SeqCst),
        0,
        "Threads panicked under concurrent load"
    );
}
