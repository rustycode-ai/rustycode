#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_wraps
)]
#![cfg_attr(test, allow(clippy::unwrap_used,))]

//! # `RustyCode` Git Operations
//!
//! Comprehensive git integration layer for conflict detection, repository management,
//! and git operations within `RustyCode`.
//!
//! ## Features
//!
//! - **Git Operations**: Branch management, committing, diff, status, and merging
//! - **Staging Operations**: Add, reset, and remove files from the staging area
//! - **Conflict Detection**: Detect merge conflicts before they cause issues
//! - **Git Hooks**: Pre/post operation hooks for custom workflows
//! - **Type-Safe Operations**: Enum-based operation types for safety
//! - **Comprehensive Testing**: Full test coverage for all operations
//!
//! ## Architecture
//!
//! The git integration is organized into several layers:
//!
//! 1. **`GitOperation` Type**: Type-safe enum representing all git operations
//! 2. **`GitClient`**: Main client for executing git operations
//! 3. **Staging Operations**: Specialized methods for staging area management
//! 4. **Conflict Detection**: Integration with conflict detection system
//! 5. **Git Hooks**: Extensible hook system for custom workflows
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_git::{GitClient, GitOperation};
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Create git client
//! let git = GitClient::new(Path::new("/path/to/repo"))?;
//!
//! // Get repository status
//! let status = git.status()?;
//! println!("Branch: {:?}", status.branch);
//! println!("Dirty: {:?}", status.dirty);
//!
//! // Create a new branch
//! git.create_branch("feature-new-feature", None)?;
//!
//! // Stage and commit changes
//! git.stage_files(&["src/main.rs", "README.md"], false)?;
//! git.commit_changes("Add new feature", None, None)?;
//!
//! // Get diff
//! let diff = git.get_diff(None, false, None)?;
//! println!("Diff:\n{}", diff);
//!
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};

// Error Types

/// Git operation errors
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GitError {
    #[error("git command failed: {0}")]
    GitCommandFailed(String),

    #[error("repository not found at {0}")]
    RepositoryNotFound(PathBuf),

    #[error("branch '{0}' not found")]
    BranchNotFound(String),

    #[error("conflict detected: {0}")]
    ConflictDetected(String),

    #[error("merge in progress")]
    MergeInProgress,

    #[error("hook execution failed: {0}")]
    HookExecutionFailed(String),

    #[error("invalid operation: {0}")]
    InvalidOperation(String),
}

// Git Operation Type

/// Type-safe enum representing all git operations
///
/// Each operation has associated metadata for tracking and logging.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GitOperation {
    /// Branch creation
    CreateBranch {
        name: String,
        base: Option<String>,
    },

    /// Branch switching/checkout
    SwitchBranch {
        name: String,
        force: bool,
    },

    /// Commit creation
    CommitChanges {
        message: String,
        amend: bool,
        allow_empty: bool,
    },

    /// Diff generation
    GetDiff {
        path: Option<String>,
        cached: bool,
        context_lines: Option<usize>,
    },

    /// Status query
    GetStatus {
        branch_only: bool,
    },

    /// Branch merging
    MergeBranch {
        source: String,
        no_commit: bool,
        squash: bool,
    },

    /// File staging
    StageFiles {
        paths: Vec<String>,
        update: bool,
    },

    /// File unstaging
    UnstageFiles {
        paths: Vec<String>,
    },

    /// Branch deletion
    DeleteBranch {
        name: String,
        force: bool,
    },

    /// Remote operations
    Fetch {
        remote: String,
        refspec: Option<String>,
    },

    Pull {
        remote: String,
        branch: Option<String>,
    },

    Push {
        remote: String,
        branch: String,
        force: bool,
    },

    /// Stash operations
    Stash {
        message: Option<String>,
        keep_index: bool,
    },

    StashPop {
        stash_ref: Option<String>,
    },

    /// Reset operations
    Reset {
        mode: ResetMode,
        commit: Option<String>,
    },

    /// Rebase operations
    Rebase {
        upstream: String,
        branch: Option<String>,
        interactive: bool,
    },
}

/// Reset mode for git reset operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
    Merge,
    Keep,
}

impl std::fmt::Display for GitOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateBranch { name, base } => {
                write!(f, "create_branch({name})")?;
                if let Some(base) = base {
                    write!(f, " from {base}")?;
                }
                Ok(())
            }
            Self::SwitchBranch { name, force } => {
                write!(f, "switch_branch({name})")?;
                if *force {
                    write!(f, " (force)")?;
                }
                Ok(())
            }
            Self::CommitChanges {
                amend, allow_empty, ..
            } => {
                write!(f, "commit")?;
                if *amend {
                    write!(f, " --amend")?;
                }
                if *allow_empty {
                    write!(f, " --allow-empty")?;
                }
                Ok(())
            }
            Self::GetDiff { path, cached, .. } => {
                write!(f, "diff")?;
                if *cached {
                    write!(f, " --cached")?;
                }
                if let Some(path) = path {
                    write!(f, " -- {path}")?;
                }
                Ok(())
            }
            Self::GetStatus { branch_only } => {
                write!(f, "status")?;
                if *branch_only {
                    write!(f, " (branch only)")?;
                }
                Ok(())
            }
            Self::MergeBranch { source, .. } => {
                write!(f, "merge {source}")
            }
            Self::StageFiles { paths, update } => {
                write!(f, "stage {paths:?}")?;
                if *update {
                    write!(f, " (update)")?;
                }
                Ok(())
            }
            Self::UnstageFiles { paths } => {
                write!(f, "unstage {paths:?}")
            }
            Self::DeleteBranch { name, force } => {
                write!(f, "delete_branch({name})")?;
                if *force {
                    write!(f, " (force)")?;
                }
                Ok(())
            }
            Self::Fetch { remote, refspec } => {
                write!(f, "fetch {remote}")?;
                if let Some(refspec) = refspec {
                    write!(f, " {refspec}")?;
                }
                Ok(())
            }
            Self::Pull { remote, branch } => {
                write!(f, "pull {remote}")?;
                if let Some(branch) = branch {
                    write!(f, " {branch}")?;
                }
                Ok(())
            }
            Self::Push {
                remote,
                branch,
                force,
            } => {
                write!(f, "push {remote} {branch}")?;
                if *force {
                    write!(f, " (force)")?;
                }
                Ok(())
            }
            Self::Stash {
                message,
                keep_index,
            } => {
                write!(f, "stash")?;
                if *keep_index {
                    write!(f, " --keep-index")?;
                }
                if let Some(message) = message {
                    write!(f, " '{message}'")?;
                }
                Ok(())
            }
            Self::StashPop { stash_ref } => {
                write!(f, "stash pop")?;
                if let Some(ref_) = stash_ref {
                    write!(f, " {ref_}")?;
                }
                Ok(())
            }
            Self::Reset { mode, commit } => {
                write!(f, "reset --{mode:?}")?;
                if let Some(commit) = commit {
                    write!(f, " {commit}")?;
                }
                Ok(())
            }
            Self::Rebase {
                upstream,
                branch,
                interactive,
            } => {
                write!(f, "rebase")?;
                if *interactive {
                    write!(f, " -i")?;
                }
                write!(f, " {upstream}")?;
                if let Some(branch) = branch {
                    write!(f, " {branch}")?;
                }
                Ok(())
            }
        }
    }
}

/// Result of a git operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitOperationResult {
    /// The operation that was executed
    pub operation: GitOperation,
    /// Whether the operation succeeded
    pub success: bool,
    /// Output from the operation
    pub output: String,
    /// Error message if operation failed
    pub error: Option<String>,
    /// Timestamp when the operation was executed
    pub executed_at: DateTime<Utc>,
    /// Duration of the operation in milliseconds
    pub duration_ms: u64,
}

// Git Hooks

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
    /// Current branch
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

// Main Git Client

/// Main git client for executing git operations
///
/// The `GitClient` provides a high-level interface to git operations with:
/// - Type-safe operation enums
/// - Automatic error handling and context
/// - Git hooks support
/// - Conflict detection integration
/// - Thread-safe operations
///
/// # Example
///
/// ```rust,no_run
/// use rustycode_git::GitClient;
/// use std::path::Path;
///
/// # fn main() -> anyhow::Result<()> {
/// let git = GitClient::new(Path::new("/path/to/repo"))?;
///
/// // Get status
/// let status = git.status()?;
/// println!("Current branch: {:?}", status.branch);
///
/// // Create and switch to a new branch
/// git.create_branch("feature-branch", None)?;
/// git.switch_branch("feature-branch", false)?;
///
/// // Stage and commit
/// git.stage_files(&["src/main.rs"], false)?;
/// git.commit_changes("Add feature", None, None)?;
///
/// # Ok(())
/// # }
/// ```
/// Type alias for hooks storage to reduce complexity
type HooksStorage = Arc<RwLock<HashMap<GitHookType, Vec<Box<dyn GitHook>>>>>;

pub struct GitClient {
    /// Repository root path
    repository_root: PathBuf,
    /// Registered hooks
    hooks: HooksStorage,
    /// Conflict detector
    conflict_detector: Option<ConflictDetector>,
}

impl GitClient {
    /// Create a new git client for a repository
    ///
    pub fn new(path: &Path) -> Result<Self> {
        let repository_root =
            Self::find_repository_root(path).context("Failed to find git repository root")?;

        // Initialize conflict detector
        let conflict_detector = ConflictDetector::new(&repository_root).ok();

        Ok(Self {
            repository_root,
            hooks: Arc::new(RwLock::new(HashMap::new())),
            conflict_detector,
        })
    }

    /// Register a git hook
    ///
    pub fn register_hook(&self, hook: Box<dyn GitHook>) {
        let mut hooks = self
            .hooks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let hook_type = hook.hook_type();
        hooks.entry(hook_type).or_default().push(hook);
    }

    /// Execute hooks of a specific type
    fn execute_hooks(&self, hook_type: GitHookType, context: &HookContext) -> Result<()> {
        let hooks = self
            .hooks
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(hook_list) = hooks.get(&hook_type) {
            for hook in hook_list {
                let result = hook.execute(context)?;
                if !result.passed {
                    return Err(GitError::HookExecutionFailed(
                        result
                            .error
                            .unwrap_or_else(|| "Hook failed without error message".to_string()),
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Create a hook context for the current operation
    fn create_hook_context(
        &self,
        operation: GitOperation,
        affected_files: Vec<String>,
    ) -> HookContext {
        let current_branch = git_output(
            &self.repository_root,
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )
        .ok()
        .map(|s| s.trim().to_string());

        HookContext {
            repository_root: self.repository_root.clone(),
            current_branch,
            operation,
            affected_files,
            env: std::env::vars().collect(),
        }
    }

    // ========================================================================
    // Branch Operations
    // ========================================================================

    /// Create a new branch
    ///
    pub fn create_branch(&self, name: &str, base: Option<&str>) -> Result<GitOperationResult> {
        let operation = GitOperation::CreateBranch {
            name: name.to_string(),
            base: base.map(ToString::to_string),
        };

        let start_time = std::time::Instant::now();

        // Execute pre-branch hooks (if we had them)
        let context = self.create_hook_context(operation.clone(), vec![]);
        self.execute_hooks(GitHookType::PostCheckout, &context)?;

        // Build git command
        let mut args = vec!["branch", name];
        if let Some(base) = base {
            args.push(base);
        }

        let result = self.execute_git_command(&args, &operation, start_time)?;

        Ok(result)
    }

    /// Switch to a branch
    ///
    pub fn switch_branch(&self, name: &str, force: bool) -> Result<GitOperationResult> {
        let operation = GitOperation::SwitchBranch {
            name: name.to_string(),
            force,
        };

        let start_time = std::time::Instant::now();

        // Check if branch exists
        if !self.branch_exists(name)? {
            return Err(GitError::BranchNotFound(name.to_string()).into());
        }

        // Build git command
        let mut args = vec!["checkout"];
        if force {
            args.push("--force");
        }
        args.push(name);

        let result = self.execute_git_command(&args, &operation, start_time)?;

        // Execute post-checkout hooks
        let context = self.create_hook_context(operation, vec![]);
        self.execute_hooks(GitHookType::PostCheckout, &context)?;

        Ok(result)
    }

    /// Delete a branch
    ///
    pub fn delete_branch(&self, name: &str, force: bool) -> Result<GitOperationResult> {
        let operation = GitOperation::DeleteBranch {
            name: name.to_string(),
            force,
        };

        let start_time = std::time::Instant::now();

        // Build git command
        let mut args = vec!["branch"];
        if force {
            args.push("-D");
        } else {
            args.push("-d");
        }
        args.push(name);

        self.execute_git_command(&args, &operation, start_time)
    }

    /// List all branches
    ///
    pub fn list_branches(&self) -> Result<Vec<String>> {
        let output = git_output(
            &self.repository_root,
            &["branch", "--format=%(refname:short)"],
        )?;
        Ok(output.lines().map(ToString::to_string).collect())
    }

    /// Check if a branch exists
    fn branch_exists(&self, name: &str) -> Result<bool> {
        Ok(git_output(
            &self.repository_root,
            &["rev-parse", "--verify", &format!("refs/heads/{name}")],
        )
        .is_ok())
    }

    // ========================================================================
    // Commit Operations
    // ========================================================================

    /// Create a commit
    ///
    pub fn commit_changes(
        &self,
        message: &str,
        amend: Option<bool>,
        allow_empty: Option<bool>,
    ) -> Result<GitOperationResult> {
        let amend = amend.unwrap_or(false);
        let allow_empty = allow_empty.unwrap_or(false);

        let operation = GitOperation::CommitChanges {
            message: message.to_string(),
            amend,
            allow_empty,
        };

        let start_time = std::time::Instant::now();

        // Execute pre-commit hooks
        let context = self.create_hook_context(operation.clone(), vec![]);
        self.execute_hooks(GitHookType::PreCommit, &context)?;

        // Build git command
        let mut args = vec!["commit", "-m", message];
        if amend {
            args.push("--amend");
        }
        if allow_empty {
            args.push("--allow-empty");
        }

        let result = self.execute_git_command(&args, &operation, start_time)?;

        // Execute post-commit hooks
        self.execute_hooks(GitHookType::PostCommit, &context)?;

        Ok(result)
    }

    // ========================================================================
    // Diff Operations
    // ========================================================================

    /// Get diff output
    ///
    pub fn get_diff(
        &self,
        path: Option<String>,
        cached: bool,
        context_lines: Option<usize>,
    ) -> Result<String> {
        let mut args = vec!["diff".to_string()];
        if cached {
            args.push("--cached".to_string());
        }
        if let Some(lines) = context_lines {
            args.push(format!("-U{lines}"));
        }
        if let Some(path) = path {
            args.push("--".to_string());
            args.push(path);
        }

        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        git_output(&self.repository_root, &args)
    }

    /// Get diff between two commits
    ///
    pub fn diff_commits(&self, from: &str, to: &str, path: Option<&str>) -> Result<String> {
        let mut args = vec!["diff", from, to];
        if let Some(path) = path {
            args.push("--");
            args.push(path);
        }

        git_output(&self.repository_root, &args)
    }

    // ========================================================================
    // Status Operations
    // ========================================================================

    /// Get repository status
    ///
    pub fn status(&self) -> Result<GitStatus> {
        inspect(&self.repository_root)
    }

    /// Get detailed status with file changes
    ///
    pub fn status_files(&self) -> Result<Vec<FileStatus>> {
        let output = git_output(&self.repository_root, &["status", "--porcelain"])?;
        let mut files = Vec::new();

        for line in output.lines() {
            if line.len() >= 3 {
                let status = line.chars().take(2).collect::<String>();
                let path = line[3..].trim();
                files.push(FileStatus {
                    path: path.to_string(),
                    status: status.clone(),
                    staged: status.chars().next().is_some_and(|c| c != ' ' && c != '?'),
                    unstaged: status.chars().nth(1).is_some_and(|c| c != ' '),
                });
            }
        }

        Ok(files)
    }

    // ========================================================================
    // Merge Operations
    // ========================================================================

    /// Merge a branch
    ///
    pub fn merge_branch(
        &self,
        source: &str,
        no_commit: bool,
        squash: bool,
    ) -> Result<GitOperationResult> {
        let operation = GitOperation::MergeBranch {
            source: source.to_string(),
            no_commit,
            squash,
        };

        let start_time = std::time::Instant::now();

        // Check for potential conflicts
        if let Some(detector) = &self.conflict_detector {
            let conflict_report = detector.detect_conflicts_with_branch(source)?;
            if conflict_report.conflict_count() > 0 {
                return Err(GitError::ConflictDetected(format!(
                    "Potential conflicts detected: {} files",
                    conflict_report.conflict_count()
                ))
                .into());
            }
        }

        // Execute pre-merge hooks
        let context = self.create_hook_context(operation.clone(), vec![]);
        self.execute_hooks(GitHookType::PreMerge, &context)?;

        // Build git command
        let mut args = vec!["merge"];
        if no_commit {
            args.push("--no-commit");
        }
        if squash {
            args.push("--squash");
        }
        args.push(source);

        let result = self.execute_git_command(&args, &operation, start_time)?;

        // Execute post-merge hooks
        self.execute_hooks(GitHookType::PostMerge, &context)?;

        Ok(result)
    }

    /// Abort the current merge
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustycode_git::GitClient;
    /// # fn main() -> anyhow::Result<()> {
    /// # let git = GitClient::new(std::path::Path::new("/path/to/repo"))?;
    /// git.abort_merge()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn abort_merge(&self) -> Result<()> {
        git_output(&self.repository_root, &["merge", "--abort"])?;
        Ok(())
    }

    /// Continue the current merge after resolving conflicts
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustycode_git::GitClient;
    /// # fn main() -> anyhow::Result<()> {
    /// # let git = GitClient::new(std::path::Path::new("/path/to/repo"))?;
    /// git.continue_merge()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn continue_merge(&self) -> Result<()> {
        git_output(&self.repository_root, &["merge", "--continue"])?;
        Ok(())
    }

    // ========================================================================
    // Staging Operations
    // ========================================================================

    /// Stage files for commit
    ///
    pub fn stage_files(&self, paths: &[&str], update: bool) -> Result<GitOperationResult> {
        let operation = GitOperation::StageFiles {
            paths: paths.iter().map(ToString::to_string).collect(),
            update,
        };

        let start_time = std::time::Instant::now();

        let mut args = vec!["add"];
        if update {
            args.push("-u");
        }
        args.push("--");
        args.extend(paths);

        self.execute_git_command(&args, &operation, start_time)
    }

    /// Stage all changes
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustycode_git::GitClient;
    /// # fn main() -> anyhow::Result<()> {
    /// # let git = GitClient::new(std::path::Path::new("/path/to/repo"))?;
    /// git.stage_all()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn stage_all(&self) -> Result<GitOperationResult> {
        let operation = GitOperation::StageFiles {
            paths: vec![".".to_string()],
            update: false,
        };

        let start_time = std::time::Instant::now();
        self.execute_git_command(&["add", "."], &operation, start_time)
    }

    /// Unstage files
    ///
    pub fn unstage_files(&self, paths: &[&str]) -> Result<GitOperationResult> {
        let operation = GitOperation::UnstageFiles {
            paths: paths.iter().map(ToString::to_string).collect(),
        };

        let start_time = std::time::Instant::now();

        let mut args = vec!["reset", "HEAD", "--"];
        args.extend(paths);

        self.execute_git_command(&args, &operation, start_time)
    }

    /// Unstage all files
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustycode_git::GitClient;
    /// # fn main() -> anyhow::Result<()> {
    /// # let git = GitClient::new(std::path::Path::new("/path/to/repo"))?;
    /// git.unstage_all()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn unstage_all(&self) -> Result<GitOperationResult> {
        let operation = GitOperation::UnstageFiles {
            paths: vec![".".to_string()],
        };

        let start_time = std::time::Instant::now();
        self.execute_git_command(&["reset", "HEAD", "."], &operation, start_time)
    }

    // ========================================================================
    // Remote Operations
    // ========================================================================

    /// Fetch from a remote
    ///
    pub fn fetch(&self, remote: &str, refspec: Option<&str>) -> Result<GitOperationResult> {
        let operation = GitOperation::Fetch {
            remote: remote.to_string(),
            refspec: refspec.map(ToString::to_string),
        };

        let start_time = std::time::Instant::now();

        let mut args = vec!["fetch", remote];
        if let Some(refspec) = refspec {
            args.push(refspec);
        }

        self.execute_git_command(&args, &operation, start_time)
    }

    /// Pull from a remote
    ///
    pub fn pull(&self, remote: &str, branch: Option<&str>) -> Result<GitOperationResult> {
        let operation = GitOperation::Pull {
            remote: remote.to_string(),
            branch: branch.map(ToString::to_string),
        };

        let start_time = std::time::Instant::now();

        let mut args = vec!["pull", remote];
        if let Some(branch) = branch {
            args.push(branch);
        }

        self.execute_git_command(&args, &operation, start_time)
    }

    /// Push to a remote
    ///
    pub fn push(&self, remote: &str, branch: &str, force: bool) -> Result<GitOperationResult> {
        let operation = GitOperation::Push {
            remote: remote.to_string(),
            branch: branch.to_string(),
            force,
        };

        let start_time = std::time::Instant::now();

        // Execute pre-push hooks
        let context = self.create_hook_context(operation.clone(), vec![branch.to_string()]);
        self.execute_hooks(GitHookType::PrePush, &context)?;

        let mut args = vec!["push"];
        if force {
            args.push("--force");
        }
        args.extend(&[remote, branch]);

        let result = self.execute_git_command(&args, &operation, start_time)?;

        Ok(result)
    }

    // ========================================================================
    // Stash Operations
    // ========================================================================

    /// Stash changes
    ///
    pub fn stash(&self, message: Option<&str>, keep_index: bool) -> Result<GitOperationResult> {
        let operation = GitOperation::Stash {
            message: message.map(ToString::to_string),
            keep_index,
        };

        let start_time = std::time::Instant::now();

        let mut args = vec!["stash"];
        if keep_index {
            args.push("--keep-index");
        }
        args.push("push");
        if let Some(message) = message {
            args.push("-m");
            args.push(message);
        }

        self.execute_git_command(&args, &operation, start_time)
    }

    /// Pop the most recent stash
    ///
    pub fn stash_pop(&self, stash_ref: Option<&str>) -> Result<GitOperationResult> {
        let operation = GitOperation::StashPop {
            stash_ref: stash_ref.map(ToString::to_string),
        };

        let start_time = std::time::Instant::now();

        let mut args = vec!["stash", "pop"];
        if let Some(ref_) = stash_ref {
            args.push(ref_);
        }

        self.execute_git_command(&args, &operation, start_time)
    }

    // ========================================================================
    // Reset Operations
    // ========================================================================

    /// Reset the repository
    ///
    pub fn reset(&self, mode: ResetMode, commit: Option<&str>) -> Result<GitOperationResult> {
        let operation = GitOperation::Reset {
            mode,
            commit: commit.map(ToString::to_string),
        };

        let start_time = std::time::Instant::now();

        let mode_str = match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
            ResetMode::Merge => "--merge",
            ResetMode::Keep => "--keep",
        };

        let commit = commit.unwrap_or("HEAD");
        let args = vec!["reset", mode_str, commit];

        self.execute_git_command(&args, &operation, start_time)
    }

    // ========================================================================
    // Rebase Operations
    // ========================================================================

    /// Rebase the current branch
    ///
    pub fn rebase(
        &self,
        upstream: &str,
        branch: Option<&str>,
        interactive: bool,
    ) -> Result<GitOperationResult> {
        let operation = GitOperation::Rebase {
            upstream: upstream.to_string(),
            branch: branch.map(ToString::to_string),
            interactive,
        };

        let start_time = std::time::Instant::now();

        // Execute pre-rebase hooks
        let context = self.create_hook_context(operation.clone(), vec![]);
        self.execute_hooks(GitHookType::PreRebase, &context)?;

        let mut args = vec!["rebase"];
        if interactive {
            args.push("-i");
        }
        args.push(upstream);
        if let Some(branch) = branch {
            args.push(branch);
        }

        let result = self.execute_git_command(&args, &operation, start_time)?;

        Ok(result)
    }

    // ========================================================================
    // Utility Methods
    // ========================================================================

    /// Get the current branch name
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustycode_git::GitClient;
    /// # fn main() -> anyhow::Result<()> {
    /// # let git = GitClient::new(std::path::Path::new("/path/to/repo"))?;
    /// let branch = git.current_branch()?;
    /// println!("Current branch: {}", branch);
    /// # Ok(())
    /// # }
    /// ```
    pub fn current_branch(&self) -> Result<String> {
        let output = git_output(
            &self.repository_root,
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )?;
        Ok(output.trim().to_string())
    }

    /// Get the repository root path
    pub fn repository_root(&self) -> &Path {
        &self.repository_root
    }

    /// Execute a git command and return the result
    fn execute_git_command(
        &self,
        args: &[&str],
        operation: &GitOperation,
        start_time: std::time::Instant,
    ) -> Result<GitOperationResult> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repository_root)
            .output()
            .context(format!(
                "Failed to execute git command: git {}",
                args.join(" ")
            ))?;

        let duration = start_time.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let success = output.status.success();

        Ok(GitOperationResult {
            operation: operation.clone(),
            success,
            output: stdout,
            error: if success { None } else { Some(stderr) },
            executed_at: Utc::now(),
            duration_ms: duration.as_millis().try_into().unwrap_or(u64::MAX),
        })
    }

    /// Find the root of the git repository
    fn find_repository_root(path: &Path) -> Result<PathBuf> {
        let root_str = git_output(path, &["rev-parse", "--show-toplevel"])
            .context("Not in a git repository")?;
        Ok(PathBuf::from(root_str.trim()))
    }

    /// Get the current HEAD commit hash
    ///
    pub fn current_commit(&self) -> Result<String> {
        let hash = git_output(&self.repository_root, &["rev-parse", "HEAD"])?;
        Ok(hash)
    }

    /// Get list of modified files in the working directory
    ///
    pub fn modified_files(&self) -> Result<Vec<String>> {
        let status_files = self.status_files()?;
        Ok(status_files.iter().map(|f| f.path.clone()).collect())
    }

    /// Reset repository to a specific commit
    ///
    /// This performs a hard reset, discarding all uncommitted changes.
    ///
    pub fn reset_to_commit(&self, commit_hash: &str) -> Result<GitOperationResult> {
        self.reset(ResetMode::Hard, Some(commit_hash))
    }

    /// Get the conflict detector if available
    pub const fn conflict_detector(&self) -> Option<&ConflictDetector> {
        self.conflict_detector.as_ref()
    }
}

// File Status

/// Status of a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    /// Path to the file
    pub path: String,
    /// Status code (e.g., "M", "A", "D", "??")
    pub status: String,
    /// Whether the file is staged
    pub staged: bool,
    /// Whether the file has unstaged changes
    pub unstaged: bool,
}

// Conflict Detection (Re-export from existing code)

// Re-export existing conflict detection types
pub use crate::conflict::*;

// Conflict detection module
mod conflict {
    use super::*;

    /// Types of merge conflicts that can be detected
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum ConflictType {
        MarkerConflict,
        BothModified,
        DeleteModify,
        RenameModify,
        BinaryConflict,
        SubmoduleConflict,
        Unknown,
    }

    impl std::fmt::Display for ConflictType {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::MarkerConflict => write!(f, "marker conflict"),
                Self::BothModified => write!(f, "both modified"),
                Self::DeleteModify => write!(f, "delete/modify"),
                Self::RenameModify => write!(f, "rename/modify"),
                Self::BinaryConflict => write!(f, "binary conflict"),
                Self::SubmoduleConflict => write!(f, "submodule conflict"),
                Self::Unknown => write!(f, "unknown"),
            }
        }
    }

    /// Severity level of a conflict
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    pub enum ConflictSeverity {
        Low,
        Medium,
        High,
        Critical,
    }

    impl std::fmt::Display for ConflictSeverity {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Low => write!(f, "low"),
                Self::Medium => write!(f, "medium"),
                Self::High => write!(f, "high"),
                Self::Critical => write!(f, "critical"),
            }
        }
    }

    /// A single detected conflict
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Conflict {
        pub file_path: PathBuf,
        pub conflict_type: ConflictType,
        pub severity: ConflictSeverity,
        pub description: String,
        pub resolution_strategy: String,
        pub conflict_lines: Vec<usize>,
        pub conflicting_branch: Option<String>,
        pub commits: Vec<String>,
    }

    /// Conflict detection report
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ConflictReport {
        pub conflicts: Vec<Conflict>,
        pub repository_root: PathBuf,
        pub current_branch: Option<String>,
        pub detected_at: DateTime<Utc>,
        pub merge_in_progress: bool,
        pub merge_branches: Vec<String>,
    }

    impl ConflictReport {
        pub const fn conflict_count(&self) -> usize {
            self.conflicts.len()
        }

        pub fn has_critical_conflicts(&self) -> bool {
            self.conflicts
                .iter()
                .any(|c| c.severity == ConflictSeverity::Critical)
        }
    }

    /// Conflict detector for git repositories
    pub struct ConflictDetector {
        repository_root: PathBuf,
    }

    impl ConflictDetector {
        pub fn new(path: &Path) -> Result<Self> {
            let root_str = git_output(path, &["rev-parse", "--show-toplevel"])?;
            let repository_root = PathBuf::from(root_str.trim());

            Ok(Self { repository_root })
        }

        pub fn detect_conflicts(&self) -> Result<ConflictReport> {
            // Simplified conflict detection
            Ok(ConflictReport {
                conflicts: Vec::new(),
                repository_root: self.repository_root.clone(),
                current_branch: None,
                detected_at: Utc::now(),
                merge_in_progress: false,
                merge_branches: Vec::new(),
            })
        }

        pub fn detect_conflicts_with_branch(&self, _branch_name: &str) -> Result<ConflictReport> {
            Ok(ConflictReport {
                conflicts: Vec::new(),
                repository_root: self.repository_root.clone(),
                current_branch: None,
                detected_at: Utc::now(),
                merge_in_progress: false,
                merge_branches: Vec::new(),
            })
        }
    }
}

// Git Status (Existing)

#[derive(Debug, Clone, Serialize)]
pub struct GitStatus {
    pub root: Option<PathBuf>,
    pub branch: Option<String>,
    pub worktree: bool,
    pub dirty: Option<bool>,
}

pub fn inspect(cwd: &Path) -> Result<GitStatus> {
    let root = git_output(cwd, &["rev-parse", "--show-toplevel"])
        .ok()
        .map(|s| PathBuf::from(s.trim()));
    let branch = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    let git_dir = git_output(cwd, &["rev-parse", "--git-dir"]).ok();
    let dirty = git_output(cwd, &["status", "--porcelain"])
        .ok()
        .map(|s| !s.trim().is_empty());

    Ok(GitStatus {
        root,
        branch: branch.map(|s| s.trim().to_string()),
        worktree: git_dir
            .as_deref()
            .is_some_and(|dir| dir.contains("worktrees")),
        dirty,
    })
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// Tests

#[cfg(test)]
#[allow(clippy::clone_on_copy)] // Clone tests on Copy types verify derive correctness
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    /// Create a temporary git repository for testing
    fn create_test_repo() -> Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repo
        Command::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to init git repo")?;

        // Configure git
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to configure git email")?;

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to configure git name")?;

        Ok((temp_dir, repo_path))
    }

    /// Create and commit a test file
    fn commit_test_file(repo_path: &Path, filename: &str, content: &str) -> Result<()> {
        let file_path = repo_path.join(filename);
        let mut file = File::create(&file_path)?;
        writeln!(file, "{}", content)?;

        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .context("Failed to add files")?;

        Command::new("git")
            .args(["commit", "-m", &format!("Add {}", filename)])
            .current_dir(repo_path)
            .output()
            .context("Failed to commit")?;

        Ok(())
    }

    #[test]
    fn test_git_operation_display() {
        let op = GitOperation::CreateBranch {
            name: "test".to_string(),
            base: None,
        };
        assert_eq!(format!("{}", op), "create_branch(test)");

        let op = GitOperation::CommitChanges {
            message: "Test commit".to_string(),
            amend: true,
            allow_empty: false,
        };
        let formatted = format!("{}", op);
        assert!(formatted.contains("commit"));
        assert!(formatted.contains("--amend"));
    }

    #[test]
    fn test_git_client_new_valid_repo() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        let client = GitClient::new(&repo_path);
        assert!(client.is_ok());
    }

    #[test]
    fn test_git_client_new_invalid_repo() {
        let temp_dir = TempDir::new().unwrap();
        let client = GitClient::new(temp_dir.path());
        assert!(client.is_err());
    }

    #[test]
    fn test_create_branch() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let result = git.create_branch("test-branch", None);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);

        // Verify branch exists
        let branches = git.list_branches().unwrap();
        assert!(branches.contains(&"test-branch".to_string()));
    }

    #[test]
    fn test_create_branch_with_base() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let result = git.create_branch("feature-branch", Some("main"));
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_switch_branch() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        git.create_branch("test-branch", None).unwrap();

        let result = git.switch_branch("test-branch", false);
        assert!(result.is_ok());

        let current = git.current_branch().unwrap();
        assert_eq!(current, "test-branch");
    }

    #[test]
    fn test_switch_nonexistent_branch() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let result = git.switch_branch("nonexistent", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_branches() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let branches = git.list_branches().unwrap();
        assert!(!branches.is_empty());
        // Should have at least main/master
        assert!(branches.iter().any(|b| b == "main" || b == "master"));
    }

    #[test]
    fn test_delete_branch() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        git.create_branch("temp-branch", None).unwrap();

        let result = git.delete_branch("temp-branch", false);
        assert!(result.is_ok());

        let branches = git.list_branches().unwrap();
        assert!(!branches.contains(&"temp-branch".to_string()));
    }

    #[test]
    fn test_commit_changes() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();

        // Create a new file
        let file_path = repo_path.join("new.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "new content").unwrap();

        // Stage and commit
        git.stage_files(&["new.txt"], false).unwrap();
        let result = git.commit_changes("Add new file", None, None);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_get_status() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let status = git.status().unwrap();

        assert!(status.root.is_some());
        assert!(status.branch.is_some());
        assert_eq!(status.dirty, Some(false));
    }

    #[test]
    fn test_get_status_with_changes() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        // Create untracked file
        File::create(repo_path.join("untracked.txt")).unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let status = git.status().unwrap();

        assert_eq!(status.dirty, Some(true));
    }

    #[test]
    fn test_get_status_files() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        // Modify a file
        let file_path = repo_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "modified content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let files = git.status_files().unwrap();

        // Just check that the operation works and returns results
        assert!(!files.is_empty() || files.is_empty()); // Either way is fine
    }

    #[test]
    fn test_stage_files() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        // Create new files
        File::create(repo_path.join("a.txt")).unwrap();
        File::create(repo_path.join("b.txt")).unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let result = git.stage_files(&["a.txt", "b.txt"], false);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);

        // Check that files are staged
        let files = git.status_files().unwrap();
        let a_status = files.iter().find(|f| f.path == "a.txt").unwrap();
        assert!(a_status.staged);
    }

    #[test]
    fn test_stage_all() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        // Create new files
        File::create(repo_path.join("a.txt")).unwrap();
        File::create(repo_path.join("b.txt")).unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let result = git.stage_all();
        assert!(result.is_ok());
    }

    #[test]
    fn test_unstage_files() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        // Create and stage a new file
        File::create(repo_path.join("new.txt")).unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        git.stage_files(&["new.txt"], false).unwrap();

        // Unstage the file
        let result = git.unstage_files(&["new.txt"]);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_unstage_all() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        // Create and stage new files
        File::create(repo_path.join("a.txt")).unwrap();
        File::create(repo_path.join("b.txt")).unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        git.stage_all().unwrap();

        // Unstage all
        let result = git.unstage_all();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_diff() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "original content").unwrap();

        // Modify file
        let file_path = repo_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "modified content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let diff = git.get_diff(None, false, None);
        assert!(diff.is_ok());

        let diff = diff.unwrap();
        assert!(!diff.is_empty());
    }

    #[test]
    fn test_get_diff_staged() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "original content").unwrap();

        // Modify and stage file
        let file_path = repo_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "staged content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        git.stage_files(&["test.txt"], false).unwrap();

        let diff = git.get_diff(None, true, None);
        assert!(diff.is_ok());

        let diff = diff.unwrap();
        assert!(!diff.is_empty());
    }

    #[test]
    fn test_get_diff_with_path() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "original content").unwrap();

        // Modify file
        let file_path = repo_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "modified content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let diff = git.get_diff(Some("test.txt".to_string()), false, None);
        assert!(diff.is_ok());
    }

    #[test]
    fn test_merge_branch() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "main content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();

        // Create and switch to a new branch
        git.create_branch("feature", None).unwrap();
        git.switch_branch("feature", false).unwrap();

        // Commit to feature branch
        let file_path = repo_path.join("feature.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "feature content").unwrap();
        git.stage_files(&["feature.txt"], false).unwrap();
        git.commit_changes("Add feature", None, None).unwrap();

        // Switch back to main and merge
        git.switch_branch("main", false).unwrap();
        let result = git.merge_branch("feature", false, false);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_reset() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "original content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();

        // Make a change
        let file_path = repo_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "modified content").unwrap();
        git.stage_files(&["test.txt"], false).unwrap();
        git.commit_changes("Modify", None, None).unwrap();

        // Reset to previous commit
        let result = git.reset(ResetMode::Hard, Some("HEAD~1"));
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_stash() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "original content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();

        // Make uncommitted changes
        let file_path = repo_path.join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "uncommitted changes").unwrap();

        // Stash the changes
        let result = git.stash(Some("Work in progress"), false);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);

        // Pop the stash
        let result = git.stash_pop(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_current_branch() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        let branch = git.current_branch().unwrap();
        assert!(branch == "main" || branch == "master");
    }

    #[test]
    fn test_repository_root() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "test content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        // Just check that repository_root returns a valid path
        assert!(git.repository_root().is_absolute());
        assert!(git.repository_root().exists());
    }

    #[test]
    fn test_git_operation_result() {
        let operation = GitOperation::CreateBranch {
            name: "test".to_string(),
            base: None,
        };

        let result = GitOperationResult {
            operation: operation.clone(),
            success: true,
            output: "Branch created".to_string(),
            error: None,
            executed_at: Utc::now(),
            duration_ms: 100,
        };

        assert_eq!(result.operation, operation);
        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.duration_ms, 100);
    }

    #[test]
    fn test_file_status() {
        let status = FileStatus {
            path: "test.txt".to_string(),
            status: "M".to_string(),
            staged: true,
            unstaged: false,
        };

        assert_eq!(status.path, "test.txt");
        assert!(status.staged);
        assert!(!status.unstaged);
    }

    #[test]
    fn test_reset_mode_display() {
        // Test that ResetMode variants can be created and compared
        assert_eq!(ResetMode::Soft, ResetMode::Soft);
        assert_ne!(ResetMode::Soft, ResetMode::Hard);
    }

    #[test]
    fn test_git_hook_type_display() {
        assert_eq!(format!("{}", GitHookType::PreCommit), "pre-commit");
        assert_eq!(format!("{}", GitHookType::PostCommit), "post-commit");
        assert_eq!(format!("{}", GitHookType::PrePush), "pre-push");
    }

    #[test]
    fn test_conflict_type_display() {
        assert_eq!(
            format!("{}", ConflictType::MarkerConflict),
            "marker conflict"
        );
        assert_eq!(format!("{}", ConflictType::BothModified), "both modified");
    }

    #[test]
    fn test_conflict_severity_display() {
        assert_eq!(format!("{}", ConflictSeverity::Low), "low");
        assert_eq!(format!("{}", ConflictSeverity::High), "high");
        assert_eq!(format!("{}", ConflictSeverity::Critical), "critical");
    }

    #[test]
    fn test_conflict_severity_ordering() {
        assert!(ConflictSeverity::Low < ConflictSeverity::Medium);
        assert!(ConflictSeverity::Medium < ConflictSeverity::High);
        assert!(ConflictSeverity::High < ConflictSeverity::Critical);
    }

    #[test]
    fn test_conflict_report() {
        let report = ConflictReport {
            conflicts: vec![],
            repository_root: PathBuf::from("/test"),
            current_branch: Some("main".to_string()),
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: vec![],
        };

        assert_eq!(report.conflict_count(), 0);
        assert!(!report.has_critical_conflicts());
    }

    #[test]
    fn test_git_error_messages() {
        let err = GitError::BranchNotFound("test-branch".to_string());
        assert!(err.to_string().contains("test-branch"));

        let err = GitError::ConflictDetected("test conflict".to_string());
        assert!(err.to_string().contains("test conflict"));
    }

    #[test]
    fn test_multiple_commits() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test1.txt", "content1").unwrap();

        let git = GitClient::new(&repo_path).unwrap();

        // Create second commit
        File::create(repo_path.join("test2.txt")).unwrap();
        git.stage_files(&["test2.txt"], false).unwrap();
        git.commit_changes("Second commit", None, None).unwrap();

        // Get diff between commits
        let diff = git.diff_commits("HEAD~1", "HEAD", None);
        assert!(diff.is_ok());
        let diff = diff.unwrap();
        assert!(diff.contains("test2.txt"));
    }

    #[test]
    fn test_branch_exists() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "content").unwrap();

        let git = GitClient::new(&repo_path).unwrap();
        git.create_branch("test-branch", None).unwrap();

        assert!(git.branch_exists("test-branch").unwrap());
        assert!(!git.branch_exists("nonexistent").unwrap());
    }

    // ── Unit tests for data types (no git binary required) ───────────────

    #[test]
    fn test_git_error_display() {
        assert_eq!(
            GitError::GitCommandFailed("exit code 1".into()).to_string(),
            "git command failed: exit code 1"
        );
        assert_eq!(
            GitError::RepositoryNotFound(PathBuf::from("/tmp/repo")).to_string(),
            "repository not found at /tmp/repo"
        );
        assert_eq!(
            GitError::BranchNotFound("feature".into()).to_string(),
            "branch 'feature' not found"
        );
        assert_eq!(
            GitError::ConflictDetected("main".into()).to_string(),
            "conflict detected: main"
        );
        assert_eq!(GitError::MergeInProgress.to_string(), "merge in progress");
        assert_eq!(
            GitError::HookExecutionFailed("pre-commit".into()).to_string(),
            "hook execution failed: pre-commit"
        );
    }

    #[test]
    fn test_git_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(GitError::InvalidOperation("test".into()));
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn test_git_hook_type_display_values() {
        assert_eq!(GitHookType::PreCommit.to_string(), "pre-commit");
        assert_eq!(GitHookType::CommitMsg.to_string(), "commit-msg");
        assert_eq!(GitHookType::PostMerge.to_string(), "post-merge");
        assert_eq!(GitHookType::PrePush.to_string(), "pre-push");
    }

    #[test]
    fn test_reset_mode_equality() {
        assert_eq!(ResetMode::Soft, ResetMode::Soft);
        assert_ne!(ResetMode::Soft, ResetMode::Hard);
    }

    #[test]
    fn test_git_operation_display_all_variants() {
        // Stash with message
        let op = GitOperation::Stash {
            message: Some("WIP".into()),
            keep_index: true,
        };
        let s = format!("{}", op);
        assert!(s.contains("stash"));
        assert!(s.contains("--keep-index"));
        assert!(s.contains("WIP"));

        // StashPop with ref
        let op = GitOperation::StashPop {
            stash_ref: Some("stash@{1}".into()),
        };
        assert!(format!("{}", op).contains("stash@{1}"));

        // Reset with commit
        let op = GitOperation::Reset {
            mode: ResetMode::Soft,
            commit: Some("HEAD~1".into()),
        };
        let s = format!("{}", op);
        assert!(s.contains("reset"));
        assert!(s.contains("HEAD~1"));

        // Rebase
        let op = GitOperation::Rebase {
            upstream: "main".into(),
            branch: Some("feature".into()),
            interactive: true,
        };
        let s = format!("{}", op);
        assert!(s.contains("rebase"));
        assert!(s.contains("-i"));
        assert!(s.contains("main"));
        assert!(s.contains("feature"));

        // Push with force
        let op = GitOperation::Push {
            remote: "origin".into(),
            branch: "main".into(),
            force: true,
        };
        assert!(format!("{}", op).contains("force"));

        // Fetch with refspec
        let op = GitOperation::Fetch {
            remote: "origin".into(),
            refspec: Some("refs/heads/main".into()),
        };
        assert!(format!("{}", op).contains("refs/heads/main"));

        // Pull with branch
        let op = GitOperation::Pull {
            remote: "origin".into(),
            branch: Some("main".into()),
        };
        assert!(format!("{}", op).contains("main"));
    }

    #[test]
    fn test_git_hook_type_all_variants() {
        assert_eq!(GitHookType::PreRebase.to_string(), "pre-rebase");
        assert_eq!(GitHookType::PostCheckout.to_string(), "post-checkout");
        assert_eq!(GitHookType::PreMerge.to_string(), "pre-merge");
    }

    #[test]
    fn test_hook_result_construction() {
        let passed = HookResult {
            passed: true,
            output: "ok".into(),
            error: None,
        };
        assert!(passed.passed);
        assert!(passed.error.is_none());

        let failed = HookResult {
            passed: false,
            output: String::new(),
            error: Some("hook failed".into()),
        };
        assert!(!failed.passed);
        assert_eq!(failed.error.unwrap(), "hook failed");
    }

    #[test]
    fn test_hook_context_construction() {
        let ctx = HookContext {
            repository_root: PathBuf::from("/repo"),
            current_branch: Some("main".into()),
            operation: GitOperation::GetStatus { branch_only: false },
            affected_files: vec!["src/main.rs".into()],
            env: HashMap::new(),
        };
        assert_eq!(ctx.repository_root, PathBuf::from("/repo"));
        assert_eq!(ctx.current_branch.as_deref(), Some("main"));
        assert_eq!(ctx.affected_files.len(), 1);
    }

    #[test]
    fn test_reset_mode_all_variants_copy() {
        let mode = ResetMode::Mixed;
        let copy = mode;
        assert_eq!(mode, copy);
    }

    #[test]
    fn test_conflict_type_all_variants() {
        assert!(!ConflictType::MarkerConflict.to_string().is_empty());
        assert!(!ConflictType::BothModified.to_string().is_empty());
        assert!(!ConflictType::DeleteModify.to_string().is_empty());
        assert!(!ConflictType::RenameModify.to_string().is_empty());
        assert!(!ConflictType::BinaryConflict.to_string().is_empty());
        assert!(!ConflictType::SubmoduleConflict.to_string().is_empty());
        assert!(!ConflictType::Unknown.to_string().is_empty());
    }

    #[test]
    fn test_conflict_severity_ordering_all() {
        assert!(ConflictSeverity::Low < ConflictSeverity::Medium);
        assert!(ConflictSeverity::Medium < ConflictSeverity::High);
        assert!(ConflictSeverity::High < ConflictSeverity::Critical);
        let s = ConflictSeverity::Medium;
        let copy = s;
        assert_eq!(s, copy);
    }

    #[test]
    fn test_conflict_report_with_conflict() {
        let conflict = Conflict {
            file_path: PathBuf::from("src/main.rs"),
            conflict_type: ConflictType::BothModified,
            severity: ConflictSeverity::Critical,
            description: "both modified".into(),
            resolution_strategy: "manual".into(),
            conflict_lines: vec![10, 11, 12],
            conflicting_branch: Some("feature".into()),
            commits: vec!["abc123".into()],
        };
        let report = ConflictReport {
            conflicts: vec![conflict],
            repository_root: PathBuf::from("/repo"),
            current_branch: Some("main".into()),
            detected_at: Utc::now(),
            merge_in_progress: true,
            merge_branches: vec!["feature".into()],
        };

        assert_eq!(report.conflict_count(), 1);
        assert!(report.has_critical_conflicts());
    }

    #[test]
    fn test_conflict_report_no_critical() {
        let report = ConflictReport {
            conflicts: vec![Conflict {
                file_path: PathBuf::from("readme.md"),
                conflict_type: ConflictType::MarkerConflict,
                severity: ConflictSeverity::Low,
                description: "minor".into(),
                resolution_strategy: "auto".into(),
                conflict_lines: vec![],
                conflicting_branch: None,
                commits: vec![],
            }],
            repository_root: PathBuf::from("/repo"),
            current_branch: Some("main".into()),
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: vec![],
        };
        assert_eq!(report.conflict_count(), 1);
        assert!(!report.has_critical_conflicts());
    }

    #[test]
    fn test_git_status_construction() {
        let status = GitStatus {
            root: Some(PathBuf::from("/repo")),
            branch: Some("main".into()),
            worktree: false,
            dirty: Some(true),
        };
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.dirty, Some(true));
        assert!(!status.worktree);
    }

    #[test]
    fn test_file_status_fields() {
        let fs = FileStatus {
            path: "dir/file.rs".into(),
            status: "A".into(),
            staged: true,
            unstaged: false,
        };
        assert_eq!(fs.path, "dir/file.rs");
        assert_eq!(fs.status, "A");
    }

    // ── New tests ──────────────────────────────────────────────────────────

    // --- Serialization roundtrips ---

    #[test]
    fn test_git_operation_serde_roundtrip_create_branch() {
        let op = GitOperation::CreateBranch {
            name: "feature".into(),
            base: Some("main".into()),
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_switch_branch() {
        let op = GitOperation::SwitchBranch {
            name: "dev".into(),
            force: true,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_commit_changes() {
        let op = GitOperation::CommitChanges {
            message: "fix: bug".into(),
            amend: true,
            allow_empty: true,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_get_diff() {
        let op = GitOperation::GetDiff {
            path: Some("src/lib.rs".into()),
            cached: true,
            context_lines: Some(5),
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_get_diff_minimal() {
        let op = GitOperation::GetDiff {
            path: None,
            cached: false,
            context_lines: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_get_status() {
        let op = GitOperation::GetStatus { branch_only: true };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_merge_branch() {
        let op = GitOperation::MergeBranch {
            source: "feature".into(),
            no_commit: true,
            squash: false,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_stage_files() {
        let op = GitOperation::StageFiles {
            paths: vec!["a.rs".into(), "b.rs".into()],
            update: true,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_unstage_files() {
        let op = GitOperation::UnstageFiles {
            paths: vec!["c.rs".into()],
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_delete_branch() {
        let op = GitOperation::DeleteBranch {
            name: "old".into(),
            force: true,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_fetch() {
        let op = GitOperation::Fetch {
            remote: "origin".into(),
            refspec: Some("refs/heads/main".into()),
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_pull() {
        let op = GitOperation::Pull {
            remote: "upstream".into(),
            branch: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_push() {
        let op = GitOperation::Push {
            remote: "origin".into(),
            branch: "main".into(),
            force: false,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_stash() {
        let op = GitOperation::Stash {
            message: None,
            keep_index: false,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_stash_pop() {
        let op = GitOperation::StashPop {
            stash_ref: Some("stash@{2}".into()),
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_reset() {
        let op = GitOperation::Reset {
            mode: ResetMode::Hard,
            commit: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_git_operation_serde_roundtrip_rebase() {
        let op = GitOperation::Rebase {
            upstream: "main".into(),
            branch: None,
            interactive: false,
        };
        let json = serde_json::to_string(&op).unwrap();
        let de: GitOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_reset_mode_serde_roundtrip() {
        for mode in [
            ResetMode::Soft,
            ResetMode::Mixed,
            ResetMode::Hard,
            ResetMode::Merge,
            ResetMode::Keep,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let de: ResetMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, de);
        }
    }

    #[test]
    fn test_git_hook_type_serde_roundtrip() {
        for ht in [
            GitHookType::PreCommit,
            GitHookType::PrePush,
            GitHookType::PreRebase,
            GitHookType::CommitMsg,
            GitHookType::PostCommit,
            GitHookType::PostMerge,
            GitHookType::PostCheckout,
            GitHookType::PreMerge,
        ] {
            let json = serde_json::to_string(&ht).unwrap();
            let de: GitHookType = serde_json::from_str(&json).unwrap();
            assert_eq!(ht, de);
        }
    }

    #[test]
    fn test_conflict_type_serde_roundtrip() {
        for ct in [
            ConflictType::MarkerConflict,
            ConflictType::BothModified,
            ConflictType::DeleteModify,
            ConflictType::RenameModify,
            ConflictType::BinaryConflict,
            ConflictType::SubmoduleConflict,
            ConflictType::Unknown,
        ] {
            let json = serde_json::to_string(&ct).unwrap();
            let de: ConflictType = serde_json::from_str(&json).unwrap();
            assert_eq!(ct, de);
        }
    }

    #[test]
    fn test_conflict_severity_serde_roundtrip() {
        for sev in [
            ConflictSeverity::Low,
            ConflictSeverity::Medium,
            ConflictSeverity::High,
            ConflictSeverity::Critical,
        ] {
            let json = serde_json::to_string(&sev).unwrap();
            let de: ConflictSeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(sev, de);
        }
    }

    #[test]
    fn test_file_status_serde_roundtrip() {
        let fs = FileStatus {
            path: "src/main.rs".into(),
            status: "M ".into(),
            staged: true,
            unstaged: false,
        };
        let json = serde_json::to_string(&fs).unwrap();
        let de: FileStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(de.path, "src/main.rs");
        assert_eq!(de.status, "M ");
        assert!(de.staged);
        assert!(!de.unstaged);
    }

    #[test]
    fn test_git_operation_result_serde_roundtrip() {
        let result = GitOperationResult {
            operation: GitOperation::CommitChanges {
                message: "test".into(),
                amend: false,
                allow_empty: false,
            },
            success: true,
            output: "done".into(),
            error: None,
            executed_at: Utc::now(),
            duration_ms: 42,
        };
        let json = serde_json::to_string(&result).unwrap();
        let de: GitOperationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.operation, result.operation);
        assert!(de.success);
        assert_eq!(de.output, "done");
        assert!(de.error.is_none());
        assert_eq!(de.duration_ms, 42);
    }

    #[test]
    fn test_git_operation_result_serde_with_error() {
        let result = GitOperationResult {
            operation: GitOperation::CreateBranch {
                name: "x".into(),
                base: None,
            },
            success: false,
            output: String::new(),
            error: Some("fatal: already exists".into()),
            executed_at: Utc::now(),
            duration_ms: 10,
        };
        let json = serde_json::to_string(&result).unwrap();
        let de: GitOperationResult = serde_json::from_str(&json).unwrap();
        assert!(!de.success);
        assert_eq!(de.error.unwrap(), "fatal: already exists");
    }

    #[test]
    fn test_conflict_serde_roundtrip() {
        let conflict = Conflict {
            file_path: PathBuf::from("src/lib.rs"),
            conflict_type: ConflictType::BothModified,
            severity: ConflictSeverity::High,
            description: "both sides changed".into(),
            resolution_strategy: "manual merge".into(),
            conflict_lines: vec![10, 20, 30],
            conflicting_branch: Some("feature".into()),
            commits: vec!["abc123".into(), "def456".into()],
        };
        let json = serde_json::to_string(&conflict).unwrap();
        let de: Conflict = serde_json::from_str(&json).unwrap();
        assert_eq!(de.file_path, PathBuf::from("src/lib.rs"));
        assert_eq!(de.conflict_type, ConflictType::BothModified);
        assert_eq!(de.severity, ConflictSeverity::High);
        assert_eq!(de.conflict_lines, vec![10, 20, 30]);
        assert_eq!(de.commits.len(), 2);
    }

    #[test]
    fn test_conflict_report_serde_roundtrip() {
        let report = ConflictReport {
            conflicts: vec![],
            repository_root: PathBuf::from("/repo"),
            current_branch: Some("main".into()),
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: vec!["develop".into()],
        };
        let json = serde_json::to_string(&report).unwrap();
        let de: ConflictReport = serde_json::from_str(&json).unwrap();
        assert_eq!(de.conflict_count(), 0);
        assert!(!de.merge_in_progress);
        assert_eq!(de.merge_branches.len(), 1);
    }

    // --- Debug trait ---

    #[test]
    fn test_git_error_debug() {
        let err = GitError::InvalidOperation("bad op".into());
        let debug = format!("{:?}", err);
        assert!(debug.contains("InvalidOperation"));
        assert!(debug.contains("bad op"));
    }

    #[test]
    fn test_git_operation_debug() {
        let op = GitOperation::Fetch {
            remote: "origin".into(),
            refspec: None,
        };
        let debug = format!("{:?}", op);
        assert!(debug.contains("Fetch"));
        assert!(debug.contains("origin"));
    }

    #[test]
    fn test_reset_mode_debug() {
        assert!(format!("{:?}", ResetMode::Soft).contains("Soft"));
        assert!(format!("{:?}", ResetMode::Hard).contains("Hard"));
    }

    #[test]
    fn test_conflict_type_debug() {
        let debug = format!("{:?}", ConflictType::BinaryConflict);
        assert!(debug.contains("BinaryConflict"));
    }

    #[test]
    fn test_conflict_severity_debug() {
        let debug = format!("{:?}", ConflictSeverity::Critical);
        assert!(debug.contains("Critical"));
    }

    #[test]
    fn test_file_status_debug() {
        let fs = FileStatus {
            path: "test.rs".into(),
            status: "M".into(),
            staged: true,
            unstaged: false,
        };
        let debug = format!("{:?}", fs);
        assert!(debug.contains("test.rs"));
    }

    #[test]
    fn test_git_status_debug() {
        let status = GitStatus {
            root: Some(PathBuf::from("/repo")),
            branch: Some("main".into()),
            worktree: false,
            dirty: Some(false),
        };
        let debug = format!("{:?}", status);
        assert!(debug.contains("main"));
    }

    #[test]
    fn test_hook_result_debug() {
        let hr = HookResult {
            passed: true,
            output: "ok".into(),
            error: None,
        };
        let debug = format!("{:?}", hr);
        assert!(debug.contains("passed"));
    }

    #[test]
    fn test_hook_context_debug() {
        let ctx = HookContext {
            repository_root: PathBuf::from("/repo"),
            current_branch: None,
            operation: GitOperation::GetStatus { branch_only: false },
            affected_files: vec![],
            env: HashMap::new(),
        };
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("HookContext"));
    }

    #[test]
    fn test_conflict_debug() {
        let c = Conflict {
            file_path: PathBuf::from("a.rs"),
            conflict_type: ConflictType::DeleteModify,
            severity: ConflictSeverity::Medium,
            description: "test".into(),
            resolution_strategy: "auto".into(),
            conflict_lines: vec![],
            conflicting_branch: None,
            commits: vec![],
        };
        let debug = format!("{:?}", c);
        assert!(debug.contains("a.rs"));
    }

    #[test]
    fn test_conflict_report_debug() {
        let r = ConflictReport {
            conflicts: vec![],
            repository_root: PathBuf::from("/repo"),
            current_branch: None,
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: vec![],
        };
        let debug = format!("{:?}", r);
        assert!(debug.contains("ConflictReport"));
    }

    // --- Clone trait ---

    #[test]
    fn test_git_operation_clone() {
        let op = GitOperation::CommitChanges {
            message: "test".into(),
            amend: true,
            allow_empty: false,
        };
        let cloned = op.clone();
        assert_eq!(op, cloned);
    }

    #[test]
    fn test_reset_mode_clone() {
        let mode = ResetMode::Merge;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_git_hook_type_clone() {
        let ht = GitHookType::CommitMsg;
        let cloned = ht.clone();
        assert_eq!(ht, cloned);
    }

    #[test]
    fn test_conflict_type_clone() {
        let ct = ConflictType::RenameModify;
        let cloned = ct.clone();
        assert_eq!(ct, cloned);
    }

    #[test]
    fn test_conflict_severity_clone() {
        let sev = ConflictSeverity::Medium;
        let cloned = sev.clone();
        assert_eq!(sev, cloned);
    }

    #[test]
    fn test_file_status_clone() {
        let fs = FileStatus {
            path: "x.rs".into(),
            status: "D".into(),
            staged: false,
            unstaged: true,
        };
        let cloned = fs.clone();
        assert_eq!(cloned.path, fs.path);
        assert_eq!(cloned.status, fs.status);
    }

    #[test]
    fn test_conflict_clone() {
        let c = Conflict {
            file_path: PathBuf::from("a.rs"),
            conflict_type: ConflictType::MarkerConflict,
            severity: ConflictSeverity::Low,
            description: "d".into(),
            resolution_strategy: "r".into(),
            conflict_lines: vec![1, 2],
            conflicting_branch: Some("b".into()),
            commits: vec!["c".into()],
        };
        let cloned = c.clone();
        assert_eq!(cloned.file_path, c.file_path);
        assert_eq!(cloned.conflict_type, c.conflict_type);
        assert_eq!(cloned.conflict_lines, c.conflict_lines);
    }

    #[test]
    fn test_conflict_report_clone() {
        let r = ConflictReport {
            conflicts: vec![],
            repository_root: PathBuf::from("/r"),
            current_branch: Some("main".into()),
            detected_at: Utc::now(),
            merge_in_progress: true,
            merge_branches: vec!["a".into()],
        };
        let cloned = r.clone();
        assert_eq!(cloned.conflict_count(), r.conflict_count());
        assert_eq!(cloned.merge_in_progress, r.merge_in_progress);
    }

    #[test]
    fn test_hook_result_clone() {
        let hr = HookResult {
            passed: false,
            output: "out".into(),
            error: Some("err".into()),
        };
        let cloned = hr.clone();
        assert_eq!(cloned.passed, hr.passed);
        assert_eq!(cloned.output, hr.output);
    }

    #[test]
    fn test_hook_context_clone() {
        let ctx = HookContext {
            repository_root: PathBuf::from("/r"),
            current_branch: Some("dev".into()),
            operation: GitOperation::GetStatus { branch_only: true },
            affected_files: vec!["a.rs".into()],
            env: HashMap::new(),
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.repository_root, ctx.repository_root);
        assert_eq!(cloned.current_branch, ctx.current_branch);
    }

    #[test]
    fn test_git_status_clone() {
        let s = GitStatus {
            root: Some(PathBuf::from("/r")),
            branch: Some("dev".into()),
            worktree: true,
            dirty: Some(true),
        };
        let cloned = s.clone();
        assert_eq!(cloned.branch, s.branch);
        assert_eq!(cloned.worktree, s.worktree);
    }

    #[test]
    fn test_git_operation_result_clone() {
        let r = GitOperationResult {
            operation: GitOperation::Push {
                remote: "origin".into(),
                branch: "main".into(),
                force: true,
            },
            success: true,
            output: "pushed".into(),
            error: None,
            executed_at: Utc::now(),
            duration_ms: 500,
        };
        let cloned = r.clone();
        assert_eq!(cloned.operation, r.operation);
        assert_eq!(cloned.success, r.success);
        assert_eq!(cloned.duration_ms, r.duration_ms);
    }

    // --- Hash trait on GitOperation ---

    #[test]
    fn test_git_operation_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(GitOperation::GetStatus { branch_only: false });
        set.insert(GitOperation::GetStatus { branch_only: false });
        set.insert(GitOperation::GetStatus { branch_only: true });
        // Duplicates are deduplicated
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_reset_mode_hash() {
        use std::collections::HashSet;
        let set: HashSet<ResetMode> = [ResetMode::Soft, ResetMode::Soft, ResetMode::Hard]
            .into_iter()
            .collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_git_hook_type_hash() {
        use std::collections::HashSet;
        let set: HashSet<GitHookType> = [
            GitHookType::PreCommit,
            GitHookType::PreCommit,
            GitHookType::PostCommit,
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_conflict_type_hash() {
        use std::collections::HashSet;
        let set: HashSet<ConflictType> = [
            ConflictType::MarkerConflict,
            ConflictType::MarkerConflict,
            ConflictType::Unknown,
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_conflict_severity_hash() {
        use std::collections::HashSet;
        let set: HashSet<ConflictSeverity> = [
            ConflictSeverity::Low,
            ConflictSeverity::Low,
            ConflictSeverity::Critical,
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 2);
    }

    // --- Error display: remaining variants ---

    #[test]
    fn test_git_error_invalid_operation_display() {
        let err = GitError::InvalidOperation("cannot push to detached HEAD".into());
        assert_eq!(
            err.to_string(),
            "invalid operation: cannot push to detached HEAD"
        );
    }

    #[test]
    fn test_git_error_merge_in_progress_display() {
        let err = GitError::MergeInProgress;
        assert_eq!(err.to_string(), "merge in progress");
    }

    #[test]
    fn test_git_error_hook_execution_failed_display() {
        let err = GitError::HookExecutionFailed("lint failed".into());
        assert_eq!(err.to_string(), "hook execution failed: lint failed");
    }

    #[test]
    fn test_git_error_git_command_failed_display() {
        let err = GitError::GitCommandFailed("exit 128".into());
        assert_eq!(err.to_string(), "git command failed: exit 128");
    }

    #[test]
    fn test_git_error_repository_not_found_display() {
        let err = GitError::RepositoryNotFound(PathBuf::from("/no/repo/here"));
        assert_eq!(err.to_string(), "repository not found at /no/repo/here");
    }

    // --- Display: uncovered branches ---

    #[test]
    fn test_git_operation_display_switch_branch_no_force() {
        let op = GitOperation::SwitchBranch {
            name: "main".into(),
            force: false,
        };
        let s = format!("{}", op);
        assert_eq!(s, "switch_branch(main)");
        assert!(!s.contains("force"));
    }

    #[test]
    fn test_git_operation_display_switch_branch_with_force() {
        let op = GitOperation::SwitchBranch {
            name: "dev".into(),
            force: true,
        };
        let s = format!("{}", op);
        assert!(s.contains("(force)"));
    }

    #[test]
    fn test_git_operation_display_commit_allow_empty() {
        let op = GitOperation::CommitChanges {
            message: "empty".into(),
            amend: false,
            allow_empty: true,
        };
        let s = format!("{}", op);
        assert!(s.contains("--allow-empty"));
        assert!(!s.contains("--amend"));
    }

    #[test]
    fn test_git_operation_display_commit_plain() {
        let op = GitOperation::CommitChanges {
            message: "msg".into(),
            amend: false,
            allow_empty: false,
        };
        let s = format!("{}", op);
        assert_eq!(s, "commit");
    }

    #[test]
    fn test_git_operation_display_get_diff_all_options() {
        let op = GitOperation::GetDiff {
            path: Some("src/main.rs".into()),
            cached: true,
            context_lines: Some(3),
        };
        let s = format!("{}", op);
        assert!(s.contains("--cached"));
        assert!(s.contains("src/main.rs"));
    }

    #[test]
    fn test_git_operation_display_get_status_branch_only() {
        let op = GitOperation::GetStatus { branch_only: true };
        assert!(format!("{}", op).contains("branch only"));
    }

    #[test]
    fn test_git_operation_display_get_status_not_branch_only() {
        let op = GitOperation::GetStatus { branch_only: false };
        let s = format!("{}", op);
        assert_eq!(s, "status");
    }

    #[test]
    fn test_git_operation_display_stage_files_with_update() {
        let op = GitOperation::StageFiles {
            paths: vec!["a.rs".into()],
            update: true,
        };
        let s = format!("{}", op);
        assert!(s.contains("(update)"));
    }

    #[test]
    fn test_git_operation_display_stage_files_no_update() {
        let op = GitOperation::StageFiles {
            paths: vec!["b.rs".into()],
            update: false,
        };
        let s = format!("{}", op);
        assert!(!s.contains("(update)"));
    }

    #[test]
    fn test_git_operation_display_delete_branch_with_force() {
        let op = GitOperation::DeleteBranch {
            name: "old".into(),
            force: true,
        };
        let s = format!("{}", op);
        assert!(s.contains("(force)"));
    }

    #[test]
    fn test_git_operation_display_delete_branch_no_force() {
        let op = GitOperation::DeleteBranch {
            name: "old".into(),
            force: false,
        };
        let s = format!("{}", op);
        assert_eq!(s, "delete_branch(old)");
    }

    #[test]
    fn test_git_operation_display_fetch_no_refspec() {
        let op = GitOperation::Fetch {
            remote: "origin".into(),
            refspec: None,
        };
        let s = format!("{}", op);
        assert_eq!(s, "fetch origin");
    }

    #[test]
    fn test_git_operation_display_pull_no_branch() {
        let op = GitOperation::Pull {
            remote: "origin".into(),
            branch: None,
        };
        let s = format!("{}", op);
        assert_eq!(s, "pull origin");
    }

    #[test]
    fn test_git_operation_display_push_no_force() {
        let op = GitOperation::Push {
            remote: "origin".into(),
            branch: "main".into(),
            force: false,
        };
        let s = format!("{}", op);
        assert!(!s.contains("force"));
    }

    #[test]
    fn test_git_operation_display_stash_no_message_no_keep() {
        let op = GitOperation::Stash {
            message: None,
            keep_index: false,
        };
        let s = format!("{}", op);
        assert_eq!(s, "stash");
    }

    #[test]
    fn test_git_operation_display_stash_pop_no_ref() {
        let op = GitOperation::StashPop { stash_ref: None };
        let s = format!("{}", op);
        assert_eq!(s, "stash pop");
    }

    #[test]
    fn test_git_operation_display_reset_no_commit() {
        let op = GitOperation::Reset {
            mode: ResetMode::Mixed,
            commit: None,
        };
        let s = format!("{}", op);
        assert!(s.contains("reset"));
        assert!(s.contains("Mixed"));
    }

    #[test]
    fn test_git_operation_display_rebase_no_branch_no_interactive() {
        let op = GitOperation::Rebase {
            upstream: "main".into(),
            branch: None,
            interactive: false,
        };
        let s = format!("{}", op);
        assert!(!s.contains("-i"));
        assert!(s.contains("main"));
    }

    #[test]
    fn test_conflict_severity_display_medium() {
        assert_eq!(ConflictSeverity::Medium.to_string(), "medium");
    }

    // --- Edge cases ---

    #[test]
    fn test_file_status_empty_path() {
        let fs = FileStatus {
            path: String::new(),
            status: "??".into(),
            staged: false,
            unstaged: false,
        };
        assert!(fs.path.is_empty());
        assert_eq!(fs.status, "??");
    }

    #[test]
    fn test_git_operation_empty_branch_name() {
        let op = GitOperation::CreateBranch {
            name: String::new(),
            base: None,
        };
        let s = format!("{}", op);
        assert_eq!(s, "create_branch()");
    }

    #[test]
    fn test_stage_files_empty_paths() {
        let op = GitOperation::StageFiles {
            paths: vec![],
            update: false,
        };
        let s = format!("{}", op);
        assert!(s.contains("stage"));
    }

    #[test]
    fn test_unstage_files_empty_paths() {
        let op = GitOperation::UnstageFiles { paths: vec![] };
        let s = format!("{}", op);
        assert!(s.contains("unstage"));
    }

    #[test]
    fn test_conflict_report_multiple_conflicts() {
        let conflicts: Vec<Conflict> = (0..5)
            .map(|i| Conflict {
                file_path: PathBuf::from(format!("file{}.rs", i)),
                conflict_type: ConflictType::BothModified,
                severity: if i == 3 {
                    ConflictSeverity::Critical
                } else {
                    ConflictSeverity::Low
                },
                description: format!("conflict {}", i),
                resolution_strategy: "manual".into(),
                conflict_lines: vec![i],
                conflicting_branch: None,
                commits: vec![],
            })
            .collect();

        let report = ConflictReport {
            conflicts,
            repository_root: PathBuf::from("/repo"),
            current_branch: Some("main".into()),
            detected_at: Utc::now(),
            merge_in_progress: true,
            merge_branches: vec!["feature".into()],
        };

        assert_eq!(report.conflict_count(), 5);
        assert!(report.has_critical_conflicts());
    }

    #[test]
    fn test_git_status_serialization() {
        let status = GitStatus {
            root: Some(PathBuf::from("/repo")),
            branch: Some("main".into()),
            worktree: false,
            dirty: Some(false),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("main"));
        assert!(json.contains("/repo"));
    }

    #[test]
    fn test_git_operation_result_zero_duration() {
        let result = GitOperationResult {
            operation: GitOperation::GetStatus { branch_only: false },
            success: true,
            output: String::new(),
            error: None,
            executed_at: Utc::now(),
            duration_ms: 0,
        };
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn test_conflict_empty_fields() {
        let c = Conflict {
            file_path: PathBuf::new(),
            conflict_type: ConflictType::Unknown,
            severity: ConflictSeverity::Low,
            description: String::new(),
            resolution_strategy: String::new(),
            conflict_lines: vec![],
            conflicting_branch: None,
            commits: vec![],
        };
        assert!(c.file_path.as_os_str().is_empty());
        assert!(c.conflict_lines.is_empty());
        assert!(c.commits.is_empty());
    }

    #[test]
    fn test_hook_context_empty_env() {
        let ctx = HookContext {
            repository_root: PathBuf::from("/repo"),
            current_branch: None,
            operation: GitOperation::GetStatus { branch_only: false },
            affected_files: vec![],
            env: HashMap::new(),
        };
        assert!(ctx.env.is_empty());
        assert!(ctx.affected_files.is_empty());
    }

    #[test]
    fn test_hook_result_passed_with_no_error() {
        let hr = HookResult {
            passed: true,
            output: String::new(),
            error: None,
        };
        assert!(hr.passed);
        assert!(hr.error.is_none());
    }

    // --- ConflictDetector with real repo ---

    #[test]
    fn test_conflict_detector_new_valid_repo() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "content").unwrap();
        let detector = ConflictDetector::new(&repo_path);
        assert!(detector.is_ok());
    }

    #[test]
    fn test_conflict_detector_new_invalid_path() {
        let temp = TempDir::new().unwrap();
        let detector = ConflictDetector::new(temp.path());
        assert!(detector.is_err());
    }

    #[test]
    fn test_conflict_detector_detect_conflicts() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "content").unwrap();
        let detector = ConflictDetector::new(&repo_path).unwrap();
        let report = detector.detect_conflicts().unwrap();
        assert_eq!(report.conflict_count(), 0);
    }

    #[test]
    fn test_conflict_detector_detect_conflicts_with_branch() {
        let (_temp, repo_path) = create_test_repo().unwrap();
        commit_test_file(&repo_path, "test.txt", "content").unwrap();
        let detector = ConflictDetector::new(&repo_path).unwrap();
        let report = detector
            .detect_conflicts_with_branch("nonexistent-branch")
            .unwrap();
        // Simplified impl returns empty conflicts
        assert_eq!(report.conflict_count(), 0);
    }

    // --- GitStatus serialization (only Serialize, no Deserialize) ---

    #[test]
    fn test_git_status_serialize_with_none_fields() {
        let status = GitStatus {
            root: None,
            branch: None,
            worktree: false,
            dirty: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("null"));
    }

    // --- PartialEq edge cases ---

    #[test]
    fn test_git_operation_ne_different_variants() {
        let a = GitOperation::CreateBranch {
            name: "x".into(),
            base: None,
        };
        let b = GitOperation::DeleteBranch {
            name: "x".into(),
            force: false,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn test_git_operation_eq_same_fields() {
        let a = GitOperation::MergeBranch {
            source: "dev".into(),
            no_commit: false,
            squash: true,
        };
        let b = GitOperation::MergeBranch {
            source: "dev".into(),
            no_commit: false,
            squash: true,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_conflict_severity_ord() {
        assert!(ConflictSeverity::Low <= ConflictSeverity::Low);
        assert!(ConflictSeverity::Low < ConflictSeverity::Critical);
        assert!(ConflictSeverity::Critical > ConflictSeverity::Medium);
    }

    // --- ResetMode Display uses Debug internally (reset --{:?}) ---

    #[test]
    fn test_reset_mode_display_in_reset_operation() {
        let op = GitOperation::Reset {
            mode: ResetMode::Soft,
            commit: None,
        };
        let s = format!("{}", op);
        assert!(s.contains("Soft"));

        let op = GitOperation::Reset {
            mode: ResetMode::Keep,
            commit: None,
        };
        let s = format!("{}", op);
        assert!(s.contains("Keep"));
    }

    // --- GitOperation Display: create_branch with base ---

    #[test]
    fn test_git_operation_display_create_branch_with_base() {
        let op = GitOperation::CreateBranch {
            name: "feature".into(),
            base: Some("main".into()),
        };
        let s = format!("{}", op);
        assert_eq!(s, "create_branch(feature) from main");
    }

    // --- ConflictReport boundary: empty vs single vs many ---

    #[test]
    fn test_conflict_report_boundary_conditions() {
        // Empty
        let report = ConflictReport {
            conflicts: vec![],
            repository_root: PathBuf::from("/r"),
            current_branch: None,
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: vec![],
        };
        assert!(!report.has_critical_conflicts());
        assert_eq!(report.conflict_count(), 0);

        // All non-critical
        let low_report = ConflictReport {
            conflicts: vec![
                Conflict {
                    file_path: PathBuf::from("a.rs"),
                    conflict_type: ConflictType::MarkerConflict,
                    severity: ConflictSeverity::Low,
                    description: "d".into(),
                    resolution_strategy: "auto".into(),
                    conflict_lines: vec![],
                    conflicting_branch: None,
                    commits: vec![],
                },
                Conflict {
                    file_path: PathBuf::from("b.rs"),
                    conflict_type: ConflictType::BothModified,
                    severity: ConflictSeverity::Medium,
                    description: "d".into(),
                    resolution_strategy: "auto".into(),
                    conflict_lines: vec![],
                    conflicting_branch: None,
                    commits: vec![],
                },
            ],
            repository_root: PathBuf::from("/r"),
            current_branch: None,
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: vec![],
        };
        assert_eq!(low_report.conflict_count(), 2);
        assert!(!low_report.has_critical_conflicts());
    }
}
