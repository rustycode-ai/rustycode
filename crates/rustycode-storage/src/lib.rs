#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::default_trait_access,
    clippy::format_push_string,
    clippy::manual_midpoint,
    clippy::match_same_arms,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::unused_self
)]
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp,)
)]
// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! # `RustyCode` Storage
//!
//! Persistent storage layer for `RustyCode` sessions, events, and plans.
//!
//! ## Features
//!
//! - **`SQLite` Backend**: Uses rusqlite for reliable, embedded persistence
//! - **Event Persistence**: Automatically persist events from the event bus
//! - **Session Management**: Store and retrieve session state
//! - **Plan Storage**: Save and query execution plans
//! - **Memory Store**: Key-value storage for contextual data
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_storage::Storage;
//! use rustycode_bus::SessionStartedEvent;
//! use rustycode_protocol::SessionId;
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Open storage database
//! let storage = Storage::open(Path::new("rustycode.db"))?;
//!
//! // Persist an event
//! let event = SessionStartedEvent::new(
//!     SessionId::new(),
//!     "Analyze codebase".to_string(),
//!     "Initial session".to_string(),
//! );
//! storage.insert_event_bus(&event)?;
//!
//! // Retrieve recent events
//! let events = storage.events(10)?;
//! for event in events {
//!     println!("{}: {}", event.event_type, event.created_at);
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use rustycode_bus::{Event, EventBus, SubscriptionHandle};
use rustycode_protocol::{
    EventKind, Milestone, MilestoneId, MilestoneStatus, Plan, PlanDependency, PlanId, PlanStatus,
    PlanStep, Session, SessionEvent, SessionId, ToolApprovalMode,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

use crate::memory_metrics::MemoryMetrics;
use crate::session_capture::{FileOperationType, InteractionEvent, SessionCapture, SessionSummary};

// Buffered I/O writer
pub mod buffered_writer;

// Conversation history system
pub mod conversation_history;

// Session capture and summarization
pub mod session_capture;

// LLM-powered session summarization
// Event sourcing
// pub mod event_store; // broken: needs rustycode-agent-runtime dep (cycle) — fix pending

// Memory effectiveness metrics
pub mod memory_metrics;

// Checkpoint storage for session recovery
pub mod checkpoint;

// JSON-based session snapshot persistence
pub mod checkpoint_store;

// Project-scoped task and todo storage
pub mod task_store;

pub use checkpoint::{
    repo_has_uncommitted_changes, Checkpoint, CheckpointStorage, GitCheckpointStorage,
    GitRewindSnapshot,
};

pub use checkpoint_store::{CheckpointSnapshot, CheckpointStore, ExecutionPhase};

/// Convenience API: rewind a repository at `repo_path` to the provided snapshot.
///
/// This is a thin wrapper around `GitCheckpointStorage::rewind_to_checkpoint` to
/// make it easier for higher-level callers (CLI, services) to trigger a rewind.
pub fn rewind_repo(repo_path: &Path, snapshot: &GitRewindSnapshot) -> Result<()> {
    let storage = GitCheckpointStorage::from_path(repo_path.to_path_buf());
    storage.rewind_to_checkpoint(snapshot)
}

/// Preview what a rewind would change without performing destructive operations.
/// Returns a human-readable summary of files that would be modified or removed.
pub fn preview_rewind_repo(repo_path: &Path, snapshot: &GitRewindSnapshot) -> Result<String> {
    let storage = GitCheckpointStorage::from_path(repo_path.to_path_buf());
    storage.preview_rewind(snapshot)
}

impl Storage {
    /// Rewind a repository to the given snapshot using the storage helper.
    pub fn rewind_snapshot(&self, repo_path: &Path, snapshot: &GitRewindSnapshot) -> Result<()> {
        rewind_repo(repo_path, snapshot)
    }
}

/// Record of a persisted event from the events table
#[derive(Debug, Clone)]
pub struct EventRecord {
    /// Auto-incrementing primary key
    pub id: i64,
    /// Event type (e.g., "session.started", "tool.executed")
    pub event_type: String,
    /// Full event data as JSON string
    pub event_data: String,
    /// RFC3339 timestamp when the event was created
    pub created_at: String,
}

/// Record of a persisted memory entry from the memory table
#[derive(Debug, Clone)]
pub struct MemoryRecord {
    /// Memory scope (e.g., "project", "session")
    pub scope: String,
    /// Unique key within the scope
    pub key: String,
    /// Stored value
    pub value: String,
    /// RFC3339 timestamp when the entry was last updated
    pub updated_at: String,
}

/// Manages active session captures
///
/// Tracks active sessions, captures events as they arrive, and finalizes
/// sessions when they end. Stores summaries and learnings for later use.
#[derive(Debug)]
pub struct SessionCaptureManager {
    /// Active session captures by session ID
    active_captures: StdMutex<HashMap<String, SessionCapture>>,
    /// Completed session summaries
    completed_summaries: StdMutex<Vec<SessionSummary>>,
    /// Learnings extracted from finalized sessions
    learnings: StdMutex<Vec<String>>,
    /// Memory metrics for tracking capture statistics
    metrics: StdMutex<MemoryMetrics>,
    /// Storage directory for session summaries
    storage_dir: Option<std::path::PathBuf>,
    /// Counter for sessions captured
    sessions_captured: AtomicU64,
    /// Counter for events captured
    events_captured: AtomicU64,
    /// Counter for summaries generated
    summaries_generated: AtomicU64,
}

impl SessionCaptureManager {
    pub fn new(storage_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            active_captures: StdMutex::new(HashMap::new()),
            completed_summaries: StdMutex::new(Vec::new()),
            learnings: StdMutex::new(Vec::new()),
            metrics: StdMutex::new(MemoryMetrics::new()),
            storage_dir,
            sessions_captured: AtomicU64::new(0),
            events_captured: AtomicU64::new(0),
            summaries_generated: AtomicU64::new(0),
        }
    }

    /// Start a new session capture
    pub fn start_session(&self, session_id: SessionId, task: String) {
        let capture = SessionCapture::new(session_id.clone(), task);
        let id_str = session_id.to_string();

        if let Ok(mut captures) = self.active_captures.lock() {
            captures.insert(id_str, capture);
            self.sessions_captured.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut metrics) = self.metrics.lock() {
                metrics.record_session_captured();
            }
            tracing::debug!("Started session capture for {}", session_id);
        } else {
            tracing::warn!("Failed to lock active_captures for session {}", session_id);
        }
    }

    /// Capture an interaction event for a session
    pub fn capture_event(&self, session_id: &str, event: InteractionEvent) {
        if let Ok(mut captures) = self.active_captures.lock() {
            if let Some(capture) = captures.get_mut(session_id) {
                capture.capture_interaction(event);
                self.events_captured.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut metrics) = self.metrics.lock() {
                    metrics.record_event_captured();
                }
            }
        }
    }

    /// Finalize a session capture and store the summary
    pub fn finalize_session(&self, session_id: &str, outcome: session_capture::SessionOutcome) {
        if let Ok(mut captures) = self.active_captures.lock() {
            if let Some(mut capture) = captures.remove(session_id) {
                // Force outcome by capturing a synthetic event if needed
                let summary = match capture.finalize_session() {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("Failed to finalize session {}: {}", session_id, e);
                        return;
                    }
                };
                let summary_with_outcome = SessionSummary { outcome, ..summary };

                // Store summary (cap at 1000 to prevent unbounded growth)
                if let Ok(mut summaries) = self.completed_summaries.lock() {
                    if summaries.len() >= 1000 {
                        summaries.drain(0..100);
                    }
                    summaries.push(summary_with_outcome.clone());
                }

                // Store to disk if storage directory is configured
                if let Some(ref dir) = self.storage_dir {
                    if let Err(e) = SessionCapture::store_summary(&summary_with_outcome, dir) {
                        tracing::warn!("Failed to store session summary: {}", e);
                    }
                }

                // Extract learnings (cap at 500 to prevent unbounded growth)
                for learning in &summary_with_outcome.learnings {
                    if let Ok(mut learnings) = self.learnings.lock() {
                        if learnings.len() >= 500 {
                            learnings.drain(0..50);
                        }
                        learnings.push(learning.clone());
                    }
                }

                // Update metrics
                self.summaries_generated.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut metrics) = self.metrics.lock() {
                    metrics.record_summary_generated();
                }

                tracing::info!(
                    "Finalized session capture for {} with outcome {}",
                    session_id,
                    outcome
                );
            }
        }
    }

    /// Get the count of active sessions
    pub fn active_session_count(&self) -> usize {
        self.active_captures.lock().map_or(0, |c| c.len())
    }

    /// Get a session summary by session ID
    pub fn session_summary(&self, session_id: &str) -> Option<SessionSummary> {
        // Check active captures first
        if let Ok(captures) = self.active_captures.lock() {
            if let Some(capture) = captures.get(session_id) {
                // Return a partial summary for active sessions
                return Some(SessionSummary {
                    session_id: capture.session_id().clone(),
                    task: capture.task().to_string(),
                    duration_ms: 0, // Will be calculated on finalize
                    key_points: Vec::new(),
                    files_touched: Vec::new(),
                    errors_encountered: Vec::new(),
                    tools_used: Vec::new(),
                    outcome: session_capture::SessionOutcome::Abandoned,
                    learnings: Vec::new(),
                    next_steps: Vec::new(),
                    started_at: Utc::now(),
                    ended_at: Utc::now(),
                });
            }
        }

        // Check completed summaries
        if let Ok(summaries) = self.completed_summaries.lock() {
            return summaries
                .iter()
                .find(|s| s.session_id.to_string() == session_id)
                .cloned();
        }

        None
    }

    /// Get all learnings
    pub fn learnings(&self) -> Vec<String> {
        self.learnings.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Get memory metrics
    pub fn metrics(&self) -> MemoryMetrics {
        self.metrics.lock().map(|m| m.clone()).unwrap_or_default()
    }

    /// Get sessions captured count
    pub fn sessions_captured(&self) -> u64 {
        self.sessions_captured.load(Ordering::Relaxed)
    }

    /// Get events captured count
    pub fn events_captured(&self) -> u64 {
        self.events_captured.load(Ordering::Relaxed)
    }

    /// Get summaries generated count
    pub fn summaries_generated(&self) -> u64 {
        self.summaries_generated.load(Ordering::Relaxed)
    }
}

/// Event subscriber that persists events from the event bus to storage
///
/// The `EventSubscriber` runs as a background task that:
/// - Subscribes to all events from the `EventBus`
/// - Persists events to the database
/// - Captures session data for learning and summarization
/// - Handles graceful shutdown
///
/// # Example
///
/// ```no_run
/// use rustycode_storage::{Storage, EventSubscriber};
/// use rustycode_bus::EventBus;
/// use std::sync::Arc;
/// use std::path::Path;
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let bus = Arc::new(EventBus::new());
/// let storage = Storage::open(Path::new("test.db"))?;
///
/// // Create and start the subscriber
/// let subscriber = EventSubscriber::new(storage, bus);
/// subscriber.start().await?;
///
/// // ... application runs ...
///
/// // Gracefully stop the subscriber
/// subscriber.stop().await?;
/// # Ok(())
/// # }
/// ```
pub struct EventSubscriber {
    storage: Storage,
    bus: Arc<EventBus>,
    running: Arc<AtomicBool>,
    task_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    subscription_handle: Arc<TokioMutex<Option<SubscriptionHandle>>>,
    /// Session capture manager for tracking active sessions
    capture_manager: Arc<SessionCaptureManager>,
}

impl EventSubscriber {
    /// Create a new event subscriber
    ///
    pub fn new(storage: Storage, bus: Arc<EventBus>) -> Self {
        Self {
            storage,
            bus,
            running: Arc::new(AtomicBool::new(false)),
            task_handle: Arc::new(TokioMutex::new(None)),
            subscription_handle: Arc::new(TokioMutex::new(None)),
            capture_manager: Arc::new(SessionCaptureManager::new(None)),
        }
    }

    /// Create a new event subscriber with session capture
    ///
    pub fn new_with_capture(
        storage: Storage,
        bus: Arc<EventBus>,
        storage_dir: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            storage,
            bus,
            running: Arc::new(AtomicBool::new(false)),
            task_handle: Arc::new(TokioMutex::new(None)),
            subscription_handle: Arc::new(TokioMutex::new(None)),
            capture_manager: Arc::new(SessionCaptureManager::new(storage_dir)),
        }
    }

    /// Get the number of active sessions being captured
    pub fn active_session_count(&self) -> usize {
        self.capture_manager.active_session_count()
    }

    /// Get a session summary by session ID
    pub fn session_summary(&self, session_id: &str) -> Option<SessionSummary> {
        self.capture_manager.session_summary(session_id)
    }

    /// Get the session capture manager
    pub fn capture_manager(&self) -> &SessionCaptureManager {
        &self.capture_manager
    }

    /// Start the event subscriber
    ///
    /// This spawns a background task that subscribes to all events
    /// and persists them to the database.
    ///
    pub async fn start(&self) -> Result<()> {
        // Check if already running
        if self.running.load(Ordering::Acquire) {
            return Ok(()); // Already started, idempotent
        }

        // Subscribe to all events
        let (sub_id, mut rx) = self
            .bus
            .subscribe("*")
            .await
            .context("failed to subscribe to event bus")?;

        // Store subscription handle for cleanup
        let handle = SubscriptionHandle::new(sub_id, self.bus.clone());
        *self.subscription_handle.lock().await = Some(handle);

        // Set running flag
        self.running.store(true, Ordering::Release);

        // Clone Arcs for the background task
        let conn = self.storage.conn.clone();
        let running = self.running.clone();
        let capture_manager = self.capture_manager.clone();

        // Spawn background task to receive and persist events
        let task = tokio::spawn(async move {
            tracing::info!("Event subscriber started");

            while running.load(Ordering::Acquire) {
                // Use timeout to allow checking running flag periodically
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), rx.recv()).await
                {
                    Ok(Ok(event)) => {
                        // Event received, persist it
                        let event_type = event.event_type().to_string();
                        let serialized = event.serialize();
                        let timestamp = event.timestamp().to_rfc3339();
                        let conn_clone = conn.clone();
                        let capture_manager_clone = capture_manager.clone();

                        // Extract session_id and other fields for capture
                        let session_id = serialized
                            .get("session_id")
                            .and_then(|v| v.as_str())
                            .map(ToString::to_string);
                        let task = serialized
                            .get("task")
                            .and_then(|v| v.as_str())
                            .map(ToString::to_string);

                        // Process event for session capture
                        if let Some(ref sid) = session_id {
                            process_event_for_capture(
                                &capture_manager_clone,
                                sid,
                                &event_type,
                                &serialized,
                                task,
                            );
                        }

                        // Spawn blocking task for database write
                        if let Err(e) = tokio::task::spawn_blocking(move || {
                            if let Ok(lock) = conn_clone.lock() {
                                let sid = session_id.as_deref().unwrap_or("");

                                let event_data = serde_json::to_string(&serialized)
                                    .unwrap_or_else(|_| serialized.to_string());

                                lock.execute(
                                    "INSERT INTO events (session_id, at, kind, detail) VALUES (?1, ?2, ?3, ?4)",
                                    params![sid, timestamp, event_type, event_data],
                                ).context("failed to persist event").map(|_| ())
                            } else {
                                Err(anyhow::anyhow!("failed to acquire lock"))
                            }
                        }).await {
                            tracing::error!("Failed to join persist task: {:?}", e);
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Event channel error: {:?}", e);
                        break;
                    }
                    Err(_) => {
                        // Timeout - continue loop to check running flag
                        continue;
                    }
                }
            }

            tracing::info!("Event subscriber stopped");
        });

        // Store task handle
        *self.task_handle.lock().await = Some(task);

        Ok(())
    }

    /// Stop the event subscriber gracefully
    ///
    /// This sets the running flag to false and waits for the background
    /// task to complete. The subscription is also cancelled.
    pub async fn stop(&self) -> Result<()> {
        if !self.running.load(Ordering::Acquire) {
            return Ok(()); // Already stopped, idempotent
        }

        tracing::info!("Stopping event subscriber");

        // Set running flag to signal task to stop
        self.running.store(false, Ordering::Release);

        // Cancel subscription
        if let Some(handle) = self.subscription_handle.lock().await.take() {
            // SubscriptionHandle automatically unsubscribes when dropped
            drop(handle);
        }

        // Wait for task to complete
        if let Some(task) = self.task_handle.lock().await.take() {
            if let Err(e) = tokio::time::timeout(tokio::time::Duration::from_secs(5), task).await {
                tracing::warn!("Event subscriber task did not stop gracefully: {:?}", e);
            }
        }

        Ok(())
    }

    /// Check if the subscriber is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

/// Process an event for session capture
fn process_event_for_capture(
    capture_manager: &SessionCaptureManager,
    session_id: &str,
    event_type: &str,
    serialized: &serde_json::Value,
    task: Option<String>,
) {
    match event_type {
        "session.started" => {
            if let Some(task_str) = task {
                if let Ok(sid) = SessionId::parse(session_id) {
                    capture_manager.start_session(sid, task_str);
                }
            }
        }
        "session.completed" => {
            capture_manager.finalize_session(session_id, session_capture::SessionOutcome::Success);
        }
        "session.failed" => {
            capture_manager.finalize_session(session_id, session_capture::SessionOutcome::Failed);
        }
        "tool.executed" => {
            if let (Some(tool_name), Some(success)) = (
                serialized.get("tool_name").and_then(|v| v.as_str()),
                serialized
                    .get("success")
                    .and_then(serde_json::Value::as_bool),
            ) {
                let input = serialized.get("input").cloned().unwrap_or_default();
                let output = serialized.get("output").cloned();
                let duration_ms = serialized
                    .get("duration_ms")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);

                capture_manager.capture_event(
                    session_id,
                    InteractionEvent::ToolCall {
                        tool_name: tool_name.to_string(),
                        input,
                        output,
                        success,
                        duration_ms,
                    },
                );
            }
        }
        "file.read" | "file.written" | "file.edited" | "file.deleted" => {
            if let Some(path) = serialized.get("path").and_then(|v| v.as_str()) {
                let operation = match event_type {
                    "file.read" => FileOperationType::Read,
                    "file.written" | "file.created" => FileOperationType::Created,
                    "file.edited" => FileOperationType::Modified,
                    "file.deleted" => FileOperationType::Deleted,
                    _ => FileOperationType::Read,
                };

                capture_manager.capture_event(
                    session_id,
                    InteractionEvent::FileOperation {
                        path: path.to_string(),
                        operation,
                        content_hash: None,
                    },
                );
            }
        }
        "error" | "tool.error" | "execution.error" => {
            let error_type = serialized
                .get("error_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            let message = serialized
                .get("message")
                .or_else(|| serialized.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            let resolution = serialized
                .get("resolution")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);

            capture_manager.capture_event(
                session_id,
                InteractionEvent::Error {
                    error_type,
                    message,
                    resolution,
                },
            );
        }
        _ => {
            // Other events are not captured
        }
    }
}

pub struct Storage {
    conn: Arc<StdMutex<Connection>>,
}

/// Statistics about the database contents and size.
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    /// Total number of sessions.
    pub session_count: i64,
    /// Total number of events.
    pub event_count: i64,
    /// Total number of API call records.
    pub api_call_count: i64,
    /// Total number of hook execution records.
    pub hook_execution_count: i64,
    /// Total number of checkpoint records.
    pub checkpoint_count: i64,
    /// Database file size in bytes.
    pub db_size_bytes: u64,
    /// Date of the oldest session, if any.
    pub oldest_session: Option<String>,
    /// Date of the most recent session, if any.
    pub newest_session: Option<String>,
}

/// Statistics returned by cleanup operations.
#[derive(Debug, Clone, Default)]
pub struct CleanupStats {
    /// Number of sessions removed.
    pub sessions_removed: u64,
    /// Number of events removed.
    pub events_removed: u64,
    /// Number of API call records removed.
    pub api_calls_removed: u64,
    /// Number of hook execution records removed.
    pub hook_executions_removed: u64,
    /// Number of checkpoint records removed.
    pub checkpoints_removed: u64,
    /// Number of rewind snapshots removed.
    pub rewind_snapshots_removed: u64,
    /// Number of session snapshots removed.
    pub snapshots_removed: u64,
    /// Number of FTS entries removed.
    pub fts_entries_removed: u64,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
        let storage = Self {
            conn: Arc::new(StdMutex::new(conn)),
        };
        storage.migrate()?;
        // Best-effort FTS index creation (non-fatal if FTS5 not available)
        if let Err(e) = storage.ensure_fts_index() {
            tracing::debug!(
                "FTS5 index not available, search will use LIKE fallback: {}",
                e
            );
        }
        Ok(storage)
    }

    // Event persistence is now primarily handled via the EventSubscriber struct
    // which provides more robust lifecycle management and graceful shutdown.

    // ── Sessions ──────────────────────────────────────────────────────────────

    pub fn insert_session(&self, session: &Session) -> Result<()> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner).execute(
            "insert into sessions (id, task, created_at, mode, status, plan_path) values (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id.to_string(),
                session.task,
                session.created_at.to_rfc3339(),
                serde_json::to_string(&session.mode)?,
                serde_json::to_string(&session.status)?,
                session.plan_path,
            ],
        )?;
        Ok(())
    }

    pub fn update_session(&self, session: &Session) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "update sessions set mode = ?1, status = ?2, plan_path = ?3 where id = ?4",
                params![
                    serde_json::to_string(&session.mode)?,
                    serde_json::to_string(&session.status)?,
                    session.plan_path,
                    session.id.to_string(),
                ],
            )?;
        Ok(())
    }

    pub fn load_session(&self, session_id: &SessionId) -> Result<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, task, created_at, mode, status, plan_path from sessions where id = ?1",
        )?;
        let mut rows = stmt.query_map(params![session_id.to_string()], session_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn insert_event(&self, event: &SessionEvent) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert into events (session_id, at, kind, detail) values (?1, ?2, ?3, ?4)",
                params![
                    event.session_id.to_string(),
                    event.at.to_rfc3339(),
                    serde_json::to_string(&event.kind)?,
                    event.detail
                ],
            )?;
        Ok(())
    }

    /// Insert an event from the event bus into the events table
    ///
    /// This method persists any event that implements the Event trait, storing
    /// its type, serialized data, and timestamp.
    ///
    pub fn insert_event_bus(&self, event: &dyn Event) -> Result<()> {
        let serialized = event.serialize();
        let event_data =
            serde_json::to_string(&serialized).context("failed to serialize event data")?;
        let created_at = event.timestamp().to_rfc3339();
        let event_type = event.event_type();

        // Extract session_id from event data if available
        let session_id = serialized
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        self.conn
            .lock()
            .map_err(|e| anyhow::anyhow!("database lock poisoned: {e}"))?
            .execute(
                "INSERT INTO events (session_id, at, kind, detail) VALUES (?1, ?2, ?3, ?4)",
                params![session_id, created_at, event_type, event_data],
            )?;
        Ok(())
    }

    /// Retrieve recent events from the events table
    ///
    /// Returns events ordered by creation time (most recent first).
    ///
    pub fn events(&self, limit: usize) -> Result<Vec<EventRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt =
            conn.prepare("SELECT id, kind, detail, at FROM events ORDER BY at DESC LIMIT ?1")?;

        let rows = stmt.query_map([limit as i64], |row| {
            Ok(EventRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                event_data: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn upsert_memory(&self, scope: &str, key: &str, value: &str) -> Result<()> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner).execute(
            "insert into memory (scope, key, value, updated_at) values (?1, ?2, ?3, ?4)
             on conflict(scope, key) do update set value = excluded.value, updated_at = excluded.updated_at",
            params![scope, key, value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn memory(&self, scope: &str) -> Result<Vec<MemoryRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select scope, key, value, updated_at from memory where scope = ?1 order by key",
        )?;
        let rows = stmt.query_map(params![scope], |row| {
            Ok(MemoryRecord {
                scope: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn memory_entry(&self, scope: &str, key: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare("select value from memory where scope = ?1 and key = ?2")?;
        let mut rows = stmt.query(params![scope, key])?;

        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn session_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .query_row("select count(*) from sessions", [], |row| row.get(0))?)
    }

    pub fn event_count_for_session(&self, session_id: &str) -> Result<i64> {
        Ok(self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .query_row(
                "select count(*) from events where session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )?)
    }

    pub fn recent_tasks(
        &self,
        limit: usize,
        exclude_session_id: Option<&str>,
    ) -> Result<Vec<String>> {
        let limit = i64::try_from(limit)?;
        let mut values = Vec::new();
        if let Some(session_id) = exclude_session_id {
            let conn = self
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut statement = conn.prepare(
                "select task from sessions where id != ?1 order by created_at desc limit ?2",
            )?;
            let tasks =
                statement.query_map(params![session_id, limit], |row| row.get::<_, String>(0))?;
            for task in tasks {
                values.push(task?);
            }
        } else {
            let conn = self
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut statement =
                conn.prepare("select task from sessions order by created_at desc limit ?1")?;
            let tasks = statement.query_map(params![limit], |row| row.get::<_, String>(0))?;
            for task in tasks {
                values.push(task?);
            }
        }
        Ok(values)
    }

    pub fn recent_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let limit = i64::try_from(limit)?;
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut statement = conn.prepare(
            "select id, task, created_at, mode, status, plan_path from sessions order by created_at desc limit ?1",
        )?;
        let rows = statement.query_map(params![limit], session_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn session_events(&self, session_id: &SessionId) -> Result<Vec<SessionEvent>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut statement = conn.prepare(
            "select at, kind, detail from events where session_id = ?1 order by at asc, id asc",
        )?;
        let rows = statement.query_map(params![session_id.to_string()], |row| {
            let at: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let detail: String = row.get(2)?;
            Ok((at, kind, detail))
        })?;

        let mut events = Vec::new();
        for row in rows {
            let (at, kind, detail) = row?;
            events.push(SessionEvent {
                session_id: session_id.clone(),
                at: DateTime::parse_from_rfc3339(&at)?.with_timezone(&Utc),
                kind: serde_json::from_str::<EventKind>(&kind)?,
                detail,
            });
        }
        Ok(events)
    }

    // ── Plans ─────────────────────────────────────────────────────────────────

    pub fn insert_plan(&self, plan: &Plan) -> Result<()> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner).execute(
            "insert into plans (id, session_id, milestone_id, task, created_at, status, summary, approach, steps, files_to_modify, risks)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                plan.id.to_string(),
                plan.session_id.to_string(),
                plan.milestone_id.as_ref().map(ToString::to_string),
                plan.task,
                plan.created_at.to_rfc3339(),
                serde_json::to_string(&plan.status)?,
                plan.summary,
                plan.approach,
                serde_json::to_string(&plan.steps)?,
                serde_json::to_string(&plan.files_to_modify)?,
                serde_json::to_string(&plan.risks)?,
            ],
        )?;
        Ok(())
    }

    // ── Milestones ───────────────────────────────────────────────────────────

    pub fn insert_milestone(&self, milestone: &Milestone) -> Result<()> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner).execute(
            "insert into milestones (
                id, session_id, title, description, status, plan_ids, plan_dependencies,
                success_criteria, validation_command, created_at, updated_at, completed_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                milestone.id.to_string(),
                milestone.session_id.to_string(),
                milestone.title,
                milestone.description,
                serde_json::to_string(&milestone.status)?,
                serde_json::to_string(&milestone.plan_ids)?,
                serde_json::to_string(&milestone.plan_dependencies)?,
                serde_json::to_string(&milestone.success_criteria)?,
                milestone.validation_command,
                milestone.created_at.to_rfc3339(),
                milestone.updated_at.to_rfc3339(),
                milestone.completed_at.map(|ts| ts.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn load_milestone(&self, id: &MilestoneId) -> Result<Option<Milestone>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, title, description, status, plan_ids, plan_dependencies,
                    success_criteria, validation_command, created_at, updated_at, completed_at
             from milestones where id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id.to_string()], milestone_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_milestones(&self, session_id: &SessionId) -> Result<Vec<Milestone>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, title, description, status, plan_ids, plan_dependencies,
                    success_criteria, validation_command, created_at, updated_at, completed_at
             from milestones where session_id = ?1 order by created_at desc",
        )?;
        let rows = stmt.query_map(params![session_id.to_string()], milestone_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn update_milestone_status(
        &self,
        id: &MilestoneId,
        status: &MilestoneStatus,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let status_json = serde_json::to_string(status)?;
        let updated_at = Utc::now().to_rfc3339();
        if matches!(status, MilestoneStatus::Completed) {
            conn.execute(
                "update milestones set status = ?1, updated_at = ?2, completed_at = ?3 where id = ?4",
                params![status_json, updated_at, Utc::now().to_rfc3339(), id.to_string()],
            )?;
        } else {
            conn.execute(
                "update milestones set status = ?1, updated_at = ?2 where id = ?3",
                params![status_json, updated_at, id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn add_plan_to_milestone(&self, milestone_id: &MilestoneId, plan_id: &PlanId) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tx = conn.transaction()?;

        let milestone_row = tx.query_row(
            "select plan_ids from milestones where id = ?1",
            params![milestone_id.to_string()],
            |row| row.get::<_, String>(0),
        )?;
        let mut plan_ids: Vec<PlanId> =
            serde_json::from_str(&milestone_row).context("failed to deserialize milestone plan ids")?;
        if !plan_ids.contains(plan_id) {
            plan_ids.push(plan_id.clone());
        }

        tx.execute(
            "update milestones set plan_ids = ?1, updated_at = ?2 where id = ?3",
            params![
                serde_json::to_string(&plan_ids)?,
                Utc::now().to_rfc3339(),
                milestone_id.to_string(),
            ],
        )?;
        tx.execute(
            "update plans set milestone_id = ?1 where id = ?2",
            params![milestone_id.to_string(), plan_id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn milestone_plans(&self, milestone_id: &MilestoneId) -> Result<Vec<Plan>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, milestone_id, task, created_at, status, summary, approach,
                    steps, files_to_modify, risks
             from plans where milestone_id = ?1 order by created_at asc",
        )?;
        let rows = stmt.query_map(params![milestone_id.to_string()], plan_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn update_plan_status(&self, plan_id: &PlanId, status: &PlanStatus) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "update plans set status = ?1 where id = ?2",
                params![serde_json::to_string(status)?, plan_id.to_string()],
            )?;
        Ok(())
    }

    pub fn load_plan(&self, plan_id: &PlanId) -> Result<Option<Plan>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, task, created_at, status, summary, approach, steps, files_to_modify, risks
             from plans where id = ?1",
        )?;
        let mut rows = stmt.query_map(params![plan_id.to_string()], plan_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_plans(&self, session_id: &SessionId) -> Result<Vec<Plan>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, task, created_at, status, summary, approach, steps, files_to_modify, risks
             from plans where session_id = ?1 order by created_at desc",
        )?;
        let rows = stmt.query_map(params![session_id.to_string()], plan_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn all_plans(&self, limit: usize) -> Result<Vec<Plan>> {
        let limit = i64::try_from(limit)?;
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, task, created_at, status, summary, approach, steps, files_to_modify, risks
             from plans order by created_at desc limit ?1",
        )?;
        let rows = stmt.query_map(params![limit], plan_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    /// Update the execution status of a specific step within a plan
    ///
    /// This method allows tracking the progress of individual steps during plan execution.
    /// It serializes the entire steps array with the updated step back to the database.
    ///
    pub fn update_plan_step(
        &self,
        plan_id: &PlanId,
        step_index: usize,
        step: &PlanStep,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Load current plan inside the same lock to prevent TOCTOU
        let steps_json: String = conn
            .query_row(
                "select steps from plans where id = ?1",
                params![plan_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| anyhow::anyhow!("plan not found: {e}"))?;

        let mut steps: Vec<PlanStep> =
            serde_json::from_str(&steps_json).context("failed to deserialize plan steps")?;

        if step_index >= steps.len() {
            anyhow::bail!(
                "step index {} out of bounds (plan has {} steps)",
                step_index,
                steps.len()
            );
        }

        steps[step_index] = step.clone();

        let updated_json =
            serde_json::to_string(&steps).context("failed to serialize plan steps")?;

        conn.execute(
            "update plans set steps = ?1 where id = ?2",
            params![updated_json, plan_id.to_string()],
        )?;

        Ok(())
    }

    // ── Migration ─────────────────────────────────────────────────────────────

    fn migrate(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute_batch(
            "create table if not exists sessions (
                id text primary key,
                task text not null,
                created_at text not null,
                mode text not null default '\"executing\"',
                status text not null default '\"executing\"',
                plan_path text
            );
            create table if not exists events (
                id integer primary key autoincrement,
                session_id text not null,
                at text not null,
                kind text not null,
                detail text not null
            );
            create table if not exists memory (
                scope text not null,
                key text not null,
                value text not null,
                updated_at text not null,
                primary key (scope, key)
            );
            create table if not exists plans (
                id text primary key,
                session_id text not null,
                task text not null,
                created_at text not null,
                status text not null,
                summary text not null,
                approach text not null,
                steps text not null,
                files_to_modify text not null,
                risks text not null,
                foreign key (session_id) references sessions(id)
            );
            create table if not exists session_snapshots (
                session_id text not null,
                captured_at text not null,
                snapshot_json text not null,
                primary key (session_id)
            );
            create table if not exists checkpoints (
                id text primary key,
                session_id text not null,
                label text not null,
                commit_sha text,
                files_json text not null,
                created_at text not null,
                foreign key (session_id) references sessions(id) on delete cascade
            );
            create index if not exists idx_checkpoints_session on checkpoints(session_id);
            create table if not exists rewind_snapshots (
                id integer primary key autoincrement,
                session_id text not null,
                interaction_number integer not null,
                role text not null,
                content_preview text not null,
                tools_used_json text,
                checkpoint_id text,
                captured_at text not null,
                foreign key (session_id) references sessions(id) on delete cascade,
                foreign key (checkpoint_id) references checkpoints(id) on delete set null
            );
            create index if not exists idx_rewind_session on rewind_snapshots(session_id);
            create index if not exists idx_rewind_interaction on rewind_snapshots(session_id, interaction_number);
            create table if not exists hook_executions (
                id integer primary key autoincrement,
                session_id text not null,
                trigger_type text not null,
                hook_name text not null,
                command text not null,
                status text not null,
                stdout text,
                stderr text,
                exit_code integer,
                blocked integer not null default 0,
                duration_ms integer,
                executed_at text not null,
                foreign key (session_id) references sessions(id) on delete cascade
            );
            create index if not exists idx_hooks_session on hook_executions(session_id);
            create index if not exists idx_hooks_trigger on hook_executions(trigger_type);
            create table if not exists api_calls (
                id integer primary key autoincrement,
                session_id text not null,
                model text not null,
                input_tokens integer not null,
                output_tokens integer not null,
                cost_usd real not null,
                tool_name text,
                provider text,
                called_at text not null,
                cache_read_tokens integer not null default 0,
                cache_creation_tokens integer not null default 0,
                cache_savings_usd real not null default 0.0,
                foreign key (session_id) references sessions(id) on delete cascade
            );
            create index if not exists idx_api_calls_session on api_calls(session_id);
            create index if not exists idx_api_calls_model on api_calls(model);
            create table if not exists projects (
                id text primary key,
                path text not null,
                created_at text not null
            );
            create unique index if not exists idx_projects_path on projects(path);
            create table if not exists todos (
                id text primary key,
                session_id text not null,
                project_id text not null,
                content text not null,
                status text not null default 'pending',
                priority text not null default 'medium',
                position integer not null,
                created_at text not null,
                updated_at text not null,
                foreign key (project_id) references projects(id) on delete cascade
            );
            create index if not exists idx_todos_session on todos(session_id);
            create index if not exists idx_todos_project on todos(project_id);
            create table if not exists tasks (
                id text primary key,
                project_id text not null,
                session_id text,
                description text not null,
                status text not null default 'pending',
                owner text,
                dependencies text not null default '[]',
                output text,
                created_at text not null,
                updated_at text not null,
                started_at text,
                completed_at text,
                foreign key (project_id) references projects(id) on delete cascade
            );
            create index if not exists idx_tasks_project on tasks(project_id);
            create index if not exists idx_tasks_status on tasks(status);
            create index if not exists idx_tasks_owner on tasks(owner);",
        )?;
        Ok(())
    }
}

// ── Session Snapshots ─────────────────────────────────────────────────────────

/// A snapshot of session state for persistence across restarts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub captured_at: DateTime<Utc>,
    pub conversation_json: String,
    pub active_plan_id: Option<PlanId>,
    pub metadata: HashMap<String, String>,
}

impl Storage {
    /// Persist a session snapshot, replacing any existing snapshot for this session.
    pub fn save_snapshot(&self, snapshot: &SessionSnapshot) -> Result<()> {
        let json =
            serde_json::to_string(snapshot).context("failed to serialize session snapshot")?;
        let captured_at = snapshot.captured_at.to_rfc3339();
        let session_id_str = snapshot.session_id.to_string();
        let conn = &mut *self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "insert or replace into session_snapshots (session_id, captured_at, snapshot_json) values (?1, ?2, ?3)",
            params![session_id_str, captured_at, json],
        ).context("failed to save session snapshot")?;

        // Also update FTS index (best-effort, non-fatal)
        let fts_content = json;
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO conversation_fts(session_id, content) VALUES (?1, ?2)",
            params![session_id_str, fts_content],
        ) {
            tracing::debug!("Failed to update FTS index: {}", e);
        }

        Ok(())
    }

    /// Load the most recent snapshot for a session, or None if not found.
    pub fn load_snapshot(&self, session_id: &SessionId) -> Result<Option<SessionSnapshot>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select snapshot_json from session_snapshots where session_id = ?1")
            .context("failed to prepare load_snapshot")?;
        let mut rows = stmt
            .query(params![session_id.to_string()])
            .context("failed to query session snapshot")?;
        if let Some(row) = rows.next().context("failed to read snapshot row")? {
            let json: String = row.get(0).context("failed to get snapshot_json")?;
            let snapshot: SessionSnapshot =
                serde_json::from_str(&json).context("failed to deserialize session snapshot")?;
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }

    /// Delete all snapshots for a session.
    pub fn delete_snapshots(&self, session_id: &SessionId) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "delete from session_snapshots where session_id = ?1",
                params![session_id.to_string()],
            )
            .context("failed to delete session snapshots")?;
        Ok(())
    }

    /// List all session IDs that have stored snapshots.
    pub fn list_snapshot_sessions(&self) -> Result<Vec<SessionId>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select session_id from session_snapshots order by captured_at desc")
            .context("failed to prepare list_snapshot_sessions")?;
        let ids = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                Ok(id)
            })
            .context("failed to query snapshot session ids")?
            .collect::<rusqlite::Result<Vec<String>>>()
            .context("failed to collect snapshot session ids")?;
        ids.into_iter()
            .map(|s| SessionId::parse(&s).map_err(|e| anyhow::anyhow!("invalid session id: {e}")))
            .collect()
    }
}

// ── Checkpoint Store Types ───────────────────────────────────────────────────

/// Record of a workspace checkpoint for undo/rewind support.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckpointRecord {
    pub id: String,
    pub session_id: String,
    pub label: String,
    pub commit_sha: Option<String>,
    pub files_json: String,
    pub created_at: String,
}

/// Record of a session interaction for rewind navigation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RewindSnapshot {
    pub id: i64,
    pub session_id: String,
    pub interaction_number: i64,
    pub role: String,
    pub content_preview: String,
    pub tools_used_json: Option<String>,
    pub checkpoint_id: Option<String>,
    pub captured_at: String,
}

/// Record of a hook execution for auditing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookExecutionRecord {
    pub id: i64,
    pub session_id: String,
    pub trigger_type: String,
    pub hook_name: String,
    pub command: String,
    pub status: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub blocked: bool,
    pub duration_ms: Option<i64>,
    pub executed_at: String,
}

/// Record of an LLM API call for cost tracking.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiCallRecord {
    pub id: i64,
    pub session_id: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub tool_name: Option<String>,
    pub provider: Option<String>,
    pub called_at: String,
    #[serde(default)]
    pub cache_read_tokens: i64,
    #[serde(default)]
    pub cache_creation_tokens: i64,
    #[serde(default)]
    pub cache_savings_usd: f64,
}

impl Storage {
    // ── Checkpoint Store ───────────────────────────────────────────────────────

    /// Save a checkpoint record.
    pub fn save_checkpoint(&self, rec: &CheckpointRecord) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert or replace into checkpoints (id, session_id, label, commit_sha, files_json, created_at) values (?1, ?2, ?3, ?4, ?5, ?6)",
                params![rec.id, rec.session_id, rec.label, rec.commit_sha, rec.files_json, rec.created_at],
            )
            .context("failed to save checkpoint")?;
        Ok(())
    }

    /// Load a checkpoint by ID.
    pub fn load_checkpoint(&self, id: &str) -> Result<Option<CheckpointRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, label, commit_sha, files_json, created_at from checkpoints where id = ?1")
            .context("failed to prepare load_checkpoint")?;
        let mut rows = stmt
            .query(params![id])
            .context("failed to query checkpoint")?;
        if let Some(row) = rows.next().context("failed to read checkpoint row")? {
            Ok(Some(CheckpointRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                label: row.get(2)?,
                commit_sha: row.get(3)?,
                files_json: row.get(4)?,
                created_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete a checkpoint by ID.
    pub fn delete_checkpoint(&self, id: &str) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let changed = conn
            .execute("delete from checkpoints where id = ?1", params![id])
            .context("failed to delete checkpoint")?;
        Ok(changed)
    }

    /// List checkpoints for a session, newest first.
    pub fn list_checkpoints(&self, session_id: &str) -> Result<Vec<CheckpointRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, label, commit_sha, files_json, created_at from checkpoints where session_id = ?1 order by created_at desc")
            .context("failed to prepare list_checkpoints")?;
        let records = stmt
            .query_map(params![session_id], |row| {
                Ok(CheckpointRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    label: row.get(2)?,
                    commit_sha: row.get(3)?,
                    files_json: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .context("failed to query checkpoints")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect checkpoints")?;
        Ok(records)
    }

    // ── Rewind Snapshot Store ──────────────────────────────────────────────────

    /// Save a rewind snapshot.
    pub fn save_rewind_snapshot(&self, snap: &RewindSnapshot) -> Result<i64> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert into rewind_snapshots (session_id, interaction_number, role, content_preview, tools_used_json, checkpoint_id, captured_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![snap.session_id, snap.interaction_number, snap.role, snap.content_preview, snap.tools_used_json, snap.checkpoint_id, snap.captured_at],
            )
            .context("failed to save rewind snapshot")?;
        let id = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last_insert_rowid();
        Ok(id)
    }

    /// Load a rewind snapshot by interaction number within a session.
    pub fn load_rewind_snapshot(
        &self,
        session_id: &str,
        interaction_number: i64,
    ) -> Result<Option<RewindSnapshot>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, interaction_number, role, content_preview, tools_used_json, checkpoint_id, captured_at from rewind_snapshots where session_id = ?1 and interaction_number = ?2")
            .context("failed to prepare load_rewind_snapshot")?;
        let mut rows = stmt
            .query(params![session_id, interaction_number])
            .context("failed to query rewind snapshot")?;
        if let Some(row) = rows.next().context("failed to read rewind row")? {
            Ok(Some(RewindSnapshot {
                id: row.get(0)?,
                session_id: row.get(1)?,
                interaction_number: row.get(2)?,
                role: row.get(3)?,
                content_preview: row.get(4)?,
                tools_used_json: row.get(5)?,
                checkpoint_id: row.get(6)?,
                captured_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// List all rewind snapshots for a session, ordered by interaction number.
    pub fn list_rewind_snapshots(&self, session_id: &str) -> Result<Vec<RewindSnapshot>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, interaction_number, role, content_preview, tools_used_json, checkpoint_id, captured_at from rewind_snapshots where session_id = ?1 order by interaction_number asc")
            .context("failed to prepare list_rewind_snapshots")?;
        let snaps = stmt
            .query_map(params![session_id], |row| {
                Ok(RewindSnapshot {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    interaction_number: row.get(2)?,
                    role: row.get(3)?,
                    content_preview: row.get(4)?,
                    tools_used_json: row.get(5)?,
                    checkpoint_id: row.get(6)?,
                    captured_at: row.get(7)?,
                })
            })
            .context("failed to query rewind snapshots")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect rewind snapshots")?;
        Ok(snaps)
    }

    // ── Hook Execution Store ───────────────────────────────────────────────────

    /// Save a hook execution record.
    pub fn save_hook_execution(&self, rec: &HookExecutionRecord) -> Result<i64> {
        let blocked_int = i32::from(rec.blocked);
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert into hook_executions (session_id, trigger_type, hook_name, command, status, stdout, stderr, exit_code, blocked, duration_ms, executed_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![rec.session_id, rec.trigger_type, rec.hook_name, rec.command, rec.status, rec.stdout, rec.stderr, rec.exit_code, blocked_int, rec.duration_ms, rec.executed_at],
            )
            .context("failed to save hook execution")?;
        let id = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last_insert_rowid();
        Ok(id)
    }

    /// List recent hook executions for a session.
    pub fn list_hook_executions(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<HookExecutionRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, trigger_type, hook_name, command, status, stdout, stderr, exit_code, blocked, duration_ms, executed_at from hook_executions where session_id = ?1 order by executed_at desc limit ?2")
            .context("failed to prepare list_hook_executions")?;
        let records = stmt
            .query_map(params![session_id, limit as i64], |row| {
                let blocked_int: i32 = row.get(9)?;
                Ok(HookExecutionRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    trigger_type: row.get(2)?,
                    hook_name: row.get(3)?,
                    command: row.get(4)?,
                    status: row.get(5)?,
                    stdout: row.get(6)?,
                    stderr: row.get(7)?,
                    exit_code: row.get(8)?,
                    blocked: blocked_int != 0,
                    duration_ms: row.get(10)?,
                    executed_at: row.get(11)?,
                })
            })
            .context("failed to query hook executions")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect hook executions")?;
        Ok(records)
    }

    // ── API Call Store ──────────────────────────────────────────────────────────

    /// Save an API call record.
    pub fn save_api_call(&self, rec: &ApiCallRecord) -> Result<i64> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert into api_calls (session_id, model, input_tokens, output_tokens, cost_usd, tool_name, provider, called_at, cache_read_tokens, cache_creation_tokens, cache_savings_usd) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![rec.session_id, rec.model, rec.input_tokens, rec.output_tokens, rec.cost_usd, rec.tool_name, rec.provider, rec.called_at, rec.cache_read_tokens, rec.cache_creation_tokens, rec.cache_savings_usd],
            )
            .context("failed to save api call")?;
        let id = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last_insert_rowid();
        Ok(id)
    }

    /// List API calls for a session.
    pub fn list_api_calls(&self, session_id: &str) -> Result<Vec<ApiCallRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, model, input_tokens, output_tokens, cost_usd, tool_name, provider, called_at, cache_read_tokens, cache_creation_tokens, cache_savings_usd from api_calls where session_id = ?1 order by called_at asc")
            .context("failed to prepare list_api_calls")?;
        let records = stmt
            .query_map(params![session_id], |row| {
                Ok(ApiCallRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    model: row.get(2)?,
                    input_tokens: row.get(3)?,
                    output_tokens: row.get(4)?,
                    cost_usd: row.get(5)?,
                    tool_name: row.get(6)?,
                    provider: row.get(7)?,
                    called_at: row.get(8)?,
                    cache_read_tokens: row.get(9).unwrap_or(0),
                    cache_creation_tokens: row.get(10).unwrap_or(0),
                    cache_savings_usd: row.get(11).unwrap_or(0.0),
                })
            })
            .context("failed to query api calls")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect api calls")?;
        Ok(records)
    }

    /// Get total cost for a session.
    pub fn session_cost(&self, session_id: &str) -> Result<f64> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total: f64 = conn
            .query_row(
                "select coalesce(sum(cost_usd), 0.0) from api_calls where session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0.0);
        Ok(total)
    }

    /// Search across all session conversations for a text query.
    ///
    /// Uses `SQLite` FTS5 full-text search on conversation content. Falls back
    /// to LIKE-based search if FTS tables are not available.
    ///
    pub fn search_conversations(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ConversationSearchHit>> {
        let limit = limit.unwrap_or(20);
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Try FTS5 first; if the table doesn't exist, fall back to LIKE
        let fts_exists: bool = {
            let mut check = conn
                .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='conversation_fts' LIMIT 1")
                .ok();
            check
                .take()
                .is_some_and(|mut stmt| stmt.exists([]).unwrap_or(false))
        };

        if fts_exists {
            // Use FTS5 for fast full-text search
            let mut stmt = conn
                .prepare(
                    "SELECT c.session_id, s.captured_at, snippet(conversation_fts, 1, '>>', '<<', '...', 32) as snippet
                     FROM conversation_fts c
                     JOIN session_snapshots s ON s.session_id = c.session_id
                     WHERE conversation_fts MATCH ?1
                     ORDER BY rank
                     LIMIT ?2",
                )
                .context("failed to prepare FTS search")?;

            let hits = stmt
                .query_map(params![query, limit as i64], |row| {
                    let session_id: String = row.get(0)?;
                    let captured_at: String = row.get(1)?;
                    let snippet: String = row.get(2)?;
                    Ok(ConversationSearchHit {
                        session_id: session_id,
                        captured_at,
                        snippet,
                    })
                })
                .context("failed to execute FTS search")?
                .collect::<rusqlite::Result<Vec<_>>>()
                .context("failed to collect search hits")?;
            return Ok(hits);
        }

        // Fallback: LIKE-based search on snapshot_json
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = conn
            .prepare(
                "SELECT session_id, captured_at, snapshot_json
                 FROM session_snapshots
                 WHERE snapshot_json LIKE ?1
                 ORDER BY captured_at DESC
                 LIMIT ?2",
            )
            .context("failed to prepare LIKE search")?;

        let hits = stmt
            .query_map(params![pattern, limit as i64], |row| {
                let session_id: String = row.get(0)?;
                let captured_at: String = row.get(1)?;
                let json: String = row.get(2)?;

                // Extract a snippet around the match
                let lower_json = json.to_lowercase();
                let lower_query = query.to_lowercase();
                let snippet = if let Some(pos) = lower_json.find(&lower_query) {
                    let start = pos.saturating_sub(64);
                    let end = (pos + query.len() + 64).min(json.len());
                    let raw = &json[start..end];
                    // Truncate at character boundary to avoid panics
                    let raw = raw.chars().take(200).collect::<String>();
                    format!("...{}...", raw.trim())
                } else {
                    String::new()
                };

                Ok(ConversationSearchHit {
                    session_id,
                    captured_at,
                    snippet,
                })
            })
            .context("failed to execute LIKE search")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect search hits")?;

        Ok(hits)
    }

    /// Create the FTS5 virtual table for conversation search indexing.
    ///
    /// This is called during `Storage::open` to enable fast full-text search
    /// across conversation content. If FTS5 is not available, search falls
    /// back to LIKE-based queries.
    fn ensure_fts_index(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Create a standalone FTS5 virtual table (not content-synced).
        // Content-synced FTS tables require triggers and have complex lifecycle;
        // a standalone table is simpler and more reliable.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS conversation_fts USING fts5(
                session_id,
                content,
                tokenize='porter unicode61'
            );",
        )
        .context("failed to create conversation_fts (FTS5 may not be compiled in)")?;

        // Populate FTS from existing snapshots that haven't been indexed yet
        conn.execute(
            "INSERT OR IGNORE INTO conversation_fts(session_id, content)
             SELECT s.session_id, s.snapshot_json
             FROM session_snapshots s
             WHERE NOT EXISTS (
                 SELECT 1 FROM conversation_fts c WHERE c.session_id = s.session_id
             )",
            [],
        )
        .context("failed to populate FTS index")?;

        Ok(())
    }

    // ── Database Cleanup & Maintenance ─────────────────────────────────────────

    /// Get statistics about the database.
    pub fn db_stats(&self) -> Result<DatabaseStats> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let session_count: i64 = conn
            .query_row("SELECT count(*) FROM sessions", [], |row| row.get(0))
            .unwrap_or(0);
        let event_count: i64 = conn
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
            .unwrap_or(0);
        let api_call_count: i64 = conn
            .query_row("SELECT count(*) FROM api_calls", [], |row| row.get(0))
            .unwrap_or(0);
        let hook_execution_count: i64 = conn
            .query_row("SELECT count(*) FROM hook_executions", [], |row| row.get(0))
            .unwrap_or(0);
        let checkpoint_count: i64 = conn
            .query_row("SELECT count(*) FROM checkpoints", [], |row| row.get(0))
            .unwrap_or(0);

        let oldest_session: Option<String> = conn
            .query_row(
                "SELECT created_at FROM sessions ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        let newest_session: Option<String> = conn
            .query_row(
                "SELECT created_at FROM sessions ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let db_size_bytes = conn
            .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
            .ok()
            .zip(
                conn.query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
                    .ok(),
            )
            .map_or(0, |(pages, size)| (pages as u64) * (size as u64));

        Ok(DatabaseStats {
            session_count,
            event_count,
            api_call_count,
            hook_execution_count,
            checkpoint_count,
            db_size_bytes,
            oldest_session,
            newest_session,
        })
    }

    /// Remove sessions older than `max_age_days` and all related data.
    ///
    /// Tables with `ON DELETE CASCADE` (checkpoints, `rewind_snapshots`,
    /// `hook_executions`, `api_calls`) are cleaned automatically when their
    /// parent session is deleted. The `events` table lacks cascade, so
    /// it is cleaned explicitly.
    pub fn cleanup_old_sessions(&self, max_age_days: u64) -> Result<CleanupStats> {
        let max_days_i64 = i64::try_from(max_age_days).unwrap_or(i64::MAX);
        let cutoff = Utc::now()
            - chrono::Duration::try_days(max_days_i64)
                .unwrap_or_else(|| chrono::Duration::days(i64::MAX));

        let cutoff_str = cutoff.to_rfc3339();
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Find sessions to delete
        let old_session_ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM sessions WHERE created_at < ?1")?;
            let rows = stmt.query_map(params![cutoff_str], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        if old_session_ids.is_empty() {
            return Ok(CleanupStats::default());
        }

        let mut stats = CleanupStats::default();

        for sid in &old_session_ids {
            // Count related records for stats before deletion
            stats.events_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM events WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.api_calls_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM api_calls WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.hook_executions_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM hook_executions WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.checkpoints_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM checkpoints WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.rewind_snapshots_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM rewind_snapshots WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.snapshots_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM session_snapshots WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);

            // Delete events explicitly (no cascade)
            conn.execute("DELETE FROM events WHERE session_id = ?1", params![sid])?;

            // Delete FTS entries explicitly (may not exist, ignore error)
            let fts_removed = conn
                .execute(
                    "DELETE FROM conversation_fts WHERE session_id = ?1",
                    params![sid],
                )
                .unwrap_or(0);
            stats.fts_entries_removed += u64::try_from(fts_removed).unwrap_or(0);

            // Delete session snapshots explicitly (no FK cascade)
            conn.execute(
                "DELETE FROM session_snapshots WHERE session_id = ?1",
                params![sid],
            )?;

            // Delete session (cascades to checkpoints, rewind_snapshots, hook_executions, api_calls)
            conn.execute("DELETE FROM sessions WHERE id = ?1", params![sid])?;
            stats.sessions_removed += 1;
        }

        Ok(stats)
    }

    /// Remove ALL sessions and related data.
    pub fn cleanup_all_sessions(&self) -> Result<CleanupStats> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let stats = CleanupStats {
            sessions_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM sessions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            events_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            api_calls_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM api_calls", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            hook_executions_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM hook_executions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            checkpoints_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM checkpoints", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            rewind_snapshots_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM rewind_snapshots", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            snapshots_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM session_snapshots", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            fts_entries_removed: 0,
        };

        // Delete from all tables (order matters for FK constraints)
        conn.execute("DELETE FROM events", [])?;
        conn.execute("DELETE FROM conversation_fts", []).ok();
        conn.execute("DELETE FROM session_snapshots", [])?;
        conn.execute("DELETE FROM api_calls", [])?;
        conn.execute("DELETE FROM hook_executions", [])?;
        conn.execute("DELETE FROM rewind_snapshots", [])?;
        conn.execute("DELETE FROM checkpoints", [])?;
        conn.execute("DELETE FROM sessions", [])?;

        Ok(stats)
    }

    /// Run `VACUUM` to reclaim disk space and defragment the database.
    pub fn vacuum(&self) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute_batch("VACUUM")
            .context("failed to vacuum database")?;
        Ok(())
    }

    /// Remove orphaned events that reference sessions that no longer exist.
    pub fn cleanup_orphaned_events(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let removed = conn.execute(
            "DELETE FROM events WHERE session_id NOT IN (SELECT id FROM sessions)",
            [],
        )?;
        Ok(u64::try_from(removed).unwrap_or(0))
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationSearchHit {
    /// Session ID where the match was found
    pub session_id: String,
    /// When this snapshot was captured
    pub captured_at: String,
    /// Text snippet around the match
    pub snippet: String,
}

// ── Row mappers ───────────────────────────────────────────────────────────────

fn session_from_row(row: &rusqlite::Row) -> rusqlite::Result<Session> {
    let id: String = row.get(0)?;
    let task: String = row.get(1)?;
    let created_at: String = row.get(2)?;
    let mode_str: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let plan_path: Option<String> = row.get(5)?;
    Ok(Session {
        id: SessionId::parse(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        task,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc),
        mode: serde_json::from_str(&mode_str).map_err(|e| {
            tracing::warn!("Failed to deserialize session mode '{}': {}", mode_str, e);
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
        status: serde_json::from_str(&status_str).map_err(|e| {
            tracing::warn!(
                "Failed to deserialize session status '{}': {}",
                status_str,
                e
            );
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        plan_path,
        tool_approval_mode: ToolApprovalMode::default(),
        execution_trace: None,
    })
}

fn plan_from_row(row: &rusqlite::Row) -> rusqlite::Result<Plan> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let task: String = row.get(2)?;
    let created_at: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let summary: String = row.get(5)?;
    let approach: String = row.get(6)?;
    let steps_str: String = row.get(7)?;
    let files_str: String = row.get(8)?;
    let risks_str: String = row.get(9)?;

    let to_sql_err = |e: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    };

    Ok(Plan {
        id: PlanId::parse(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        session_id: SessionId::parse(&session_id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?,
        task,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc),
        status: serde_json::from_str(&status_str).unwrap_or_default(),
        summary,
        approach,
        steps: serde_json::from_str::<Vec<PlanStep>>(&steps_str).map_err(to_sql_err)?,
        files_to_modify: serde_json::from_str::<Vec<String>>(&files_str).map_err(to_sql_err)?,
        risks: serde_json::from_str::<Vec<String>>(&risks_str).map_err(to_sql_err)?,
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
        task_profile: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{SessionCaptureManager, Storage};
    use chrono::Utc;
    use rustycode_protocol::{
        EventKind, Plan, PlanId, PlanStatus, Session, SessionEvent, SessionId, SessionMode,
        SessionStatus, ToolApprovalMode,
    };
    use std::fs;
    use std::path::PathBuf;

    fn temp_db_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustycode-storage-{}", SessionId::new()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("test.db")
    }

    fn make_session(task: &str) -> Session {
        Session {
            id: SessionId::new(),
            task: task.to_string(),
            created_at: Utc::now(),
            mode: SessionMode::Executing,
            status: SessionStatus::Executing,
            plan_path: None,
            tool_approval_mode: ToolApprovalMode::default(),
            execution_trace: None,
        }
    }

    #[test]
    fn persists_sessions_events_and_memory() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("inspect");
        storage.insert_session(&session).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: session.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "started".to_string(),
            })
            .unwrap();
        storage
            .upsert_memory("project", "style", "prefer tests")
            .unwrap();
        storage
            .upsert_memory("project", "style", "prefer coverage")
            .unwrap();

        assert_eq!(storage.session_count().unwrap(), 1);
        assert_eq!(
            storage
                .event_count_for_session(&session.id.to_string())
                .unwrap(),
            1
        );
        assert_eq!(
            storage
                .recent_tasks(5, Some(&session.id.to_string()))
                .unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(storage.recent_sessions(5).unwrap().len(), 1);
        assert_eq!(storage.session_events(&session.id).unwrap().len(), 1);
    }

    #[test]
    fn plan_mode_round_trip() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        // Start a planning session
        let session = Session {
            id: SessionId::new(),
            task: "add logging".to_string(),
            created_at: Utc::now(),
            mode: SessionMode::Planning,
            status: SessionStatus::Planning,
            plan_path: None,
            tool_approval_mode: ToolApprovalMode::default(),
            execution_trace: None,
        };
        storage.insert_session(&session).unwrap();

        // Create and store a plan
        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "add logging".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Add debug logging".to_string(),
            approach: "Insert log statements".to_string(),
            steps: vec![],
            files_to_modify: vec!["src/main.rs".to_string()],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Load it back
        let loaded = storage
            .load_plan(&plan.id)
            .unwrap()
            .expect("plan should exist");
        assert_eq!(loaded.summary, plan.summary);
        assert_eq!(loaded.files_to_modify, plan.files_to_modify);

        // Approve
        storage
            .update_plan_status(&plan.id, &PlanStatus::Approved)
            .unwrap();
        let approved = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(approved.status, PlanStatus::Approved);

        // Session update
        let mut updated = session.clone();
        updated.status = SessionStatus::Executing;
        updated.mode = SessionMode::Executing;
        storage.update_session(&updated).unwrap();
        let reloaded = storage.recent_sessions(1).unwrap();
        assert_eq!(reloaded[0].status, SessionStatus::Executing);
    }

    #[test]
    fn plan_crud_operations() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        // Create session
        let session = make_session("implement feature");
        storage.insert_session(&session).unwrap();

        // CREATE: Insert a new plan
        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "implement feature".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Implement new feature X".to_string(),
            approach: "Use TDD approach".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Write tests".to_string(),
                description: "Create unit tests".to_string(),
                tools: vec!["editor".to_string()],
                expected_outcome: "Tests fail".to_string(),
                rollback_hint: "Delete test file".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec!["src/lib.rs".to_string(), "tests/test.rs".to_string()],
            risks: vec!["May break existing functionality".to_string()],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // READ: Load plan by ID
        let loaded = storage.load_plan(&plan.id).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, plan.id);
        assert_eq!(loaded.summary, plan.summary);
        assert_eq!(loaded.approach, plan.approach);
        assert_eq!(loaded.steps.len(), 1);
        assert_eq!(loaded.steps[0].title, "Write tests");
        assert_eq!(loaded.files_to_modify.len(), 2);
        assert_eq!(loaded.risks.len(), 1);

        // UPDATE: Change plan status
        storage
            .update_plan_status(&plan.id, &PlanStatus::Ready)
            .unwrap();
        let updated = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(updated.status, PlanStatus::Ready);

        // READ: List plans for session
        let plans = storage.list_plans(&session.id).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].id, plan.id);

        // READ: Load non-existent plan returns None
        let fake_id = PlanId::new();
        let not_found = storage.load_plan(&fake_id).unwrap();
        assert!(not_found.is_none());

        // CREATE: Add multiple plans
        let plan2 = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "implement feature".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Approved,
            summary: "Alternative plan".to_string(),
            approach: "Different approach".to_string(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan2).unwrap();

        // READ: List all plans with limit
        let all_plans = storage.all_plans(10).unwrap();
        assert_eq!(all_plans.len(), 2);
    }

    #[test]
    fn plan_status_transitions() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("test status transitions");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Test plan".to_string(),
            approach: "Test approach".to_string(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Test all status transitions
        for status in [
            PlanStatus::Ready,
            PlanStatus::Approved,
            PlanStatus::Executing,
            PlanStatus::Completed,
        ] {
            storage.update_plan_status(&plan.id, &status).unwrap();
            let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
            assert_eq!(loaded.status, status);
        }
    }

    #[test]
    fn plan_with_complex_data() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("complex plan");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "complex feature".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Ready,
            summary: "A complex plan with many steps".to_string(),
            approach: "Multi-phase implementation".to_string(),
            steps: (1..=5)
                .map(|i| rustycode_protocol::PlanStep {
                    order: i,
                    title: format!("Step {}", i),
                    description: format!("Description for step {}", i),
                    tools: vec!["editor".to_string(), "bash".to_string()],
                    expected_outcome: format!("Outcome {}", i),
                    rollback_hint: format!("Rollback step {}", i),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                })
                .collect(),
            files_to_modify: vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "tests/integration.rs".to_string(),
            ],
            risks: vec![
                "Risk 1: Performance impact".to_string(),
                "Risk 2: Breaking changes".to_string(),
                "Risk 3: Compatibility issues".to_string(),
            ],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.steps.len(), 5);
        assert_eq!(loaded.files_to_modify.len(), 3);
        assert_eq!(loaded.risks.len(), 3);

        // Verify step details
        assert_eq!(loaded.steps[0].order, 1);
        assert_eq!(loaded.steps[4].title, "Step 5");
    }

    #[test]
    fn update_plan_step_successfully() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("step execution tracking");
        storage.insert_session(&session).unwrap();

        // Create a plan with multiple steps
        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "track step execution".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test step updates".to_string(),
            approach: "Update individual steps".to_string(),
            steps: vec![
                rustycode_protocol::PlanStep {
                    order: 1,
                    title: "First step".to_string(),
                    description: "Initial step".to_string(),
                    tools: vec!["editor".to_string()],
                    expected_outcome: "File created".to_string(),
                    rollback_hint: "Delete file".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 2,
                    title: "Second step".to_string(),
                    description: "Follow-up step".to_string(),
                    tools: vec!["bash".to_string()],
                    expected_outcome: "Tests pass".to_string(),
                    rollback_hint: "Revert changes".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
            ],
            files_to_modify: vec!["src/test.rs".to_string()],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Update the first step to InProgress
        let mut updated_step = plan.steps[0].clone();
        updated_step.execution_status = rustycode_protocol::StepStatus::InProgress;
        updated_step.started_at = Some(Utc::now());
        updated_step.results = vec!["Started execution".to_string()];

        storage
            .update_plan_step(&plan.id, 0, &updated_step)
            .unwrap();

        // Load the plan and verify the step was updated
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.steps.len(), 2);
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::InProgress
        );
        assert!(loaded.steps[0].started_at.is_some());
        assert_eq!(
            loaded.steps[0].results,
            vec!["Started execution".to_string()]
        );

        // Verify the second step was not affected
        assert_eq!(
            loaded.steps[1].execution_status,
            rustycode_protocol::StepStatus::Pending
        );
        assert!(loaded.steps[1].started_at.is_none());
    }

    #[test]
    fn update_plan_step_to_completed_with_results() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("complete step");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "complete step test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test step completion".to_string(),
            approach: "Update step to completed".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Execute step".to_string(),
                description: "Run the step".to_string(),
                tools: vec!["bash".to_string()],
                expected_outcome: "Success".to_string(),
                rollback_hint: "N/A".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Update step to Completed with full execution details
        let mut completed_step = plan.steps[0].clone();
        completed_step.execution_status = rustycode_protocol::StepStatus::Completed;
        completed_step.started_at = Some(Utc::now() - chrono::Duration::seconds(60));
        completed_step.completed_at = Some(Utc::now());
        completed_step.results = vec![
            "Command executed successfully".to_string(),
            "Output: test passed".to_string(),
        ];
        completed_step.tool_executions = vec![rustycode_protocol::StepToolExecution {
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command": "cargo test"}).to_string(),
            output: "test result: ok".to_string(),
            error: None,
            timestamp: Utc::now(),
        }];

        storage
            .update_plan_step(&plan.id, 0, &completed_step)
            .unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::Completed
        );
        assert!(loaded.steps[0].started_at.is_some());
        assert!(loaded.steps[0].completed_at.is_some());
        assert_eq!(loaded.steps[0].results.len(), 2);
        assert_eq!(loaded.steps[0].tool_executions.len(), 1);
        assert_eq!(loaded.steps[0].tool_executions[0].tool_name, "bash");
    }

    #[test]
    fn update_plan_step_to_failed_with_errors() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("failed step");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "failed step test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test step failure".to_string(),
            approach: "Update step to failed".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Failing step".to_string(),
                description: "This step will fail".to_string(),
                tools: vec!["bash".to_string()],
                expected_outcome: "Success".to_string(),
                rollback_hint: "Check logs".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Update step to Failed with error details
        let mut failed_step = plan.steps[0].clone();
        failed_step.execution_status = rustycode_protocol::StepStatus::Failed;
        failed_step.started_at = Some(Utc::now() - chrono::Duration::seconds(30));
        failed_step.completed_at = Some(Utc::now());
        failed_step.errors = vec![
            "Compilation failed".to_string(),
            "Error: undefined variable 'x'".to_string(),
        ];
        failed_step.results = vec!["Attempted compilation but failed".to_string()];

        storage.update_plan_step(&plan.id, 0, &failed_step).unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::Failed
        );
        assert_eq!(loaded.steps[0].errors.len(), 2);
        assert!(loaded.steps[0].errors[0].contains("Compilation failed"));
        assert_eq!(loaded.steps[0].results.len(), 1);
    }

    #[test]
    fn update_plan_step_with_invalid_index_fails() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("invalid step index");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "invalid index test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test invalid step index".to_string(),
            approach: "Try to update non-existent step".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Only step".to_string(),
                description: "Single step".to_string(),
                tools: vec![],
                expected_outcome: "Success".to_string(),
                rollback_hint: "N/A".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Try to update a step that doesn't exist
        let fake_step = rustycode_protocol::PlanStep {
            order: 99,
            title: "Non-existent".to_string(),
            description: "Does not exist".to_string(),
            tools: vec![],
            expected_outcome: "N/A".to_string(),
            rollback_hint: "N/A".to_string(),
            tool_calls: vec![],
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };

        let result = storage.update_plan_step(&plan.id, 5, &fake_step);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of bounds"));
    }

    #[test]
    fn update_plan_step_on_nonexistent_plan_fails() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        let fake_plan_id = PlanId::new();
        let fake_step = rustycode_protocol::PlanStep {
            order: 1,
            title: "Fake".to_string(),
            description: "Fake".to_string(),
            tools: vec![],
            expected_outcome: "N/A".to_string(),
            rollback_hint: "N/A".to_string(),
            tool_calls: vec![],
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };

        let result = storage.update_plan_step(&fake_plan_id, 0, &fake_step);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("plan not found"));
    }

    #[test]
    fn update_multiple_steps_sequentially() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("sequential step updates");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "multi-step execution".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test sequential updates".to_string(),
            approach: "Update steps one by one".to_string(),
            steps: vec![
                rustycode_protocol::PlanStep {
                    order: 1,
                    title: "Step 1".to_string(),
                    description: "First".to_string(),
                    tools: vec![],
                    expected_outcome: "Done".to_string(),
                    rollback_hint: "N/A".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 2,
                    title: "Step 2".to_string(),
                    description: "Second".to_string(),
                    tools: vec![],
                    expected_outcome: "Done".to_string(),
                    rollback_hint: "N/A".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 3,
                    title: "Step 3".to_string(),
                    description: "Third".to_string(),
                    tools: vec![],
                    expected_outcome: "Done".to_string(),
                    rollback_hint: "N/A".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
            ],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Complete steps one by one
        for i in 0..3 {
            let mut step = plan.steps[i].clone();
            step.execution_status = rustycode_protocol::StepStatus::Completed;
            step.started_at = Some(Utc::now());
            step.completed_at = Some(Utc::now());
            step.results = vec![format!("Step {} completed", i + 1)];

            storage.update_plan_step(&plan.id, i, &step).unwrap();
        }

        // Verify all steps are Completed
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.steps.len(), 3);
        for (i, step) in loaded.steps.iter().enumerate() {
            assert_eq!(
                step.execution_status,
                rustycode_protocol::StepStatus::Completed
            );
            assert_eq!(step.results.len(), 1);
            assert!(step.results[0].contains(&format!("{}", i + 1)));
        }
    }

    #[test]
    fn full_plan_lifecycle_test() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        // Phase 1: Create session and initial plan
        let session = make_session("full lifecycle test");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "implement feature with tests".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Implement feature X with comprehensive tests".to_string(),
            approach: "TDD approach with full coverage".to_string(),
            steps: vec![
                rustycode_protocol::PlanStep {
                    order: 1,
                    title: "Write failing tests".to_string(),
                    description: "Create test cases for the new feature".to_string(),
                    tools: vec!["editor".to_string()],
                    expected_outcome: "Tests compile and fail".to_string(),
                    rollback_hint: "Delete test file".to_string(),
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 2,
                    title: "Implement feature".to_string(),
                    description: "Write the feature code to pass tests".to_string(),
                    tools: vec!["editor".to_string()],
                    expected_outcome: "Tests pass".to_string(),
                    rollback_hint: "Revert implementation".to_string(),
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 3,
                    title: "Run tests".to_string(),
                    description: "Execute test suite to verify implementation".to_string(),
                    tools: vec!["bash".to_string()],
                    expected_outcome: "All tests pass".to_string(),
                    rollback_hint: "Fix failing tests".to_string(),
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
            ],
            files_to_modify: vec![
                "src/feature.rs".to_string(),
                "tests/feature_test.rs".to_string(),
            ],
            risks: vec![
                "Test flakiness".to_string(),
                "Performance regression".to_string(),
            ],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };

        // CREATE: Insert the plan
        storage.insert_plan(&plan).unwrap();
        assert_eq!(storage.list_plans(&session.id).unwrap().len(), 1);

        // READ: Load and verify initial state
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.status, PlanStatus::Draft);
        assert_eq!(loaded.steps.len(), 3);
        assert!(loaded
            .steps
            .iter()
            .all(|s| s.execution_status == rustycode_protocol::StepStatus::Pending));

        // UPDATE: Change status to Ready
        storage
            .update_plan_status(&plan.id, &PlanStatus::Ready)
            .unwrap();
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.status, PlanStatus::Ready);

        // UPDATE: Change status to Approved
        storage
            .update_plan_status(&plan.id, &PlanStatus::Approved)
            .unwrap();
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.status, PlanStatus::Approved);

        // UPDATE: Start execution
        storage
            .update_plan_status(&plan.id, &PlanStatus::Executing)
            .unwrap();

        // Simulate executing each step
        // Step 1: Write tests
        let mut step1 = loaded.steps[0].clone();
        step1.execution_status = rustycode_protocol::StepStatus::Completed;
        step1.started_at = Some(Utc::now() - chrono::Duration::minutes(10));
        step1.completed_at = Some(Utc::now() - chrono::Duration::minutes(9));
        step1.results = vec!["Test file created".to_string()];
        storage.update_plan_step(&plan.id, 0, &step1).unwrap();

        // Step 2: Implement feature
        let mut step2 = loaded.steps[1].clone();
        step2.execution_status = rustycode_protocol::StepStatus::Completed;
        step2.started_at = Some(Utc::now() - chrono::Duration::minutes(8));
        step2.completed_at = Some(Utc::now() - chrono::Duration::minutes(5));
        step2.results = vec!["Implementation complete".to_string()];
        storage.update_plan_step(&plan.id, 1, &step2).unwrap();

        // Step 3: Run tests (let's say it fails first)
        let mut step3 = loaded.steps[2].clone();
        step3.execution_status = rustycode_protocol::StepStatus::InProgress;
        step3.started_at = Some(Utc::now() - chrono::Duration::minutes(4));
        step3.results = vec!["Running tests...".to_string()];
        storage.update_plan_step(&plan.id, 2, &step3).unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::Completed
        );
        assert_eq!(
            loaded.steps[1].execution_status,
            rustycode_protocol::StepStatus::Completed
        );
        assert_eq!(
            loaded.steps[2].execution_status,
            rustycode_protocol::StepStatus::InProgress
        );

        // Step 3: Tests pass after fix
        let mut step3_final = loaded.steps[2].clone();
        step3_final.execution_status = rustycode_protocol::StepStatus::Completed;
        step3_final.completed_at = Some(Utc::now());
        step3_final.results = vec!["All tests passed".to_string(), "Coverage: 95%".to_string()];
        step3_final.tool_executions = vec![rustycode_protocol::StepToolExecution {
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command": "cargo test"}).to_string(),
            output: "test result: ok. 15 passed, 0 failed".to_string(),
            error: None,
            timestamp: Utc::now(),
        }];
        storage.update_plan_step(&plan.id, 2, &step3_final).unwrap();

        // UPDATE: Mark plan as completed
        storage
            .update_plan_status(&plan.id, &PlanStatus::Completed)
            .unwrap();

        // READ: Final verification
        let final_plan = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(final_plan.status, PlanStatus::Completed);
        assert!(final_plan
            .steps
            .iter()
            .all(|s| s.execution_status == rustycode_protocol::StepStatus::Completed));

        // READ: Verify plan appears in session list
        let session_plans = storage.list_plans(&session.id).unwrap();
        assert_eq!(session_plans.len(), 1);
        assert_eq!(session_plans[0].id, plan.id);

        // READ: Verify plan appears in all plans list
        let all_plans = storage.all_plans(10).unwrap();
        assert_eq!(all_plans.len(), 1);
        assert_eq!(all_plans[0].id, plan.id);
    }

    #[test]
    fn event_persistence_and_retrieval() {
        use rustycode_bus::{SessionStartedEvent, ToolExecutedEvent};
        use serde_json::json;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Create and insert a session started event
        let session_event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        storage
            .insert_event_bus(&session_event)
            .expect("Failed to insert session event");

        // Create and insert a tool executed event
        let tool_event = ToolExecutedEvent::new(
            SessionId::new(),
            "read_file".to_string(),
            json!({ "path": "/test/path" }),
            true,
            "success".to_string(),
            None,
        );

        storage
            .insert_event_bus(&tool_event)
            .expect("Failed to insert tool event");

        // Retrieve events
        let events = storage.events(10).expect("Failed to get events");

        // Verify we got 2 events
        assert_eq!(events.len(), 2);

        // Verify first event (most recent - tool event)
        assert_eq!(events[0].event_type, "tool.executed");
        assert!(events[0].event_data.contains("read_file"));
        assert!(events[0].id > 0);

        // Verify second event (session started)
        assert_eq!(events[1].event_type, "session.started");
        assert!(events[1].event_data.contains("test task"));

        // Verify timestamps are valid RFC3339
        assert!(chrono::DateTime::parse_from_rfc3339(&events[0].created_at).is_ok());
        assert!(chrono::DateTime::parse_from_rfc3339(&events[1].created_at).is_ok());
    }

    #[test]
    fn get_events_respects_limit() {
        use rustycode_bus::SessionStartedEvent;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Insert 5 events
        for i in 0..5 {
            let event = SessionStartedEvent::new(
                SessionId::new(),
                format!("task {}", i),
                format!("detail {}", i),
            );
            storage.insert_event_bus(&event).unwrap();
        }

        // Request only 3 events
        let events = storage.events(3).unwrap();
        assert_eq!(events.len(), 3);

        // Request 10 events but only 5 exist
        let events = storage.events(10).unwrap();
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn get_events_returns_most_recent_first() {
        use rustycode_bus::SessionStartedEvent;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Insert events with a small delay to ensure different timestamps
        let session_ids: Vec<_> = (0..3).map(|_| SessionId::new()).collect();

        for (i, session_id) in session_ids.iter().enumerate() {
            let event = SessionStartedEvent::new(
                session_id.clone(),
                format!("task {}", i),
                format!("detail {}", i),
            );
            storage.insert_event_bus(&event).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let events = storage.events(10).unwrap();

        // Events should be in reverse chronological order
        assert_eq!(events[0].event_type, "session.started");
        assert!(events[0].event_data.contains("task 2"));
        assert!(events[1].event_data.contains("task 1"));
        assert!(events[2].event_data.contains("task 0"));
    }

    #[test]
    fn event_persistence_with_complex_data() {
        use rustycode_bus::{ContextAssembledEvent, ToolExecutedEvent};
        use rustycode_protocol::{ContextPlan, ContextSection, ContextSectionKind};
        use serde_json::json;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Create a context assembled event with complex nested data
        let context_plan = ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![ContextSection {
                kind: ContextSectionKind::CodeExcerpts,
                tokens_reserved: 50000,
                tokens_used: 5000,
                items: vec!["src/main.rs".to_string()],
                note: "Main entry point".to_string(),
            }],
        };

        let context_event = ContextAssembledEvent::new(
            SessionId::new(),
            context_plan,
            "Context assembled".to_string(),
        );

        storage
            .insert_event_bus(&context_event)
            .expect("Failed to insert context event");

        // Create a tool event with complex arguments
        let tool_event = ToolExecutedEvent::new(
            SessionId::new(),
            "complex_tool".to_string(),
            json!({
                "nested": {
                    "array": [1, 2, 3],
                    "string": "test",
                    "number": 42.5
                }
            }),
            true,
            "Complex tool output".to_string(),
            None,
        );

        storage
            .insert_event_bus(&tool_event)
            .expect("Failed to insert tool event");

        // Retrieve and verify
        let events = storage.events(10).unwrap();
        assert_eq!(events.len(), 2);

        // Verify JSON data can be parsed
        let tool_data: serde_json::Value =
            serde_json::from_str(&events[0].event_data).expect("Failed to parse tool event data");
        assert_eq!(tool_data["tool_name"], "complex_tool");
        assert_eq!(tool_data["arguments"]["nested"]["number"], 42.5);

        let context_data: serde_json::Value = serde_json::from_str(&events[1].event_data)
            .expect("Failed to parse context event data");
        // Verify context plan data is preserved
        assert_eq!(context_data["context_plan"]["total_budget"], 200000);
        assert_eq!(context_data["detail"], "Context assembled");

        // Verify event types are correct
        assert_eq!(events[0].event_type, "tool.executed");
        assert_eq!(events[1].event_type, "context.assembled");
    }

    #[test]
    fn empty_events_table_returns_empty_vec() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        let events = storage.events(10).unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn event_record_fields_are_accessible() {
        use rustycode_bus::SessionStartedEvent;

        let storage = Storage::open(&temp_db_path()).unwrap();

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        storage.insert_event_bus(&event).unwrap();

        let events = storage.events(1).unwrap();
        assert_eq!(events.len(), 1);

        let record = &events[0];
        assert!(record.id > 0);
        assert_eq!(record.event_type, "session.started");
        assert!(!record.event_data.is_empty());
        assert!(!record.created_at.is_empty());

        // Verify we can parse the timestamp
        let parsed_time = chrono::DateTime::parse_from_rfc3339(&record.created_at);
        assert!(parsed_time.is_ok());
    }

    #[test]
    fn test_session_snapshot_round_trip() {
        use super::SessionSnapshot;
        use rustycode_protocol::{PlanId, SessionId};

        let storage = Storage::open(&temp_db_path()).unwrap();

        let snapshot = SessionSnapshot {
            session_id: SessionId::new(),
            captured_at: Utc::now(),
            conversation_json: r#"{"messages":[]}"#.to_string(),
            active_plan_id: None,
            metadata: std::collections::HashMap::new(),
        };

        storage.save_snapshot(&snapshot).unwrap();
        let loaded = storage
            .load_snapshot(&snapshot.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.session_id, snapshot.session_id);
        assert_eq!(loaded.conversation_json, snapshot.conversation_json);

        // Test list_snapshot_sessions
        let sessions = storage.list_snapshot_sessions().unwrap();
        assert!(sessions.contains(&snapshot.session_id));

        storage.delete_snapshots(&snapshot.session_id).unwrap();
        assert!(storage
            .load_snapshot(&snapshot.session_id)
            .unwrap()
            .is_none());

        // Verify list is now empty for this session
        let sessions = storage.list_snapshot_sessions().unwrap();
        assert!(!sessions.contains(&snapshot.session_id));

        // Test with active_plan_id
        let snap2 = SessionSnapshot {
            session_id: SessionId::new(),
            captured_at: Utc::now(),
            conversation_json: r#"{"messages":[{"role":"user","content":"hello"}]}"#.to_string(),
            active_plan_id: Some(PlanId::new()),
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("key".to_string(), "value".to_string());
                m
            },
        };
        storage.save_snapshot(&snap2).unwrap();
        let loaded2 = storage.load_snapshot(&snap2.session_id).unwrap().unwrap();
        assert!(loaded2.active_plan_id.is_some());
        assert_eq!(loaded2.metadata.get("key").unwrap(), "value");
    }

    #[test]
    fn search_conversations_like_fallback() {
        use super::SessionSnapshot;

        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Create a session and snapshot with known content
        let session = make_session("test search");
        storage.insert_session(&session).unwrap();

        let snap = SessionSnapshot {
            session_id: session.id.clone(),
            captured_at: Utc::now(),
            conversation_json: r#"{"messages":[{"role":"user","content":"implement fibonacci in rust"},{"role":"assistant","content":"fn fib(n: u32) -> u32 { ... }"}]}"#.to_string(),
            active_plan_id: None,
            metadata: Default::default(),
        };
        storage.save_snapshot(&snap).unwrap();

        // Search for "fibonacci"
        let hits = storage.search_conversations("fibonacci", None).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.to_lowercase().contains("fibonacci"));

        // Search for non-existent term
        let misses = storage
            .search_conversations("xyzzy_nonexistent", None)
            .unwrap();
        assert!(misses.is_empty());

        // Test limit
        let hits_limited = storage.search_conversations("fibonacci", Some(0)).unwrap();
        assert!(hits_limited.is_empty());
    }

    #[test]
    fn load_session_returns_inserted_session() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("load by id");
        storage.insert_session(&session).unwrap();

        let loaded = storage.load_session(&session.id).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.task, "load by id");
        assert_eq!(loaded.mode, SessionMode::Executing);
    }

    #[test]
    fn load_session_returns_none_for_unknown_id() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let result = storage.load_session(&SessionId::new()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_memory_returns_entries_for_scope() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        storage.upsert_memory("project", "lang", "rust").unwrap();
        storage.upsert_memory("project", "style", "async").unwrap();
        storage.upsert_memory("global", "theme", "dark").unwrap();

        let project_mem = storage.memory("project").unwrap();
        assert_eq!(project_mem.len(), 2);
        assert_eq!(project_mem[0].key, "lang"); // ordered by key
        assert_eq!(project_mem[1].key, "style");
        assert_eq!(project_mem[0].scope, "project");

        let global_mem = storage.memory("global").unwrap();
        assert_eq!(global_mem.len(), 1);
        assert_eq!(global_mem[0].value, "dark");
    }

    #[test]
    fn get_memory_returns_empty_for_unknown_scope() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let result = storage.memory("nonexistent").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn get_memory_entry_returns_specific_value() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        storage.upsert_memory("project", "style", "async").unwrap();

        let value = storage.memory_entry("project", "style").unwrap();
        assert_eq!(value, Some("async".to_string()));

        let missing = storage.memory_entry("project", "missing").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn upsert_memory_overwrites_existing() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        storage.upsert_memory("project", "style", "sync").unwrap();
        storage.upsert_memory("project", "style", "async").unwrap();

        let value = storage.memory_entry("project", "style").unwrap();
        assert_eq!(value, Some("async".to_string()));

        let all = storage.memory("project").unwrap();
        assert_eq!(all.len(), 1); // Still only one entry
    }

    #[test]
    fn multiple_sessions_coexist() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let s1 = make_session("task one");
        let s2 = make_session("task two");
        storage.insert_session(&s1).unwrap();
        storage.insert_session(&s2).unwrap();

        assert_eq!(storage.session_count().unwrap(), 2);
        let loaded1 = storage.load_session(&s1.id).unwrap().unwrap();
        assert_eq!(loaded1.task, "task one");
        let loaded2 = storage.load_session(&s2.id).unwrap().unwrap();
        assert_eq!(loaded2.task, "task two");

        let recent = storage.recent_sessions(10).unwrap();
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_db_stats_empty() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();
        let stats = storage.db_stats().unwrap();

        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.event_count, 0);
        assert_eq!(stats.api_call_count, 0);
        assert!(stats.oldest_session.is_none());
        assert!(stats.newest_session.is_none());
    }

    #[test]
    fn test_db_stats_with_data() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        let session = make_session("stats test");
        storage.insert_session(&session).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: session.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "test".to_string(),
            })
            .unwrap();

        let stats = storage.db_stats().unwrap();
        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.event_count, 1);
        assert!(stats.oldest_session.is_some());
        assert!(stats.newest_session.is_some());
    }

    #[test]
    fn test_cleanup_old_sessions_removes_nothing_when_all_recent() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        let session = make_session("recent task");
        storage.insert_session(&session).unwrap();

        // Cleanup sessions older than 365 days — nothing should be removed
        let stats = storage.cleanup_old_sessions(365).unwrap();
        assert_eq!(stats.sessions_removed, 0);

        // Session should still be there
        assert!(storage.load_session(&session.id).unwrap().is_some());
    }

    #[test]
    fn test_cleanup_all_sessions() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Insert sessions and events
        let s1 = make_session("task one");
        let s2 = make_session("task two");
        storage.insert_session(&s1).unwrap();
        storage.insert_session(&s2).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: s1.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "event1".to_string(),
            })
            .unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: s2.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "event2".to_string(),
            })
            .unwrap();

        let stats = storage.cleanup_all_sessions().unwrap();
        assert_eq!(stats.sessions_removed, 2);
        assert_eq!(stats.events_removed, 2);

        // Verify everything is gone
        let db_stats = storage.db_stats().unwrap();
        assert_eq!(db_stats.session_count, 0);
        assert_eq!(db_stats.event_count, 0);
    }

    #[test]
    fn test_vuum_compacts_database() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Insert and delete data to create fragmentation
        for i in 0..50 {
            let session = make_session(&format!("task {}", i));
            storage.insert_session(&session).unwrap();
        }
        storage.cleanup_all_sessions().unwrap();

        let size_before = storage.db_stats().unwrap().db_size_bytes;

        // Vacuum should succeed
        storage.vacuum().unwrap();

        let size_after = storage.db_stats().unwrap().db_size_bytes;
        // After vacuum, the DB should be smaller or equal (pages reclaimed)
        assert!(size_after <= size_before);
    }

    #[test]
    fn test_cleanup_orphaned_events() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Insert a session and its event
        let session = make_session("orphan test");
        storage.insert_session(&session).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: session.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "event".to_string(),
            })
            .unwrap();

        // Insert an event for a session that doesn't exist
        let fake_id = SessionId::new();
        storage
            .insert_event(&SessionEvent {
                session_id: fake_id,
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "orphan event".to_string(),
            })
            .unwrap();

        assert_eq!(storage.db_stats().unwrap().event_count, 2);

        // Cleanup orphans
        let removed = storage.cleanup_orphaned_events().unwrap();
        assert_eq!(removed, 1);

        // Only the real session's event should remain
        assert_eq!(storage.db_stats().unwrap().event_count, 1);
    }

    #[test]
    fn test_completed_summaries_capped_at_1000() {
        use crate::session_capture::SessionOutcome;
        let mgr = SessionCaptureManager::new(None);
        // Finalize 1010 sessions to exceed the cap
        for i in 0..1010 {
            let sid = SessionId::new();
            let session_id_str = sid.to_string();
            mgr.start_session(sid, format!("cap-test-{}", i));
            mgr.finalize_session(&session_id_str, SessionOutcome::Success);
        }
        // Summaries should be capped (drain 100 when reaching 1000, then push 10 more)
        {
            let summaries = mgr.completed_summaries.lock().unwrap();
            let len = summaries.len();
            assert!(len <= 1010, "Summaries grew beyond expected: {}", len);
        }
    }

    #[test]
    fn test_learnings_capped_at_500() {
        use crate::session_capture::SessionOutcome;
        let mgr = SessionCaptureManager::new(None);
        // Each session adds one learning
        for i in 0..510 {
            let sid = SessionId::new();
            let session_id_str = sid.to_string();
            mgr.start_session(sid, format!("learn-test-{}", i));
            mgr.finalize_session(&session_id_str, SessionOutcome::Success);
        }
        // Learnings should be bounded (not 510+)
        {
            let learnings = mgr.learnings.lock().unwrap();
            let len = learnings.len();
            assert!(len <= 510, "Learnings grew beyond expected: {}", len);
        }
    }
}
