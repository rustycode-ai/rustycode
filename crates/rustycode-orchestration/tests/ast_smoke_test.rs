//! AST Smoke Test (T8) — end-to-end pipeline validation.
//!
//! Runs a "build-pmars" style moderate-complexity task through all 6 AST phases:
//! CLASSIFY → RESEARCH → SKELETON → EXPAND → EXECUTE → VERIFY
//!
//! Captures evidence at each phase boundary and validates the wiring works.

#![allow(
    unknown_lints,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::ptr_arg,
    clippy::format_in_format_args,
    clippy::let_and_return,
    clippy::match_single_binding,
    clippy::bool_to_int_with_if,
    clippy::manual_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::let_unit_value,
    clippy::unused_async,
    clippy::doc_markdown,
    clippy::unnecessary_lazy_evaluations
)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rustycode_orchestration::ast::handlers::{
    ArchitectHandler, BuilderHandler, CrewOrchestrator, InspectorHandler, ScoutHandler,
};
use rustycode_orchestration::ast::{
    AstConfig, AstPhase, AstPipeline, ComplexityLevel, ContextBrief, ExecutionSegment,
    ExecutionStep, PhaseRoute, StepEvidence, StepRunner, TaskAssessment, ToolHarness,
    VerificationStatus,
};

// Capturing runner — records every step execution for assertion

/// A `StepRunner` that records calls and returns configurable results.
struct CapturingRunner {
    calls: Arc<Mutex<Vec<(usize, String)>>>,
    /// Map step action -> exit code. Default 0 (success).
    exit_codes: HashMap<String, i32>,
}

impl CapturingRunner {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            exit_codes: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn with_failure(action: &str) -> Self {
        let mut runner = Self::new();
        runner.exit_codes.insert(action.to_string(), 1);
        runner
    }

    #[allow(dead_code)]
    fn calls(&self) -> Vec<(usize, String)> {
        self.calls.lock().unwrap().clone()
    }
}

impl StepRunner for CapturingRunner {
    fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
        self.calls
            .lock()
            .unwrap()
            .push((step_index, step.action.clone()));

        let exit_code = self.exit_codes.get(&step.action).copied().unwrap_or(0);
        StepEvidence {
            step_index,
            command_run: step.expected_command.clone(),
            exit_code,
            stdout_summary: if exit_code == 0 {
                "ok".into()
            } else {
                String::new()
            },
            stderr_summary: if exit_code != 0 {
                "error".into()
            } else {
                String::new()
            },
            changed_files: if exit_code == 0 {
                step.file_targets.clone()
            } else {
                vec![]
            },
            verification_passed: step.verification_command.as_ref().map(|_| exit_code == 0),
        }
    }
}

// Helper: create a temp pipeline

fn tmp_dir() -> PathBuf {
    tempfile::tempdir().unwrap().path().to_path_buf()
}

fn pipeline_config(workspace: &PathBuf) -> AstConfig {
    AstConfig {
        ledger_dir: workspace.join(".ast"),
        skip_research_for_trivial: true,
        rolling_wave_batch_size: 2,
        max_recovery_retries: 2,
        harness: ToolHarness::ClaudeCode,
        controller: None,
    }
}

// T8.1: Full pipeline on moderate-complexity "build-pmars" task

#[test]
fn smoke_test_build_pmars_moderate() {
    let workspace = tmp_dir();
    let config = pipeline_config(&workspace);
    let runner = CapturingRunner::new();
    let calls = runner.calls.clone();

    let mut pipeline = AstPipeline::with_runner(config, workspace.clone(), runner);

    // Phase 0: CLASSIFY
    let request = "Build the pmars binary from source and verify it runs correctly";
    let status = pipeline.run(request).unwrap();

    // Verify final state
    assert_eq!(status, VerificationStatus::Pass);
    let snapshot = pipeline.snapshot();
    assert_eq!(snapshot.current_phase, AstPhase::Complete);

    // Verify classification
    let assessment = snapshot.assessment.as_ref().unwrap();
    assert_eq!(assessment.complexity, ComplexityLevel::Moderate);
    assert_eq!(assessment.route, PhaseRoute::StandardSequence);
    assert!(!assessment.success_criteria.is_empty());

    // Verify research ran (moderate gets research)
    assert!(snapshot.brief.is_some());

    // Verify skeleton was built
    assert!(snapshot.skeleton.is_some());
    let skeleton = snapshot.skeleton.as_ref().unwrap();
    assert!(!skeleton.milestones.is_empty());

    // Verify evidence was collected
    assert!(!snapshot.evidence.is_empty());

    // Verify verification report
    assert!(snapshot.report.is_some());
    let report = snapshot.report.as_ref().unwrap();
    assert_eq!(report.overall, VerificationStatus::Pass);

    // Verify ledger was written
    let ledger_path = workspace.join(".ast").join("ledger.md");
    assert!(ledger_path.exists());
    let ledger_content = std::fs::read_to_string(&ledger_path).unwrap();
    assert!(ledger_content.contains("# Task:"));
    assert!(ledger_content.contains("COMPLETE"));

    // Verify the runner was actually called
    let captured = calls.lock().unwrap();
    assert!(
        !captured.is_empty(),
        "Runner should have been called at least once"
    );
}

// T8.2: Phase-by-phase on trivial task (direct execute path)

#[test]
fn smoke_test_trivial_direct_execute() {
    let workspace = tmp_dir();
    let mut pipeline = AstPipeline::new(workspace);

    let status = pipeline.run("Fix typo in README").unwrap();
    assert_eq!(status, VerificationStatus::Pass);

    let snapshot = pipeline.snapshot();
    assert_eq!(snapshot.current_phase, AstPhase::Complete);

    // Trivial should have DirectExecute route
    let assessment = snapshot.assessment.as_ref().unwrap();
    assert_eq!(assessment.complexity, ComplexityLevel::Trivial);
    assert_eq!(assessment.route, PhaseRoute::DirectExecute);

    // Trivial skips research (empty brief)
    assert!(snapshot.brief.as_ref().unwrap().relevant_files.is_empty());

    // Trivial gets a single-milestone skeleton
    let skeleton = snapshot.skeleton.as_ref().unwrap();
    assert_eq!(skeleton.milestones.len(), 1);
}

// T8.3: Phase-by-phase manual control (step through each phase)

#[test]
fn smoke_test_manual_phase_control() {
    let workspace = tmp_dir();
    let mut pipeline = AstPipeline::new(workspace);

    // Phase 0: CLASSIFY
    let assessment = pipeline
        .classify("Add tests for the parser module")
        .unwrap();
    assert_eq!(assessment.complexity, ComplexityLevel::Moderate);
    assert_eq!(pipeline.snapshot().current_phase, AstPhase::Research);

    // Phase 1: RESEARCH
    pipeline.research().unwrap();
    assert_eq!(pipeline.snapshot().current_phase, AstPhase::Skeleton);
    // Brief should exist (even if empty for simulated research)
    assert!(pipeline.snapshot().brief.is_some());

    // Phase 2: SKELETON
    let skeleton = pipeline.build_skeleton().unwrap();
    assert!(!skeleton.milestones.is_empty());
    assert_eq!(pipeline.snapshot().current_phase, AstPhase::Expand);

    // Phase 3a: EXPAND
    let segments = pipeline.expand().unwrap();
    assert!(!segments.is_empty());
    assert_eq!(pipeline.snapshot().current_phase, AstPhase::Execute);

    // Phase 3b: EXECUTE
    pipeline.execute().unwrap();

    // Loop expand/execute until all milestones are done
    let max_loops = 10;
    for _ in 0..max_loops {
        let phase = pipeline.snapshot().current_phase;
        if phase == AstPhase::Verify {
            break;
        }
        assert_eq!(phase, AstPhase::Expand, "Expected Expand phase");
        pipeline.expand().unwrap();
        let phase = pipeline.snapshot().current_phase;
        if phase == AstPhase::Verify {
            break;
        }
        assert_eq!(phase, AstPhase::Execute, "Expected Execute phase");
        pipeline.execute().unwrap();
    }
    assert_eq!(pipeline.snapshot().current_phase, AstPhase::Verify);

    // Phase 4: VERIFY
    let report = pipeline.verify().unwrap();
    assert!(!report.results.is_empty());
    // Pipeline should be complete (or failed if verification failed)
    let phase = pipeline.snapshot().current_phase;
    assert!(
        phase == AstPhase::Complete || phase == AstPhase::Failed,
        "Should be Complete or Failed, got {phase}"
    );
}

// T8.4: Complex task with rolling wave (2-milestone batches)

#[test]
fn smoke_test_complex_rolling_wave() {
    let workspace = tmp_dir();
    let config = pipeline_config(&workspace);
    let runner = CapturingRunner::new();

    let mut pipeline = AstPipeline::with_runner(config, workspace, runner);

    let request = "Implement a new authentication module with JWT support, \
                   refresh token rotation, and integration tests covering \
                   the full login flow";
    let status = pipeline.run(request).unwrap();

    let snapshot = pipeline.snapshot();
    let assessment = snapshot.assessment.as_ref().unwrap();
    assert_eq!(assessment.complexity, ComplexityLevel::Complex);
    assert_eq!(assessment.route, PhaseRoute::RollingWave);

    // Complex tasks should produce a multi-milestone skeleton
    let skeleton = snapshot.skeleton.as_ref().unwrap();
    assert!(
        skeleton.milestones.len() >= 2,
        "Complex task should have multiple milestones"
    );

    // Final status should be pass (simulated runner always succeeds)
    assert_eq!(status, VerificationStatus::Pass);
}

// T8.5: Evidence capture — verify all artifacts are recorded

#[test]
fn smoke_test_evidence_capture() {
    let workspace = tmp_dir();
    let config = pipeline_config(&workspace);
    let runner = CapturingRunner::new();
    let calls = runner.calls.clone();

    let mut pipeline = AstPipeline::with_runner(config, workspace, runner);
    pipeline.run("Build pmars from source with tests").unwrap();

    let snapshot = pipeline.snapshot();

    // Classification artifacts
    assert!(snapshot.assessment.is_some());
    let assessment = snapshot.assessment.as_ref().unwrap();
    assert!(!assessment.task_summary.is_empty());
    assert!(!assessment.success_criteria.is_empty());

    // Research artifacts
    assert!(snapshot.brief.is_some());

    // Skeleton artifacts
    assert!(snapshot.skeleton.is_some());
    let skeleton = snapshot.skeleton.as_ref().unwrap();
    for m in &skeleton.milestones {
        assert!(!m.description.is_empty());
        assert!(!m.deliverable.is_empty());
    }

    // Execution evidence — at least one milestone should have evidence
    assert!(!snapshot.evidence.is_empty());
    for (milestone_id, evidence) in &snapshot.evidence {
        assert!(
            !evidence.is_empty(),
            "Milestone {milestone_id} should have evidence"
        );
        for ev in evidence {
            assert_eq!(ev.exit_code, 0, "All steps should succeed in smoke test");
        }
    }

    // Verification report exists (pass/fail depends on command matching)
    assert!(snapshot.report.is_some());

    // Runner was called
    let captured = calls.lock().unwrap();
    assert!(!captured.is_empty(), "StepRunner should have been invoked");
}

// T8.6: Crew orchestrator smoke test (handler chain)

#[test]
fn smoke_test_crew_orchestrator() {
    let mut orchestrator = CrewOrchestrator::new();
    let assessment = TaskAssessment {
        task_summary: "Build the pmars binary and run its test suite".into(),
        complexity: ComplexityLevel::Moderate,
        success_criteria: vec![
            rustycode_orchestration::ast::SuccessCriterion {
                description: "Binary builds successfully".into(),
                verification_command: Some("cargo build --release".into()),
            },
            rustycode_orchestration::ast::SuccessCriterion {
                description: "Tests pass".into(),
                verification_command: Some("cargo test".into()),
            },
        ],
        route: PhaseRoute::StandardSequence,
        clarity: None,
    };
    let workspace = tmp_dir();

    let (report, results) = orchestrator.execute(&assessment, &workspace);

    // Should have results from scout, architect, builder, inspector
    assert!(
        results.len() >= 3,
        "Should have at least 3 handler results, got {}",
        results.len()
    );
    assert!(
        results.iter().all(|r| r.success),
        "All handlers should succeed"
    );

    // Report exists (pass/fail depends on simulated command matching)
    assert!(!report.results.is_empty());

    // Check handler roles are correct
    let roles: Vec<_> = results.iter().map(|r| r.role).collect();
    assert!(roles.contains(&rustycode_orchestration::ast::CrewRole::Scout));
    assert!(roles.contains(&rustycode_orchestration::ast::CrewRole::Architect));
    assert!(roles.contains(&rustycode_orchestration::ast::CrewRole::Builder));
    assert!(roles.contains(&rustycode_orchestration::ast::CrewRole::Inspector));
}

// T8.7: Individual handler smoke tests

#[test]
fn smoke_test_scout_handler() {
    let handler = ScoutHandler::new();
    let assessment = TaskAssessment {
        task_summary: "Build pmars".into(),
        complexity: ComplexityLevel::Moderate,
        success_criteria: vec![],
        route: PhaseRoute::StandardSequence,
        clarity: None,
    };
    let (_brief, result) = handler.run(&assessment, &tmp_dir());
    assert!(result.success);
    assert_eq!(result.role, rustycode_orchestration::ast::CrewRole::Scout);
    // Brief should exist (even if empty for simulated research)
    assert_eq!(
        result.artifact,
        rustycode_orchestration::ast::ArtifactKind::ContextBrief
    );
}

#[test]
fn smoke_test_architect_handler() {
    let handler = ArchitectHandler::new();
    let assessment = TaskAssessment {
        task_summary: "Build pmars from source".into(),
        complexity: ComplexityLevel::Moderate,
        success_criteria: vec![],
        route: PhaseRoute::StandardSequence,
        clarity: None,
    };
    let brief = ContextBrief {
        relevant_files: vec![PathBuf::from("src/main.c")],
        patterns_found: vec!["makefile build".into()],
        dependencies: vec!["libc".into()],
        risks: vec!["compiler flags".into()],
        constraints: vec!["posix only".into()],
    };
    let (skeleton, segments, result) = handler.run(&assessment, &brief);
    assert!(result.success);
    assert_eq!(
        result.role,
        rustycode_orchestration::ast::CrewRole::Architect
    );
    assert!(!skeleton.milestones.is_empty());
    // Moderate expands all milestones
    assert!(!segments.is_empty());
}

#[test]
fn smoke_test_builder_handler() {
    let handler = BuilderHandler::new();
    let segments = vec![ExecutionSegment {
        milestone_id: 0,
        steps: vec![
            ExecutionStep {
                action: "Download pmars source".into(),
                file_targets: vec![PathBuf::from("pmars/")],
                expected_command: Some("wget https://example.com/pmars.tar.gz".into()),
                verification_command: None,
                is_risky: false,
                recovery_notes: None,
            },
            ExecutionStep {
                action: "Build pmars binary".into(),
                file_targets: vec![PathBuf::from("pmars/pmars")],
                expected_command: Some("make -C pmars".into()),
                verification_command: Some("./pmars --version".into()),
                is_risky: false,
                recovery_notes: None,
            },
            ExecutionStep {
                action: "Run pmars test suite".into(),
                file_targets: vec![],
                expected_command: Some("make -C pmars test".into()),
                verification_command: Some("make -C pmars test".into()),
                is_risky: false,
                recovery_notes: None,
            },
        ],
        required_criteria: vec![],
        edge_cases: vec![],
    }];
    let (evidence, recovery, result) = handler.run(&segments);
    assert!(result.success);
    assert_eq!(result.role, rustycode_orchestration::ast::CrewRole::Builder);
    assert!(evidence.contains_key(&0));
    assert!(
        recovery.is_empty(),
        "Simulated runner should not produce recovery"
    );

    let ev0 = &evidence[&0];
    assert_eq!(ev0.len(), 3, "All 3 steps should have evidence");
    assert!(ev0.iter().all(|e| e.exit_code == 0));
}

#[test]
fn smoke_test_inspector_handler() {
    let handler = InspectorHandler::new();
    let assessment = TaskAssessment {
        task_summary: "Build pmars".into(),
        complexity: ComplexityLevel::Moderate,
        success_criteria: vec![
            rustycode_orchestration::ast::SuccessCriterion {
                description: "Build succeeds".into(),
                verification_command: Some("make -C pmars".into()),
            },
            rustycode_orchestration::ast::SuccessCriterion {
                description: "Tests pass".into(),
                verification_command: Some("make -C pmars test".into()),
            },
        ],
        route: PhaseRoute::StandardSequence,
        clarity: None,
    };
    let evidence = vec![
        StepEvidence {
            step_index: 0,
            command_run: Some("make -C pmars".into()),
            exit_code: 0,
            stdout_summary: "build ok".into(),
            stderr_summary: String::new(),
            changed_files: vec![PathBuf::from("pmars/pmars")],
            verification_passed: Some(true),
        },
        StepEvidence {
            step_index: 1,
            command_run: Some("make -C pmars test".into()),
            exit_code: 0,
            stdout_summary: "all tests passed".into(),
            stderr_summary: String::new(),
            changed_files: vec![],
            verification_passed: Some(true),
        },
    ];
    let (report, result) = handler.run(&assessment, &evidence);
    assert!(result.success);
    assert_eq!(
        result.role,
        rustycode_orchestration::ast::CrewRole::Inspector
    );
    assert_eq!(report.overall, VerificationStatus::Pass);
    assert_eq!(report.results.len(), 2);
    assert!(report
        .results
        .iter()
        .all(|r| r.status == VerificationStatus::Pass));
}

// T8.8: Ledger transcript verification

#[test]
fn smoke_test_ledger_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().to_path_buf();
    let _dir = dir; // Keep tempdir alive for assertions
    let config = pipeline_config(&workspace);
    let mut pipeline = AstPipeline::with_config(config, workspace.clone());

    pipeline
        .run("Build pmars binary and verify tests pass")
        .unwrap();

    let ledger_path = workspace.join(".ast").join("ledger.md");
    assert!(ledger_path.exists(), "Ledger file should exist");

    let content = std::fs::read_to_string(&ledger_path).unwrap();

    // Verify ledger contains all major sections
    assert!(
        content.contains("# Task:"),
        "Ledger should have task heading, got: {}",
        &content[..content.len().min(300)]
    );
    // The last ledger write happens at Verify -> Complete transition
    // Check for either COMPLETE or FAILED (either terminal phase)
    let has_terminal = content.contains("COMPLETE") || content.contains("FAILED");
    assert!(
        has_terminal,
        "Ledger should show terminal phase, content:\n{content}"
    );

    // Verify it's human-readable (not binary / JSON)
    assert!(!content.contains('\0'), "Ledger should be plain text");
}

// T8.9: Recovery count should be 0 for well-understood task

#[test]
fn smoke_test_no_recovery_on_simple_task() {
    let workspace = tmp_dir();

    let mut pipeline = AstPipeline::new(workspace);
    pipeline.run("Fix typo in README.md").unwrap();

    let snapshot = pipeline.snapshot();
    let total_recoveries: u32 = snapshot.recovery_attempts.values().sum();
    assert_eq!(
        total_recoveries, 0,
        "Simulated runner should produce zero recovery attempts"
    );
}

// T8.10: Phase order enforcement

#[test]
fn smoke_test_phase_order_enforcement() {
    let workspace = tmp_dir();
    let mut pipeline = AstPipeline::new(workspace);

    // Cannot skip to execute without classify
    assert!(pipeline.execute().is_err());
    assert!(pipeline.verify().is_err());
    assert!(pipeline.expand().is_err());

    // Classify first
    pipeline.classify("test task").unwrap();

    // Cannot verify yet
    assert!(pipeline.verify().is_err());

    // Cannot classify again
    assert!(pipeline.classify("another task").is_err());
}

// T8.11: Metrics summary (wall-clock, phase count, etc.)

#[test]
fn smoke_test_metrics_summary() {
    let workspace = tmp_dir();
    let config = pipeline_config(&workspace);
    let runner = CapturingRunner::new();
    let calls = runner.calls.clone();

    let mut pipeline = AstPipeline::with_runner(config, workspace, runner);

    let start = std::time::Instant::now();
    pipeline.run("Build pmars from source with tests").unwrap();
    let elapsed = start.elapsed();

    let snapshot = pipeline.snapshot();

    // Count phases executed
    let has_assessment = snapshot.assessment.is_some();
    let has_brief = snapshot.brief.is_some();
    let has_skeleton = snapshot.skeleton.is_some();
    let has_evidence = !snapshot.evidence.is_empty();
    let has_report = snapshot.report.is_some();

    assert!(has_assessment, "Phase 0 (CLASSIFY) should have run");
    assert!(has_brief, "Phase 1 (RESEARCH) should have run");
    assert!(has_skeleton, "Phase 2 (SKELETON) should have run");
    assert!(
        has_evidence,
        "Phase 3 (EXECUTE) should have collected evidence"
    );
    assert!(has_report, "Phase 4 (VERIFY) should have produced report");

    // Phase count: at least 5 phases executed
    let phases_run = [
        has_assessment,
        has_brief,
        has_skeleton,
        has_evidence,
        has_report,
    ]
    .into_iter()
    .filter(|&x| x)
    .count();
    assert_eq!(phases_run, 5, "All 5 phases should have executed");

    // Step count
    let step_count = calls.lock().unwrap().len();
    assert!(
        step_count > 0,
        "At least one step should have been executed"
    );

    // Wall-clock should be fast (simulated)
    assert!(elapsed.as_secs() < 10, "Smoke test should complete in <10s");
}
