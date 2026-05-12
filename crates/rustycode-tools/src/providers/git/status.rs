use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct GitStatusTool;

    name: "GitStatus",
    namespace: "git",
    description: "Show git status for current workspace.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Ops],
    defer_loading: true,

    execute(_params: GitStatusParams, ctx) {
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

        std::process::Command::new("git")
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
        let dir = tempfile::TempDir::new().unwrap();
        let ctx = create_context(&dir.path().to_path_buf());
        let tool = GitStatusTool;

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_git_branch_detection() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());

        // Create and checkout a new branch
        std::process::Command::new("git")
            .args(["checkout", "-b", "test-branch"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to create branch");

        let tool = GitStatusTool;
        let result = tool.execute(json!({}), &ctx).unwrap();

        let structured = result.structured.unwrap();
        assert_eq!(structured["branch"], "test-branch");
    }
}
