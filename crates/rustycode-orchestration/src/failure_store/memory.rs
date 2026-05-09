#![allow(clippy::significant_drop_tightening)]
use std::sync::Mutex;

use super::{
    CustomCategoryStats, EscalationLog, FailurePattern, FailurePatternStore, StoredPattern,
};
use crate::error::Result;
use crate::error_signal::SignalCategory;

#[derive(Default)]
pub struct MemoryFailureStore {
    patterns: Mutex<Vec<StoredPattern>>,
    escalations: Mutex<Vec<EscalationLog>>,
    custom_categories: Mutex<Vec<CustomCategoryStats>>,
}

impl MemoryFailureStore {
    pub fn new() -> Self {
        Self::default()
    }
}

macro_rules! lock_mutex {
    ($self:expr, $field:ident) => {
        $self.$field.lock().unwrap_or_else(|e| e.into_inner())
    };
}

impl FailurePatternStore for MemoryFailureStore {
    fn record_failure(&self, pattern: &FailurePattern) -> Result<()> {
        let mut patterns = lock_mutex!(self, patterns);
        let now = chrono::Utc::now();
        if let Some(existing) = patterns.iter_mut().find(|p| {
            p.task_type == pattern.task_type
                && p.step_index == pattern.step_index
                && p.error_category == pattern.error_category
        }) {
            existing.occurrence_count = existing.occurrence_count.saturating_add(1);
            existing.last_seen = now;
        } else {
            patterns.push(StoredPattern {
                task_type: pattern.task_type.clone(),
                step_index: pattern.step_index,
                error_category: pattern.error_category.clone(),
                occurrence_count: 1,
                first_seen: now,
                last_seen: now,
                suggested_fix: pattern.suggested_fix.clone(),
                alternative_approach: pattern.alternative_approach.clone(),
                tier_failed: pattern.tier_failed.clone(),
                escalation_success_rate: 0.5,
            });
        }
        Ok(())
    }

    fn record_escalation(&self, log: &EscalationLog) -> Result<()> {
        let mut escalations = lock_mutex!(self, escalations);
        escalations.push(log.clone());
        Ok(())
    }

    fn record_custom_category(&self, name: &str, _example: &str) -> Result<()> {
        let mut cats = lock_mutex!(self, custom_categories);
        let now = chrono::Utc::now();
        if let Some(existing) = cats.iter_mut().find(|c| c.category_name == name) {
            existing.occurrence_count = existing.occurrence_count.saturating_add(1);
            existing.last_seen = now;
        } else {
            cats.push(CustomCategoryStats {
                category_name: name.to_string(),
                occurrence_count: 1,
                first_seen: now,
                last_seen: now,
            });
        }
        Ok(())
    }

    fn query_patterns(&self, task_type: &str) -> Result<Vec<StoredPattern>> {
        let patterns = lock_mutex!(self, patterns);
        Ok(patterns
            .iter()
            .filter(|p| p.task_type == task_type)
            .cloned()
            .collect())
    }

    fn get_escalation_success_rate(&self, _error: &SignalCategory) -> Result<Option<f64>> {
        Ok(None)
    }

    fn promotion_candidates(&self, min_occurrences: u32) -> Result<Vec<CustomCategoryStats>> {
        let cats = lock_mutex!(self, custom_categories);
        Ok(cats
            .iter()
            .filter(|c| c.occurrence_count >= min_occurrences)
            .cloned()
            .collect())
    }
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
    fn test_new_is_empty() {
        let store = MemoryFailureStore::new();
        let patterns = store.query_patterns("anything").unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_record_and_query_failure() {
        let store = MemoryFailureStore::new();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::LogicError))
            .unwrap();

        let patterns = store.query_patterns("Bash").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].occurrence_count, 1);
    }

    #[test]
    fn test_record_failure_increments_existing() {
        let store = MemoryFailureStore::new();
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
    fn test_record_failure_different_categories() {
        let store = MemoryFailureStore::new();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::LogicError))
            .unwrap();
        store
            .record_failure(&make_pattern("Bash", SignalCategory::ToolTimeout))
            .unwrap();

        let patterns = store.query_patterns("Bash").unwrap();
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn test_record_escalation() {
        let store = MemoryFailureStore::new();
        store.record_escalation(&make_escalation("t1")).unwrap();
        store.record_escalation(&make_escalation("t2")).unwrap();
    }

    #[test]
    fn test_custom_category_increments() {
        let store = MemoryFailureStore::new();
        store.record_custom_category("my_error", "example").unwrap();
        store.record_custom_category("my_error", "another").unwrap();

        let candidates = store.promotion_candidates(1).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].occurrence_count, 2);
    }

    #[test]
    fn test_promotion_candidates_filters() {
        let store = MemoryFailureStore::new();
        store.record_custom_category("rare", "ex").unwrap();
        store.record_custom_category("common", "ex").unwrap();
        store.record_custom_category("common", "ex2").unwrap();
        store.record_custom_category("common", "ex3").unwrap();

        let candidates = store.promotion_candidates(3).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].category_name, "common");
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
    fn test_default_trait() {
        let store = MemoryFailureStore::default();
        let patterns = store.query_patterns("anything").unwrap();
        assert!(patterns.is_empty());
    }
}
