use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct GitCommitTool;

    name: "GitCommit",
    description: "Stage files and create a git commit with provided message.",
    permission: ToolPermission::Write,
    tags: [ToolTag::Ops],
    defer_loading: true,

    execute(params: GitCommitParams, ctx) {
        let message = params.message;
        let staged_files = if let Some(files) = params.files {
            // Validate all file paths are within workspace
            for p in &files {
                validate_read_path(p, &ctx.cwd, !ctx.allow_outside_workspace)?;
            }
            if !files.is_empty() {
                let mut add_args = vec!["add", "--"];
                let paths: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
                add_args.extend_from_slice(&paths);
                run_git(ctx, &add_args)?;
                Some(files)
            } else {
                None
            }
        } else {
            None
        };

        let result = run_git(ctx, &["commit", "-m", &message])?;

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

        Ok(ToolOutput::text(result.text).with_metadata(ctx, || structured))
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
    fn test_git_commit_with_message() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create and stage a file
        let new_file_path = repo.path().join("test.txt");
        let mut file = File::create(&new_file_path).unwrap();
        writeln!(file, "Test content").unwrap();

        std::process::Command::new("git")
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

        std::process::Command::new("git")
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
