//! AST pipeline controller.
//!
//! Manages the CLASSIFY → RESEARCH → SKELETON → EXPAND → EXECUTE → VERIFY flow
//! with skip rules for trivial tasks, re-entry for failures, and rolling-wave expansion.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{OrchestrationError, Result};

use super::classifier::TaskClassifier;
use super::executor::{SimulatedRunner, StepExecutor, StepRunner, MAX_RETRIES};
use super::expander::MilestoneExpander;
use super::hooks::AstHookPayload;
use super::hooks::{AstHookResponse, AstPhaseController};
use super::ledger::TaskLedger;
use super::research::{ResearchBriefGenerator, ResearchConfig};
use super::skeleton::SkeletonBuilder;
use super::tool_adapter::ToolHarness;
use super::types::*;
use super::verifier::Verifier;

/// Result from a complete AST pipeline run.
///
/// Contains the full execution context: what was assessed, how it verified,
/// where the ledger was written, and whether any milestones triggered
/// Consultant escalation.
#[derive(Debug, Clone)]
pub struct AstExecutionResult {
    /// Final verification status (Pass, Partial, or Fail).
    pub status: VerificationStatus,
    /// The Phase 0 assessment with task summary and complexity.
    pub assessment: Option<TaskAssessment>,
    /// The Phase 4 verification report with per-criterion results.
    pub report: Option<VerificationReport>,
    /// Path to the markdown ledger file.
    pub ledger_path: PathBuf,
    /// Milestone IDs that completed successfully.
    pub completed_milestones: Vec<usize>,
    /// Milestone IDs that triggered Consultant escalation.
    pub consultant_escalation: Vec<usize>,
}

/// Configuration for the AST pipeline.
#[derive(Debug)]
pub struct AstConfig {
    /// Directory for the task ledger file.
    pub ledger_dir: PathBuf,
    /// Whether to skip research for trivial tasks (default: true).
    pub skip_research_for_trivial: bool,
    /// Maximum milestones to expand per batch for complex tasks (default: 2).
    pub rolling_wave_batch_size: usize,
    /// Maximum recovery retries per step (default: 2).
    pub max_recovery_retries: u32,
    /// Tool harness for adapter-based tool-call normalization.
    pub harness: ToolHarness,
    /// Optional phase controller for firing hooks at each phase transition.
    pub controller: Option<AstPhaseController>,
}

impl Clone for AstConfig {
    fn clone(&self) -> Self {
        Self {
            ledger_dir: self.ledger_dir.clone(),
            skip_research_for_trivial: self.skip_research_for_trivial,
            rolling_wave_batch_size: self.rolling_wave_batch_size,
            max_recovery_retries: self.max_recovery_retries,
            harness: self.harness,
            controller: None,
        }
    }
}

impl Default for AstConfig {
    fn default() -> Self {
        Self {
            ledger_dir: PathBuf::from(".ast"),
            skip_research_for_trivial: true,
            rolling_wave_batch_size: 2,
            max_recovery_retries: MAX_RETRIES,
            harness: ToolHarness::ClaudeCode,
            controller: None,
        }
    }
}

/// The AST pipeline controller.
///
/// Drives a task through the 6-phase AST pipeline, maintaining a single
/// source-of-truth snapshot at each phase boundary.
pub struct AstPipeline<R: StepRunner = SimulatedRunner> {
    config: AstConfig,
    snapshot: AstSnapshot,
    classifier: TaskClassifier,
    researcher: ResearchBriefGenerator,
    skeleton_builder: SkeletonBuilder,
    expander: MilestoneExpander,
    executor: StepExecutor<R>,
    verifier: Verifier,
    controller: Option<AstPhaseController>,
    pending_responses: Vec<AstHookResponse>,
    workspace: PathBuf,
}

impl AstPipeline<SimulatedRunner> {
    /// Create a new pipeline for planning/dry-run mode.
    pub fn new(workspace: PathBuf) -> Self {
        Self::with_config(AstConfig::default(), workspace)
    }

    /// Create with a specific config.
    pub fn with_config(mut config: AstConfig, workspace: PathBuf) -> Self {
        let controller = config.controller.take();
        Self {
            snapshot: AstSnapshot {
                current_phase: AstPhase::Classify,
                assessment: None,
                brief: None,
                skeleton: None,
                active_segments: vec![],
                completed_milestones: vec![],
                evidence: HashMap::new(),
                recovery_attempts: HashMap::new(),
                consultant_escalation: vec![],
                failed_milestones: vec![],
                report: None,
            },
            config,
            classifier: TaskClassifier::new(),
            researcher: ResearchBriefGenerator::new(ResearchConfig::default()),
            skeleton_builder: SkeletonBuilder::new(),
            expander: MilestoneExpander::new(),
            executor: StepExecutor::new(),
            verifier: Verifier::new(),
            controller,
            pending_responses: Vec::new(),
            workspace,
        }
    }
}

/// Maximum iterations of the expand→execute loop before forcing termination.
const MAX_EXPAND_EXECUTE_ITERATIONS: u32 = 50;

impl<R: StepRunner> AstPipeline<R> {
    /// Create a pipeline with a custom step runner.
    pub fn with_runner(mut config: AstConfig, workspace: PathBuf, runner: R) -> Self {
        let controller = config.controller.take();
        Self {
            snapshot: AstSnapshot {
                current_phase: AstPhase::Classify,
                assessment: None,
                brief: None,
                skeleton: None,
                active_segments: vec![],
                completed_milestones: vec![],
                evidence: HashMap::new(),
                recovery_attempts: HashMap::new(),
                consultant_escalation: vec![],
                failed_milestones: vec![],
                report: None,
            },
            config,
            classifier: TaskClassifier::new(),
            researcher: ResearchBriefGenerator::new(ResearchConfig::default()),
            skeleton_builder: SkeletonBuilder::new(),
            expander: MilestoneExpander::new(),
            executor: StepExecutor::with_runner(runner),
            verifier: Verifier::new(),
            controller,
            pending_responses: Vec::new(),
            workspace,
        }
    }
}

impl<R: StepRunner> AstPipeline<R> {
    /// Get a snapshot of the current pipeline state.
    pub const fn snapshot(&self) -> &AstSnapshot {
        &self.snapshot
    }

    /// Whether Consultant escalation has been triggered for any milestone.
    pub const fn has_consultant_escalation(&self) -> bool {
        !self.snapshot.consultant_escalation.is_empty()
    }

    /// Milestone IDs that triggered Consultant escalation.
    pub fn escalated_milestones(&self) -> &[usize] {
        &self.snapshot.consultant_escalation
    }

    /// Hook responses requiring external action from recent phase transitions.
    pub fn pending_hook_responses(&self) -> &[AstHookResponse] {
        &self.pending_responses
    }

    fn process_pending_responses(&mut self) {
        let responses = std::mem::take(&mut self.pending_responses);

        for response in responses {
            match response {
                AstHookResponse::InjectContext { files, constraints } => {
                    let brief = self.snapshot.brief.get_or_insert_with(|| ContextBrief {
                        relevant_files: Vec::new(),
                        patterns_found: Vec::new(),
                        dependencies: Vec::new(),
                        risks: Vec::new(),
                        constraints: Vec::new(),
                    });
                    for file in files {
                        let path = PathBuf::from(&file);
                        if !brief.relevant_files.contains(&path) {
                            brief.relevant_files.push(path);
                        }
                    }
                    for constraint in constraints {
                        if !brief.constraints.contains(&constraint) {
                            brief.constraints.push(constraint);
                        }
                    }
                    tracing::info!(
                        "[ast-pipeline] injected context: {} files, {} constraints",
                        brief.relevant_files.len(),
                        brief.constraints.len(),
                    );
                }
                AstHookResponse::ModifyMilestones { reorder, drop } => {
                    if let Some(ref mut skeleton) = self.snapshot.skeleton {
                        if let Some(drop_ids) = drop {
                            let drop_set: std::collections::HashSet<usize> =
                                drop_ids.iter().copied().collect();
                            skeleton.milestones.retain(|m| !drop_set.contains(&m.id));
                            tracing::info!("[ast-pipeline] dropped {} milestones", drop_set.len(),);
                        }
                        if let Some(order) = reorder {
                            let mut reordered = Vec::with_capacity(order.len());
                            for id in &order {
                                if let Some(milestone) =
                                    skeleton.milestones.iter().find(|m| m.id == *id).cloned()
                                {
                                    reordered.push(milestone);
                                }
                            }
                            let ordered_set: std::collections::HashSet<usize> =
                                order.iter().copied().collect();
                            for milestone in &skeleton.milestones {
                                if !ordered_set.contains(&milestone.id) {
                                    reordered.push(milestone.clone());
                                }
                            }
                            skeleton.milestones = reordered;
                            tracing::info!("[ast-pipeline] reordered milestones from hook");
                        }
                    } else {
                        tracing::warn!(
                            "[ast-pipeline] ModifyMilestones ignored: no skeleton available"
                        );
                    }
                }
                AstHookResponse::RequestRecovery {
                    milestone_id,
                    strategy,
                } => {
                    tracing::warn!(
                        milestone_id,
                        strategy,
                        "[ast-pipeline] RequestRecovery not yet implemented"
                    );
                }
                AstHookResponse::RequestHumanReview { reason } => {
                    tracing::warn!(
                        reason,
                        "[ast-pipeline] RequestHumanReview not yet implemented"
                    );
                }
                AstHookResponse::Proceed | AstHookResponse::OverrideComplexity { .. } => {}
            }
        }
    }

    fn fire_phase_hook(&mut self, completed_phase: AstPhase) {
        if let Some(ref ctrl) = self.controller {
            let payload = AstHookPayload {
                task_id: String::new(),
                phase: completed_phase,
                assessment: self.snapshot.assessment.clone(),
                brief: self.snapshot.brief.clone(),
                skeleton: self.snapshot.skeleton.clone(),
                active_segments: self.snapshot.active_segments.clone(),
                evidence: self.snapshot.evidence.clone(),
                report: self.snapshot.report.clone(),
            };

            let (modified_payload, actions) = ctrl.after_phase(completed_phase, payload);

            if let Some(ref updated) = modified_payload.assessment {
                if let Some(ref current) = self.snapshot.assessment {
                    if updated.complexity != current.complexity {
                        self.snapshot.assessment = Some(updated.clone());
                    }
                }
            }

            self.pending_responses.extend(actions);
        }
    }

    /// Run Phase 0: CLASSIFY.
    pub fn classify(&mut self, request: &str) -> Result<&TaskAssessment> {
        if self.snapshot.current_phase != AstPhase::Classify {
            return Err(OrchestrationError::ast_phase(
                "Classify",
                self.snapshot.current_phase.to_string(),
            ));
        }

        let assessment = self.classifier.classify(request);
        self.snapshot.assessment = Some(assessment.clone());

        match assessment.route {
            PhaseRoute::DirectExecute => {
                self.snapshot.skeleton = Some(MilestoneSkeleton {
                    milestones: vec![Milestone {
                        id: 0,
                        description: assessment.task_summary,
                        deliverable: "task complete".into(),
                        depends_on: vec![],
                    }],
                });
            }
            PhaseRoute::StandardSequence | PhaseRoute::RollingWave => {}
        }

        self.fire_phase_hook(AstPhase::Classify);
        self.snapshot.current_phase = AstPhase::Research;
        self.persist_ledger()?;
        self.snapshot
            .assessment
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No assessment available"))
    }

    /// Run Phase 1: RESEARCH.
    pub fn research(&mut self) -> Result<&ContextBrief> {
        if self.snapshot.current_phase != AstPhase::Research {
            return Err(OrchestrationError::ast_phase(
                "Research",
                self.snapshot.current_phase.to_string(),
            ));
        }

        let assessment = self
            .snapshot
            .assessment
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No assessment available"))?;

        if assessment.complexity == ComplexityLevel::Trivial
            && self.config.skip_research_for_trivial
        {
            self.snapshot.brief = Some(ContextBrief {
                relevant_files: vec![],
                patterns_found: vec![],
                dependencies: vec![],
                risks: vec![],
                constraints: vec![],
            });
            self.fire_phase_hook(AstPhase::Research);
            self.snapshot.current_phase = AstPhase::Expand;
            self.persist_ledger()?;
            return self.snapshot.brief.as_ref().ok_or_else(|| {
                OrchestrationError::ast_ledger("No context brief available")
            });
        }

        let brief = self
            .researcher
            .research(&assessment.task_summary, &self.workspace);
        self.snapshot.brief = Some(brief);
        self.fire_phase_hook(AstPhase::Research);
        self.snapshot.current_phase = AstPhase::Skeleton;
        self.persist_ledger()?;
        self.snapshot
            .brief
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No context brief available"))
    }

    /// Run Phase 2: SKELETON.
    pub fn build_skeleton(&mut self) -> Result<&MilestoneSkeleton> {
        if self.snapshot.current_phase != AstPhase::Skeleton {
            return Err(OrchestrationError::ast_phase(
                "Skeleton",
                self.snapshot.current_phase.to_string(),
            ));
        }

        let assessment = self
            .snapshot
            .assessment
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No assessment available"))?;
        let brief = self
            .snapshot
            .brief
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No context brief available"))?;

        let skeleton = self.skeleton_builder.build(assessment, brief);
        self.snapshot.skeleton = Some(skeleton);
        self.fire_phase_hook(AstPhase::Skeleton);
        self.snapshot.current_phase = AstPhase::Expand;
        self.persist_ledger()?;
        self.snapshot
            .skeleton
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No skeleton available"))
    }

    /// Run Phase 3a: EXPAND — expand the next batch of milestones.
    pub fn expand(&mut self) -> Result<&[ExecutionSegment]> {
        if self.snapshot.current_phase != AstPhase::Expand {
            return Err(OrchestrationError::ast_phase(
                "Expand",
                self.snapshot.current_phase.to_string(),
            ));
        }

        let skeleton = self
            .snapshot
            .skeleton
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No skeleton available"))?;
        let assessment = self
            .snapshot
            .assessment
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No assessment available"))?;

        let ready: Vec<&Milestone> = skeleton.ready_milestones(
            &self.snapshot.completed_milestones,
            &self.snapshot.failed_milestones,
        );

        if ready.is_empty() {
            self.snapshot.current_phase = AstPhase::Verify;
            self.persist_ledger()?;
            return Ok(&self.snapshot.active_segments);
        }

        let batch_size = match assessment.complexity {
            ComplexityLevel::Complex => self.config.rolling_wave_batch_size,
            _ => ready.len(),
        };

        let batch_milestones: Vec<Milestone> =
            ready.into_iter().take(batch_size).cloned().collect();
        let segments = self.expander.expand(
            &batch_milestones,
            assessment,
            &self.snapshot.completed_milestones,
            self.snapshot.brief.as_ref(),
        );
        self.snapshot.active_segments = segments;
        self.fire_phase_hook(AstPhase::Expand);
        self.snapshot.current_phase = AstPhase::Execute;
        self.persist_ledger()?;
        Ok(&self.snapshot.active_segments)
    }

    /// Run Phase 3b: EXECUTE — execute the current batch.
    pub fn execute(&mut self) -> Result<()> {
        if self.snapshot.current_phase != AstPhase::Execute {
            return Err(OrchestrationError::ast_phase(
                "Execute",
                self.snapshot.current_phase.to_string(),
            ));
        }

        let segments = std::mem::take(&mut self.snapshot.active_segments);
        let mut newly_completed = Vec::with_capacity(segments.len());

        for segment in &segments {
            let (evidence, recovery) = self.executor.execute_segment(segment);

            self.snapshot
                .evidence
                .insert(segment.milestone_id, evidence.clone());

            if let Some(_recovery) = recovery {
                let attempts = self
                    .snapshot
                    .recovery_attempts
                    .entry(segment.milestone_id)
                    .or_insert(0);
                *attempts += 1;

                if *attempts > self.config.max_recovery_retries {
                    tracing::warn!(
                        "Milestone {} failed after {} retries, marking as failed",
                        segment.milestone_id,
                        *attempts
                    );
                    self.snapshot.failed_milestones.push(segment.milestone_id);
                    if *attempts >= 3
                        && !self
                            .snapshot
                            .consultant_escalation
                            .contains(&segment.milestone_id)
                    {
                        self.snapshot
                            .consultant_escalation
                            .push(segment.milestone_id);
                    }
                }
            } else {
                newly_completed.push(segment.milestone_id);
            }
        }

        self.snapshot.completed_milestones.extend(newly_completed);

        self.fire_phase_hook(AstPhase::Execute);
        self.snapshot.current_phase = AstPhase::Expand;
        // active_segments already taken above — no clear needed
        self.persist_ledger()?;
        Ok(())
    }

    /// Run Phase 4: VERIFY.
    pub fn verify(&mut self) -> Result<&VerificationReport> {
        if self.snapshot.current_phase != AstPhase::Verify {
            return Err(OrchestrationError::ast_phase(
                "Verify",
                self.snapshot.current_phase.to_string(),
            ));
        }

        let assessment = self
            .snapshot
            .assessment
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("No assessment available"))?;

        let all_evidence: Vec<crate::ast::types::StepEvidence> =
            self.snapshot.evidence.values().flatten().cloned().collect();

        let report = self
            .verifier
            .verify(&assessment.success_criteria, &all_evidence);

        let status = report.overall;
        self.snapshot.report = Some(report);
        self.fire_phase_hook(AstPhase::Verify);
        self.snapshot.current_phase = if status == VerificationStatus::Fail {
            AstPhase::Failed
        } else {
            AstPhase::Complete
        };
        self.persist_ledger()?;
        self.snapshot
            .report
            .as_ref()
            .ok_or_else(|| OrchestrationError::ast_ledger("report was just set above"))
    }

    /// Run the full pipeline from start to finish (for trivial/moderate tasks).
    pub fn run(&mut self, request: &str) -> Result<VerificationStatus> {
        self.classify(request)?;
        self.process_pending_responses();

        if self.snapshot.current_phase == AstPhase::Research {
            self.research()?;
            self.process_pending_responses();
        }

        if self.snapshot.current_phase == AstPhase::Skeleton {
            self.build_skeleton()?;
            self.process_pending_responses();
        }

        // Expand/execute loop with iteration guard
        let mut iteration: u32 = 0;
        loop {
            if self.snapshot.current_phase == AstPhase::Verify {
                break;
            }
            iteration += 1;
            if iteration > MAX_EXPAND_EXECUTE_ITERATIONS {
                tracing::warn!(
                    "expand/execute loop exceeded max iterations ({MAX_EXPAND_EXECUTE_ITERATIONS}), forcing transition to Verify"
                );
                self.snapshot.current_phase = AstPhase::Verify;
                break;
            }
            self.expand()?;
            self.process_pending_responses();
            if self.snapshot.current_phase == AstPhase::Verify {
                break;
            }
            self.execute()?;
            self.process_pending_responses();
        }

        let report = self.verify()?;
        let status = report.overall;
        self.process_pending_responses();
        Ok(status)
    }

    /// Run the full pipeline and return a rich result with all artifacts.
    ///
    /// This is the primary production entry point.
    pub fn run_to_completion(&mut self, request: &str) -> Result<AstExecutionResult> {
        let status = self.run(request)?;
        let ledger_path = self.config.ledger_dir.join("ledger.md");
        Ok(AstExecutionResult {
            status,
            assessment: self.snapshot.assessment.clone(),
            report: self.snapshot.report.clone(),
            ledger_path,
            completed_milestones: self.snapshot.completed_milestones.clone(),
            consultant_escalation: self.snapshot.consultant_escalation.clone(),
        })
    }

    /// Run the pipeline with a pre-classify clarity assessment.
    ///
    /// Scores the task across four dimensions (Goal, Constraints, Success
    /// Criteria, Context). If ambiguity exceeds the threshold, generates
    /// clarification questions. When answers are provided, enriches the
    /// task description before classification.
    ///
    /// The clarity report is attached to the resulting `TaskAssessment`.
    pub fn run_with_clarity(
        &mut self,
        request: &str,
        answers: &[String],
    ) -> Result<AstExecutionResult> {
        use super::clarity::{ClarityConfig, ClarityScorer};

        let scorer = ClarityScorer::new(ClarityConfig::default());
        let mut report = scorer.assess(request);

        let task = if answers.is_empty() {
            request.to_string()
        } else {
            ClarityScorer::enrich_task(request, &mut report, answers)
        };

        if !answers.is_empty() {
            tracing::info!(
                ambiguity = report.ambiguity,
                questions = report.questions.len(),
                "[ast-clarity] pre-pipeline assessment complete"
            );
        }

        // Run the standard pipeline with the (possibly enriched) task
        let status = self.run(&task)?;

        // Attach the clarity report to the assessment
        if let Some(ref mut assessment) = self.snapshot.assessment {
            assessment.clarity = Some(report);
        }

        let ledger_path = self.config.ledger_dir.join("ledger.md");
        Ok(AstExecutionResult {
            status,
            assessment: self.snapshot.assessment.clone(),
            report: self.snapshot.report.clone(),
            ledger_path,
            completed_milestones: self.snapshot.completed_milestones.clone(),
            consultant_escalation: self.snapshot.consultant_escalation.clone(),
        })
    }

    /// Persist the ledger to disk.
    fn persist_ledger(&self) -> Result<()> {
        let ledger_path = self.config.ledger_dir.join("ledger.md");
        if let Some(parent) = ledger_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| OrchestrationError::AstLedger {
                message: format!("failed to create ledger dir: {e}"),
            })?;
        }
        TaskLedger::write_snapshot_to_file(&ledger_path, &self.snapshot).map_err(|e| {
            OrchestrationError::AstLedger {
                message: format!("failed to write ledger: {e}"),
            }
        })?;
        Ok(())
    }
}

/// Assess clarity without running the pipeline.
///
/// Returns the clarity report so callers (e.g. TUI) can present
/// questions to the user before deciding whether to proceed.
pub fn assess_clarity(request: &str) -> super::clarity::ClarityReport {
    use super::clarity::{ClarityConfig, ClarityScorer};
    let scorer = ClarityScorer::new(ClarityConfig::default());
    scorer.assess(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_pipeline() -> AstPipeline<SimulatedRunner> {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let config = AstConfig {
            ledger_dir: workspace.join(".ast"),
            ..Default::default()
        };
        AstPipeline::with_config(config, workspace)
    }

    #[test]
    fn full_trivial_pipeline() {
        let mut p = tmp_pipeline();
        let status = p.run("Fix typo in README.md").unwrap();
        assert_eq!(status, VerificationStatus::Pass);
        assert_eq!(p.snapshot().current_phase, AstPhase::Complete);
    }

    #[test]
    fn classify_sets_phase() {
        let mut p = tmp_pipeline();
        p.classify("Fix typo").unwrap();
        assert_eq!(p.snapshot().current_phase, AstPhase::Research);
    }

    #[test]
    fn classify_wrong_phase_fails() {
        let mut p = tmp_pipeline();
        p.classify("Fix typo").unwrap();
        assert!(p.classify("another task").is_err());
    }

    #[test]
    fn research_wrong_phase_fails() {
        let mut p = tmp_pipeline();
        assert!(p.research().is_err());
    }

    #[test]
    fn expand_wrong_phase_fails() {
        let mut p = tmp_pipeline();
        assert!(p.expand().is_err());
    }

    #[test]
    fn execute_wrong_phase_fails() {
        let mut p = tmp_pipeline();
        assert!(p.execute().is_err());
    }

    #[test]
    fn verify_wrong_phase_fails() {
        let mut p = tmp_pipeline();
        assert!(p.verify().is_err());
    }

    #[test]
    fn ledger_is_written() {
        let mut p = tmp_pipeline();
        p.run("Fix typo").unwrap();
        let ledger_path = p.config.ledger_dir.join("ledger.md");
        assert!(ledger_path.exists());
        let content = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(content.contains("# Task:"));
    }

    #[test]
    fn trivial_skips_research() {
        let mut p = tmp_pipeline();
        p.classify("Fix typo").unwrap();
        p.research().unwrap();
        // For trivial, research is skipped so brief is empty
        assert!(p
            .snapshot()
            .brief
            .as_ref()
            .unwrap()
            .relevant_files
            .is_empty());
    }

    #[test]
    fn hooks_fire_during_run() {
        use super::super::hooks::{AstHookBridge, AstHookPoint};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let fired = std::sync::Arc::new(AtomicUsize::new(0));
        let phases_seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let f = fired.clone();
        let ps = phases_seen.clone();
        let mut bridge = AstHookBridge::new();
        bridge.register(AstHookPoint::ClassifyComplete, move |payload| {
            f.fetch_add(1, Ordering::SeqCst);
            ps.lock().unwrap().push(payload.phase);
            Ok(AstHookResponse::Proceed)
        });

        let f2 = fired.clone();
        let ps2 = phases_seen.clone();
        bridge.register(AstHookPoint::ResearchComplete, move |payload| {
            f2.fetch_add(1, Ordering::SeqCst);
            ps2.lock().unwrap().push(payload.phase);
            Ok(AstHookResponse::Proceed)
        });

        let f3 = fired.clone();
        let ps3 = phases_seen.clone();
        bridge.register(AstHookPoint::ExpandComplete, move |payload| {
            f3.fetch_add(1, Ordering::SeqCst);
            ps3.lock().unwrap().push(payload.phase);
            Ok(AstHookResponse::Proceed)
        });

        let f4 = fired.clone();
        let ps4 = phases_seen.clone();
        bridge.register(AstHookPoint::ExecuteComplete, move |payload| {
            f4.fetch_add(1, Ordering::SeqCst);
            ps4.lock().unwrap().push(payload.phase);
            Ok(AstHookResponse::Proceed)
        });

        let f5 = fired.clone();
        let ps5 = phases_seen.clone();
        bridge.register(AstHookPoint::VerifyComplete, move |payload| {
            f5.fetch_add(1, Ordering::SeqCst);
            ps5.lock().unwrap().push(payload.phase);
            Ok(AstHookResponse::Proceed)
        });

        let controller = AstPhaseController::with_bridge(bridge);

        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let config = AstConfig {
            ledger_dir: workspace.join(".ast"),
            controller: Some(controller),
            ..Default::default()
        };
        let mut p = AstPipeline::with_config(config, workspace);

        let status = p.run("Fix typo in README.md").unwrap();
        assert_eq!(status, VerificationStatus::Pass);

        // For a trivial task: classify → research (skip) → expand → execute → verify = 5 hooks
        let total = fired.load(Ordering::SeqCst);
        assert!(total >= 4, "expected at least 4 hook firings, got {total}");

        let phases = phases_seen.lock().unwrap();
        assert!(
            phases.contains(&AstPhase::Classify),
            "Classify hook should have fired, got phases: {phases:?}"
        );
        assert!(
            phases.contains(&AstPhase::Verify),
            "Verify hook should have fired, got phases: {phases:?}"
        );
    }

    // -- US-001: infinite loop fix tests --

    /// A step runner that always fails, simulating persistent failures.
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

    fn tmp_failing_pipeline() -> AstPipeline<AlwaysFailRunner> {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let config = AstConfig {
            ledger_dir: workspace.join(".ast"),
            max_recovery_retries: 1,
            ..Default::default()
        };
        AstPipeline::with_runner(config, workspace, AlwaysFailRunner)
    }

    #[test]
    fn failing_pipeline_terminates_without_hanging() {
        let mut p = tmp_failing_pipeline();
        // With AlwaysFailRunner, all steps fail. The pipeline should terminate
        // (not loop forever) because milestones get marked as failed.
        let result = p.run("Fix a complex bug requiring multiple files");
        // Should complete without hanging (the test itself is the assertion —
        // if it hangs, the test runner will time out).
        assert!(
            result.is_ok(),
            "pipeline should complete even with failures"
        );
        let snapshot = p.snapshot();
        assert!(
            !snapshot.failed_milestones.is_empty(),
            "expected some failed milestones, got {:?}",
            snapshot.failed_milestones
        );
    }

    #[test]
    fn failing_pipeline_marks_milestones_as_failed() {
        let mut p = tmp_failing_pipeline();
        p.run("Fix a complex bug requiring multiple files").unwrap();
        let snapshot = p.snapshot();
        // The milestone should be in failed_milestones, not completed_milestones
        assert!(
            !snapshot.failed_milestones.is_empty(),
            "expected failed milestones"
        );
        assert!(
            snapshot.completed_milestones.is_empty(),
            "expected no completed milestones with AlwaysFailRunner"
        );
    }

    #[test]
    fn max_iterations_guard_forces_verify() {
        // Create a pipeline with max_recovery_retries=0 so milestones are
        // immediately failed on first recovery, but with a runner that
        // never actually fails (SimulatedRunner). Instead, we test the
        // iteration guard by creating a scenario where the pipeline
        // keeps cycling. We use a runner that sometimes fails.
        struct FailOnceRunner {
            count: std::cell::Cell<usize>,
        }
        impl StepRunner for FailOnceRunner {
            fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
                let n = self.count.get();
                self.count.set(n + 1);
                // Fail every other call to create oscillation
                if n.is_multiple_of(2) {
                    StepEvidence {
                        step_index,
                        command_run: step.expected_command.clone(),
                        exit_code: 1,
                        stdout_summary: String::new(),
                        stderr_summary: "transient error".into(),
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

        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let config = AstConfig {
            ledger_dir: workspace.join(".ast"),
            max_recovery_retries: 0,
            ..Default::default()
        };
        let runner = FailOnceRunner {
            count: std::cell::Cell::new(0),
        };
        let mut p = AstPipeline::with_runner(config, workspace, runner);
        let result = p.run("Fix typo").unwrap();
        // Pipeline should terminate regardless
        assert!(
            matches!(
                result,
                VerificationStatus::Pass | VerificationStatus::Partial | VerificationStatus::Fail
            ),
            "pipeline should return a valid status"
        );
    }

    #[test]
    fn all_milestones_fail_pipeline_returns_fail() {
        use super::super::executor::FailingStepRunner;

        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let config = AstConfig {
            ledger_dir: workspace.join(".ast"),
            max_recovery_retries: 1,
            ..Default::default()
        };
        let mut p = AstPipeline::with_runner(config, workspace, FailingStepRunner::generic());
        let status = p.run("Fix a complex bug requiring multiple files").unwrap();

        assert_eq!(
            status,
            VerificationStatus::Fail,
            "all milestones failing should result in Fail status"
        );
        assert!(
            !p.snapshot().failed_milestones.is_empty(),
            "expected at least one failed milestone"
        );
        assert!(
            p.snapshot().completed_milestones.is_empty(),
            "expected no completed milestones"
        );
    }

    #[test]
    fn partial_pipeline_success_completes_without_hanging() {
        // Runner that succeeds on first call per step (simulating partial success)
        // by alternating between success and failure based on total call count.
        struct PartialSuccessRunner {
            fail_on: std::cell::Cell<usize>,
        }
        impl StepRunner for PartialSuccessRunner {
            fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
                let n = self.fail_on.get();
                self.fail_on.set(n + 1);
                // Fail on the 2nd call (index 0 succeeds, then retry at index 0 fails)
                if n == 1 {
                    StepEvidence {
                        step_index,
                        command_run: step.expected_command.clone(),
                        exit_code: 1,
                        stdout_summary: String::new(),
                        stderr_summary: "transient failure".into(),
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

        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let config = AstConfig {
            ledger_dir: workspace.join(".ast"),
            max_recovery_retries: 2,
            ..Default::default()
        };
        let runner = PartialSuccessRunner {
            fail_on: std::cell::Cell::new(0),
        };
        let mut p = AstPipeline::with_runner(config, workspace, runner);
        // This should complete without hanging
        let status = p.run("Fix typo").unwrap();
        assert!(
            matches!(
                status,
                VerificationStatus::Pass | VerificationStatus::Partial | VerificationStatus::Fail
            ),
            "pipeline should complete with a valid status"
        );
    }
}
