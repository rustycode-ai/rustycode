//! Agent role handlers for the AST Construction Crew (T13).
//!
//! Each handler wraps a pipeline phase with crew role semantics: input/output
//! contracts, handoff protocol integration, and ledger event logging.
//!
//! Handlers are designed to be composable — they can be backed by real LLM
//! calls, simulated runs, or any other execution strategy.

use std::collections::HashMap;
use std::path::Path;

use super::crew::{ArtifactKind, ConsultationReport, CrewDispatcher, CrewRole};
use super::executor::{StepExecutor, StepRunner};
use super::expander::MilestoneExpander;
use super::recovery::MilestoneRecovery;
use super::research::{ResearchBriefGenerator, ResearchConfig};
use super::skeleton::SkeletonBuilder;
use super::types::*;
use super::verifier::Verifier;

#[derive(Debug, Clone)]
pub struct HandlerResult {
    pub role: CrewRole,
    pub artifact: ArtifactKind,
    pub event_id: String,
    pub timestamp: String,
    pub success: bool,
    pub error: Option<String>,
}

impl HandlerResult {
    fn ok(role: CrewRole, artifact: ArtifactKind) -> Self {
        Self {
            event_id: format!("evt-{}", uuid::Uuid::new_v4()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            role,
            artifact,
            success: true,
            error: None,
        }
    }

    #[allow(dead_code)]
    fn fail(role: CrewRole, artifact: ArtifactKind, error: impl Into<String>) -> Self {
        Self {
            event_id: format!("evt-{}", uuid::Uuid::new_v4()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            role,
            artifact,
            success: false,
            error: Some(error.into()),
        }
    }
}

/// Scout role handler — runs Phase 1 (RESEARCH).
pub struct ScoutHandler {
    researcher: ResearchBriefGenerator,
}

impl ScoutHandler {
    pub fn new() -> Self {
        Self {
            researcher: ResearchBriefGenerator::new(ResearchConfig::default()),
        }
    }

    pub fn run(
        &self,
        assessment: &TaskAssessment,
        workspace: &Path,
    ) -> (ContextBrief, HandlerResult) {
        let brief = self
            .researcher
            .research(&assessment.task_summary, workspace);
        (
            brief,
            HandlerResult::ok(CrewRole::Scout, ArtifactKind::ContextBrief),
        )
    }
}

impl Default for ScoutHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Architect role handler — Phase 2 (SKELETON) + Phase 3 (EXPAND).
/// For complex tasks, runs the BEDD funnel before skeleton generation.
pub struct ArchitectHandler {
    skeleton_builder: SkeletonBuilder,
    expander: MilestoneExpander,
}

impl ArchitectHandler {
    pub fn new() -> Self {
        Self {
            skeleton_builder: SkeletonBuilder::new(),
            expander: MilestoneExpander::new(),
        }
    }

    /// For COMPLEX tasks, runs BEDD first and expands only the first 2 milestones.
    /// For MODERATE, expands all milestones.
    pub fn run(
        &self,
        assessment: &TaskAssessment,
        brief: &ContextBrief,
    ) -> (MilestoneSkeleton, Vec<ExecutionSegment>, HandlerResult) {
        let skeleton = self.skeleton_builder.build(assessment, brief);

        let batch_size = match assessment.complexity {
            ComplexityLevel::Complex => 2,
            _ => skeleton.milestones.len(),
        };

        let ready: Vec<&Milestone> = skeleton.ready_milestones(&[], &[]);
        let batch: Vec<Milestone> = ready.into_iter().take(batch_size).cloned().collect();
        let segments = self.expander.expand(&batch, assessment, &[], Some(brief));

        let result = HandlerResult::ok(CrewRole::Architect, ArtifactKind::ExecutionSegment);
        (skeleton, segments, result)
    }
}

impl Default for ArchitectHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder role handler — Phase 3b (EXECUTE).
/// Executes steps sequentially, collects evidence, triggers recovery on failure.
pub struct BuilderHandler<R: StepRunner> {
    executor: StepExecutor<R>,
}

impl BuilderHandler<super::executor::SimulatedRunner> {
    pub const fn new() -> Self {
        Self {
            executor: StepExecutor::new(),
        }
    }
}

impl<R: StepRunner> BuilderHandler<R> {
    pub const fn with_runner(runner: R) -> Self {
        Self {
            executor: StepExecutor::with_runner(runner),
        }
    }

    /// Returns evidence and all recovery actions from failed segments.
    pub fn run(
        &self,
        segments: &[ExecutionSegment],
    ) -> (
        HashMap<usize, Vec<StepEvidence>>,
        Vec<RecoveryAction>,
        HandlerResult,
    ) {
        let mut all_evidence = HashMap::new();
        let mut recoveries = Vec::new();

        for segment in segments {
            if segment.required_criteria.is_empty() {
                tracing::warn!(
                    milestone_id = segment.milestone_id,
                    "segment has no required_criteria — handoff may have dropped requirements"
                );
            }

            let (evidence, recovery) = self.executor.execute_segment(segment);
            all_evidence.insert(segment.milestone_id, evidence);

            if let Some(action) = recovery {
                recoveries.push(action);
            }
        }

        let result = HandlerResult::ok(CrewRole::Builder, ArtifactKind::ExecutionEvidence);

        (all_evidence, recoveries, result)
    }
}

impl Default for BuilderHandler<super::executor::SimulatedRunner> {
    fn default() -> Self {
        Self::new()
    }
}

/// Inspector role handler — Phase 4 (VERIFY).
/// Two-stage review: spec compliance, then code quality.
pub struct InspectorHandler {
    verifier: Verifier,
}

impl InspectorHandler {
    pub const fn new() -> Self {
        Self {
            verifier: Verifier::new(),
        }
    }

    pub fn run(
        &self,
        assessment: &TaskAssessment,
        evidence: &[StepEvidence],
    ) -> (VerificationReport, HandlerResult) {
        let report = self.verifier.verify(&assessment.success_criteria, evidence);
        let result = HandlerResult::ok(CrewRole::Inspector, ArtifactKind::VerificationReport);
        (report, result)
    }
}

impl Default for InspectorHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Consultant role handler — escalation for systemic blockers.
/// Deep analysis on failure root cause. Proposes reclassification, scope expansion, or strategy change.
pub struct ConsultantHandler;

impl ConsultantHandler {
    pub const fn new() -> Self {
        Self
    }

    pub fn run(
        &self,
        assessment: &TaskAssessment,
        failed_milestones: &[usize],
        total_milestones: usize,
        recovery_attempts: &HashMap<usize, u32>,
    ) -> (ConsultationReport, HandlerResult) {
        let recovery = MilestoneRecovery::new();

        let is_systemic = recovery.is_systemic(failed_milestones, total_milestones);
        let should_escalate = failed_milestones
            .iter()
            .any(|m| *recovery_attempts.get(m).unwrap_or(&0) >= 3);

        let proposed_reclassification = if is_systemic {
            match assessment.complexity {
                ComplexityLevel::Trivial => Some(ComplexityLevel::Moderate),
                ComplexityLevel::Moderate => Some(ComplexityLevel::Complex),
                ComplexityLevel::Complex => None,
            }
        } else {
            None
        };

        let report = ConsultationReport {
            blocker_description: format!(
                "{} milestone(s) failed out of {}",
                failed_milestones.len(),
                total_milestones
            ),
            failure_pattern: if is_systemic {
                "systemic".into()
            } else {
                "local".into()
            },
            proposed_reclassification,
            proposed_scope_expansion: if is_systemic {
                vec!["Root cause analysis needed before retry".into()]
            } else {
                vec![]
            },
            proposed_strategy_change: if should_escalate {
                Some("Consider alternative approach after 3+ failures".into())
            } else {
                None
            },
            findings: vec![
                format!("Failed milestones: {:?}", failed_milestones),
                format!(
                    "Recovery attempts: {:?}",
                    failed_milestones
                        .iter()
                        .map(|m| (*m, *recovery_attempts.get(m).unwrap_or(&0)))
                        .collect::<Vec<_>>()
                ),
            ],
        };

        let result = HandlerResult::ok(CrewRole::Consultant, ArtifactKind::ConsultationReport);
        (report, result)
    }
}

impl Default for ConsultantHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Orchestrates the full crew handoff chain: Scout -> Architect -> Builder -> Inspector,
/// with Consultant escalation when needed.
pub struct CrewOrchestrator<R: StepRunner = super::executor::SimulatedRunner> {
    scout: ScoutHandler,
    architect: ArchitectHandler,
    builder: BuilderHandler<R>,
    inspector: InspectorHandler,
    consultant: ConsultantHandler,
    dispatcher: CrewDispatcher,
    recovery: MilestoneRecovery,
}

impl CrewOrchestrator<super::executor::SimulatedRunner> {
    pub fn new() -> Self {
        Self {
            scout: ScoutHandler::new(),
            architect: ArchitectHandler::new(),
            builder: BuilderHandler::new(),
            inspector: InspectorHandler::new(),
            consultant: ConsultantHandler::new(),
            dispatcher: CrewDispatcher::new(),
            recovery: MilestoneRecovery::new(),
        }
    }
}

impl<R: StepRunner> CrewOrchestrator<R> {
    pub fn with_runner(runner: R) -> Self {
        Self {
            scout: ScoutHandler::new(),
            architect: ArchitectHandler::new(),
            builder: BuilderHandler::with_runner(runner),
            inspector: InspectorHandler::new(),
            consultant: ConsultantHandler::new(),
            dispatcher: CrewDispatcher::new(),
            recovery: MilestoneRecovery::new(),
        }
    }

    /// If the handoff from `from` to `to` with `artifact` is illegal, push a
    /// failure result and return `Some(early-return tuple)`.  Otherwise `None`.
    fn check_handoff(
        &self,
        results: &mut Vec<HandlerResult>,
        from: CrewRole,
        to: CrewRole,
        artifact: &ArtifactKind,
        fail_role: CrewRole,
        fail_artifact: ArtifactKind,
        msg: &str,
    ) -> Option<(VerificationReport, Vec<HandlerResult>)> {
        if self.dispatcher.validate_handoff(from, to, artifact) {
            return None;
        }
        results.push(HandlerResult::fail(fail_role, fail_artifact, msg));
        Some((
            VerificationReport {
                results: Vec::new(),
                overall: VerificationStatus::Fail,
            },
            std::mem::take(results),
        ))
    }

    /// Main entry point that drives the handoff chain.
    ///
    /// Each handler-to-handler transition is validated against the legal handoff
    /// table maintained by `CrewDispatcher`.  If a handoff is illegal the phase
    /// result is recorded as a failure and execution stops early.
    pub fn execute(
        &mut self,
        assessment: &TaskAssessment,
        workspace: &Path,
    ) -> (VerificationReport, Vec<HandlerResult>) {
        let mut results = Vec::new();
        let mut recovery_attempts: HashMap<usize, u32> = HashMap::new();

        // Phase 1: Scout
        let (brief, result) = self.scout.run(assessment, workspace);
        results.push(result);

        if let Some(early) = self.check_handoff(
            &mut results,
            CrewRole::Scout,
            CrewRole::Architect,
            &ArtifactKind::ContextBrief,
            CrewRole::Architect,
            ArtifactKind::ExecutionSegment,
            "illegal handoff: Scout → Architect with ContextBrief rejected",
        ) {
            return early;
        }

        // Phase 2+3a: Architect
        let (skeleton, segments, result) = self.architect.run(assessment, &brief);
        results.push(result);

        if let Some(early) = self.check_handoff(
            &mut results,
            CrewRole::Architect,
            CrewRole::Builder,
            &ArtifactKind::ExecutionSegment,
            CrewRole::Builder,
            ArtifactKind::ExecutionEvidence,
            "illegal handoff: Architect → Builder with ExecutionSegment rejected",
        ) {
            return early;
        }

        // Phase 3b: Builder
        let (evidence, recovery_action, result) = self.builder.run(&segments);
        results.push(result);

        if !recovery_action.is_empty() {
            for seg in &segments {
                *recovery_attempts.entry(seg.milestone_id).or_insert(0) += 1;
            }
        }

        let failed_milestones: Vec<usize> = evidence
            .iter()
            .filter_map(|(id, ev)| ev.iter().any(|e| e.exit_code != 0).then_some(*id))
            .collect();

        if self
            .recovery
            .is_systemic(&failed_milestones, skeleton.milestones.len())
        {
            if self.dispatcher.validate_handoff(
                CrewRole::Builder,
                CrewRole::Consultant,
                &ArtifactKind::ExecutionEvidence,
            ) {
                let (_report, result) = self.consultant.run(
                    assessment,
                    &failed_milestones,
                    skeleton.milestones.len(),
                    &recovery_attempts,
                );
                results.push(result);
            } else {
                results.push(HandlerResult::fail(
                    CrewRole::Consultant,
                    ArtifactKind::ConsultationReport,
                    "illegal handoff: Builder → Consultant rejected",
                ));
            }
        }

        if let Some(early) = self.check_handoff(
            &mut results,
            CrewRole::Builder,
            CrewRole::Inspector,
            &ArtifactKind::ExecutionEvidence,
            CrewRole::Inspector,
            ArtifactKind::VerificationReport,
            "illegal handoff: Builder → Inspector rejected",
        ) {
            return early;
        }

        // Phase 4: Inspector
        let all_evidence: Vec<StepEvidence> = evidence.values().flatten().cloned().collect();
        let (report, result) = self.inspector.run(assessment, &all_evidence);
        results.push(result);

        (report, results)
    }

    /// Get the dispatcher for handoff tracking.
    pub const fn dispatcher(&self) -> &CrewDispatcher {
        &self.dispatcher
    }
}

impl Default for CrewOrchestrator<super::executor::SimulatedRunner> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn trivial_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Fix typo in README".into(),
            complexity: ComplexityLevel::Trivial,
            success_criteria: vec![SuccessCriterion {
                description: "Typo is fixed".into(),
                verification_command: Some("grep fixed README.md".into()),
            }],
            route: PhaseRoute::DirectExecute,

            clarity: None,
        }
    }

    fn complex_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Implement JWT authentication with refresh tokens".into(),
            complexity: ComplexityLevel::Complex,
            success_criteria: vec![
                SuccessCriterion {
                    description: "JWT tokens issued".into(),
                    verification_command: Some("cargo test jwt".into()),
                },
                SuccessCriterion {
                    description: "Refresh flow works".into(),
                    verification_command: Some("cargo test refresh".into()),
                },
            ],
            route: PhaseRoute::RollingWave,

            clarity: None,
        }
    }

    fn workspace() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn scout_produces_context_brief() {
        let handler = ScoutHandler::new();
        let assessment = trivial_assessment();
        let (_brief, result) = handler.run(&assessment, &workspace());
        assert!(result.success);
        assert_eq!(result.role, CrewRole::Scout);
        assert_eq!(result.artifact, ArtifactKind::ContextBrief);
    }

    #[test]
    fn architect_produces_skeleton_and_segments() {
        let handler = ArchitectHandler::new();
        let assessment = trivial_assessment();
        let brief = ContextBrief {
            relevant_files: vec![],
            patterns_found: vec![],
            dependencies: vec![],
            risks: vec![],
            constraints: vec![],
        };
        let (skeleton, _segments, result) = handler.run(&assessment, &brief);
        assert!(result.success);
        assert_eq!(result.role, CrewRole::Architect);
        assert!(!skeleton.milestones.is_empty());
    }

    #[test]
    fn architect_complex_limits_batch() {
        let handler = ArchitectHandler::new();
        let assessment = complex_assessment();
        let brief = ContextBrief {
            relevant_files: vec!["auth.rs".into()],
            patterns_found: vec!["middleware pattern".into()],
            dependencies: vec!["jsonwebtoken".into()],
            risks: vec!["token expiry".into()],
            constraints: vec!["no external state".into()],
        };
        let (_skeleton, segments, result) = handler.run(&assessment, &brief);
        assert!(result.success);
        // Complex tasks should only expand first 2 milestones
        assert!(segments.len() <= 2);
    }

    #[test]
    fn builder_executes_and_collects_evidence() {
        let handler = BuilderHandler::new();
        let segments = vec![ExecutionSegment {
            milestone_id: 0,
            steps: vec![ExecutionStep {
                action: "Fix typo".into(),
                file_targets: vec!["README.md".into()],
                expected_command: None,
                verification_command: None,
                is_risky: false,
                recovery_notes: None,
            }],
            required_criteria: vec![],
            edge_cases: vec![],
        }];
        let (evidence, _recovery, result) = handler.run(&segments);
        assert!(result.success);
        assert_eq!(result.role, CrewRole::Builder);
        assert!(evidence.contains_key(&0));
    }

    #[test]
    fn inspector_verifies_against_criteria() {
        let handler = InspectorHandler::new();
        let assessment = trivial_assessment();
        let evidence = vec![StepEvidence {
            step_index: 0,
            command_run: Some("grep fixed README.md".into()),
            exit_code: 0,
            stdout_summary: "fixed".into(),
            stderr_summary: String::new(),
            changed_files: vec!["README.md".into()],
            verification_passed: Some(true),
        }];
        let (_report, result) = handler.run(&assessment, &evidence);
        assert!(result.success);
        assert_eq!(result.role, CrewRole::Inspector);
    }

    #[test]
    fn consultant_detects_systemic_failure() {
        let handler = ConsultantHandler::new();
        let assessment = complex_assessment();
        let mut attempts = HashMap::new();
        attempts.insert(0, 3);
        attempts.insert(1, 3);

        let (report, result) = handler.run(&assessment, &[0, 1], 3, &attempts);
        assert!(result.success);
        assert_eq!(result.role, CrewRole::Consultant);
        assert_eq!(report.failure_pattern, "systemic");
    }

    #[test]
    fn consultant_proposes_reclassification() {
        let handler = ConsultantHandler::new();
        let assessment = TaskAssessment {
            task_summary: "Simple task gone wrong".into(),
            complexity: ComplexityLevel::Trivial,
            success_criteria: vec![],
            route: PhaseRoute::DirectExecute,

            clarity: None,
        };
        let (report, _result) = handler.run(&assessment, &[0, 1], 2, &HashMap::new());
        assert!(report.proposed_reclassification.is_some());
        assert_eq!(
            report.proposed_reclassification,
            Some(ComplexityLevel::Moderate)
        );
    }

    #[test]
    fn orchestrator_runs_full_pipeline() {
        let mut orchestrator = CrewOrchestrator::new();
        let assessment = trivial_assessment();
        let (_report, results) = orchestrator.execute(&assessment, &workspace());

        // Should have results from scout, architect, builder, inspector at minimum
        assert!(results.len() >= 3);
        assert!(results.iter().all(|r| r.success));
    }

    #[test]
    fn orchestrator_complex_includes_consultant_on_failure() {
        let mut orchestrator = CrewOrchestrator::new();
        let assessment = complex_assessment();
        let (_report, results) = orchestrator.execute(&assessment, &workspace());

        // At minimum: scout, architect, builder, inspector
        assert!(results.len() >= 3);
    }

    #[test]
    fn handler_result_ok_fields() {
        let result = HandlerResult::ok(CrewRole::Scout, ArtifactKind::ContextBrief);
        assert!(result.success);
        assert!(result.error.is_none());
        assert!(!result.event_id.is_empty());
        assert!(!result.timestamp.is_empty());
    }

    #[test]
    fn handler_result_fail_fields() {
        let result = HandlerResult::fail(
            CrewRole::Builder,
            ArtifactKind::ExecutionEvidence,
            "exit code 1",
        );
        assert!(!result.success);
        assert_eq!(result.error, Some("exit code 1".into()));
    }

    #[test]
    fn consultant_local_failure_no_reclassification() {
        let handler = ConsultantHandler::new();
        let assessment = complex_assessment();
        let (report, _result) = handler.run(&assessment, &[0], 5, &HashMap::new());
        // Single milestone out of 5 is not systemic
        assert_eq!(report.failure_pattern, "local");
        assert!(report.proposed_reclassification.is_none());
    }

    #[test]
    fn consultant_escalation_trigger() {
        let handler = ConsultantHandler::new();
        let assessment = complex_assessment();
        let mut attempts = HashMap::new();
        attempts.insert(0, 4); // > 3 retries
        let (report, _result) = handler.run(&assessment, &[0], 3, &attempts);
        assert!(report.proposed_strategy_change.is_some());
    }
}
