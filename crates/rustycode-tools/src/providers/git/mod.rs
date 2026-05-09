use crate::security::validate_read_path;
use crate::{ToolContext, ToolOutput};
use anyhow::Result;
use schemars::JsonSchema;
use serde_json::json;
use std::process::Command;

// Re-export all tools
pub use commit::*;
pub use diff::*;
pub use log::*;
pub use status::*;

pub mod commit;
pub mod diff;
pub mod log;
pub mod status;

// ── Params structs ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize, JsonSchema)]
pub struct GitStatusParams {}

#[derive(serde::Deserialize, JsonSchema)]
pub struct GitDiffParams {
    /// Show staged (cached) diff (default false)
    #[serde(default)]
    pub staged: bool,
    /// Optional path to show diff for (alias: file_path)
    pub path: Option<String>,
    /// Alias for path
    pub file_path: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct GitCommitParams {
    /// Commit message
    pub message: String,
    /// Files to stage before committing (omit to commit already-staged changes)
    pub files: Option<Vec<String>>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct GitLogParams {
    /// Maximum number of commits to show (default: 10, max: 1000)
    pub limit: Option<u64>,
}

pub(crate) fn run_git(ctx: &ToolContext, args: &[&str]) -> Result<ToolOutput> {
    let output = Command::new("git")
        .args(args)
        .current_dir(&ctx.cwd)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    anyhow::ensure!(output.status.success(), stderr.trim().to_string());

    let metadata = json!({
        "args": args,
        "stdout": stdout.clone(),
        "stderr": stderr,
        "exit_code": output.status.code().unwrap_or(-1)
    });

    Ok(ToolOutput::with_structured(stdout, metadata))
}

#[cfg(test)]
pub(crate) mod tests_common {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper to create a test git repository
    pub fn create_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let repo_path = dir.path();

        // Initialize git repo with explicit branch name to avoid
        // default-branch differences across git versions (master vs main)
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to init git repo");

        // Configure git user
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to configure git user.email");

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to configure git user.name");

        // Create initial commit
        let readme_path = repo_path.join("README.md");
        let mut file = File::create(&readme_path).unwrap();
        writeln!(file, "# Test Repository").unwrap();

        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to add README.md");

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to create initial commit");

        dir
    }

    /// Helper to create a ToolContext from a path
    pub fn create_context(path: &PathBuf) -> ToolContext {
        ToolContext::new(path)
    }
}
