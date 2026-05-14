//! File-based context types for cross-agent communication.

use serde::{Deserialize, Serialize};

/// Represents a change to a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// The absolute path to the file.
    pub file_path: String,
    /// The type of change (Added, Modified, Deleted).
    pub change_type: ChangeType,
    /// Contextual snippet before/after the change.
    pub snippet: Option<FileSnippet>,
}

/// Represents the content around a specific change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnippet {
    /// The starting line of the snippet.
    pub start_line: usize,
    /// The content of the snippet.
    pub content: String,
}

/// The type of file change.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
}
