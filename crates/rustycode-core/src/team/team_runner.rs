//! TeamRunner — wires the team system into the Runtime's agent loop.
//!
//! This is the integration layer between the team orchestration types
//! (Coordinator, TaskProfiler, BriefingBuilder) and the actual LLM-based
//! agent execution. It runs the Builder→Skeptic→Judge loop with role-filtered
//! briefings and structured turns.

use anyhow::{Context, Result};
use rustycode_protocol::team::*;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use super::briefing::BriefingBuilder;
use super::coordinator::{Coordinator, TurnOutcome};
use super::profiler::TaskProfiler;

/// Configuration for the team runner.
#[derive(Debug, Clone)]
pub struct TeamRunnerConfig {
    /// Maximum number of team loop iterations.
    pub max_turns: u32,
    /// Whether to use streaming for LLM calls.
    pub streaming: bool,
    /// Maximum tokens per LLM response.
    pub max_response_tokens: usize,
    /// Whether the Architect phase should run before the first Builder turn.
    /// When true, produces a StructuralDeclaration that constrains all Builder work.
    /// Default: true.
    pub architect_enabled: bool,
    /// Whether Scalpel phase should be used for targeted fixes after Judge failures.
    /// Default: true.
    pub scalpel_enabled: bool,
}

impl Default for TeamRunnerConfig {
    fn default() -> Self {
        Self {
            max_turns: 20,
            streaming: true,
            max_response_tokens: 16384,
            architect_enabled: true,
            scalpel_enabled: true,
        }
    }
}

/// The outcome of a team execution run.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum TeamRunOutcome {
    /// Task completed successfully.
    Complete {
        /// Files modified during execution.
        files_modified: Vec<String>,
        /// Final builder trust score.
        final_trust: f64,
        /// Number of turns taken.
        turns: u32,
    },
    /// Execution stopped due to an issue.
    Stopped {
        /// Why execution stopped.
        reason: StopReason,
        /// Files modified before stopping.
        files_modified: Vec<String>,
        /// Final builder trust score.
        final_trust: f64,
        /// Number of turns taken.
        turns: u32,
    },
    /// User escalation required.
    Escalated {
        /// The escalation details.
        escalation: Escalation,
        /// Files modified before escalation.
        files_modified: Vec<String>,
        /// Number of turns taken.
        turns: u32,
    },
}

/// Runs the team orchestration loop for a given task.
///
/// This is the top-level entry point that:
/// 1. Profiles the task to determine team composition
/// 2. Creates a Coordinator to manage the loop
/// 3. Runs Builder→Skeptic→Judge phases with role-filtered briefings
/// 4. Tracks trust, progress, and approach fingerprints
/// 5. Stops on completion, degradation, or escalation
pub struct TeamRunner {
    /// Project root for disk operations.
    project_root: PathBuf,
    /// Runner configuration.
    config: TeamRunnerConfig,
}

impl TeamRunner {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            config: TeamRunnerConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(project_root: impl Into<PathBuf>, config: TeamRunnerConfig) -> Self {
        Self {
            project_root: project_root.into(),
            config,
        }
    }

    /// Profile a task and return the team configuration without running.
    ///
    /// Useful for previewing what team would be assembled.
    pub fn preview_team(&self, task: &str) -> (TaskProfile, TeamConfig) {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile(task);
        let team = profile.assemble_team();
        (profile, team)
    }

    /// Execute the team loop for a task.
    ///
    /// This is the main integration point. It profiles the task, assembles
    /// the team, and runs the coordination loop. The `provider` is used for
    /// LLM calls when actual agent execution is needed.
    ///
    /// Note: In the current implementation, this runs the *coordination logic*
    /// (trust tracking, approach fingerprinting, doom detection, escalation)
    /// but delegates actual LLM calls to the caller. The coordination state
    /// machine advances based on the inputs provided via `process_turn()`.
    pub fn run_coordination_loop(
        &self,
        task: &str,
        turn_inputs: Vec<super::coordinator::TurnInput>,
    ) -> Result<TeamRunOutcome> {
        let profiler = TaskProfiler::new();
        let profile = profiler.profile(task);
        let mut coordinator = Coordinator::new(self.project_root.clone(), profile);

        info!(
            risk = %coordinator.team_config().attitude.burden_of_proof as u8,
            "Team coordination loop starting for task: {}",
            task
        );

        let mut files_modified: Vec<String> = Vec::new();
        let mut i = 0;

        for input in turn_inputs {
            i += 1;
            if i > self.config.max_turns {
                warn!("Team loop exceeded max turns ({})", self.config.max_turns);
                return Ok(TeamRunOutcome::Stopped {
                    reason: StopReason::BudgetExhausted,
                    files_modified,
                    final_trust: coordinator.state().builder_trust.value,
                    turns: i,
                });
            }

            let outcome = coordinator.process_turn(input);

            match outcome {
                TurnOutcome::ChangeProposed {
                    files_changed,
                    approach_fingerprint: _,
                } => {
                    debug!(
                        "Turn {}: builder proposed changes to {} files",
                        i,
                        files_changed.len()
                    );
                    for f in &files_changed {
                        if !files_modified.contains(f) {
                            files_modified.push(f.clone());
                        }
                    }
                }
                TurnOutcome::Approved => {
                    debug!("Turn {}: skeptic approved changes", i);
                }
                TurnOutcome::Vetoed { reason, evidence } => {
                    debug!("Turn {}: skeptic vetoed: {} — {}", i, reason, evidence);
                }
                TurnOutcome::Verified(state) => {
                    debug!(
                        "Turn {}: judge verified — compiles={}, tests={}/{}",
                        i, state.compiles, state.tests.passed, state.tests.total
                    );
                }
                TurnOutcome::Complete => {
                    info!("Task completed successfully in {} turns", i);
                    return Ok(TeamRunOutcome::Complete {
                        files_modified,
                        final_trust: coordinator.state().builder_trust.value,
                        turns: i,
                    });
                }
                TurnOutcome::Stop(reason) => {
                    info!("Team loop stopped: {:?}", reason);
                    return Ok(TeamRunOutcome::Stopped {
                        reason,
                        files_modified,
                        final_trust: coordinator.state().builder_trust.value,
                        turns: i,
                    });
                }
                TurnOutcome::Escalate(escalation) => {
                    info!("Team loop escalating to user");
                    return Ok(TeamRunOutcome::Escalated {
                        escalation,
                        files_modified,
                        turns: i,
                    });
                }
            }
        }

        // Ran out of inputs without completing
        Ok(TeamRunOutcome::Stopped {
            reason: StopReason::BudgetExhausted,
            files_modified,
            final_trust: coordinator.state().builder_trust.value,
            turns: i,
        })
    }

    /// Build a role-filtered briefing for a specific role.
    ///
    /// This is used by the agent loop to construct the right context
    /// for each role before making an LLM call.
    #[allow(clippy::too_many_arguments)]
    pub async fn build_role_briefing(
        &self,
        role: TeamRole,
        task: &str,
        dirty_files: &[String],
        attempts: &[AttemptSummary],
        insights: &[String],
        verification: Option<&VerificationState>,
        trust: Option<&TrustScore>,
        state: Option<&TeamLoopState>,
        profile: &TaskProfile,
    ) -> Result<RoleBriefing> {
        let builder = BriefingBuilder::new(&self.project_root);
        let briefing = builder
            .build(task, dirty_files, attempts, insights, verification.cloned())
            .await
            .context("failed to build briefing")?;

        Ok(RoleBriefing::for_role(
            role,
            &briefing.task,
            &briefing.relevant_code,
            &briefing.attempts,
            &briefing.constraints,
            &briefing.insights,
            verification,
            trust,
            state,
            profile,
        ))
    }

    /// Returns true if the given Judge failures are targeted (compile/type errors)
    /// and should be handled by the Scalpel before retrying with Builder.
    pub fn should_use_scalpel(&self, failures: &[String]) -> bool {
        self.config.scalpel_enabled
            && crate::team::scalpel::ScalpelPhase::is_scalpel_appropriate(failures)
    }

    /// Get the project root.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::team::{
        Familiarity, ReachLevel, RiskLevel, TaskProfile, TeamRole, TestSummary, VerificationState,
    };

    fn test_runner() -> TeamRunner {
        TeamRunner::new("/tmp/test-team-runner")
    }

    #[test]
    fn preview_team_for_simple_task() {
        let runner = test_runner();
        let (profile, team) = runner.preview_team("fix a typo in the readme");
        assert!(matches!(profile.risk, RiskLevel::Low));
        assert!(team.roles.contains(&TeamRole::Builder));
    }

    #[test]
    fn preview_team_for_security_task() {
        let runner = test_runner();
        let (profile, team) = runner.preview_team("fix the auth token security vulnerability");
        assert!(matches!(profile.risk, RiskLevel::Critical));
        assert!(team.roles.contains(&TeamRole::Skeptic));
    }

    #[test]
    fn coordination_loop_completes_on_success() {
        let runner = test_runner();

        let inputs = vec![super::super::coordinator::TurnInput {
            builder_action: Some(super::super::coordinator::BuilderAction {
                approach: "fix the bug".to_string(),
                files_changed: vec!["src/main.rs".to_string()],
                claims_done: true,
            }),
            skeptic_review: None,
            judge_results: Some(VerificationState {
                compiles: true,
                tests: TestSummary {
                    total: 5,
                    passed: 5,
                    failed: 0,
                    failed_names: vec![],
                },
                dirty_files: vec!["src/main.rs".to_string()],
            }),
        }];

        let outcome = runner.run_coordination_loop("fix the bug", inputs).unwrap();
        assert!(matches!(outcome, TeamRunOutcome::Complete { turns: 1, .. }));
    }

    #[test]
    fn coordination_loop_detects_doom() {
        let runner = test_runner();

        // Doom loop requires 3+ occurrences of the same approach category
        let inputs = vec![
            super::super::coordinator::TurnInput {
                builder_action: Some(super::super::coordinator::BuilderAction {
                    approach: "edit auth.rs".to_string(),
                    files_changed: vec!["src/auth.rs".to_string()],
                    claims_done: false,
                }),
                skeptic_review: None,
                judge_results: None,
            },
            super::super::coordinator::TurnInput {
                builder_action: Some(super::super::coordinator::BuilderAction {
                    approach: "modify auth.rs".to_string(),
                    files_changed: vec!["src/auth.rs".to_string()],
                    claims_done: false,
                }),
                skeptic_review: None,
                judge_results: None,
            },
            super::super::coordinator::TurnInput {
                builder_action: Some(super::super::coordinator::BuilderAction {
                    approach: "change auth.rs".to_string(),
                    files_changed: vec!["src/auth.rs".to_string()],
                    claims_done: false,
                }),
                skeptic_review: None,
                judge_results: None,
            },
        ];

        let outcome = runner.run_coordination_loop("fix auth", inputs).unwrap();
        assert!(matches!(
            outcome,
            TeamRunOutcome::Stopped {
                reason: StopReason::DoomLoop,
                ..
            }
        ));
    }

    #[test]
    fn coordination_loop_stops_on_budget_exhaustion() {
        let config = TeamRunnerConfig {
            max_turns: 2,
            ..Default::default()
        };
        let runner = TeamRunner::with_config("/tmp/test", config);

        // Provide 5 inputs but budget is only 2
        let inputs: Vec<_> = (0..5)
            .map(|_| super::super::coordinator::TurnInput {
                builder_action: None,
                skeptic_review: Some(super::super::coordinator::SkepticReview {
                    verdict: super::super::coordinator::SkepticVerdict::Approve,
                    issues: vec![],
                    hallucination_detected: false,
                }),
                judge_results: None,
            })
            .collect();

        let outcome = runner.run_coordination_loop("task", inputs).unwrap();
        assert!(matches!(
            outcome,
            TeamRunOutcome::Stopped {
                reason: StopReason::BudgetExhausted,
                ..
            }
        ));
    }

    #[test]
    fn coordination_loop_handles_veto() {
        let runner = test_runner();

        let inputs = vec![super::super::coordinator::TurnInput {
            builder_action: None,
            skeptic_review: Some(super::super::coordinator::SkepticReview {
                verdict: super::super::coordinator::SkepticVerdict::RevisionNeeded,
                issues: vec![("src/auth.rs".to_string(), "missing import".to_string())],
                hallucination_detected: false,
            }),
            judge_results: None,
        }];

        let outcome = runner.run_coordination_loop("fix auth", inputs).unwrap();
        // Should have completed (vetoed but not stopped)
        match outcome {
            TeamRunOutcome::Stopped { turns: 1, .. } => {} // ran out of inputs after 1 turn
            other => panic!("expected Stopped(BudgetExhausted), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn build_role_briefing_for_builder() {
        let runner = test_runner();
        let profile = TaskProfile {
            risk: RiskLevel::Moderate,
            reach: ReachLevel::Local,
            familiarity: Familiarity::SomewhatKnown,
            reversibility: Reversibility::Moderate,
            strategy: ReasoningStrategy::default(),
            signals: vec![],
        };

        let briefing = runner
            .build_role_briefing(
                TeamRole::Builder,
                "fix the bug",
                &[],
                &[],
                &[],
                None,
                None,
                None,
                &profile,
            )
            .await
            .unwrap();

        assert_eq!(briefing.role, TeamRole::Builder);
        assert_eq!(briefing.task, "fix the bug");
        // Builder should have a budget > 0
        assert!(briefing.budget.briefing > 0);
    }
}
