//! AST Integration Tests
//!
//! Comprehensive tests covering the four main layers of the AST pipeline:
//!   Group 1: TaskClassifier (classification heuristics)
//!   Group 2: AstPipeline (end-to-end execution with SimulatedRunner)
//!   Group 3: AstPhaseState (TUI progress widget logic)
//!   Group 4: StructuredThinkingToolSchema (tool definition for LLM consumption)
//!
//! All tests use SimulatedRunner -- no real LLM calls are made.

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

use std::path::PathBuf;

use rustycode_orchestration::ast::{
    AstConfig, AstExecutionResult, AstPhase, AstPipeline, ComplexityLevel, PhaseRoute,
    TaskAssessment, TaskClassifier, VerificationStatus,
};
use rustycode_orchestration::structured_thinking_tool::{
    should_use_ast, StructuredThinkingToolSchema,
};

// Helpers

/// Build a pipeline backed by SimulatedRunner writing its ledger into a temp dir.
fn make_pipeline() -> (AstPipeline, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir creation failed");
    let workspace = dir.path().to_path_buf();
    let config = AstConfig {
        ledger_dir: workspace.join(".ast"),
        ..AstConfig::default()
    };
    let pipeline = AstPipeline::with_config(config, workspace);
    (pipeline, dir)
}

/// Convenience: classify a single request and return the assessment.
fn classify(request: &str) -> TaskAssessment {
    TaskClassifier::new().classify(request)
}

// Group 1: Classification tests

mod classification {
    use super::*;

    #[test]
    fn trivial_task_classified_as_trivial() {
        let assessment = classify("Fix typo");
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Trivial,
            "short 'Fix typo' should classify as Trivial"
        );
        assert_eq!(
            assessment.route,
            PhaseRoute::DirectExecute,
            "Trivial tasks should route to DirectExecute"
        );
        assert!(
            !assessment.success_criteria.is_empty(),
            "Assessment must always have at least one success criterion"
        );
    }

    #[test]
    fn moderate_task_with_implement_keyword() {
        // "implement" is a complex signal, not moderate. Use a moderate keyword instead.
        let assessment = classify("Add unit tests for the auth module");
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Moderate,
            "'Add unit tests' should classify as Moderate"
        );
        assert_eq!(
            assessment.route,
            PhaseRoute::StandardSequence,
            "Moderate tasks should route to StandardSequence"
        );
        // Should extract a test-related criterion
        let has_test_criterion = assessment
            .success_criteria
            .iter()
            .any(|c| c.description.contains("Tests") || c.description.contains("test"));
        assert!(
            has_test_criterion,
            "Should extract a test-related success criterion"
        );
    }

    #[test]
    fn complex_task_with_architecture_keyword() {
        let assessment = classify("Redesign the architecture for cross-cutting concerns");
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Complex,
            "task with 'architecture' and 'redesign' keywords should be Complex"
        );
        assert_eq!(
            assessment.route,
            PhaseRoute::RollingWave,
            "Complex tasks should route to RollingWave"
        );
    }

    #[test]
    fn long_task_is_complex() {
        // Tasks with >25 words and NO keyword matches should fall through to the
        // word-count heuristic as Complex. We deliberately avoid all keyword signals
        // (trivial/complex/moderate) so the classifier relies on word count alone.
        let long_task = "The authentication system needs to support OAuth2 with PKCE flow \
                         including token refresh rotation and session management across \
                         multiple distributed services while maintaining backward compatibility \
                         and ensuring zero downtime during the migration process for all users";
        let word_count = long_task.split_whitespace().count();
        assert!(
            word_count > 25,
            "test task must have >25 words, got {word_count}"
        );

        let assessment = classify(long_task);
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Complex,
            "long task (>25 words) with no keyword matches should classify as Complex via word-count heuristic"
        );
    }

    #[test]
    fn empty_task_handled_gracefully() {
        let assessment = classify("");
        // Empty string has 0 words -> Trivial by word-count fallback.
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Trivial,
            "empty task should default to Trivial"
        );
        assert!(
            !assessment.success_criteria.is_empty(),
            "Even empty tasks should get a default success criterion"
        );
        // Summary should be empty string, not panic
        assert_eq!(assessment.task_summary, "");
    }
}

// Group 2: AST Pipeline tests

mod ast_pipeline {
    use super::*;

    #[test]
    fn ast_pipeline_runs_trivial_task_to_completion() {
        let (mut pipeline, _dir) = make_pipeline();
        let status = pipeline.run("Fix typo in README.md").unwrap();
        assert_eq!(
            status,
            VerificationStatus::Pass,
            "SimulatedRunner should produce Pass for trivial tasks"
        );
        assert_eq!(
            pipeline.snapshot().current_phase,
            AstPhase::Complete,
            "Pipeline should end on Complete phase"
        );
    }

    #[test]
    fn ast_pipeline_produces_assessment() {
        let (mut pipeline, _dir) = make_pipeline();
        let result: AstExecutionResult = pipeline.run_to_completion("Fix typo").unwrap();
        assert!(
            result.assessment.is_some(),
            "run_to_completion should populate the assessment"
        );
        let assessment = result.assessment.unwrap();
        assert_eq!(assessment.complexity, ComplexityLevel::Trivial);
        assert_eq!(assessment.route, PhaseRoute::DirectExecute);
    }

    #[test]
    fn ast_pipeline_creates_ledger_file() {
        let (mut pipeline, _dir) = make_pipeline();
        let result = pipeline.run_to_completion("Fix typo").unwrap();

        assert!(
            result.ledger_path.exists(),
            "Ledger file should be created at {:?}",
            result.ledger_path
        );

        let content = std::fs::read_to_string(&result.ledger_path).unwrap();
        assert!(
            content.contains("# Task:"),
            "Ledger should contain a task heading"
        );
    }

    #[test]
    fn ast_pipeline_handles_nonexistent_workspace() {
        let nonexistent = PathBuf::from("/tmp/rustycode_ast_test_nonexistent_42");
        let config = AstConfig {
            ledger_dir: nonexistent.join(".ast"),
            ..AstConfig::default()
        };
        let mut pipeline = AstPipeline::with_config(config, nonexistent);

        // The pipeline creates the ledger dir inside persist_ledger, so it should
        // succeed (std::fs::create_dir_all on the parent) even for a nonexistent path.
        // If the OS forbids it (e.g. permissions), we accept either Ok or an error.
        let result = pipeline.run("Fix typo");
        // On most systems this will succeed because create_dir_all creates the path.
        // If it fails, that is also acceptable (e.g. sandbox restrictions).
        if let Ok(status) = result {
            assert_eq!(status, VerificationStatus::Pass);
        }
        // Either way, it should not panic.
    }

    #[test]
    fn should_use_ast_detects_complex_keywords() {
        assert!(
            should_use_ast("Implement a new auth system"),
            "keyword 'Implement' should trigger AST"
        );
        assert!(
            should_use_ast("Refactor the database layer"),
            "keyword 'Refactor' should trigger AST"
        );
        assert!(
            should_use_ast("Redesign the UI architecture"),
            "keyword 'Redesign' should trigger AST"
        );
        assert!(
            should_use_ast("Migrate from REST to GraphQL"),
            "keyword 'Migrate' should trigger AST"
        );
        assert!(
            should_use_ast("Integrate with the payment gateway"),
            "keyword 'Integrate' should trigger AST"
        );

        // Long task (>80 chars) should also trigger AST regardless of keywords
        let long = "x".repeat(81);
        assert!(should_use_ast(&long), "task >80 chars should trigger AST");

        // Short simple task should not trigger AST
        assert!(
            !should_use_ast("Fix typo"),
            "short simple task should NOT trigger AST"
        );
        assert!(
            !should_use_ast("Add comment"),
            "'Add comment' has no complex keyword and is <80 chars"
        );
    }
}

// Group 3: AstPhaseState tests
//
// NOTE: AstPhaseState lives in rustycode-tui (ratatui widget). It cannot be
// imported from this orchestration crate's integration tests because the
// TUI depends on orchestration, not the other way around.
//
// Instead we replicate the pure-logic portions of AstPhaseState here to test
// the computation in isolation. The canonical tests remain in
// `crates/rustycode-tui/src/ui/ast_progress.rs`.
//
// If you need the full widget rendering tests, add them to the TUI crate.

mod ast_phase_state_logic {

    /// Minimal replica of AstPhaseState logic for testing in this crate.
    /// This mirrors the computation in `crates/rustycode-tui/src/ui/ast_progress.rs`.
    struct PhaseStateLogic {
        phase_index: usize,
        total_phases: usize,
        milestones_completed: usize,
        milestones_total: usize,
        success: bool,
    }

    impl PhaseStateLogic {
        fn progress_fraction(&self) -> f64 {
            if self.total_phases == 0 {
                return 0.0;
            }
            let phase_base = self.phase_index as f64 / self.total_phases as f64;
            let milestone_increment = if self.milestones_total > 0 {
                let milestone_frac =
                    self.milestones_completed as f64 / self.milestones_total as f64;
                milestone_frac / self.total_phases as f64
            } else {
                0.0
            };
            (phase_base + milestone_increment).min(1.0)
        }

        fn progress_bar(&self, width: usize) -> String {
            if width == 0 {
                return String::new();
            }
            let fraction = self.progress_fraction();
            let filled = (fraction * width as f64).round() as usize;
            let filled = filled.min(width);
            let empty = width - filled;
            format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
        }

        fn phase_dot_indicator(&self) -> String {
            let total = self.total_phases.max(1);
            let dots: Vec<char> = (0..total)
                .map(|i| match i.cmp(&self.phase_index) {
                    std::cmp::Ordering::Less => '\u{2713}',    // checkmark
                    std::cmp::Ordering::Equal => '\u{25CF}',   // filled circle
                    std::cmp::Ordering::Greater => '\u{25CB}', // empty circle
                })
                .collect();
            let mut s = String::new();
            for (i, &dot) in dots.iter().enumerate() {
                if i > 0 {
                    s.push(' ');
                }
                s.push(dot);
            }
            s
        }

        /// Returns a color index per phase to verify variation.
        const fn status_color_index(&self) -> usize {
            if self.success {
                99 // sentinel for "success green"
            } else {
                self.phase_index
            }
        }
    }

    #[test]
    fn phase_state_starts_inactive() {
        // Default state has phase_index=0, total_phases=0, active=false
        // progress_fraction should return 0.0 when total_phases == 0
        let state = PhaseStateLogic {
            phase_index: 0,
            total_phases: 0,
            milestones_completed: 0,
            milestones_total: 0,
            success: false,
        };
        assert_eq!(state.progress_fraction(), 0.0, "no phases means 0 progress");
    }

    #[test]
    fn progress_fraction_first_phase_zero() {
        let state = PhaseStateLogic {
            phase_index: 0,
            total_phases: 6,
            milestones_completed: 0,
            milestones_total: 0,
            success: false,
        };
        assert_eq!(
            state.progress_fraction(),
            0.0,
            "first phase with no milestones should be 0.0"
        );
    }

    #[test]
    fn progress_fraction_with_milestones() {
        let state = PhaseStateLogic {
            phase_index: 0,
            total_phases: 6,
            milestones_completed: 5,
            milestones_total: 10,
            success: false,
        };
        let frac = state.progress_fraction();
        assert!(
            frac > 0.0,
            "milestone progress within first phase should be >0"
        );
        // 5/10 milestones in phase 0/6: (0/6) + (0.5/6) = 0.0833...
        let expected = (5.0_f64 / 10.0) / 6.0;
        assert!(
            (frac - expected).abs() < 0.001,
            "expected ~{expected:.4}, got {frac:.4}"
        );
        assert!(frac < 1.0, "should not reach 1.0 yet");
    }

    #[test]
    fn progress_bar_fully_filled_at_end() {
        let state = PhaseStateLogic {
            phase_index: 5,
            total_phases: 6,
            milestones_completed: 10,
            milestones_total: 10,
            success: false,
        };
        let bar = state.progress_bar(10);
        assert_eq!(
            bar, "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            "completed pipeline should produce a full bar"
        );
    }

    #[test]
    fn progress_bar_empty_at_start() {
        let state = PhaseStateLogic {
            phase_index: 0,
            total_phases: 6,
            milestones_completed: 0,
            milestones_total: 0,
            success: false,
        };
        let bar = state.progress_bar(10);
        assert_eq!(
            bar, "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}",
            "initial state should produce an empty bar"
        );
    }

    #[test]
    fn phase_dot_indicator_shows_correct_symbols() {
        // Phase index 3 (Expand) out of 6 phases
        let state = PhaseStateLogic {
            phase_index: 3,
            total_phases: 6,
            milestones_completed: 0,
            milestones_total: 0,
            success: false,
        };
        let dots = state.phase_dot_indicator();
        // Should contain: 3 checkmarks, 1 filled circle, 2 empty circles
        let checkmark_count = dots.chars().filter(|&c| c == '\u{2713}').count();
        let filled_count = dots.chars().filter(|&c| c == '\u{25CF}').count();
        let empty_count = dots.chars().filter(|&c| c == '\u{25CB}').count();
        assert_eq!(checkmark_count, 3, "should have 3 completed phase markers");
        assert_eq!(filled_count, 1, "should have 1 active phase marker");
        assert_eq!(empty_count, 2, "should have 2 pending phase markers");
    }

    #[test]
    fn status_color_varies_by_phase() {
        let c0 = PhaseStateLogic {
            phase_index: 0,
            total_phases: 6,
            milestones_completed: 0,
            milestones_total: 0,
            success: false,
        }
        .status_color_index();

        let c5 = PhaseStateLogic {
            phase_index: 5,
            total_phases: 6,
            milestones_completed: 0,
            milestones_total: 0,
            success: false,
        }
        .status_color_index();

        assert_ne!(c0, c5, "different phases should produce different colors");

        // Success state should produce the sentinel value
        let c_success = PhaseStateLogic {
            phase_index: 0,
            total_phases: 6,
            milestones_completed: 0,
            milestones_total: 0,
            success: true,
        }
        .status_color_index();
        assert_eq!(c_success, 99, "success state should use green sentinel");
        assert_ne!(
            c_success, c0,
            "success color should differ from phase 0 color"
        );
    }
}

// Group 4: StructuredThinkingTool schema tests

mod structured_thinking_tool_schema {
    use super::*;

    #[test]
    fn schema_is_valid_json_with_required_fields() {
        let schema = StructuredThinkingToolSchema::schema();

        // Top-level structure
        assert_eq!(schema["type"], "function", "schema type must be 'function'");
        assert!(
            schema.get("function").is_some(),
            "schema must contain 'function' key"
        );

        // Required fields must be an array with at least 5 entries
        let required = schema["function"]["parameters"]["required"].as_array();
        assert!(required.is_some(), "required must be a JSON array");
        let required = required.unwrap();
        assert!(
            required.len() >= 5,
            "should have at least 5 required fields, got {}",
            required.len()
        );

        // Verify specific required field names
        let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        for expected in [
            "thought",
            "phase",
            "type",
            "confidence",
            "next_thought_needed",
        ] {
            assert!(
                required_names.contains(&expected),
                "required fields must include '{expected}'"
            );
        }

        // Properties must exist for all required fields
        let properties = &schema["function"]["parameters"]["properties"];
        for field in &required_names {
            assert!(
                properties.get(*field).is_some(),
                "property '{field}' must be defined in schema"
            );
        }
    }

    #[test]
    fn system_prompt_guidance_is_nonempty() {
        let guidance = StructuredThinkingToolSchema::system_prompt_guidance();
        assert!(
            !guidance.is_empty(),
            "system prompt guidance must not be empty"
        );
        // Should contain key usage instructions
        let lower = guidance.to_lowercase();
        assert!(
            lower.contains("structured_thinking") || lower.contains("structured thinking"),
            "guidance should mention structured_thinking tool"
        );
        assert!(
            lower.contains("phase"),
            "guidance should describe phase usage"
        );
        assert!(
            lower.contains("confidence"),
            "guidance should mention confidence rating"
        );
    }

    #[test]
    fn schema_has_correct_tool_name() {
        let schema = StructuredThinkingToolSchema::schema();
        assert_eq!(
            schema["function"]["name"], "structured_thinking",
            "tool name must be 'structured_thinking'"
        );

        // Description should be present and non-empty
        let description = schema["function"]["description"].as_str();
        assert!(description.is_some(), "function must have a description");
        assert!(
            !description.unwrap().is_empty(),
            "function description must not be empty"
        );

        // Parameter type must be "object"
        assert_eq!(
            schema["function"]["parameters"]["type"], "object",
            "parameters must be type 'object'"
        );
    }
}

// Cross-cutting integration: classify -> pipeline consistency

mod cross_cutting {
    use super::*;

    /// Verify that the classifier's routing decision matches what the pipeline
    /// actually does for a trivial task.
    #[test]
    fn classifier_and_pipeline_agree_on_trivial() {
        let task = "Fix typo";
        let assessment = classify(task);
        assert_eq!(assessment.complexity, ComplexityLevel::Trivial);

        let (mut pipeline, _dir) = make_pipeline();
        let result = pipeline.run_to_completion(task).unwrap();
        assert_eq!(result.status, VerificationStatus::Pass);

        // The pipeline should have completed milestone 0 (single direct-execute milestone)
        assert_eq!(result.completed_milestones, vec![0]);
    }

    /// Verify that a moderate task produces a StandardSequence route.
    #[test]
    fn classifier_routes_moderate_to_standard_sequence() {
        let task = "Add unit tests for the parser module";
        let assessment = classify(task);
        assert_eq!(assessment.complexity, ComplexityLevel::Moderate);
        assert_eq!(assessment.route, PhaseRoute::StandardSequence);
    }

    /// Verify the should_use_ast heuristic agrees with the classifier for edge cases.
    #[test]
    fn should_use_ast_aligns_with_complexity_for_long_tasks() {
        let long = "Implement a comprehensive new module for handling cross-system \
                    event propagation with retry logic and dead letter queues";
        assert!(should_use_ast(long), "long task should trigger AST");
        let assessment = classify(long);
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Complex,
            "classifier should also rate long tasks as Complex"
        );
    }

    /// Verify that execution result has all fields populated for a trivial task.
    #[test]
    fn execution_result_fields_populated() {
        let (mut pipeline, _dir) = make_pipeline();
        let result = pipeline.run_to_completion("Fix typo in README.md").unwrap();

        assert_eq!(result.status, VerificationStatus::Pass);
        assert!(result.assessment.is_some());
        assert!(result.report.is_some());
        assert!(result.ledger_path.exists());
        assert!(!result.completed_milestones.is_empty());
        assert!(result.consultant_escalation.is_empty());
    }

    /// Verify that the pipeline snapshot reflects the correct final phase.
    #[test]
    fn snapshot_reflects_completion() {
        let (mut pipeline, _dir) = make_pipeline();
        pipeline.run("Fix typo").unwrap();

        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.current_phase, AstPhase::Complete);
        assert!(snapshot.assessment.is_some());
        assert!(snapshot.report.is_some());
        assert!(!snapshot.completed_milestones.is_empty());
    }

    /// Verify the pipeline produces a ledger with meaningful content.
    #[test]
    fn ledger_contains_task_summary() {
        let (mut pipeline, _dir) = make_pipeline();
        let result = pipeline.run_to_completion("Fix typo in docs").unwrap();

        let content = std::fs::read_to_string(&result.ledger_path).unwrap();
        // The task summary should appear somewhere in the ledger
        assert!(
            content.contains("Fix typo") || content.contains("fix typo"),
            "Ledger should contain the task description"
        );
    }
}
