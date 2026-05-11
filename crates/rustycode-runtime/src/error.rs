//! Typed error types for the runtime crate.
//!
//! Replaces `Result<T, String>` with proper thiserror-based error enums.
//! Each module domain has its own error type, all convertible to [`RuntimeError`].

use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type for the runtime crate.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("resource error: {0}")]
    Resource(#[from] ResourceError),

    #[error("task error: {0}")]
    Task(#[from] TaskError),

    #[error("monitoring error: {0}")]
    Monitoring(#[from] MonitoringError),

    #[error("worktree error: {0}")]
    Worktree(#[from] WorktreeError),

    #[error("event error: {0}")]
    Event(#[from] EventError),

    #[error("benchmark error: {0}")]
    Benchmark(#[from] BenchmarkError),

    #[error("execution error: {0}")]
    Execution(#[from] ExecutionError),

    #[error("service discovery error: {0}")]
    ServiceDiscovery(#[from] ServiceDiscoveryError),

    #[error("intent error: {0}")]
    Intent(#[from] IntentError),

    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Agent errors (agent_lifecycle, agent_health, agent_learning)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("maximum agents ({max}) reached for role {role:?}")]
    MaxAgentsReached { max: usize, role: String },

    #[error("agent {agent_id} cannot transition from {from:?} to {to:?}")]
    InvalidTransition {
        agent_id: String,
        from: String,
        to: String,
    },

    #[error("agent {agent_id} is not in Error state")]
    NotInErrorState { agent_id: String },

    #[error("cannot terminate agent {agent_id}: {dependent_count} agents depend on it")]
    HasDependents {
        agent_id: String,
        dependent_count: usize,
    },

    #[error("agent {agent_id} has exceeded max restart attempts ({max_attempts})")]
    MaxRestartAttempts { agent_id: String, max_attempts: u32 },

    #[error("agent {agent_id} not found")]
    NotFound { agent_id: String },

    #[error("agent {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Resource errors (resource_manager)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error("insufficient capacity: requested {requested}, available {available}")]
    InsufficientCapacity { requested: f64, available: f64 },

    #[error("cannot allocate more than reserved: requested {requested}, reserved {reserved}")]
    OverAllocation { requested: f64, reserved: f64 },

    #[error("cannot release more than allocated: requested {requested}, allocated {allocated}")]
    OverRelease { requested: f64, allocated: f64 },

    #[error("cannot cancel more than reserved: requested {requested}, reserved {reserved}")]
    OverCancel { requested: f64, reserved: f64 },

    #[error("reservation {0} not found")]
    ReservationNotFound(String),

    #[error("resource {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Task errors (task_scheduler)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task {0} not found in active tasks")]
    NotFound(String),

    #[error("task {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Monitoring errors (monitoring)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum MonitoringError {
    #[error("metric {0} already registered")]
    MetricAlreadyRegistered(String),

    #[error("no data found for metric {0}")]
    NoData(String),

    #[error("alert {0} not found")]
    AlertNotFound(String),

    #[error("monitoring {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Worktree errors (git_worktree)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("failed to create worktree: {0}")]
    CreationFailed(String),

    #[error("failed to remove worktree: {0}")]
    RemovalFailed(String),

    #[error("failed to prune worktrees: {0}")]
    PruneFailed(String),

    #[error("git error: {0}")]
    GitError(String),

    #[error("io error: {source} (path: {path})")]
    Io {
        #[source]
        source: std::io::Error,
        path: PathBuf,
    },

    #[error("worktree {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Event errors (event_system)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum EventError {
    #[error("failed to publish event: {0}")]
    PublishFailed(String),

    #[error("subscription {0} not found")]
    SubscriptionNotFound(String),

    #[error("event {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Benchmark errors (benchmark/)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BenchmarkError {
    #[error("task execution failed: {0}")]
    TaskFailed(String),

    #[error("failed to save results: {0}")]
    SaveFailed(String),

    #[error("failed to load results: {0}")]
    LoadFailed(String),

    #[error("benchmark {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Execution errors (parallel_executor)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("repository path does not exist: {0}")]
    RepoNotFound(PathBuf),

    #[error("git add failed: {0}")]
    GitAddFailed(String),

    #[error("git commit failed: {0}")]
    GitCommitFailed(String),

    #[error("git log failed: {0}")]
    GitLogFailed(String),

    #[error("cherry-pick conflict on {branch}: {details}")]
    CherryPickConflict { branch: String, details: String },

    #[error("merge conflict: {0}")]
    MergeConflict(String),

    #[error("failed to create temp branch: {0}")]
    TempBranchFailed(String),

    #[error("checkout back to '{branch}' failed: {details}")]
    CheckoutFailed { branch: String, details: String },

    #[error("fast-forward failed: {0}")]
    FastForwardFailed(String),

    #[error("rebase conflict: {0}")]
    RebaseConflict(String),

    #[error("execution {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Service discovery errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ServiceDiscoveryError {
    #[error("service instance {0} not found")]
    InstanceNotFound(String),

    #[error("service discovery {0}")]
    Custom(String),
}

// ---------------------------------------------------------------------------
// Intent errors (orchestration/llm_intent)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("unknown category: {0}")]
    UnknownCategory(String),
}
