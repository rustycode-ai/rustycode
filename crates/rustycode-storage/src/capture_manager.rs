//! Session capture manager for tracking active sessions.
//!
//! Manages active session captures, captures events as they arrive, and
//! finalizes sessions when they end. Stores summaries and learnings for
//! later use.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex as StdMutex;

use chrono::Utc;
use rustycode_protocol::SessionId;

use crate::memory_metrics::MemoryMetrics;
use crate::session_capture::{InteractionEvent, SessionCapture, SessionSummary};

/// Manages active session captures
///
/// Tracks active sessions, captures events as they arrive, and finalizes
/// sessions when they end. Stores summaries and learnings for later use.
#[derive(Debug)]
pub struct SessionCaptureManager {
    /// Active session captures by session ID
    active_captures: StdMutex<HashMap<String, SessionCapture>>,
    /// Completed session summaries
    pub(crate) completed_summaries: StdMutex<Vec<SessionSummary>>,
    /// Learnings extracted from finalized sessions
    pub(crate) learnings: StdMutex<Vec<String>>,
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
    pub fn finalize_session(&self, session_id: &str, outcome: crate::session_capture::SessionOutcome) {
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
                    outcome: crate::session_capture::SessionOutcome::Abandoned,
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
