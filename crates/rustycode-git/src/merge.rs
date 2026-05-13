//! Merge operations for `GitClient`.

use anyhow::Result;

use crate::hooks::GitHookType;
use crate::util::git_output;
use crate::{GitClient, GitError, GitOperation, GitOperationResult};

impl GitClient {
    /// Merge a branch
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

        if result.success {
            self.execute_hooks(GitHookType::PostMerge, &context)?;
        }

        Ok(result)
    }

    /// Abort the current merge
    pub fn abort_merge(&self) -> Result<()> {
        git_output(&self.repository_root, &["merge", "--abort"])?;
        Ok(())
    }

    /// Continue the current merge after resolving conflicts
    pub fn continue_merge(&self) -> Result<()> {
        git_output(&self.repository_root, &["merge", "--continue"])?;
        Ok(())
    }
}
