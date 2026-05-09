#![allow(clippy::significant_drop_tightening)]
use std::path::Path;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};

use super::{
    CustomCategoryStats, EscalationLog, FailurePattern, FailurePatternStore, StoredPattern,
};
use crate::error::{OrchestrationError, Result};
use crate::error_signal::SignalCategory;

pub struct SqliteFailureStore {
    conn: Mutex<Connection>,
}

impl SqliteFailureStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| OrchestrationError::Storage {
            message: e.to_string(),
        })?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| OrchestrationError::Storage {
            message: e.to_string(),
        })?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[allow(
        clippy::significant_drop_tightening,
        clippy::redundant_closure_for_method_calls
    )]
    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS failure_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_type TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            error_category TEXT NOT NULL,
            occurrence_count INTEGER DEFAULT 1,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            suggested_fix TEXT,
            alternative_approach TEXT,
            tier_failed TEXT,
            escalation_success_rate REAL DEFAULT 0.5,
            UNIQUE(task_type, step_index, error_category)
        );
        CREATE INDEX IF NOT EXISTS idx_patterns_task ON failure_patterns(task_type);
        CREATE INDEX IF NOT EXISTS idx_patterns_error ON failure_patterns(error_category);
        CREATE TABLE IF NOT EXISTS escalation_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            from_state TEXT NOT NULL,
            to_state TEXT NOT NULL,
            error_category TEXT,
            cost_used REAL,
            timestamp TEXT NOT NULL,
            success INTEGER
        );
        CREATE TABLE IF NOT EXISTS custom_categories (
            category_name TEXT PRIMARY KEY,
            occurrence_count INTEGER DEFAULT 1,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            example_messages TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_custom_count ON custom_categories(occurrence_count DESC);",
    )
    .map_err(|e| OrchestrationError::Storage {
        message: e.to_string(),
    })?;
    Ok(())
}

fn encode_category(cat: &SignalCategory) -> String {
    serde_json::to_string(cat).unwrap_or_else(|_| r#""Unknown""#.to_string())
}

fn decode_category(s: &str) -> SignalCategory {
    serde_json::from_str(s).unwrap_or_else(|_| SignalCategory::Custom("Unknown".into()))
}

impl FailurePatternStore for SqliteFailureStore {
    fn record_failure(&self, pattern: &FailurePattern) -> Result<()> {
        let conn = self.lock();
        let now = Utc::now().to_rfc3339();
        let category_json = encode_category(&pattern.error_category);
        conn.execute(
            "INSERT INTO failure_patterns
                (task_type, step_index, error_category, occurrence_count, first_seen, last_seen, suggested_fix, alternative_approach, tier_failed)
            VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?)
            ON CONFLICT(task_type, step_index, error_category) DO UPDATE SET
                occurrence_count = occurrence_count + 1,
                last_seen = excluded.last_seen,
                suggested_fix = COALESCE(excluded.suggested_fix, failure_patterns.suggested_fix),
                alternative_approach = COALESCE(excluded.alternative_approach, failure_patterns.alternative_approach)",
            params![
                pattern.task_type, pattern.step_index, category_json,
                now, now, pattern.suggested_fix, pattern.alternative_approach, pattern.tier_failed,
            ],
        ).map_err(|e| OrchestrationError::Storage { message: e.to_string() })?;
        Ok(())
    }

    fn record_escalation(&self, log: &EscalationLog) -> Result<()> {
        let conn = self.lock();
        let now = Utc::now().to_rfc3339();
        let category_json = log.error_category.as_ref().map(encode_category);
        conn.execute(
            "INSERT INTO escalation_logs
                (task_id, from_state, to_state, error_category, cost_used, timestamp, success)
            VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                log.task_id,
                log.from_state,
                log.to_state,
                category_json,
                log.cost_used,
                now,
                i32::from(log.success),
            ],
        )
        .map_err(|e| OrchestrationError::Storage {
            message: e.to_string(),
        })?;
        Ok(())
    }

    fn record_custom_category(&self, name: &str, _example: &str) -> Result<()> {
        let conn = self.lock();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO custom_categories (category_name, occurrence_count, first_seen, last_seen)
            VALUES (?, 1, ?, ?)
            ON CONFLICT(category_name) DO UPDATE SET
                occurrence_count = occurrence_count + 1,
                last_seen = excluded.last_seen",
            params![name, now, now],
        )
        .map_err(|e| OrchestrationError::Storage {
            message: e.to_string(),
        })?;
        Ok(())
    }

    fn query_patterns(&self, task_type: &str) -> Result<Vec<StoredPattern>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT task_type, step_index, error_category, occurrence_count,
                    first_seen, last_seen, suggested_fix, alternative_approach,
                    tier_failed, escalation_success_rate
             FROM failure_patterns
             WHERE task_type = ?
             ORDER BY occurrence_count DESC",
            )
            .map_err(|e| OrchestrationError::Storage {
                message: e.to_string(),
            })?;
        let rows = stmt
            .query_map(params![task_type], |r| {
                Ok(StoredPattern {
                    task_type: r.get(0)?,
                    step_index: r.get(1)?,
                    error_category: decode_category(&r.get::<_, String>(2)?),
                    occurrence_count: r.get(3)?,
                    first_seen: parse_ts(&r.get::<_, String>(4)?),
                    last_seen: parse_ts(&r.get::<_, String>(5)?),
                    suggested_fix: r.get(6)?,
                    alternative_approach: r.get(7)?,
                    tier_failed: r.get(8)?,
                    escalation_success_rate: r.get(9)?,
                })
            })
            .map_err(|e| OrchestrationError::Storage {
                message: e.to_string(),
            })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| OrchestrationError::Storage {
                message: e.to_string(),
            })?);
        }
        Ok(out)
    }

    fn get_escalation_success_rate(&self, error: &SignalCategory) -> Result<Option<f64>> {
        let conn = self.lock();
        let category_json = encode_category(error);
        let rate: Option<f64> = match conn.query_row(
            "SELECT AVG(escalation_success_rate) FROM failure_patterns WHERE error_category = ?",
            params![category_json],
            |r| r.get(0),
        ) {
            Ok(rate) => Some(rate),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => {
                tracing::debug!(error = %e, "failure pattern rate query failed");
                None
            }
        };
        Ok(rate)
    }

    fn promotion_candidates(&self, min_occurrences: u32) -> Result<Vec<CustomCategoryStats>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT category_name, occurrence_count, first_seen, last_seen
             FROM custom_categories
             WHERE occurrence_count >= ?
             ORDER BY occurrence_count DESC",
            )
            .map_err(|e| OrchestrationError::Storage {
                message: e.to_string(),
            })?;
        let rows = stmt
            .query_map(params![min_occurrences], |r| {
                Ok(CustomCategoryStats {
                    category_name: r.get(0)?,
                    occurrence_count: r.get(1)?,
                    first_seen: parse_ts(&r.get::<_, String>(2)?),
                    last_seen: parse_ts(&r.get::<_, String>(3)?),
                })
            })
            .map_err(|e| OrchestrationError::Storage {
                message: e.to_string(),
            })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| OrchestrationError::Storage {
                message: e.to_string(),
            })?);
        }
        Ok(out)
    }
}

fn parse_ts(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map_or_else(|_| chrono::Utc::now(), |d| d.with_timezone(&chrono::Utc))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_pattern(task: &str, category: SignalCategory) -> FailurePattern {
        FailurePattern {
            task_type: task.into(),
            step_index: 0,
            error_category: category,
            suggested_fix: None,
            alternative_approach: None,
            tier_failed: "tier_2".into(),
        }
    }

    fn make_escalation(task: &str) -> EscalationLog {
        EscalationLog {
            task_id: task.into(),
            from_state: "tier2".into(),
            to_state: "tier3".into(),
            error_category: Some(SignalCategory::LogicError),
            cost_used: 0.01,
            success: true,
        }
    }

    #[test]
    fn test_in_memory_creation() {
        let store = SqliteFailureStore::in_memory().unwrap();
        drop(store);
    }

    #[test]
    fn test_record_and_query_failure() {
        let store = SqliteFailureStore::in_memory().unwrap();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::LogicError))
            .unwrap();

        let patterns = store.query_patterns("Bash").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].occurrence_count, 1);
        assert_eq!(patterns[0].error_category, SignalCategory::LogicError);
    }

    #[test]
    fn test_record_failure_increments_count() {
        let store = SqliteFailureStore::in_memory().unwrap();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::LogicError))
            .unwrap();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::LogicError))
            .unwrap();

        let patterns = store.query_patterns("Bash").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].occurrence_count, 2);
    }

    #[test]
    fn test_query_patterns_filters_by_task_type() {
        let store = SqliteFailureStore::in_memory().unwrap();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::LogicError))
            .unwrap();
        store
            .record_failure(&make_pattern("python", SignalCategory::ToolTimeout))
            .unwrap();

        let bash_patterns = store.query_patterns("Bash").unwrap();
        assert_eq!(bash_patterns.len(), 1);
        assert_eq!(bash_patterns[0].task_type, "Bash");
    }

    #[test]
    fn test_query_patterns_empty() {
        let store = SqliteFailureStore::in_memory().unwrap();
        let patterns = store.query_patterns("nonexistent").unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_record_escalation() {
        let store = SqliteFailureStore::in_memory().unwrap();
        store.record_escalation(&make_escalation("t1")).unwrap();
    }

    #[test]
    fn test_record_custom_category() {
        let store = SqliteFailureStore::in_memory().unwrap();
        store.record_custom_category("my_error", "example").unwrap();
        store
            .record_custom_category("my_error", "another example")
            .unwrap();

        let candidates = store.promotion_candidates(1).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].category_name, "my_error");
        assert_eq!(candidates[0].occurrence_count, 2);
    }

    #[test]
    fn test_promotion_candidates_filters_by_min_occurrences() {
        let store = SqliteFailureStore::in_memory().unwrap();
        store.record_custom_category("rare", "ex1").unwrap();
        store.record_custom_category("common", "ex1").unwrap();
        store.record_custom_category("common", "ex2").unwrap();
        store.record_custom_category("common", "ex3").unwrap();

        let candidates = store.promotion_candidates(3).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].category_name, "common");
    }

    #[test]
    fn test_get_escalation_success_rate_no_data() {
        let store = SqliteFailureStore::in_memory().unwrap();
        let rate = store
            .get_escalation_success_rate(&SignalCategory::LogicError)
            .unwrap();
        assert!(rate.is_none());
    }

    #[test]
    fn test_get_escalation_success_rate_with_data() {
        let store = SqliteFailureStore::in_memory().unwrap();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::LogicError))
            .unwrap();

        let rate = store
            .get_escalation_success_rate(&SignalCategory::LogicError)
            .unwrap();
        assert!(rate.is_some());
    }
}
