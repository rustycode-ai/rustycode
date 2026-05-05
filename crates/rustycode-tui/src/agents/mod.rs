//! Agent lifecycle management for TUI
//!
//! Submodules:
//! - `definitions`: Built-in agent types with `when_to_use` auto-activation descriptions
//! - `agent_tool`: Functional Agent tool backed by AgentSession
//! - `delegation_executor`: Delegation executor that runs real sub-agent sessions
//!
//! Real-time tracking and management of spawned agents:
//! - Spawn agents with specific tasks
//! - Monitor agent progress in real-time
//! - Display agent results when complete
//! - Manage agent lifecycle (cancel, retry, cleanup)
//! - Handle failures with proper error recovery
//!
//! ## Performance Monitoring
//!
//! Agent execution is instrumented with metrics for:
//! - Execution time tracking
//! - Retry counts and patterns
//! - Timeout occurrences
//! - Cancellation events
//! - Success/failure rates

pub mod agent_tool;
pub mod definitions;
pub mod delegation_executor;

use anyhow::Result;
use rustycode_runtime::multi_agent::{
    AgentResponse, AgentRole, MultiAgentConfig, MultiAgentOrchestrator,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Unique identifier for an agent task
pub type AgentId = usize;

/// Performance metrics for agent execution
#[derive(Debug)]
pub struct AgentMetrics {
    /// Total agents spawned
    pub total_spawned: AtomicU64,
    /// Total agents completed successfully
    pub total_completed: AtomicU64,
    /// Total agents failed
    pub total_failed: AtomicU64,
    /// Total agents cancelled
    pub total_cancelled: AtomicU64,
    /// Total agents timed out
    pub total_timed_out: AtomicU64,
    /// Total retry attempts
    pub total_retries: AtomicU64,
}

impl AgentMetrics {
    pub fn new() -> Self {
        Self {
            total_spawned: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
            total_cancelled: AtomicU64::new(0),
            total_timed_out: AtomicU64::new(0),
            total_retries: AtomicU64::new(0),
        }
    }

    /// Record agent spawned
    pub fn record_spawned(&self) {
        self.total_spawned.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            "Agent metrics: spawned (total: {})",
            self.total_spawned.load(Ordering::Relaxed)
        );
    }

    /// Record agent completed
    pub fn record_completed(&self) {
        self.total_completed.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            "Agent metrics: completed (total: {})",
            self.total_completed.load(Ordering::Relaxed)
        );
    }

    /// Record agent failed
    pub fn record_failed(&self) {
        self.total_failed.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            "Agent metrics: failed (total: {})",
            self.total_failed.load(Ordering::Relaxed)
        );
    }

    /// Record agent cancelled
    pub fn record_cancelled(&self) {
        self.total_cancelled.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            "Agent metrics: cancelled (total: {})",
            self.total_cancelled.load(Ordering::Relaxed)
        );
    }

    /// Record agent timed out
    pub fn record_timed_out(&self) {
        self.total_timed_out.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            "Agent metrics: timed out (total: {})",
            self.total_timed_out.load(Ordering::Relaxed)
        );
    }

    /// Record retry attempt
    pub fn record_retry(&self) {
        self.total_retries.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            "Agent metrics: retry (total: {})",
            self.total_retries.load(Ordering::Relaxed)
        );
    }

    /// Get success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.total_completed.load(Ordering::Relaxed)
            + self.total_failed.load(Ordering::Relaxed)
            + self.total_cancelled.load(Ordering::Relaxed)
            + self.total_timed_out.load(Ordering::Relaxed);

        if total == 0 {
            return 0.0;
        }

        self.total_completed.load(Ordering::Relaxed) as f64 / total as f64
    }

    /// Get all metrics as a summary
    pub fn summary(&self) -> String {
        format!(
            "Agents: {} spawned, {} completed, {} failed, {} cancelled, {} timed out | Success rate: {:.1}%",
            self.total_spawned.load(Ordering::Relaxed),
            self.total_completed.load(Ordering::Relaxed),
            self.total_failed.load(Ordering::Relaxed),
            self.total_cancelled.load(Ordering::Relaxed),
            self.total_timed_out.load(Ordering::Relaxed),
            self.success_rate() * 100.0
        )
    }
}

impl Default for AgentMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentStatus {
    /// Agent is waiting to start
    Pending,
    /// Agent is currently running
    Running,
    /// Agent completed successfully
    Completed,
    /// Agent failed with error
    Failed,
}

/// Information about a running agent
#[derive(Debug, Clone)]
pub struct AgentTask {
    /// Unique identifier
    pub id: AgentId,
    /// Agent role
    pub role: AgentRole,
    /// Current status
    pub status: AgentStatus,
    /// Task description
    pub task: String,
    /// Start time
    pub started_at: Instant,
    /// Elapsed time (updates while running)
    pub elapsed_secs: u64,
    /// Result (when complete)
    pub result: Option<AgentResponse>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Timeout in seconds
    pub timeout_secs: u64,
    /// Retry count
    pub retry_count: u32,
    /// Max retries
    pub max_retries: u32,
    /// Cancellation flag
    cancelled: Arc<AtomicBool>,
    /// When the agent reached a terminal state (Completed/Failed)
    completed_at: Option<Instant>,
}

impl AgentTask {
    pub fn new(id: AgentId, role: AgentRole, task: String) -> Self {
        Self {
            id,
            role,
            status: AgentStatus::Pending,
            task,
            started_at: Instant::now(),
            elapsed_secs: 0,
            result: None,
            error: None,
            timeout_secs: 300, // Default 5 minutes
            retry_count: 0,
            max_retries: 3,
            cancelled: Arc::new(AtomicBool::new(false)),
            completed_at: None,
        }
    }

    /// Create a new agent task with custom timeout
    pub fn with_timeout(id: AgentId, role: AgentRole, task: String, timeout_secs: u64) -> Self {
        Self {
            id,
            role,
            status: AgentStatus::Pending,
            task,
            started_at: Instant::now(),
            elapsed_secs: 0,
            result: None,
            error: None,
            timeout_secs,
            retry_count: 0,
            max_retries: 3,
            cancelled: Arc::new(AtomicBool::new(false)),
            completed_at: None,
        }
    }

    /// Update elapsed time
    pub fn update_elapsed(&mut self) {
        self.elapsed_secs = self.started_at.elapsed().as_secs();
    }

    /// Get status icon for display
    pub fn status_icon(&self) -> &str {
        match self.status {
            AgentStatus::Pending => "⏳",
            AgentStatus::Running => "🔄",
            AgentStatus::Completed => "✅",
            AgentStatus::Failed => "❌",
        }
    }

    /// Get formatted elapsed time
    pub fn formatted_time(&self) -> String {
        if self.elapsed_secs < 60 {
            format!("{}s", self.elapsed_secs)
        } else {
            let mins = self.elapsed_secs / 60;
            let secs = self.elapsed_secs % 60;
            format!("{}m {}s", mins, secs)
        }
    }
}

/// Agent manager - tracks and manages running agents
#[derive(Clone)]
pub struct AgentManager {
    /// Active agents (id -> task)
    agents: Arc<Mutex<HashMap<AgentId, AgentTask>>>,
    /// Next agent ID
    next_id: Arc<Mutex<AgentId>>,
    /// Performance metrics
    metrics: Arc<AgentMetrics>,
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
            metrics: Arc::new(AgentMetrics::new()),
        }
    }

    /// Spawn a new agent with a task
    pub fn spawn_agent(
        &self,
        role: AgentRole,
        task: String,
        content: String,
        workspace_context: Option<String>,
    ) -> Result<AgentId> {
        // Generate unique ID
        let id = {
            let mut next = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            let id = *next;
            *next += 1;
            id
        };

        // Create agent task
        let agent_task = AgentTask::new(id, role, task);

        // Store in agents map
        {
            let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
            agents.insert(id, agent_task);
        }

        // Record agent spawn in metrics
        self.metrics.record_spawned();

        // Spawn background task to run the agent
        let agents_arc = self.agents.clone();
        let role_clone = role;
        let id_copy = id;
        let metrics_arc = self.metrics.clone();

        std::thread::spawn(move || {
            Self::run_agent_background(
                agents_arc,
                id_copy,
                role_clone,
                content,
                workspace_context,
                metrics_arc,
            );
        });

        Ok(id)
    }

    /// Get all agents
    pub fn agents(&self) -> Vec<AgentTask> {
        let agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents.values().cloned().collect()
    }

    /// Get a specific agent by ID
    pub fn agent(&self, id: AgentId) -> Option<AgentTask> {
        let agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents.get(&id).cloned()
    }

    /// Cancel an agent (marks as failed and stops background execution)
    pub fn cancel_agent(&self, id: AgentId) -> Result<()> {
        let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(agent) = agents.get_mut(&id) {
            // Set cancellation flag (background thread will check this and record metric)
            agent.cancelled.store(true, Ordering::Relaxed);
            agent.status = AgentStatus::Failed;
            agent.error = Some("Cancelled by user".to_string());
            Ok(())
        } else {
            anyhow::bail!("Agent {} not found", id)
        }
    }

    /// Retry a failed agent
    pub fn retry_agent(&self, id: AgentId) -> Result<()> {
        // Record retry in metrics
        self.metrics.record_retry();

        // Extract agent data while holding lock
        let (role, task, _cancelled) = {
            let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(agent) = agents.get_mut(&id) {
                // Only allow retrying failed agents
                if agent.status != AgentStatus::Failed {
                    anyhow::bail!(
                        "Agent {} is not in a failed state (status: {:?})",
                        id,
                        agent.status
                    );
                }

                // Reset for retry
                let role = agent.role;
                let task = agent.task.clone();
                let cancelled = agent.cancelled.clone();

                // Reset status
                agent.status = AgentStatus::Pending;
                agent.error = None;
                agent.retry_count = agent.retry_count.saturating_add(1);
                agent.elapsed_secs = 0;
                agent.started_at = Instant::now();

                // Clear cancellation flag
                cancelled.store(false, Ordering::Relaxed);

                (role, task, cancelled)
            } else {
                anyhow::bail!("Agent {} not found", id)
            }
        };

        // Prepare execution data (outside lock)
        let content = format!("Retry: {}", task);
        let workspace_context = None;

        // Spawn new execution
        let agents_arc = self.agents.clone();
        let metrics_arc = self.metrics.clone();
        std::thread::spawn(move || {
            Self::run_agent_background(
                agents_arc,
                id,
                role,
                content,
                workspace_context,
                metrics_arc,
            );
        });

        Ok(())
    }

    /// Clean up completed/failed agents older than specified duration.
    /// Also removes old completed/failed agents when total count exceeds `max_kept`.
    pub fn cleanup_old_agents(&self, older_than_secs: u64) {
        let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents.retain(|_, agent| {
            // Always keep running/pending agents
            if agent.status == AgentStatus::Pending || agent.status == AgentStatus::Running {
                return true;
            }
            // Keep completed/failed agents that finished recently
            agent
                .completed_at
                .is_none_or(|t| t.elapsed().as_secs() < older_than_secs)
        });
    }

    /// Remove oldest completed/failed agents when total exceeds `max_agents`.
    /// Returns the number of agents removed.
    pub fn cleanup_excess_agents(&self, max_agents: usize) -> usize {
        let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());

        let terminal_count = agents
            .values()
            .filter(|a| a.status == AgentStatus::Completed || a.status == AgentStatus::Failed)
            .count();

        if terminal_count <= max_agents {
            return 0;
        }

        let to_remove = terminal_count - max_agents;
        let mut removed = 0;

        // Collect IDs of terminal agents sorted by elapsed time (oldest first)
        let mut terminal_ids: Vec<_> = agents
            .iter()
            .filter(|(_, a)| a.status == AgentStatus::Completed || a.status == AgentStatus::Failed)
            .map(|(id, a)| (*id, a.elapsed_secs))
            .collect();
        terminal_ids.sort_by_key(|(_, elapsed)| std::cmp::Reverse(*elapsed));

        for (id, _) in terminal_ids {
            if removed >= to_remove {
                break;
            }
            agents.remove(&id);
            removed += 1;
        }

        removed
    }

    /// Update elapsed time for all running agents
    pub fn update_running_agents(&self) {
        let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
        for agent in agents.values_mut() {
            if agent.status == AgentStatus::Running {
                agent.update_elapsed();
            }
        }
    }

    /// Get the current metrics summary
    pub fn get_metrics_summary(&self) -> String {
        self.metrics.summary()
    }

    /// Get the current success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        self.metrics.success_rate()
    }

    /// Run an agent in the background with timeout and cancellation support
    fn run_agent_background(
        agents: Arc<Mutex<HashMap<AgentId, AgentTask>>>,
        id: AgentId,
        role: AgentRole,
        content: String,
        workspace_context: Option<String>,
        metrics: Arc<AgentMetrics>,
    ) {
        // Get timeout before starting
        let timeout_secs = {
            let agents_guard = agents.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(agent) = agents_guard.get(&id) {
                agent.timeout_secs
            } else {
                // Agent was removed, exit early
                return;
            }
        };

        // Get cancellation flag (clone it here to avoid move issues)
        let cancelled_flag = {
            let agents_guard = agents.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(agent) = agents_guard.get(&id) {
                agent.cancelled.clone()
            } else {
                // Agent was removed, exit early
                return;
            }
        };

        // Mark as running
        {
            let mut agents_guard = agents.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(agent) = agents_guard.get_mut(&id) {
                agent.status = AgentStatus::Running;
            }
        }

        // Create multi-agent config
        let config = MultiAgentConfig {
            roles: vec![role],
            max_parallelism: 1,
            context: workspace_context.clone().unwrap_or_default(),
            content: content.clone(),
            file_path: None,
            instructions: None,
        };

        // Spawn thread with timeout monitoring
        let agents_clone = agents.clone();
        let cancelled_flag_final = cancelled_flag.clone();
        let result = std::thread::spawn(move || {
            // Create runtime with timeout
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| anyhow::anyhow!("Failed to create runtime: {}", e));

            match rt {
                Ok(runtime) => {
                    // Set up timeout
                    let timeout_duration = Duration::from_secs(timeout_secs);
                    let agents_ref = agents_clone.clone();
                    let id_copy = id;

                    // Spawn timeout monitor (clone cancelled_flag for this closure)
                    let cancel_monitor = cancelled_flag.clone();
                    let monitor_handle = std::thread::spawn(move || {
                        let start = Instant::now();
                        loop {
                            std::thread::sleep(Duration::from_secs(1));

                            // Check cancellation
                            if cancel_monitor.load(Ordering::Relaxed) {
                                tracing::info!("Agent {} cancelled by user", id_copy);
                                return;
                            }

                            // Check timeout
                            if start.elapsed() >= timeout_duration {
                                tracing::warn!(
                                    "Agent {} timed out after {}s",
                                    id_copy,
                                    timeout_secs
                                );
                                return;
                            }

                            // Check if agent is still running
                            let agents = agents_ref.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(agent) = agents.get(&id_copy) {
                                if agent.status != AgentStatus::Running {
                                    break;
                                }
                            } else {
                                break; // Agent was removed
                            }
                        }
                    });

                    // Run the agent
                    let result = runtime.block_on(async {
                        let orchestrator = MultiAgentOrchestrator::from_config(config)?;
                        orchestrator.analyze().await
                    });

                    // Join monitor thread (ignore errors if it's already done)
                    let _ = monitor_handle.join();

                    result
                }
                Err(e) => Err(e),
            }
        })
        .join();

        // Update agent status based on result with retry logic
        let mut agents_guard = agents.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(agent) = agents_guard.get_mut(&id) {
            match result {
                Ok(Ok(analysis)) => {
                    // Extract the single agent response
                    if let Some(response) = analysis.agent_responses.first() {
                        agent.result = Some(response.clone());
                    }
                    agent.status = AgentStatus::Completed;
                    // Record completion in metrics
                    metrics.record_completed();
                }
                Ok(Err(e)) => {
                    // Check if we should retry
                    if agent.retry_count < agent.max_retries {
                        agent.retry_count += 1;
                        agent.error = Some(format!(
                            "Retry {}/{}: {}",
                            agent.retry_count, agent.max_retries, e
                        ));

                        // Record retry in metrics
                        metrics.record_retry();

                        // Re-queue for retry (spawn new thread)
                        let agents_clone = agents.clone();
                        let role_clone = role;
                        let content_clone = content.clone();
                        let workspace_clone = workspace_context.clone();
                        let metrics_clone = metrics.clone();

                        std::thread::spawn(move || {
                            Self::run_agent_background(
                                agents_clone,
                                id,
                                role_clone,
                                content_clone,
                                workspace_clone,
                                metrics_clone,
                            );
                        });
                    } else {
                        agent.status = AgentStatus::Failed;
                        agent.error = Some(format!(
                            "Agent failed after {} retries: {}",
                            agent.max_retries, e
                        ));
                        // Record failure in metrics
                        metrics.record_failed();
                    }
                }
                Err(_) => {
                    agent.status = AgentStatus::Failed;
                    agent.error = Some("Agent task panicked".to_string());
                    // Record failure in metrics
                    metrics.record_failed();
                }
            }

            // Handle cancellation (use the clone we made for later use)
            // Only check cancellation/timeout if the task didn't already fail
            if agent.status == AgentStatus::Running {
                if cancelled_flag_final.load(Ordering::Relaxed) {
                    agent.status = AgentStatus::Failed;
                    agent.error = Some("Cancelled by user".to_string());
                    // Record cancellation in metrics
                    metrics.record_cancelled();
                } else {
                    // Handle timeout — compute elapsed directly from start time
                    let elapsed = agent.started_at.elapsed().as_secs();
                    if elapsed >= agent.timeout_secs {
                        agent.status = AgentStatus::Failed;
                        agent.error = Some(format!("Timed out after {}s", agent.timeout_secs));
                        // Record timeout in metrics
                        metrics.record_timed_out();
                    }
                }
            }

            if agent.status == AgentStatus::Completed || agent.status == AgentStatus::Failed {
                agent.completed_at = Some(Instant::now());
            }
        }
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}
