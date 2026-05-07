use crate::security::validate_read_path;
use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::process::Command;

pub struct GitStatusTool;
pub struct GitDiffTool;
pub struct GitCommitTool;
pub struct GitLogTool;

impl Tool for GitStatusTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "git_status"
    }

    fn description(&self) -> &'static str {
        "Show git status for current workspace."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, _params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let result = run_git(ctx, &["status", "--short", "--branch"])?;

        // Parse git status output for structured metadata
        let lines: Vec<&str> = result.text.lines().collect();
        let branch = lines
            .first()
            .and_then(|l| l.strip_prefix("## "))
            .unwrap_or("unknown");

        let mut staged = Vec::new();
        let mut modified = Vec::new();
        let mut untracked = Vec::new();

        for line in lines.iter().skip(1) {
            if line.len() < 4 {
                continue;
            }
            let status = line.chars().take(2).collect::<String>();
            let path = line[3..].trim();

            // First char: staged status, second char: unstaged status
            match status.chars().next() {
                Some('M') => staged.push(path),
                Some('A') => staged.push(path),
                Some('D') => staged.push(path),
                Some('R') => staged.push(path),
                _ => {}
            }

            match status.chars().nth(1) {
                Some('M') => modified.push(path),
                Some('D') => modified.push(path),
                Some('?') => untracked.push(path),
                _ => {}
            }
        }

        // Build structured metadata
        let mut structured = result.structured.unwrap_or(json!({}));
        structured["branch"] = json!(branch);
        if !staged.is_empty() {
            structured["staged"] = json!(staged);
        }
        if !modified.is_empty() {
            structured["modified"] = json!(modified);
        }
        if !untracked.is_empty() {
            structured["untracked"] = json!(untracked);
        }
        structured["has_changes"] =
            json!(!staged.is_empty() || !modified.is_empty() || !untracked.is_empty());

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

impl Tool for GitDiffTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "git_diff"
    }

    fn description(&self) -> &'static str {
        "Show git diff, optionally staged and/or for a specific path."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "staged": { "type": "boolean", "description": "Show staged (cached) diff (default false)" },
                "path": { "type": "string", "description": "Optional path to show diff for (alias: file_path)" },
                "file_path": { "type": "string", "description": "Alias for path" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let staged = params
            .get("staged")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut args = vec!["diff", "--numstat"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        if let Some(path) = params
            .get("path")
            .or_else(|| params.get("file_path"))
            .and_then(Value::as_str)
        {
            // Validate path is within workspace
            validate_read_path(path, &ctx.cwd, !ctx.allow_outside_workspace)?;
            args.push(path);
        }

        // Get numstat output for structured metadata
        let numstat_output = Command::new("git")
            .args(&args)
            .current_dir(&ctx.cwd)
            .output()?;

        let mut files_changed = Vec::new();
        let mut total_additions = 0;
        let mut total_deletions = 0;

        if numstat_output.status.success() {
            let numstat = String::from_utf8_lossy(&numstat_output.stdout);
            for line in numstat.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3 {
                    let additions = parts[0].parse::<u32>().unwrap_or(0);
                    let deletions = parts[1].parse::<u32>().unwrap_or(0);
                    let file = parts[2];

                    if additions > 0 || deletions > 0 {
                        total_additions += additions;
                        total_deletions += deletions;
                        files_changed.push(json!({
                            "path": file,
                            "additions": additions,
                            "deletions": deletions
                        }));
                    }
                }
            }
        }

        // Get the actual diff output
        let mut diff_args = vec!["diff"];
        if staged {
            diff_args.push("--cached");
        }
        diff_args.push("--");
        if let Some(path) = params.get("path").and_then(Value::as_str) {
            diff_args.push(path);
        }
        let result = run_git(ctx, &diff_args)?;

        // Build structured metadata
        let mut structured = result.structured.unwrap_or(json!({}));
        structured["staged"] = json!(staged);
        if !files_changed.is_empty() {
            structured["files_changed"] = json!(files_changed.len());
            structured["total_additions"] = json!(total_additions);
            structured["total_deletions"] = json!(total_deletions);
            structured["changes"] = json!(files_changed);
        }

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

impl Tool for GitCommitTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "git_commit"
    }

    fn description(&self) -> &'static str {
        "Stage files and create a git commit with provided message."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": { "type": "string" },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Files to stage before committing (omit to commit already-staged changes)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let message = params
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing string parameter 'message'"))?;

        let staged_files = if let Some(files) = params.get("files").and_then(Value::as_array) {
            let paths: Vec<&str> = files.iter().filter_map(|v| v.as_str()).collect();
            // Validate all file paths are within workspace
            for p in &paths {
                validate_read_path(p, &ctx.cwd, !ctx.allow_outside_workspace)?;
            }
            if !paths.is_empty() {
                let mut add_args = vec!["add", "--"];
                add_args.extend_from_slice(&paths);
                run_git(ctx, &add_args)?;
                Some(paths)
            } else {
                None
            }
        } else {
            None
        };

        let result = run_git(ctx, &["commit", "-m", message])?;

        // Get the commit SHA that was just created
        let rev_parse = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&ctx.cwd)
            .output();

        let mut structured = result.structured.unwrap_or(json!({}));
        if let Ok(rev_output) = rev_parse {
            if rev_output.status.success() {
                let sha = String::from_utf8_lossy(&rev_output.stdout)
                    .trim()
                    .to_string();
                structured["commit_sha"] = json!(sha);
            }
        }

        if let Some(files) = staged_files {
            structured["staged_files"] = json!(files);
        }

        Ok(ToolOutput::with_structured(result.text, structured))
    }
}

impl Tool for GitLogTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "git_log"
    }

    fn description(&self) -> &'static str {
        "Show recent git commits."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "limit": { "type": "integer" } }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(10)
            .min(1000);
        let n = limit.to_string();
        let output = run_git(ctx, &["log", "--oneline", "--no-decorate", "-n", &n])?;
        let commits: Vec<Value> = output
            .text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let (sha, msg) = l.split_once(' ').unwrap_or((l, ""));
                json!({ "sha": sha, "message": msg })
            })
            .collect();
        Ok(ToolOutput::with_structured(
            output.text,
            json!({ "commits": commits }),
        ))
    }
}

fn run_git(ctx: &ToolContext, args: &[&str]) -> Result<ToolOutput> {
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
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper to create a test git repository
    fn create_test_repo() -> TempDir {
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
    fn create_context(path: &PathBuf) -> ToolContext {
        ToolContext::new(path)
    }

    // ============================================================================
    // GitStatusTool Tests
    // ============================================================================

    #[test]
    fn test_git_status_clean_repo() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitStatusTool;

        let result = tool.execute(json!({}), &ctx).unwrap();

        assert!(result.text.contains("## main"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["branch"], "main");
        assert_eq!(structured["has_changes"], false);
    }

    #[test]
    fn test_git_status_with_modified_files() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Modify a file
        let readme_path = repo.path().join("README.md");
        let mut file = File::create(&readme_path).unwrap();
        writeln!(file, "# Modified README").unwrap();

        let tool = GitStatusTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        assert!(result.text.contains("M README.md"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["branch"], "main");
        assert_eq!(structured["has_changes"], true);
        assert_eq!(structured["modified"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_git_status_with_staged_files() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create and stage a new file
        let new_file_path = repo.path().join("new.txt");
        let mut file = File::create(&new_file_path).unwrap();
        writeln!(file, "New file content").unwrap();

        Command::new("git")
            .args(["add", "new.txt"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to stage file");

        let tool = GitStatusTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        // Git status shows staged files as "A  filename" (A + two spaces + filename)
        assert!(result.text.contains("new.txt"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["has_changes"], true);
        assert_eq!(structured["staged"].as_array().unwrap().len(), 1);
        assert_eq!(structured["staged"].as_array().unwrap()[0], "new.txt");
    }

    #[test]
    fn test_git_status_with_untracked_files() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create untracked file
        let untracked_path = repo.path().join("untracked.txt");
        let mut file = File::create(&untracked_path).unwrap();
        writeln!(file, "Untracked content").unwrap();

        let tool = GitStatusTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        assert!(result.text.contains("?? untracked.txt"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["has_changes"], true);
        assert_eq!(structured["untracked"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_git_status_mixed_changes() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Modify README
        let readme_path = repo.path().join("README.md");
        let mut file = File::create(&readme_path).unwrap();
        writeln!(file, "# Modified README").unwrap();

        // Create new untracked file
        let untracked_path = repo.path().join("untracked.txt");
        let mut file = File::create(&untracked_path).unwrap();
        writeln!(file, "Untracked").unwrap();

        let tool = GitStatusTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        let structured = result.structured.unwrap();
        assert_eq!(structured["has_changes"], true);
        assert!(
            structured["modified"].as_array().is_some()
                || structured["untracked"].as_array().is_some()
        );
    }

    #[test]
    fn test_git_status_not_a_git_repo() {
        let dir = TempDir::new().unwrap();
        let ctx = create_context(&dir.path().to_path_buf());
        let tool = GitStatusTool;

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
    }

    // ============================================================================
    // GitDiffTool Tests
    // ============================================================================

    #[test]
    fn test_git_diff_no_changes() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitDiffTool;

        let result = tool.execute(json!({}), &ctx).unwrap();

        // Empty diff
        assert_eq!(result.text.trim(), "");
        let structured = result.structured.unwrap();
        assert_eq!(structured["staged"], false);
    }

    #[test]
    fn test_git_diff_with_changes() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Modify README
        let readme_path = repo.path().join("README.md");
        let mut file = File::create(&readme_path).unwrap();
        writeln!(file, "# Modified\n\nNew line").unwrap();

        let tool = GitDiffTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        assert!(result.text.contains("diff --git"));
        assert!(result.text.contains("README.md"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["staged"], false);
        assert!(structured["total_additions"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_git_diff_staged() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Modify and stage a file
        let readme_path = repo.path().join("README.md");
        let mut file = File::create(&readme_path).unwrap();
        writeln!(file, "# Modified\n\nNew line").unwrap();

        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to stage file");

        let tool = GitDiffTool;
        let result = tool.execute(json!({"staged": true}), &ctx).unwrap();

        assert!(result.text.contains("diff --git"));
        assert!(result.text.contains("a/README.md"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["staged"], true);
    }

    #[test]
    fn test_git_diff_specific_path() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create two files
        let file1_path = repo.path().join("file1.txt");
        let mut file = File::create(&file1_path).unwrap();
        writeln!(file, "File 1 content").unwrap();

        let file2_path = repo.path().join("file2.txt");
        let mut file = File::create(&file2_path).unwrap();
        writeln!(file, "File 2 content").unwrap();

        Command::new("git")
            .args(["add", "."])
            .current_dir(repo.path())
            .output()
            .expect("Failed to add files");

        Command::new("git")
            .args(["commit", "-m", "Add two files"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to commit");

        // Modify only file1
        let mut file = File::create(&file1_path).unwrap();
        writeln!(file, "Modified file 1").unwrap();

        let tool = GitDiffTool;
        let result = tool.execute(json!({"path": "file1.txt"}), &ctx).unwrap();

        assert!(result.text.contains("file1.txt"));
        assert!(!result.text.contains("file2.txt"));
    }

    #[test]
    fn test_git_diff_numstat_parsing() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Make changes with known additions/deletions
        let readme_path = repo.path().join("README.md");
        let mut file = File::create(&readme_path).unwrap();
        writeln!(file, "Line 1\nLine 2\nLine 3").unwrap();

        let tool = GitDiffTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        let structured = result.structured.unwrap();
        // Should have changes recorded
        if structured.get("total_additions").is_some() {
            assert!(structured["total_additions"].as_u64().unwrap() > 0);
        }
    }

    // ============================================================================
    // GitCommitTool Tests
    // ============================================================================

    #[test]
    fn test_git_commit_with_message() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create and stage a file
        let new_file_path = repo.path().join("test.txt");
        let mut file = File::create(&new_file_path).unwrap();
        writeln!(file, "Test content").unwrap();

        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to stage file");

        let tool = GitCommitTool;
        let result = tool
            .execute(json!({"message": "Test commit"}), &ctx)
            .unwrap();

        assert!(result.text.contains("Test commit") || result.text.contains("1 file changed"));

        let structured = result.structured.unwrap();
        assert!(structured["commit_sha"].is_string());
        assert!(structured["commit_sha"].as_str().unwrap().len() == 40);
    }

    #[test]
    fn test_git_commit_with_files() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create untracked file
        let new_file_path = repo.path().join("auto.txt");
        let mut file = File::create(&new_file_path).unwrap();
        writeln!(file, "Auto staged content").unwrap();

        let tool = GitCommitTool;
        let result = tool
            .execute(
                json!({
                    "message": "Auto commit",
                    "files": ["auto.txt"]
                }),
                &ctx,
            )
            .unwrap();

        assert!(result.text.contains("Auto commit") || result.text.contains("1 file changed"));

        let structured = result.structured.unwrap();
        assert!(structured["commit_sha"].is_string());
        assert_eq!(structured["staged_files"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_git_commit_multiple_files() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create multiple files
        for i in 1..=3 {
            let file_path = repo.path().join(format!("file{}.txt", i));
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "Content {}", i).unwrap();
        }

        let tool = GitCommitTool;
        let result = tool
            .execute(
                json!({
                    "message": "Commit multiple files",
                    "files": ["file1.txt", "file2.txt", "file3.txt"]
                }),
                &ctx,
            )
            .unwrap();

        let structured = result.structured.unwrap();
        assert_eq!(structured["staged_files"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_git_commit_missing_message() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitCommitTool;

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[test]
    fn test_git_commit_empty_files_array() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Stage a file manually
        let new_file_path = repo.path().join("test.txt");
        let mut file = File::create(&new_file_path).unwrap();
        writeln!(file, "Test content").unwrap();

        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to stage file");

        let tool = GitCommitTool;
        let result = tool
            .execute(
                json!({
                    "message": "Commit with empty files array",
                    "files": []
                }),
                &ctx,
            )
            .unwrap();

        // Should succeed and commit staged changes
        let structured = result.structured.unwrap();
        assert!(structured["commit_sha"].is_string());
    }

    #[test]
    fn test_git_commit_nothing_to_commit() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitCommitTool;

        // No changes made
        let result = tool.execute(json!({"message": "Empty commit"}), &ctx);
        assert!(result.is_err());
    }

    // ============================================================================
    // GitLogTool Tests
    // ============================================================================

    #[test]
    fn test_git_log_default_limit() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        // Create additional commits
        for i in 1..=5 {
            let file_path = repo.path().join(format!("commit{}.txt", i));
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "Commit {}", i).unwrap();

            Command::new("git")
                .args(["add", "."])
                .current_dir(repo.path())
                .output()
                .expect("Failed to add");

            Command::new("git")
                .args(["commit", "-m", &format!("Commit {}", i)])
                .current_dir(repo.path())
                .output()
                .expect("Failed to commit");
        }

        let result = tool.execute(json!({}), &ctx).unwrap();

        assert!(result.text.contains("Commit"));
        let structured = result.structured.unwrap();
        let commits = structured["commits"].as_array().unwrap();
        // Default limit is 10, we have 6 commits total (1 initial + 5 new)
        assert!(commits.len() <= 10);
        assert!(!commits.is_empty());
    }

    #[test]
    fn test_git_log_custom_limit() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        // Create additional commits
        for i in 1..=5 {
            let file_path = repo.path().join(format!("commit{}.txt", i));
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "Commit {}", i).unwrap();

            Command::new("git")
                .args(["add", "."])
                .current_dir(repo.path())
                .output()
                .expect("Failed to add");

            Command::new("git")
                .args(["commit", "-m", &format!("Commit {}", i)])
                .current_dir(repo.path())
                .output()
                .expect("Failed to commit");
        }

        let result = tool.execute(json!({"limit": 3}), &ctx).unwrap();

        let structured = result.structured.unwrap();
        let commits = structured["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 3);
    }

    #[test]
    fn test_git_log_parsing() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        let result = tool.execute(json!({"limit": 1}), &ctx).unwrap();

        let structured = result.structured.unwrap();
        let commits = structured["commits"].as_array().unwrap();
        assert!(!commits.is_empty());

        let first_commit = &commits[0];
        assert!(first_commit["sha"].is_string());
        assert!(!first_commit["sha"].as_str().unwrap().is_empty());
        assert!(first_commit["message"].is_string());
    }

    #[test]
    fn test_git_log_empty_repo() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        // We have the initial commit, so log should show it
        let result = tool.execute(json!({}), &ctx).unwrap();
        let structured = result.structured.unwrap();
        let commits = structured["commits"].as_array().unwrap();
        assert!(!commits.is_empty());
    }

    #[test]
    fn test_git_log_commit_ordering() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        // Create commits with specific order
        for i in 1..=3 {
            let file_path = repo.path().join(format!("file{}.txt", i));
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "File {}", i).unwrap();

            Command::new("git")
                .args(["add", "."])
                .current_dir(repo.path())
                .output()
                .expect("Failed to add");

            Command::new("git")
                .args(["commit", "-m", &format!("Message {}", i)])
                .current_dir(repo.path())
                .output()
                .expect("Failed to commit");
        }

        let result = tool.execute(json!({"limit": 5}), &ctx).unwrap();
        let structured = result.structured.unwrap();
        let commits = structured["commits"].as_array().unwrap();

        // Most recent commit should be first
        assert!(commits[0]["message"].as_str().unwrap().contains("3"));
    }

    #[test]
    fn test_git_log_large_limit() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        // Request more commits than exist
        let result = tool.execute(json!({"limit": 100}), &ctx).unwrap();
        let structured = result.structured.unwrap();
        let commits = structured["commits"].as_array().unwrap();

        // Should return all available commits (at least 1)
        assert!(!commits.is_empty());
    }

    // ============================================================================
    // Integration Tests
    // ============================================================================

    #[test]
    fn test_git_workflow_full_cycle() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Check initial status
        let status_tool = GitStatusTool;
        let status = status_tool.execute(json!({}), &ctx).unwrap();
        assert_eq!(status.structured.unwrap()["has_changes"], false);

        // Make changes
        let file_path = repo.path().join("workflow.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Workflow test").unwrap();

        // Check status with changes
        let status = status_tool.execute(json!({}), &ctx).unwrap();
        assert_eq!(status.structured.unwrap()["has_changes"], true);

        // Commit changes
        let commit_tool = GitCommitTool;
        let commit = commit_tool
            .execute(
                json!({
                    "message": "Workflow commit",
                    "files": ["workflow.txt"]
                }),
                &ctx,
            )
            .unwrap();
        assert!(commit.structured.unwrap()["commit_sha"].is_string());

        // Check log
        let log_tool = GitLogTool;
        let log = log_tool.execute(json!({"limit": 1}), &ctx).unwrap();
        let log_structured = log.structured.unwrap();
        let commits = log_structured["commits"].as_array().unwrap();
        assert!(commits[0]["message"].as_str().unwrap().contains("Workflow"));

        // Check final status
        let status = status_tool.execute(json!({}), &ctx).unwrap();
        assert_eq!(status.structured.unwrap()["has_changes"], false);
    }

    #[test]
    fn test_git_branch_detection() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create and checkout a new branch
        Command::new("git")
            .args(["checkout", "-b", "test-branch"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to create branch");

        let tool = GitStatusTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        let structured = result.structured.unwrap();
        assert_eq!(structured["branch"], "test-branch");
    }

    #[test]
    fn test_git_diff_blocks_path_traversal() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitDiffTool;

        let result = tool.execute(json!({"path": "../../../etc/passwd"}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_git_commit_blocks_path_traversal() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitCommitTool;

        let result = tool.execute(
            json!({
                "message": "Bad commit",
                "files": ["../../../etc/passwd"]
            }),
            &ctx,
        );
        assert!(result.is_err());
    }
}
