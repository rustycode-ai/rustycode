//! Worker Registry — Sub-agent lifecycle tracking.
//!
//! This module provides centralized tracking of spawned sub-agents:
//! - Worker status lifecycle (Spawning → Running → Finished/Failed)
//! - Task assignment tracking
//! - Completion state and result tracking
//! - Event history for debugging
//!
//! Inspired by claw-code's worker_boot module.
//!
//! # Architecture
//!
//! ```text
//! SpawnAgentTool → WorkerRegistry::spawn() → Worker { status, task, events }
//!                                              │
//!                              ┌───────────────┼───────────────┐
//!                              │               │               │
//!                         Spawning         Running        Finished
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Worker status lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkerStatus {
    /// Worker is being spawned
    Spawning,
    /// Worker is ready to receive prompt
    ReadyForPrompt,
    /// Worker is actively processing task
    Running,
    /// Worker completed successfully
    Finished,
    /// Worker failed or errored
    Failed,
}

impl std::fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawning => write!(f, "spawning"),
            Self::ReadyForPrompt => write!(f, "ready_for_prompt"),
            Self::Running => write!(f, "running"),
            Self::Finished => write!(f, "finished"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Type of worker failure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkerFailureKind {
    /// Failed to spawn
    SpawnFailed,
    /// Provider API error
    ProviderError,
    /// Protocol/communication error
    ProtocolError,
    /// Task execution error
    ExecutionError,
}

impl std::fmt::Display for WorkerFailureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpawnFailed => write!(f, "spawn_failed"),
            Self::ProviderError => write!(f, "provider_error"),
            Self::ProtocolError => write!(f, "protocol_error"),
            Self::ExecutionError => write!(f, "execution_error"),
        }
    }
}

/// Worker failure details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerFailure {
    pub kind: WorkerFailureKind,
    pub message: String,
    pub created_at: u64,
}

/// Event types in worker lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "event_type")]
#[non_exhaustive]
pub enum WorkerEvent {
    StatusChanged {
        from: WorkerStatus,
        to: WorkerStatus,
        reason: Option<String>,
        timestamp: u64,
    },
    TaskAssigned {
        task_id: String,
        task_description: String,
        timestamp: u64,
    },
    TaskCompleted {
        result_summary: String,
        timestamp: u64,
    },
    TaskFailed {
        error: WorkerFailure,
        timestamp: u64,
    },
    ToolCalled {
        tool_name: String,
        target: String,
        timestamp: u64,
    },
}

/// A tracked worker (sub-agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    /// Unique worker identifier (sortable, time-based)
    pub worker_id: String,
    pub status: WorkerStatus,
    /// Working directory for this worker
    pub cwd: String,
    /// Task ID if assigned
    pub task_id: Option<String>,
    pub task_description: Option<String>,
    pub trust_gate_cleared: bool,
    /// Last error if any
    pub last_error: Option<WorkerFailure>,
    /// Result summary on completion
    pub result_summary: Option<String>,
    /// Creation timestamp (unix epoch seconds)
    pub created_at: u64,
    /// Last update timestamp (unix epoch seconds)
    pub updated_at: u64,
    /// Event history for debugging
    pub events: Vec<WorkerEvent>,
}

impl Worker {
    const MAX_EVENTS: usize = 10;

    fn push_event(&mut self, event: WorkerEvent) {
        if self.events.len() >= Self::MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
        self.updated_at = now_secs();
    }

    /// Record a tool call for this worker.
    pub fn record_tool_call(&mut self, tool_name: impl Into<String>, target: impl Into<String>) {
        self.push_event(WorkerEvent::ToolCalled {
            tool_name: tool_name.into(),
            target: target.into(),
            timestamp: now_secs(),
        });
    }

    fn transition_status(&mut self, new_status: WorkerStatus, reason: Option<String>) {
        let old_status = self.status;
        self.push_event(WorkerEvent::StatusChanged {
            from: old_status,
            to: new_status,
            reason,
            timestamp: now_secs(),
        });
        self.status = new_status;
    }
}

/// Internal registry state
#[derive(Debug, Default)]
struct WorkerRegistryInner {
    workers: HashMap<String, Worker>,
    counter: u64,
}

/// Registry for tracking sub-agent lifecycle
///
/// Provides centralized state management for spawned workers,
/// enabling status queries, task tracking, and debugging.
#[derive(Debug, Clone, Default)]
pub struct WorkerRegistry {
    inner: Arc<Mutex<WorkerRegistryInner>>,
}

impl WorkerRegistry {
    /// Create a new empty worker registry
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn a new worker and register it
    ///
    #[must_use]
    pub fn spawn(&self, cwd: &str) -> Worker {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        inner.counter += 1;
        let ts = now_secs();
        let worker_id = format!("wkr_{:08x}_{:04x}", ts, inner.counter);

        let mut worker = Worker {
            worker_id: worker_id.clone(),
            status: WorkerStatus::Spawning,
            cwd: cwd.to_owned(),
            task_id: None,
            task_description: None,
            trust_gate_cleared: false,
            last_error: None,
            result_summary: None,
            created_at: ts,
            updated_at: ts,
            events: Vec::new(),
        };

        worker.push_event(WorkerEvent::StatusChanged {
            from: WorkerStatus::Spawning,
            to: WorkerStatus::Spawning,
            reason: Some("worker spawned".to_string()),
            timestamp: ts,
        });

        inner.workers.insert(worker_id, worker.clone());
        worker
    }

    /// Get a worker by ID
    #[must_use]
    pub fn get(&self, worker_id: &str) -> Option<Worker> {
        let inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        inner.workers.get(worker_id).cloned()
    }

    /// List all workers
    #[must_use]
    pub fn list(&self) -> Vec<Worker> {
        let inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        inner.workers.values().cloned().collect()
    }

    /// Assign a task to a worker
    ///
    pub fn assign_task(
        &self,
        worker_id: &str,
        task_id: &str,
        description: &str,
    ) -> Result<Worker, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        let worker = inner
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| format!("worker not found: {worker_id}"))?;

        worker.task_id = Some(task_id.to_owned());
        worker.task_description = Some(description.to_owned());
        worker.transition_status(
            WorkerStatus::ReadyForPrompt,
            Some("task assigned".to_string()),
        );

        worker.push_event(WorkerEvent::TaskAssigned {
            task_id: task_id.to_owned(),
            task_description: description.to_owned(),
            timestamp: now_secs(),
        });

        Ok(worker.clone())
    }

    /// Mark worker as running (processing task)
    ///
    pub fn mark_running(&self, worker_id: &str) -> Result<Worker, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        let worker = inner
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| format!("worker not found: {worker_id}"))?;

        worker.transition_status(
            WorkerStatus::Running,
            Some("task execution started".to_string()),
        );
        Ok(worker.clone())
    }

    /// Record a tool call for a worker.
    pub fn record_tool_call(
        &self,
        worker_id: &str,
        tool_name: &str,
        target: &str,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        let worker = inner
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| format!("worker not found: {worker_id}"))?;

        worker.record_tool_call(tool_name, target);
        Ok(())
    }

    /// Mark worker as finished (successful completion)
    ///
    pub fn mark_finished(&self, worker_id: &str, result_summary: &str) -> Result<Worker, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        let worker = inner
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| format!("worker not found: {worker_id}"))?;

        worker.result_summary = Some(result_summary.to_owned());
        worker.transition_status(
            WorkerStatus::Finished,
            Some("task completed successfully".to_string()),
        );

        worker.push_event(WorkerEvent::TaskCompleted {
            result_summary: result_summary.to_owned(),
            timestamp: now_secs(),
        });

        Ok(worker.clone())
    }

    /// Mark worker as failed
    ///
    pub fn mark_failed(
        &self,
        worker_id: &str,
        kind: WorkerFailureKind,
        message: &str,
    ) -> Result<Worker, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        let worker = inner
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| format!("worker not found: {worker_id}"))?;

        let failure = WorkerFailure {
            kind,
            message: message.to_owned(),
            created_at: now_secs(),
        };

        worker.last_error = Some(failure.clone());
        worker.transition_status(WorkerStatus::Failed, Some(failure.message.clone()));

        worker.push_event(WorkerEvent::TaskFailed {
            error: failure,
            timestamp: now_secs(),
        });

        Ok(worker.clone())
    }

    /// Remove a worker from the registry
    ///
    #[must_use]
    pub fn remove(&self, worker_id: &str) -> Option<Worker> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        inner.workers.remove(worker_id)
    }

    /// Get count of workers
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        inner.workers.len()
    }

    /// Check if registry is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get workers by status
    #[must_use]
    pub fn workers_by_status(&self, status: WorkerStatus) -> Vec<Worker> {
        let inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        inner
            .workers
            .values()
            .filter(|w| w.status == status)
            .cloned()
            .collect()
    }

    /// Clear trust gate for a worker (auto-resolve)
    ///
    pub fn clear_trust_gate(&self, worker_id: &str) -> Result<Worker, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("worker_registry mutex poisoned, recovering: {e}");
            e.into_inner()
        });
        let worker = inner
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| format!("worker not found: {worker_id}"))?;

        worker.trust_gate_cleared = true;
        Ok(worker.clone())
    }
}

// ── Global Registry Accessor ────────────────────────────────────────────────────────

use std::sync::OnceLock;

/// Global worker registry accessor for centralized state management.
///
/// This follows the claw-code pattern of using OnceLock for global registries,
/// enabling any part of the codebase to access shared state without threading
/// `Arc<Registry>` through every layer.
///
/// # Example
///
/// ```
/// use rustycode_protocol::worker_registry::global_worker_registry;
/// let registry = global_worker_registry();
/// let workers = registry.list();
/// ```
pub fn global_worker_registry() -> &'static WorkerRegistry {
    static REGISTRY: OnceLock<WorkerRegistry> = OnceLock::new();
    REGISTRY.get_or_init(WorkerRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_lifecycle() {
        let registry = WorkerRegistry::new();

        // Spawn worker
        let worker = registry.spawn("/path/to/project");
        assert_eq!(worker.status, WorkerStatus::Spawning);
        assert!(!worker.trust_gate_cleared);

        // Assign task
        let worker = registry
            .assign_task(&worker.worker_id, "task_001", "Fix the bug")
            .unwrap();
        assert_eq!(worker.status, WorkerStatus::ReadyForPrompt);
        assert_eq!(worker.task_id, Some("task_001".to_string()));

        // Mark running
        let worker = registry.mark_running(&worker.worker_id).unwrap();
        assert_eq!(worker.status, WorkerStatus::Running);

        // Mark finished
        let worker = registry
            .mark_finished(&worker.worker_id, "Fixed the null pointer bug")
            .unwrap();
        assert_eq!(worker.status, WorkerStatus::Finished);
        assert_eq!(
            worker.result_summary,
            Some("Fixed the null pointer bug".to_string())
        );
    }

    #[test]
    fn test_worker_failure() {
        let registry = WorkerRegistry::new();
        let worker = registry.spawn("/path/to/project");

        let worker = registry
            .assign_task(&worker.worker_id, "task_002", "Refactor auth")
            .unwrap();

        let worker = registry
            .mark_failed(
                &worker.worker_id,
                WorkerFailureKind::ProviderError,
                "API rate limit exceeded",
            )
            .unwrap();

        assert_eq!(worker.status, WorkerStatus::Failed);
        assert!(worker.last_error.is_some());
        assert_eq!(
            worker.last_error.unwrap().kind,
            WorkerFailureKind::ProviderError
        );
    }

    #[test]
    fn test_worker_list_and_filter() {
        let registry = WorkerRegistry::new();

        let w1 = registry.spawn("/project1");
        let _w2 = registry.spawn("/project2");
        let _w3 = registry.spawn("/project3");

        // All should be spawning
        assert_eq!(registry.len(), 3);
        assert_eq!(registry.workers_by_status(WorkerStatus::Spawning).len(), 3);

        // Transition w1 to running
        registry.assign_task(&w1.worker_id, "t1", "task 1").unwrap();
        registry.mark_running(&w1.worker_id).unwrap();

        assert_eq!(registry.workers_by_status(WorkerStatus::Running).len(), 1);
        assert_eq!(registry.workers_by_status(WorkerStatus::Spawning).len(), 2);
    }

    #[test]
    fn test_worker_not_found() {
        let registry = WorkerRegistry::new();

        assert!(registry.get("nonexistent").is_none());
        assert!(registry.assign_task("nonexistent", "t1", "desc").is_err());
        assert!(registry.mark_running("nonexistent").is_err());
        assert!(registry.mark_finished("nonexistent", "result").is_err());
        assert!(registry
            .mark_failed("nonexistent", WorkerFailureKind::SpawnFailed, "err")
            .is_err());
        assert!(registry.remove("nonexistent").is_none());
    }

    #[test]
    fn test_worker_id_format() {
        let registry = WorkerRegistry::new();
        let worker = registry.spawn("/test");

        // Worker ID should start with "wkr_"
        assert!(worker.worker_id.starts_with("wkr_"));

        // Worker ID should contain timestamp and counter
        let parts: Vec<&str> = worker.worker_id.split('_').collect();
        assert_eq!(parts.len(), 3); // "wkr", timestamp, counter
    }

    #[test]
    fn test_event_history() {
        let registry = WorkerRegistry::new();
        let worker = registry.spawn("/test");

        // Should have at least one event (spawn)
        assert!(!worker.events.is_empty());

        let worker = registry
            .assign_task(&worker.worker_id, "t1", "test task")
            .unwrap();

        // Should have more events now
        assert!(worker.events.len() >= 2);

        // Last event should be task assignment
        match worker.events.last().unwrap() {
            WorkerEvent::TaskAssigned { task_id, .. } => {
                assert_eq!(task_id, "t1");
            }
            _ => panic!("Expected TaskAssigned event"),
        }
    }

    #[test]
    fn test_global_registry() {
        // First call initializes
        let registry1 = global_worker_registry();
        let worker = registry1.spawn("/test");

        // Second call returns same registry
        let registry2 = global_worker_registry();
        let retrieved = registry2.get(&worker.worker_id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().worker_id, worker.worker_id);
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let registry = Arc::new(WorkerRegistry::new());
        let registry_clone = Arc::clone(&registry);

        let handle = thread::spawn(move || {
            let worker = registry_clone.spawn("/thread-test");
            registry_clone
                .assign_task(&worker.worker_id, "t1", "thread task")
                .unwrap()
        });

        let worker = registry.spawn("/main-thread");
        let thread_worker = handle.join().unwrap();

        assert!(registry.get(&worker.worker_id).is_some());
        assert!(registry.get(&thread_worker.worker_id).is_some());
    }
}
