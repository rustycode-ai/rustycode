use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct GitDiffTool;

    name: "git_diff",
    description: "Show git diff, optionally staged and/or for a specific path.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Ops],
    defer_loading: true,

    execute(params: GitDiffParams, ctx) {
        let staged = params.staged;
        let path_opt = params.path.as_deref().or(params.file_path.as_deref());
        let mut args = vec!["diff", "--numstat"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        if let Some(path) = path_opt {
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
        if let Some(path) = path_opt {
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

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;
    use std::fs::File;
    use std::io::Write;

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

        std::process::Command::new("git")
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

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo.path())
            .output()
            .expect("Failed to add files");

        std::process::Command::new("git")
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

    #[test]
    fn test_git_diff_blocks_path_traversal() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitDiffTool;

        let result = tool.execute(json!({"path": "../../../etc/passwd"}), &ctx);
        assert!(result.is_err());
    }
}
