//! AST Phase 3b: EXECUTE with recovery.
//!
//! The `StepExecutor` processes `ExecutionSegment`s step by step, collecting
//! `StepEvidence` after each step. On failure, a `RecoveryAction` is produced
//! with diagnosis and replanned steps.
//!
//! Real command execution is abstracted behind the `StepRunner` trait so the
//! executor can be tested with mocks. The default runner returns simulated
//! success evidence.

use super::types::{ExecutionSegment, ExecutionStep, RecoveryAction, StepEvidence};
use std::path::PathBuf;

/// Maximum number of retry attempts for a failed step.
pub const MAX_RETRIES: u32 = 2;

/// Abstraction over command execution.
///
/// Production code would inject a real runner; tests inject a mock.
pub trait StepRunner {
    /// Execute a step and return the resulting evidence.
    ///
    /// The runner is responsible for determining exit code, capturing output
    /// summaries, and detecting changed files. It does **not** decide on
    /// recovery -- that is the executor's job.
    fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence;
}

/// Default runner that returns simulated success evidence.
///
/// Useful for dry-run / planning mode and as a baseline for tests that
/// do not care about actual execution.
pub struct SimulatedRunner;

impl StepRunner for SimulatedRunner {
    fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
        StepEvidence {
            step_index,
            command_run: step.expected_command.clone(),
            exit_code: 0,
            stdout_summary: String::new(),
            stderr_summary: String::new(),
            changed_files: step.file_targets.clone(),
            verification_passed: step.verification_command.as_ref().map(|_| true),
        }
    }
}

/// Executes steps via real shell commands.
///
/// For each step, runs `expected_command` (if set) in the workspace directory,
/// captures real `stdout/stderr/exit_code`, checks which `file_targets` exist on
/// disk, and optionally runs `verification_command`.
pub struct ShellStepRunner {
    workspace: PathBuf,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl ShellStepRunner {
    pub const fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            timeout_secs: 120,
        }
    }
}

const MAX_SUMMARY_LEN: usize = 500;

fn truncate_summary(s: &str) -> String {
    if s.len() <= MAX_SUMMARY_LEN {
        s.to_string()
    } else {
        let mut truncated = s[..s.floor_char_boundary(MAX_SUMMARY_LEN)].to_string();
        truncated.push_str("... [truncated]");
        truncated
    }
}

impl StepRunner for ShellStepRunner {
    fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
        // Determine primary command to run
        let primary_cmd = step
            .expected_command
            .as_deref()
            .or(step.verification_command.as_deref());

        let Some(cmd_str) = primary_cmd else {
            // Step has no concrete command — it's a planning/abstract step.
            // Return success so the pipeline proceeds; real execution happens
            // via the tool harness when wired through an LLM agent.
            return StepEvidence {
                step_index,
                command_run: None,
                exit_code: 0,
                stdout_summary: String::new(),
                stderr_summary: String::new(),
                changed_files: self.existing_files(&step.file_targets),
                verification_passed: None,
            };
        };

        // Run the primary command
        let result = self.run_shell_command(cmd_str);

        // Only run verification if main command succeeded
        let verification_passed = if result.exit_code == 0 {
            step.verification_command.as_ref().map(|verify_cmd| {
                if step.expected_command.is_some() {
                    let verify_result = self.run_shell_command(verify_cmd);
                    verify_result.exit_code == 0
                } else {
                    // verification_command was already run as the primary command
                    true
                }
            })
        } else {
            step.verification_command.as_ref().map(|_| false)
        };

        StepEvidence {
            step_index,
            command_run: Some(cmd_str.to_string()),
            exit_code: result.exit_code,
            stdout_summary: truncate_summary(&result.stdout),
            stderr_summary: truncate_summary(&result.stderr),
            changed_files: self.existing_files(&step.file_targets),
            verification_passed,
        }
    }
}

struct ShellResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

impl ShellStepRunner {
    fn run_shell_command(&self, cmd: &str) -> ShellResult {
        let output = rustycode_tools::subprocess::new_shell_command(cmd)
            .current_dir(&self.workspace)
            .output();

        match output {
            Ok(out) => ShellResult {
                exit_code: out.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            },
            Err(e) => ShellResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("Failed to execute command: {e}"),
            },
        }
    }

    fn existing_files(&self, targets: &[PathBuf]) -> Vec<PathBuf> {
        targets
            .iter()
            .filter(|p| self.workspace.join(p).exists() || p.exists())
            .cloned()
            .collect()
    }
}

/// A configurable step runner that always fails with a specified exit code and stderr.
///
/// Designed for testing failure paths: recovery actions, retry exhaustion,
/// milestone failure propagation, and partial success scenarios.
pub struct FailingStepRunner {
    /// Exit code to return (non-zero indicates failure).
    pub exit_code: i32,
    /// Stderr summary to include in evidence.
    pub stderr: String,
}

impl FailingStepRunner {
    pub const fn new(exit_code: i32, stderr: String) -> Self {
        Self { exit_code, stderr }
    }

    /// Create a runner that fails with exit code 1 and a generic error message.
    pub fn generic() -> Self {
        Self {
            exit_code: 1,
            stderr: "command failed".into(),
        }
    }
}

impl StepRunner for FailingStepRunner {
    fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
        StepEvidence {
            step_index,
            command_run: step.expected_command.clone(),
            exit_code: self.exit_code,
            stdout_summary: String::new(),
            stderr_summary: self.stderr.clone(),
            changed_files: vec![],
            verification_passed: None,
        }
    }
}

/// Executes `ExecutionSegment`s, collecting evidence and producing recovery
/// actions on failure.
pub struct StepExecutor<R: StepRunner> {
    runner: R,
}

impl StepExecutor<SimulatedRunner> {
    /// Create an executor with the default simulated runner.
    pub const fn new() -> Self {
        Self {
            runner: SimulatedRunner,
        }
    }
}

impl<R: StepRunner> StepExecutor<R> {
    /// Create an executor with a custom runner (for testing or injection).
    pub const fn with_runner(runner: R) -> Self {
        Self { runner }
    }

    /// Execute a single step and return evidence.
    pub fn execute_step(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
        self.runner.run(step, step_index)
    }

    /// Execute all steps in a segment sequentially.
    ///
    /// Returns a tuple of:
    /// * All collected evidence (including for failed steps).
    /// * `Some(RecoveryAction)` if any step failed, `None` if all passed.
    pub fn execute_segment(
        &self,
        segment: &ExecutionSegment,
    ) -> (Vec<StepEvidence>, Option<RecoveryAction>) {
        let mut evidence = Vec::with_capacity(segment.steps.len());

        for (idx, step) in segment.steps.iter().enumerate() {
            let step_evidence = self.execute_step(step, idx);
            let failed = step_evidence.exit_code != 0;
            evidence.push(step_evidence);

            if failed {
                let recovery = RecoveryAction {
                    failed_step: idx,
                    diagnosis: self.diagnose_failure(&evidence, idx),
                    research_needed: self.suggest_research(step),
                    replanned_steps: self.replan_steps(step, evidence.len() as u32),
                    retry_attempt: 0,
                };
                return (evidence, Some(recovery));
            }
        }

        (evidence, None)
    }

    /// Execute a segment with retry support.
    ///
    /// On failure, replanned steps from the recovery action are executed
    /// up to `MAX_RETRIES` times.
    pub fn execute_segment_with_retry(
        &self,
        segment: &ExecutionSegment,
    ) -> (Vec<StepEvidence>, Option<RecoveryAction>) {
        let mut all_evidence = Vec::new();
        let mut current_steps: Vec<ExecutionStep> = segment.steps.clone();
        let mut attempt: u32 = 0;

        loop {
            let sub_segment = ExecutionSegment {
                milestone_id: segment.milestone_id,
                steps: current_steps.clone(),
                required_criteria: vec![],
                edge_cases: vec![],
            };
            let (evidence, recovery) = self.execute_segment(&sub_segment);
            all_evidence.extend(evidence);

            match recovery {
                None => return (all_evidence, None),
                Some(mut action) => {
                    attempt += 1;
                    if attempt > MAX_RETRIES {
                        action.retry_attempt = attempt;
                        return (all_evidence, Some(action));
                    }
                    action.retry_attempt = attempt;
                    current_steps.clone_from(&action.replanned_steps);
                    if current_steps.is_empty() {
                        action.retry_attempt = attempt;
                        return (all_evidence, Some(action));
                    }
                }
            }
        }
    }

    /// Produce a diagnosis string for a failed step.
    #[allow(clippy::unused_self)]
    fn diagnose_failure(&self, evidence: &[StepEvidence], failed_index: usize) -> String {
        let failed = &evidence[failed_index];
        let mut parts = Vec::new();

        parts.push(format!(
            "Step {} failed with exit code {}",
            failed_index, failed.exit_code
        ));

        if !failed.stderr_summary.is_empty() {
            parts.push(format!("stderr: {}", failed.stderr_summary));
        }

        if !failed.stdout_summary.is_empty() {
            parts.push(format!("stdout: {}", failed.stdout_summary));
        }

        parts.join("; ")
    }

    /// Suggest research topics based on the failed step.
    #[allow(clippy::unused_self)]
    fn suggest_research(&self, step: &ExecutionStep) -> Vec<String> {
        let mut topics = Vec::new();

        if !step.file_targets.is_empty() {
            let files = step.file_targets
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
            topics.push(format!("Investigate files: {files}"));
            topics.push(format!("Verify symbol existence in: {files} (Mandatory step before coding)"));
        }

        if let Some(ref cmd) = step.expected_command {
            topics.push(format!("Understand command behavior: {cmd}"));
        }

        if step.is_risky {
            topics.push("Review risky operation safety constraints".into());
        }

        topics
    }

    /// Generate replanned steps from a failed step.
    ///
    /// Strategy: produce contextual recovery steps based on what the failed
    /// step was trying to do, rather than blindly retrying the same action.
    #[allow(clippy::unused_self)]
    fn replan_steps(&self, failed_step: &ExecutionStep, attempt: u32) -> Vec<ExecutionStep> {
        let mut steps = Vec::new();

        // If the step has a verification command, suggest running it first
        // to check the environment before retrying.
        if let Some(ref verify_cmd) = failed_step.verification_command {
            steps.push(ExecutionStep {
                action: format!("Verify environment before retry: {verify_cmd}"),
                file_targets: vec![],
                expected_command: Some(verify_cmd.clone()),
                verification_command: None,
                is_risky: false,
                recovery_notes: Some(format!(
                    "Pre-retry verification (attempt {attempt}) for: {}",
                    failed_step.action
                )),
            });
        }

        // If the step targets files, suggest checking their state.
        if !failed_step.file_targets.is_empty() {
            let files = failed_step
                .file_targets
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            steps.push(ExecutionStep {
                action: format!("Check file state: {files}"),
                file_targets: failed_step.file_targets.clone(),
                expected_command: Some(format!("ls -la {files}")),
                verification_command: None,
                is_risky: false,
                recovery_notes: Some(format!(
                    "File existence check (attempt {attempt}) — original action: {}",
                    failed_step.action
                )),
            });
        }

        // Retry the original step with risk flag and recovery context.
        steps.push(ExecutionStep {
            action: format!("Retry (attempt {}): {}", attempt + 1, failed_step.action),
            file_targets: failed_step.file_targets.clone(),
            expected_command: failed_step.expected_command.clone(),
            verification_command: failed_step.verification_command.clone(),
            is_risky: true,
            recovery_notes: Some(format!(
                "Retry after failure — original: {}, attempt: {attempt}",
                failed_step.action
            )),
        });

        steps
    }
}

impl Default for StepExecutor<SimulatedRunner> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_step(action: &str, _exit_code: i32) -> ExecutionStep {
        ExecutionStep {
            action: action.into(),
            file_targets: vec![PathBuf::from("src/main.rs")],
            expected_command: Some("cargo build".into()),
            verification_command: None,
            is_risky: false,
            recovery_notes: None,
        }
    }

    fn sample_segment(steps: Vec<ExecutionStep>) -> ExecutionSegment {
        ExecutionSegment {
            milestone_id: 0,
            steps,
            required_criteria: vec![],
            edge_cases: vec![],
        }
    }

    // -- Simulated runner tests --

    #[test]
    fn simulated_runner_returns_exit_code_zero() {
        let runner = SimulatedRunner;
        let step = sample_step("build", 0);
        let evidence = runner.run(&step, 0);

        assert_eq!(evidence.exit_code, 0);
        assert_eq!(evidence.step_index, 0);
    }

    #[test]
    fn simulated_runner_copies_file_targets() {
        let runner = SimulatedRunner;
        let step = sample_step("edit file", 0);
        let evidence = runner.run(&step, 0);

        assert_eq!(evidence.changed_files.len(), 1);
        assert_eq!(evidence.changed_files[0], PathBuf::from("src/main.rs"));
    }

    #[test]
    fn simulated_runner_sets_verification_passed() {
        let runner = SimulatedRunner;
        let step = ExecutionStep {
            action: "run tests".into(),
            file_targets: vec![],
            expected_command: Some("cargo test".into()),
            verification_command: Some("cargo test".into()),
            is_risky: false,
            recovery_notes: None,
        };
        let evidence = runner.run(&step, 3);

        assert_eq!(evidence.verification_passed, Some(true));
    }

    // -- Executor with simulated runner --

    #[test]
    fn execute_step_returns_evidence() {
        let executor = StepExecutor::new();
        let step = sample_step("build", 0);
        let evidence = executor.execute_step(&step, 0);

        assert_eq!(evidence.exit_code, 0);
        assert_eq!(evidence.step_index, 0);
    }

    #[test]
    fn execute_segment_all_pass_no_recovery() {
        let executor = StepExecutor::new();
        let segment = sample_segment(vec![sample_step("step 1", 0), sample_step("step 2", 0)]);
        let (evidence, recovery) = executor.execute_segment(&segment);

        assert_eq!(evidence.len(), 2);
        assert!(recovery.is_none());
    }

    #[test]
    fn execute_segment_stops_on_failure() {
        // Use a mock runner that fails on step index 1.
        struct FailingRunner;
        impl StepRunner for FailingRunner {
            fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
                if step_index == 1 {
                    StepEvidence {
                        step_index,
                        command_run: step.expected_command.clone(),
                        exit_code: 1,
                        stdout_summary: String::new(),
                        stderr_summary: "error: build failed".into(),
                        changed_files: vec![],
                        verification_passed: None,
                    }
                } else {
                    StepEvidence {
                        step_index,
                        command_run: step.expected_command.clone(),
                        exit_code: 0,
                        stdout_summary: String::new(),
                        stderr_summary: String::new(),
                        changed_files: step.file_targets.clone(),
                        verification_passed: None,
                    }
                }
            }
        }

        let executor = StepExecutor::with_runner(FailingRunner);
        let segment = sample_segment(vec![
            sample_step("step 1", 0),
            sample_step("step 2", 0),
            sample_step("step 3", 0), // Should not be reached.
        ]);
        let (evidence, recovery) = executor.execute_segment(&segment);

        // Only 2 evidence items: step 0 passed, step 1 failed.
        assert_eq!(evidence.len(), 2);
        assert!(recovery.is_some());

        let action = recovery.unwrap();
        assert_eq!(action.failed_step, 1);
        assert!(action.diagnosis.contains("exit code 1"));
        assert!(action.diagnosis.contains("build failed"));
    }

    #[test]
    fn recovery_action_includes_research_topics() {
        let executor = StepExecutor::new();
        let step = sample_step("edit auth module", 0);
        let topics = executor.suggest_research(&step);

        assert!(topics.iter().any(|t| t.contains("src/main.rs")));
        assert!(topics.iter().any(|t| t.contains("cargo build")));
    }

    #[test]
    fn recovery_action_includes_risky_note() {
        let executor = StepExecutor::new();
        let step = ExecutionStep {
            action: "drop database table".into(),
            file_targets: vec![],
            expected_command: None,
            verification_command: None,
            is_risky: true,
            recovery_notes: None,
        };
        let topics = executor.suggest_research(&step);

        assert!(topics.iter().any(|t| t.contains("risky")));
    }

    #[test]
    fn replan_steps_produces_retry_with_note() {
        let executor = StepExecutor::new();
        let step = sample_step("compile", 0);
        let replanned = executor.replan_steps(&step, 1);

        // Should include file check + retry = 2 steps
        assert!(!replanned.is_empty());
        let retry = replanned.last().unwrap();
        assert!(retry.action.contains("Retry"));
        assert!(retry.recovery_notes.is_some());
        assert!(retry.is_risky, "retry step should be marked risky");
    }

    // -- US-003: replan_steps strategy tests --

    #[test]
    fn replan_steps_with_verification_includes_verify_step() {
        let executor = StepExecutor::new();
        let step = ExecutionStep {
            action: "run tests".into(),
            file_targets: vec![],
            expected_command: Some("cargo test".into()),
            verification_command: Some("cargo test".into()),
            is_risky: false,
            recovery_notes: None,
        };
        let replanned = executor.replan_steps(&step, 1);

        // Should have: verify step + retry step = 2
        assert_eq!(replanned.len(), 2);
        assert!(
            replanned[0].action.contains("Verify environment"),
            "first step should be verification, got: {}",
            replanned[0].action
        );
        assert!(replanned[0].action.contains("cargo test"));
        assert!(!replanned[0].is_risky);
    }

    #[test]
    fn replan_steps_with_file_targets_includes_file_check() {
        let executor = StepExecutor::new();
        let step = ExecutionStep {
            action: "edit config".into(),
            file_targets: vec![PathBuf::from("src/main.rs"), PathBuf::from("src/lib.rs")],
            expected_command: Some("cargo build".into()),
            verification_command: None,
            is_risky: false,
            recovery_notes: None,
        };
        let replanned = executor.replan_steps(&step, 2);

        // Should have: file check + retry = 2
        assert_eq!(replanned.len(), 2);
        assert!(
            replanned[0].action.contains("Check file state"),
            "first step should be file check, got: {}",
            replanned[0].action
        );
        assert!(replanned[0].action.contains("src/main.rs"));
    }

    #[test]
    fn replan_steps_with_both_verification_and_files() {
        let executor = StepExecutor::new();
        let step = ExecutionStep {
            action: "run build and tests".into(),
            file_targets: vec![PathBuf::from("src/main.rs")],
            expected_command: Some("cargo build".into()),
            verification_command: Some("cargo test".into()),
            is_risky: false,
            recovery_notes: None,
        };
        let replanned = executor.replan_steps(&step, 3);

        // verify + file check + retry = 3
        assert_eq!(replanned.len(), 3);
        assert!(replanned[0].action.contains("Verify environment"));
        assert!(replanned[1].action.contains("Check file state"));
        assert!(replanned[2].action.contains("Retry"));
    }

    #[test]
    fn replan_steps_marks_retry_as_risky() {
        let executor = StepExecutor::new();
        let step = ExecutionStep {
            action: "deploy".into(),
            file_targets: vec![],
            expected_command: None,
            verification_command: None,
            is_risky: false,
            recovery_notes: None,
        };
        let replanned = executor.replan_steps(&step, 1);

        let retry = replanned.last().unwrap();
        assert!(retry.is_risky, "retry step must be marked risky");
        assert!(retry.action.contains("Retry (attempt 2)"));
    }

    #[test]
    fn replan_steps_recovery_notes_include_attempt_context() {
        let executor = StepExecutor::new();
        let step = sample_step("build project", 0);
        let replanned = executor.replan_steps(&step, 4);

        for step in &replanned {
            assert!(
                step.recovery_notes.is_some(),
                "all replanned steps should have recovery notes"
            );
        }
        let retry = replanned.last().unwrap();
        let notes = retry.recovery_notes.as_ref().unwrap();
        assert!(
            notes.contains("attempt: 4"),
            "recovery notes should mention attempt number, got: {notes}"
        );
        assert!(
            notes.contains("build project"),
            "recovery notes should mention original action"
        );
    }

    #[test]
    fn execute_segment_with_retry_stops_at_max_retries() {
        struct AlwaysFailRunner;
        impl StepRunner for AlwaysFailRunner {
            fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
                StepEvidence {
                    step_index,
                    command_run: step.expected_command.clone(),
                    exit_code: 1,
                    stdout_summary: String::new(),
                    stderr_summary: "fatal error".into(),
                    changed_files: vec![],
                    verification_passed: None,
                }
            }
        }

        let executor = StepExecutor::with_runner(AlwaysFailRunner);
        let segment = sample_segment(vec![sample_step("failing step", 0)]);
        let (_evidence, recovery) = executor.execute_segment_with_retry(&segment);

        let action = recovery.unwrap();
        assert_eq!(action.retry_attempt, MAX_RETRIES + 1);
    }

    #[test]
    fn empty_segment_produces_no_evidence() {
        let executor = StepExecutor::new();
        let segment = sample_segment(vec![]);
        let (evidence, recovery) = executor.execute_segment(&segment);

        assert!(evidence.is_empty());
        assert!(recovery.is_none());
    }

    #[test]
    fn default_impl_matches_new() {
        let _default: StepExecutor<SimulatedRunner> = StepExecutor::default();
        let _new: StepExecutor<SimulatedRunner> = StepExecutor::new();
    }

    // -- US-002: FailingStepRunner and failure-path tests --

    #[test]
    fn failing_step_runner_returns_configured_exit_code() {
        let runner = FailingStepRunner::new(42, "custom error".into());
        let step = sample_step("build", 0);
        let evidence = runner.run(&step, 0);
        assert_eq!(evidence.exit_code, 42);
        assert_eq!(evidence.stderr_summary, "custom error");
    }

    #[test]
    fn failing_step_runner_generic_factory() {
        let runner = FailingStepRunner::generic();
        let step = sample_step("build", 0);
        let evidence = runner.run(&step, 0);
        assert_eq!(evidence.exit_code, 1);
        assert_eq!(evidence.stderr_summary, "command failed");
    }

    #[test]
    fn failing_step_runner_no_changed_files() {
        let runner = FailingStepRunner::generic();
        let step = sample_step("edit file", 0);
        let evidence = runner.run(&step, 0);
        assert!(evidence.changed_files.is_empty());
    }

    #[test]
    fn single_step_failure_produces_recovery_action() {
        let executor = StepExecutor::with_runner(FailingStepRunner::generic());
        let segment = sample_segment(vec![sample_step("build", 0)]);
        let (evidence, recovery) = executor.execute_segment(&segment);

        assert_eq!(evidence.len(), 1);
        let action = recovery.expect("should produce recovery action");
        assert_eq!(action.failed_step, 0);
        assert!(action.diagnosis.contains("exit code 1"));
        assert!(action.diagnosis.contains("command failed"));
    }

    #[test]
    fn recovery_action_has_research_topics() {
        let executor = StepExecutor::with_runner(FailingStepRunner::generic());
        let segment = sample_segment(vec![sample_step("edit auth", 0)]);
        let (_evidence, recovery) = executor.execute_segment(&segment);
        let action = recovery.unwrap();
        assert!(!action.research_needed.is_empty());
        assert!(action
            .research_needed
            .iter()
            .any(|t| t.contains("src/main.rs")));
        assert!(action
            .research_needed
            .iter()
            .any(|t| t.contains("cargo build")));
    }

    #[test]
    fn recovery_action_has_replanned_steps() {
        let executor = StepExecutor::with_runner(FailingStepRunner::generic());
        let segment = sample_segment(vec![sample_step("compile", 0)]);
        let (_evidence, recovery) = executor.execute_segment(&segment);
        let action = recovery.unwrap();
        assert!(!action.replanned_steps.is_empty());
        assert!(
            action
                .replanned_steps
                .iter()
                .any(|s| s.action.contains("Retry")),
            "replanned steps should include a retry step"
        );
    }

    #[test]
    fn execute_segment_with_retry_exhausts_max_retries_with_failing_runner() {
        let executor = StepExecutor::with_runner(FailingStepRunner::generic());
        let segment = sample_segment(vec![sample_step("failing step", 0)]);
        let (evidence, recovery) = executor.execute_segment_with_retry(&segment);

        let action = recovery.expect("should still fail after retries");
        assert_eq!(action.retry_attempt, MAX_RETRIES + 1);
        // Evidence should contain attempts: initial + MAX_RETRIES replays
        assert!(
            evidence.len() > 1,
            "should have evidence from multiple attempts"
        );
    }

    #[test]
    fn failure_stops_at_first_failing_step_in_segment() {
        let executor = StepExecutor::with_runner(FailingStepRunner::generic());
        let segment = sample_segment(vec![
            sample_step("step 1", 0),
            sample_step("step 2", 0),
            sample_step("step 3", 0),
        ]);
        let (evidence, recovery) = executor.execute_segment(&segment);

        // Only 1 evidence — first step failed immediately
        assert_eq!(evidence.len(), 1);
        assert!(recovery.is_some());
    }

    #[test]
    fn mixed_segment_first_fails_rest_skipped() {
        // Mix: first step succeeds, second fails
        struct MixedRunner;
        impl StepRunner for MixedRunner {
            fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
                if step_index == 1 {
                    StepEvidence {
                        step_index,
                        command_run: step.expected_command.clone(),
                        exit_code: 1,
                        stdout_summary: String::new(),
                        stderr_summary: "partial failure".into(),
                        changed_files: vec![],
                        verification_passed: None,
                    }
                } else {
                    StepEvidence {
                        step_index,
                        command_run: step.expected_command.clone(),
                        exit_code: 0,
                        stdout_summary: String::new(),
                        stderr_summary: String::new(),
                        changed_files: step.file_targets.clone(),
                        verification_passed: step.verification_command.as_ref().map(|_| true),
                    }
                }
            }
        }

        let executor = StepExecutor::with_runner(MixedRunner);
        let segment = sample_segment(vec![
            sample_step("step 1", 0),
            sample_step("step 2", 0),
            sample_step("step 3", 0),
        ]);
        let (evidence, recovery) = executor.execute_segment(&segment);

        // Step 0 passed, step 1 failed — 2 evidence items
        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].exit_code, 0);
        assert_eq!(evidence[1].exit_code, 1);
        let action = recovery.unwrap();
        assert_eq!(action.failed_step, 1);
        assert!(action.diagnosis.contains("partial failure"));
    }

    #[test]
    fn empty_segment_with_failing_runner_still_no_recovery() {
        let executor = StepExecutor::with_runner(FailingStepRunner::generic());
        let segment = sample_segment(vec![]);
        let (evidence, recovery) = executor.execute_segment(&segment);
        assert!(evidence.is_empty());
        assert!(recovery.is_none());
    }
}
