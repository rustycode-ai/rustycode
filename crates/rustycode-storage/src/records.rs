//! Record types for the storage layer.
//!
//! Pure data structs used to map database rows to Rust types.

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
