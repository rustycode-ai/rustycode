//! Session types for RustyCode
//!
//! Sessions are the primary unit of work in RustyCode, tracking the complete lifecycle
//! from initial task specification through planning and execution.
//!
//! # Session Lifecycle
//!
//! 1. **Planning Mode** (`SessionMode::Planning`):
//!    - Read-only exploration of codebase
//!    - Write and execute tools are blocked
//!    - Status: `Created` → `Planning` → `PlanReady`
//!
//! 2. **Execution Mode** (`SessionMode::Executing`):
//!    - Full access to all tools
//!    - Executes approved plan steps
//!    - Status: `Executing` → `Completed` or `Failed`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::SessionId;

/// A snapshot of session state at a particular point in time, with optional git checkpoint tracking.
///
/// SessionSnapshot captures the state of a session and optionally tracks a git checkpoint
/// for recovery/rewind operations. This enables the checkpoint-based recovery system.
///
/// # Example
///
/// ```rust
/// use rustycode_protocol::{SessionSnapshot, SessionState};
///
/// let snapshot = SessionSnapshot {
///     id: "sess_123".to_string(),
///     created_at: "2025-01-01T00:00:00Z".to_string(),
///     last_step: 5,
///     state: SessionState::Active,
///     context: "Working on feature X".to_string(),
///     checkpoint_git_hash: None,
///     checkpoint_modified_files: Vec::new(),
///     checkpoint_created_at: None,
/// };
///
/// // Create a snapshot with a checkpoint
/// let snapshot_with_checkpoint = snapshot.with_checkpoint(
///     "abc123def456".to_string(),
///     vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
/// );
///
/// assert_eq!(snapshot_with_checkpoint.checkpoint_git_hash, Some("abc123def456".to_string()));
/// assert_eq!(snapshot_with_checkpoint.checkpoint_modified_files.len(), 2);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// Unique identifier for this session snapshot
    pub id: String,
    /// ISO 8601 timestamp when snapshot was created
    pub created_at: String,
    /// Last executed step number in the session
    pub last_step: usize,
    /// Current session state
    pub state: SessionState,
    /// Session context information
    pub context: String,
    /// Git commit hash for checkpoint-based recovery (optional)
    pub checkpoint_git_hash: Option<String>,
    /// List of modified files at checkpoint time
    pub checkpoint_modified_files: Vec<String>,
    /// Timestamp when checkpoint was created (optional)
    pub checkpoint_created_at: Option<String>,
}

/// Enumeration of possible session states.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session is active and running
    Active,
    /// Session is paused
    Paused,
    /// Session has completed
    Completed,
    /// Session encountered an error
    Failed,
}

impl SessionSnapshot {
    pub fn new(
        id: String,
        created_at: String,
        last_step: usize,
        state: SessionState,
        context: String,
    ) -> Self {
        Self {
            id,
            created_at,
            last_step,
            state,
            context,
            checkpoint_git_hash: None,
            checkpoint_modified_files: Vec::new(),
            checkpoint_created_at: None,
        }
    }

    /// Create a checkpoint at the current state with git hash.
    ///
    /// Returns a new SessionSnapshot with checkpoint information set.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rustycode_protocol::{SessionSnapshot, SessionState};
    ///
    /// let snapshot = SessionSnapshot::new(
    ///     "sess_123".to_string(),
    ///     "2025-01-01T00:00:00Z".to_string(),
    ///     5,
    ///     SessionState::Active,
    ///     "Working on feature".to_string(),
    /// );
    ///
    /// let with_checkpoint = snapshot.with_checkpoint(
    ///     "abc123def456".to_string(),
    ///     vec!["src/main.rs".to_string()],
    /// );
    ///
    /// assert!(with_checkpoint.checkpoint_git_hash.is_some());
    /// assert!(with_checkpoint.checkpoint_created_at.is_some());
    /// ```
    pub fn with_checkpoint(mut self, git_hash: String, modified_files: Vec<String>) -> Self {
        self.checkpoint_git_hash = Some(git_hash);
        self.checkpoint_modified_files = modified_files;
        self.checkpoint_created_at = Some(Utc::now().to_rfc3339());
        self
    }
}

/// A session representing a single task or workflow in RustyCode.
///
/// # Example
///
/// ```rust
/// use rustycode_protocol::{Session, SessionMode};
///
/// let session = Session::builder()
///     .task("Implement user authentication")
///     .with_mode(SessionMode::Planning)
///     .build();
///
/// println!("Session {} created in {:?} mode", session.id, session.mode);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier for this session
    pub id: SessionId,
    /// The task or objective this session is working on
    pub task: String,
    /// When the session was created
    pub created_at: DateTime<Utc>,
    /// Current phase of the session lifecycle (Planning vs Executing)
    #[serde(default)]
    pub mode: SessionMode,
    /// Fine-grained lifecycle state
    #[serde(default)]
    pub status: SessionStatus,
    /// Tool approval mode controlling confirmation flow
    #[serde(default)]
    pub tool_approval_mode: ToolApprovalMode,
    /// Path to the plan markdown file, when one has been generated
    pub plan_path: Option<String>,
    /// Orchestration execution trace
    #[serde(default)]
    pub execution_trace: Option<serde_json::Value>,
}

/// Coarse two-phase mode: exploring vs. executing.
///
/// Sessions operate in two primary modes to separate exploration from execution:
///
/// - **Planning**: Read-only mode for exploring the codebase and creating plans
/// - **Executing**: Full access mode for implementing approved plans
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionMode {
    /// Read-only exploration; write/exec tools are blocked
    Planning,
    /// Full access; all tools permitted
    #[default]
    Executing,
}

/// Tool approval mode controlling how tool execution is authorized.
///
/// Inspired by goose's GooseMode. This controls the confirmation flow
/// for tool calls, independent of the session phase (Planning/Executing).
///
/// # Example
///
/// ```rust
/// use rustycode_protocol::ToolApprovalMode;
///
/// let mode = ToolApprovalMode::SmartApprove;
/// assert!(mode.requires_confirmation_for("Bash"));
/// assert!(!mode.requires_confirmation_for("Read"));
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolApprovalMode {
    /// Auto-approve all tool calls (no confirmation needed)
    Auto,
    /// Require confirmation for every tool call
    Approve,
    /// Only require confirmation for sensitive operations (file writes, bash, network)
    #[default]
    SmartApprove,
    /// No tool execution allowed - chat only
    Chat,
}

impl ToolApprovalMode {
    /// Check if a given tool requires confirmation under this mode.
    pub fn requires_confirmation_for(&self, tool_name: &str) -> bool {
        match self {
            Self::Auto => false,
            Self::Approve => true,
            Self::Chat => true, // All tools blocked in chat mode
            Self::SmartApprove => Self::is_sensitive_tool(tool_name),
        }
    }

    /// Check if a tool is sensitive (requires confirmation in SmartApprove mode).
    fn is_sensitive_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "Bash"
                | "Write"
                | "Edit"
                | "text_editor_20250728"
                | "text_editor_20250124"
                | "git_commit"
                | "git_push"
                | "git_reset"
                | "docker_run"
                | "docker_build"
                | "docker_stop"
                | "apply_patch"
                | "multi_edit"
                | "database_query"
                | "database_transaction"
                | "http_post"
                | "http_put"
                | "http_delete"
        )
    }

    /// Check if tool execution is completely blocked (Chat mode).
    pub fn is_tool_blocked(&self) -> bool {
        matches!(self, Self::Chat)
    }

    /// Get a human-readable description of this mode.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Auto => "Auto-approve all tool calls",
            Self::Approve => "Require approval for every tool call",
            Self::SmartApprove => "Approve read tools, confirm sensitive operations",
            Self::Chat => "Chat only - no tool execution",
        }
    }
}

/// Fine-grained lifecycle state within a session.
///
/// # Status Transitions
///
/// Sessions follow a defined state machine with these valid transitions:
///
/// ```text
///     Created
///        |
///        v
///     Planning
///        |
///        v
///    PlanReady ──> Rejected
///        |
///        v
///    Executing ──> Failed(String)
///        |
///        v
///    Completed
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionStatus {
    /// Session created but not yet started
    Created,
    /// Actively exploring codebase (planning mode)
    Planning,
    /// Plan written, awaiting user approval
    PlanReady,
    /// Running approved plan (execution mode)
    #[default]
    Executing,
    /// Plan completed successfully
    Completed,
    /// Plan rejected by user
    Rejected,
    /// Execution encountered a fatal error
    Failed(String),
}

impl SessionStatus {
    /// Check if this status is a terminal state (no further transitions possible).
    ///
    /// Terminal states are: `Completed`, `Rejected`, and `Failed`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rustycode_protocol::SessionStatus;
    ///
    /// assert!(SessionStatus::Completed.is_terminal());
    /// assert!(SessionStatus::Rejected.is_terminal());
    /// assert!(SessionStatus::Failed("error".to_string()).is_terminal());
    /// assert!(!SessionStatus::Planning.is_terminal());
    /// ```
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Rejected | Self::Failed(_))
    }

    /// Check if this status allows transitioning to the target status.
    ///
    /// # Valid Transitions
    ///
    /// - `Created` → `Planning`
    /// - `Planning` → `PlanReady`
    /// - `PlanReady` → `Executing` or `Rejected`
    /// - `Executing` → `Completed` or `Failed`
    /// - All terminal states have no valid transitions
    ///
    /// # Example
    ///
    /// ```rust
    /// use rustycode_protocol::SessionStatus;
    ///
    /// assert!(SessionStatus::Created.can_transition_to(&SessionStatus::Planning));
    /// assert!(SessionStatus::PlanReady.can_transition_to(&SessionStatus::Executing));
    /// assert!(SessionStatus::PlanReady.can_transition_to(&SessionStatus::Rejected));
    /// assert!(!SessionStatus::Completed.can_transition_to(&SessionStatus::Planning));
    /// ```
    pub fn can_transition_to(&self, target: &Self) -> bool {
        match (self, target) {
            // Non-terminal to non-terminal
            (Self::Created, Self::Planning) => true,
            (Self::Planning, Self::PlanReady) => true,
            (Self::PlanReady, Self::Executing | Self::Rejected) => true,
            (Self::Executing, Self::Completed) => true,

            // Executing to Failed (with message)
            (Self::Executing, Self::Failed(_)) => true,

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Get the error message if this status represents a failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rustycode_protocol::SessionStatus;
    ///
    /// assert_eq!(SessionStatus::Failed("database error".to_string()).error_message(), Some("database error"));
    /// assert_eq!(SessionStatus::Completed.error_message(), None);
    /// ```
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Failed(msg) => Some(msg.as_str()),
            _ => None,
        }
    }

    /// Create a new failed status with an error message.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rustycode_protocol::SessionStatus;
    ///
    /// let failed = SessionStatus::failed("Connection timeout");
    /// assert_eq!(failed.error_message(), Some("Connection timeout"));
    /// ```
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "Created"),
            Self::Planning => write!(f, "Planning"),
            Self::PlanReady => write!(f, "Plan Ready"),
            Self::Executing => write!(f, "Executing"),
            Self::Completed => write!(f, "Completed"),
            Self::Rejected => write!(f, "Rejected"),
            Self::Failed(msg) => write!(f, "Failed: {}", msg),
        }
    }
}

impl Session {
    /// Create a new SessionBuilder for convenient session construction
    pub fn builder() -> SessionBuilder {
        SessionBuilder::new()
    }
}

/// Builder for creating Session instances with a fluent API
///
/// # Example
///
/// ```rust
/// use rustycode_protocol::{Session, SessionMode, SessionStatus};
///
/// let session = Session::builder()
///     .task("Implement feature X")
///     .with_mode(SessionMode::Planning)
///     .build();
/// ```
#[derive(Debug)]
pub struct SessionBuilder {
    task: String,
    mode: SessionMode,
    status: SessionStatus,
    plan_path: Option<String>,
    tool_approval_mode: ToolApprovalMode,
}

impl SessionBuilder {
    pub fn new() -> Self {
        Self {
            task: String::new(),
            mode: SessionMode::Executing,
            status: SessionStatus::Created,
            plan_path: None,
            tool_approval_mode: ToolApprovalMode::default(),
        }
    }

    /// Set the task description for this session
    pub fn task(mut self, task: impl Into<String>) -> Self {
        self.task = task.into();
        self
    }

    /// Set the session mode (Planning or Executing)
    pub fn with_mode(mut self, mode: SessionMode) -> Self {
        // Update status to match mode
        self.status = match &mode {
            SessionMode::Planning => SessionStatus::Planning,
            SessionMode::Executing => SessionStatus::Executing,
        };
        self.mode = mode;
        self
    }

    /// Set the session status explicitly
    pub fn with_status(mut self, status: SessionStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the plan path for this session
    pub fn with_plan_path(mut self, path: impl Into<String>) -> Self {
        self.plan_path = Some(path.into());
        self
    }

    /// Build the Session instance
    ///
    pub fn with_tool_approval(mut self, mode: ToolApprovalMode) -> Self {
        self.tool_approval_mode = mode;
        self
    }

    pub fn build(self) -> Session {
        assert!(!self.task.is_empty(), "Session task cannot be empty");
        Session {
            id: SessionId::new(),
            task: self.task,
            created_at: Utc::now(),
            mode: self.mode,
            status: self.status,
            plan_path: self.plan_path,
            tool_approval_mode: self.tool_approval_mode,
            execution_trace: None,
        }
    }
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_builder() {
        let session = Session::builder()
            .task("Test task")
            .with_mode(SessionMode::Planning)
            .build();

        assert_eq!(session.task, "Test task");
        assert_eq!(session.mode, SessionMode::Planning);
        assert_eq!(session.status, SessionStatus::Planning);
    }

    #[test]
    fn test_session_status_transitions() {
        assert!(SessionStatus::Created.can_transition_to(&SessionStatus::Planning));
        assert!(SessionStatus::Planning.can_transition_to(&SessionStatus::PlanReady));
        assert!(SessionStatus::PlanReady.can_transition_to(&SessionStatus::Executing));
        assert!(SessionStatus::Executing.can_transition_to(&SessionStatus::Completed));
    }

    #[test]
    fn test_terminal_states() {
        assert!(SessionStatus::Completed.is_terminal());
        assert!(SessionStatus::Rejected.is_terminal());
        assert!(SessionStatus::Failed("error".to_string()).is_terminal());
        assert!(!SessionStatus::Planning.is_terminal());
    }

    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", SessionStatus::Completed), "Completed");
        assert_eq!(
            format!("{}", SessionStatus::Failed("test".to_string())),
            "Failed: test"
        );
    }

    #[test]
    #[should_panic(expected = "Session task cannot be empty")]
    fn test_session_builder_empty_task_panics() {
        Session::builder().build();
    }
}
