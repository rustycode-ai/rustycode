use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::error_signal::SignalCategory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    pub task_type: String,
    pub step_index: u8,
    pub error_category: SignalCategory,
    pub suggested_fix: Option<String>,
    pub alternative_approach: Option<String>,
    pub tier_failed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPattern {
    pub task_type: String,
    pub step_index: u8,
    pub error_category: SignalCategory,
    pub occurrence_count: u32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub suggested_fix: Option<String>,
    pub alternative_approach: Option<String>,
    pub tier_failed: String,
    pub escalation_success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationLog {
    pub task_id: String,
    pub from_state: String,
    pub to_state: String,
    pub error_category: Option<SignalCategory>,
    pub cost_used: f64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCategoryStats {
    pub category_name: String,
    pub occurrence_count: u32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

pub trait FailurePatternStore: Send + Sync {
    fn record_failure(&self, pattern: &FailurePattern) -> Result<()>;
    fn record_escalation(&self, log: &EscalationLog) -> Result<()>;
    fn record_custom_category(&self, name: &str, example: &str) -> Result<()>;

    fn query_patterns(&self, task_type: &str) -> Result<Vec<StoredPattern>>;
    fn get_escalation_success_rate(&self, error: &SignalCategory) -> Result<Option<f64>>;
    fn promotion_candidates(&self, min_occurrences: u32) -> Result<Vec<CustomCategoryStats>>;
}

pub mod memory;
pub mod metrics_db;
pub mod sqlite;

pub use memory::MemoryFailureStore;
pub use metrics_db::{ExecutionMetrics, MetricsDb};
pub use sqlite::SqliteFailureStore;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_failure_pattern_serialization() {
        let pattern = FailurePattern {
            task_type: "build".into(),
            step_index: 0,
            error_category: SignalCategory::CompileError,
            suggested_fix: Some("check imports".into()),
            alternative_approach: None,
            tier_failed: "tier2".into(),
        };
        let json = serde_json::to_string(&pattern).unwrap();
        let back: FailurePattern = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_type, "build");
        assert_eq!(back.error_category, SignalCategory::CompileError);
        assert_eq!(back.suggested_fix, Some("check imports".into()));
    }

    #[test]
    fn test_escalation_log_serialization() {
        let log = EscalationLog {
            task_id: "t1".into(),
            from_state: "tier2".into(),
            to_state: "tier3".into(),
            error_category: Some(SignalCategory::LogicError),
            cost_used: 0.05,
            success: true,
        };
        let json = serde_json::to_string(&log).unwrap();
        let back: EscalationLog = serde_json::from_str(&json).unwrap();
        assert!(back.success);
        assert_eq!(back.task_id, "t1");
    }

    #[test]
    fn test_stored_pattern_fields() {
        let now = Utc::now();
        let stored = StoredPattern {
            task_type: "test".into(),
            step_index: 1,
            error_category: SignalCategory::TypeError,
            occurrence_count: 5,
            first_seen: now,
            last_seen: now,
            suggested_fix: None,
            alternative_approach: Some("retry".into()),
            tier_failed: "tier3".into(),
            escalation_success_rate: 0.8,
        };
        assert_eq!(stored.occurrence_count, 5);
    }

    #[test]
    fn test_custom_category_stats_serialization() {
        let now = Utc::now();
        let stats = CustomCategoryStats {
            category_name: "my_error".into(),
            occurrence_count: 3,
            first_seen: now,
            last_seen: now,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: CustomCategoryStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back.category_name, "my_error");
    }
}
