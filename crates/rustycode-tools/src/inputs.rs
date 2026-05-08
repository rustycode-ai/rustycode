//! Shared input parameter types for tools
//!
//! This module consolidates all tool input types in one place,
//! eliminating duplication across provider implementations.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ReadFile Input

/// Input parameters for `ReadFile`
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ReadFileInput {
    /// File path to read
    #[serde(alias = "file_path")]
    pub path: PathBuf,
    /// Optional start line (1-indexed, inclusive)
    #[serde(alias = "offset", skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    /// Optional end line (1-indexed, inclusive)
    #[serde(alias = "limit", skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
}

// WriteFile Input

/// Input parameters for `WriteFile`
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct WriteFileInput {
    /// File path to write
    #[serde(alias = "file_path")]
    pub path: PathBuf,
    /// Content to write
    pub content: String,
    /// Create parent directories if they don't exist
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_parents: Option<bool>,
}

// Bash Input

/// Input parameters for Bash
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BashInput {
    /// Command to execute
    pub command: String,
    /// Optional arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Optional working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    /// Optional timeout in seconds (default: 30)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

// Grep Input

/// Input parameters for Grep
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GrepInput {
    /// Regex pattern to search for
    pub pattern: String,
    /// Root directory to search (default: current directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Maximum search depth (default: 4)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    /// Case-insensitive search
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_insensitive: Option<bool>,
}

// Glob Input

/// Input parameters for Glob
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GlobInput {
    /// Glob pattern (supports *, **, ? wildcards)
    pub pattern: String,
    /// Root directory to search (default: current directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Maximum search depth (default: 5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    /// Case-insensitive matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_insensitive: Option<bool>,
}
