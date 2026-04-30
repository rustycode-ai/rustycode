#![allow(clippy::unwrap_used)]
use rustycode_orchestration::failure_store::{
    EscalationLog, FailurePattern, FailurePatternStore, MemoryFailureStore,
};
use rustycode_orchestration::SignalCategory;

#[test]
fn test_record_and_query_failure() {
    let store = MemoryFailureStore::new();
    let pattern = FailurePattern {
        task_type: "rust_test".into(),
        step_index: 1,
        error_category: SignalCategory::SyntaxError,
        suggested_fix: Some("check semicolons".into()),
        alternative_approach: None,
        tier_failed: "Tier2".into(),
    };
    store.record_failure(&pattern).unwrap();

    let results = store.query_patterns("rust_test").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].error_category, SignalCategory::SyntaxError);
    assert_eq!(results[0].occurrence_count, 1);
    assert_eq!(
        results[0].suggested_fix.as_deref(),
        Some("check semicolons")
    );
}

#[test]
fn test_occurrence_count_increments() {
    let store = MemoryFailureStore::new();
    let pattern = FailurePattern {
        task_type: "t".into(),
        step_index: 0,
        error_category: SignalCategory::CompileError,
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
fn test_record_escalation() {
    let store = MemoryFailureStore::new();
    let log = EscalationLog {
        task_id: "task-1".into(),
        from_state: "Tier2".into(),
        to_state: "Tier3".into(),
        error_category: Some(SignalCategory::LogicError),
        cost_used: 0.5,
        success: false,
    };
    store.record_escalation(&log).unwrap();

    // Verify escalation was recorded (query_patterns should still be empty)
    let patterns = store.query_patterns("task-1").unwrap();
    assert!(patterns.is_empty());
}

#[test]
fn test_query_empty_store() {
    let store = MemoryFailureStore::new();
    let results = store.query_patterns("nonexistent").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_promotion_candidates() {
    let store = MemoryFailureStore::new();
    store.record_custom_category("Rare", "example").unwrap();
    for _ in 0..5 {
        store.record_custom_category("Common", "example").unwrap();
    }

    let candidates = store.promotion_candidates(3).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].category_name, "Common");
    assert_eq!(candidates[0].occurrence_count, 5);
}

#[test]
fn test_escalation_success_rate_returns_none() {
    let store = MemoryFailureStore::new();
    let rate = store
        .get_escalation_success_rate(&SignalCategory::LogicError)
        .unwrap();
    assert!(rate.is_none());
}

#[test]
fn test_different_task_types_isolated() {
    let store = MemoryFailureStore::new();
    let p1 = FailurePattern {
        task_type: "rust".into(),
        step_index: 0,
        error_category: SignalCategory::SyntaxError,
        suggested_fix: None,
        alternative_approach: None,
        tier_failed: "Tier2".into(),
    };
    let p2 = FailurePattern {
        task_type: "python".into(),
        step_index: 0,
        error_category: SignalCategory::LogicError,
        suggested_fix: None,
        alternative_approach: None,
        tier_failed: "Tier2".into(),
    };
    store.record_failure(&p1).unwrap();
    store.record_failure(&p2).unwrap();

    let rust = store.query_patterns("rust").unwrap();
    let python = store.query_patterns("python").unwrap();
    assert_eq!(rust.len(), 1);
    assert_eq!(python.len(), 1);
    assert_eq!(rust[0].error_category, SignalCategory::SyntaxError);
    assert_eq!(python[0].error_category, SignalCategory::LogicError);
}
