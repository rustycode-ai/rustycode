//! Delegation decision module for task spawning and ensemble planning.
//!
//! Determines whether a sub-task should be executed inline, spawned as a
//! separate task, parallelised across modules, or handed to an ensemble
//! strategy. The three-gate decision model considers context pressure,
//! task complexity, and past failure history.

use crate::ensemble_strategy::{EnsembleStrategy, ParticipantSpec, StrategyKind};
use crate::strategy_selector::StrategySelector;
use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// TaskRole
// ---------------------------------------------------------------------------

/// Semantic role assigned to a delegated task.
///
/// Drives default execution tier, write/bash permissions, and prompt framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRole {
    Explore,
    Research,
    Code,
    Review,
    Verify,
    Plan,
    Debug,
}

impl TaskRole {
    /// Default execution tier for this role.
    pub const fn default_tier(self) -> ExecutionTier {
        match self {
            Self::Explore | Self::Research => ExecutionTier::Musician,
            Self::Code | Self::Review | Self::Verify | Self::Debug => ExecutionTier::Editor,
            Self::Plan => ExecutionTier::Composer,
        }
    }

    /// Whether tasks with this role need filesystem write access.
    pub const fn needs_write_access(self) -> bool {
        matches!(self, Self::Code | Self::Verify)
    }

    /// Whether tasks with this role need bash/command execution.
    pub const fn needs_bash(self) -> bool {
        matches!(self, Self::Code | Self::Verify | Self::Debug)
    }

    /// System prompt fragment for this role.
    pub const fn system_prompt(self) -> &'static str {
        match self {
            Self::Explore => "You are an exploration agent. Search the codebase, find relevant files, patterns, and implementation details. Return concise findings with file paths and line numbers. Do NOT modify any files.",
            Self::Research => "You are a research agent. Investigate external documentation, APIs, and best practices. Synthesize findings into actionable recommendations. Do NOT modify any files.",
            Self::Code => "You are a coding agent. Implement the specified changes following existing codebase patterns. Write clean, idiomatic code with proper error handling. You may read and write files.",
            Self::Review => "You are a code review agent. Analyze the specified code for correctness, security, performance, and style. Provide specific, actionable feedback with file paths and line numbers. Do NOT modify any files.",
            Self::Verify => "You are a verification agent. Run tests, validate behavior, and confirm that changes work as expected. Report pass/fail results with evidence. You may execute commands and read files.",
            Self::Plan => "You are a planning agent. Analyze requirements and produce a detailed implementation plan with specific steps, file locations, and dependencies. Do NOT modify any files.",
            Self::Debug => "You are a debugging agent. Investigate the reported issue systematically. Identify root cause, propose minimal fix, and verify the fix resolves the issue. You may execute commands and read files.",
        }
    }

    /// Tools this role is allowed to use.
    pub const fn allowed_tools(self) -> &'static [&'static str] {
        match self {
            Self::Explore | Self::Research | Self::Review | Self::Plan => {
                &["read_file", "search_files", "list_directory", "glob"]
            }
            Self::Code => &[
                "read_file",
                "write_file",
                "edit_file",
                "search_files",
                "list_directory",
                "glob",
                "bash",
            ],
            Self::Verify | Self::Debug => &[
                "read_file",
                "search_files",
                "list_directory",
                "glob",
                "bash",
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// TaskSpec
// ---------------------------------------------------------------------------

/// Full specification for a delegated (spawned) task.
#[derive(Debug, Clone)]
pub struct TaskSpec {
    /// Unique identifier within the delegation scope.
    pub task_id: String,
    /// The prompt / instruction the spawned agent should follow.
    pub prompt: String,
    /// Semantic role driving defaults and permissions.
    pub role: TaskRole,
    /// Optional prior checkpoint or state to resume from.
    pub resume_from: Option<String>,
    /// Filesystem paths this task is allowed to touch.
    pub path_scope: Vec<PathBuf>,
    /// Override the role's default execution tier.
    pub tier_override: Option<ExecutionTier>,
    /// Maximum budget (USD) the task may consume.
    pub budget_limit: f64,
    /// Optional hard cap on the number of agent steps.
    pub max_steps: Option<u32>,
}

impl TaskSpec {
    /// Create a new task spec with an auto-generated ID.
    pub fn new(prompt: impl Into<String>, role: TaskRole) -> Self {
        let task_id = format!(
            "task-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis())
        );
        Self {
            task_id,
            prompt: prompt.into(),
            role,
            resume_from: None,
            path_scope: Vec::new(),
            tier_override: None,
            budget_limit: 1.0,
            max_steps: None,
        }
    }

    /// Set the resume checkpoint.
    pub fn with_resume_from(mut self, checkpoint: impl Into<String>) -> Self {
        self.resume_from = Some(checkpoint.into());
        self
    }

    /// Add a single path to the scope.
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path_scope.push(path);
        self
    }

    /// Override the default execution tier.
    pub const fn with_tier_override(mut self, tier: ExecutionTier) -> Self {
        self.tier_override = Some(tier);
        self
    }

    /// Set budget limit.
    pub const fn with_budget_limit(mut self, limit: f64) -> Self {
        self.budget_limit = limit;
        self
    }

    /// Set max steps cap.
    pub const fn with_max_steps(mut self, steps: u32) -> Self {
        self.max_steps = Some(steps);
        self
    }

    /// Effective execution tier (override or role default).
    pub fn effective_tier(&self) -> ExecutionTier {
        self.tier_override
            .unwrap_or_else(|| self.role.default_tier())
    }
}

// ---------------------------------------------------------------------------
// EnsemblePlan
// ---------------------------------------------------------------------------

/// Plan for an ensemble strategy execution with mapped participants.
#[derive(Debug, Clone)]
pub struct EnsemblePlan {
    /// Which ensemble strategy to use.
    pub strategy: StrategyKind,
    /// Participant specs paired with their task assignments.
    pub participants: Vec<(ParticipantSpec, TaskSpec)>,
}

// ---------------------------------------------------------------------------
// SpawnDecision
// ---------------------------------------------------------------------------

/// Decision outcome from the delegation planner.
#[derive(Debug, Clone)]
pub enum SpawnDecision {
    /// Execute inline in the current agent context.
    Inline,
    /// Spawn a single child task.
    Spawn(TaskSpec),
    /// Spawn multiple independent tasks in parallel.
    SpawnParallel(Vec<TaskSpec>),
    /// Hand off to an ensemble strategy.
    Ensemble(EnsemblePlan),
}

// ---------------------------------------------------------------------------
// DelegationContext
// ---------------------------------------------------------------------------

/// Snapshot of the current execution context used for delegation decisions.
#[derive(Debug, Clone)]
pub struct DelegationContext {
    /// 0.0–1.0 indicating how much of the context window is consumed.
    pub context_pressure: f64,
    /// Remaining budget (USD) in the current session.
    pub remaining_budget: f64,
    /// Filesystem paths the current task touches.
    pub affected_paths: Vec<PathBuf>,
    /// How many times related tasks have failed previously.
    pub past_failure_count: usize,
    /// ID of the parent task for tracing.
    pub parent_task_id: String,
}

impl DelegationContext {
    /// Create a context suitable for tool-initiated delegation.
    ///
    /// Uses sensible defaults for fields that aren't available at the
    /// tool call boundary (context pressure, budget, failure count).
    pub fn for_tool_call(cwd: &std::path::Path, parent_task_id: impl Into<String>) -> Self {
        Self {
            context_pressure: 0.5,
            remaining_budget: 5.0,
            affected_paths: vec![cwd.to_path_buf()],
            past_failure_count: 0,
            parent_task_id: parent_task_id.into(),
        }
    }

    /// Extract unique top-level module directories from affected paths.
    ///
    /// For `/foo/bar/baz.rs` and `/foo/qux.rs`, returns `["foo"]`.
    /// For `/alpha/beta.rs` and `/gamma/delta.rs`, returns `["alpha", "gamma"]`.
    pub fn affected_modules(&self) -> Vec<String> {
        let mut modules: Vec<String> = self
            .affected_paths
            .iter()
            .filter_map(|p| {
                // Take the first meaningful directory component after root.
                p.iter()
                    .nth(1) // skip the leading "/" or ""
                    .map(|c| c.to_string_lossy().into_owned())
            })
            .collect();

        modules.sort();
        modules.dedup();
        modules
    }
}

// ---------------------------------------------------------------------------
// DelegationConfig
// ---------------------------------------------------------------------------

/// Tunable thresholds for the delegation planner.
#[derive(Debug, Clone)]
pub struct DelegationConfig {
    /// Context pressure above which we always spawn (0.0–1.0).
    pub context_pressure_threshold: f64,
    /// Complexity score above which we consider spawning.
    pub spawn_complexity_threshold: f64,
    /// Whether parallel spawning is permitted.
    pub allow_parallel: bool,
    /// Maximum number of concurrent child tasks.
    pub max_concurrent_tasks: usize,
}

impl Default for DelegationConfig {
    fn default() -> Self {
        Self {
            context_pressure_threshold: 0.75,
            spawn_complexity_threshold: 3.0,
            allow_parallel: true,
            max_concurrent_tasks: 4,
        }
    }
}

// ---------------------------------------------------------------------------
// DelegationPlanner
// ---------------------------------------------------------------------------

/// Three-gate delegation decision engine.
///
/// Gate 1 — Context pressure: if the context window is nearly full, spawn
///           to a fresh context regardless of complexity.
/// Gate 2 — Complexity: if the task is complex enough, spawn in parallel
///           when multiple modules are affected, or as a single spawn otherwise.
/// Gate 3 — Ensemble: very high complexity + past failures triggers an
///           ensemble strategy for adversarial or voting-based execution.
pub struct DelegationPlanner {
    #[allow(dead_code)] // Reserved for future instance-level strategy selection
    strategy_selector: StrategySelector,
    config: DelegationConfig,
}

impl DelegationPlanner {
    /// Create a new planner with the given configuration.
    pub const fn new(config: DelegationConfig) -> Self {
        Self {
            strategy_selector: StrategySelector::new(),
            config,
        }
    }

    /// Evaluate the three gates and return a delegation decision.
    pub fn should_spawn(
        &self,
        task_description: &str,
        context: &DelegationContext,
    ) -> SpawnDecision {
        let complexity = StrategySelector::detect_complexity(task_description);

        // Gate 1: context pressure override — always spawn to get a fresh window.
        if context.context_pressure >= self.config.context_pressure_threshold {
            let role = infer_role_from_description(task_description);
            let spec =
                TaskSpec::new(task_description, role).with_budget_limit(context.remaining_budget);
            return SpawnDecision::Spawn(spec);
        }

        // Gate 3: very high complexity + past failures → ensemble.
        if complexity >= 4.5 && context.past_failure_count > 0 {
            return self.plan_ensemble(task_description, context);
        }

        // Gate 2: high enough complexity to warrant spawning.
        if complexity >= self.config.spawn_complexity_threshold {
            let modules = context.affected_modules();
            if self.config.allow_parallel && modules.len() > 1 {
                return self.decompose_parallel(task_description, context, &modules);
            }
            let role = infer_role_from_description(task_description);
            let spec =
                TaskSpec::new(task_description, role).with_budget_limit(context.remaining_budget);
            return SpawnDecision::Spawn(spec);
        }

        // Below all thresholds — execute inline.
        SpawnDecision::Inline
    }

    /// Decompose a high-complexity task into parallel sub-tasks by module.
    fn decompose_parallel(
        &self,
        task_description: &str,
        context: &DelegationContext,
        modules: &[String],
    ) -> SpawnDecision {
        let role = infer_role_from_description(task_description);
        let count = modules.len().min(self.config.max_concurrent_tasks);
        let budget_per_task = context.remaining_budget / (count.max(1) as f64);

        let specs: Vec<TaskSpec> = modules
            .iter()
            .take(self.config.max_concurrent_tasks)
            .map(|module| {
                let scoped_paths: Vec<PathBuf> = context
                    .affected_paths
                    .iter()
                    .filter(|p| p.iter().any(|c| c.to_string_lossy() == *module))
                    .cloned()
                    .collect();

                let mut spec = TaskSpec::new(format!("[{module}] {task_description}"), role)
                    .with_budget_limit(budget_per_task);
                spec.path_scope = scoped_paths;
                spec
            })
            .collect();

        if specs.is_empty() {
            let spec =
                TaskSpec::new(task_description, role).with_budget_limit(context.remaining_budget);
            return SpawnDecision::Spawn(spec);
        }

        SpawnDecision::SpawnParallel(specs)
    }

    #[allow(clippy::unused_self)]
    fn plan_ensemble(&self, task_description: &str, context: &DelegationContext) -> SpawnDecision {
        // Map complexity (0.0–5.0) to u8 (0–100) for EnsembleStrategy.
        let complexity_u8 =
            (StrategySelector::detect_complexity(task_description) / 5.0 * 100.0).min(100.0) as u8;

        let ensemble = EnsembleStrategy::select_for_complexity(complexity_u8);
        let strategy_kind = ensemble.kind();
        let participants = ensemble.participants().to_vec();

        let role = infer_role_from_description(task_description);
        let budget_per_participant = context.remaining_budget / (participants.len().max(1) as f64);

        let paired: Vec<(ParticipantSpec, TaskSpec)> = participants
            .into_iter()
            .map(|spec| {
                let task = TaskSpec::new(format!("[{}] {}", spec.role, task_description), role)
                    .with_budget_limit(budget_per_participant);
                (spec, task)
            })
            .collect();

        SpawnDecision::Ensemble(EnsemblePlan {
            strategy: strategy_kind,
            participants: paired,
        })
    }
}

// ---------------------------------------------------------------------------
// Role inference
// ---------------------------------------------------------------------------

/// Infer a task role from the description text using keyword matching.
pub fn infer_role_from_description(description: &str) -> TaskRole {
    let lower = description.to_lowercase();

    // Order matters: more specific patterns first.
    if contains_any(
        &lower,
        &["implement", "create", "build", "add", "write", "develop"],
    ) {
        return TaskRole::Code;
    }
    if contains_any(
        &lower,
        &["research", "docs", "documentation", "read up", "look up"],
    ) {
        return TaskRole::Research;
    }
    if contains_any(
        &lower,
        &["debug", "fix", "investigate", "troubleshoot", "resolve"],
    ) {
        return TaskRole::Debug;
    }
    if contains_any(&lower, &["review", "check", "audit", "inspect", "examine"]) {
        return TaskRole::Review;
    }
    if contains_any(&lower, &["verify", "test", "validate", "confirm"]) {
        return TaskRole::Verify;
    }
    if contains_any(
        &lower,
        &["plan", "design", "architect", "blueprint", "specify"],
    ) {
        return TaskRole::Plan;
    }
    if contains_any(&lower, &["explore", "find", "search", "scan", "discover"]) {
        return TaskRole::Explore;
    }

    // Default to research for ambiguous tasks.
    TaskRole::Research
}

/// Check whether `text` contains any of the given keywords.
fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(*kw))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ---- TaskRole::default_tier ----

    #[test]
    fn task_role_default_tier_explore_is_musician() {
        assert_eq!(TaskRole::Explore.default_tier(), ExecutionTier::Musician);
    }

    #[test]
    fn task_role_default_tier_research_is_musician() {
        assert_eq!(TaskRole::Research.default_tier(), ExecutionTier::Musician);
    }

    #[test]
    fn task_role_default_tier_code_is_editor() {
        assert_eq!(TaskRole::Code.default_tier(), ExecutionTier::Editor);
    }

    #[test]
    fn task_role_default_tier_review_is_editor() {
        assert_eq!(TaskRole::Review.default_tier(), ExecutionTier::Editor);
    }

    #[test]
    fn task_role_default_tier_verify_is_editor() {
        assert_eq!(TaskRole::Verify.default_tier(), ExecutionTier::Editor);
    }

    #[test]
    fn task_role_default_tier_debug_is_editor() {
        assert_eq!(TaskRole::Debug.default_tier(), ExecutionTier::Editor);
    }

    #[test]
    fn task_role_default_tier_plan_is_composer() {
        assert_eq!(TaskRole::Plan.default_tier(), ExecutionTier::Composer);
    }

    // ---- TaskRole access helpers ----

    #[test]
    fn task_role_needs_write_access() {
        assert!(TaskRole::Code.needs_write_access());
        assert!(TaskRole::Verify.needs_write_access());
        assert!(!TaskRole::Explore.needs_write_access());
        assert!(!TaskRole::Research.needs_write_access());
        assert!(!TaskRole::Review.needs_write_access());
        assert!(!TaskRole::Plan.needs_write_access());
        assert!(!TaskRole::Debug.needs_write_access());
    }

    #[test]
    fn task_role_needs_bash() {
        assert!(TaskRole::Code.needs_bash());
        assert!(TaskRole::Verify.needs_bash());
        assert!(TaskRole::Debug.needs_bash());
        assert!(!TaskRole::Explore.needs_bash());
        assert!(!TaskRole::Research.needs_bash());
        assert!(!TaskRole::Review.needs_bash());
        assert!(!TaskRole::Plan.needs_bash());
    }

    // ---- DelegationConfig::default ----

    #[test]
    fn delegation_config_defaults() {
        let config = DelegationConfig::default();
        assert!((config.context_pressure_threshold - 0.75).abs() < f64::EPSILON);
        assert!((config.spawn_complexity_threshold - 3.0).abs() < f64::EPSILON);
        assert!(config.allow_parallel);
        assert_eq!(config.max_concurrent_tasks, 4);
    }

    // ---- infer_role_from_description ----

    #[test]
    fn infer_role_explore() {
        assert_eq!(
            infer_role_from_description("explore the codebase for auth patterns"),
            TaskRole::Explore
        );
        assert_eq!(
            infer_role_from_description("find the bug location"),
            TaskRole::Explore
        );
    }

    #[test]
    fn infer_role_research() {
        assert_eq!(
            infer_role_from_description("research best practices for error handling"),
            TaskRole::Research
        );
        assert_eq!(
            infer_role_from_description("read the docs on tokio"),
            TaskRole::Research
        );
    }

    #[test]
    fn infer_role_code() {
        assert_eq!(
            infer_role_from_description("implement user authentication"),
            TaskRole::Code
        );
        assert_eq!(
            infer_role_from_description("create a new handler"),
            TaskRole::Code
        );
        assert_eq!(
            infer_role_from_description("build the API endpoint"),
            TaskRole::Code
        );
        assert_eq!(
            infer_role_from_description("add logging to the module"),
            TaskRole::Code
        );
        assert_eq!(
            infer_role_from_description("write a new parser"),
            TaskRole::Code
        );
    }

    #[test]
    fn infer_role_review() {
        assert_eq!(
            infer_role_from_description("review the PR for security issues"),
            TaskRole::Review
        );
        assert_eq!(
            infer_role_from_description("check the code quality"),
            TaskRole::Review
        );
        assert_eq!(
            infer_role_from_description("audit the error handling"),
            TaskRole::Review
        );
    }

    #[test]
    fn infer_role_verify() {
        assert_eq!(
            infer_role_from_description("verify the test results"),
            TaskRole::Verify
        );
        assert_eq!(
            infer_role_from_description("test the new feature"),
            TaskRole::Verify
        );
        assert_eq!(
            infer_role_from_description("validate the input schema"),
            TaskRole::Verify
        );
    }

    #[test]
    fn infer_role_plan() {
        assert_eq!(
            infer_role_from_description("plan the migration strategy"),
            TaskRole::Plan
        );
        assert_eq!(
            infer_role_from_description("design the new architecture"),
            TaskRole::Plan
        );
        assert_eq!(
            infer_role_from_description("architect the service boundaries"),
            TaskRole::Plan
        );
    }

    #[test]
    fn infer_role_debug() {
        assert_eq!(
            infer_role_from_description("debug the race condition"),
            TaskRole::Debug
        );
        assert_eq!(
            infer_role_from_description("fix the broken test"),
            TaskRole::Debug
        );
        assert_eq!(
            infer_role_from_description("investigate the memory leak"),
            TaskRole::Debug
        );
    }

    #[test]
    fn infer_role_default_is_research() {
        assert_eq!(
            infer_role_from_description("something unrelated to keywords"),
            TaskRole::Research
        );
    }

    #[test]
    fn infer_role_case_insensitive() {
        assert_eq!(
            infer_role_from_description("IMPLEMENT the feature"),
            TaskRole::Code
        );
        assert_eq!(
            infer_role_from_description("DEBUG this issue"),
            TaskRole::Debug
        );
    }

    // ---- DelegationPlanner::should_spawn - inline ----

    #[test]
    fn should_spawn_inline_for_low_complexity() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![PathBuf::from("/foo/bar.rs")],
            past_failure_count: 0,
            parent_task_id: "parent-1".into(),
        };

        let decision = planner.should_spawn("fix a typo in readme", &context);
        assert!(matches!(decision, SpawnDecision::Inline));
    }

    // ---- DelegationPlanner::should_spawn - spawn ----

    #[test]
    fn should_spawn_single_for_high_complexity_single_module() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![PathBuf::from("/src/main.rs")],
            past_failure_count: 0,
            parent_task_id: "parent-2".into(),
        };

        // "explore" triggers complexity >= 4.5 which is above spawn threshold 3.0
        let decision = planner.should_spawn("explore the interpreter architecture", &context);
        assert!(matches!(decision, SpawnDecision::Spawn(_)));
    }

    // ---- DelegationPlanner::should_spawn - context pressure ----

    #[test]
    fn should_spawn_on_high_context_pressure() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.85,
            remaining_budget: 10.0,
            affected_paths: vec![],
            past_failure_count: 0,
            parent_task_id: "parent-3".into(),
        };

        // Even a simple task should spawn when context pressure is high.
        let decision = planner.should_spawn("fix a typo", &context);
        assert!(matches!(decision, SpawnDecision::Spawn(_)));
    }

    // ---- DelegationPlanner::should_spawn - ensemble ----

    #[test]
    fn should_spawn_ensemble_for_very_complex_with_failures() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![PathBuf::from("/src/main.rs")],
            past_failure_count: 2,
            parent_task_id: "parent-4".into(),
        };

        let decision =
            planner.should_spawn("explore and investigate the compiler failure", &context);
        assert!(matches!(decision, SpawnDecision::Ensemble(_)));
    }

    // ---- DelegationPlanner::should_spawn - parallel ----

    #[test]
    fn should_spawn_parallel_for_multi_module() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![
                PathBuf::from("/alpha/mod.rs"),
                PathBuf::from("/beta/mod.rs"),
                PathBuf::from("/gamma/mod.rs"),
            ],
            past_failure_count: 0,
            parent_task_id: "parent-5".into(),
        };

        let decision = planner.should_spawn("explore the full codebase architecture", &context);
        assert!(matches!(decision, SpawnDecision::SpawnParallel(_)));
    }

    // ---- DelegationContext::affected_modules ----

    #[test]
    fn affected_modules_extracts_unique_parents() {
        let ctx = DelegationContext {
            context_pressure: 0.0,
            remaining_budget: 10.0,
            affected_paths: vec![
                PathBuf::from("/foo/bar.rs"),
                PathBuf::from("/foo/baz.rs"),
                PathBuf::from("/qux/quux.rs"),
            ],
            past_failure_count: 0,
            parent_task_id: "test".into(),
        };

        let modules = ctx.affected_modules();
        assert_eq!(modules, vec!["foo", "qux"]);
    }

    #[test]
    fn affected_modules_empty_paths() {
        let ctx = DelegationContext {
            context_pressure: 0.0,
            remaining_budget: 10.0,
            affected_paths: vec![],
            past_failure_count: 0,
            parent_task_id: "test".into(),
        };

        assert!(ctx.affected_modules().is_empty());
    }

    #[test]
    fn affected_modules_deduplicates() {
        let ctx = DelegationContext {
            context_pressure: 0.0,
            remaining_budget: 10.0,
            affected_paths: vec![
                PathBuf::from("/src/a.rs"),
                PathBuf::from("/src/b.rs"),
                PathBuf::from("/src/c.rs"),
            ],
            past_failure_count: 0,
            parent_task_id: "test".into(),
        };

        let modules = ctx.affected_modules();
        assert_eq!(modules, vec!["src"]);
    }

    // ---- DelegationContext::for_tool_call ----

    #[test]
    fn delegation_context_for_tool_call_has_defaults() {
        let ctx = DelegationContext::for_tool_call(std::path::Path::new("/project"), "parent-123");
        assert_eq!(ctx.parent_task_id, "parent-123");
        assert!(!ctx.affected_paths.is_empty());
        assert_eq!(ctx.past_failure_count, 0);
        assert!(ctx.remaining_budget > 0.0);
        assert!((0.0..=1.0).contains(&ctx.context_pressure));
    }

    // ---- Serialization roundtrips ----

    #[test]
    fn task_role_serialization_roundtrip() {
        let roles = [
            TaskRole::Explore,
            TaskRole::Research,
            TaskRole::Code,
            TaskRole::Review,
            TaskRole::Verify,
            TaskRole::Plan,
            TaskRole::Debug,
        ];

        for role in roles {
            let json = serde_json::to_string(&role).unwrap();
            let back: TaskRole = serde_json::from_str(&json).unwrap();
            assert_eq!(role, back, "roundtrip failed for {role:?}");
        }
    }

    #[test]
    fn task_role_serialized_format() {
        let json = serde_json::to_string(&TaskRole::Code).unwrap();
        assert_eq!(json, "\"code\"");
    }

    // ---- TaskSpec builder ----

    #[test]
    fn task_spec_builder_chaining() {
        let spec = TaskSpec::new("do the thing", TaskRole::Code)
            .with_budget_limit(5.0)
            .with_max_steps(10)
            .with_path(PathBuf::from("/src/main.rs"))
            .with_resume_from("checkpoint-1");

        assert_eq!(spec.role, TaskRole::Code);
        assert!((spec.budget_limit - 5.0).abs() < f64::EPSILON);
        assert_eq!(spec.max_steps, Some(10));
        assert_eq!(spec.path_scope.len(), 1);
        assert_eq!(spec.resume_from.as_deref(), Some("checkpoint-1"));
    }

    #[test]
    fn task_spec_effective_tier_with_override() {
        let spec =
            TaskSpec::new("task", TaskRole::Explore).with_tier_override(ExecutionTier::Thinking);
        assert_eq!(spec.effective_tier(), ExecutionTier::Thinking);
    }

    #[test]
    fn task_spec_effective_tier_without_override() {
        let spec = TaskSpec::new("task", TaskRole::Plan);
        assert_eq!(spec.effective_tier(), ExecutionTier::Composer);
    }

    // ---- SpawnDecision debug ----

    #[test]
    fn spawn_decision_debug_inline() {
        let decision = SpawnDecision::Inline;
        let debug = format!("{decision:?}");
        assert!(debug.contains("Inline"));
    }

    #[test]
    fn spawn_decision_debug_spawn() {
        let spec = TaskSpec::new("test", TaskRole::Code);
        let decision = SpawnDecision::Spawn(spec);
        let debug = format!("{decision:?}");
        assert!(debug.contains("Spawn"));
    }

    // ---- Parallel respects max_concurrent_tasks ----

    #[test]
    fn parallel_spawn_respects_max_concurrent() {
        let config = DelegationConfig {
            max_concurrent_tasks: 2,
            ..DelegationConfig::default()
        };
        let planner = DelegationPlanner::new(config);
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![
                PathBuf::from("/a/mod.rs"),
                PathBuf::from("/b/mod.rs"),
                PathBuf::from("/c/mod.rs"),
                PathBuf::from("/d/mod.rs"),
            ],
            past_failure_count: 0,
            parent_task_id: "parent-6".into(),
        };

        let decision =
            planner.should_spawn("explore the architecture across all modules", &context);
        if let SpawnDecision::SpawnParallel(specs) = decision {
            assert!(specs.len() <= 2, "should not exceed max_concurrent_tasks");
        } else {
            panic!("expected SpawnParallel, got {decision:?}");
        }
    }

    // ---- No parallel when disabled ----

    #[test]
    fn no_parallel_when_disabled() {
        let config = DelegationConfig {
            allow_parallel: false,
            ..DelegationConfig::default()
        };
        let planner = DelegationPlanner::new(config);
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![PathBuf::from("/a/mod.rs"), PathBuf::from("/b/mod.rs")],
            past_failure_count: 0,
            parent_task_id: "parent-7".into(),
        };

        let decision = planner.should_spawn("explore the full system", &context);
        assert!(
            matches!(decision, SpawnDecision::Spawn(_)),
            "expected single Spawn when parallel disabled, got {decision:?}"
        );
    }

    // ---- Boundary: context pressure exactly at threshold ----

    #[test]
    fn spawn_at_exact_pressure_threshold() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.75,
            remaining_budget: 10.0,
            affected_paths: vec![],
            past_failure_count: 0,
            parent_task_id: "parent-8".into(),
        };

        let decision = planner.should_spawn("fix a typo", &context);
        assert!(matches!(decision, SpawnDecision::Spawn(_)));
    }

    // ---- Boundary: complexity exactly at spawn threshold ----

    #[test]
    fn spawn_at_exact_complexity_threshold() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![PathBuf::from("/src/main.rs")],
            past_failure_count: 0,
            parent_task_id: "parent-9".into(),
        };

        // "refactor" is 3.0 — exactly at threshold
        let decision = planner.should_spawn("refactor the module", &context);
        assert!(
            matches!(decision, SpawnDecision::Spawn(_)),
            "complexity at threshold should spawn, got {decision:?}"
        );
    }

    // ---- Ensemble plan contains strategy and participants ----

    #[test]
    fn ensemble_plan_structure() {
        let planner = DelegationPlanner::new(DelegationConfig::default());
        let context = DelegationContext {
            context_pressure: 0.1,
            remaining_budget: 10.0,
            affected_paths: vec![PathBuf::from("/src/main.rs")],
            past_failure_count: 1,
            parent_task_id: "parent-10".into(),
        };

        let decision =
            planner.should_spawn("explore and investigate the complex failure", &context);
        if let SpawnDecision::Ensemble(plan) = decision {
            assert!(!plan.participants.is_empty());
            for (spec, task) in &plan.participants {
                assert!(task.prompt.contains(&spec.role));
            }
        } else {
            panic!("expected Ensemble, got {decision:?}");
        }
    }
}
