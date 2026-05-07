//! Git hook types and trait definitions.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::GitOperation;

/// Git hook types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GitHookType {
    PreCommit,
    PrePush,
    PreRebase,
    CommitMsg,
    PostCommit,
    PostMerge,
    PostCheckout,
    PreMerge,
}

impl std::fmt::Display for GitHookType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreCommit => write!(f, "pre-commit"),
            Self::PrePush => write!(f, "pre-push"),
            Self::PreRebase => write!(f, "pre-rebase"),
            Self::CommitMsg => write!(f, "commit-msg"),
            Self::PostCommit => write!(f, "post-commit"),
            Self::PostMerge => write!(f, "post-merge"),
            Self::PostCheckout => write!(f, "post-checkout"),
            Self::PreMerge => write!(f, "pre-merge"),
        }
    }
}

/// Result of hook execution
#[derive(Debug, Clone)]
pub struct HookResult {
    /// Whether the hook passed
    pub passed: bool,
    /// Output from the hook
    pub output: String,
    /// Error message if hook failed
    pub error: Option<String>,
}

/// Git hook for custom workflows
pub trait GitHook: Send + Sync {
    /// Execute the hook
    fn execute(&self, context: &HookContext) -> Result<HookResult>;

    /// Get the hook type
    fn hook_type(&self) -> GitHookType;
}

/// Context provided to hooks
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Repository root path
    pub repository_root: PathBuf,
    pub current_branch: Option<String>,
    /// Operation being executed
    pub operation: GitOperation,
    /// Files affected by the operation
    pub affected_files: Vec<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
}

/// Alias to disambiguate from protocol's `HookResult`.
pub type GitHookResult = HookResult;

/// Alias to disambiguate from protocol's `HookContext`.
pub type GitHookContext = HookContext;
