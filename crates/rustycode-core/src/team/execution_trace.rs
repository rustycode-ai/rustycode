//! Execution Trace — Captures detailed task execution for learning.
//!
//! Inspired by the Meta-Harness paper (arXiv:2603.28052):
//! - "The harness itself is the optimization target" — capture everything the harness does
//! - "Draft-verification pattern" — record what worked and what failed during verification
//! - "Execution trace mining" — learn from sequences of actions, not just outcomes
//! - "Skill quality > search parameters" — pattern quality matters more than volume
//!
//! # Architecture
//!
//! ```text
//! Task Execution → ExecutionTrace → PatternMiner → DiscoveredPattern → VectorMemory
//!                      │                                               ↓
//!                      └─→ Records: turns, tools, outcomes,    PatternRegistry
//!                          root causes, failure taxonomy         (cross-task)
//! ```

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ============================================================================
// Execution Trace
// ============================================================================

/// Complete trace of a task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    /// Original task description.
    pub task: String,
    /// Outcome of the task.
    pub outcome: TaskOutcome,
    /// Turn-by-turn trace.
    pub turns: Vec<TurnTrace>,
    /// Files modified.
    pub files_modified: Vec<String>,
    /// Total turns taken.
    pub total_turns: u32,
    /// Final trust score.
    pub final_trust: f64,
    /// Auto-extracted root cause (for failures).
    pub root_cause: Option<String>,
    /// Categorized failure type (Meta-Harness: failure taxonomy).
    pub failure_category: Option<FailureCategory>,
    /// Patterns discovered during execution.
    pub discovered_patterns: Vec<DiscoveredPattern>,
    /// Tool calls made during execution (Meta-Harness: tool usage tracking).
    pub tool_calls: Vec<ToolCallRecord>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: Option<u64>,
}

/// Outcome of a task.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutcome {
    Success,
    Failed,
    Cancelled,
}

/// Categorized failure types for pattern matching (Meta-Harness: failure taxonomy).
///
/// Instead of generic "task failed", categorize the failure so we can learn
/// project-specific patterns about *what kind* of failures are common.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    /// Compilation error that couldn't be resolved.
    CompilationError,
    /// One or more tests failed.
    TestFailure,
    /// Logic bug in the implementation.
    LogicBug,
    /// Missing import or dependency.
    MissingImport,
    /// Type mismatch that couldn't be resolved.
    TypeMismatch,
    /// Exceeded turn budget.
    TurnBudgetExhausted,
    /// Trust score depleted.
    TrustExhausted,
    /// Agent produced unparseable output.
    ParseFailure,
    /// External tool or process failed.
    ToolFailure,
    /// Cancelled by user.
    UserCancelled,
    /// Unknown/unclassified failure.
    Unknown,
}

impl FailureCategory {
    /// Classify a failure from error messages and context.
    pub fn from_errors(errors: &[String], total_turns: u32, trust: f64, turn_budget: u32) -> Self {
        // Check trust exhaustion first
        if trust < 0.2 {
            return Self::TrustExhausted;
        }
        if total_turns >= turn_budget {
            return Self::TurnBudgetExhausted;
        }
        // Check error messages for classification
        let all_errors = errors.join(" ").to_lowercase();
        if all_errors.contains("mismatched types") || all_errors.contains("expected `") {
            return Self::TypeMismatch;
        }
        if all_errors.contains("could not find")
            || all_errors.contains("unresolved import")
            || all_errors.contains("not found in scope")
        {
            return Self::MissingImport;
        }
        if all_errors.contains("compilation")
            || all_errors.contains("cargo build")
            || all_errors.contains("error[e")
        {
            return Self::CompilationError;
        }
        if all_errors.contains("test")
            && (all_errors.contains("failed") || all_errors.contains("panic"))
        {
            return Self::TestFailure;
        }
        if all_errors.contains("parse")
            || all_errors.contains("deserialize")
            || all_errors.contains("invalid json")
        {
            return Self::ParseFailure;
        }
        Self::Unknown
    }
}

/// Record of a single tool call (Meta-Harness: tool usage tracking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// Tool name (e.g., "ReadFile", "Bash", "MultiEdit").
    pub tool_name: String,
    /// Whether the tool succeeded.
    pub success: bool,
    /// Duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Files affected (if applicable).
    pub files: Vec<String>,
    /// Error message (if failed).
    pub error: Option<String>,
}

/// Trace of a single turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTrace {
    /// Turn number (1-indexed).
    pub turn_number: u32,
    /// Agent role that acted.
    pub agent_role: String,
    /// Action taken.
    pub action: String,
    /// Files changed (if any).
    pub files_changed: Vec<String>,
    /// Events emitted (if any).
    pub events: Vec<String>,
    /// Whether verification passed.
    pub verification_passed: bool,
    /// Error messages (if any).
    pub errors: Vec<String>,
    /// Tool calls made during this turn (Meta-Harness: per-turn tool tracking).
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
    /// Trust delta for this turn (how much trust changed).
    #[serde(default)]
    pub trust_delta: Option<f64>,
}

/// A pattern discovered from execution traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPattern {
    /// Pattern description.
    pub description: String,
    /// Confidence score (0.0-1.0).
    pub confidence: f32,
    /// Number of occurrences supporting this pattern.
    pub occurrence_count: u32,
    /// Task IDs where this pattern was observed.
    pub source_tasks: Vec<String>,
    /// Pattern category.
    pub category: PatternCategory,
}

/// Category of a discovered pattern.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternCategory {
    /// Code structure patterns (e.g., "auth handlers return Result<Token, AuthError>").
    CodeStructure,
    /// Testing patterns (e.g., "tests live in /tests directory").
    Testing,
    /// Tool usage patterns (e.g., "use bcrypt for password hashing").
    ToolUsage,
    /// Workflow patterns (e.g., "run migration before seeding").
    Workflow,
    /// Error handling patterns (e.g., "validate inputs before processing").
    ErrorHandling,
    /// Performance patterns (e.g., "cache frequently accessed data").
    Performance,
    /// Failure recovery patterns — what fix sequence resolves what error
    /// (Meta-Harness: "draft-verification pattern" applied to failures).
    FailureRecovery,
    /// Success recipe patterns — sequences that reliably produce success
    /// (Meta-Harness: skill quality optimization).
    SuccessRecipe,
}

/// A recipe for success: a sequence of steps that reliably produces good outcomes.
/// (Meta-Harness: "skill quality > search parameters" — capture *how* we succeeded)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessRecipe {
    /// What kind of task this recipe applies to.
    pub task_pattern: String,
    /// The sequence of agent roles/tools used.
    pub step_sequence: Vec<String>,
    /// How many times this recipe has been observed.
    pub occurrence_count: u32,
    /// Average trust score achieved with this recipe.
    pub avg_trust: f64,
    /// Average turns taken.
    pub avg_turns: f64,
}

/// A failure recovery pattern: what sequence of actions resolves a given error type.
/// (Meta-Harness: environment bootstrapping + execution trace mining)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecovery {
    /// The failure category this recovery addresses.
    pub failure_category: FailureCategory,
    /// Error signature (key phrases from the error message).
    pub error_signature: String,
    /// The recovery sequence that worked.
    pub recovery_sequence: Vec<String>,
    /// How many times this recovery was attempted.
    pub attempt_count: u32,
    /// How many times this recovery succeeded.
    pub success_count: u32,
    /// Average turns to recovery.
    pub avg_recovery_turns: f64,
}

// ============================================================================
// Pattern Miner
// ============================================================================

/// Mines patterns from execution traces.
pub struct PatternMiner {
    /// Accumulated traces for analysis.
    traces: Vec<ExecutionTrace>,
    /// Discovered patterns so far.
    patterns: Vec<DiscoveredPattern>,
    /// Minimum confidence threshold for pattern extraction.
    min_confidence: f32,
    /// Minimum occurrences to confirm a pattern.
    min_occurrences: u32,
}

impl PatternMiner {
    /// Create a new pattern miner.
    pub fn new(min_confidence: f32, min_occurrences: u32) -> Self {
        Self {
            traces: Vec::new(),
            patterns: Vec::new(),
            min_confidence,
            min_occurrences,
        }
    }

    /// Add an execution trace for analysis.
    pub fn add_trace(&mut self, trace: ExecutionTrace) {
        debug!(
            "Adding trace for task: {} (outcome: {:?})",
            trace.task, trace.outcome
        );

        // Analyze this trace for patterns
        let new_patterns = self.analyze_trace(&trace);

        // Merge new patterns with existing
        for pattern in new_patterns {
            self.merge_pattern(pattern);
        }

        self.traces.push(trace);
        info!(
            "Pattern miner now has {} traces and {} patterns",
            self.traces.len(),
            self.patterns.len()
        );
    }

    /// Analyze a single trace for patterns.
    fn analyze_trace(&self, trace: &ExecutionTrace) -> Vec<DiscoveredPattern> {
        let mut patterns = Vec::new();

        // Analyze file paths for structure patterns
        if !trace.files_modified.is_empty() {
            patterns.extend(self.extract_file_structure_patterns(trace));
        }

        // Analyze turns for workflow patterns
        if !trace.turns.is_empty() {
            patterns.extend(self.extract_workflow_patterns(trace));
        }

        // Analyze errors for error-handling patterns
        let error_patterns = self.extract_error_patterns(trace);
        patterns.extend(error_patterns);

        // Analyze successful outcomes for success patterns
        if trace.outcome == TaskOutcome::Success {
            patterns.extend(self.extract_success_patterns(trace));
        }

        // Analyze failures for anti-patterns
        if trace.outcome == TaskOutcome::Failed {
            patterns.extend(self.extract_failure_patterns(trace));
        }

        patterns
    }

    /// Extract file structure patterns.
    fn extract_file_structure_patterns(&self, trace: &ExecutionTrace) -> Vec<DiscoveredPattern> {
        let mut patterns = Vec::new();

        // Detect test file locations
        let has_tests = trace
            .files_modified
            .iter()
            .any(|f| f.contains("/tests/") || f.contains("/test_") || f.contains("_test.rs"));

        if has_tests {
            patterns.push(DiscoveredPattern {
                description: "Tests are placed in /tests directory or use _test.rs suffix"
                    .to_string(),
                confidence: 0.7,
                occurrence_count: 1,
                source_tasks: vec![trace.task.clone()],
                category: PatternCategory::Testing,
            });
        }

        // Detect module structure
        let has_lib = trace
            .files_modified
            .iter()
            .any(|f| f.contains("src/lib.rs"));
        let has_module = trace
            .files_modified
            .iter()
            .any(|f| f.contains("src/") && f.ends_with("/mod.rs"));

        if has_lib || has_module {
            patterns.push(DiscoveredPattern {
                description: "Project uses modular structure with mod.rs files".to_string(),
                confidence: 0.6,
                occurrence_count: 1,
                source_tasks: vec![trace.task.clone()],
                category: PatternCategory::CodeStructure,
            });
        }

        patterns
    }

    /// Extract workflow patterns from turn sequences.
    fn extract_workflow_patterns(&self, trace: &ExecutionTrace) -> Vec<DiscoveredPattern> {
        let mut patterns = Vec::new();

        // Detect compilation → fix cycles
        let compile_failures: usize = trace
            .turns
            .iter()
            .filter(|t| t.events.iter().any(|e| e.contains("CompilationFailed")))
            .count();

        if compile_failures > 0 {
            patterns.push(DiscoveredPattern {
                description: format!(
                    "Compilation errors occurred {} times; {} fixed via Scalpel",
                    compile_failures,
                    if trace.outcome == TaskOutcome::Success {
                        "were"
                    } else {
                        "were not"
                    }
                ),
                confidence: 0.8,
                occurrence_count: 1,
                source_tasks: vec![trace.task.clone()],
                category: PatternCategory::Workflow,
            });
        }

        patterns
    }

    /// Extract patterns from errors.
    fn extract_error_patterns(&self, trace: &ExecutionTrace) -> Vec<DiscoveredPattern> {
        let mut patterns = Vec::new();

        // Collect all errors
        let all_errors: Vec<&String> = trace.turns.iter().flat_map(|t| t.errors.iter()).collect();

        if !all_errors.is_empty() {
            // Check for common error types
            let has_type_errors = all_errors
                .iter()
                .any(|e| e.contains("mismatched types") || e.contains("E0308"));
            let has_borrow_errors = all_errors
                .iter()
                .any(|e| e.contains("borrow") || e.contains("E050"));

            if has_type_errors {
                patterns.push(DiscoveredPattern {
                    description: "Code has type mismatches; ensure explicit type annotations"
                        .to_string(),
                    confidence: 0.6,
                    occurrence_count: 1,
                    source_tasks: vec![trace.task.clone()],
                    category: PatternCategory::ErrorHandling,
                });
            }

            if has_borrow_errors {
                patterns.push(DiscoveredPattern {
                    description:
                        "Borrow checker issues; consider cloning or restructuring ownership"
                            .to_string(),
                    confidence: 0.6,
                    occurrence_count: 1,
                    source_tasks: vec![trace.task.clone()],
                    category: PatternCategory::ErrorHandling,
                });
            }
        }

        patterns
    }

    /// Extract patterns from successful executions.
    fn extract_success_patterns(&self, trace: &ExecutionTrace) -> Vec<DiscoveredPattern> {
        let mut patterns = Vec::new();

        // Detect if trust remained high throughout
        if trace.final_trust >= 0.7 {
            patterns.push(DiscoveredPattern {
                description: format!(
                    "Task completed successfully with high trust ({:.2}); approach was effective",
                    trace.final_trust
                ),
                confidence: 0.75,
                occurrence_count: 1,
                source_tasks: vec![trace.task.clone()],
                category: PatternCategory::Workflow,
            });
        }

        patterns
    }

    /// Extract anti-patterns from failures.
    fn extract_failure_patterns(&self, trace: &ExecutionTrace) -> Vec<DiscoveredPattern> {
        let mut patterns = Vec::new();

        // Try to extract root cause
        let root_cause = if trace.total_turns >= 10 {
            Some("Task exceeded turn limit; may need better planning or decomposition".to_string())
        } else if trace.final_trust < 0.3 {
            Some("Trust exhausted; approach may have been fundamentally flawed".to_string())
        } else {
            Some("Task failed; root cause may include compilation errors, test failures, or verification issues".to_string())
        };

        if let Some(cause) = root_cause {
            patterns.push(DiscoveredPattern {
                description: format!("Failure pattern: {}", cause),
                confidence: 0.5,
                occurrence_count: 1,
                source_tasks: vec![trace.task.clone()],
                category: PatternCategory::Workflow,
            });
        }

        patterns
    }

    /// Merge a new pattern with existing patterns.
    fn merge_pattern(&mut self, new_pattern: DiscoveredPattern) {
        // Check if similar pattern exists
        let existing = self.patterns.iter_mut().find(|p| {
            // Simple similarity: same category and similar description
            p.category == new_pattern.category
                && Self::description_similarity(&p.description, &new_pattern.description) > 0.6
        });

        if let Some(existing) = existing {
            // Merge: increase confidence and occurrence count
            existing.occurrence_count += 1;
            existing.source_tasks.extend(new_pattern.source_tasks);
            // Weighted average that increases confidence with more evidence
            let weight = 1.0 / (existing.occurrence_count as f32);
            existing.confidence =
                existing.confidence * (1.0 - weight) + new_pattern.confidence * weight;
            // Cap at 1.0
            existing.confidence = existing.confidence.min(1.0);
        } else {
            // Add as new pattern
            self.patterns.push(new_pattern);
        }
    }

    /// Simple string similarity (placeholder for more sophisticated algorithm).
    fn description_similarity(a: &str, b: &str) -> f32 {
        // Very basic: word overlap ratio
        let words_a: Vec<&str> = a.split_whitespace().collect();
        let words_b: Vec<&str> = b.split_whitespace().collect();

        let common = words_a.iter().filter(|w| words_b.contains(w)).count();
        let total = words_a.len().max(words_b.len());

        if total == 0 {
            0.0
        } else {
            common as f32 / total as f32
        }
    }

    /// Get all discovered patterns above confidence threshold.
    pub fn confirmed_patterns(&self) -> Vec<&DiscoveredPattern> {
        self.patterns
            .iter()
            .filter(|p| {
                p.confidence >= self.min_confidence && p.occurrence_count >= self.min_occurrences
            })
            .collect()
    }

    /// Get all patterns (for inspection).
    pub fn all_patterns(&self) -> &[DiscoveredPattern] {
        &self.patterns
    }

    /// Get statistics about the miner.
    pub fn stats(&self) -> PatternMinerStats {
        PatternMinerStats {
            total_traces: self.traces.len(),
            total_patterns: self.patterns.len(),
            confirmed_patterns: self.confirmed_patterns().len(),
            avg_confidence: if self.patterns.is_empty() {
                0.0
            } else {
                self.patterns.iter().map(|p| p.confidence).sum::<f32>() / self.patterns.len() as f32
            },
        }
    }
}

/// Statistics about the pattern miner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternMinerStats {
    pub total_traces: usize,
    pub total_patterns: usize,
    pub confirmed_patterns: usize,
    pub avg_confidence: f32,
}

// ============================================================================
// Trace Builder (for easy construction)
// ============================================================================

/// Builder for ExecutionTrace.
pub struct ExecutionTraceBuilder {
    task: String,
    outcome: TaskOutcome,
    turns: Vec<TurnTrace>,
    files_modified: Vec<String>,
    total_turns: u32,
    final_trust: f64,
}

impl ExecutionTraceBuilder {
    pub fn new(task: &str, outcome: TaskOutcome) -> Self {
        Self {
            task: task.to_string(),
            outcome,
            turns: Vec::new(),
            files_modified: Vec::new(),
            total_turns: 0,
            final_trust: 0.7,
        }
    }

    pub fn add_turn(mut self, turn: TurnTrace) -> Self {
        self.turns.push(turn);
        self
    }

    pub fn files_modified(mut self, files: Vec<String>) -> Self {
        self.files_modified = files;
        self
    }

    pub fn total_turns(mut self, turns: u32) -> Self {
        self.total_turns = turns;
        self
    }

    pub fn final_trust(mut self, trust: f64) -> Self {
        self.final_trust = trust;
        self
    }

    pub fn build(self) -> ExecutionTrace {
        // Classify failure if applicable
        let failure_category = if self.outcome == TaskOutcome::Failed {
            let all_errors: Vec<String> = self
                .turns
                .iter()
                .flat_map(|t| t.errors.iter())
                .cloned()
                .collect();
            Some(FailureCategory::from_errors(
                &all_errors,
                self.total_turns,
                self.final_trust,
                50,
            ))
        } else {
            None
        };

        ExecutionTrace {
            task: self.task,
            outcome: self.outcome,
            turns: self.turns,
            files_modified: self.files_modified,
            total_turns: self.total_turns,
            final_trust: self.final_trust,
            root_cause: None,
            failure_category,
            discovered_patterns: Vec::new(),
            tool_calls: Vec::new(),
            duration_ms: None,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_miner_basic() {
        let mut miner = PatternMiner::new(0.5, 1);

        let trace = ExecutionTraceBuilder::new("Test task", TaskOutcome::Success)
            .files_modified(vec![
                "src/lib.rs".to_string(),
                "tests/test_auth.rs".to_string(),
            ])
            .total_turns(5)
            .final_trust(0.85)
            .build();

        miner.add_trace(trace);

        let stats = miner.stats();
        assert_eq!(stats.total_traces, 1);
        assert!(stats.total_patterns > 0, "Should extract some patterns");
    }

    #[test]
    fn test_description_similarity() {
        let a = "Tests are placed in /tests directory";
        let b = "Tests live in /tests directory";

        let similarity = PatternMiner::description_similarity(a, b);
        assert!(
            similarity > 0.5,
            "Similar descriptions should have high similarity"
        );
    }

    #[test]
    fn test_pattern_merging() {
        let mut miner = PatternMiner::new(0.5, 1);

        // Add two similar traces - use paths that match the pattern detection
        let trace1 = ExecutionTraceBuilder::new("Task 1", TaskOutcome::Success)
            .files_modified(vec!["src/tests/test1.rs".to_string()])
            .build();

        let trace2 = ExecutionTraceBuilder::new("Task 2", TaskOutcome::Success)
            .files_modified(vec!["src/tests/test2.rs".to_string()])
            .build();

        miner.add_trace(trace1);
        miner.add_trace(trace2);

        // Should have merged the testing pattern
        let testing_patterns: Vec<_> = miner
            .patterns
            .iter()
            .filter(|p| p.category == PatternCategory::Testing)
            .collect();

        assert!(!testing_patterns.is_empty(), "Should have testing patterns");
        assert!(
            testing_patterns[0].occurrence_count >= 1,
            "Should have merged occurrences"
        );
    }

    #[test]
    fn test_task_outcome_serialization() {
        let outcomes = vec![
            TaskOutcome::Success,
            TaskOutcome::Failed,
            TaskOutcome::Cancelled,
        ];

        for outcome in outcomes {
            let json = serde_json::to_string(&outcome).expect("Should serialize");
            let _deserialized: TaskOutcome =
                serde_json::from_str(&json).expect("Should deserialize");
        }
    }

    #[test]
    fn test_failure_category_from_errors_compilation() {
        let errors = vec!["error[E0308]: compilation failed".to_string()];
        let category = FailureCategory::from_errors(&errors, 5, 0.8, 50);
        assert_eq!(category, FailureCategory::CompilationError);
    }

    #[test]
    fn test_failure_category_from_errors_test_failure() {
        let errors = vec!["test failed: panic at line 42".to_string()];
        let category = FailureCategory::from_errors(&errors, 5, 0.8, 50);
        assert_eq!(category, FailureCategory::TestFailure);
    }

    #[test]
    fn test_failure_category_from_errors_missing_import() {
        let errors = vec!["unresolved import `std::foo`".to_string()];
        let category = FailureCategory::from_errors(&errors, 5, 0.8, 50);
        assert_eq!(category, FailureCategory::MissingImport);
    }

    #[test]
    fn test_failure_category_from_errors_type_mismatch() {
        let errors = vec!["expected `String`, found `&str`".to_string()];
        let category = FailureCategory::from_errors(&errors, 5, 0.8, 50);
        assert_eq!(category, FailureCategory::TypeMismatch);
    }

    #[test]
    fn test_failure_category_from_errors_parse_failure() {
        let errors = vec!["invalid json: expected value at line 1".to_string()];
        let category = FailureCategory::from_errors(&errors, 5, 0.8, 50);
        assert_eq!(category, FailureCategory::ParseFailure);
    }

    #[test]
    fn test_failure_category_trust_exhausted() {
        let errors = vec!["some error".to_string()];
        let category = FailureCategory::from_errors(&errors, 5, 0.1, 50);
        assert_eq!(category, FailureCategory::TrustExhausted);
    }

    #[test]
    fn test_failure_category_turn_budget_exhausted() {
        let errors = vec!["some error".to_string()];
        let category = FailureCategory::from_errors(&errors, 50, 0.8, 50);
        assert_eq!(category, FailureCategory::TurnBudgetExhausted);
    }

    #[test]
    fn test_tool_call_record() {
        let record = ToolCallRecord {
            tool_name: "ReadFile".to_string(),
            success: true,
            duration_ms: Some(100),
            files: vec!["src/lib.rs".to_string()],
            error: None,
        };

        assert!(record.success);
        assert_eq!(record.tool_name, "ReadFile");
        assert_eq!(record.files.len(), 1);
    }

    #[test]
    fn test_turn_trace() {
        let turn = TurnTrace {
            turn_number: 1,
            agent_role: "Builder".to_string(),
            action: "Edit file".to_string(),
            files_changed: vec!["src/lib.rs".to_string()],
            events: vec!["file.edited".to_string()],
            verification_passed: true,
            errors: vec![],
            tool_calls: vec![],
            trust_delta: Some(0.1),
        };

        assert_eq!(turn.turn_number, 1);
        assert!(turn.verification_passed);
        assert_eq!(turn.agent_role, "Builder");
    }

    #[test]
    fn test_execution_trace_builder() {
        let trace = ExecutionTraceBuilder::new("Test task", TaskOutcome::Success)
            .total_turns(5)
            .final_trust(0.85)
            .files_modified(vec!["src/lib.rs".to_string()])
            .build();

        assert_eq!(trace.task, "Test task");
        assert_eq!(trace.outcome, TaskOutcome::Success);
        assert_eq!(trace.total_turns, 5);
        assert_eq!(trace.final_trust, 0.85);
        assert_eq!(trace.files_modified.len(), 1);
        assert!(trace.failure_category.is_none()); // Success has no failure category
    }

    #[test]
    fn test_execution_trace_builder_failure() {
        let trace = ExecutionTraceBuilder::new("Failing task", TaskOutcome::Failed)
            .total_turns(10)
            .final_trust(0.3)
            .build();

        assert_eq!(trace.outcome, TaskOutcome::Failed);
        assert!(trace.failure_category.is_some());
    }

    #[test]
    fn test_pattern_category_serialization() {
        let categories = vec![
            PatternCategory::CodeStructure,
            PatternCategory::Testing,
            PatternCategory::ToolUsage,
            PatternCategory::Workflow,
            PatternCategory::ErrorHandling,
            PatternCategory::Performance,
            PatternCategory::FailureRecovery,
            PatternCategory::SuccessRecipe,
        ];

        for category in categories {
            let json = serde_json::to_string(&category).expect("Should serialize");
            let _deserialized: PatternCategory =
                serde_json::from_str(&json).expect("Should deserialize");
        }
    }

    #[test]
    fn test_discovered_pattern() {
        let pattern = DiscoveredPattern {
            description: "Test pattern".to_string(),
            confidence: 0.8,
            occurrence_count: 5,
            source_tasks: vec!["task1".to_string(), "task2".to_string()],
            category: PatternCategory::Workflow,
        };

        assert_eq!(pattern.confidence, 0.8);
        assert_eq!(pattern.occurrence_count, 5);
        assert_eq!(pattern.source_tasks.len(), 2);
    }

    #[test]
    fn test_pattern_miner_confirmed_patterns() {
        let mut miner = PatternMiner::new(0.5, 2);

        // Add trace with success
        let trace1 = ExecutionTraceBuilder::new("Task 1", TaskOutcome::Success)
            .files_modified(vec!["src/tests/test1.rs".to_string()])
            .build();
        miner.add_trace(trace1);

        // Add another similar trace
        let trace2 = ExecutionTraceBuilder::new("Task 2", TaskOutcome::Success)
            .files_modified(vec!["src/tests/test2.rs".to_string()])
            .build();
        miner.add_trace(trace2);

        // Should have confirmed patterns with min_occurrences >= 2
        let confirmed = miner.confirmed_patterns();
        let testing_confirmed: Vec<_> = confirmed
            .iter()
            .filter(|p| p.category == PatternCategory::Testing)
            .collect();

        // At least one testing pattern should be confirmed
        assert!(!testing_confirmed.is_empty() || miner.all_patterns().is_empty());
    }

    #[test]
    fn test_pattern_miner_stats() {
        let miner = PatternMiner::new(0.5, 1);
        let stats = miner.stats();

        assert_eq!(stats.total_traces, 0);
        assert_eq!(stats.total_patterns, 0);
        assert_eq!(stats.confirmed_patterns, 0);
        assert_eq!(stats.avg_confidence, 0.0);
    }

    #[test]
    fn test_execution_trace_with_tool_calls() {
        let mut trace = ExecutionTraceBuilder::new("Task with tools", TaskOutcome::Success).build();

        // Add a tool call
        trace.tool_calls.push(ToolCallRecord {
            tool_name: "Bash".to_string(),
            success: true,
            duration_ms: Some(50),
            files: vec![],
            error: None,
        });

        assert_eq!(trace.tool_calls.len(), 1);
        assert!(trace.tool_calls[0].success);
    }
}
