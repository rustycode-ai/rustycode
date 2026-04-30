#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
//! Simplified tests for the current DeadlockDetector API
//!
//! These tests exercise the public surface that exists in
//! `crates/rustycode-core/src/deadlock.rs` as of this edit.

use chrono::Duration as ChronoDuration;
use rustycode_core::deadlock::{DeadlockDetector, DeadlockType, DetectorConfig, LockType};

#[tokio::test]
async fn test_basic_detector_creation() {
    let detector = DeadlockDetector::new();

    let lock_id = detector
        .register_lock("test_mutex".to_string(), LockType::Mutex)
        .await
        .expect("register_lock failed");

    let locks = detector.locks.read().await;
    assert!(locks.contains_key(&lock_id));
    assert_eq!(locks[&lock_id].name, "test_mutex");
}

#[tokio::test]
async fn test_detector_with_custom_config() {
    let config = DetectorConfig::builder()
        .enable_cycle_detection(true)
        .enable_timeout_detection(false)
        .timeout_threshold(ChronoDuration::seconds(10))
        .max_tracked_locks(100)
        .sampling_rate(0.5)
        .build();

    let detector = DeadlockDetector::with_config(config);
    assert_eq!(detector.config.max_tracked_locks, 100);
    assert_eq!(detector.config.sampling_rate, 0.5);
}

#[tokio::test]
async fn test_lock_statistics_and_acquisitions() {
    let detector = DeadlockDetector::new();

    let a = detector
        .register_lock("a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let b = detector
        .register_lock("b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    detector
        .record_acquisition(a, 1, true, Some(10))
        .await
        .unwrap();
    detector
        .record_acquisition(b, 1, true, Some(15))
        .await
        .unwrap();

    let stats = detector.lock_statistics().await;
    assert_eq!(stats.total_locks, 2);
    assert!(stats.total_acquisitions >= 2);
}

#[tokio::test]
async fn test_simple_cycle_detection() {
    let detector = DeadlockDetector::new();

    let a = detector
        .register_lock("res_a".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let b = detector
        .register_lock("res_b".to_string(), LockType::Mutex)
        .await
        .unwrap();

    {
        let mut graph = detector.graph.write().await;
        graph.add_dependency(a, b);
        graph.add_dependency(b, a);
    }

    let report = detector.detect_deadlocks().await;
    assert!(report.has_deadlock());
    assert_eq!(report.deadlock_type, DeadlockType::CycleDetected);
    assert!(report.lock_names.contains(&"res_a".to_string()));
    assert!(report.lock_names.contains(&"res_b".to_string()));
}

#[tokio::test]
async fn test_timeout_detection() {
    let config = DetectorConfig::builder()
        .enable_cycle_detection(false)
        .enable_timeout_detection(true)
        .timeout_threshold(ChronoDuration::milliseconds(100))
        .build();

    let detector = DeadlockDetector::with_config(config);
    let id = detector
        .register_lock("slow".to_string(), LockType::Mutex)
        .await
        .unwrap();

    // insert a pending acquisition older than the threshold
    let mut pending = detector.pending_acquisitions.write().await;
    pending.insert(id, chrono::Utc::now() - ChronoDuration::milliseconds(200));

    let report = detector.detect_deadlocks().await;
    assert!(report.has_deadlock());
    assert_eq!(report.deadlock_type, DeadlockType::TimeoutDetected);
}

#[tokio::test]
async fn test_get_deadlock_reports_nonempty_on_cycle() {
    let detector = DeadlockDetector::new();
    let a = detector
        .register_lock("x".to_string(), LockType::Mutex)
        .await
        .unwrap();
    let b = detector
        .register_lock("y".to_string(), LockType::Mutex)
        .await
        .unwrap();
    {
        let mut g = detector.graph.write().await;
        g.add_dependency(a, b);
        g.add_dependency(b, a);
    }
    let reports = detector.get_deadlock_reports().await;
    assert!(!reports.is_empty());
}
