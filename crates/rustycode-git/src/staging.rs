//! Staging operations for `GitClient`.

use anyhow::Result;

use crate::{GitClient, GitOperation, GitOperationResult};

impl GitClient {
    /// Stage files for commit
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
    pub fn stage_all(&self) -> Result<GitOperationResult> {
        let operation = GitOperation::StageFiles {
            paths: vec![".".to_string()],
            update: false,
        };

        let start_time = std::time::Instant::now();
        self.execute_git_command(&["add", "."], &operation, start_time)
    }

    /// Unstage files
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
    pub fn unstage_all(&self) -> Result<GitOperationResult> {
        let operation = GitOperation::UnstageFiles {
            paths: vec![".".to_string()],
        };

        let start_time = std::time::Instant::now();
        self.execute_git_command(&["reset", "HEAD", "."], &operation, start_time)
    }
}
