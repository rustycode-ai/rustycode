use crate::error::{OrchestrationError, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    pub task_id: String,
    pub task_description: String,
    pub classification: String,
    pub execution_path: String,
    pub outcome: String,
    pub duration_ms: u64,
    pub cost_usd: f64,
    pub escalations: u32,
}

pub struct MetricsDb {
    conn: Arc<Mutex<Connection>>,
}

impl MetricsDb {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| OrchestrationError::Storage {
            message: e.to_string(),
        })?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS execution_metrics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                task_description TEXT NOT NULL,
                classification TEXT,
                execution_path TEXT,
                outcome TEXT,
                duration_ms INTEGER,
                cost_usd REAL,
                escalations INTEGER,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS migration_phase_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_phase TEXT,
                to_phase TEXT,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                metrics_summary TEXT,
                triggered_by TEXT
            );

            CREATE TABLE IF NOT EXISTS rollback_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                phase TEXT,
                reason TEXT,
                metric_degradation TEXT,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                manual BOOLEAN DEFAULT FALSE
            );
        ",
        )
        .map_err(|e| OrchestrationError::Storage {
            message: e.to_string(),
        })?;
        Ok(())
    }

    #[allow(
        clippy::significant_drop_tightening,
        clippy::redundant_closure_for_method_calls
    )]
    pub fn record_execution(&self, metrics: &ExecutionMetrics) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("metrics_db mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        conn.execute(
            "INSERT INTO execution_metrics (task_id, task_description, classification,
                execution_path, outcome, duration_ms, cost_usd, escalations)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                metrics.task_id,
                metrics.task_description,
                metrics.classification,
                metrics.execution_path,
                metrics.outcome,
                metrics.duration_ms.cast_signed(),
                metrics.cost_usd,
                i64::from(metrics.escalations),
            ],
        )
        .map_err(|e| OrchestrationError::Storage {
            message: e.to_string(),
        })?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_metrics(task: &str) -> ExecutionMetrics {
        ExecutionMetrics {
            task_id: task.into(),
            task_description: format!("test task {task}"),
            classification: "simple".into(),
            execution_path: "tier2".into(),
            outcome: "success".into(),
            duration_ms: 100,
            cost_usd: 0.001,
            escalations: 0,
        }
    }

    #[test]
    fn test_in_memory_metrics_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_metrics.db");
        let db = MetricsDb::new(&db_path).unwrap();
        db.record_execution(&make_metrics("t1")).unwrap();
    }

    #[test]
    fn test_record_multiple_executions() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("multi_metrics.db");
        let db = MetricsDb::new(&db_path).unwrap();

        for i in 0..5 {
            let mut m = make_metrics(&format!("t{i}"));
            m.duration_ms = (i + 1) * 100;
            m.cost_usd = f64::from(u32::try_from(i).unwrap_or(0)) * 0.001 + 0.001;
            db.record_execution(&m).unwrap();
        }
    }

    #[test]
    fn test_execution_metrics_clone_debug() {
        let m = make_metrics("t1");
        let cloned = m.clone();
        assert_eq!(cloned.task_id, "t1");
        let _ = format!("{m:?}");
    }

    #[test]
    fn test_metrics_with_escalations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("escalation_metrics.db");
        let db = MetricsDb::new(&db_path).unwrap();

        let mut m = make_metrics("t-esc");
        m.escalations = 3;
        m.outcome = "escalated".into();
        db.record_execution(&m).unwrap();
    }

    #[test]
    fn test_metrics_large_duration() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("large_dur.db");
        let db = MetricsDb::new(&db_path).unwrap();

        let mut m = make_metrics("t-large");
        m.duration_ms = u64::MAX;
        db.record_execution(&m).unwrap();
    }

    #[test]
    fn test_metrics_with_high_cost() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("high_cost.db");
        let db = MetricsDb::new(&db_path).unwrap();

        let mut m = make_metrics("t-cost");
        m.cost_usd = 999.999;
        db.record_execution(&m).unwrap();
    }

    #[test]
    fn test_execution_metrics_field_access() {
        let m = make_metrics("fields");
        assert_eq!(m.task_id, "fields");
        assert_eq!(m.classification, "simple");
        assert_eq!(m.execution_path, "tier2");
        assert_eq!(m.outcome, "success");
        assert_eq!(m.duration_ms, 100);
        assert!((m.cost_usd - 0.001).abs() < f64::EPSILON);
        assert_eq!(m.escalations, 0);
    }

    #[test]
    fn test_metrics_db_schema_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("idempotent.db");
        // Creating twice should not fail (IF NOT EXISTS)
        let _db1 = MetricsDb::new(&db_path).unwrap();
        let _db2 = MetricsDb::new(&db_path).unwrap();
    }

    #[test]
    fn test_metrics_various_classifications() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("classifications.db");
        let db = MetricsDb::new(&db_path).unwrap();

        for (i, classification) in ["simple", "complex", "research", "debugging"]
            .iter()
            .enumerate()
        {
            let mut m = make_metrics(&format!("t{i}"));
            m.classification = (*classification).to_string();
            db.record_execution(&m).unwrap();
        }
    }
}
