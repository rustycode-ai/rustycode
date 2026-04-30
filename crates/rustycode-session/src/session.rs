//! Session management for conversations
//!
//! This module provides session management with:
//! - Message tracking
//! - Metadata management
//! - Token counting
//! - Session status tracking

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::compaction::CompactionSnapshot;
use crate::message::Message;

/// Unique session identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new session ID
    pub fn new() -> Self {
        Self(format!("sess_{}", nanoid::nanoid!(10)))
    }

    /// Get the inner ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse from string
    ///
    /// Rejects strings containing path traversal characters and allows only safe characters.
    /// Session IDs must start with "sess_" and contain only alphanumeric characters, hyphens, and underscores.
    pub fn parse(s: impl Into<String>) -> Option<Self> {
        let s = s.into();
        if s.starts_with("sess_") &&
           s.len() >= 6 && // "sess_" + at least 1 character (e.g., "sess_a")
           s.len() <= 64 && // reasonable maximum length
           s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') &&
           !s.contains('/') && !s.contains('\\') && !s.contains("..")
        {
            // path traversal protection
            Some(Self(s))
        } else {
            None
        }
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionStatus {
    Active,
    Archived,
    Deleted,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Project path (if applicable)
    pub project_path: Option<PathBuf>,

    /// Git branch (if applicable)
    pub git_branch: Option<String>,

    /// Model used in this session
    pub model_used: Option<String>,

    /// Provider used in this session
    pub provider_used: Option<String>,

    /// Total tokens across all messages
    pub total_tokens: usize,

    /// Total cost in USD
    pub total_cost: f64,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Custom metadata
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub custom: std::collections::HashMap<String, String>,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            project_path: None,
            git_branch: None,
            model_used: None,
            provider_used: None,
            total_tokens: 0,
            total_cost: 0.0,
            tags: Vec::new(),
            custom: Default::default(),
        }
    }
}

/// Session context for tracking state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionContext {
    /// Current task being worked on
    pub task: Option<String>,

    /// Files that have been created or modified
    pub files_touched: Vec<String>,

    /// Key decisions made
    pub decisions: Vec<String>,

    /// Errors encountered and their resolutions
    pub errors_resolved: Vec<String>,

    /// Current step/phase
    pub current_phase: Option<String>,
}

/// Session for managing conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub id: SessionId,

    /// Session name
    pub name: String,

    /// Messages in the session
    pub messages: Vec<Message>,

    /// Session metadata
    pub metadata: SessionMetadata,

    /// Session context
    pub context: SessionContext,

    /// Session status
    pub status: SessionStatus,

    /// Creation timestamp
    pub created_at: SystemTime,

    /// Last update timestamp
    pub updated_at: SystemTime,
}

impl Session {
    /// Create a new session
    pub fn new(name: impl Into<String>) -> Self {
        let now = SystemTime::now();
        Self {
            id: SessionId::new(),
            name: name.into(),
            messages: Vec::new(),
            metadata: SessionMetadata::default(),
            context: SessionContext::default(),
            status: SessionStatus::Active,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a message to the session
    pub fn add_message(&mut self, message: Message) {
        // Update metadata
        if let Some(tokens) = message.metadata.tokens {
            self.metadata.total_tokens = self.metadata.total_tokens.saturating_add(tokens);
        }
        if let Some(cost) = message.metadata.cost {
            self.metadata.total_cost += cost;
        }
        if let Some(ref model) = message.metadata.model {
            if self.metadata.model_used.is_none() {
                self.metadata.model_used = Some(model.clone());
            }
        }
        if let Some(ref provider) = message.metadata.provider {
            if self.metadata.provider_used.is_none() {
                self.metadata.provider_used = Some(provider.clone());
            }
        }

        self.messages.push(message);
        self.updated_at = SystemTime::now();
    }

    /// Get message count
    pub const fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Check if session is empty
    pub const fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get total token count
    pub const fn token_count(&self) -> usize {
        self.metadata.total_tokens
    }

    /// Estimate token count (if actual count not available)
    pub fn estimate_tokens(&self) -> usize {
        if self.metadata.total_tokens > 0 {
            self.metadata.total_tokens
        } else {
            self.messages.iter().map(Message::estimate_tokens).sum()
        }
    }

    /// Get the last message, if any
    pub fn last_message(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Get all user messages
    pub fn user_messages(&self) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.role == crate::message::MessageRole::User)
            .collect()
    }

    /// Get all assistant messages
    pub fn assistant_messages(&self) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.role == crate::message::MessageRole::Assistant)
            .collect()
    }

    /// Get all tool messages
    pub fn tool_messages(&self) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.role == crate::message::MessageRole::Tool)
            .collect()
    }

    /// Record a file touch in context
    pub fn touch_file(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !self.context.files_touched.contains(&path) {
            self.context.files_touched.push(path);
        }
    }

    /// Record a decision in context
    pub fn record_decision(&mut self, decision: impl Into<String>) {
        self.context.decisions.push(decision.into());
    }

    /// Record an error resolution in context
    pub fn record_error_resolution(
        &mut self,
        error: impl Into<String>,
        resolution: impl Into<String>,
    ) {
        self.context
            .errors_resolved
            .push(format!("{} → {}", error.into(), resolution.into()));
    }

    /// Set the current task
    pub fn set_task(&mut self, task: impl Into<String>) {
        self.context.task = Some(task.into());
    }

    /// Set the current phase
    pub fn set_phase(&mut self, phase: impl Into<String>) {
        self.context.current_phase = Some(phase.into());
    }

    /// Add a tag to metadata
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.metadata.tags.contains(&tag) {
            self.metadata.tags.push(tag);
        }
    }

    /// Archive the session
    pub const fn archive(&mut self) {
        self.status = SessionStatus::Archived;
    }

    /// Delete the session
    pub const fn delete(&mut self) {
        self.status = SessionStatus::Deleted;
    }

    /// Clone session for branching (creates new ID)
    pub fn fork(&self) -> Self {
        Self {
            id: SessionId::new(),
            name: format!("{} (fork)", self.name),
            messages: self.messages.clone(),
            metadata: self.metadata.clone(),
            context: self.context.clone(),
            status: SessionStatus::Active,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
        }
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
        self.metadata.total_tokens = 0;
        self.metadata.total_cost = 0.0;
        self.updated_at = SystemTime::now();
    }

    /// Capture a pre-compaction snapshot of in-flight state
    pub fn pre_compact(&self) -> anyhow::Result<CompactionSnapshot> {
        // Build a snapshot from current session state
        let session_id = self.id.as_str().to_string();
        let snapshot_at = chrono::Utc::now();

        // Current in-flight task
        let current_task = self.context.task.clone();

        // Active file edits (simplified view: map touched paths to a 'touched' description)
        let mut active_files: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for f in &self.context.files_touched {
            active_files.insert(f.clone(), "touched".to_string());
        }

        // Summary of pending changes (accumulate decisions as a simple summary)
        let pending_changes_summary = if self.context.decisions.is_empty() {
            None
        } else {
            Some(self.context.decisions.join("; "))
        };

        // Pending tool call (best-effort: inspect last tool call in messages)
        let mut pending_tool_call: Option<String> = None;
        for m in &self.messages {
            if m.has_tool_calls() {
                if let Some(part) = m
                    .parts
                    .iter()
                    .find(|p| matches!(p, crate::message::MessagePart::ToolCall { .. }))
                {
                    if let crate::message::MessagePart::ToolCall { id, .. } = part {
                        pending_tool_call = Some(id.clone());
                    }
                    break;
                }
            }
        }

        // Token count snapshot (best-effort: use estimate)
        let token_count = self.estimate_tokens();

        // Custom state snapshot (copy custom map from metadata)
        let custom_state = self.metadata.custom.clone();

        Ok(CompactionSnapshot {
            session_id,
            snapshot_at,
            current_task,
            active_files,
            pending_changes_summary,
            pending_tool_call,
            token_count,
            custom_state,
        })
    }

    /// Restore a pre-compaction state into the session after compaction
    pub fn post_compact(&mut self, snapshot: &CompactionSnapshot) -> anyhow::Result<()> {
        // Restore task if present
        if let Some(task) = &snapshot.current_task {
            self.context.task = Some(task.clone());
        }

        // Restore a summary message to inform the LLM about restoration
        let mut restoration = String::from("[Pre-compact snapshot restored]");
        if !snapshot.active_files.is_empty() {
            let files = snapshot
                .active_files
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            restoration.push_str(&format!(" Active files: [{files}]."));
        }
        restoration.push_str(&format!(" Tokens snapshot: {}.", snapshot.token_count));

        self.add_message(Message::system(restoration));

        // Optionally merge any custom state back into metadata.custom (best-effort)
        for (k, v) in &snapshot.custom_state {
            self.metadata
                .custom
                .entry(k.clone())
                .or_insert_with(|| v.clone());
        }
        Ok(())
    }

    /// Get age of session in seconds
    pub fn age_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(self.created_at)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new("Test Session");
        assert_eq!(session.name, "Test Session");
        assert!(session.is_empty());
        assert_eq!(session.message_count(), 0);
        assert!(session.status == SessionStatus::Active);
    }

    #[test]
    fn test_add_message() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hello"));

        assert_eq!(session.message_count(), 1);
        assert!(!session.is_empty());
    }

    #[test]
    fn test_token_tracking() {
        let mut session = Session::new("Test");

        let msg = Message::user("Hello").with_tokens(10).with_cost(0.001);

        session.add_message(msg);

        assert_eq!(session.token_count(), 10);
        assert_eq!(session.metadata.total_cost, 0.001);
    }

    #[test]
    fn test_context_tracking() {
        let mut session = Session::new("Test");

        session.set_task("Implement feature");
        session.touch_file("src/main.rs");
        session.record_decision("Use async pattern");

        assert_eq!(session.context.task, Some("Implement feature".to_string()));
        assert!(session
            .context
            .files_touched
            .contains(&"src/main.rs".to_string()));
        assert_eq!(session.context.decisions.len(), 1);
    }

    #[test]
    fn test_session_fork() {
        let mut session = Session::new("Original");
        session.add_message(Message::user("Hello"));

        let forked = session.fork();

        assert_ne!(session.id, forked.id);
        assert_eq!(forked.message_count(), 1);
        assert!(forked.name.contains("fork"));
    }

    #[test]
    fn test_message_filters() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("User msg"));
        session.add_message(Message::assistant("Assistant msg"));
        session.add_message(Message::tool_result("call_123", "Result", false));

        assert_eq!(session.user_messages().len(), 1);
        assert_eq!(session.assistant_messages().len(), 1);
        assert_eq!(session.tool_messages().len(), 1);
    }

    #[test]
    fn test_session_status() {
        let mut session = Session::new("Test");
        assert_eq!(session.status, SessionStatus::Active);

        session.archive();
        assert_eq!(session.status, SessionStatus::Archived);

        session.delete();
        assert_eq!(session.status, SessionStatus::Deleted);
    }

    #[test]
    fn test_clear_session() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hello").with_tokens(10));
        session.add_message(Message::assistant("Hi").with_tokens(5));

        assert_eq!(session.message_count(), 2);
        assert_eq!(session.token_count(), 15);

        session.clear();

        assert_eq!(session.message_count(), 0);
        assert_eq!(session.token_count(), 0);
    }

    #[test]
    fn test_tags() {
        let mut session = Session::new("Test");
        session.add_tag("important");
        session.add_tag("bug-fix");
        session.add_tag("important"); // Duplicate

        assert_eq!(session.metadata.tags.len(), 2);
        assert!(session.metadata.tags.contains(&"important".to_string()));
    }

    // --- SessionId tests ---

    #[test]
    fn test_session_id_new_format() {
        let id = SessionId::new();
        let s = id.as_str();
        assert!(s.starts_with("sess_"));
        assert!(s.len() > 5); // "sess_" + nanoid
    }

    #[test]
    fn test_session_id_parse_valid() {
        let id = SessionId::parse("sess_abc123xyz").unwrap();
        assert_eq!(id.as_str(), "sess_abc123xyz");
    }

    #[test]
    fn test_session_id_parse_invalid() {
        assert!(SessionId::parse("invalid").is_none());
        assert!(SessionId::parse("session_123").is_none());
        assert!(SessionId::parse("").is_none());
    }

    #[test]
    fn test_session_id_parse_rejects_path_traversal() {
        assert!(SessionId::parse("sess_../../etc/passwd").is_none());
        assert!(SessionId::parse("sess_foo\\bar").is_none());
        assert!(SessionId::parse("sess_..").is_none());
        assert!(SessionId::parse("sess_a/b").is_none());
    }

    #[test]
    fn test_session_id_display() {
        let id = SessionId::new();
        let display = format!("{}", id);
        assert!(display.starts_with("sess_"));
    }

    #[test]
    fn test_session_id_default() {
        let id = SessionId::default();
        assert!(id.as_str().starts_with("sess_"));
    }

    #[test]
    fn test_session_id_equality() {
        let id1 = SessionId::parse("sess_same").unwrap();
        let id2 = SessionId::parse("sess_same").unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_session_id_inequality() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_session_id_serde_roundtrip() {
        let id = SessionId::new();
        let json = serde_json::to_string(&id).unwrap();
        let de: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, de);
    }

    #[test]
    fn test_session_id_hash_in_hashmap() {
        use std::collections::HashSet;
        let id1 = SessionId::parse("sess_a").unwrap();
        let id2 = SessionId::parse("sess_a").unwrap();
        let id3 = SessionId::parse("sess_b").unwrap();
        let mut set = HashSet::new();
        set.insert(id1.clone());
        set.insert(id3.clone());
        assert!(set.contains(&id2));
        assert_eq!(set.len(), 2);
    }

    // --- SessionStatus tests ---

    #[test]
    fn test_session_status_serde_roundtrip() {
        for status in [
            SessionStatus::Active,
            SessionStatus::Archived,
            SessionStatus::Deleted,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let de: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, de);
        }
    }

    #[test]
    fn test_session_status_equality() {
        assert_eq!(SessionStatus::Active, SessionStatus::Active);
        assert_ne!(SessionStatus::Active, SessionStatus::Archived);
        assert_ne!(SessionStatus::Archived, SessionStatus::Deleted);
    }

    // --- SessionMetadata tests ---

    #[test]
    fn test_session_metadata_default() {
        let meta = SessionMetadata::default();
        assert!(meta.project_path.is_none());
        assert!(meta.git_branch.is_none());
        assert!(meta.model_used.is_none());
        assert!(meta.provider_used.is_none());
        assert_eq!(meta.total_tokens, 0);
        assert_eq!(meta.total_cost, 0.0);
        assert!(meta.tags.is_empty());
        assert!(meta.custom.is_empty());
    }

    #[test]
    fn test_session_metadata_serde_roundtrip() {
        let meta = SessionMetadata::default();
        let json = serde_json::to_string(&meta).unwrap();
        let de: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(de.total_tokens, 0);
        assert_eq!(de.total_cost, 0.0);
    }

    #[test]
    fn test_session_metadata_custom_skipped_when_empty() {
        let meta = SessionMetadata::default();
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("custom"));
    }

    #[test]
    fn test_session_metadata_custom_present_when_nonempty() {
        let mut meta = SessionMetadata::default();
        meta.custom.insert("k".to_string(), "v".to_string());
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("custom"));
        assert!(json.contains("\"k\""));
        assert!(json.contains("\"v\""));
    }

    #[test]
    fn test_session_metadata_with_all_fields() {
        let mut meta = SessionMetadata {
            project_path: Some(PathBuf::from("/tmp/project")),
            git_branch: Some("main".to_string()),
            model_used: Some("gpt-4".to_string()),
            provider_used: Some("openai".to_string()),
            total_tokens: 100,
            total_cost: 0.5,
            ..Default::default()
        };
        meta.tags.push("test".to_string());
        meta.custom.insert("env".to_string(), "dev".to_string());

        let json = serde_json::to_string(&meta).unwrap();
        let de: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(de.project_path, Some(PathBuf::from("/tmp/project")));
        assert_eq!(de.git_branch, Some("main".to_string()));
        assert_eq!(de.model_used, Some("gpt-4".to_string()));
        assert_eq!(de.provider_used, Some("openai".to_string()));
        assert_eq!(de.total_tokens, 100);
        assert_eq!(de.total_cost, 0.5);
        assert_eq!(de.tags, vec!["test".to_string()]);
        assert_eq!(de.custom.get("env"), Some(&"dev".to_string()));
    }

    // --- SessionContext tests ---

    #[test]
    fn test_session_context_default() {
        let ctx = SessionContext::default();
        assert!(ctx.task.is_none());
        assert!(ctx.files_touched.is_empty());
        assert!(ctx.decisions.is_empty());
        assert!(ctx.errors_resolved.is_empty());
        assert!(ctx.current_phase.is_none());
    }

    #[test]
    fn test_session_context_serde_roundtrip() {
        let ctx = SessionContext {
            task: Some("Build feature".to_string()),
            files_touched: vec!["src/main.rs".to_string()],
            decisions: vec!["Use async".to_string()],
            errors_resolved: vec!["fix → resolved".to_string()],
            current_phase: Some("testing".to_string()),
        };

        let json = serde_json::to_string(&ctx).unwrap();
        let de: SessionContext = serde_json::from_str(&json).unwrap();
        assert_eq!(de.task, Some("Build feature".to_string()));
        assert_eq!(de.files_touched, vec!["src/main.rs".to_string()]);
        assert_eq!(de.decisions, vec!["Use async".to_string()]);
        assert_eq!(de.errors_resolved, vec!["fix → resolved".to_string()]);
        assert_eq!(de.current_phase, Some("testing".to_string()));
    }

    // --- Session serde roundtrip ---

    #[test]
    fn test_session_serde_roundtrip() {
        let mut session = Session::new("Serde Test");
        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi"));
        session.set_task("Testing");
        session.add_tag("serde");

        let json = serde_json::to_string(&session).unwrap();
        let de: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(de.name, "Serde Test");
        assert_eq!(de.message_count(), 2);
        assert_eq!(de.status, SessionStatus::Active);
        assert_eq!(de.context.task, Some("Testing".to_string()));
        assert!(de.metadata.tags.contains(&"serde".to_string()));
    }

    // --- Session method tests ---

    #[test]
    fn test_session_last_message_empty() {
        let session = Session::new("Empty");
        assert!(session.last_message().is_none());
    }

    #[test]
    fn test_session_last_message_present() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("First"));
        session.add_message(Message::assistant("Second"));
        let last = session.last_message().unwrap();
        assert_eq!(last.role, crate::message::MessageRole::Assistant);
    }

    #[test]
    fn test_session_model_used_set_once() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hi").with_model("model-a"));
        session.add_message(Message::assistant("Hey").with_model("model-b"));
        // model_used should only be set from the first message with a model
        assert_eq!(session.metadata.model_used, Some("model-a".to_string()));
    }

    #[test]
    fn test_session_provider_used_set_once() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hi").with_provider("provider-a"));
        session.add_message(Message::assistant("Hey").with_provider("provider-b"));
        assert_eq!(
            session.metadata.provider_used,
            Some("provider-a".to_string())
        );
    }

    #[test]
    fn test_session_no_model_used_when_none() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hi"));
        session.add_message(Message::assistant("Hey"));
        assert!(session.metadata.model_used.is_none());
    }

    #[test]
    fn test_session_no_provider_used_when_none() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hi"));
        session.add_message(Message::assistant("Hey"));
        assert!(session.metadata.provider_used.is_none());
    }

    #[test]
    fn test_session_token_accumulation() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("A").with_tokens(10));
        session.add_message(Message::assistant("B").with_tokens(20));
        session.add_message(Message::user("C").with_tokens(30));
        assert_eq!(session.token_count(), 60);
    }

    #[test]
    fn test_pre_compact_and_post_compact_roundtrip() {
        let mut session = Session::new("TestPreserve");
        session.add_message(Message::user("Hello"));
        session.context.task = Some("Do something".to_string());
        session.touch_file("src/main.rs");

        // Pre-compact snapshot
        let snapshot = session.pre_compact().unwrap();
        assert!(!snapshot.session_id.is_empty());
        // Post-compact should insert a system restoration message
        session.post_compact(&snapshot).unwrap();
        assert!(session.message_count() >= 2);
    }

    #[test]
    fn test_session_cost_accumulation() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("A").with_cost(0.01));
        session.add_message(Message::assistant("B").with_cost(0.02));
        let total: f64 = session.metadata.total_cost;
        assert!((total - 0.03).abs() < f64::EPSILON);
    }

    #[test]
    fn test_session_message_no_tokens_or_cost() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hello"));
        assert_eq!(session.token_count(), 0);
        assert_eq!(session.metadata.total_cost, 0.0);
    }

    #[test]
    fn test_session_estimate_tokens_with_metadata() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hello").with_tokens(100));
        assert_eq!(session.estimate_tokens(), 100);
    }

    #[test]
    fn test_session_estimate_tokens_without_metadata() {
        let mut session = Session::new("Test");
        // No tokens metadata, estimate from content
        session.add_message(Message::user("Hello, world! This is a test."));
        let estimated = session.estimate_tokens();
        assert!(estimated > 0);
    }

    #[test]
    fn test_touch_file_deduplication() {
        let mut session = Session::new("Test");
        session.touch_file("src/main.rs");
        session.touch_file("src/main.rs");
        session.touch_file("src/lib.rs");
        assert_eq!(session.context.files_touched.len(), 2);
    }

    #[test]
    fn test_record_error_resolution() {
        let mut session = Session::new("Test");
        session.record_error_resolution("compile error", "added missing import");
        session.record_error_resolution("linker error", "added dependency");
        assert_eq!(session.context.errors_resolved.len(), 2);
        assert!(session.context.errors_resolved[0].contains("compile error"));
        assert!(session.context.errors_resolved[0].contains("added missing import"));
        assert!(session.context.errors_resolved[1].contains("linker error → added dependency"));
    }

    #[test]
    fn test_set_phase() {
        let mut session = Session::new("Test");
        session.set_phase("development");
        assert_eq!(
            session.context.current_phase,
            Some("development".to_string())
        );
    }

    #[test]
    fn test_archive_and_delete_status() {
        let mut session = Session::new("Test");
        assert_eq!(session.status, SessionStatus::Active);
        session.archive();
        assert_eq!(session.status, SessionStatus::Archived);
        session.delete();
        assert_eq!(session.status, SessionStatus::Deleted);
    }

    #[test]
    fn test_fork_preserves_messages() {
        let mut session = Session::new("Original");
        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi"));
        session.add_tag("test-tag");

        let forked = session.fork();
        assert_ne!(session.id, forked.id);
        assert_eq!(forked.message_count(), 2);
        assert!(forked.name.contains("fork"));
        assert_eq!(forked.status, SessionStatus::Active);
        assert!(forked.metadata.tags.contains(&"test-tag".to_string()));
    }

    #[test]
    fn test_clear_resets_tokens_and_cost() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("Hi").with_tokens(100).with_cost(0.5));
        assert_eq!(session.token_count(), 100);
        session.clear();
        assert_eq!(session.message_count(), 0);
        assert_eq!(session.token_count(), 0);
        assert_eq!(session.metadata.total_cost, 0.0);
    }

    #[test]
    fn test_age_seconds() {
        let session = Session::new("Test");
        let age = session.age_seconds();
        // Should be essentially 0 since just created
        assert!(age < 5);
    }

    #[test]
    fn test_user_messages_empty() {
        let session = Session::new("Empty");
        assert!(session.user_messages().is_empty());
    }

    #[test]
    fn test_assistant_messages_empty() {
        let session = Session::new("Empty");
        assert!(session.assistant_messages().is_empty());
    }

    #[test]
    fn test_tool_messages_empty() {
        let session = Session::new("Empty");
        assert!(session.tool_messages().is_empty());
    }

    #[test]
    fn test_message_filters_mixed() {
        let mut session = Session::new("Test");
        session.add_message(Message::user("u1"));
        session.add_message(Message::assistant("a1"));
        session.add_message(Message::user("u2"));
        session.add_message(Message::tool_result("c1", "t1", false));
        session.add_message(Message::assistant("a2"));

        assert_eq!(session.user_messages().len(), 2);
        assert_eq!(session.assistant_messages().len(), 2);
        assert_eq!(session.tool_messages().len(), 1);
    }
}
