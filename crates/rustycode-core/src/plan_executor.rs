//! Plan execution engine.
//!
//! Provides `PlanExecutor` for running plan steps against a tool function,
//! with support for dry-run mode, fail-fast, and step limits.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::recovery::checkpoint::CheckpointRecovery;
use chrono::Utc;
use rustycode_protocol::plan::ToolCall;
use rustycode_protocol::{Plan, PlanStatus, StepStatus, ToolResult};

// ── ExecutionOptions ─────────────────────────────────────────────────────────

/// Options controlling how a plan is executed.
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// Stop on first failed step (default: true).
    pub fail_fast: bool,
    /// Maximum number of steps to run; 0 means unlimited (default: 0).
    pub max_steps: usize,
    /// Simulate execution without invoking tools (default: false).
    pub dry_run: bool,
    /// Whether plan mode (inspection-only) is active
    pub plan_mode_active: bool,
    /// Optional checkpoint manager for automatic state snapshots
    pub checkpoint_manager: Option<Arc<CheckpointRecovery>>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            fail_fast: true,
            max_steps: 0,
            dry_run: false,
            plan_mode_active: false,
            checkpoint_manager: None,
        }
    }
}

// ── ExecutionReport ──────────────────────────────────────────────────────────

/// Summary of a completed plan execution run.
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    /// Total steps in the plan.
    pub steps_total: usize,
    /// Steps that finished with `Completed` status.
    pub steps_completed: usize,
    /// Steps that finished with `Failed` status.
    pub steps_failed: usize,
    /// Steps that were skipped (not `Pending` at start, or halted by fail_fast/max_steps).
    pub steps_skipped: usize,
    /// Wall-clock duration of the execution in milliseconds.
    pub duration_ms: u64,
    /// Whether execution finished without any failures.
    pub success: bool,
    /// Error message from the first failing step, if any.
    pub error: Option<String>,
}

impl ExecutionReport {
    /// Returns a human-readable one-line summary.
    pub fn summary(&self) -> String {
        if self.success {
            format!(
                "Plan executed successfully: {}/{} steps completed in {}ms",
                self.steps_completed, self.steps_total, self.duration_ms
            )
        } else {
            format!(
                "Plan execution failed: {}/{} steps completed, {} failed, {} skipped in {}ms — {}",
                self.steps_completed,
                self.steps_total,
                self.steps_failed,
                self.steps_skipped,
                self.duration_ms,
                self.error.as_deref().unwrap_or("unknown error"),
            )
        }
    }
}

// ── PlanExecutor ─────────────────────────────────────────────────────────────

const INSPECTION_TOOLS: &[&str] = &[
    "read_file",
    "grep",
    "lsp_find_symbol",
    "lsp_get_symbols_overview",
    "list_dir",
    "glob",
];

/// Executes a plan step-by-step, invoking tool calls via a caller-supplied function.
pub struct PlanExecutor {
    options: ExecutionOptions,
}

impl PlanExecutor {
    pub fn new(options: ExecutionOptions) -> Self {
        Self { options }
    }

    fn validate_plan_step(&self, tool_name: &str) -> anyhow::Result<()> {
        if self.options.plan_mode_active && !INSPECTION_TOOLS.contains(&tool_name) {
            return Err(anyhow::anyhow!(
                "Plan mode: only inspection tools allowed (tried '{}')",
                tool_name
            ));
        }
        Ok(())
    }

    /// Execute `plan`, returning the mutated plan and an `ExecutionReport`.
    ///
    /// `tool_fn` is called for each `ToolCall` in a step and must return a `ToolResult`.
    /// `cwd` is the working directory passed to `tool_fn`.
    pub fn execute(
        &self,
        mut plan: Plan,
        tool_fn: &dyn Fn(&ToolCall, &Path) -> anyhow::Result<ToolResult>,
        cwd: &Path,
    ) -> (Plan, ExecutionReport) {
        let wall_start = Instant::now();

        plan.status = PlanStatus::Executing;
        plan.execution_started_at = Some(Utc::now());
        plan.execution_completed_at = None;
        plan.execution_error = None;

        let steps_total = plan.steps.len();
        let mut steps_completed: usize = 0;
        let mut steps_failed: usize = 0;
        let mut steps_skipped: usize = 0;
        let mut first_error: Option<String> = None;
        let mut steps_run: usize = 0;

        for i in 0..plan.steps.len() {
            // Enforce max_steps limit (0 = unlimited).
            if self.options.max_steps > 0 && steps_run >= self.options.max_steps {
                steps_skipped += plan.steps[i..]
                    .iter()
                    .filter(|s| s.execution_status == StepStatus::Pending)
                    .count();
                break;
            }

            // Only execute Pending steps.
            if plan.steps[i].execution_status != StepStatus::Pending {
                steps_skipped += 1;
                continue;
            }

            steps_run += 1;
            plan.current_step_index = Some(i);

            // Mark InProgress.
            plan.steps[i].execution_status = StepStatus::InProgress;
            plan.steps[i].started_at = Some(Utc::now());

            let step_failed: bool;

            if self.options.dry_run {
                // Dry-run: mark completed without invoking tools.
                let description = plan.steps[i].description.clone();
                plan.steps[i]
                    .results
                    .push(format!("DRY RUN: {}", description));
                plan.steps[i].execution_status = StepStatus::Completed;
                plan.steps[i].completed_at = Some(Utc::now());
                step_failed = false;
            } else if plan.steps[i].tool_calls.is_empty() {
                // No tool calls: mark completed with a generic result.
                let description = plan.steps[i].description.clone();
                plan.steps[i]
                    .results
                    .push(format!("{} completed", description));
                plan.steps[i].execution_status = StepStatus::Completed;
                plan.steps[i].completed_at = Some(Utc::now());
                step_failed = false;
            } else {
                // Execute each tool call.
                let mut had_error = false;
                let tool_calls = plan.steps[i].tool_calls.clone();
                for tc in &tool_calls {
                    // Validate plan mode access
                    if let Err(e) = self.validate_plan_step(&tc.name) {
                        plan.steps[i].errors.push(e.to_string());
                        if first_error.is_none() {
                            first_error = Some(e.to_string());
                        }
                        had_error = true;
                        continue;
                    }

                    // Inject automatic checkpoint before potentially destructive tools
                    if let Some(ref manager) = self.options.checkpoint_manager {
                        let trigger_tools = ["bash", "write_file", "edit_file", "apply_patch"];
                        if trigger_tools.contains(&tc.name.as_str()) {
                            let _ = manager.create();
                            tracing::info!("Auto-checkpoint created before tool: {}", tc.name);
                        }
                    }

                    // Convert plan::ToolCall to protocol::ToolCall
                    let proto_call = ToolCall {
                        call_id: tc.call_id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    };
                    match tool_fn(&proto_call, cwd) {
                        Ok(result) => {
                            if result.error.is_none() {
                                plan.steps[i].results.push(result.output.clone());
                            } else {
                                let err = result.error.clone().unwrap_or_else(|| {
                                    format!("Tool {} returned failure", tc.name)
                                });
                                plan.steps[i].errors.push(err.clone());
                                if first_error.is_none() {
                                    first_error = Some(err);
                                }
                                had_error = true;
                            }
                        }
                        Err(e) => {
                            let err = format!("Tool {} error: {}", tc.name, e);
                            plan.steps[i].errors.push(err.clone());
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                            had_error = true;
                        }
                    }
                }

                if had_error {
                    plan.steps[i].execution_status = StepStatus::Failed;
                    plan.steps[i].completed_at = Some(Utc::now());
                    step_failed = true;
                } else {
                    plan.steps[i].execution_status = StepStatus::Completed;
                    plan.steps[i].completed_at = Some(Utc::now());
                    step_failed = false;
                }
            }

            if step_failed {
                steps_failed += 1;
                if self.options.fail_fast {
                    // Count remaining Pending steps as skipped.
                    if i + 1 < plan.steps.len() {
                        steps_skipped += plan.steps[i + 1..]
                            .iter()
                            .filter(|s| s.execution_status == StepStatus::Pending)
                            .count();
                    }
                    break;
                }
            } else {
                steps_completed += 1;
            }
        }

        let duration_ms = wall_start.elapsed().as_millis() as u64;
        let success = steps_failed == 0;

        plan.execution_completed_at = Some(Utc::now());
        plan.status = if success {
            PlanStatus::Completed
        } else {
            PlanStatus::Failed
        };
        if let Some(ref err) = first_error {
            plan.execution_error = Some(err.clone());
        }

        let report = ExecutionReport {
            steps_total,
            steps_completed,
            steps_failed,
            steps_skipped,
            duration_ms,
            success,
            error: first_error,
        };

        (plan, report)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::{PlanId, PlanStatus, PlanStep, SessionId};

    fn make_plan(steps: Vec<PlanStep>) -> Plan {
        Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "test task".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Approved,
            summary: "test plan".to_string(),
            approach: "test approach".to_string(),
            steps,
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        }
    }

    fn make_step(description: &str) -> PlanStep {
        PlanStep {
            order: 0,
            title: description.to_string(),
            description: description.to_string(),
            tools: vec![],
            expected_outcome: "done".to_string(),
            rollback_hint: "undo".to_string(),
            execution_status: StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        }
    }

    fn make_step_with_tool(description: &str, tool_name: &str) -> PlanStep {
        let mut step = make_step(description);
        step.tool_calls = vec![ToolCall {
            call_id: "call-1".to_string(),
            name: tool_name.to_string(),
            arguments: serde_json::json!({}),
        }];
        step
    }

    /// dry_run mode should mark every Pending step as Completed without invoking tool_fn.
    #[test]
    fn test_dry_run_marks_all_completed() {
        let steps = vec![
            make_step_with_tool("step one", "bash"),
            make_step_with_tool("step two", "read_file"),
            make_step("step three"),
        ];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions {
            dry_run: true,
            ..ExecutionOptions::default()
        });

        // tool_fn should never be called in dry-run mode.
        let tool_fn = |_: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            panic!("tool_fn must not be called in dry_run mode");
        };

        let (updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(report.success, "dry run should succeed");
        assert_eq!(report.steps_completed, 3);
        assert_eq!(report.steps_failed, 0);
        assert_eq!(report.steps_skipped, 0);
        assert_eq!(updated_plan.status, PlanStatus::Completed);

        for step in &updated_plan.steps {
            assert_eq!(step.execution_status, StepStatus::Completed);
            assert!(
                step.results.iter().any(|r| r.starts_with("DRY RUN:")),
                "expected DRY RUN result for step '{}'",
                step.description
            );
        }
    }

    /// fail_fast should stop execution after the first failing step and skip the rest.
    #[test]
    fn test_fail_fast_stops_after_first_failure() {
        let steps = vec![
            make_step_with_tool("step one", "bash"),   // will fail
            make_step_with_tool("step two", "bash"),   // should be skipped
            make_step_with_tool("step three", "bash"), // should be skipped
        ];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions {
            fail_fast: true,
            dry_run: false,
            ..ExecutionOptions::default()
        });

        let call_count = std::sync::atomic::AtomicUsize::new(0);
        let tool_fn = |tc: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(ToolResult {
                call_id: tc.call_id.clone(),
                output: String::new(),
                error: Some("simulated failure".to_string()),
                exit_code: Some(1),
                success: false,
                data: None,
            })
        };

        let (updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(!report.success, "execution should have failed");
        assert_eq!(
            report.steps_failed, 1,
            "exactly one step should have failed"
        );
        assert_eq!(report.steps_completed, 0);
        assert_eq!(
            report.steps_skipped, 2,
            "two steps should have been skipped"
        );
        assert_eq!(updated_plan.status, PlanStatus::Failed);
        assert!(updated_plan.execution_error.is_some());

        // Only the first step should have been attempted.
        assert_eq!(updated_plan.steps[0].execution_status, StepStatus::Failed);
        assert_eq!(
            updated_plan.steps[1].execution_status,
            StepStatus::Pending,
            "step two should remain Pending"
        );
        assert_eq!(
            updated_plan.steps[2].execution_status,
            StepStatus::Pending,
            "step three should remain Pending"
        );
    }

    /// fail_fast=false should continue executing after failures.
    #[test]
    fn test_continue_on_failure() {
        let steps = vec![
            make_step_with_tool("step one", "bash"),
            make_step_with_tool("step two", "bash"),
            make_step_with_tool("step three", "bash"),
        ];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions {
            fail_fast: false,
            dry_run: false,
            ..ExecutionOptions::default()
        });

        let tool_fn = |tc: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                call_id: tc.call_id.clone(),
                output: String::new(),
                error: Some("failure".to_string()),
                exit_code: Some(1),
                success: false,
                data: None,
            })
        };

        let (_updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(!report.success);
        assert_eq!(report.steps_failed, 3, "all steps should fail");
        assert_eq!(report.steps_completed, 0);
        assert_eq!(report.steps_skipped, 0, "no steps should be skipped");
    }

    /// max_steps should limit the number of executed steps.
    #[test]
    fn test_max_steps_limits_execution() {
        let steps = vec![
            make_step("step one"),
            make_step("step two"),
            make_step("step three"),
            make_step("step four"),
        ];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions {
            max_steps: 2,
            dry_run: false,
            ..ExecutionOptions::default()
        });

        let tool_fn = |_: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                call_id: "x".into(),
                output: "ok".into(),
                error: None,
                exit_code: Some(0),
                success: true,
                data: None,
            })
        };

        let (_updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(report.success);
        assert_eq!(report.steps_completed, 2);
        assert_eq!(report.steps_skipped, 2, "remaining steps should be skipped");
    }

    /// Steps without tool calls should complete without invoking tool_fn.
    #[test]
    fn test_steps_without_tools_complete_automatically() {
        let steps = vec![make_step("no tools step"), make_step("another no tools")];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions::default());

        let tool_fn = |_: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            panic!("tool_fn should not be called for steps without tools");
        };

        let (_updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(report.success);
        assert_eq!(report.steps_completed, 2);
        assert_eq!(report.steps_failed, 0);
    }

    /// tool_fn returning an Err should mark the step as failed.
    #[test]
    fn test_tool_fn_error_marks_step_failed() {
        let steps = vec![make_step_with_tool("bad tool", "crash")];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions::default());

        let tool_fn = |tc: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            Err(anyhow::anyhow!("Tool {} crashed", tc.name))
        };

        let (updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(!report.success);
        assert_eq!(report.steps_failed, 1);
        assert!(report.error.is_some());
        assert!(report.error.unwrap().contains("crash"));
        assert_eq!(updated_plan.steps[0].execution_status, StepStatus::Failed);
        assert!(!updated_plan.steps[0].errors.is_empty());
    }

    /// Successful tool execution should complete the step.
    #[test]
    fn test_successful_tool_execution() {
        let steps = vec![make_step_with_tool("good step", "read_file")];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions::default());

        let tool_fn = |tc: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                call_id: tc.call_id.clone(),
                output: "file contents here".to_string(),
                error: None,
                exit_code: Some(0),
                success: true,
                data: None,
            })
        };

        let (updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(report.success);
        assert_eq!(report.steps_completed, 1);
        assert_eq!(
            updated_plan.steps[0].execution_status,
            StepStatus::Completed
        );
        assert!(updated_plan.steps[0].results[0].contains("file contents"));
    }

    /// ExecutionReport summary should differ for success vs failure.
    #[test]
    fn test_execution_report_summary() {
        let success_report = ExecutionReport {
            steps_total: 3,
            steps_completed: 3,
            steps_failed: 0,
            steps_skipped: 0,
            duration_ms: 150,
            success: true,
            error: None,
        };
        assert!(success_report.summary().contains("successfully"));
        assert!(success_report.summary().contains("3/3"));
        assert!(success_report.summary().contains("150ms"));

        let failure_report = ExecutionReport {
            steps_total: 3,
            steps_completed: 1,
            steps_failed: 1,
            steps_skipped: 1,
            duration_ms: 200,
            success: false,
            error: Some("tool error".to_string()),
        };
        assert!(failure_report.summary().contains("failed"));
        assert!(failure_report.summary().contains("tool error"));
    }

    /// ExecutionOptions defaults should be reasonable.
    #[test]
    fn test_execution_options_defaults() {
        let opts = ExecutionOptions::default();
        assert!(opts.fail_fast);
        assert_eq!(opts.max_steps, 0, "0 means unlimited");
        assert!(!opts.dry_run);
    }

    /// Already-completed steps should be skipped during execution.
    #[test]
    fn test_already_completed_steps_skipped() {
        let mut step1 = make_step("already done");
        step1.execution_status = StepStatus::Completed;
        let step2 = make_step("pending step");

        let steps = vec![step1, step2];
        let plan = make_plan(steps);

        let executor = PlanExecutor::new(ExecutionOptions::default());

        let tool_fn = |_: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            panic!("no tools to execute");
        };

        let (_updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(report.success);
        assert_eq!(report.steps_completed, 1, "only pending step completes");
        assert_eq!(report.steps_skipped, 1, "completed step is skipped");
    }

    /// A plan with only already-completed steps should still count as a successful no-op.
    #[test]
    fn test_all_completed_steps_still_successful() {
        let mut step1 = make_step("already done 1");
        step1.execution_status = StepStatus::Completed;
        let mut step2 = make_step("already done 2");
        step2.execution_status = StepStatus::Completed;
        let plan = make_plan(vec![step1, step2]);

        let executor = PlanExecutor::new(ExecutionOptions::default());

        let tool_fn = |_: &ToolCall, _: &Path| -> anyhow::Result<ToolResult> {
            panic!("no tools should be executed for already-completed steps");
        };

        let (_updated_plan, report) = executor.execute(plan, &tool_fn, Path::new("/tmp"));

        assert!(report.success);
        assert_eq!(report.steps_completed, 0);
        assert_eq!(report.steps_failed, 0);
        assert_eq!(report.steps_skipped, 2);
    }
}
