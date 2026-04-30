//! Environment context for prompt injection
//!
//! Dynamically gathers environment information for injection into prompts.

use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment context gathered at runtime
#[derive(Debug, Clone)]
pub struct EnvironmentContext {
    pub working_directory: PathBuf,
    pub workspace_root: PathBuf,
    pub is_git_repo: bool,
    pub platform: String,
    pub date: String,
    pub git_status: Option<GitStatus>,
}

/// Git repository status
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub modified: Vec<String>,
    pub staged: Vec<String>,
    pub untracked: Vec<String>,
}

impl EnvironmentContext {
    /// Gather environment context from current directory
    #[allow(clippy::unused_async)]
    pub async fn gather() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let workspace_root = Self::find_workspace_root(&cwd);
        let is_git_repo = workspace_root.join(".git").exists();

        let git_status = if is_git_repo {
            Some(Self::get_git_status(&workspace_root))
        } else {
            None
        };

        Ok(Self {
            working_directory: cwd,
            workspace_root,
            is_git_repo,
            platform: std::env::consts::OS.to_string(),
            date: Utc::now().format("%Y-%m-%d").to_string(),
            git_status,
        })
    }

    /// Find workspace root by looking for .git directory or project markers
    fn find_workspace_root(start: &Path) -> PathBuf {
        let mut current = Some(start);

        while let Some(path) = current {
            // Check for .git directory
            if path.join(".git").exists() {
                return path.to_path_buf();
            }

            // Check for common project markers
            for marker in &["Cargo.toml", "package.json", "pyproject.toml", "go.mod"] {
                if path.join(marker).exists() {
                    return path.to_path_buf();
                }
            }

            // Move to parent
            current = path.parent();
        }

        // Fallback to current directory if no markers found
        start.to_path_buf()
    }

    /// Get git status information
    fn get_git_status(repo_path: &Path) -> GitStatus {
        // Get current branch
        let branch = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_path)
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Some(stdout.trim().to_string())
                } else {
                    None
                }
            });

        // Get modified files
        let modified = Command::new("git")
            .args(["diff", "--name-only", "HEAD"])
            .current_dir(repo_path)
            .output()
            .ok()
            .map(|output| {
                String::from_utf8(output.stdout)
                    .unwrap_or_default()
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Get staged files
        let staged = Command::new("git")
            .args(["diff", "--cached", "--name-only", "HEAD"])
            .current_dir(repo_path)
            .output()
            .ok()
            .map(|output| {
                String::from_utf8(output.stdout)
                    .unwrap_or_default()
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Get untracked files (limited)
        let untracked = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard"])
            .current_dir(repo_path)
            .output()
            .ok()
            .map(|output| {
                String::from_utf8(output.stdout)
                    .unwrap_or_default()
                    .lines()
                    .filter(|l| !l.is_empty())
                    .take(20) // Limit to 20 files
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        GitStatus {
            branch,
            modified,
            staged,
            untracked,
        }
    }

    /// Format environment as markdown for prompt injection
    pub fn format_markdown(&self) -> String {
        let mut parts = vec![
            "## Environment".to_string(),
            format!("Working directory: `{}`", self.working_directory.display()),
            format!("Workspace root: `{}`", self.workspace_root.display()),
            format!(
                "Git repository: {}",
                if self.is_git_repo { "yes" } else { "no" }
            ),
            format!("Platform: {}", self.platform),
            format!("Date: {}", self.date),
        ];

        if let Some(git) = &self.git_status {
            if let Some(branch) = &git.branch {
                parts.push(format!("Git branch: `{branch}`"));
            }

            if !git.modified.is_empty() {
                parts.push(format!("Modified files: {}", git.modified.len()));
            }

            if !git.staged.is_empty() {
                parts.push(format!("Staged files: {}", git.staged.len()));
            }

            if !git.untracked.is_empty() {
                parts.push(format!(
                    "Untracked files: {} (showing 20)",
                    git.untracked.len()
                ));
            }
        }

        parts.join("\n")
    }

    /// Format environment as JSON for structured injection
    pub fn format_json(&self) -> serde_json::Value {
        json!({
            "working_directory": self.working_directory.display().to_string(),
            "workspace_root": self.workspace_root.display().to_string(),
            "is_git_repo": self.is_git_repo,
            "platform": self.platform,
            "date": self.date,
            "git_status": self.git_status.as_ref().map(|git| json!({
                "branch": git.branch,
                "modified": git.modified,
                "staged": git.staged,
                "untracked": git.untracked
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gather_environment() {
        let env = EnvironmentContext::gather().await.unwrap();

        assert!(!env.working_directory.as_os_str().is_empty());
        assert!(!env.platform.is_empty());
        assert!(!env.date.is_empty());
    }

    #[test]
    fn test_format_markdown() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/Users/dev/myapp"),
            workspace_root: PathBuf::from("/Users/dev/myapp"),
            is_git_repo: true,
            platform: "macos".to_string(),
            date: "2025-03-13".to_string(),
            git_status: Some(GitStatus {
                branch: Some("main".to_string()),
                modified: vec!["src/main.rs".to_string()],
                staged: vec![],
                untracked: vec![],
            }),
        };

        let markdown = env.format_markdown();

        assert!(markdown.contains("## Environment"));
        assert!(markdown.contains("Working directory: `/Users/dev/myapp`"));
        assert!(markdown.contains("Git branch: `main`"));
        assert!(markdown.contains("Modified files: 1"));
    }

    // --- New tests: JSON formatting, edge cases, error display ---

    #[test]
    fn test_format_markdown_no_git_status() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/project"),
            workspace_root: PathBuf::from("/tmp/project"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: None,
        };

        let markdown = env.format_markdown();
        assert!(markdown.contains("## Environment"));
        assert!(markdown.contains("Git repository: no"));
        assert!(!markdown.contains("Git branch"));
        assert!(!markdown.contains("Modified files"));
        assert!(!markdown.contains("Staged files"));
        assert!(!markdown.contains("Untracked files"));
    }

    #[test]
    fn test_format_markdown_git_no_branch() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/project"),
            workspace_root: PathBuf::from("/tmp/project"),
            is_git_repo: true,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: Some(GitStatus {
                branch: None,
                modified: vec![],
                staged: vec![],
                untracked: vec![],
            }),
        };

        let markdown = env.format_markdown();
        assert!(markdown.contains("Git repository: yes"));
        assert!(!markdown.contains("Git branch"));
    }

    #[test]
    fn test_format_markdown_all_git_fields() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/project"),
            workspace_root: PathBuf::from("/project"),
            is_git_repo: true,
            platform: "macos".to_string(),
            date: "2025-06-15".to_string(),
            git_status: Some(GitStatus {
                branch: Some("feature/test".to_string()),
                modified: vec!["a.rs".to_string(), "b.rs".to_string()],
                staged: vec!["c.rs".to_string()],
                untracked: vec!["d.rs".to_string()],
            }),
        };

        let markdown = env.format_markdown();
        assert!(markdown.contains("Git branch: `feature/test`"));
        assert!(markdown.contains("Modified files: 2"));
        assert!(markdown.contains("Staged files: 1"));
        assert!(markdown.contains("Untracked files: 1"));
    }

    #[test]
    fn test_format_markdown_platform_and_date() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/home/user/proj"),
            workspace_root: PathBuf::from("/home/user/proj"),
            is_git_repo: false,
            platform: "freebsd".to_string(),
            date: "2025-12-25".to_string(),
            git_status: None,
        };

        let markdown = env.format_markdown();
        assert!(markdown.contains("Platform: freebsd"));
        assert!(markdown.contains("Date: 2025-12-25"));
    }

    #[test]
    fn test_format_json_basic_fields() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp/test"),
            workspace_root: PathBuf::from("/tmp/test"),
            is_git_repo: true,
            platform: "linux".to_string(),
            date: "2025-03-13".to_string(),
            git_status: Some(GitStatus {
                branch: Some("main".to_string()),
                modified: vec!["src/main.rs".to_string()],
                staged: vec!["src/lib.rs".to_string()],
                untracked: vec!["new_file.rs".to_string()],
            }),
        };

        let json = env.format_json();
        let obj = json.as_object().unwrap();

        assert_eq!(obj["working_directory"].as_str(), Some("/tmp/test"));
        assert_eq!(obj["workspace_root"].as_str(), Some("/tmp/test"));
        assert_eq!(obj["is_git_repo"].as_bool(), Some(true));
        assert_eq!(obj["platform"].as_str(), Some("linux"));
        assert_eq!(obj["date"].as_str(), Some("2025-03-13"));

        let git = obj["git_status"].as_object().unwrap();
        assert_eq!(git["branch"].as_str(), Some("main"));
        assert_eq!(git["modified"].as_array().unwrap().len(), 1);
        assert_eq!(git["staged"].as_array().unwrap().len(), 1);
        assert_eq!(git["untracked"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_format_json_null_git_status() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp"),
            workspace_root: PathBuf::from("/tmp"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: None,
        };

        let json = env.format_json();
        let obj = json.as_object().unwrap();
        assert!(obj["git_status"].is_null());
    }

    #[test]
    fn test_format_json_empty_git_arrays() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp"),
            workspace_root: PathBuf::from("/tmp"),
            is_git_repo: true,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: Some(GitStatus {
                branch: None,
                modified: vec![],
                staged: vec![],
                untracked: vec![],
            }),
        };

        let json = env.format_json();
        let git = json["git_status"].as_object().unwrap();
        assert!(git["branch"].is_null());
        assert!(git["modified"].as_array().unwrap().is_empty());
        assert!(git["staged"].as_array().unwrap().is_empty());
        assert!(git["untracked"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_format_json_roundtrip_via_serde() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/home/dev/project"),
            workspace_root: PathBuf::from("/home/dev/project"),
            is_git_repo: true,
            platform: "macos".to_string(),
            date: "2025-06-01".to_string(),
            git_status: Some(GitStatus {
                branch: Some("develop".to_string()),
                modified: vec!["a.rs".to_string()],
                staged: vec![],
                untracked: vec!["b.rs".to_string(), "c.rs".to_string()],
            }),
        };

        let json_val = env.format_json();
        let serialized = serde_json::to_string(&json_val).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(json_val, deserialized);
    }

    #[test]
    fn test_git_status_debug() {
        let status = GitStatus {
            branch: Some("feature".to_string()),
            modified: vec!["x.rs".to_string()],
            staged: vec![],
            untracked: vec![],
        };
        let debug = format!("{status:?}");
        assert!(debug.contains("GitStatus"));
        assert!(debug.contains("feature"));
    }

    #[test]
    fn test_git_status_clone() {
        let status = GitStatus {
            branch: Some("main".to_string()),
            modified: vec!["a.rs".to_string(), "b.rs".to_string()],
            staged: vec!["c.rs".to_string()],
            untracked: vec![],
        };
        let cloned = status.clone();
        assert_eq!(cloned.branch, status.branch);
        assert_eq!(cloned.modified, status.modified);
        assert_eq!(cloned.staged, status.staged);
        assert_eq!(cloned.untracked, status.untracked);
    }

    #[test]
    fn test_environment_context_clone() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp"),
            workspace_root: PathBuf::from("/tmp"),
            is_git_repo: false,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: None,
        };
        let cloned = env.clone();
        assert_eq!(cloned.working_directory, env.working_directory);
        assert_eq!(cloned.platform, env.platform);
    }

    #[test]
    fn test_environment_context_debug() {
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp"),
            workspace_root: PathBuf::from("/tmp"),
            is_git_repo: true,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: Some(GitStatus {
                branch: Some("main".to_string()),
                modified: vec![],
                staged: vec![],
                untracked: vec![],
            }),
        };
        let debug = format!("{env:?}");
        assert!(debug.contains("EnvironmentContext"));
        assert!(debug.contains("is_git_repo: true"));
    }

    #[test]
    fn test_find_workspace_root_with_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();

        let result = EnvironmentContext::find_workspace_root(dir.path());
        assert_eq!(result, dir.path());
    }

    #[test]
    fn test_find_workspace_root_with_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let result = EnvironmentContext::find_workspace_root(dir.path());
        assert_eq!(result, dir.path());
    }

    #[test]
    fn test_find_workspace_root_with_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let result = EnvironmentContext::find_workspace_root(dir.path());
        assert_eq!(result, dir.path());
    }

    #[test]
    fn test_find_workspace_root_with_pyproject_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();

        let result = EnvironmentContext::find_workspace_root(dir.path());
        assert_eq!(result, dir.path());
    }

    #[test]
    fn test_find_workspace_root_with_go_mod() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module test\n").unwrap();

        let result = EnvironmentContext::find_workspace_root(dir.path());
        assert_eq!(result, dir.path());
    }

    #[test]
    fn test_find_workspace_root_nested_git() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("sub").join("deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();

        let result = EnvironmentContext::find_workspace_root(&nested);
        assert_eq!(result, dir.path());
    }

    #[test]
    fn test_find_workspace_root_fallback() {
        // A tempdir with no markers should fall back to start path
        let dir = tempfile::tempdir().unwrap();
        let start = dir.path();
        let result = EnvironmentContext::find_workspace_root(start);
        assert_eq!(result, start);
    }

    #[test]
    fn test_format_markdown_untracked_count_message() {
        let files: Vec<String> = (0..20).map(|i| format!("file_{i}.rs")).collect();
        let env = EnvironmentContext {
            working_directory: PathBuf::from("/tmp"),
            workspace_root: PathBuf::from("/tmp"),
            is_git_repo: true,
            platform: "linux".to_string(),
            date: "2025-01-01".to_string(),
            git_status: Some(GitStatus {
                branch: Some("main".to_string()),
                modified: vec![],
                staged: vec![],
                untracked: files,
            }),
        };

        let markdown = env.format_markdown();
        assert!(markdown.contains("Untracked files: 20 (showing 20)"));
    }
}
