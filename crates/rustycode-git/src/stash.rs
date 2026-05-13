//! Stash, reset, and rebase operations for `GitClient`.

use anyhow::{Context, Result};
use chrono::Utc;

use crate::hooks::GitHookType;
use crate::util::git_output;
use crate::{GitClient, GitOperation, GitOperationResult, ResetMode};

impl GitClient {
    /// Stash changes
    pub fn stash(&self, message: Option<&str>, keep_index: bool) -> Result<GitOperationResult> {
        let operation = GitOperation::Stash {
            message: message.map(ToString::to_string),
            keep_index,
        };

        let start_time = std::time::Instant::now();

        let mut args = vec!["stash", "push"];
        if keep_index {
            args.push("--keep-index");
        }
        if let Some(message) = message {
            args.push("-m");
            args.push(message);
        }

        self.execute_git_command(&args, &operation, start_time)
    }

    /// Pop the most recent stash
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

    /// Reset the repository
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

    /// Rebase the current branch
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

    /// Get the current branch name
    pub fn current_branch(&self) -> Result<String> {
        let output = git_output(
            &self.repository_root,
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )?;
        Ok(output.trim().to_string())
    }

    /// Get the current HEAD commit hash
    pub fn current_commit(&self) -> Result<String> {
        let hash = git_output(&self.repository_root, &["rev-parse", "HEAD"])?;
        Ok(hash)
    }

    /// Get list of modified files in the working directory
    pub fn modified_files(&self) -> Result<Vec<String>> {
        let status_files = self.status_files()?;
        Ok(status_files.iter().map(|f| f.path.clone()).collect())
    }

    /// Reset repository to a specific commit
    ///
    /// This performs a hard reset, discarding all uncommitted changes.
    pub fn reset_to_commit(&self, commit_hash: &str) -> Result<GitOperationResult> {
        self.reset(ResetMode::Hard, Some(commit_hash))
    }

    /// Execute a git command and return the result
    pub(crate) fn execute_git_command(
        &self,
        args: &[&str],
        operation: &GitOperation,
        start_time: std::time::Instant,
    ) -> Result<GitOperationResult> {
        let output = std::process::Command::new("git")
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
}
