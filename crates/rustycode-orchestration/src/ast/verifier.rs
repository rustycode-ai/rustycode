//! AST Phase 4: VERIFY.
//!
//! The `Verifier` checks collected `StepEvidence` against the
//! `SuccessCriterion`s from the initial task assessment. Each criterion
//! produces a `CriterionResult` (Pass / Fail / Partial), and the overall
//! `VerificationReport` aggregates them.

use super::types::{
    CriterionResult, StepEvidence, SuccessCriterion, VerificationReport, VerificationStatus,
};

/// Verifies task outcomes against success criteria.
pub struct Verifier;

impl Verifier {
    pub const fn new() -> Self {
        Self
    }

    /// Check all criteria against collected evidence.
    ///
    /// * **Pass** -- if all criteria pass.
    /// * **Fail** -- if any criterion fails.
    /// * **Partial** -- if no failures exist but at least one criterion is
    ///   only partially satisfied (e.g. no verification command and evidence
    ///   is ambiguous).
    pub fn verify(
        &self,
        criteria: &[SuccessCriterion],
        evidence: &[StepEvidence],
    ) -> VerificationReport {
        let results: Vec<CriterionResult> = criteria
            .iter()
            .map(|criterion| self.verify_criterion(criterion, evidence))
            .collect();

        let overall = Self::aggregate_status(&results);

        VerificationReport { results, overall }
    }

    /// Evaluate a single criterion against all available evidence.
    fn verify_criterion(
        &self,
        criterion: &SuccessCriterion,
        evidence: &[StepEvidence],
    ) -> CriterionResult {
        criterion.verification_command.as_ref().map_or_else(
            || self.infer_from_evidence(criterion, evidence),
            |cmd| self.match_verification_command(criterion, cmd, evidence),
        )
    }

    /// Match a verification command from a criterion to evidence.
    ///
    /// Looks for an evidence item whose `command_run` matches the
    /// verification command, then checks its exit code.
    #[allow(clippy::unused_self)]
    fn match_verification_command(
        &self,
        criterion: &SuccessCriterion,
        expected_cmd: &str,
        evidence: &[StepEvidence],
    ) -> CriterionResult {
        // Find evidence whose command_run matches.
        let matching: Vec<&StepEvidence> = evidence
            .iter()
            .filter(|e| {
                e.command_run
                    .as_ref()
                    .is_some_and(|ran| commands_match(ran, expected_cmd))
            })
            .collect();

        if matching.is_empty() {
            // No evidence for this verification command.
            return CriterionResult {
                criterion: criterion.clone(),
                status: VerificationStatus::Fail,
                evidence: format!("No evidence found for verification command: {expected_cmd}"),
            };
        }

        // Check if any matching evidence explicitly passed verification.
        let any_passed = matching.iter().any(|e| e.verification_passed == Some(true));
        let all_zero_exit = matching.iter().all(|e| e.exit_code == 0);

        if any_passed || all_zero_exit {
            CriterionResult {
                criterion: criterion.clone(),
                status: VerificationStatus::Pass,
                evidence: format!(
                    "Verification command '{}' passed ({} evidence items)",
                    expected_cmd,
                    matching.len()
                ),
            }
        } else {
            let failed_codes: Vec<i32> = matching
                .iter()
                .filter(|e| e.exit_code != 0)
                .map(|e| e.exit_code)
                .collect();
            CriterionResult {
                criterion: criterion.clone(),
                status: VerificationStatus::Fail,
                evidence: format!(
                    "Verification command '{expected_cmd}' failed with exit codes: {failed_codes:?}"
                ),
            }
        }
    }

    /// Infer pass/fail when no explicit verification command exists.
    ///
    /// If any evidence shows a successful step (`exit_code` 0), mark as
    /// Partial since we cannot fully verify the criterion without a
    /// command. If no evidence at all, mark as Fail.
    #[allow(clippy::unused_self)]
    fn infer_from_evidence(
        &self,
        criterion: &SuccessCriterion,
        evidence: &[StepEvidence],
    ) -> CriterionResult {
        if evidence.is_empty() {
            return CriterionResult {
                criterion: criterion.clone(),
                status: VerificationStatus::Fail,
                evidence: "No evidence collected".into(),
            };
        }

        let any_success = evidence.iter().any(|e| e.exit_code == 0);
        let all_success = evidence.iter().all(|e| e.exit_code == 0);

        if all_success {
            CriterionResult {
                criterion: criterion.clone(),
                status: VerificationStatus::Pass,
                evidence: format!(
                    "No verification command; all {} steps exited 0",
                    evidence.len()
                ),
            }
        } else if any_success {
            CriterionResult {
                criterion: criterion.clone(),
                status: VerificationStatus::Partial,
                evidence: format!(
                    "No verification command; {}/{} steps exited 0 (inferred partial)",
                    evidence.iter().filter(|e| e.exit_code == 0).count(),
                    evidence.len()
                ),
            }
        } else {
            CriterionResult {
                criterion: criterion.clone(),
                status: VerificationStatus::Fail,
                evidence: format!(
                    "No verification command; all {} steps failed",
                    evidence.len()
                ),
            }
        }
    }

    /// Determine overall status from individual results.
    fn aggregate_status(results: &[CriterionResult]) -> VerificationStatus {
        if results.is_empty() {
            return VerificationStatus::Pass;
        }

        let any_fail = results.iter().any(|r| r.status == VerificationStatus::Fail);
        let all_pass = results.iter().all(|r| r.status == VerificationStatus::Pass);

        if any_fail {
            VerificationStatus::Fail
        } else if all_pass {
            VerificationStatus::Pass
        } else {
            VerificationStatus::Partial
        }
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Check whether two command strings are equivalent for matching purposes.
///
/// Normalizes whitespace and does a prefix/contains check to handle
/// cases like `cargo test` matching `cargo test --lib`.
fn commands_match(ran: &str, expected: &str) -> bool {
    let ran_normalized = normalize_command(ran);
    let expected_normalized = normalize_command(expected);

    // Exact match after normalization.
    if ran_normalized == expected_normalized {
        return true;
    }

    // Prefix match: `cargo test` matches `cargo test --lib`.
    if ran_normalized.starts_with(&expected_normalized) {
        return true;
    }

    // Expected is a prefix of ran.
    if expected_normalized.starts_with(&ran_normalized) {
        return true;
    }

    false
}

/// Normalize a command string for comparison.
fn normalize_command(cmd: &str) -> String {
    cmd.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn criterion(desc: &str, cmd: Option<&str>) -> SuccessCriterion {
        SuccessCriterion {
            description: desc.into(),
            verification_command: cmd.map(String::from),
        }
    }

    fn evidence(step_index: usize, exit_code: i32, command_run: Option<&str>) -> StepEvidence {
        StepEvidence {
            step_index,
            command_run: command_run.map(String::from),
            exit_code,
            stdout_summary: String::new(),
            stderr_summary: String::new(),
            changed_files: vec![],
            verification_passed: if exit_code == 0 { Some(true) } else { None },
        }
    }

    #[test]
    fn all_criteria_pass_with_matching_evidence() {
        let verifier = Verifier::new();
        let criteria = vec![
            criterion("Tests pass", Some("cargo test")),
            criterion("Build succeeds", Some("cargo build")),
        ];
        let evidence = vec![
            evidence(0, 0, Some("cargo test")),
            evidence(1, 0, Some("cargo build")),
        ];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Pass);
        assert!(report
            .results
            .iter()
            .all(|r| r.status == VerificationStatus::Pass));
    }

    #[test]
    fn fails_when_verification_command_has_no_evidence() {
        let verifier = Verifier::new();
        let criteria = vec![criterion("Tests pass", Some("cargo test"))];
        let evidence = vec![evidence(0, 0, Some("cargo build"))];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Fail);
        assert_eq!(report.results[0].status, VerificationStatus::Fail);
        assert!(report.results[0].evidence.contains("No evidence found"));
    }

    #[test]
    fn fails_when_evidence_shows_nonzero_exit() {
        let verifier = Verifier::new();
        let criteria = vec![criterion("Tests pass", Some("cargo test"))];
        let evidence = vec![evidence(0, 1, Some("cargo test"))];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Fail);
        assert!(report.results[0].evidence.contains("exit codes"));
    }

    #[test]
    fn no_verification_command_pass_on_all_success() {
        let verifier = Verifier::new();
        let criteria = vec![criterion("Task objective met", None)];
        let evidence = vec![evidence(0, 0, None)];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Pass);
        assert_eq!(report.results[0].status, VerificationStatus::Pass);
    }

    #[test]
    fn no_verification_command_no_evidence_fails() {
        let verifier = Verifier::new();
        let criteria = vec![criterion("Task done", None)];
        let evidence = vec![];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Fail);
    }

    #[test]
    fn no_verification_command_all_failure_evidence_fails() {
        let verifier = Verifier::new();
        let criteria = vec![criterion("Task done", None)];
        let evidence = vec![evidence(0, 1, None)];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Fail);
        assert!(report.results[0].evidence.contains("all 1 steps failed"));
    }

    #[test]
    fn mixed_pass_and_fail_overall_fails() {
        let verifier = Verifier::new();
        let criteria = vec![
            criterion("Build", Some("cargo build")),
            criterion("Tests", Some("cargo test")),
        ];
        let evidence = vec![
            evidence(0, 0, Some("cargo build")),
            evidence(1, 1, Some("cargo test")),
        ];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Fail);
    }

    #[test]
    fn prefix_matching_for_commands() {
        let verifier = Verifier::new();
        let criteria = vec![criterion("Tests", Some("cargo test"))];
        let evidence = vec![evidence(0, 0, Some("cargo test --lib"))];

        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Pass);
    }

    #[test]
    fn empty_criteria_produces_pass() {
        let verifier = Verifier::new();
        let report = verifier.verify(&[], &[]);

        assert_eq!(report.overall, VerificationStatus::Pass);
        assert!(report.results.is_empty());
    }

    #[test]
    fn commands_match_normalizes_whitespace() {
        assert!(commands_match("cargo  test", "cargo test"));
        assert!(commands_match("cargo test", "cargo  test"));
    }

    #[allow(clippy::no_effect_underscore_binding)]
    #[test]
    fn default_impl_matches_new() {
        let _default: Verifier = Verifier;
        let _new: Verifier = Verifier::new();
    }

    // -- US-004: edge-case tests --

    #[test]
    fn verifier_with_empty_evidence_returns_fail() {
        let verifier = Verifier::new();
        let criteria = vec![
            criterion("Tests pass", Some("cargo test")),
            criterion("Build succeeds", Some("cargo build")),
        ];
        let report = verifier.verify(&criteria, &[]);
        assert_eq!(report.overall, VerificationStatus::Fail);
        assert!(report
            .results
            .iter()
            .all(|r| r.status == VerificationStatus::Fail));
    }

    #[test]
    fn verifier_with_all_passing_evidence_returns_pass() {
        let verifier = Verifier::new();
        let criteria = vec![
            criterion("Build", Some("cargo build")),
            criterion("Test", Some("cargo test")),
            criterion("Lint", Some("cargo clippy")),
        ];
        let evidence = vec![
            evidence(0, 0, Some("cargo build")),
            evidence(1, 0, Some("cargo test")),
            evidence(2, 0, Some("cargo clippy")),
        ];
        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Pass);
        assert!(report
            .results
            .iter()
            .all(|r| r.status == VerificationStatus::Pass));
    }

    #[test]
    fn verifier_with_partial_evidence_fails() {
        let verifier = Verifier::new();
        let criteria = vec![
            criterion("Build", Some("cargo build")),
            criterion("Test", Some("cargo test")),
        ];
        // Only build evidence, no test evidence
        let evidence = vec![evidence(0, 0, Some("cargo build"))];
        let report = verifier.verify(&criteria, &evidence);
        // One passes, one fails — overall should fail
        assert_eq!(report.overall, VerificationStatus::Fail);
        assert_eq!(report.results[0].status, VerificationStatus::Pass);
        assert_eq!(report.results[1].status, VerificationStatus::Fail);
    }

    #[test]
    fn verifier_with_single_criterion_pass() {
        let verifier = Verifier::new();
        let criteria = vec![criterion("Done", None)];
        let evidence = vec![evidence(0, 0, None)];
        let report = verifier.verify(&criteria, &evidence);
        assert_eq!(report.overall, VerificationStatus::Pass);
    }
}
