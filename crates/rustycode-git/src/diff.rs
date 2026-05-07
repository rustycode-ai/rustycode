//! Diff operations for `GitClient`.

use anyhow::Result;

use crate::util::git_output;
use crate::GitClient;

impl GitClient {
    /// Get diff output
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
    pub fn diff_commits(&self, from: &str, to: &str, path: Option<&str>) -> Result<String> {
        let mut args = vec!["diff", from, to];
        if let Some(path) = path {
            args.push("--");
            args.push(path);
        }

        git_output(&self.repository_root, &args)
    }
}
