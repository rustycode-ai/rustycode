//! Session Capture and Summarization
//!
//! Captures rich interaction data during user sessions and generates
//! summaries for learning and retrospective analysis.
//!
//! ## Features
//!
//! - **Rich Event Capture**: Record user messages, assistant responses, tool calls,
//!   errors, file operations, and mode changes
//! - **Session Summarization**: Extract key points, files touched, errors encountered,
//!   and learnings from session data
//! - **Metrics Tracking**: Track interaction counts, tool usage, errors, and duration
//! - **Persistent Storage**: Store session summaries for future reference
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_storage::session_capture::{SessionCapture, InteractionEvent, SessionOutcome};
//! use rustycode_protocol::SessionId;
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Create a new session capture
//! let mut capture = SessionCapture::new(
//!     SessionId::new(),
//!     "Implement user authentication".to_string(),
//! );
//!
//! // Capture interactions as they occur
//! capture.capture_interaction(InteractionEvent::UserMessage {
//!     content: "Add login functionality".to_string(),
//!     timestamp: chrono::Utc::now(),
//! });
//!
//! // Finalize and store the session summary
//! let summary = capture.finalize_session()?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rustycode_protocol::{tool_names as tn, SessionId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// The outcome of a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionOutcome {
    /// Session completed successfully with all goals achieved
    Success,
    /// Session ended with failures or errors
    Failed,
    /// Session was abandoned before completion
    Abandoned,
}

impl std::fmt::Display for SessionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failed => write!(f, "failed"),
            Self::Abandoned => write!(f, "abandoned"),
        }
    }
}

/// Types of interactions that can occur during a session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum InteractionEvent {
    /// A message from the user
    UserMessage {
        /// The content of the user's message
        content: String,
        /// When the message was sent
        timestamp: DateTime<Utc>,
    },
    /// A response from the assistant
    AssistantMessage {
        /// The content of the assistant's response
        content: String,
        /// Optional reasoning or thought process
        reasoning: Option<String>,
        /// When the response was generated
        timestamp: DateTime<Utc>,
    },
    /// A tool call execution
    ToolCall {
        /// Name of the tool that was called
        tool_name: String,
        /// Input parameters to the tool
        input: serde_json::Value,
        /// Output from the tool execution
        output: Option<serde_json::Value>,
        /// Whether the tool call succeeded
        success: bool,
        /// Duration of the tool call in milliseconds
        duration_ms: u64,
    },
    /// An error that occurred during the session
    Error {
        /// Type of error (e.g., "`ToolError`", "`ParseError`")
        error_type: String,
        /// Error message
        message: String,
        /// How the error was resolved, if at all
        resolution: Option<String>,
    },
    /// A file operation (read, write, edit)
    FileOperation {
        /// Path to the file
        path: String,
        /// Type of operation performed
        operation: FileOperationType,
        /// Content hash for tracking changes (optional)
        content_hash: Option<String>,
    },
    /// A change in the session mode
    ModeChange {
        /// Previous mode
        from: String,
        /// New mode
        to: String,
    },
}

/// Types of file operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileOperationType {
    /// File was read
    Read,
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed
    Renamed,
}

impl std::fmt::Display for FileOperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read => write!(f, "read"),
            Self::Created => write!(f, "created"),
            Self::Modified => write!(f, "modified"),
            Self::Deleted => write!(f, "deleted"),
            Self::Renamed => write!(f, "renamed"),
        }
    }
}

/// A summary of a completed session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Unique identifier for the session
    pub session_id: SessionId,
    /// The task or goal of the session
    pub task: String,
    /// Duration of the session in milliseconds
    pub duration_ms: u64,
    /// Key points or milestones from the session
    pub key_points: Vec<String>,
    /// Files that were touched during the session
    pub files_touched: Vec<String>,
    /// Errors that were encountered
    pub errors_encountered: Vec<String>,
    /// Tools that were used
    pub tools_used: Vec<String>,
    /// Outcome of the session
    pub outcome: SessionOutcome,
    /// Learnings or insights from the session
    pub learnings: Vec<String>,
    /// Recommended next steps
    pub next_steps: Vec<String>,
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// When the session ended
    pub ended_at: DateTime<Utc>,
}

/// Metrics tracked during a session
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Total number of interactions
    pub interactions_count: u64,
    /// Number of tool calls made
    pub tool_calls_count: u64,
    /// Number of errors encountered
    pub errors_count: u64,
    /// Number of files modified
    pub files_modified_count: u64,
    /// Duration of the session in milliseconds
    pub session_duration_ms: u64,
}

/// Captures and summarizes user sessions
///
/// The `SessionCapture` struct accumulates interaction events throughout
/// a session and provides methods to generate a summary and store it.
///
/// # Example
///
/// ```
/// use rustycode_storage::session_capture::{SessionCapture, InteractionEvent};
/// use rustycode_protocol::SessionId;
///
/// # fn main() -> anyhow::Result<()> {
/// let mut capture = SessionCapture::new(
///     SessionId::new(),
///     "Fix bug in authentication".to_string(),
/// );
///
/// capture.capture_interaction(InteractionEvent::UserMessage {
///     content: "The login is broken".to_string(),
///     timestamp: chrono::Utc::now(),
/// });
///
/// let summary = capture.finalize_session()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct SessionCapture {
    session_id: SessionId,
    task: String,
    started_at: DateTime<Utc>,
    events: Vec<InteractionEvent>,
    metrics: SessionMetrics,
    files_touched: HashSet<String>,
    tools_used: HashSet<String>,
    errors_encountered: Vec<String>,
    is_finalized: bool,
}

impl SessionCapture {
    /// Create a new session capture
    ///
    pub fn new(session_id: SessionId, task: String) -> Self {
        Self {
            session_id,
            task,
            started_at: Utc::now(),
            events: Vec::new(),
            metrics: SessionMetrics::default(),
            files_touched: HashSet::new(),
            tools_used: HashSet::new(),
            errors_encountered: Vec::new(),
            is_finalized: false,
        }
    }

    /// Capture an interaction event
    ///
    /// This method records an interaction and updates internal metrics.
    /// Can be called multiple times throughout the session.
    ///
    pub fn capture_interaction(&mut self, event: InteractionEvent) {
        match &event {
            InteractionEvent::UserMessage { .. } | InteractionEvent::AssistantMessage { .. } => {
                self.metrics.interactions_count += 1;
            }
            InteractionEvent::ToolCall {
                tool_name, success, ..
            } => {
                self.metrics.tool_calls_count += 1;
                self.tools_used.insert(tool_name.clone());
                if !success {
                    self.metrics.errors_count += 1;
                }
            }
            InteractionEvent::Error {
                error_type,
                message,
                ..
            } => {
                self.metrics.errors_count += 1;
                self.errors_encountered
                    .push(format!("{error_type}: {message}"));
            }
            InteractionEvent::FileOperation {
                path, operation, ..
            } => {
                self.files_touched.insert(path.clone());
                if matches!(
                    operation,
                    FileOperationType::Created
                        | FileOperationType::Modified
                        | FileOperationType::Deleted
                ) {
                    self.metrics.files_modified_count += 1;
                }
            }
            InteractionEvent::ModeChange { .. } => {
                self.metrics.interactions_count += 1;
            }
        }

        self.events.push(event);
    }

    /// Finalize the session and generate a summary
    ///
    /// This method should be called once at the end of a session.
    /// It calculates final metrics and extracts key information.
    ///
    pub fn finalize_session(&mut self) -> Result<SessionSummary> {
        if self.is_finalized {
            anyhow::bail!("session {} already finalized", self.session_id);
        }
        self.is_finalized = true;

        let ended_at = Utc::now();
        let duration_ms = (ended_at - self.started_at).num_milliseconds().max(0) as u64;
        self.metrics.session_duration_ms = duration_ms;

        // Extract key points from events
        let key_points = self.extract_key_points();

        // Generate learnings based on errors and patterns
        let learnings = self.generate_learnings();

        // Suggest next steps based on outcome
        let next_steps = self.generate_next_steps();

        Ok(SessionSummary {
            session_id: self.session_id.clone(),
            task: self.task.clone(),
            duration_ms,
            key_points,
            files_touched: self.files_touched.iter().cloned().collect(),
            errors_encountered: self.errors_encountered.clone(),
            tools_used: self.tools_used.iter().cloned().collect(),
            outcome: self.determine_outcome(),
            learnings,
            next_steps,
            started_at: self.started_at,
            ended_at,
        })
    }

    /// Store a session summary to disk
    ///
    pub fn store_summary(summary: &SessionSummary, storage_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(storage_dir).with_context(|| {
            format!(
                "failed to create storage directory {}",
                storage_dir.display()
            )
        })?;

        let filename = format!("{}_summary.json", summary.session_id);
        let path = storage_dir.join(&filename);

        let json =
            serde_json::to_string_pretty(summary).context("failed to serialize session summary")?;

        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json).with_context(|| {
            format!("failed to write session summary to {}", tmp_path.display())
        })?;
        std::fs::rename(&tmp_path, &path)
            .with_context(|| format!("failed to rename session summary to {}", path.display()))?;

        Ok(())
    }

    /// Get current metrics
    ///
    /// Returns a snapshot of the current session metrics.
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::session_capture::SessionCapture;
    /// use rustycode_protocol::SessionId;
    ///
    /// let capture = SessionCapture::new(SessionId::new(), "test".to_string());
    /// let metrics = capture.metrics();
    /// assert_eq!(metrics.interactions_count, 0);
    /// ```
    pub const fn metrics(&self) -> SessionMetrics {
        self.metrics
    }

    /// Get the session ID
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Get the task description
    pub fn task(&self) -> &str {
        &self.task
    }

    /// Check if the session has been finalized
    pub const fn is_finalized(&self) -> bool {
        self.is_finalized
    }

    /// Extract key points from the captured events
    fn extract_key_points(&self) -> Vec<String> {
        let mut points = Vec::new();

        for event in &self.events {
            match event {
                InteractionEvent::ToolCall {
                    tool_name, success, ..
                } if *success => {
                    points.push(format!("Successfully executed {tool_name}"));
                }
                InteractionEvent::FileOperation {
                    path, operation, ..
                } => {
                    points.push(format!("{operation} file: {path}"));
                }
                InteractionEvent::ModeChange { from, to } => {
                    points.push(format!("Mode changed from '{from}' to '{to}'"));
                }
                _ => {}
            }
        }

        // Deduplicate while preserving order
        let mut seen = HashSet::new();
        points.retain(|p| seen.insert(p.clone()));

        points
    }

    /// Generate learnings from the session
    fn generate_learnings(&self) -> Vec<String> {
        let mut learnings = Vec::new();

        if !self.errors_encountered.is_empty() {
            learnings.push(format!(
                "Encountered {} error(s) during session",
                self.errors_encountered.len()
            ));
        }

        if !self.tools_used.is_empty() {
            learnings.push(format!("Used {} different tool(s)", self.tools_used.len()));
        }

        if !self.files_touched.is_empty() {
            learnings.push(format!(
                "Modified {} file(s)",
                self.metrics.files_modified_count
            ));
        }

        learnings
    }

    /// Generate suggested next steps
    fn generate_next_steps(&self) -> Vec<String> {
        let mut steps = Vec::new();

        if !self.errors_encountered.is_empty() {
            steps.push("Review and address encountered errors".to_string());
        }

        if self.metrics.files_modified_count > 10 {
            steps.push("Consider breaking down future tasks into smaller chunks".to_string());
        }

        if self.tools_used.contains(tn::BASH) {
            steps.push("Verify any shell commands executed during session".to_string());
        }

        steps
    }

    /// Determine the session outcome based on captured data
    const fn determine_outcome(&self) -> SessionOutcome {
        if self.is_finalized {
            if self.errors_encountered.is_empty() {
                SessionOutcome::Success
            } else {
                SessionOutcome::Failed
            }
        } else {
            SessionOutcome::Abandoned
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_capture() -> SessionCapture {
        SessionCapture::new(SessionId::new(), "Test task".to_string())
    }

    #[test]
    fn test_new_capture() {
        let capture = create_test_capture();
        assert!(!capture.is_finalized());
        assert_eq!(capture.metrics().interactions_count, 0);
        assert_eq!(capture.task(), "Test task");
    }

    #[test]
    fn test_capture_user_message() {
        let mut capture = create_test_capture();
        capture.capture_interaction(InteractionEvent::UserMessage {
            content: "Hello".to_string(),
            timestamp: Utc::now(),
        });

        assert_eq!(capture.metrics().interactions_count, 1);
    }

    #[test]
    fn test_capture_tool_call() {
        let mut capture = create_test_capture();
        capture.capture_interaction(InteractionEvent::ToolCall {
            tool_name: "Read".to_string(),
            input: serde_json::json!({"path": "/tmp/test"}),
            output: None,
            success: true,
            duration_ms: 100,
        });

        assert_eq!(capture.metrics().tool_calls_count, 1);
    }

    #[test]
    fn test_capture_error() {
        let mut capture = create_test_capture();
        capture.capture_interaction(InteractionEvent::Error {
            error_type: "ToolError".to_string(),
            message: "File not found".to_string(),
            resolution: None,
        });

        assert_eq!(capture.metrics().errors_count, 1);
    }

    #[test]
    fn test_capture_file_operation() {
        let mut capture = create_test_capture();
        capture.capture_interaction(InteractionEvent::FileOperation {
            path: "/tmp/test.txt".to_string(),
            operation: FileOperationType::Modified,
            content_hash: Some("abc123".to_string()),
        });

        assert_eq!(capture.metrics().files_modified_count, 1);
    }

    #[test]
    fn test_finalize_session() {
        let mut capture = create_test_capture();
        capture.capture_interaction(InteractionEvent::UserMessage {
            content: "Test".to_string(),
            timestamp: Utc::now(),
        });

        let summary = capture.finalize_session().unwrap();

        assert!(capture.is_finalized());
        assert_eq!(summary.task, "Test task");
        let _ = summary.duration_ms;
    }

    #[test]
    fn test_store_and_load_summary() {
        let dir = TempDir::new().unwrap();
        let mut capture = create_test_capture();

        capture.capture_interaction(InteractionEvent::UserMessage {
            content: "Test message".to_string(),
            timestamp: Utc::now(),
        });

        let summary = capture.finalize_session().unwrap();
        SessionCapture::store_summary(&summary, dir.path()).unwrap();

        // Verify file was created
        let filename = format!("{}_summary.json", summary.session_id);
        let path = dir.path().join(&filename);
        assert!(path.exists());

        // Verify content can be loaded
        let json = std::fs::read_to_string(&path).unwrap();
        let loaded: SessionSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.task, summary.task);
        assert_eq!(loaded.session_id, summary.session_id);
    }

    #[test]
    fn test_session_outcome_display() {
        assert_eq!(SessionOutcome::Success.to_string(), "success");
        assert_eq!(SessionOutcome::Failed.to_string(), "failed");
        assert_eq!(SessionOutcome::Abandoned.to_string(), "abandoned");
    }

    #[test]
    fn test_file_operation_type_display() {
        assert_eq!(FileOperationType::Read.to_string(), "read");
        assert_eq!(FileOperationType::Created.to_string(), "created");
        assert_eq!(FileOperationType::Modified.to_string(), "modified");
        assert_eq!(FileOperationType::Deleted.to_string(), "deleted");
        assert_eq!(FileOperationType::Renamed.to_string(), "renamed");
    }

    #[test]
    fn test_key_points_extraction() {
        let mut capture = create_test_capture();

        capture.capture_interaction(InteractionEvent::FileOperation {
            path: "/tmp/file.rs".to_string(),
            operation: FileOperationType::Created,
            content_hash: None,
        });

        capture.capture_interaction(InteractionEvent::ModeChange {
            from: "planning".to_string(),
            to: "executing".to_string(),
        });

        let summary = capture.finalize_session().unwrap();
        assert!(!summary.key_points.is_empty());
    }

    #[test]
    fn test_learnings_generation() {
        let mut capture = create_test_capture();

        capture.capture_interaction(InteractionEvent::Error {
            error_type: "TestError".to_string(),
            message: "Test message".to_string(),
            resolution: None,
        });

        let summary = capture.finalize_session().unwrap();
        assert!(!summary.learnings.is_empty());
    }

    #[test]
    fn test_finalize_twice_returns_error() {
        let mut capture = create_test_capture();
        capture.finalize_session().unwrap();
        let result = capture.finalize_session();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already finalized"));
    }

    #[test]
    fn test_multiple_file_operations() {
        let mut capture = create_test_capture();

        capture.capture_interaction(InteractionEvent::FileOperation {
            path: "/tmp/file1.rs".to_string(),
            operation: FileOperationType::Modified,
            content_hash: None,
        });

        capture.capture_interaction(InteractionEvent::FileOperation {
            path: "/tmp/file2.rs".to_string(),
            operation: FileOperationType::Modified,
            content_hash: None,
        });

        capture.capture_interaction(InteractionEvent::FileOperation {
            path: "/tmp/file1.rs".to_string(), // Duplicate
            operation: FileOperationType::Modified,
            content_hash: None,
        });

        let summary = capture.finalize_session().unwrap();
        // Should deduplicate files but count all modifications
        assert_eq!(summary.files_touched.len(), 2);
        assert_eq!(capture.metrics().files_modified_count, 3);
    }
}
