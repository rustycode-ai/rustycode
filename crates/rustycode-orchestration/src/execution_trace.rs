use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error_signal::ErrorSignal;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEntry {
    pub step_id: String,
    pub step_index: u8,
    pub tier: u8,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub output: String,
    pub exit_code: Option<i32>,
    pub error_signal: Option<ErrorSignal>,
    pub timestamp: DateTime<Utc>,
    pub cost_usd: f64,
}

impl TraceEntry {
    pub fn new_success(
        step_id: String,
        step_index: u8,
        tier: u8,
        tool_name: String,
        tool_args: serde_json::Value,
        output: String,
        exit_code: Option<i32>,
        cost_usd: f64,
    ) -> Self {
        Self {
            step_id,
            step_index,
            tier,
            tool_name,
            tool_args,
            output,
            exit_code,
            error_signal: None,
            timestamp: Utc::now(),
            cost_usd,
        }
    }

    pub fn new_failure(
        step_id: String,
        step_index: u8,
        tier: u8,
        tool_name: String,
        tool_args: serde_json::Value,
        output: String,
        exit_code: Option<i32>,
        error_signal: ErrorSignal,
        cost_usd: f64,
    ) -> Self {
        Self {
            step_id,
            step_index,
            tier,
            tool_name,
            tool_args,
            output,
            exit_code,
            error_signal: Some(error_signal),
            timestamp: Utc::now(),
            cost_usd,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub task_id: String,
    pub steps: Vec<TraceEntry>,
}

impl ExecutionTrace {
    pub const fn new(task_id: String) -> Self {
        Self {
            task_id,
            steps: Vec::new(),
        }
    }

    pub fn append(&mut self, entry: TraceEntry) {
        self.steps.push(entry);
    }

    pub fn total_cost(&self) -> f64 {
        self.steps.iter().map(|s| s.cost_usd).sum()
    }

    pub fn last_n_tool_calls(&self, n: usize) -> Vec<&TraceEntry> {
        self.steps.iter().rev().take(n).collect()
    }

    pub fn failures(&self) -> Vec<&TraceEntry> {
        self.steps
            .iter()
            .filter(|e| e.error_signal.is_some())
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_append() {
        let mut trace = ExecutionTrace::new("t1".into());
        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.01,
        ));
        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.task_id, "t1");
    }

    #[test]
    fn test_total_cost() {
        let mut trace = ExecutionTrace::new("t1".into());
        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.01,
        ));
        trace.append(TraceEntry::new_success(
            "s2".into(),
            1,
            3,
            "bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.05,
        ));
        assert!((trace.total_cost() - 0.06).abs() < f64::EPSILON);
    }

    #[test]
    fn test_failures_returns_only_failed() {
        let mut trace = ExecutionTrace::new("t1".into());
        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.01,
        ));
        let error = ErrorSignal::new(
            crate::error_signal::SignalCategory::LogicError,
            Some(1),
            "failed".into(),
            "s2".into(),
            "bash".into(),
        );
        trace.append(TraceEntry::new_failure(
            "s2".into(),
            1,
            2,
            "bash".into(),
            serde_json::json!({}),
            "error output".into(),
            Some(1),
            error,
            0.01,
        ));
        trace.append(TraceEntry::new_success(
            "s3".into(),
            2,
            2,
            "bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.01,
        ));
        let failures = trace.failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].step_id, "s2");
    }

    #[test]
    fn test_last_n_tool_calls() {
        let mut trace = ExecutionTrace::new("t1".into());
        for i in 0..5 {
            trace.append(TraceEntry::new_success(
                format!("s{i}"),
                i,
                2,
                "bash".into(),
                serde_json::json!({}),
                "ok".into(),
                Some(0),
                0.01,
            ));
        }
        let last_3 = trace.last_n_tool_calls(3);
        assert_eq!(last_3.len(), 3);
        assert_eq!(last_3[0].step_id, "s4");
        assert_eq!(last_3[1].step_id, "s3");
        assert_eq!(last_3[2].step_id, "s2");
    }

    #[test]
    fn test_empty_trace() {
        let trace = ExecutionTrace::new("t1".into());
        assert!(trace.steps.is_empty());
        assert_eq!(trace.total_cost(), 0.0);
        assert!(trace.failures().is_empty());
        assert!(trace.last_n_tool_calls(5).is_empty());
    }

    #[test]
    fn test_trace_entry_new_success_has_no_error() {
        let entry = TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({"cmd": "ls"}),
            "output".into(),
            Some(0),
            0.01,
        );
        assert!(entry.error_signal.is_none());
        assert_eq!(entry.step_id, "s1");
        assert_eq!(entry.step_index, 0);
        assert_eq!(entry.tier, 2);
        assert_eq!(entry.tool_name, "bash");
    }

    #[test]
    fn test_trace_entry_new_failure_has_error() {
        let error = ErrorSignal::new(
            crate::error_signal::SignalCategory::Fatal,
            Some(1),
            "boom".into(),
            "s1".into(),
            "bash".into(),
        );
        let entry = TraceEntry::new_failure(
            "s1".into(),
            0,
            3,
            "bash".into(),
            serde_json::json!({}),
            "error output".into(),
            Some(1),
            error,
            0.02,
        );
        assert!(entry.error_signal.is_some());
        assert_eq!(entry.tier, 3);
        assert_eq!(entry.cost_usd, 0.02);
    }

    #[test]
    fn test_last_n_with_zero() {
        let mut trace = ExecutionTrace::new("t1".into());
        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.01,
        ));
        assert!(trace.last_n_tool_calls(0).is_empty());
    }

    #[test]
    fn test_last_n_exceeds_length() {
        let mut trace = ExecutionTrace::new("t1".into());
        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.01,
        ));
        let last = trace.last_n_tool_calls(10);
        assert_eq!(last.len(), 1);
    }

    #[test]
    fn test_trace_serialization_roundtrip() {
        let mut trace = ExecutionTrace::new("t1".into());
        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({"a": 1}),
            "ok".into(),
            Some(0),
            0.01,
        ));
        let json = serde_json::to_string(&trace).unwrap();
        let back: ExecutionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_id, "t1");
        assert_eq!(back.steps.len(), 1);
        assert_eq!(back.steps[0].step_id, "s1");
    }

    #[test]
    fn test_total_cost_empty() {
        let trace = ExecutionTrace::new("t1".into());
        assert_eq!(trace.total_cost(), 0.0);
    }

    #[test]
    fn test_total_cost_multiple_entries() {
        let mut trace = ExecutionTrace::new("t1".into());
        for i in 0..3 {
            trace.append(TraceEntry::new_success(
                format!("s{i}"),
                i,
                2,
                "bash".into(),
                serde_json::json!({}),
                "ok".into(),
                Some(0),
                0.10,
            ));
        }
        assert!((trace.total_cost() - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn test_all_failures() {
        let mut trace = ExecutionTrace::new("t1".into());
        for i in 0..3 {
            let error = ErrorSignal::new(
                crate::error_signal::SignalCategory::LogicError,
                Some(1),
                "fail".into(),
                format!("s{i}"),
                "bash".into(),
            );
            trace.append(TraceEntry::new_failure(
                format!("s{i}"),
                i,
                2,
                "bash".into(),
                serde_json::json!({}),
                "err".into(),
                Some(1),
                error,
                0.01,
            ));
        }
        assert_eq!(trace.failures().len(), 3);
    }
}
