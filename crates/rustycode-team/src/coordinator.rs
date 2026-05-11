//! Team Coordinator — manages the agent team loop.
//!
//! The coordinator is the "tech lead" of the team. It:
//! - Builds fresh briefings from disk every turn
//! - Dispatches work to the builder
//! - Collects skeptic reviews
//! - Runs judge verification
//! - Tracks progress, trust, and approach fingerprints
//! - Detects stuck/degrading/doom conditions
//! - Rotates builders on repeated failure
//! - Escalates to the user when needed

use rustycode_protocol::team::*;
use rustycode_protocol::ConvoyPlan; // Added
use std::path::{Path, PathBuf};
use tracing::warn;

/// The team coordinator. Manages the execution loop.
pub struct Coordinator {
    /// Project root for disk operations.
    project_root: PathBuf,
    /// Current loop state.
    state: TeamLoopState,
    /// Attempt log (grows, fed to BriefingBuilder for compression).
    attempt_log: Vec<AttemptSummary>,
    /// Insights discovered during execution.
    insights: Vec<String>,
    /// Architectural contract from Architect phase. None until Architect has run.
    structural_declaration: Option<StructuralDeclaration>,
    /// Associated execution plan (from Convoy)
    plan: Option<ConvoyPlan>,
}

/// Outcome of a single coordination turn.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum TurnOutcome {
    /// Builder produced a change, needs review.
    ChangeProposed {
        files_changed: Vec<String>,
        approach_fingerprint: ApproachFingerprint,
    },
    /// Skeptic approved the change.
    Approved,
    /// Skeptic vetoed the change, builder needs to retry.
    Vetoed { reason: String, evidence: String },
    /// Judge verification results.
    Verified(VerificationState),
    /// Task is complete (all tests pass, promise met).
    Complete,
    /// Loop should stop.
    Stop(StopReason),
    /// User escalation needed.
    Escalate(Escalation),
}

/// Input needed for the coordinator to process a turn.
#[derive(Debug, Clone)]
pub struct TurnInput {
    /// What the builder did this turn (if anything).
    pub builder_action: Option<BuilderAction>,
    /// What the skeptic found (if anything).
    pub skeptic_review: Option<SkepticReview>,
    /// What the judge measured (if anything).
    pub judge_results: Option<VerificationState>,
}

/// What the builder did in a turn.
#[derive(Debug, Clone)]
pub struct BuilderAction {
    /// Description of the approach used.
    pub approach: String,
    /// Files that were modified.
    pub files_changed: Vec<String>,
    /// Whether the builder claims the task is done.
    pub claims_done: bool,
}

/// The skeptic's review of the builder's work.
#[derive(Debug, Clone)]
pub struct SkepticReview {
    /// The skeptic's verdict.
    pub verdict: SkepticVerdict,
    /// Specific issues found (file path → issue description).
    pub issues: Vec<(String, String)>,
    /// Whether any hallucinations were detected.
    pub hallucination_detected: bool,
}

/// The skeptic's verdict on the builder's work.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkepticVerdict {
    /// The change looks good.
    Approve,
    /// The change needs revision (specific issues listed).
    RevisionNeeded,
    /// Hard stop — hallucination or critical issue.
    Stop,
}

impl Coordinator {
    pub fn new(project_root: PathBuf, profile: TaskProfile) -> Self {
        let team_config = profile.assemble_team();
        let state = TeamLoopState::new(team_config);
        Self {
            project_root,
            state,
            attempt_log: Vec::new(),
            insights: Vec::new(),
            structural_declaration: None,
            plan: None,
        }
    }

    /// Create with an existing team config (for testing or custom teams).
    pub fn with_config(project_root: PathBuf, team_config: TeamConfig) -> Self {
        let state = TeamLoopState::new(team_config);
        Self {
            project_root,
            state,
            attempt_log: Vec::new(),
            insights: Vec::new(),
            structural_declaration: None,
            plan: None,
        }
    }

    /// Set an execution plan for the coordinator.
    pub fn with_plan(mut self, plan: ConvoyPlan) -> Self {
        self.plan = Some(plan);
        self
    }

    /// Process one turn of the coordination loop.
    /// Returns what should happen next.
    pub fn process_turn(&mut self, input: TurnInput) -> TurnOutcome {
        self.state.turn += 1;
        let turn = self.state.turn;

        // Check stopping conditions first
        self.state.check_stop_conditions();
        if let Some(ref stop) = self.state.stop_reason {
            return TurnOutcome::Stop(stop.clone());
        }
        if let Some(ref escalation) = self.state.escalation {
            return TurnOutcome::Escalate(escalation.clone());
        }

        // Process builder action
        if let Some(action) = &input.builder_action {
            let fingerprint =
                ApproachFingerprint::new(&action.approach, action.files_changed.clone());

            // Check for approach repetition (doom detection)
            if self.state.is_repeating_approach(&fingerprint) {
                warn!(
                    "Turn {}: Builder repeating approach: {}",
                    turn, fingerprint.strategy
                );

                // Rotate builder
                self.state.team_config.builder_generation += 1;
                self.state.builder_trust.record(TrustEvent {
                    kind: TrustEventKind::RepeatedFailure,
                    turn,
                    note: format!("repeated approach: {}", fingerprint.strategy),
                });

                // Check if trust is exhausted
                self.state.check_stop_conditions();
                if let Some(ref stop) = self.state.stop_reason {
                    return TurnOutcome::Stop(stop.clone());
                }
                if let Some(ref escalation) = self.state.escalation {
                    return TurnOutcome::Escalate(escalation.clone());
                }

                return TurnOutcome::Stop(StopReason::DoomLoop);
            }

            // If builder claims done, check with judge
            if action.claims_done {
                if let Some(ref judge) = input.judge_results {
                    if judge.compiles && judge.tests.failed == 0 {
                        self.state.builder_trust.record(TrustEvent {
                            kind: TrustEventKind::TaskCompleted,
                            turn,
                            note: "task completed, all tests pass".to_string(),
                        });
                        self.state.stop_reason = Some(StopReason::TaskComplete);
                        return TurnOutcome::Complete;
                    }
                }
                // Builder claims done but can't verify yet — need judge
            }

            self.state.previous_approaches.push(fingerprint.clone());

            return TurnOutcome::ChangeProposed {
                files_changed: action.files_changed.clone(),
                approach_fingerprint: fingerprint,
            };
        }

        // Process skeptic review
        if let Some(review) = &input.skeptic_review {
            if review.hallucination_detected {
                self.state.builder_trust.record(TrustEvent {
                    kind: TrustEventKind::HallucinationCaught,
                    turn,
                    note: review
                        .issues
                        .iter()
                        .map(|(f, i)| format!("{}: {}", f, i))
                        .collect::<Vec<_>>()
                        .join("; "),
                });
                self.state.team_config.attitude.tighten();
            }

            match review.verdict {
                SkepticVerdict::Approve => {
                    self.state.builder_trust.record(TrustEvent {
                        kind: TrustEventKind::ClaimVerified,
                        turn,
                        note: "skeptic approved".to_string(),
                    });
                    // Relax attitude on success
                    if self.state.progress_delta.is_improving() {
                        self.state.team_config.attitude.relax();
                    }
                    return TurnOutcome::Approved;
                }
                SkepticVerdict::RevisionNeeded => {
                    self.state.builder_trust.record(TrustEvent {
                        kind: TrustEventKind::ClaimRefuted,
                        turn,
                        note: review
                            .issues
                            .iter()
                            .map(|(f, i)| format!("{}: {}", f, i))
                            .collect::<Vec<_>>()
                            .join("; "),
                    });
                    return TurnOutcome::Vetoed {
                        reason: "skeptic requires revision".to_string(),
                        evidence: review
                            .issues
                            .iter()
                            .map(|(f, i)| format!("- {}: {}", f, i))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    };
                }
                SkepticVerdict::Stop => {
                    self.state.builder_trust.record(TrustEvent {
                        kind: TrustEventKind::HallucinationCaught,
                        turn,
                        note: "skeptic hard-stop".to_string(),
                    });
                    return TurnOutcome::Vetoed {
                        reason: "skeptic veto (hard stop)".to_string(),
                        evidence: review
                            .issues
                            .iter()
                            .map(|(f, i)| format!("- {}: {}", f, i))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    };
                }
            }
        }

        // Process judge results
        if let Some(judge) = input.judge_results {
            let current_passed = judge.tests.passed.min(i32::MAX as usize) as i32;
            let tests_delta = match self.state.last_tests_passed {
                Some(prev) => current_passed - (prev.min(i32::MAX as usize) as i32),
                None => 0,
            };
            self.state.last_tests_passed = Some(judge.tests.passed);
            self.state.progress_delta.push(tests_delta);

            // Track compilation failures
            if !judge.compiles {
                self.state.builder_trust.record(TrustEvent {
                    kind: TrustEventKind::CompilationFailed,
                    turn,
                    note: format!(
                        "compilation failed, {} dirty files",
                        judge.dirty_files.len()
                    ),
                });
            }

            // Track regressions
            if judge.tests.failed > 0 {
                self.state.builder_trust.record(TrustEvent {
                    kind: TrustEventKind::RegressionsIntroduced,
                    turn,
                    note: format!(
                        "{} tests failed: {}",
                        judge.tests.failed,
                        judge.tests.failed_names.join(", ")
                    ),
                });
            }

            // Check if task is complete
            if judge.compiles && judge.tests.failed == 0 && !judge.dirty_files.is_empty() {
                // All tests pass and there are changes — likely done
                return TurnOutcome::Verified(judge);
            }

            return TurnOutcome::Verified(judge);
        }

        // No input — shouldn't happen, but handle gracefully
        TurnOutcome::Stop(StopReason::UserStop)
    }

    /// Record an attempt summary (called after each builder attempt completes).
    pub fn record_attempt(&mut self, summary: AttemptSummary) {
        let gen = self.state.team_config.builder_generation;
        let mut summary = summary;
        summary.builder_generation = gen;

        // Track progress delta based on outcome
        let delta = match &summary.outcome {
            AttemptOutcome::Success => 1,
            AttemptOutcome::TestFailure => -1,
            AttemptOutcome::CompilationError => -2,
            AttemptOutcome::Vetoed(_) => -1,
            AttemptOutcome::WrongApproach => -1,
            AttemptOutcome::Timeout => 0,
            #[allow(unreachable_patterns)]
            _ => 0,
        };
        self.state.progress_delta.push(delta);

        self.attempt_log.push(summary);
    }

    /// Add an insight discovered during execution.
    pub fn add_insight(&mut self, insight: String) {
        // Deduplicate
        if !self.insights.contains(&insight) {
            self.insights.push(insight);
        }
    }

    /// Get the current team config (may have evolved during execution).
    pub fn team_config(&self) -> &TeamConfig {
        &self.state.team_config
    }

    /// Get the current loop state.
    pub fn state(&self) -> &TeamLoopState {
        &self.state
    }

    /// Get the attempt log (for BriefingBuilder).
    pub fn attempt_log(&self) -> &[AttemptSummary] {
        &self.attempt_log
    }

    /// Get insights (for BriefingBuilder).
    pub fn insights(&self) -> &[String] {
        &self.insights
    }

    /// Get the project root.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the structural declaration from the Architect phase, if set.
    pub fn structural_declaration(&self) -> Option<&StructuralDeclaration> {
        self.structural_declaration.as_ref()
    }

    /// Set the structural declaration from the Architect phase.
    pub fn set_structural_declaration(&mut self, decl: StructuralDeclaration) {
        self.structural_declaration = Some(decl);
    }

    /// Check if the Architect phase still needs to run.
    pub fn needs_architect_phase(&self) -> bool {
        self.structural_declaration.is_none()
    }

    /// Should we rotate to a fresh builder?
    pub fn should_rotate_builder(&self) -> bool {
        // Rotate after 3 consecutive failures
        let recent: Vec<_> = self.attempt_log.iter().rev().take(3).collect();
        recent.len() >= 3
            && recent
                .iter()
                .all(|a| !matches!(a.outcome, AttemptOutcome::Success))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_coordinator() -> Coordinator {
        let profile = TaskProfile {
            risk: RiskLevel::Moderate,
            reach: ReachLevel::Local,
            familiarity: Familiarity::SomewhatKnown,
            reversibility: Reversibility::Moderate,
            strategy: ReasoningStrategy::default(),
            signals: vec![],
        };
        Coordinator::new(PathBuf::from("/tmp/test"), profile)
    }

    #[test]
    fn coordinator_starts_with_standard_team() {
        let coord = test_coordinator();
        assert!(coord.team_config().roles.contains(&TeamRole::Builder));
        assert!(coord.team_config().roles.contains(&TeamRole::Skeptic));
        assert!(coord.team_config().roles.contains(&TeamRole::Judge));
        assert!(coord.team_config().roles.contains(&TeamRole::Coordinator));
    }

    #[test]
    fn coordinator_processes_builder_action() {
        let mut coord = test_coordinator();
        let outcome = coord.process_turn(TurnInput {
            builder_action: Some(BuilderAction {
                approach: "edit auth.rs validate function".to_string(),
                files_changed: vec!["src/auth.rs".to_string()],
                claims_done: false,
            }),
            skeptic_review: None,
            judge_results: None,
        });
        assert!(matches!(outcome, TurnOutcome::ChangeProposed { .. }));
    }

    #[test]
    fn coordinator_detects_doom_loop() {
        let mut coord = test_coordinator();

        // First attempt (no prior history)
        let _ = coord.process_turn(TurnInput {
            builder_action: Some(BuilderAction {
                approach: "edit auth.rs".to_string(),
                files_changed: vec!["src/auth.rs".to_string()],
                claims_done: false,
            }),
            skeptic_review: None,
            judge_results: None,
        });

        // Second attempt — same file, same category (Modification). Still not doom.
        let _ = coord.process_turn(TurnInput {
            builder_action: Some(BuilderAction {
                approach: "modify auth.rs".to_string(),
                files_changed: vec!["src/auth.rs".to_string()],
                claims_done: false,
            }),
            skeptic_review: None,
            judge_results: None,
        });

        // Third attempt — same file, same category. NOW triggers doom (3rd occurrence).
        let outcome = coord.process_turn(TurnInput {
            builder_action: Some(BuilderAction {
                approach: "change auth.rs".to_string(),
                files_changed: vec!["src/auth.rs".to_string()],
                claims_done: false,
            }),
            skeptic_review: None,
            judge_results: None,
        });

        assert!(matches!(outcome, TurnOutcome::Stop(StopReason::DoomLoop)));
    }

    #[test]
    fn coordinator_processes_skeptic_approval() {
        let mut coord = test_coordinator();
        let outcome = coord.process_turn(TurnInput {
            builder_action: None,
            skeptic_review: Some(SkepticReview {
                verdict: SkepticVerdict::Approve,
                issues: vec![],
                hallucination_detected: false,
            }),
            judge_results: None,
        });
        assert!(matches!(outcome, TurnOutcome::Approved));
    }

    #[test]
    fn coordinator_processes_skeptic_veto() {
        let mut coord = test_coordinator();
        let outcome = coord.process_turn(TurnInput {
            builder_action: None,
            skeptic_review: Some(SkepticReview {
                verdict: SkepticVerdict::RevisionNeeded,
                issues: vec![(
                    "src/auth.rs".to_string(),
                    "import foo::bar does not resolve".to_string(),
                )],
                hallucination_detected: false,
            }),
            judge_results: None,
        });
        assert!(matches!(outcome, TurnOutcome::Vetoed { .. }));
    }

    #[test]
    fn coordinator_processes_hallucination() {
        let mut coord = test_coordinator();
        let outcome = coord.process_turn(TurnInput {
            builder_action: None,
            skeptic_review: Some(SkepticReview {
                verdict: SkepticVerdict::Stop,
                issues: vec![(
                    "src/auth.rs".to_string(),
                    "function baz() does not exist in module".to_string(),
                )],
                hallucination_detected: true,
            }),
            judge_results: None,
        });
        assert!(matches!(outcome, TurnOutcome::Vetoed { .. }));
        // Trust should have dropped
        assert!(coord.state().builder_trust.value < 0.7);
    }

    #[test]
    fn coordinator_detects_task_complete() {
        let mut coord = test_coordinator();
        let outcome = coord.process_turn(TurnInput {
            builder_action: Some(BuilderAction {
                approach: "fix the bug".to_string(),
                files_changed: vec!["src/auth.rs".to_string()],
                claims_done: true,
            }),
            skeptic_review: None,
            judge_results: Some(VerificationState {
                compiles: true,
                tests: TestSummary {
                    total: 10,
                    passed: 10,
                    failed: 0,
                    failed_names: vec![],
                },
                dirty_files: vec!["src/auth.rs".to_string()],
            }),
        });
        assert!(matches!(outcome, TurnOutcome::Complete));
    }

    #[test]
    fn coordinator_tracks_attempts() {
        let mut coord = test_coordinator();
        coord.record_attempt(AttemptSummary {
            approach: "first try".to_string(),
            files_changed: vec!["src/auth.rs".to_string()],
            outcome: AttemptOutcome::TestFailure,
            root_cause: "2 tests fail".to_string(),
            builder_generation: 0,
        });
        coord.record_attempt(AttemptSummary {
            approach: "second try".to_string(),
            files_changed: vec!["src/auth.rs".to_string()],
            outcome: AttemptOutcome::CompilationError,
            root_cause: "missing import".to_string(),
            builder_generation: 0,
        });
        assert_eq!(coord.attempt_log().len(), 2);
    }

    #[test]
    fn coordinator_deduplicates_insights() {
        let mut coord = test_coordinator();
        coord.add_insight("auth module uses token-based auth".to_string());
        coord.add_insight("auth module uses token-based auth".to_string());
        coord.add_insight("tests are in tests/auth_test.rs".to_string());
        assert_eq!(coord.insights().len(), 2);
    }

    #[test]
    fn coordinator_builder_rotation() {
        let mut coord = test_coordinator();
        // 3 failures
        for i in 0..3 {
            coord.record_attempt(AttemptSummary {
                approach: format!("attempt {}", i),
                files_changed: vec!["src/auth.rs".to_string()],
                outcome: AttemptOutcome::TestFailure,
                root_cause: "tests fail".to_string(),
                builder_generation: 0,
            });
        }
        assert!(coord.should_rotate_builder());
    }

    #[test]
    fn coordinator_no_rotation_on_success() {
        let mut coord = test_coordinator();
        coord.record_attempt(AttemptSummary {
            approach: "fixed it".to_string(),
            files_changed: vec!["src/auth.rs".to_string()],
            outcome: AttemptOutcome::Success,
            root_cause: "success".to_string(),
            builder_generation: 0,
        });
        assert!(!coord.should_rotate_builder());
    }
}
