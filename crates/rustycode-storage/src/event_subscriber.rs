//! Event subscriber that persists events from the event bus to storage.
//!
//! The `EventSubscriber` runs as a background task that:
//! - Subscribes to all events from the `EventBus`
//! - Persists events to the database
//! - Captures session data for learning and summarization
//! - Handles graceful shutdown

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::params;
use rustycode_bus::{EventBus, SubscriptionHandle};
use rustycode_protocol::SessionId;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

use crate::capture_manager::SessionCaptureManager;
use crate::session_capture::{FileOperationType, InteractionEvent, SessionSummary};
use crate::Storage;

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
pub(crate) fn process_event_for_capture(
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
            capture_manager
                .finalize_session(session_id, crate::session_capture::SessionOutcome::Success);
        }
        "session.failed" => {
            capture_manager
                .finalize_session(session_id, crate::session_capture::SessionOutcome::Failed);
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
