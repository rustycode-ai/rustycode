use crate::task_routing::TaskHarness;
use serde::{Deserialize, Serialize};

/// Result returned by any harness after execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Which harness executed this task
    pub harness_used: TaskHarness,

    /// Final conclusion or output
    pub conclusion: String,

    /// Confidence in the conclusion (0.0-1.0)
    pub confidence: f64,

    /// How long execution took (milliseconds)
    pub execution_time_ms: u64,

    /// Verified artifacts (files, decisions, code, etc.)
    pub verified_artifacts: Vec<String>,

    /// Full reasoning graph from Deep-Thinker (if used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_graph: Option<String>,

    /// Metacognitive actions taken during execution
    pub metacognitive_actions: Vec<String>,

    /// If execution succeeded and a new workflow is suggested
    pub next_workflow: Option<TaskHarness>,

    /// If execution failed, the error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ExecutionResult {
    pub fn success(harness: TaskHarness, conclusion: String, confidence: f64) -> Self {
        Self {
            harness_used: harness,
            conclusion,
            confidence,
            execution_time_ms: 0,
            verified_artifacts: vec![],
            reasoning_graph: None,
            metacognitive_actions: vec![],
            next_workflow: None,
            error: None,
        }
    }

    pub fn failure(harness: TaskHarness, error: String) -> Self {
        Self {
            harness_used: harness,
            conclusion: String::new(),
            confidence: 0.0,
            execution_time_ms: 0,
            verified_artifacts: vec![],
            reasoning_graph: None,
            metacognitive_actions: vec![],
            next_workflow: None,
            error: Some(error),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_routing::TaskHarness;

    #[test]
    fn test_success_factory() {
        let result = ExecutionResult::success(TaskHarness::Direct, "completed".into(), 0.9);
        assert!(result.is_success());
        assert_eq!(result.harness_used, TaskHarness::Direct);
        assert_eq!(result.conclusion, "completed");
        assert!((result.confidence - 0.9).abs() < f64::EPSILON);
        assert!(result.verified_artifacts.is_empty());
        assert!(result.reasoning_graph.is_none());
        assert!(result.metacognitive_actions.is_empty());
        assert!(result.next_workflow.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_failure_factory() {
        let result = ExecutionResult::failure(TaskHarness::Tiered, "exhausted retries".into());
        assert!(!result.is_success());
        assert_eq!(result.harness_used, TaskHarness::Tiered);
        assert!(result.conclusion.is_empty());
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.error.as_deref(), Some("exhausted retries"));
    }

    #[test]
    fn test_is_success_with_error() {
        let result = ExecutionResult {
            harness_used: TaskHarness::Direct,
            conclusion: "partial".into(),
            confidence: 0.5,
            execution_time_ms: 100,
            verified_artifacts: vec![],
            reasoning_graph: None,
            metacognitive_actions: vec![],
            next_workflow: None,
            error: Some("timeout".into()),
        };
        assert!(!result.is_success());
    }

    #[test]
    fn test_is_success_without_error() {
        let result = ExecutionResult {
            harness_used: TaskHarness::Direct,
            conclusion: "done".into(),
            confidence: 0.9,
            execution_time_ms: 50,
            verified_artifacts: vec!["file.rs".into()],
            reasoning_graph: Some("graph data".into()),
            metacognitive_actions: vec!["action".into()],
            next_workflow: Some(TaskHarness::Ultrawork),
            error: None,
        };
        assert!(result.is_success());
    }
}
