use rustycode_orchestration::execution_trace::{ExecutionTrace, TraceEntry};

#[test]
fn test_trace_append_only() {
    let mut trace = ExecutionTrace::new("task-1".to_string());
    let entry = TraceEntry::new_success(
        "step-1".to_string(),
        0,
        2,
        "bash".to_string(),
        serde_json::json!({"command": "ls"}),
        "file1\nfile2".to_string(),
        Some(0),
        0.001,
    );
    trace.append(entry);
    assert_eq!(trace.steps.len(), 1);
    assert_eq!(trace.steps[0].tier, 2);
}

#[test]
fn test_trace_total_cost() {
    let mut trace = ExecutionTrace::new("task-2".to_string());
    trace.append(TraceEntry::new_success(
        "step-1".into(),
        0,
        2,
        "bash".into(),
        serde_json::json!({}),
        "ok".into(),
        Some(0),
        0.01,
    ));
    trace.append(TraceEntry::new_success(
        "step-2".into(),
        1,
        3,
        "bash".into(),
        serde_json::json!({}),
        "ok".into(),
        Some(0),
        0.05,
    ));
    assert!((trace.total_cost() - 0.06).abs() < 1e-9);
}
