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
use std::time::SystemTime;

// Error Classification

/// Categorizes errors to determine retry vs. escalation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Transient: auto-retry with backoff.
    RateLimit429,
    /// Transient: auto-retry with backoff.
    Timeout,
    /// Transient: auto-retry with backoff.
    ServerError5xx,
    /// Persistent: escalate to parent for decision.
    BadRequest400,
    /// Persistent: escalate to parent for decision.
    InvalidDelegation,
    /// Persistent: escalate to parent for decision.
    PermissionDenied,
    /// Persistent: escalate to parent for decision.
    ContextWindow,
}

impl ErrorCategory {
    /// True if this error should trigger automatic retry with backoff.
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            Self::RateLimit429 | Self::Timeout | Self::ServerError5xx
        )
    }

    /// True if this error should escalate to parent conversation.
    pub fn is_persistent(self) -> bool {
        !self.is_transient()
    }
}

// TaskRole

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
                &["Read", "search_files", "list_directory", "Glob"]
            }
            Self::Code => &[
                "Read",
                "Write",
                "Edit",
                "search_files",
                "list_directory",
                "Glob",
                "Bash",
            ],
            Self::Verify | Self::Debug => {
                &["Read", "search_files", "list_directory", "Glob", "Bash"]
            }
        }
    }
}

impl TryFrom<TaskRole> for rustycode_protocol::AgentRole {
    type Error = String;

    fn try_from(role: TaskRole) -> Result<Self, Self::Error> {
        match role {
            TaskRole::Explore => Ok(rustycode_protocol::AgentRole::Researcher),
            TaskRole::Research => Ok(rustycode_protocol::AgentRole::Researcher),
            TaskRole::Code => Ok(rustycode_protocol::AgentRole::Builder),
            TaskRole::Review => Ok(rustycode_protocol::AgentRole::Reviewer),
            TaskRole::Verify => Ok(rustycode_protocol::AgentRole::Judge),
            TaskRole::Plan => Ok(rustycode_protocol::AgentRole::Planner),
            TaskRole::Debug => Ok(rustycode_protocol::AgentRole::Scalpel),
        }
    }
}

// TaskSpec

/// Token tracking delegation ancestry and constraints (immutable metadata).
///
/// Propagated from parent to child on each delegation. Enforces maximum
/// delegation depth and restricts which tools/roles a child agent may use.
/// This token is metadata only — retry state is tracked separately in RetryState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationToken {
    /// Agent that issued this delegation.
    pub parent_agent_id: String,
    /// Current depth (0 = root, 1 = first delegation).
    pub depth: u32,
    /// Maximum allowed delegation depth (default: 3).
    pub max_depth: u32,
    /// Roles this agent is allowed to delegate to (empty = all allowed).
    pub allowed_roles: Vec<crate::agent_registry::SpecialistType>,
    /// Tool names this agent is allowed to use (empty = all allowed).
    pub allowed_tools: Vec<String>,
    /// Maximum automatic retries per transient error (default: 3).
    pub max_retries_per_error: u32,
}

/// Mutable retry state for a delegated task execution.
///
/// Tracks current retry count, last error, and timing. Lives in ExecutionContext
/// and is mutable across the lifetime of a task execution.
#[derive(Debug, Clone)]
pub struct RetryState {
    /// Current retry count for the last error type.
    pub current_error_retries: u32,
    /// Category of the last error encountered (None if no error yet).
    pub last_error: Option<ErrorCategory>,
    /// Timestamp of the last error (used for retry backoff calculation).
    pub last_error_at: Option<SystemTime>,
}

impl Default for RetryState {
    fn default() -> Self {
        Self::new()
    }
}

impl DelegationToken {
    /// Create a root token for the originating agent.
    pub fn root(agent_id: impl Into<String>) -> Self {
        Self {
            parent_agent_id: agent_id.into(),
            depth: 0,
            max_depth: 3,
            allowed_roles: Vec::new(),
            allowed_tools: Vec::new(),
            max_retries_per_error: 3,
        }
    }

    /// Create a child token with incremented depth.
    ///
    /// Returns `None` if `max_depth` would be exceeded.
    pub fn child(&self, child_agent_id: impl Into<String>) -> Option<Self> {
        let new_depth = self.depth + 1;
        if new_depth >= self.max_depth {
            return None;
        }
        Some(Self {
            parent_agent_id: child_agent_id.into(),
            depth: new_depth,
            max_depth: self.max_depth,
            allowed_roles: self.allowed_roles.clone(),
            allowed_tools: self.allowed_tools.clone(),
            max_retries_per_error: self.max_retries_per_error,
        })
    }

    /// Check if delegation is allowed (depth + 1 < max_depth).
    pub fn can_delegate(&self) -> bool {
        self.depth + 1 < self.max_depth
    }
}

impl RetryState {
    /// Create a fresh retry state for a new task execution.
    pub fn new() -> Self {
        Self {
            current_error_retries: 0,
            last_error: None,
            last_error_at: None,
        }
    }

    /// Check if this error should be automatically retried.
    ///
    /// Requires the token to check max_retries_per_error limit.
    /// Returns true if:
    /// 1. Error is transient (429, timeout, 5xx)
    /// 2. Retries haven't been exhausted yet
    /// 3. Either it's a new error type, or same error with retries remaining
    pub fn should_retry(&self, token: &DelegationToken, error: ErrorCategory) -> bool {
        if !error.is_transient() {
            return false;
        }

        match self.last_error {
            None => true, // First error, try to retry
            Some(last) if last == error => {
                // Same error type, check retry count against token limit
                self.current_error_retries < token.max_retries_per_error
            }
            Some(last) if last != error => {
                // Different error type, reset retry counter
                true
            }
            _ => false,
        }
    }

    /// Check if this error should escalate to parent conversation.
    ///
    /// Requires the token to check max_retries_per_error limit.
    /// Returns true if:
    /// 1. Error is persistent (400, invalid delegation, etc)
    /// 2. Error is transient but retries exhausted
    pub fn should_escalate(&self, token: &DelegationToken, error: ErrorCategory) -> bool {
        if error.is_persistent() {
            return true;
        }

        if !error.is_transient() {
            return false;
        }

        // Transient error: escalate if retries exhausted
        match self.last_error {
            Some(last) if last == error => {
                self.current_error_retries >= token.max_retries_per_error
            }
            _ => false,
        }
    }

    /// Calculate exponential backoff delay for the next retry (in milliseconds).
    ///
    /// Uses formula: 2^(retry_count - 1) * 1000ms, capped at 32s.
    /// After the first error, retries=1, so backoff=2^0*1000ms=1000ms.
    /// After the second error, retries=2, so backoff=2^1*1000ms=2000ms.
    pub fn next_backoff_ms(&self) -> u64 {
        if self.current_error_retries == 0 {
            return 0; // No error yet, no backoff
        }
        let exponent = self.current_error_retries.saturating_sub(1);
        let base_ms = 2_u64.saturating_pow(exponent);
        (base_ms * 1000).min(32_000) // Cap at 32 seconds
    }

    /// Record that an error occurred and update retry state.
    ///
    /// If this is a different error type, resets the retry counter.
    pub fn record_error(&mut self, error: ErrorCategory) {
        match self.last_error {
            Some(last) if last == error => {
                // Same error, increment retry count
                self.current_error_retries += 1;
            }
            _ => {
                // New error type, reset counter
                self.current_error_retries = 1;
                self.last_error = Some(error);
            }
        }
        self.last_error_at = Some(SystemTime::now());
    }

    /// Check if sufficient time has passed since last error for the next retry.
    pub fn is_backoff_satisfied(&self) -> bool {
        match self.last_error_at {
            None => true,
            Some(last_time) => {
                let elapsed = last_time.elapsed().unwrap_or_default().as_millis() as u64;
                elapsed >= self.next_backoff_ms()
            }
        }
    }
}

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
    /// Delegation token tracking ancestry and depth constraints.
    pub delegation_token: Option<DelegationToken>,
}

impl TaskSpec {
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
            delegation_token: None,
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

    /// Attach a delegation token for depth/tool-scoping enforcement.
    pub fn with_delegation_token(mut self, token: DelegationToken) -> Self {
        self.delegation_token = Some(token);
        self
    }

    /// Effective execution tier (override or role default).
    pub fn effective_tier(&self) -> ExecutionTier {
        self.tier_override
            .unwrap_or_else(|| self.role.default_tier())
    }
}

// EnsemblePlan

/// Plan for an ensemble strategy execution with mapped participants.
#[derive(Debug, Clone)]
pub struct EnsemblePlan {
    /// Which ensemble strategy to use.
    pub strategy: StrategyKind,
    /// Participant specs paired with their task assignments.
    pub participants: Vec<(ParticipantSpec, TaskSpec)>,
}

// SpawnDecision

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

// DelegationContext

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

// DelegationConfig

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

// DelegationPlanner

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

// Role inference

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

// Tests

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

    // ---- TaskRole to AgentRole conversion ----

    #[test]
    fn task_role_to_agent_role_conversion() {
        use std::convert::TryFrom;

        assert_eq!(
            rustycode_protocol::AgentRole::try_from(TaskRole::Explore).unwrap(),
            rustycode_protocol::AgentRole::Researcher
        );
        assert_eq!(
            rustycode_protocol::AgentRole::try_from(TaskRole::Code).unwrap(),
            rustycode_protocol::AgentRole::Builder
        );
        assert_eq!(
            rustycode_protocol::AgentRole::try_from(TaskRole::Review).unwrap(),
            rustycode_protocol::AgentRole::Reviewer
        );
        assert_eq!(
            rustycode_protocol::AgentRole::try_from(TaskRole::Verify).unwrap(),
            rustycode_protocol::AgentRole::Judge
        );
        assert_eq!(
            rustycode_protocol::AgentRole::try_from(TaskRole::Plan).unwrap(),
            rustycode_protocol::AgentRole::Planner
        );
        assert_eq!(
            rustycode_protocol::AgentRole::try_from(TaskRole::Debug).unwrap(),
            rustycode_protocol::AgentRole::Scalpel
        );
    }

    // ---- DelegationToken ----

    #[test]
    fn delegation_token_root_creation() {
        let token = DelegationToken::root("coordinator-1");
        assert_eq!(token.parent_agent_id, "coordinator-1");
        assert_eq!(token.depth, 0);
        assert_eq!(token.max_depth, 3);
        assert!(token.can_delegate());
        assert!(token.allowed_roles.is_empty());
        assert!(token.allowed_tools.is_empty());
        assert_eq!(token.max_retries_per_error, 3);
    }

    #[test]
    fn delegation_token_child_increments_depth() {
        let root = DelegationToken::root("coordinator-1");
        let child = root.child("agent-1").unwrap();
        assert_eq!(child.parent_agent_id, "agent-1");
        assert_eq!(child.depth, 1);
        assert_eq!(child.max_depth, 3);
        assert!(child.can_delegate());
    }

    #[test]
    fn delegation_token_max_depth_enforcement() {
        let root = DelegationToken::root("coordinator-1");
        let child1 = root.child("agent-1").unwrap();
        assert_eq!(child1.depth, 1);
        let child2 = child1.child("agent-2").unwrap();
        assert_eq!(child2.depth, 2);
        assert!(!child2.can_delegate());
        assert!(child2.child("agent-3").is_none());
    }

    #[test]
    fn delegation_token_child_inherits_allowed_roles() {
        use crate::agent_registry::SpecialistType;
        let mut root = DelegationToken::root("coordinator");
        root.allowed_roles = vec![SpecialistType::SecurityAudit];
        root.allowed_tools = vec!["Read".into()];

        let child = root.child("child").unwrap();
        assert_eq!(child.allowed_roles.len(), 1);
        assert_eq!(child.allowed_tools.len(), 1);
    }

    #[test]
    fn task_spec_with_delegation_token() {
        let token = DelegationToken::root("root-agent");
        let spec = TaskSpec::new("implement auth", TaskRole::Code).with_delegation_token(token);

        assert!(spec.delegation_token.is_some());
        assert_eq!(spec.delegation_token.unwrap().depth, 0);
    }

    #[test]
    fn delegation_token_serialization_roundtrip() {
        let token = DelegationToken::root("agent-x");
        let json = serde_json::to_string(&token).unwrap();
        let back: DelegationToken = serde_json::from_str(&json).unwrap();
        assert_eq!(back.parent_agent_id, "agent-x");
        assert_eq!(back.depth, 0);
        assert_eq!(back.max_depth, 3);
    }

    // ---- Retry and backoff logic ----

    #[test]
    fn error_category_is_transient() {
        assert!(ErrorCategory::RateLimit429.is_transient());
        assert!(ErrorCategory::Timeout.is_transient());
        assert!(ErrorCategory::ServerError5xx.is_transient());
        assert!(!ErrorCategory::BadRequest400.is_transient());
    }

    #[test]
    fn error_category_is_persistent() {
        assert!(ErrorCategory::BadRequest400.is_persistent());
        assert!(ErrorCategory::InvalidDelegation.is_persistent());
        assert!(ErrorCategory::PermissionDenied.is_persistent());
        assert!(ErrorCategory::ContextWindow.is_persistent());
        assert!(!ErrorCategory::RateLimit429.is_persistent());
    }

    #[test]
    fn retry_state_new() {
        let state = RetryState::new();
        assert_eq!(state.current_error_retries, 0);
        assert_eq!(state.last_error, None);
        assert_eq!(state.last_error_at, None);
    }

    #[test]
    fn should_retry_transient_error_first_time() {
        let token = DelegationToken::root("agent");
        let state = RetryState::new();
        assert!(state.should_retry(&token, ErrorCategory::RateLimit429));
        assert!(state.should_retry(&token, ErrorCategory::Timeout));
        assert!(state.should_retry(&token, ErrorCategory::ServerError5xx));
    }

    #[test]
    fn should_not_retry_persistent_error() {
        let token = DelegationToken::root("agent");
        let state = RetryState::new();
        assert!(!state.should_retry(&token, ErrorCategory::BadRequest400));
        assert!(!state.should_retry(&token, ErrorCategory::InvalidDelegation));
        assert!(!state.should_retry(&token, ErrorCategory::PermissionDenied));
    }

    #[test]
    fn should_escalate_persistent_error() {
        let token = DelegationToken::root("agent");
        let state = RetryState::new();
        assert!(state.should_escalate(&token, ErrorCategory::BadRequest400));
        assert!(state.should_escalate(&token, ErrorCategory::InvalidDelegation));
        assert!(state.should_escalate(&token, ErrorCategory::ContextWindow));
    }

    #[test]
    fn should_escalate_after_exhausted_retries() {
        let mut token = DelegationToken::root("agent");
        token.max_retries_per_error = 2;
        let mut state = RetryState::new();

        // First transient error: should retry
        assert!(state.should_retry(&token, ErrorCategory::RateLimit429));
        state.record_error(ErrorCategory::RateLimit429);
        assert_eq!(state.current_error_retries, 1);

        // Same error again: should still retry
        assert!(state.should_retry(&token, ErrorCategory::RateLimit429));
        state.record_error(ErrorCategory::RateLimit429);
        assert_eq!(state.current_error_retries, 2);

        // Third time: retries exhausted, should escalate
        assert!(!state.should_retry(&token, ErrorCategory::RateLimit429));
        assert!(state.should_escalate(&token, ErrorCategory::RateLimit429));
    }

    #[test]
    fn error_type_change_resets_retry_counter() {
        let _token = DelegationToken::root("agent");
        let mut state = RetryState::new();

        // First error: timeout
        state.record_error(ErrorCategory::Timeout);
        assert_eq!(state.current_error_retries, 1);
        assert_eq!(state.last_error, Some(ErrorCategory::Timeout));

        // Different error: rate limit (counter should reset)
        state.record_error(ErrorCategory::RateLimit429);
        assert_eq!(state.current_error_retries, 1);
        assert_eq!(state.last_error, Some(ErrorCategory::RateLimit429));
    }

    #[test]
    fn next_backoff_ms_exponential() {
        let mut state = RetryState::new();

        // No retries yet: 0ms
        assert_eq!(state.next_backoff_ms(), 0);

        state.current_error_retries = 1;
        // 2^(1-1) * 1000 = 2^0 * 1000 = 1000ms
        assert_eq!(state.next_backoff_ms(), 1000);

        state.current_error_retries = 2;
        // 2^(2-1) * 1000 = 2^1 * 1000 = 2000ms
        assert_eq!(state.next_backoff_ms(), 2000);

        state.current_error_retries = 3;
        // 2^(3-1) * 1000 = 2^2 * 1000 = 4000ms
        assert_eq!(state.next_backoff_ms(), 4000);

        state.current_error_retries = 4;
        // 2^(4-1) * 1000 = 2^3 * 1000 = 8000ms
        assert_eq!(state.next_backoff_ms(), 8000);

        state.current_error_retries = 6;
        // 2^(6-1) * 1000 = 2^5 * 1000 = 32000ms (capped)
        assert_eq!(state.next_backoff_ms(), 32000);

        state.current_error_retries = 10;
        // Still capped at 32000ms
        assert_eq!(state.next_backoff_ms(), 32000);
    }

    #[test]
    fn full_retry_workflow() {
        let mut token = DelegationToken::root("agent");
        token.max_retries_per_error = 3;
        let mut state = RetryState::new();

        // Encounter first 429 error
        assert!(state.should_retry(&token, ErrorCategory::RateLimit429));
        state.record_error(ErrorCategory::RateLimit429);
        assert_eq!(state.current_error_retries, 1);
        assert_eq!(state.next_backoff_ms(), 1000); // 2^(1-1) * 1000 = 1000ms

        // Same error again
        assert!(state.should_retry(&token, ErrorCategory::RateLimit429));
        state.record_error(ErrorCategory::RateLimit429);
        assert_eq!(state.current_error_retries, 2);
        assert_eq!(state.next_backoff_ms(), 2000); // 2^(2-1) * 1000 = 2000ms

        // Still retrying
        assert!(state.should_retry(&token, ErrorCategory::RateLimit429));
        state.record_error(ErrorCategory::RateLimit429);
        assert_eq!(state.current_error_retries, 3);
        assert_eq!(state.next_backoff_ms(), 4000); // 2^(3-1) * 1000 = 4000ms

        // Retries exhausted
        assert!(!state.should_retry(&token, ErrorCategory::RateLimit429));
        assert!(state.should_escalate(&token, ErrorCategory::RateLimit429));
    }

    #[test]
    fn is_backoff_satisfied() {
        let mut state = RetryState::new();

        // No error yet, no backoff needed
        assert!(state.is_backoff_satisfied());

        // Record an error (timestamp set to now)
        state.record_error(ErrorCategory::RateLimit429);
        // Immediately checking should fail because not enough time passed
        assert!(!state.is_backoff_satisfied()); // Would need 1000ms to pass

        // Manually set last_error_at to far past
        state.last_error_at = Some(SystemTime::now() - std::time::Duration::from_secs(1));
        assert!(state.is_backoff_satisfied());
    }
}
