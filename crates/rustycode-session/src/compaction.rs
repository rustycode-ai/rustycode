//! Session compaction strategies and engine
//!
//! This module provides multiple compaction strategies for reducing
//! token usage while preserving important context.

use crate::message::{Message, MessagePart, MessageRole};
use crate::session::Session;
use crate::summary::{Summary, SummaryGenerator};
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

// CompactionSnapshot holds WIP state before compaction and is restored after.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionSnapshot {
    pub session_id: String,
    pub snapshot_at: chrono::DateTime<Utc>,
    pub current_task: Option<String>,
    pub active_files: HashMap<String, String>,
    pub pending_changes_summary: Option<String>,
    pub pending_tool_call: Option<String>,
    pub token_count: usize,
    pub custom_state: HashMap<String, String>,
}

impl CompactionSnapshot {
    /// Save snapshot to disk next to the session data (atomic via temp file + rename)
    pub fn save_to_disk(&self, session_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(session_dir).with_context(|| {
            format!("Failed to create session dir for compaction snapshot: {session_dir:?}")
        })?;
        let snapshot_path = session_dir.join("compaction-snapshot.json");
        let tmp_path = session_dir.join("compaction-snapshot.json.tmp");
        let json = serde_json::to_string_pretty(self).context("serialize compaction snapshot")?;
        std::fs::write(&tmp_path, &json)
            .with_context(|| format!("Failed to write compaction snapshot to {tmp_path:?}"))?;
        if let Err(e) = std::fs::rename(&tmp_path, &snapshot_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e)
                .with_context(|| format!("Failed to rename {tmp_path:?} to {snapshot_path:?}"));
        }
        Ok(())
    }

    /// Load the most recent snapshot from disk
    pub fn load_from_disk(session_dir: &Path) -> Result<Option<Self>> {
        let snapshot_path = session_dir.join("compaction-snapshot.json");
        if !snapshot_path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(&snapshot_path).with_context(|| {
            format!("Failed to read compaction snapshot from {snapshot_path:?}")
        })?;
        let snapshot: Self =
            serde_json::from_str(&json).context("deserialize compaction snapshot")?;
        Ok(Some(snapshot))
    }

    /// Remove snapshot from disk (after successful restore)
    pub fn cleanup(session_dir: &Path) -> Result<()> {
        let snapshot_path = session_dir.join("compaction-snapshot.json");
        if snapshot_path.exists() {
            std::fs::remove_file(&snapshot_path).with_context(|| {
                format!("Failed to remove compaction snapshot at {snapshot_path:?}")
            })?;
        }
        Ok(())
    }
}

/// Type alias for custom compaction function to reduce type complexity
type CompactionFn = dyn Fn(&Session) -> Result<Vec<Message>, CompactionError> + Send + Sync;

/// Optimized token estimation for multiple messages using parallel processing
fn estimate_tokens_parallel(messages: &[Message]) -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    // For small message counts, use sequential estimation
    if messages.len() < 50 {
        return messages.iter().map(Message::estimate_tokens).sum();
    }

    // For larger message counts, use parallel estimation
    let total = AtomicUsize::new(0);
    let chunk_size = (messages.len() / num_cpus::get()).max(1);

    thread::scope(|s| {
        for chunk in messages.chunks(chunk_size) {
            s.spawn(|| {
                let chunk_tokens: usize = chunk.iter().map(Message::estimate_tokens).sum();
                total.fetch_add(chunk_tokens, Ordering::Relaxed);
            });
        }
    });

    total.load(Ordering::Relaxed)
}

/// Errors that can occur during compaction
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CompactionError {
    #[error("No messages to compact")]
    EmptySession,

    #[error("Cannot compact below minimum message count")]
    BelowMinimum,

    #[error("Summary generation failed: {0}")]
    SummaryError(String),

    #[error("Invalid compaction strategy: {0}")]
    InvalidStrategy(String),
}

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactionReport {
    /// Original message count
    pub original_count: usize,

    /// New message count after compaction
    pub new_count: usize,

    /// Original token count
    pub original_tokens: usize,

    /// New token count after compaction
    pub new_tokens: usize,

    /// Whether a summary was generated
    pub summary_generated: bool,

    /// Summary text (if generated)
    pub summary: Option<String>,

    pub messages_removed: usize,
}

impl CompactionReport {
    /// Calculate token reduction percentage
    pub fn reduction_percentage(&self) -> f64 {
        if self.original_tokens == 0 {
            0.0
        } else {
            (self.original_tokens.saturating_sub(self.new_tokens) as f64 / self.original_tokens as f64) * 100.0
        }
    }

    /// Calculate message reduction percentage
    pub fn message_reduction_percentage(&self) -> f64 {
        if self.original_count == 0 {
            0.0
        } else {
            ((self.original_count.saturating_sub(self.new_count)) as f64 / self.original_count as f64) * 100.0
        }
    }
}

/// Compaction strategy
#[derive(Clone)]
#[non_exhaustive]
pub enum CompactionStrategy {
    /// Compact when token count exceeds threshold
    TokenThreshold {
        target_ratio: f64,
        min_messages: usize,
    },

    /// Compact messages older than duration
    MessageAge {
        max_age: Duration,
        keep_recent: usize,
    },

    /// Compact based on semantic importance (0.0-1.0)
    SemanticImportance {
        importance_threshold: f32,
        min_messages: usize,
    },

    /// Custom compaction function
    Custom(std::sync::Arc<CompactionFn>),
}

impl std::fmt::Debug for CompactionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TokenThreshold {
                target_ratio,
                min_messages,
            } => f
                .debug_struct("TokenThreshold")
                .field("target_ratio", target_ratio)
                .field("min_messages", min_messages)
                .finish(),
            Self::MessageAge {
                max_age,
                keep_recent,
            } => f
                .debug_struct("MessageAge")
                .field("max_age", max_age)
                .field("keep_recent", keep_recent)
                .finish(),
            Self::SemanticImportance {
                importance_threshold,
                min_messages,
            } => f
                .debug_struct("SemanticImportance")
                .field("importance_threshold", importance_threshold)
                .field("min_messages", min_messages)
                .finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<custom function>").finish(),
            #[allow(unreachable_patterns)]
            _ => f.debug_struct("Unknown").finish_non_exhaustive(),
        }
    }
}

impl CompactionStrategy {
    /// Create a token threshold strategy
    ///
    pub const fn token_threshold(target_ratio: f64, min_messages: usize) -> Self {
        Self::TokenThreshold {
            target_ratio,
            min_messages,
        }
    }

    /// Create a message age strategy
    ///
    pub const fn message_age(max_age: Duration, keep_recent: usize) -> Self {
        Self::MessageAge {
            max_age,
            keep_recent,
        }
    }

    /// Create a semantic importance strategy
    ///
    pub const fn semantic_importance(importance_threshold: f32, min_messages: usize) -> Self {
        Self::SemanticImportance {
            importance_threshold,
            min_messages,
        }
    }
}

/// Compaction engine
#[derive(Clone)]
pub struct CompactionEngine {
    strategy: CompactionStrategy,
    use_summarization: bool,
    /// Base directory for session data. When set, snapshot paths are deterministic
    /// regardless of CWD. Falls back to `./sessions` when `None` for backward compat.
    sessions_dir: Option<std::path::PathBuf>,
}

impl CompactionEngine {
    pub const fn new(strategy: CompactionStrategy) -> Self {
        Self {
            strategy,
            use_summarization: true,
            sessions_dir: None,
        }
    }

    pub const fn with_summarization(mut self, enable: bool) -> Self {
        self.use_summarization = enable;
        self
    }

    pub fn with_sessions_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.sessions_dir = Some(dir.into());
        self
    }

    /// Compact a session using the configured strategy
    pub fn compact(
        &self,
        session: &Session,
    ) -> Result<(Vec<Message>, CompactionReport), CompactionError> {
        if session.messages.is_empty() {
            return Err(CompactionError::EmptySession);
        }

        // Take a pre-compact snapshot and persist it
        let snapshot = session
            .pre_compact()
            .map_err(|e| CompactionError::SummaryError(e.to_string()))?;
        let session_dir = self
            .sessions_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("./sessions"))
            .join(session.id.as_str());
        snapshot.save_to_disk(&session_dir).map_err(|e| {
            CompactionError::SummaryError(format!(
                "Failed to save pre-compact snapshot for session {}: {}. \
                 Aborting compaction to prevent data loss.",
                session.id.as_str(),
                e
            ))
        })?;

        let original_count = session.messages.len();
        let original_tokens = estimate_tokens_parallel(&session.messages);

        let compacted = match &self.strategy {
            CompactionStrategy::TokenThreshold {
                target_ratio,
                min_messages,
            } => self.compact_by_tokens(session, *target_ratio, *min_messages)?,

            CompactionStrategy::MessageAge {
                max_age,
                keep_recent,
            } => self.compact_by_age(session, *max_age, *keep_recent)?,

            CompactionStrategy::SemanticImportance {
                importance_threshold,
                min_messages,
            } => self.compact_by_importance(session, *importance_threshold, *min_messages)?,

            CompactionStrategy::Custom(func) => func(session)?,

            #[allow(unreachable_patterns)]
            _ => {
                return Err(CompactionError::InvalidStrategy(
                    "unknown compaction strategy".to_string(),
                ));
            }
        };

        let new_count = compacted.len();
        let new_tokens = estimate_tokens_parallel(&compacted);
        let messages_removed = original_count.saturating_sub(new_count);

        let report = CompactionReport {
            original_count,
            new_count,
            original_tokens,
            new_tokens,
            summary_generated: false,
            summary: None,
            messages_removed,
        };

        Ok((compacted, report))
    }

    /// Compact by token threshold
    fn compact_by_tokens(
        &self,
        session: &Session,
        target_ratio: f64,
        min_messages: usize,
    ) -> Result<Vec<Message>, CompactionError> {
        if session.messages.len() <= min_messages {
            return Ok(session.messages.clone());
        }

        let target_tokens =
            (estimate_tokens_parallel(&session.messages) as f64 * target_ratio).max(0.0) as usize;

        let mut compacted = Vec::new();
        let mut tokens = 0;

        // Keep recent messages (iterating from end)
        for message in session.messages.iter().rev() {
            let message_tokens = message.estimate_tokens();

            if tokens + message_tokens > target_tokens && compacted.len() >= min_messages {
                break;
            }

            compacted.push(message.clone());
            tokens += message_tokens;
        }

        // Reverse to maintain chronological order
        compacted.reverse();

        // Add summary if we dropped messages
        if compacted.len() < session.messages.len() && self.use_summarization {
            let dropped_count = session.messages.len() - compacted.len();
            let summary_msg = Message::system(format!(
                "[Compacted {dropped_count} previous messages to save tokens. Context preserved.]"
            ));
            compacted.insert(0, summary_msg);
        }

        Ok(compacted)
    }

    /// Compact by message age
    fn compact_by_age(
        &self,
        session: &Session,
        max_age: Duration,
        keep_recent: usize,
    ) -> Result<Vec<Message>, CompactionError> {
        let cutoff_seconds = max_age.num_seconds();

        // Separate recent and old messages
        let mut recent_messages = Vec::new();
        let mut old_messages = Vec::new();

        for message in &session.messages {
            let msg_age = Utc::now()
                .signed_duration_since(message.timestamp)
                .num_seconds();

            if msg_age < cutoff_seconds {
                recent_messages.push(message.clone());
            } else {
                old_messages.push(message.clone());
            }
        }

        // If all messages are recent, no compaction needed
        if old_messages.is_empty() {
            return Ok(session.messages.clone());
        }

        // Ensure we keep at least keep_recent messages from the END (most recent)
        if recent_messages.len() < keep_recent {
            let needed = keep_recent - recent_messages.len();
            let take_from = old_messages.len().saturating_sub(needed);
            let promoted: Vec<_> = old_messages.drain(take_from..).collect();
            // Prepend promoted old messages before the recent ones
            let mut final_messages = promoted;
            final_messages.extend(recent_messages);
            recent_messages = final_messages;
        }

        let mut compacted = recent_messages;

        // Add summary if we dropped messages
        if self.use_summarization && !old_messages.is_empty() {
            let summary_msg = Message::system(format!(
                "[Compacted {} older messages (> {} seconds ago). Context preserved.]",
                old_messages.len(),
                cutoff_seconds
            ));
            compacted.insert(0, summary_msg);
        }

        Ok(compacted)
    }

    /// Compact by semantic importance
    fn compact_by_importance(
        &self,
        session: &Session,
        _importance_threshold: f32,
        min_messages: usize,
    ) -> Result<Vec<Message>, CompactionError> {
        if session.messages.len() <= min_messages {
            return Ok(session.messages.clone());
        }

        // For now, use a simple heuristic:
        // - Keep all user messages
        // - Keep assistant messages with tool calls
        // - Keep most recent messages
        // - Keep system messages

        let mut important = Vec::new();
        let mut regular = Vec::new();

        for message in &session.messages {
            match message.role {
                MessageRole::User | MessageRole::System => {
                    important.push(message.clone());
                }
                MessageRole::Assistant => {
                    if message.has_tool_calls() {
                        important.push(message.clone());
                    } else {
                        regular.push(message.clone());
                    }
                }
                MessageRole::Tool => {
                    // Keep tool results if they're errors
                    if message
                        .parts
                        .iter()
                        .any(|part| matches!(part, MessagePart::ToolResult { is_error: true, .. }))
                    {
                        important.push(message.clone());
                    } else {
                        regular.push(message.clone());
                    }
                }
                #[allow(unreachable_patterns)]
                _ => {
                    // Unknown roles are treated as regular messages
                    regular.push(message.clone());
                }
            }
        }

        // Keep recent regular messages if we need more
        let needed = min_messages.saturating_sub(important.len());
        if needed > 0 && !regular.is_empty() {
            let recent_regular: Vec<_> = regular.into_iter().rev().take(needed).collect();

            important.extend(recent_regular.into_iter().rev());
        }

        Ok(important)
    }

    /// Generate a summary of compacted messages
    pub async fn generate_summary(
        &self,
        session: &Session,
        generator: &SummaryGenerator,
    ) -> Result<Summary, CompactionError> {
        generator
            .generate(session)
            .await
            .map_err(|e| CompactionError::SummaryError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageRole;

    fn create_test_session(message_count: usize) -> Session {
        let mut session = Session::new("Test");
        for i in 0..message_count {
            if i % 2 == 0 {
                session.add_message(Message::user(format!("User message {}", i)));
            } else {
                session.add_message(Message::assistant(format!("Assistant message {}", i)));
            }
        }
        session
    }

    #[test]
    fn test_token_threshold_compaction() {
        let session = create_test_session(20);
        let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 4));

        let (compacted, report) = engine.compact(&session).unwrap();

        assert!(compacted.len() < 20);
        assert!(compacted.len() >= 4);
        assert!(report.reduction_percentage() > 0.0);
    }

    #[test]
    fn test_compaction_below_min_messages() {
        let session = create_test_session(3);
        let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 4));

        let (compacted, report) = engine.compact(&session).unwrap();

        assert_eq!(compacted.len(), 3);
        assert_eq!(report.messages_removed, 0);
    }

    #[test]
    fn test_empty_session_compaction() {
        let session = Session::new("Empty");
        let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 4));

        let result = engine.compact(&session);
        assert!(matches!(result, Err(CompactionError::EmptySession)));
    }

    #[test]
    fn test_compaction_report() {
        let session = create_test_session(20);
        let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 4));

        let (_, report) = engine.compact(&session).unwrap();

        assert_eq!(report.original_count, 20);
        assert!(report.new_count < 20);
        assert!(report.original_tokens > 0);
        assert!(report.new_tokens > 0);
        assert!(report.reduction_percentage() > 0.0);
    }

    #[test]
    fn test_semantic_importance_compaction() {
        let mut session = create_test_session(10);

        // Add a tool call message (should be kept)
        let mut tool_msg = Message::assistant("I'll use a tool");
        tool_msg.add_part(MessagePart::ToolCall {
            id: "call_123".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        });
        session.add_message(tool_msg);

        let engine = CompactionEngine::new(CompactionStrategy::semantic_importance(0.5, 4));
        let (compacted, _) = engine.compact(&session).unwrap();

        // Should keep user messages and tool calls
        assert!(compacted.iter().any(|m| m.role == MessageRole::User));
        assert!(compacted.iter().any(|m| m.has_tool_calls()));
    }

    #[test]
    fn test_message_age_compaction() {
        let mut session = create_test_session(10);

        // Make some messages old
        for msg in &mut session.messages[..5] {
            msg.timestamp = Utc::now() - Duration::seconds(3600); // 1 hour ago
        }

        let engine = CompactionEngine::new(CompactionStrategy::message_age(
            Duration::seconds(1800), // 30 minutes
            3,
        ));

        let (compacted, report) = engine.compact(&session).unwrap();

        assert!(compacted.len() < 10);
        assert!(report.messages_removed > 0);
    }

    // --- CompactionError display tests ---

    #[test]
    fn test_compaction_error_display() {
        assert_eq!(
            CompactionError::EmptySession.to_string(),
            "No messages to compact"
        );
        assert_eq!(
            CompactionError::BelowMinimum.to_string(),
            "Cannot compact below minimum message count"
        );
        assert_eq!(
            CompactionError::SummaryError("oops".to_string()).to_string(),
            "Summary generation failed: oops"
        );
        assert_eq!(
            CompactionError::InvalidStrategy("bad".to_string()).to_string(),
            "Invalid compaction strategy: bad"
        );
    }

    // --- CompactionReport tests ---

    #[test]
    fn test_compaction_report_reduction_percentage_zero_original() {
        let report = CompactionReport {
            original_count: 0,
            new_count: 0,
            original_tokens: 0,
            new_tokens: 0,
            summary_generated: false,
            summary: None,
            messages_removed: 0,
        };
        assert_eq!(report.reduction_percentage(), 0.0);
    }

    #[test]
    fn test_compaction_report_message_reduction_percentage_zero() {
        let report = CompactionReport {
            original_count: 0,
            new_count: 0,
            original_tokens: 100,
            new_tokens: 50,
            summary_generated: false,
            summary: None,
            messages_removed: 0,
        };
        assert_eq!(report.message_reduction_percentage(), 0.0);
    }

    #[test]
    fn test_compaction_report_message_reduction_percentage_calculation() {
        let report = CompactionReport {
            original_count: 10,
            new_count: 4,
            original_tokens: 1000,
            new_tokens: 400,
            summary_generated: false,
            summary: None,
            messages_removed: 6,
        };
        let pct = report.message_reduction_percentage();
        assert!((pct - 60.0).abs() < f64::EPSILON);
    }

    // --- CompactionStrategy Debug and constructor tests ---

    #[test]
    fn test_token_threshold_strategy_debug() {
        let strategy = CompactionStrategy::token_threshold(0.5, 10);
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("TokenThreshold"));
        assert!(debug.contains("target_ratio"));
    }

    #[test]
    fn test_message_age_strategy_debug() {
        let strategy = CompactionStrategy::message_age(Duration::seconds(60), 5);
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("MessageAge"));
        assert!(debug.contains("keep_recent"));
    }

    #[test]
    fn test_semantic_importance_strategy_debug() {
        let strategy = CompactionStrategy::semantic_importance(0.7, 3);
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("SemanticImportance"));
        assert!(debug.contains("importance_threshold"));
    }

    #[test]
    fn test_custom_strategy_debug() {
        let strategy = CompactionStrategy::Custom(std::sync::Arc::new(|_session| Ok(vec![])));
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("Custom"));
    }

    // --- CompactionEngine with_summarization ---

    #[test]
    fn test_engine_with_summarization_disabled() {
        let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 2))
            .with_summarization(false);
        let session = create_test_session(20);
        let (compacted, report) = engine.compact(&session).unwrap();
        // Should not contain compaction summary message
        assert!(!report.summary_generated);
        assert!(report.messages_removed > 0);
        // Should not start with a system message about compaction
        if let Some(first) = compacted.first() {
            let text = first.text();
            assert!(!text.contains("Compacted"));
        }
    }

    #[test]
    fn test_engine_with_summarization_enabled() {
        let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 2))
            .with_summarization(true);
        let session = create_test_session(20);
        let (compacted, _report) = engine.compact(&session).unwrap();
        // Should start with a system summary message
        if let Some(first) = compacted.first() {
            assert_eq!(first.role, MessageRole::System);
            assert!(first.text().contains("Compacted"));
        }
    }

    // --- Edge cases ---

    #[test]
    fn test_token_threshold_exact_min_messages() {
        let session = create_test_session(5);
        let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 5));
        let (compacted, report) = engine.compact(&session).unwrap();
        // At exactly min_messages, should return all messages unchanged
        assert_eq!(compacted.len(), 5);
        assert_eq!(report.messages_removed, 0);
    }

    #[test]
    fn test_semantic_importance_tool_error_kept() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Do something"));
        session.add_message(Message::tool_result("c1", "Error!", true));
        session.add_message(Message::assistant("OK"));
        session.add_message(Message::user("More"));
        session.add_message(Message::assistant("Done"));
        session.add_message(Message::tool_result("c2", "OK", false));

        let engine = CompactionEngine::new(CompactionStrategy::semantic_importance(0.5, 3));
        let (compacted, _report) = engine.compact(&session).unwrap();

        // Tool error should be kept
        assert!(compacted.iter().any(|m| m
            .parts
            .iter()
            .any(|p| matches!(p, MessagePart::ToolResult { is_error: true, .. }))));
    }

    #[test]
    fn test_message_age_all_recent() {
        let session = create_test_session(5);
        let engine = CompactionEngine::new(CompactionStrategy::message_age(
            Duration::seconds(3600), // 1 hour - all messages are recent
            3,
        ));
        let (compacted, report) = engine.compact(&session).unwrap();
        // All recent, no compaction
        assert_eq!(compacted.len(), 5);
        assert_eq!(report.messages_removed, 0);
    }

    #[test]
    fn test_compaction_report_fields() {
        let report = CompactionReport {
            original_count: 20,
            new_count: 10,
            original_tokens: 500,
            new_tokens: 200,
            summary_generated: true,
            summary: Some("A summary".to_string()),
            messages_removed: 10,
        };
        assert_eq!(report.original_count, 20);
        assert_eq!(report.new_count, 10);
        assert!(report.summary_generated);
        assert_eq!(report.summary, Some("A summary".to_string()));
        let pct = report.reduction_percentage();
        assert!((pct - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compaction_snapshot_serde_roundtrip() {
        use std::collections::HashMap;
        let snapshot = CompactionSnapshot {
            session_id: "sess_xxx".to_string(),
            snapshot_at: Utc::now(),
            current_task: Some("Doing thing".to_string()),
            active_files: HashMap::from([("path/file.rs".to_string(), "edit".to_string())]),
            pending_changes_summary: Some("unsaved".to_string()),
            pending_tool_call: Some("call_1".to_string()),
            token_count: 42,
            custom_state: HashMap::new(),
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let de: CompactionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(de.session_id, snapshot.session_id);
        assert_eq!(de.token_count, 42);
        assert_eq!(de.current_task, snapshot.current_task);
    }

    #[test]
    fn test_compaction_snapshot_disk_roundtrip() {
        use std::fs;
        let snapshot = CompactionSnapshot {
            session_id: "sess_disk".to_string(),
            snapshot_at: Utc::now(),
            current_task: None,
            active_files: HashMap::new(),
            pending_changes_summary: None,
            pending_tool_call: None,
            token_count: 5,
            custom_state: HashMap::new(),
        };
        let dir = fs::canonicalize(".").unwrap().join("test_session_snapshot");
        let _ = fs::create_dir_all(&dir);
        snapshot.save_to_disk(&dir).unwrap();
        let loaded = CompactionSnapshot::load_from_disk(&dir).unwrap().unwrap();
        assert_eq!(loaded.session_id, snapshot.session_id);
        CompactionSnapshot::cleanup(&dir).unwrap();
        let removed = dir.clone().join("compaction-snapshot.json");
        assert!(!removed.exists());
    }
}
