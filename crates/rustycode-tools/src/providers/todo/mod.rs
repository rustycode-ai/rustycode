use dashmap::DashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// Re-export all tools
pub use update::*;
pub use write::*;

pub mod update;
pub mod write;

// ============================================================================
// Global state management
// ============================================================================

/// Global todo state storage, keyed by session ID
pub(crate) static GLOBAL_TODO_STATES: std::sync::LazyLock<DashMap<String, TodoState>> =
    std::sync::LazyLock::new(DashMap::new);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
}

pub type TodoState = Arc<Mutex<Vec<TodoItem>>>;

pub fn new_todo_state() -> TodoState {
    Arc::new(Mutex::new(Vec::new()))
}

/// Retrieve or create TodoState for a given session.
pub(crate) fn get_or_create_todo_state(session_id: &str) -> TodoState {
    if let Some(entry) = GLOBAL_TODO_STATES.get(session_id) {
        entry.clone()
    } else {
        let state = new_todo_state();
        let _ = GLOBAL_TODO_STATES.insert(session_id.to_string(), state.clone());
        state
    }
}

pub(crate) fn format_status(status: TodoStatus) -> String {
    match status {
        TodoStatus::Pending => "pending".to_string(),
        TodoStatus::InProgress => "in progress".to_string(),
        TodoStatus::Completed => "completed".to_string(),
    }
}

// ============================================================================
// Params structs
// ============================================================================

/// A single todo item input
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TodoItemInput {
    /// Unique identifier for this todo
    pub id: String,
    /// Task description
    pub title: String,
    /// Current status: pending, in_progress, or completed
    pub status: String,
    /// Present continuous form for spinner
    #[serde(rename = "activeForm")]
    pub active_form: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoWriteParams {
    /// Title for this todo list
    pub title: String,
    /// Array of todo items
    pub todos: Vec<TodoItemInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoUpdateParams {
    /// Todo item identifier
    pub id: String,
    /// New status: pending, in_progress, or completed
    pub status: Option<String>,
    /// Updated task description
    pub title: Option<String>,
    /// Updated present continuous form for spinner
    #[serde(rename = "activeForm")]
    pub active_form: Option<String>,
}
