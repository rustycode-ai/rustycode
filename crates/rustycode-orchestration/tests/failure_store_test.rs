#![allow(clippy::unwrap_used)]
use rustycode_orchestration::failure_store::{
    FailurePattern, FailurePatternStore, SqliteFailureStore,
};
use rustycode_orchestration::SignalCategory;

#[test]
fn test_record_and_query_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("patterns.db");
    let store = SqliteFailureStore::open(&path).unwrap();

    let pattern = FailurePattern {
        task_type: "rust_refactoring".into(),
        step_index: 3,
        error_category: SignalCategory::CompileError,
        suggested_fix: Some("add use statement".into()),
        alternative_approach: None,
        tier_failed: "Tier2".into(),
    };
    store.record_failure(&pattern).unwrap();

    let results = store.query_patterns("rust_refactoring").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].step_index, 3);
}

#[test]
fn test_occurrence_count_increments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("patterns.db");
    let store = SqliteFailureStore::open(&path).unwrap();

    let pattern = FailurePattern {
        task_type: "t".into(),
        step_index: 1,
        error_category: SignalCategory::SyntaxError,
        suggested_fix: None,
        alternative_approach: None,
        tier_failed: "Tier2".into(),
    };
    store.record_failure(&pattern).unwrap();
    store.record_failure(&pattern).unwrap();
    store.record_failure(&pattern).unwrap();

    let results = store.query_patterns("t").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].occurrence_count, 3);
}

#[test]
fn test_custom_category_recording() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("patterns.db");
    let store = SqliteFailureStore::open(&path).unwrap();

    store
        .record_custom_category("NetworkTimeout", "connection timed out")
        .unwrap();
    store
        .record_custom_category("NetworkTimeout", "another timeout")
        .unwrap();
    store
        .record_custom_category("RateLimited", "429 too many requests")
        .unwrap();

    let candidates = store.promotion_candidates(2).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].category_name, "NetworkTimeout");
    assert_eq!(candidates[0].occurrence_count, 2);
}
