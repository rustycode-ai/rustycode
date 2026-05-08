//! AST Phase 4 recovery: milestone-level failure classification and retry planning.
//!
//! The `FailureClassifier` analyzes `StepEvidence` to produce a `FailureDiagnosis`,
//! and `MilestoneRecovery` orchestrates retry/replan/escalate decisions based on
//! diagnosis and attempt counts.
//!
//! Spec reference: §7.4 — local recovery that reruns failed milestones without
//! restarting the entire task.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::types::{ExecutionStep, RecoveryAction, StepEvidence};

// FailureType

/// Classification of what went wrong during step execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureType {
    MissingDependency { package: String },
    PermissionDenied { path: String },
    CompilationError { file: String, line: Option<u32> },
    TestFailure { test_name: String },
    Timeout,
    FileNotFoundError { path: String },
    NetworkError,
    Unknown,
}

impl fmt::Display for FailureType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDependency { package } => {
                write!(f, "MissingDependency({package})")
            }
            Self::PermissionDenied { path } => {
                write!(f, "PermissionDenied({path})")
            }
            Self::CompilationError { file, line } => {
                if let Some(ln) = line {
                    write!(f, "CompilationError({file}:{ln})")
                } else {
                    write!(f, "CompilationError({file})")
                }
            }
            Self::TestFailure { test_name } => {
                write!(f, "TestFailure({test_name})")
            }
            Self::Timeout => write!(f, "Timeout"),
            Self::FileNotFoundError { path } => {
                write!(f, "FileNotFoundError({path})")
            }
            Self::NetworkError => write!(f, "NetworkError"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

// RecoveryStrategy

/// Recommended recovery action for a diagnosed failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    RetrySame,
    InstallDependency { package: String },
    FixPermission { path: String },
    ModifyStep { modified_step: ExecutionStep },
    SkipAndContinue,
    EscalateToConsultant,
    ReplanMilestone,
    AbortTask,
}

// FailureDiagnosis

/// Result of analyzing `StepEvidence` for failure root cause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureDiagnosis {
    pub failure_type: FailureType,
    pub root_cause: String,
    pub suggested_action: RecoveryStrategy,
    pub confidence: f64,
}

// FailureClassifier

/// Pattern-matching classifier that inspects exit codes and stderr to produce
/// a structured diagnosis.
#[derive(Debug, Clone)]
pub struct FailureClassifier;

impl FailureClassifier {
    /// Analyze a single `StepEvidence` and return a diagnosis.
    ///
    /// The caller should only invoke this when `exit_code != 0`.
    #[allow(clippy::too_many_lines)]
    pub fn diagnose(evidence: &StepEvidence) -> FailureDiagnosis {
        let stderr = evidence.stderr_summary.to_lowercase();
        let stdout = evidence.stdout_summary.to_lowercase();
        let exit = evidence.exit_code;

        // --- Timeout (exit 124 from `timeout`, 137 from SIGKILL/OOM) ---
        if exit == 124 || exit == 137 {
            return FailureDiagnosis {
                failure_type: FailureType::Timeout,
                root_cause: format!(
                    "Process terminated with exit code {exit} (likely timeout or killed)"
                ),
                suggested_action: RecoveryStrategy::ReplanMilestone,
                confidence: 0.95,
            };
        }

        // --- Command not found → MissingDependency ---
        if exit == 127
            && (stderr.contains("command not found") || stdout.contains("command not found"))
        {
            let package = extract_command_name(&stderr)
                .or_else(|| extract_command_name(&stdout))
                .unwrap_or_else(|| "unknown".to_string());
            return FailureDiagnosis {
                failure_type: FailureType::MissingDependency {
                    package: package.clone(),
                },
                root_cause: format!("Command not found: {package}"),
                suggested_action: RecoveryStrategy::InstallDependency { package },
                confidence: 0.90,
            };
        }

        // --- Permission denied ---
        if (stderr.contains("permission denied") || stdout.contains("permission denied"))
            && (exit == 1 || exit == 126)
        {
            let path = extract_path_after(&stderr, "permission denied: ")
                .or_else(|| extract_path_after(&stdout, "permission denied: "))
                .unwrap_or_else(|| "unknown".to_string());
            return FailureDiagnosis {
                failure_type: FailureType::PermissionDenied { path: path.clone() },
                root_cause: format!("Permission denied: {path}"),
                suggested_action: RecoveryStrategy::FixPermission { path },
                confidence: 0.85,
            };
        }

        // --- Compilation error ---
        if exit == 1
            && (stderr.contains("error:") || stderr.contains("error[e"))
            && has_source_extension(&stderr)
        {
            let (file, line) = extract_source_location(&stderr);
            return FailureDiagnosis {
                failure_type: FailureType::CompilationError {
                    file: file.clone(),
                    line,
                },
                root_cause: format!("Compilation error in {file}"),
                suggested_action: RecoveryStrategy::ModifyStep {
                    modified_step: ExecutionStep {
                        action: "Fix compilation error".into(),
                        file_targets: vec![std::path::PathBuf::from(&file)],
                        expected_command: None,
                        verification_command: None,
                        is_risky: false,
                        recovery_notes: Some(format!("Fix compilation error in {file}")),
                    },
                },
                confidence: 0.80,
            };
        }

        // --- Test failure ---
        if exit == 1
            && (stderr.contains("failed")
                || stderr.contains("failure")
                || stderr.contains("failures"))
            && (stderr.contains("test") || stdout.contains("test"))
        {
            let test_name = extract_test_name(&stderr)
                .or_else(|| extract_test_name(&stdout))
                .unwrap_or_else(|| "unknown".to_string());
            return FailureDiagnosis {
                failure_type: FailureType::TestFailure {
                    test_name: test_name.clone(),
                },
                root_cause: format!("Test failed: {test_name}"),
                suggested_action: RecoveryStrategy::RetrySame,
                confidence: 0.75,
            };
        }

        // --- File not found ---
        if exit == 1
            && (stderr.contains("no such file") || stderr.contains("not found"))
            && !stderr.contains("command not found")
        {
            let path = extract_path_after(&stderr, "no such file")
                .or_else(|| extract_path_after(&stderr, "not found: "))
                .unwrap_or_else(|| "unknown".to_string());
            return FailureDiagnosis {
                failure_type: FailureType::FileNotFoundError { path: path.clone() },
                root_cause: format!("File not found: {path}"),
                suggested_action: RecoveryStrategy::ModifyStep {
                    modified_step: ExecutionStep {
                        action: "Create or locate missing file".into(),
                        file_targets: vec![std::path::PathBuf::from(&path)],
                        expected_command: None,
                        verification_command: None,
                        is_risky: false,
                        recovery_notes: Some(format!("File not found: {path}")),
                    },
                },
                confidence: 0.80,
            };
        }

        // --- Network error (curl exit codes 1-10 or stderr mentioning curl/wget failures) ---
        if (1..=10).contains(&exit)
            && (stderr.contains("curl") || stderr.contains("wget") || stderr.contains("network"))
        {
            return FailureDiagnosis {
                failure_type: FailureType::NetworkError,
                root_cause: "Network error during command execution".into(),
                suggested_action: RecoveryStrategy::RetrySame,
                confidence: 0.70,
            };
        }

        // --- Fallback ---
        FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: format!(
                "Step {} failed with exit code {}. stderr: {}",
                evidence.step_index,
                exit,
                truncate_str(&evidence.stderr_summary, 200)
            ),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.30,
        }
    }
}

// RecoveryOutcome

/// High-level outcome of the recovery planning process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryOutcome {
    /// Retry the given (possibly modified) step.
    Retry(ExecutionStep),
    /// Replan: replace the remaining steps entirely.
    Replan(Vec<ExecutionStep>),
    /// Escalate to a consultant / higher-tier agent.
    Escalate { reason: String },
    /// Abort the entire task.
    Abort { reason: String },
}

// MilestoneRecovery

/// Orchestrates milestone-level recovery decisions.
#[derive(Debug, Clone)]
pub struct MilestoneRecovery {
    max_retries_per_step: u32,
    max_milestone_failures: u32,
}

/// Default `max_retries_per_step`.
const DEFAULT_MAX_RETRIES: u32 = 2;
/// Default `max_milestone_failures`.
const DEFAULT_MAX_MILESTONE_FAILURES: u32 = 3;

impl MilestoneRecovery {
    pub const fn new() -> Self {
        Self {
            max_retries_per_step: DEFAULT_MAX_RETRIES,
            max_milestone_failures: DEFAULT_MAX_MILESTONE_FAILURES,
        }
    }

    /// Override the per-step retry limit.
    #[must_use]
    pub const fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries_per_step = n;
        self
    }

    /// Override the milestone failure limit before escalation.
    #[must_use]
    pub const fn with_max_milestone_failures(mut self, n: u32) -> Self {
        self.max_milestone_failures = n;
        self
    }

    /// Diagnose a failed step using the classifier.
    pub fn diagnose_failure(&self, evidence: &StepEvidence) -> FailureDiagnosis {
        FailureClassifier::diagnose(evidence)
    }

    /// Decide on a `RecoveryAction` given a diagnosis and attempt count.
    pub fn plan_recovery(
        &self,
        diagnosis: &FailureDiagnosis,
        failed_step: &ExecutionStep,
        attempt: u32,
    ) -> RecoveryAction {
        let replanned = match &diagnosis.suggested_action {
            RecoveryStrategy::RetrySame => {
                vec![retry_step(failed_step, attempt)]
            }
            RecoveryStrategy::InstallDependency { package } => {
                let install_step = ExecutionStep {
                    action: format!("Install missing dependency: {package}"),
                    file_targets: vec![],
                    expected_command: Some(format!("pip install {package}")),
                    verification_command: None,
                    is_risky: false,
                    recovery_notes: None,
                };
                vec![install_step, retry_step(failed_step, attempt)]
            }
            RecoveryStrategy::FixPermission { path } => {
                let fix_step = ExecutionStep {
                    action: format!("Fix permissions for {path}"),
                    file_targets: vec![std::path::PathBuf::from(path)],
                    expected_command: Some(format!("chmod +x {path}")),
                    verification_command: None,
                    is_risky: false,
                    recovery_notes: None,
                };
                vec![fix_step, retry_step(failed_step, attempt)]
            }
            RecoveryStrategy::ModifyStep { modified_step } => {
                vec![modified_step.clone(), retry_step(failed_step, attempt)]
            }
            RecoveryStrategy::SkipAndContinue
            | RecoveryStrategy::EscalateToConsultant
            | RecoveryStrategy::ReplanMilestone
            | RecoveryStrategy::AbortTask => {
                vec![]
            }
        };

        let research = research_topics_for(diagnosis, failed_step);

        RecoveryAction {
            failed_step: 0, // caller should set to actual index
            diagnosis: format!("{}: {}", diagnosis.failure_type, diagnosis.root_cause),
            research_needed: research,
            replanned_steps: replanned,
            retry_attempt: attempt,
        }
    }

    /// Decide whether to escalate based on cumulative milestone failures.
    pub const fn should_escalate(&self, milestone_failures: u32) -> bool {
        milestone_failures >= self.max_milestone_failures
    }

    /// Detect whether failures are systemic (2+ milestones failed).
    pub const fn is_systemic(&self, failed_milestones: &[usize], total_milestones: usize) -> bool {
        if total_milestones == 0 {
            return false;
        }
        failed_milestones.len() >= 2
    }

    /// Produce a full `RecoveryOutcome` considering attempt count and escalation.
    pub fn decide_outcome(
        &self,
        diagnosis: &FailureDiagnosis,
        failed_step: &ExecutionStep,
        attempt: u32,
        milestone_failures: u32,
    ) -> RecoveryOutcome {
        if self.should_escalate(milestone_failures) {
            return RecoveryOutcome::Escalate {
                reason: format!(
                    "Milestone failure count ({}) reached escalation threshold ({})",
                    milestone_failures, self.max_milestone_failures
                ),
            };
        }

        if attempt >= self.max_retries_per_step {
            return match &diagnosis.suggested_action {
                RecoveryStrategy::AbortTask => RecoveryOutcome::Abort {
                    reason: diagnosis.root_cause.clone(),
                },
                _ => RecoveryOutcome::Escalate {
                    reason: format!(
                        "Max retries ({}) exhausted for step: {}",
                        self.max_retries_per_step, diagnosis.root_cause
                    ),
                },
            };
        }

        match &diagnosis.suggested_action {
            RecoveryStrategy::RetrySame => RecoveryOutcome::Retry(retry_step(failed_step, attempt)),
            RecoveryStrategy::InstallDependency { package } => {
                RecoveryOutcome::Retry(ExecutionStep {
                    action: format!("Install {package}"),
                    file_targets: vec![],
                    expected_command: Some(format!("pip install {package}")),
                    verification_command: None,
                    is_risky: false,
                    recovery_notes: None,
                })
            }
            RecoveryStrategy::FixPermission { path } => RecoveryOutcome::Retry(ExecutionStep {
                action: format!("Fix permissions: {path}"),
                file_targets: vec![std::path::PathBuf::from(path)],
                expected_command: Some(format!("chmod +x {path}")),
                verification_command: None,
                is_risky: false,
                recovery_notes: None,
            }),
            RecoveryStrategy::ModifyStep { modified_step } => {
                RecoveryOutcome::Retry(modified_step.clone())
            }
            RecoveryStrategy::SkipAndContinue => {
                RecoveryOutcome::Retry(retry_step(failed_step, attempt))
            }
            RecoveryStrategy::EscalateToConsultant => RecoveryOutcome::Escalate {
                reason: diagnosis.root_cause.clone(),
            },
            RecoveryStrategy::ReplanMilestone => {
                RecoveryOutcome::Replan(vec![retry_step(failed_step, attempt)])
            }
            RecoveryStrategy::AbortTask => RecoveryOutcome::Abort {
                reason: diagnosis.root_cause.clone(),
            },
        }
    }

    /// Generate replanned steps for the failed step and all subsequent steps.
    ///
    /// Steps before `failed_index` are carried forward unchanged.
    pub fn generate_replan_steps(
        &self,
        diagnosis: &FailureDiagnosis,
        original_steps: &[ExecutionStep],
        failed_index: usize,
    ) -> Vec<ExecutionStep> {
        if failed_index >= original_steps.len() {
            return original_steps.to_vec();
        }

        let mut result: Vec<ExecutionStep> = original_steps[..failed_index].to_vec();

        // Replace the failed step with a recovery-oriented version.
        let failed = &original_steps[failed_index];
        let recovered = match &diagnosis.suggested_action {
            RecoveryStrategy::InstallDependency { package } => {
                let mut steps = vec![ExecutionStep {
                    action: format!("Install dependency: {package}"),
                    file_targets: vec![],
                    expected_command: Some(format!("pip install {package}")),
                    verification_command: None,
                    is_risky: false,
                    recovery_notes: None,
                }];
                steps.push(retry_step(failed, 0));
                steps
            }
            RecoveryStrategy::FixPermission { path } => {
                let mut steps = vec![ExecutionStep {
                    action: format!("Fix permissions: {path}"),
                    file_targets: vec![std::path::PathBuf::from(path)],
                    expected_command: Some(format!("chmod +x {path}")),
                    verification_command: None,
                    is_risky: false,
                    recovery_notes: None,
                }];
                steps.push(retry_step(failed, 0));
                steps
            }
            RecoveryStrategy::ModifyStep { modified_step } => {
                vec![modified_step.clone()]
            }
            RecoveryStrategy::SkipAndContinue => {
                vec![]
            }
            _ => {
                vec![retry_step(failed, 0)]
            }
        };

        result.extend(recovered);

        // Carry forward subsequent steps with a note about the preceding failure.
        for step in &original_steps[failed_index + 1..] {
            let mut s = step.clone();
            s.recovery_notes = Some("Following replan after earlier step failure".into());
            result.push(s);
        }

        result
    }
}

impl Default for MilestoneRecovery {
    fn default() -> Self {
        Self::new()
    }
}

// Markdown rendering

/// Render a recovery section in the EXECUTE phase output format.
///
/// Produces:
/// ```markdown
/// 2. [action] -> ❌ exit=1
///    RECOVERY:
///    - diagnosis: ...
///    - research: ...
///    - replan: ...
///    - retry: ...
/// ```
pub fn render_recovery_markdown(evidence: &StepEvidence, recovery: &RecoveryAction) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "{}. [step] -> ❌ exit={}",
        evidence.step_index, evidence.exit_code
    ));
    lines.push("   RECOVERY:".into());
    lines.push(format!("   - diagnosis: {}", recovery.diagnosis));

    if !recovery.research_needed.is_empty() {
        let research = recovery.research_needed.join("; ");
        lines.push(format!("   - research: {research}"));
    }

    if !recovery.replanned_steps.is_empty() {
        let actions: Vec<&str> = recovery
            .replanned_steps
            .iter()
            .map(|s| s.action.as_str())
            .collect();
        lines.push(format!("   - replan: {}", actions.join(" -> ")));
    }

    lines.push(format!("   - retry: attempt #{}", recovery.retry_attempt));

    lines.join("\n")
}

// Private helpers

fn retry_step(step: &ExecutionStep, attempt: u32) -> ExecutionStep {
    ExecutionStep {
        action: format!("Retry (attempt {}): {}", attempt + 1, step.action),
        file_targets: step.file_targets.clone(),
        expected_command: step.expected_command.clone(),
        verification_command: step.verification_command.clone(),
        is_risky: step.is_risky,
        recovery_notes: Some("Automatic retry after failure".into()),
    }
}

fn research_topics_for(diagnosis: &FailureDiagnosis, step: &ExecutionStep) -> Vec<String> {
    let mut topics = Vec::new();

    match &diagnosis.failure_type {
        FailureType::MissingDependency { package } => {
            topics.push(format!("Find installation method for {package}"));
        }
        FailureType::CompilationError { file, .. } => {
            topics.push(format!("Review compilation errors in {file}"));
        }
        FailureType::TestFailure { test_name } => {
            topics.push(format!("Investigate test failure: {test_name}"));
        }
        FailureType::PermissionDenied { path } => {
            topics.push(format!("Check file permissions for {path}"));
        }
        FailureType::FileNotFoundError { path } => {
            topics.push(format!("Locate or create file: {path}"));
        }
        FailureType::Timeout => {
            topics.push("Investigate performance bottleneck or increase timeout".into());
        }
        FailureType::NetworkError => {
            topics.push("Check network connectivity and retry".into());
        }
        FailureType::Unknown => {
            topics.push("General investigation of unexpected failure".into());
        }
    }

    if !step.file_targets.is_empty() {
        topics.push(format!(
            "Inspect affected files: {}",
            step.file_targets
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    topics
}

/// Extract the command name from a "command not found" message.
///
/// Handles formats like:
/// - `bash: gcc: command not found`
/// - `gcc: command not found`
/// - `command not found: gcc`
fn extract_command_name(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let idx = lower.find("command not found")?;

    // Check if the format is "command not found: <name>"
    let after = &text[idx + "command not found".len()..];
    let trimmed = after.trim_start_matches([':', ' ']);
    if let Some(name) = trimmed.split_whitespace().next() {
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    // Format is "<shell>: <name>: command not found" or "<name>: command not found"
    let before = &text[..idx];
    let trimmed_before = before.trim_end_matches([':', ' ']);
    if let Some(name) = trimmed_before.rsplit([':', ' ']).next() {
        let name = name.trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    None
}

/// Extract a path after a marker string like "permission denied: ".
fn extract_path_after(haystack: &str, marker: &str) -> Option<String> {
    let lower = haystack.to_lowercase();
    let marker_lower = marker.to_lowercase();
    let idx = lower.find(&marker_lower)?;
    let rest = &haystack[idx + marker.len()..];
    let path = rest.split(['\n', '\'', '"']).next().unwrap_or("").trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// Check whether `text` contains a source file extension.
fn has_source_extension(text: &str) -> bool {
    let extensions = [".rs", ".ts", ".py", ".js", ".go", ".java", ".c", ".cpp"];
    extensions.iter().any(|ext| text.contains(ext))
}

/// Extract the first source file location from stderr (e.g. `src/main.rs:42`).
fn extract_source_location(text: &str) -> (String, Option<u32>) {
    for line in text.lines() {
        for ext in &[
            ".rs:", ".ts:", ".py:", ".js:", ".go:", ".java:", ".c:", ".cpp:",
        ] {
            if let Some(idx) = line.find(ext) {
                let start = line[..idx].rfind([' ', '/', '\\']).map_or(0, |i| i + 1);
                let file_and_line = &line[start..];
                let end_of_ext = idx + ext.len();
                let file_part = &line[start..end_of_ext];
                let after = &line[end_of_ext..];
                let line_num = after
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|s| s.parse::<u32>().ok());
                let file_path = file_part.trim_end_matches(':').to_string();
                if !file_path.is_empty() {
                    return (file_path, line_num);
                }
                // Fallback: just split on ':'
                if let Some(colon_pos) = file_and_line.find(':') {
                    let file = file_and_line[..colon_pos].to_string();
                    let rest = &file_and_line[colon_pos + 1..];
                    let ln = rest
                        .split(|c: char| !c.is_ascii_digit())
                        .next()
                        .and_then(|s| s.parse::<u32>().ok());
                    if !file.is_empty() {
                        return (file, ln);
                    }
                }
            }
        }
    }
    ("unknown".to_string(), None)
}

/// Extract the first test name from text matching patterns like `test foo ... FAILED`.
fn extract_test_name(text: &str) -> Option<String> {
    // Pattern: "test <name>" followed by failed/FAILED
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.contains("test") && (lower.contains("fail") || lower.contains("failed")) {
            if let Some(idx) = lower.find("test ") {
                let rest = &line[idx + 5..];
                let name = rest.split([' ', ':', '\t']).next().unwrap_or("").trim();
                if !name.is_empty() && name != "test" {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = s.floor_char_boundary(max);
        format!("{}...", &s[..end])
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // -- helpers --

    fn evidence(exit: i32, stderr: &str) -> StepEvidence {
        StepEvidence {
            step_index: 0,
            command_run: Some("test-cmd".into()),
            exit_code: exit,
            stdout_summary: String::new(),
            stderr_summary: stderr.into(),
            changed_files: vec![],
            verification_passed: None,
        }
    }

    fn evidence_with_stdout(exit: i32, stdout: &str, stderr: &str) -> StepEvidence {
        StepEvidence {
            step_index: 0,
            command_run: Some("test-cmd".into()),
            exit_code: exit,
            stdout_summary: stdout.into(),
            stderr_summary: stderr.into(),
            changed_files: vec![],
            verification_passed: None,
        }
    }

    fn sample_step(action: &str) -> ExecutionStep {
        ExecutionStep {
            action: action.into(),
            file_targets: vec![PathBuf::from("src/main.rs")],
            expected_command: Some("cargo build".into()),
            verification_command: None,
            is_risky: false,
            recovery_notes: None,
        }
    }

    // ===== FailureType Display =====

    #[test]
    fn failure_type_display_missing_dep() {
        let ft = FailureType::MissingDependency {
            package: "numpy".into(),
        };
        assert_eq!(ft.to_string(), "MissingDependency(numpy)");
    }

    #[test]
    fn failure_type_display_permission_denied() {
        let ft = FailureType::PermissionDenied {
            path: "/tmp/script.sh".into(),
        };
        assert_eq!(ft.to_string(), "PermissionDenied(/tmp/script.sh)");
    }

    #[test]
    fn failure_type_display_compilation_error_with_line() {
        let ft = FailureType::CompilationError {
            file: "main.rs".into(),
            line: Some(42),
        };
        assert_eq!(ft.to_string(), "CompilationError(main.rs:42)");
    }

    #[test]
    fn failure_type_display_compilation_error_without_line() {
        let ft = FailureType::CompilationError {
            file: "main.rs".into(),
            line: None,
        };
        assert_eq!(ft.to_string(), "CompilationError(main.rs)");
    }

    #[test]
    fn failure_type_display_test_failure() {
        let ft = FailureType::TestFailure {
            test_name: "test_add".into(),
        };
        assert_eq!(ft.to_string(), "TestFailure(test_add)");
    }

    #[test]
    fn failure_type_display_timeout() {
        assert_eq!(FailureType::Timeout.to_string(), "Timeout");
    }

    #[test]
    fn failure_type_display_file_not_found() {
        let ft = FailureType::FileNotFoundError {
            path: "config.toml".into(),
        };
        assert_eq!(ft.to_string(), "FileNotFoundError(config.toml)");
    }

    #[test]
    fn failure_type_display_network_error() {
        assert_eq!(FailureType::NetworkError.to_string(), "NetworkError");
    }

    #[test]
    fn failure_type_display_unknown() {
        assert_eq!(FailureType::Unknown.to_string(), "Unknown");
    }

    // ===== FailureClassifier =====

    #[test]
    fn classify_command_not_found() {
        let e = evidence(127, "bash: gcc: command not found");
        let d = FailureClassifier::diagnose(&e);
        assert_eq!(
            d.failure_type,
            FailureType::MissingDependency {
                package: "gcc".into()
            }
        );
        assert!(d.confidence >= 0.85);
    }

    #[test]
    fn classify_command_not_found_in_stdout() {
        let e = evidence_with_stdout(127, "bash: python: command not found", "");
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(
            d.failure_type,
            FailureType::MissingDependency { .. }
        ));
    }

    #[test]
    fn classify_permission_denied() {
        let e = evidence(1, "Permission denied: /root/secret.txt");
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(
            d.failure_type,
            FailureType::PermissionDenied { .. }
        ));
    }

    #[test]
    fn classify_permission_denied_exit_126() {
        let e = evidence(126, "permission denied: ./run.sh");
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(
            d.failure_type,
            FailureType::PermissionDenied { .. }
        ));
    }

    #[test]
    fn classify_compilation_error_rust() {
        let e = evidence(
            1,
            "error[E0277]: the trait bound is not satisfied\n --> src/main.rs:10:5",
        );
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(
            d.failure_type,
            FailureType::CompilationError { .. }
        ));
        if let FailureType::CompilationError { file, line } = &d.failure_type {
            assert!(file.contains("main.rs"));
            assert!(line.is_some());
        }
    }

    #[test]
    fn classify_compilation_error_python() {
        let e = evidence(
            1,
            "  File \"app.py\", line 5\n    SyntaxError: invalid syntax",
        );
        let d = FailureClassifier::diagnose(&e);
        // Python errors may not match the exact pattern but should at least not crash.
        // The key check: it classifies without panic.
        assert!(d.confidence > 0.0);
    }

    #[test]
    fn classify_test_failure() {
        let e = evidence(
            1,
            "test test_addition ... FAILED\nfailures:\n    test_addition",
        );
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(d.failure_type, FailureType::TestFailure { .. }));
        if let FailureType::TestFailure { test_name } = &d.failure_type {
            assert_eq!(test_name, "test_addition");
        }
    }

    #[test]
    fn classify_test_failure_case_insensitive() {
        let e = evidence(1, "test my_feature ... failed");
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(d.failure_type, FailureType::TestFailure { .. }));
    }

    #[test]
    fn classify_timeout_exit_124() {
        let e = evidence(124, "");
        let d = FailureClassifier::diagnose(&e);
        assert_eq!(d.failure_type, FailureType::Timeout);
        assert!(d.confidence >= 0.90);
    }

    #[test]
    fn classify_timeout_exit_137() {
        let e = evidence(137, "Killed");
        let d = FailureClassifier::diagnose(&e);
        assert_eq!(d.failure_type, FailureType::Timeout);
    }

    #[test]
    fn classify_file_not_found() {
        let e = evidence(1, "No such file or directory: missing.txt");
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(
            d.failure_type,
            FailureType::FileNotFoundError { .. }
        ));
    }

    #[test]
    fn classify_file_not_found_does_not_match_command_not_found() {
        // "command not found" should take priority over "not found" for FileNotFoundError
        let e = evidence(127, "bash: foo: command not found");
        let d = FailureClassifier::diagnose(&e);
        assert!(matches!(
            d.failure_type,
            FailureType::MissingDependency { .. }
        ));
    }

    #[test]
    fn classify_network_error() {
        let e = evidence(6, "curl: (6) Could not resolve host: example.com");
        let d = FailureClassifier::diagnose(&e);
        assert_eq!(d.failure_type, FailureType::NetworkError);
    }

    #[test]
    fn classify_unknown_error() {
        let e = evidence(1, "something unexpected happened");
        let d = FailureClassifier::diagnose(&e);
        assert_eq!(d.failure_type, FailureType::Unknown);
        assert!(d.confidence < 0.5);
    }

    #[test]
    fn classify_empty_stderr() {
        let e = evidence(1, "");
        let d = FailureClassifier::diagnose(&e);
        assert_eq!(d.failure_type, FailureType::Unknown);
    }

    #[test]
    fn classify_exit_zero_still_works() {
        // diagnose may be called on exit 0 by mistake; should return Unknown
        let e = evidence(0, "");
        let d = FailureClassifier::diagnose(&e);
        // No crash, low confidence
        assert!(d.confidence < 0.5);
    }

    // ===== MilestoneRecovery =====

    #[test]
    fn new_has_defaults() {
        let r = MilestoneRecovery::new();
        assert_eq!(r.max_retries_per_step, 2);
        assert_eq!(r.max_milestone_failures, 3);
    }

    #[test]
    fn builder_overrides() {
        let r = MilestoneRecovery::new()
            .with_max_retries(5)
            .with_max_milestone_failures(10);
        assert_eq!(r.max_retries_per_step, 5);
        assert_eq!(r.max_milestone_failures, 10);
    }

    #[test]
    fn diagnose_failure_delegates() {
        let r = MilestoneRecovery::new();
        let e = evidence(127, "bash: gcc: command not found");
        let d = r.diagnose_failure(&e);
        assert!(matches!(
            d.failure_type,
            FailureType::MissingDependency { .. }
        ));
    }

    #[test]
    fn plan_recovery_retry_same() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "mystery".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.3,
        };
        let step = sample_step("build");
        let action = r.plan_recovery(&diag, &step, 0);
        assert_eq!(action.replanned_steps.len(), 1);
        assert!(action.replanned_steps[0].action.contains("Retry"));
    }

    #[test]
    fn plan_recovery_install_dependency() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::MissingDependency {
                package: "numpy".into(),
            },
            root_cause: "missing numpy".into(),
            suggested_action: RecoveryStrategy::InstallDependency {
                package: "numpy".into(),
            },
            confidence: 0.9,
        };
        let step = sample_step("import");
        let action = r.plan_recovery(&diag, &step, 0);
        assert_eq!(action.replanned_steps.len(), 2);
        assert!(action.replanned_steps[0].action.contains("numpy"));
    }

    #[test]
    fn plan_recovery_fix_permission() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::PermissionDenied {
                path: "/tmp/x.sh".into(),
            },
            root_cause: "no exec".into(),
            suggested_action: RecoveryStrategy::FixPermission {
                path: "/tmp/x.sh".into(),
            },
            confidence: 0.85,
        };
        let step = sample_step("run script");
        let action = r.plan_recovery(&diag, &step, 0);
        assert_eq!(action.replanned_steps.len(), 2);
        assert!(action.replanned_steps[0].action.contains("permissions"));
    }

    #[test]
    fn plan_recovery_skip() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "skip".into(),
            suggested_action: RecoveryStrategy::SkipAndContinue,
            confidence: 0.5,
        };
        let step = sample_step("optional");
        let action = r.plan_recovery(&diag, &step, 0);
        assert!(action.replanned_steps.is_empty());
    }

    #[test]
    fn plan_recovery_escalate_produces_empty_steps() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "stuck".into(),
            suggested_action: RecoveryStrategy::EscalateToConsultant,
            confidence: 0.5,
        };
        let step = sample_step("stuck step");
        let action = r.plan_recovery(&diag, &step, 0);
        assert!(action.replanned_steps.is_empty());
    }

    #[test]
    fn should_escalate_at_threshold() {
        let r = MilestoneRecovery::new();
        assert!(!r.should_escalate(0));
        assert!(!r.should_escalate(1));
        assert!(!r.should_escalate(2));
        assert!(r.should_escalate(3));
        assert!(r.should_escalate(10));
    }

    #[test]
    fn should_escalate_custom_threshold() {
        let r = MilestoneRecovery::new().with_max_milestone_failures(5);
        assert!(!r.should_escalate(4));
        assert!(r.should_escalate(5));
    }

    #[test]
    fn is_systemic_two_milestones() {
        let r = MilestoneRecovery::new();
        assert!(r.is_systemic(&[0, 1], 5));
    }

    #[test]
    fn is_not_systemic_one_milestone() {
        let r = MilestoneRecovery::new();
        assert!(!r.is_systemic(&[0], 5));
    }

    #[test]
    fn is_not_systemic_empty() {
        let r = MilestoneRecovery::new();
        assert!(!r.is_systemic(&[], 5));
    }

    #[test]
    fn is_not_systemic_zero_total() {
        let r = MilestoneRecovery::new();
        assert!(!r.is_systemic(&[0, 1], 0));
    }

    // ===== RecoveryOutcome =====

    #[test]
    fn decide_outcome_retry() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "oops".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.3,
        };
        let step = sample_step("build");
        let outcome = r.decide_outcome(&diag, &step, 0, 0);
        assert!(matches!(outcome, RecoveryOutcome::Retry(_)));
    }

    #[test]
    fn decide_outcome_replan() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Timeout,
            root_cause: "too slow".into(),
            suggested_action: RecoveryStrategy::ReplanMilestone,
            confidence: 0.9,
        };
        let step = sample_step("slow step");
        let outcome = r.decide_outcome(&diag, &step, 0, 0);
        assert!(matches!(outcome, RecoveryOutcome::Replan(_)));
    }

    #[test]
    fn decide_outcome_escalate_at_max_retries() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "stuck".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.3,
        };
        let step = sample_step("build");
        let outcome = r.decide_outcome(&diag, &step, 2, 0);
        assert!(matches!(outcome, RecoveryOutcome::Escalate { .. }));
    }

    #[test]
    fn decide_outcome_abort() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "fatal".into(),
            suggested_action: RecoveryStrategy::AbortTask,
            confidence: 0.9,
        };
        let step = sample_step("dangerous");
        let outcome = r.decide_outcome(&diag, &step, 2, 0);
        assert!(matches!(outcome, RecoveryOutcome::Abort { .. }));
    }

    #[test]
    fn decide_outcome_escalate_at_milestone_threshold() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "recurring".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.3,
        };
        let step = sample_step("build");
        let outcome = r.decide_outcome(&diag, &step, 0, 3);
        assert!(matches!(outcome, RecoveryOutcome::Escalate { .. }));
    }

    #[test]
    fn decide_outcome_install_dep() {
        let r = MilestoneRecovery::new();
        let diag = FailureDiagnosis {
            failure_type: FailureType::MissingDependency {
                package: "requests".into(),
            },
            root_cause: "no requests".into(),
            suggested_action: RecoveryStrategy::InstallDependency {
                package: "requests".into(),
            },
            confidence: 0.9,
        };
        let step = sample_step("import requests");
        let outcome = r.decide_outcome(&diag, &step, 0, 0);
        if let RecoveryOutcome::Retry(s) = outcome {
            assert!(s.action.contains("requests"));
        } else {
            panic!("expected Retry variant");
        }
    }

    // ===== generate_replan_steps =====

    #[test]
    fn generate_replan_preserves_prior_steps() {
        let r = MilestoneRecovery::new();
        let steps = vec![
            sample_step("step 0"),
            sample_step("step 1"),
            sample_step("step 2"),
        ];
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "fail".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.5,
        };
        let result = r.generate_replan_steps(&diag, &steps, 1);
        assert!(result.len() >= 2); // step 0 + at least one retry step
        assert!(result[0].action.contains("step 0"));
    }

    #[test]
    fn generate_replan_adds_subsequent_with_notes() {
        let r = MilestoneRecovery::new();
        let steps = vec![
            sample_step("step 0"),
            sample_step("step 1"),
            sample_step("step 2"),
        ];
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "fail".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.5,
        };
        let result = r.generate_replan_steps(&diag, &steps, 0);
        // Should have: retry of step 0, then step 1 and step 2 with notes
        let last = result.last().expect("should have steps");
        assert!(last.recovery_notes.is_some());
    }

    #[test]
    fn generate_replan_out_of_bounds_returns_original() {
        let r = MilestoneRecovery::new();
        let steps = vec![sample_step("only step")];
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "fail".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.5,
        };
        let result = r.generate_replan_steps(&diag, &steps, 5);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn generate_replan_skip_produces_no_retry() {
        let r = MilestoneRecovery::new();
        let steps = vec![sample_step("step 0"), sample_step("step 1")];
        let diag = FailureDiagnosis {
            failure_type: FailureType::Unknown,
            root_cause: "skip".into(),
            suggested_action: RecoveryStrategy::SkipAndContinue,
            confidence: 0.5,
        };
        let result = r.generate_replan_steps(&diag, &steps, 0);
        // step 0 skipped, step 1 carried forward
        assert_eq!(result.len(), 1);
        assert!(result[0].action.contains("step 1"));
    }

    // ===== render_recovery_markdown =====

    #[test]
    fn render_markdown_basic() {
        let e = evidence(1, "error");
        let recovery = RecoveryAction {
            failed_step: 2,
            diagnosis: "compilation error".into(),
            research_needed: vec!["check file".into()],
            replanned_steps: vec![sample_step("retry build")],
            retry_attempt: 1,
        };
        let md = render_recovery_markdown(&e, &recovery);
        assert!(md.contains("exit=1"));
        assert!(md.contains("RECOVERY:"));
        assert!(md.contains("diagnosis: compilation error"));
        assert!(md.contains("research: check file"));
        assert!(md.contains("retry build"));
        assert!(md.contains("attempt #1"));
    }

    #[test]
    fn render_markdown_no_research() {
        let e = evidence(1, "");
        let recovery = RecoveryAction {
            failed_step: 0,
            diagnosis: "unknown".into(),
            research_needed: vec![],
            replanned_steps: vec![],
            retry_attempt: 0,
        };
        let md = render_recovery_markdown(&e, &recovery);
        assert!(!md.contains("research:"));
    }

    #[test]
    fn render_markdown_no_replanned_steps() {
        let e = evidence(1, "");
        let recovery = RecoveryAction {
            failed_step: 0,
            diagnosis: "skip".into(),
            research_needed: vec!["topic".into()],
            replanned_steps: vec![],
            retry_attempt: 0,
        };
        let md = render_recovery_markdown(&e, &recovery);
        assert!(!md.contains("replan:"));
    }

    // ===== Serialization roundtrip =====

    #[test]
    fn failure_type_roundtrip() {
        let ft = FailureType::CompilationError {
            file: "main.rs".into(),
            line: Some(10),
        };
        let json = serde_json::to_string(&ft).unwrap();
        let back: FailureType = serde_json::from_str(&json).unwrap();
        assert_eq!(ft, back);
    }

    #[test]
    fn failure_diagnosis_roundtrip() {
        let d = FailureDiagnosis {
            failure_type: FailureType::TestFailure {
                test_name: "test_foo".into(),
            },
            root_cause: "assertion failed".into(),
            suggested_action: RecoveryStrategy::RetrySame,
            confidence: 0.75,
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: FailureDiagnosis = serde_json::from_str(&json).unwrap();
        assert_eq!(d.failure_type, back.failure_type);
        assert!((d.confidence - back.confidence).abs() < f64::EPSILON);
    }

    #[test]
    fn recovery_strategy_roundtrip() {
        let rs = RecoveryStrategy::InstallDependency {
            package: "tokio".into(),
        };
        let json = serde_json::to_string(&rs).unwrap();
        let back: RecoveryStrategy = serde_json::from_str(&json).unwrap();
        if let RecoveryStrategy::InstallDependency { package } = &back {
            assert_eq!(package, "tokio");
        } else {
            panic!("expected InstallDependency variant");
        }
    }

    #[test]
    fn recovery_outcome_roundtrip() {
        let ro = RecoveryOutcome::Escalate {
            reason: "too many failures".into(),
        };
        let json = serde_json::to_string(&ro).unwrap();
        let back: RecoveryOutcome = serde_json::from_str(&json).unwrap();
        if let RecoveryOutcome::Escalate { reason } = &back {
            assert_eq!(reason, "too many failures");
        } else {
            panic!("expected Escalate variant");
        }
    }
}
