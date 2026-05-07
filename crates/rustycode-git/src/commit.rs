//! Commit operations for `GitClient`.

use anyhow::Result;

use crate::hooks::GitHookType;
use crate::{GitClient, GitOperation, GitOperationResult};

impl GitClient {
    /// Create a commit
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
}
