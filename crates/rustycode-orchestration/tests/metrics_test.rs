#![allow(clippy::unwrap_used, clippy::float_cmp)]

use rustycode_orchestration::failure_store::metrics_db::{ExecutionMetrics, MetricsDb};
use tempfile::tempdir;

#[test]
fn test_record_and_query_metrics() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("metrics.db");
    let db = MetricsDb::new(&path).unwrap();

    let metrics = ExecutionMetrics {
        task_id: "test_1".into(),
        task_description: "Refactor Rust code".into(),
        classification: "Complex".into(),
        execution_path: "Orchestration".into(),
        outcome: "Success".into(),
        duration_ms: 1000,
        cost_usd: 0.01,
        escalations: 0,
    };

    db.record_execution(&metrics).unwrap();

    // Verify it was recorded
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("SELECT task_id, outcome FROM execution_metrics")
        .unwrap();
    let row = stmt
        .query_row([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap();

    assert_eq!(row.0, "test_1");
    assert_eq!(row.1, "Success");
}
