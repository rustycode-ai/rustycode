//! Branch operations for `GitClient`.

use anyhow::Result;

use crate::hooks::GitHookType;
use crate::util::git_output;
use crate::{GitClient, GitError, GitOperation, GitOperationResult};

impl GitClient {
    /// Create a new branch
    pub fn create_branch(&self, name: &str, base: Option<&str>) -> Result<GitOperationResult> {
        let operation = GitOperation::CreateBranch {
            name: name.to_string(),
            base: base.map(ToString::to_string),
        };

        let start_time = std::time::Instant::now();

        // Build git command
        let mut args = vec!["branch", name];
        if let Some(base) = base {
            args.push(base);
        }

        let result = self.execute_git_command(&args, &operation, start_time)?;

        Ok(result)
    }

    /// Switch to a branch
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

        if result.success {
            let context = self.create_hook_context(operation, vec![]);
            self.execute_hooks(GitHookType::PostCheckout, &context)?;
        }

        Ok(result)
    }

    /// Delete a branch
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
    pub fn list_branches(&self) -> Result<Vec<String>> {
        let output = git_output(
            &self.repository_root,
            &["branch", "--format=%(refname:short)"],
        )?;
        Ok(output.lines().map(ToString::to_string).collect())
    }

    /// Check if a branch exists
    pub(crate) fn branch_exists(&self, name: &str) -> Result<bool> {
        Ok(git_output(
            &self.repository_root,
            &["rev-parse", "--verify", &format!("refs/heads/{name}")],
        )
        .is_ok())
    }
}
