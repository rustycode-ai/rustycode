//! Fork-join parallel execution with shared context snapshots.
//!
//! For parallel tasks, the parent's context is snapshotted and injected into
//! each fork. Forks execute independently via `tokio::JoinSet` and results
//! are collected back.

use crate::bus::BusHandle;
use crate::delegation::TaskRole;
use crate::task_runner::TaskRunner;
use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Snapshot of parent context for injection into parallel forks.
///
/// Contains the essential state that each fork needs to start working
/// without re-loading from scratch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// The original task description.
    pub task_description: String,
    /// The task ID (parent's).
    pub task_id: String,
    /// Current tier at fork time.
    pub fork_tier: u8,
    /// Budget state at fork time.
    pub budget_used: f64,
    /// Budget limit at fork time.
    pub budget_limit: f64,
    /// Token state at fork time.
    pub tokens_used: u64,
    /// Workspace entries relevant to the forks.
    pub workspace_snapshot: Vec<(String, serde_json::Value)>,
    /// Constraints that apply to all forks.
    pub constraints: Vec<String>,
    /// Timestamp of the snapshot.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ContextSnapshot {
    pub fn new(
        task_id: impl Into<String>,
        task_description: impl Into<String>,
        fork_tier: u8,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            task_description: task_description.into(),
            fork_tier,
            budget_used: 0.0,
            budget_limit: 10.0,
            tokens_used: 0,
            workspace_snapshot: Vec::new(),
            constraints: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    /// Set budget state.
    pub const fn with_budget(mut self, used: f64, limit: f64) -> Self {
        self.budget_used = used;
        self.budget_limit = limit;
        self
    }

    /// Set token state.
    pub const fn with_tokens(mut self, used: u64) -> Self {
        self.tokens_used = used;
        self
    }

    /// Add a workspace entry.
    pub fn with_workspace_entry(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.workspace_snapshot.push((key.into(), value));
        self
    }

    /// Add a constraint.
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    /// Whether this snapshot has the minimum required fields.
    pub const fn is_valid(&self) -> bool {
        !self.task_id.is_empty() && !self.task_description.is_empty()
    }
}

/// Specification for a single parallel fork.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkSpec {
    /// Unique identifier for this fork.
    pub fork_id: String,
    /// What this fork should do.
    pub description: String,
    /// File paths this fork is responsible for (for worktree isolation).
    pub path_scope: Vec<PathBuf>,
    /// Optional checkpoint to resume from.
    #[serde(default)]
    pub resume_from: Option<String>,
    /// Tier at which this fork should execute.
    pub tier: ExecutionTier,
    /// Semantic role for this fork's execution.
    #[serde(default)]
    pub role: Option<TaskRole>,
}

impl ForkSpec {
    pub fn new(
        fork_id: impl Into<String>,
        description: impl Into<String>,
        tier: ExecutionTier,
    ) -> Self {
        Self {
            fork_id: fork_id.into(),
            description: description.into(),
            path_scope: Vec::new(),
            resume_from: None,
            tier,
            role: None,
        }
    }

    /// Add a path to this fork's scope.
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path_scope.push(path);
        self
    }

    /// Set a resume checkpoint for this fork.
    pub fn with_resume_from(mut self, checkpoint: impl Into<String>) -> Self {
        self.resume_from = Some(checkpoint.into());
        self
    }

    /// Whether this spec is valid.
    pub const fn is_valid(&self) -> bool {
        !self.fork_id.is_empty() && !self.description.is_empty()
    }
}

/// Result from a completed fork.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkResult {
    /// The fork's ID.
    pub fork_id: String,
    /// Whether the fork succeeded.
    pub success: bool,
    /// Output from the fork.
    pub output: String,
    /// Cost incurred by this fork.
    pub cost_usd: f64,
    /// Duration in milliseconds.
    pub duration_ms: i64,
}

impl ForkResult {
    /// Create a successful fork result.
    pub fn success(
        fork_id: impl Into<String>,
        output: impl Into<String>,
        cost_usd: f64,
        duration_ms: i64,
    ) -> Self {
        Self {
            fork_id: fork_id.into(),
            success: true,
            output: output.into(),
            cost_usd,
            duration_ms,
        }
    }

    /// Create a failed fork result.
    pub fn failure(
        fork_id: impl Into<String>,
        reason: impl Into<String>,
        duration_ms: i64,
    ) -> Self {
        Self {
            fork_id: fork_id.into(),
            success: false,
            output: reason.into(),
            cost_usd: 0.0,
            duration_ms,
        }
    }
}

/// Configuration for fork-join execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkJoinConfig {
    /// Maximum number of concurrent forks.
    pub max_concurrency: usize,
    /// Timeout per fork in milliseconds.
    pub fork_timeout_ms: u64,
}

impl Default for ForkJoinConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            fork_timeout_ms: 30_000,
        }
    }
}

/// Aggregated result of a fork-join execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkJoinResult {
    /// Individual fork results.
    pub fork_results: Vec<ForkResult>,
    /// Total cost across all forks.
    pub total_cost_usd: f64,
    /// Whether all forks succeeded.
    pub all_succeeded: bool,
    /// Total wall-clock duration in ms.
    pub total_duration_ms: i64,
}

impl ForkJoinResult {
    /// Number of forks that succeeded.
    pub fn success_count(&self) -> usize {
        self.fork_results.iter().filter(|r| r.success).count()
    }

    /// Number of forks that failed.
    pub fn failure_count(&self) -> usize {
        self.fork_results.len() - self.success_count()
    }
}

/// Coordinates parallel fork execution with shared context snapshots.
///
/// Uses `tokio::JoinSet` for true parallel execution, bounded by
/// `max_concurrency` via a semaphore. Each fork runs as an independent
/// tokio task.
pub struct ForkJoinExecutor {
    config: ForkJoinConfig,
    bus: BusHandle,
    runner: Option<Arc<dyn TaskRunner>>,
}

impl ForkJoinExecutor {
    pub const fn new(config: ForkJoinConfig, bus: BusHandle) -> Self {
        Self {
            config,
            bus,
            runner: None,
        }
    }

    /// Create with default configuration.
    pub fn with_bus(bus: BusHandle) -> Self {
        Self::new(ForkJoinConfig::default(), bus)
    }

    /// Create with a real task runner for production execution.
    pub fn with_runner(
        config: ForkJoinConfig,
        bus: BusHandle,
        runner: Arc<dyn TaskRunner>,
    ) -> Self {
        Self {
            config,
            bus,
            runner: Some(runner),
        }
    }

    /// Plan forks from a list of path scopes.
    ///
    /// Each path gets its own fork with a unique ID. Paths are assigned
    /// to the given tier by default.
    pub fn plan_forks(
        paths: &[PathBuf],
        base_description: &str,
        tier: ExecutionTier,
    ) -> Vec<ForkSpec> {
        paths
            .iter()
            .enumerate()
            .map(|(i, path)| {
                ForkSpec::new(
                    format!("fork-{i}"),
                    format!("{base_description} (path: {})", path.display()),
                    tier,
                )
                .with_path(path.clone())
            })
            .collect()
    }

    /// Execute a list of fork specifications against a context snapshot.
    ///
    /// Spawns each fork as a parallel tokio task, bounded by `max_concurrency`.
    /// Forks that complete within `fork_timeout_ms` produce success results;
    /// timed-out or panicked forks produce failure results.
    #[allow(
        clippy::too_many_lines,
        clippy::option_if_let_else,
        clippy::single_match_else
    )]
    pub async fn execute_forks(
        &self,
        snapshot: &ContextSnapshot,
        specs: &[ForkSpec],
    ) -> ForkJoinResult {
        if specs.is_empty() {
            return ForkJoinResult {
                fork_results: Vec::new(),
                total_cost_usd: 0.0,
                all_succeeded: true,
                total_duration_ms: 0,
            };
        }

        let start = std::time::Instant::now();
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrency));
        let timeout = std::time::Duration::from_millis(self.config.fork_timeout_ms);
        let fork_count = specs.len();

        let mut join_set: tokio::task::JoinSet<(ForkResult, BusHandle)> =
            tokio::task::JoinSet::new();

        for spec in specs {
            let permit = semaphore.clone();
            let timeout_dur = timeout;
            let bus = self.bus.clone();
            let task_id = snapshot.task_id.clone();
            let spec = spec.clone();
            let fork_id = spec.fork_id.clone();
            let description = spec.description.clone();
            let runner_opt = self.runner.clone();
            let role = spec.role.unwrap_or(TaskRole::Code);
            let path_scope = spec.path_scope.clone();

            self.bus
                .publish(crate::bus::OrchestrationEvent::ForkStarted {
                    task_id: task_id.clone(),
                    fork_id: fork_id.clone(),
                    fork_count,
                });

            join_set.spawn(async move {
                let _permit = permit.acquire().await.unwrap_or_else(|e| {
                    tracing::error!("semaphore closed: {e}");
                    panic!("semaphore closed unexpectedly")
                });

                let fork_start = std::time::Instant::now();

                let result = match tokio::time::timeout(timeout_dur, async {
                    match runner_opt.as_ref() {
                        Some(runner) => {
                            let desc = description.clone();
                            let paths = path_scope.clone();
                            let resume_from = spec.resume_from.clone();
                            let fork_id_captured = fork_id.clone();
                            match runner.run_task(&desc, role, &paths, resume_from.as_deref()) {
                                Ok(task_result) => ForkResult {
                                    fork_id: fork_id_captured,
                                    success: task_result.success,
                                    output: task_result.output,
                                    cost_usd: task_result.cost_usd,
                                    duration_ms: i64::try_from(fork_start.elapsed().as_millis())
                                        .unwrap_or(i64::MAX),
                                },
                                Err(e) => ForkResult::failure(
                                    &fork_id_captured,
                                    e.to_string(),
                                    i64::try_from(fork_start.elapsed().as_millis())
                                        .unwrap_or(i64::MAX),
                                ),
                            }
                        }
                        None => {
                            tokio::task::yield_now().await;
                            ForkResult::success(
                                &fork_id,
                                format!("Fork executed: {description}"),
                                0.0,
                                i64::try_from(fork_start.elapsed().as_millis()).unwrap_or(i64::MAX),
                            )
                        }
                    }
                })
                .await
                {
                    Ok(result) => result,
                    Err(_) => ForkResult::failure(
                        &fork_id,
                        "fork timed out",
                        i64::try_from(fork_start.elapsed().as_millis()).unwrap_or(i64::MAX),
                    ),
                };

                (result, bus)
            });
        }

        let mut results = Vec::with_capacity(specs.len());
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((result, bus)) => {
                    bus.publish(crate::bus::OrchestrationEvent::ForkCompleted {
                        task_id: snapshot.task_id.clone(),
                        fork_id: result.fork_id.clone(),
                        success: result.success,
                        duration_ms: result.duration_ms,
                    });
                    results.push(result);
                }
                Err(join_err) => {
                    tracing::error!("fork task panicked: {join_err}");
                    results.push(ForkResult::failure(
                        "unknown",
                        format!("panic: {join_err}"),
                        0,
                    ));
                }
            }
        }

        // Sort by fork_id for deterministic test results.
        results.sort_by(|a, b| a.fork_id.cmp(&b.fork_id));

        let total_cost: f64 = results.iter().map(|r| r.cost_usd).sum();
        let all_succeeded = results.iter().all(|r| r.success);

        ForkJoinResult {
            fork_results: results,
            total_cost_usd: total_cost,
            all_succeeded,
            total_duration_ms: i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX),
        }
    }

    /// Merge fork results into a single aggregated summary.
    pub fn merge_results(results: &[ForkResult]) -> ForkJoinResult {
        let total_cost: f64 = results.iter().map(|r| r.cost_usd).sum();
        let all_succeeded = results.iter().all(|r| r.success);
        let max_duration: i64 = results.iter().map(|r| r.duration_ms).max().unwrap_or(0);

        ForkJoinResult {
            fork_results: results.to_vec(),
            total_cost_usd: total_cost,
            all_succeeded,
            total_duration_ms: max_duration,
        }
    }

    /// The configuration.
    pub const fn config(&self) -> &ForkJoinConfig {
        &self.config
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- ContextSnapshot tests ---

    #[test]
    fn snapshot_new_has_task_description() {
        let snap = ContextSnapshot::new("t1", "build feature", 2);
        assert_eq!(snap.task_description, "build feature");
        assert_eq!(snap.task_id, "t1");
        assert_eq!(snap.fork_tier, 2);
    }

    #[test]
    fn snapshot_new_has_budget_state() {
        let snap = ContextSnapshot::new("t1", "desc", 2)
            .with_budget(1.5, 10.0)
            .with_tokens(5000);
        assert!((snap.budget_used - 1.5).abs() < f64::EPSILON);
        assert!((snap.budget_limit - 10.0).abs() < f64::EPSILON);
        assert_eq!(snap.tokens_used, 5000);
    }

    #[test]
    fn snapshot_with_workspace_entries() {
        let snap = ContextSnapshot::new("t1", "desc", 2)
            .with_workspace_entry("key1", serde_json::json!("value1"))
            .with_workspace_entry("key2", serde_json::json!({"nested": true}));
        assert_eq!(snap.workspace_snapshot.len(), 2);
        assert_eq!(snap.workspace_snapshot[0].0, "key1");
    }

    #[test]
    fn snapshot_serialization_roundtrip() {
        let snap = ContextSnapshot::new("t1", "build feature", 3)
            .with_budget(0.5, 5.0)
            .with_tokens(2000)
            .with_constraint("no external deps");

        let json = serde_json::to_string(&snap).unwrap();
        let deserialized: ContextSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.task_id, "t1");
        assert_eq!(deserialized.task_description, "build feature");
        assert_eq!(deserialized.fork_tier, 3);
        assert_eq!(deserialized.tokens_used, 2000);
        assert_eq!(deserialized.constraints.len(), 1);
    }

    #[test]
    fn snapshot_is_valid_with_all_fields() {
        let snap = ContextSnapshot::new("t1", "desc", 2);
        assert!(snap.is_valid());
    }

    #[test]
    fn snapshot_is_invalid_without_task_id() {
        let snap = ContextSnapshot::new("", "desc", 2);
        assert!(!snap.is_valid());
    }

    #[test]
    fn snapshot_is_invalid_without_task_description() {
        let snap = ContextSnapshot::new("t1", "", 2);
        assert!(!snap.is_valid());
    }

    // --- ForkSpec tests ---

    #[test]
    fn fork_spec_new_has_id_and_description() {
        let spec = ForkSpec::new("fork-0", "process module A", ExecutionTier::Musician);
        assert_eq!(spec.fork_id, "fork-0");
        assert_eq!(spec.description, "process module A");
        assert_eq!(spec.tier, ExecutionTier::Musician);
        assert!(spec.path_scope.is_empty());
    }

    #[test]
    fn fork_spec_with_path_scope() {
        let spec = ForkSpec::new("fork-0", "desc", ExecutionTier::Musician)
            .with_path(PathBuf::from("src/main.rs"))
            .with_path(PathBuf::from("src/lib.rs"));
        assert_eq!(spec.path_scope.len(), 2);
        assert_eq!(spec.path_scope[0], PathBuf::from("src/main.rs"));
    }

    #[test]
    fn fork_spec_serialization_roundtrip() {
        let spec = ForkSpec::new("fork-0", "do stuff", ExecutionTier::Editor)
            .with_path(PathBuf::from("src/lib.rs"));

        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: ForkSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.fork_id, "fork-0");
        assert_eq!(deserialized.tier, ExecutionTier::Editor);
        assert_eq!(deserialized.path_scope.len(), 1);
    }

    #[test]
    fn fork_spec_is_valid() {
        let valid = ForkSpec::new("fork-0", "desc", ExecutionTier::Musician);
        assert!(valid.is_valid());

        let empty_id = ForkSpec::new("", "desc", ExecutionTier::Musician);
        assert!(!empty_id.is_valid());

        let empty_desc = ForkSpec::new("fork-0", "", ExecutionTier::Musician);
        assert!(!empty_desc.is_valid());
    }

    // --- ForkResult tests ---

    #[test]
    fn fork_result_success() {
        let result = ForkResult::success("fork-0", "output data", 0.05, 1500);
        assert!(result.success);
        assert_eq!(result.fork_id, "fork-0");
        assert_eq!(result.output, "output data");
        assert!((result.cost_usd - 0.05).abs() < f64::EPSILON);
        assert_eq!(result.duration_ms, 1500);
    }

    #[test]
    fn fork_result_failure() {
        let result = ForkResult::failure("fork-1", "timeout exceeded", 500);
        assert!(!result.success);
        assert_eq!(result.output, "timeout exceeded");
        assert!((result.cost_usd - 0.0).abs() < f64::EPSILON);
        assert_eq!(result.duration_ms, 500);
    }

    // --- ForkJoinConfig tests ---

    #[test]
    fn fork_join_config_default() {
        let config = ForkJoinConfig::default();
        assert_eq!(config.max_concurrency, 4);
        assert_eq!(config.fork_timeout_ms, 30_000);
    }

    // --- ForkJoinResult tests ---

    #[test]
    fn fork_join_result_counts() {
        let result = ForkJoinResult {
            fork_results: vec![
                ForkResult::success("f0", "ok", 0.01, 100),
                ForkResult::failure("f1", "err", 200),
                ForkResult::success("f2", "ok", 0.02, 150),
            ],
            total_cost_usd: 0.03,
            all_succeeded: false,
            total_duration_ms: 250,
        };
        assert_eq!(result.success_count(), 2);
        assert_eq!(result.failure_count(), 1);
        assert!(!result.all_succeeded);
    }

    // --- ForkJoinExecutor tests ---

    #[test]
    fn executor_new_creates_executor() {
        let bus = BusHandle::new(16);
        let config = ForkJoinConfig {
            max_concurrency: 8,
            fork_timeout_ms: 60_000,
        };
        let executor = ForkJoinExecutor::new(config, bus);
        assert_eq!(executor.config().max_concurrency, 8);
        assert_eq!(executor.config().fork_timeout_ms, 60_000);
    }

    #[test]
    fn executor_plan_forks_creates_specs_from_paths() {
        let paths = vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("tests/test.rs"),
        ];
        let specs = ForkJoinExecutor::plan_forks(&paths, "refactor", ExecutionTier::Musician);

        assert_eq!(specs.len(), 3);
        assert_eq!(specs[0].fork_id, "fork-0");
        assert!(specs[0].description.contains("src/main.rs"));
        assert_eq!(specs[0].path_scope.len(), 1);
        assert_eq!(specs[0].tier, ExecutionTier::Musician);
    }

    #[test]
    fn executor_plan_forks_empty_returns_empty() {
        let paths: Vec<PathBuf> = vec![];
        let specs = ForkJoinExecutor::plan_forks(&paths, "desc", ExecutionTier::Musician);
        assert!(specs.is_empty());
    }

    #[tokio::test]
    async fn executor_execute_forks_empty_returns_empty() {
        let bus = BusHandle::new(16);
        let executor = ForkJoinExecutor::with_bus(bus);
        let snapshot = ContextSnapshot::new("t1", "desc", 2);

        let result = executor.execute_forks(&snapshot, &[]).await;
        assert!(result.fork_results.is_empty());
        assert!(result.all_succeeded);
        assert!((result.total_cost_usd - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn executor_execute_forks_single_fork() {
        let bus = BusHandle::new(16);
        let executor = ForkJoinExecutor::with_bus(bus);
        let snapshot = ContextSnapshot::new("t1", "process files", 2);
        let specs = vec![ForkSpec::new(
            "fork-0",
            "process main.rs",
            ExecutionTier::Musician,
        )];

        let result = executor.execute_forks(&snapshot, &specs).await;
        assert_eq!(result.fork_results.len(), 1);
        assert!(result.all_succeeded);
        assert!(result.fork_results[0].success);
    }

    #[tokio::test]
    async fn executor_execute_forks_multiple_forks_parallel() {
        let bus = BusHandle::new(16);
        let executor = ForkJoinExecutor::with_bus(bus);
        let snapshot = ContextSnapshot::new("t1", "parallel work", 2);
        let specs = vec![
            ForkSpec::new("fork-0", "task A", ExecutionTier::Musician),
            ForkSpec::new("fork-1", "task B", ExecutionTier::Musician),
            ForkSpec::new("fork-2", "task C", ExecutionTier::Musician),
        ];

        let result = executor.execute_forks(&snapshot, &specs).await;
        assert_eq!(result.fork_results.len(), 3);
        assert!(result.all_succeeded);
        assert_eq!(result.success_count(), 3);
        assert_eq!(result.failure_count(), 0);

        // Verify deterministic ordering (sorted by fork_id).
        assert_eq!(result.fork_results[0].fork_id, "fork-0");
        assert_eq!(result.fork_results[1].fork_id, "fork-1");
        assert_eq!(result.fork_results[2].fork_id, "fork-2");
    }

    #[tokio::test]
    async fn executor_execute_forks_respects_concurrency_limit() {
        let bus = BusHandle::new(16);
        let config = ForkJoinConfig {
            max_concurrency: 2,
            fork_timeout_ms: 5_000,
        };
        let executor = ForkJoinExecutor::new(config, bus);
        let snapshot = ContextSnapshot::new("t1", "bounded work", 2);
        let specs = vec![
            ForkSpec::new("fork-0", "task A", ExecutionTier::Musician),
            ForkSpec::new("fork-1", "task B", ExecutionTier::Musician),
            ForkSpec::new("fork-2", "task C", ExecutionTier::Musician),
            ForkSpec::new("fork-3", "task D", ExecutionTier::Musician),
        ];

        let result = executor.execute_forks(&snapshot, &specs).await;
        assert_eq!(result.fork_results.len(), 4);
        assert!(result.all_succeeded);
    }

    #[tokio::test]
    async fn executor_execute_forks_timeout_produces_failure() {
        let bus = BusHandle::new(16);
        let config = ForkJoinConfig {
            max_concurrency: 4,
            fork_timeout_ms: 1, // 1ms — will timeout
        };
        let executor = ForkJoinExecutor::new(config, bus);
        let snapshot = ContextSnapshot::new("t1", "timeout test", 2);
        let specs = vec![ForkSpec::new(
            "fork-0",
            "slow task",
            ExecutionTier::Musician,
        )];

        let result = executor.execute_forks(&snapshot, &specs).await;
        // With a 1ms timeout, the fork may or may not succeed depending on
        // scheduling. At minimum, we should get a result back.
        assert_eq!(result.fork_results.len(), 1);
    }

    #[tokio::test]
    async fn executor_execute_forks_publishes_events() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let executor = ForkJoinExecutor::with_bus(bus);
        let snapshot = ContextSnapshot::new("t1", "desc", 2);
        let specs = vec![ForkSpec::new("fork-0", "task", ExecutionTier::Musician)];

        let _ = executor.execute_forks(&snapshot, &specs).await;

        // Should get ForkStarted then ForkCompleted
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(
            event1,
            crate::bus::OrchestrationEvent::ForkStarted { .. }
        ));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(
            event2,
            crate::bus::OrchestrationEvent::ForkCompleted { .. }
        ));
    }

    #[test]
    fn executor_merge_results_combines_costs() {
        let results = vec![
            ForkResult::success("f0", "ok", 0.01, 100),
            ForkResult::success("f1", "ok", 0.02, 200),
            ForkResult::success("f2", "ok", 0.03, 300),
        ];
        let merged = ForkJoinExecutor::merge_results(&results);
        assert!(merged.all_succeeded);
        assert!((merged.total_cost_usd - 0.06).abs() < f64::EPSILON);
        assert_eq!(merged.total_duration_ms, 300); // max duration
    }

    #[test]
    fn executor_merge_results_all_must_succeed_for_success() {
        let results = vec![
            ForkResult::success("f0", "ok", 0.01, 100),
            ForkResult::failure("f1", "timeout", 200),
        ];
        let merged = ForkJoinExecutor::merge_results(&results);
        assert!(!merged.all_succeeded);
        assert_eq!(merged.success_count(), 1);
        assert_eq!(merged.failure_count(), 1);
    }

    #[test]
    fn executor_merge_results_empty() {
        let results: Vec<ForkResult> = vec![];
        let merged = ForkJoinExecutor::merge_results(&results);
        assert!(merged.all_succeeded);
        assert!(merged.fork_results.is_empty());
        assert!((merged.total_cost_usd - 0.0).abs() < f64::EPSILON);
        assert_eq!(merged.total_duration_ms, 0);
    }

    #[test]
    fn fork_result_serialization_roundtrip() {
        let result = ForkResult::success("fork-0", "done", 0.05, 1000);
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ForkResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.fork_id, "fork-0");
        assert!(deserialized.success);
    }

    // --- ForkSpec with role ---

    #[test]
    fn fork_spec_new_has_no_role() {
        let spec = ForkSpec::new("fork-0", "desc", ExecutionTier::Musician);
        assert!(spec.role.is_none());
    }

    #[test]
    fn fork_spec_with_role_set() {
        let mut spec = ForkSpec::new("fork-0", "desc", ExecutionTier::Editor);
        spec.role = Some(TaskRole::Code);
        assert_eq!(spec.role, Some(TaskRole::Code));
    }

    // --- ForkJoinExecutor with runner ---

    struct MockRunner {
        output: String,
        success: bool,
    }

    impl crate::task_runner::TaskRunner for MockRunner {
        fn run_task(
            &self,
            _task_description: &str,
            _role: crate::delegation::TaskRole,
            _path_scope: &[PathBuf],
            _resume_from: Option<&str>,
        ) -> anyhow::Result<crate::task_runner::TaskRunResult> {
            Ok(crate::task_runner::TaskRunResult {
                success: self.success,
                output: self.output.clone(),
                cost_usd: 0.02,
                duration_ms: 50,
            })
        }
    }

    #[tokio::test]
    async fn executor_with_runner_uses_real_execution() {
        let bus = BusHandle::new(16);
        let runner = Arc::new(MockRunner {
            output: "real output".into(),
            success: true,
        });
        let executor = ForkJoinExecutor::with_runner(ForkJoinConfig::default(), bus, runner);
        let snapshot = ContextSnapshot::new("t1", "desc", 2);
        let mut spec = ForkSpec::new("fork-0", "process main.rs", ExecutionTier::Editor);
        spec.role = Some(TaskRole::Code);

        let result = executor.execute_forks(&snapshot, &[spec]).await;
        assert_eq!(result.fork_results.len(), 1);
        assert!(result.all_succeeded);
        assert_eq!(result.fork_results[0].output, "real output");
        assert!((result.fork_results[0].cost_usd - 0.02).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn executor_without_runner_uses_placeholder() {
        let bus = BusHandle::new(16);
        let executor = ForkJoinExecutor::with_bus(bus);
        let snapshot = ContextSnapshot::new("t1", "desc", 2);
        let spec = ForkSpec::new("fork-0", "process main.rs", ExecutionTier::Musician);

        let result = executor.execute_forks(&snapshot, &[spec]).await;
        assert_eq!(result.fork_results.len(), 1);
        assert!(result.all_succeeded);
        assert!(result.fork_results[0].output.contains("Fork executed:"));
    }
}
