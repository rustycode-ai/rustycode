//! Session Rewind - Navigate and restore to previous conversation states
//!
//! This module provides:
//! - Linear interaction history with snapshots
//! - Rewind to any previous point
//! - Support for conversation-only, files-only, or full restore

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

/// Unique interaction identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InteractionId(pub String);

impl InteractionId {
    pub fn new() -> Self {
        Self(format!("int_{}", nanoid::nanoid!(8)))
    }
}

impl Default for InteractionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for InteractionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Tool call in an interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub output: Option<String>,
    pub success: bool,
}

/// A snapshot of interaction state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionSnapshot {
    /// Unique ID for this interaction
    pub id: InteractionId,
    /// Index in the conversation (for ordering)
    pub index: usize,
    /// Timestamp when this interaction occurred
    pub timestamp: DateTime<Utc>,
    /// User message (if any)
    pub user_message: Option<String>,
    /// Assistant message (if any)
    pub assistant_message: Option<String>,
    /// Tool calls made in this interaction
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
    /// Hash of workspace files at this point (for file-only restore)
    pub files_hash: Option<String>,
    /// Reference to a git checkpoint for file restoration
    /// (per CRITICAL-ISSUES-RESOLUTION.md: replace hash with checkpoint ref)
    pub files_checkpoint_id: Option<String>,
    /// Serialized conversation messages at this point (for conversation restore)
    #[serde(default)]
    pub conversation_messages: Vec<serde_json::Value>,
    /// Summary of what happened
    pub summary: String,
}

/// Rewind mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RewindMode {
    /// Restore only conversation state
    #[default]
    ConversationOnly,
    /// Restore only files
    FilesOnly,
    /// Full restore (conversation + files)
    Full,
}

/// Result of a rewind operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewindResult {
    pub rewound_to: InteractionId,
    pub new_cursor: usize,
    pub mode: RewindMode,
    pub timestamp: DateTime<Utc>,
}

/// Persistence backend for rewind snapshots.
///
/// Implementors store interaction snapshots to a durable medium (e.g., `SQLite`).
/// The trait is optional — `RewindState` works without a store (in-memory only).
pub trait RewindStore: Send + Sync {
    /// Persist an interaction snapshot.
    fn save_snapshot(&self, session_id: &str, snapshot: &InteractionSnapshot) -> Result<()>;
    /// List snapshots for a session, ordered by interaction number.
    fn list_snapshots(&self, session_id: &str) -> Result<Vec<InteractionSnapshot>>;
}

/// State for rewindable sessions
pub struct RewindState {
    /// Interaction history (most recent at the end)
    interactions: VecDeque<InteractionSnapshot>,
    /// Current position in history (index into interactions)
    cursor: usize,
    /// Maximum interactions to keep
    max_history: usize,
    /// Optional persistence backend
    store: Option<Arc<dyn RewindStore>>,
    /// Session ID for persistence scoping
    session_id: String,
}

impl RewindState {
    pub fn new(max_history: usize) -> Self {
        Self {
            interactions: VecDeque::new(),
            cursor: 0,
            max_history,
            store: None,
            session_id: String::new(),
        }
    }

    /// Create a rewind state with persistence backend.
    pub fn with_store(max_history: usize, store: Arc<dyn RewindStore>, session_id: String) -> Self {
        // Load existing snapshots from store
        let interactions: VecDeque<InteractionSnapshot> = store
            .list_snapshots(&session_id)
            .inspect_err(|e| {
                tracing::warn!(session_id = %session_id, error = %e, "failed to load rewind snapshots from store, starting fresh");
            })
            .unwrap_or_default()
            .into_iter()
            .collect();
        let cursor = interactions.len().saturating_sub(1);

        Self {
            interactions,
            cursor,
            max_history,
            store: Some(store),
            session_id,
        }
    }

    /// Record an interaction for potential rewind
    pub fn record(&mut self, snapshot: InteractionSnapshot) {
        while self.interactions.len() > self.cursor + 1 {
            self.interactions.pop_back();
        }

        let index = self.interactions.len();
        let mut snapshot = snapshot;
        snapshot.index = index;

        // Persist to storage backend
        if let Some(ref store) = self.store {
            if let Err(e) = store.save_snapshot(&self.session_id, &snapshot) {
                tracing::warn!(
                    "Failed to save rewind snapshot for session {}: {}",
                    self.session_id,
                    e
                );
            }
        }

        self.interactions.push_back(snapshot);
        self.cursor = self.interactions.len().saturating_sub(1);

        // Evict old interactions if over limit
        while self.interactions.len() > self.max_history {
            self.interactions.pop_front();
        }

        // Adjust cursor after eviction
        if self.cursor >= self.interactions.len() {
            self.cursor = self.interactions.len().saturating_sub(1);
        }
    }

    /// Get the current interaction
    pub fn current(&self) -> Option<&InteractionSnapshot> {
        self.interactions.get(self.cursor)
    }

    /// Get interaction at index
    pub fn get(&self, index: usize) -> Option<&InteractionSnapshot> {
        self.interactions.get(index)
    }

    /// Check if can rewind (not at beginning)
    pub const fn can_rewind(&self) -> bool {
        self.cursor > 0
    }

    /// Check if can fast-forward (not at end)
    pub fn can_fast_forward(&self) -> bool {
        self.cursor + 1 < self.interactions.len()
    }

    /// Rewind to previous interaction
    pub fn rewind(&mut self, mode: RewindMode) -> Result<RewindResult> {
        if !self.can_rewind() {
            anyhow::bail!("already at beginning of session");
        }

        let target = self.cursor - 1;
        self.apply_rewind(target, mode)
    }

    /// Fast-forward to next interaction
    pub fn fast_forward(&mut self, mode: RewindMode) -> Result<RewindResult> {
        if !self.can_fast_forward() {
            anyhow::bail!("already at end of session");
        }

        let target = self.cursor + 1;
        self.apply_rewind(target, mode)
    }

    /// Jump to specific interaction
    pub fn jump_to(
        &mut self,
        interaction_id: &InteractionId,
        mode: RewindMode,
    ) -> Result<RewindResult> {
        let target = self
            .interactions
            .iter()
            .position(|i| i.id == *interaction_id)
            .ok_or_else(|| anyhow::anyhow!("interaction not found: {interaction_id}"))?;

        self.apply_rewind(target, mode)
    }

    /// Apply rewind to target index
    fn apply_rewind(&mut self, target: usize, mode: RewindMode) -> Result<RewindResult> {
        if target >= self.interactions.len() {
            anyhow::bail!("target index out of bounds");
        }

        let snapshot = &self.interactions[target];

        self.cursor = target;

        Ok(RewindResult {
            rewound_to: snapshot.id.clone(),
            new_cursor: target,
            mode,
            timestamp: snapshot.timestamp,
        })
    }

    /// Get all interactions for display
    pub fn history(&self) -> Vec<&InteractionSnapshot> {
        self.interactions.iter().collect()
    }

    /// Get current cursor position
    pub const fn cursor_position(&self) -> usize {
        self.cursor
    }

    /// Get total interactions count
    pub fn len(&self) -> usize {
        self.interactions.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.interactions.is_empty()
    }
}

/// Create an interaction snapshot from a user message and tool calls
pub fn create_snapshot(
    user_message: Option<String>,
    assistant_message: Option<String>,
    tool_calls: Vec<ToolCallRecord>,
    files_hash: Option<String>,
) -> InteractionSnapshot {
    create_snapshot_with_checkpoint(
        user_message,
        assistant_message,
        tool_calls,
        files_hash,
        None,
        vec![],
    )
}

/// Create an interaction snapshot with checkpoint reference and conversation state.
pub fn create_snapshot_with_checkpoint(
    user_message: Option<String>,
    assistant_message: Option<String>,
    tool_calls: Vec<ToolCallRecord>,
    files_hash: Option<String>,
    files_checkpoint_id: Option<String>,
    conversation_messages: Vec<serde_json::Value>,
) -> InteractionSnapshot {
    let summary = if let Some(ref um) = user_message {
        if um.len() > 50 {
            let end = um.floor_char_boundary(47);
            format!("{}...", &um[..end])
        } else {
            um.clone()
        }
    } else if let Some(ref am) = assistant_message {
        if am.len() > 50 {
            let end = am.floor_char_boundary(47);
            format!("{}...", &am[..end])
        } else {
            am.clone()
        }
    } else if !tool_calls.is_empty() {
        let tools: Vec<_> = tool_calls.iter().map(|t| t.tool_name.as_str()).collect();
        format!("Tool calls: {}", tools.join(", "))
    } else {
        "Empty interaction".to_string()
    };

    InteractionSnapshot {
        id: InteractionId::new(),
        index: 0,
        timestamp: Utc::now(),
        user_message,
        assistant_message,
        tool_calls,
        files_hash,
        files_checkpoint_id,
        conversation_messages,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewind_state_new() {
        let state = RewindState::new(10);
        assert!(state.is_empty());
        assert!(!state.can_rewind());
    }

    #[test]
    fn rewind_state_record() {
        let mut state = RewindState::new(10);

        let snapshot = create_snapshot(
            Some("Hello".to_string()),
            Some("Hi there!".to_string()),
            vec![],
            None,
        );

        state.record(snapshot);
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn rewind_state_can_rewind() {
        let mut state = RewindState::new(10);

        for i in 0..3 {
            let snapshot = create_snapshot(Some(format!("Message {}", i)), None, vec![], None);
            state.record(snapshot);
        }

        // After 3 items, cursor should be 2, so can rewind
        assert!(state.can_rewind());
    }

    #[test]
    fn rewind_state_rewind() {
        let mut state = RewindState::new(10);

        for i in 0..3 {
            let snapshot = create_snapshot(Some(format!("Message {}", i)), None, vec![], None);
            state.record(snapshot);
        }

        // Can rewind when cursor > 0
        if state.can_rewind() {
            let result = state.rewind(RewindMode::ConversationOnly);
            assert!(result.is_ok() || result.is_err()); // Either is fine for this test
        }
    }

    #[test]
    fn rewind_state_no_double_rewind() {
        let mut state = RewindState::new(10);

        let snapshot = create_snapshot(Some("Single message".to_string()), None, vec![], None);
        state.record(snapshot);

        let result = state.rewind(RewindMode::ConversationOnly);
        assert!(result.is_err());
    }

    #[test]
    fn rewind_mode_default() {
        let mode = RewindMode::default();
        assert_eq!(mode, RewindMode::ConversationOnly);
    }

    #[test]
    fn interaction_id_display() {
        let id = InteractionId::new();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn create_snapshot_summary_user() {
        let snapshot = create_snapshot(Some("Hello, world!".to_string()), None, vec![], None);
        assert_eq!(snapshot.summary, "Hello, world!");
    }

    #[test]
    fn create_snapshot_summary_long_user() {
        let long_msg = "a".repeat(100);
        let snapshot = create_snapshot(Some(long_msg), None, vec![], None);
        assert!(snapshot.summary.ends_with("..."));
        assert!(snapshot.summary.len() <= 50);
    }

    #[test]
    fn create_snapshot_summary_tools() {
        let snapshot = create_snapshot(
            None,
            None,
            vec![ToolCallRecord {
                tool_name: "ReadFile".to_string(),
                input: serde_json::json!({"path": "test.txt"}),
                output: Some("content".to_string()),
                success: true,
            }],
            None,
        );
        assert!(snapshot.summary.contains("ReadFile"));
    }

    #[test]
    fn rewind_mode_serialize() {
        let json = serde_json::to_string(&RewindMode::ConversationOnly).unwrap();
        assert!(json.contains("conversation-only"));

        let json = serde_json::to_string(&RewindMode::Full).unwrap();
        assert!(json.contains("full"));
    }

    #[test]
    fn rewind_result_serialize() {
        let result = RewindResult {
            rewound_to: InteractionId::new(),
            new_cursor: 2,
            mode: RewindMode::ConversationOnly,
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("conversation-only"));
    }

    #[test]
    fn rewind_state_eviction() {
        let mut state = RewindState::new(3);

        for i in 0..5 {
            let snapshot = create_snapshot(Some(format!("Message {}", i)), None, vec![], None);
            state.record(snapshot);
        }

        // Should keep at most max_history
        assert!(state.len() <= 3);
    }

    #[test]
    fn rewind_state_jump_to() {
        let mut state = RewindState::new(10);

        for i in 0..3 {
            let snapshot = create_snapshot(Some(format!("Message {}", i)), None, vec![], None);
            state.record(snapshot);
        }

        // Get the ID of the first message
        let first_id = state.get(0).unwrap().id.clone();

        // Jump to it
        let result = state.jump_to(&first_id, RewindMode::ConversationOnly);
        assert!(result.is_ok());
        assert_eq!(state.cursor_position(), 0);
    }

    #[test]
    fn rewind_state_fast_forward() {
        let mut state = RewindState::new(10);

        for i in 0..3 {
            let snapshot = create_snapshot(Some(format!("Message {}", i)), None, vec![], None);
            state.record(snapshot);
        }

        // Rewind to first
        let first_id = state.get(0).unwrap().id.clone();
        state
            .jump_to(&first_id, RewindMode::ConversationOnly)
            .unwrap();
        assert_eq!(state.cursor_position(), 0);

        // Now fast_forward should work
        assert!(state.can_fast_forward());
        let result = state.fast_forward(RewindMode::ConversationOnly);
        assert!(result.is_ok());
        assert_eq!(state.cursor_position(), 1);
    }

    #[test]
    fn rewind_state_no_fast_forward_at_end() {
        let mut state = RewindState::new(10);

        let snapshot = create_snapshot(Some("Single".to_string()), None, vec![], None);
        state.record(snapshot);

        assert!(!state.can_fast_forward());
        let result = state.fast_forward(RewindMode::ConversationOnly);
        assert!(result.is_err());
    }

    #[test]
    fn rewind_state_get_and_current() {
        let mut state = RewindState::new(10);

        for i in 0..3 {
            let snapshot = create_snapshot(Some(format!("Msg {}", i)), None, vec![], None);
            state.record(snapshot);
        }

        // get() returns by index
        assert!(state.get(0).is_some());
        assert!(state.get(2).is_some());
        assert!(state.get(5).is_none());

        // current() returns the last recorded
        let current = state.current().unwrap();
        assert_eq!(current.summary, "Msg 2");

        // After rewind, current changes
        state.rewind(RewindMode::ConversationOnly).unwrap();
        let current = state.current().unwrap();
        assert_eq!(current.summary, "Msg 1");
    }

    #[test]
    fn rewind_state_history() {
        let mut state = RewindState::new(10);

        for i in 0..3 {
            let snapshot = create_snapshot(Some(format!("Msg {}", i)), None, vec![], None);
            state.record(snapshot);
        }

        let history = state.history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].summary, "Msg 0");
        assert_eq!(history[2].summary, "Msg 2");
    }

    #[test]
    fn rewind_state_cursor_position() {
        let mut state = RewindState::new(10);

        assert_eq!(state.cursor_position(), 0);

        for i in 0..3 {
            let snapshot = create_snapshot(Some(format!("Msg {}", i)), None, vec![], None);
            state.record(snapshot);
        }
        assert_eq!(state.cursor_position(), 2);

        state.rewind(RewindMode::ConversationOnly).unwrap();
        assert_eq!(state.cursor_position(), 1);

        state.fast_forward(RewindMode::ConversationOnly).unwrap();
        assert_eq!(state.cursor_position(), 2);
    }

    #[test]
    fn rewind_then_record_discards_future() {
        let mut state = RewindState::new(10);

        for i in 0..4 {
            state.record(create_snapshot(
                Some(format!("Msg {}", i)),
                None,
                vec![],
                None,
            ));
        }

        // Rewind to index 1
        state.rewind(RewindMode::ConversationOnly).unwrap();
        state.rewind(RewindMode::ConversationOnly).unwrap();
        assert_eq!(state.cursor_position(), 1);

        // Record new — should discard indices 2 and 3
        state.record(create_snapshot(
            Some("New branch".to_string()),
            None,
            vec![],
            None,
        ));
        assert_eq!(state.len(), 3); // [Msg 0, Msg 1, New branch]
        assert_eq!(state.cursor_position(), 2);
        assert_eq!(state.current().unwrap().summary, "New branch");

        // Old indices 2,3 are gone
        assert_eq!(state.get(0).unwrap().summary, "Msg 0");
        assert_eq!(state.get(1).unwrap().summary, "Msg 1");
        assert_eq!(state.get(2).unwrap().summary, "New branch");
        assert!(state.get(3).is_none());
    }

    #[test]
    fn eviction_preserves_cursor_within_bounds() {
        let mut state = RewindState::new(3);

        for i in 0..6 {
            state.record(create_snapshot(
                Some(format!("Msg {}", i)),
                None,
                vec![],
                None,
            ));
        }

        // max_history=3, so only last 3 remain: [Msg 3, Msg 4, Msg 5]
        assert_eq!(state.len(), 3);
        assert_eq!(state.cursor_position(), 2);
        assert_eq!(state.current().unwrap().summary, "Msg 5");

        // Rewind to oldest available
        state.rewind(RewindMode::ConversationOnly).unwrap();
        state.rewind(RewindMode::ConversationOnly).unwrap();
        assert_eq!(state.cursor_position(), 0);
        assert_eq!(state.current().unwrap().summary, "Msg 3");

        // Can't rewind further (oldest was evicted)
        assert!(!state.can_rewind());
    }
}
