//! Git utility functions and status types.

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run a git command and return its stdout output.
pub(crate) fn git_output(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Overall repository status.
#[derive(Debug, Clone, Serialize)]
pub struct GitStatus {
    pub root: Option<PathBuf>,
    pub branch: Option<String>,
    pub worktree: bool,
    pub dirty: Option<bool>,
}

/// Inspect the git repository at `cwd` and return its status.
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

/// Find the root of the git repository containing `path`.
pub(crate) fn find_repository_root(path: &Path) -> Result<PathBuf> {
    let root_str = git_output(path, &["rev-parse", "--show-toplevel"])
        .context("Not in a git repository")?;
    Ok(PathBuf::from(root_str.trim()))
}
