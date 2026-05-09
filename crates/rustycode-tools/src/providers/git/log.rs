use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::{json, Value};

rustycode_tools_api::define_tool! {
    pub struct GitLogTool;

    name: "git_log",
    description: "Show recent git commits.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Ops],
    defer_loading: true,

    execute(params: GitLogParams, ctx) {
        let limit = params.limit.unwrap_or(10).min(1000);
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
        Ok(ToolOutput::text(output.text).with_metadata(ctx, || json!({ "commits": commits })))
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
    fn test_git_log_default_limit() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        // Create additional commits
        for i in 1..=5 {
            let file_path = repo.path().join(format!("commit{}.txt", i));
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "Commit {}", i).unwrap();

            std::process::Command::new("git")
                .args(["add", "."])
                .current_dir(repo.path())
                .output()
                .expect("Failed to add");

            std::process::Command::new("git")
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

            std::process::Command::new("git")
                .args(["add", "."])
                .current_dir(repo.path())
                .output()
                .expect("Failed to add");

            std::process::Command::new("git")
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
    fn test_git_log_ordering() {
        let repo = create_test_repo();
        let ctx = create_context(&repo.path().to_path_buf());
        let tool = GitLogTool;

        // Create commits with specific order
        for i in 1..=3 {
            let file_path = repo.path().join(format!("file{}.txt", i));
            let mut file = File::create(&file_path).unwrap();
            writeln!(file, "File {}", i).unwrap();

            std::process::Command::new("git")
                .args(["add", "."])
                .current_dir(repo.path())
                .output()
                .expect("Failed to add");

            std::process::Command::new("git")
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
}
