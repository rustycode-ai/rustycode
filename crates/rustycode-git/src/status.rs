//! Status operations and types for `GitClient`.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::util::git_output;
use crate::GitClient;

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

impl GitClient {
    /// Get repository status
    pub fn status(&self) -> Result<crate::GitStatus> {
        crate::util::inspect(&self.repository_root)
    }

    /// Get detailed status with file changes
    pub fn status_files(&self) -> Result<Vec<FileStatus>> {
        let output = git_output(&self.repository_root, &["status", "--porcelain"])?;
        let mut files = Vec::new();

        for line in output.lines() {
            if line.len() >= 3 {
                let status = line.chars().take(2).collect::<String>();
                let raw_path = line[3..].trim();
                let path = if status.starts_with('R') || status.starts_with('r') {
                    raw_path.split("->").last().unwrap_or(raw_path).trim()
                } else {
                    raw_path
                };
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
}
