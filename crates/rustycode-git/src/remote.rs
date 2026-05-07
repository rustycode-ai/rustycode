//! Remote operations (fetch, pull, push) for `GitClient`.

use anyhow::Result;

use crate::hooks::GitHookType;
use crate::{GitClient, GitOperation, GitOperationResult};

impl GitClient {
    /// Fetch from a remote
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
}
