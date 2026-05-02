//! Abstraction for executing individual tasks, implemented by the TUI crate
//! with real `AgentSession` execution. The orchestration crate defines the trait
//! so that `ForkJoinExecutor` and `TaskDispatcher` can use it without depending
//! on TUI types.

use crate::delegation::TaskRole;
use anyhow::Result;
use std::path::PathBuf;

/// Outcome of running a single delegated task.
#[derive(Debug, Clone)]
pub struct TaskRunResult {
    /// Whether the task succeeded.
    pub success: bool,
    /// Text output from the task.
    pub output: String,
    /// Cost in USD.
    pub cost_usd: f64,
    /// Duration in milliseconds.
    pub duration_ms: i64,
}

impl TaskRunResult {
    /// Create a successful result.
    pub fn success(output: impl Into<String>, cost_usd: f64, duration_ms: i64) -> Self {
        Self {
            success: true,
            output: output.into(),
            cost_usd,
            duration_ms,
        }
    }

    /// Create a failed result.
    pub fn failure(reason: impl Into<String>, duration_ms: i64) -> Self {
        Self {
            success: false,
            output: reason.into(),
            cost_usd: 0.0,
            duration_ms,
        }
    }
}

/// Trait for executing individual delegated tasks.
///
/// Implemented by the TUI crate using `AgentSession`. The orchestration crate
/// calls this from `ForkJoinExecutor` to run each parallel fork.
pub trait TaskRunner: Send + Sync {
    /// Execute a single task.
    ///
    /// # Arguments
    /// * `task_description` - What the task should do
    /// * `role` - The semantic role (determines system prompt and allowed tools)
    /// * `path_scope` - Optional list of paths the task should focus on (first is used as cwd)
    /// * `resume_from` - Optional checkpoint to resume from
    fn run_task(
        &self,
        task_description: &str,
        role: TaskRole,
        path_scope: &[PathBuf],
        resume_from: Option<&str>,
    ) -> Result<TaskRunResult>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn task_run_result_success_factory() {
        let result = TaskRunResult::success("done", 0.05, 1200);
        assert!(result.success);
        assert_eq!(result.output, "done");
        assert!((result.cost_usd - 0.05).abs() < f64::EPSILON);
        assert_eq!(result.duration_ms, 1200);
    }

    #[test]
    fn task_run_result_failure_factory() {
        let result = TaskRunResult::failure("timeout exceeded", 500);
        assert!(!result.success);
        assert_eq!(result.output, "timeout exceeded");
        assert!((result.cost_usd).abs() < f64::EPSILON);
        assert_eq!(result.duration_ms, 500);
    }
}
