//! Integration tests for end-to-end cost tracking.
//! Cost tracking now depends on rustycode_llm which causes circular deps.
//! These tests are temporarily ignored.

use rustycode_storage::{ApiCallRecord, Storage};
use std::sync::Arc;

fn test_db(prefix: &str) -> Arc<Storage> {
    let db_path = std::env::temp_dir().join(format!("{}-{}.db", prefix, std::process::id()));
    let _ = std::fs::remove_file(&db_path);
    let storage = Storage::open(&db_path).expect("failed to open test db");
    Arc::new(storage)
}

fn create_test_session(storage: &Storage) -> String {
    let session = rustycode_protocol::Session::builder()
        .task("test cost")
        .build();
    let id = session.id.to_string();
    storage.insert_session(&session).expect("insert session");
    id
}

#[test]
fn test_save_and_list_api_calls() {
    let storage = test_db("api-calls");
    let session_id = create_test_session(&storage);

    let api_rec = ApiCallRecord {
        id: 0,
        session_id: session_id.clone(),
        model: "claude-sonnet-4-6".to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.015,
        tool_name: Some("edit_file".to_string()),
        provider: Some("anthropic".to_string()),
        called_at: chrono::Utc::now().to_rfc3339(),
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cache_savings_usd: 0.0,
    };
    storage.save_api_call(&api_rec).expect("save api call");

    let calls = storage.list_api_calls(&session_id).expect("list calls");
    assert_eq!(calls.len(), 1);
}

#[test]
fn test_cost_tracker_in_memory() {
    use rustycode_tool_integration::cost::CostTracker;

    let mut tracker = CostTracker::unlimited();
    tracker
        .record_tokens("claude-sonnet-4", 1000, 500, Some("test".to_string()))
        .unwrap();

    assert_eq!(tracker.calls_count(), 1);
    assert!(tracker.total_cost() > 0.0);
    let summary = tracker.session_summary();
    assert_eq!(summary.calls_count, 1);
    assert!(summary.by_model.contains_key("claude-sonnet-4"));
}

#[test]
fn test_budget_enforcement() {
    use rustycode_tool_integration::cost::CostTracker;

    let mut tracker = CostTracker::with_budget(0.01);
    // Cost for 10M tokens will exceed $0.01
    let result = tracker.record_tokens("claude-sonnet-4", 10_000_000, 0, None);
    assert!(result.is_err());
}
