use crate::{ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

use rustycode_tools_api::{
    clear_session_original_cwd, define_tool, in_worktree_session, session_original_cwd,
    set_session_original_cwd,
};

// --- Params structs ---

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeCreateParams {
    /// Name for the new worktree
    pub name: String,
    /// Branch name for the worktree
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeListParams;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorktreeDeleteParams {
    /// Name of the worktree to delete
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EnterWorktreeParams {
    /// Name for a new worktree. Random name generated if neither name nor path is provided.
    pub name: Option<String>,
    /// Path to an existing worktree to enter (must appear in `git worktree list`). Mutually exclusive with name.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExitWorktreeParams {
    /// 'keep' leaves the worktree on disk. 'remove' deletes it.
    pub action: String,
    /// Only meaningful with action='remove'. Must be true to remove a worktree with uncommitted changes.
    #[serde(default)]
    pub discard_changes: bool,
}

// --- Helpers ---

/// Get the git root directory for the given CWD.
fn git_root(cwd: &PathBuf) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow!("Failed to find git root: {}", e))?;
    if !output.status.success() {
        return Err(anyhow!("Not inside a git repository"));
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

/// Parse `git worktree list --porcelain` into a list of (path, branch) tuples.
fn parse_worktree_list(output: &str) -> Vec<(PathBuf, Option<String>)> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path));
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            if let Some(path) = current_path.take() {
                worktrees.push((path, Some(branch.to_string())));
            }
        } else if line.is_empty() {
            current_path = None;
        }
    }
    // Handle last entry without trailing blank line
    if let Some(path) = current_path.take() {
        worktrees.push((path, None));
    }
    worktrees
}

// --- CRUD tools ---

define_tool! {
    pub struct WorktreeCreateTool;

    name: "worktree_create",
    description: "Create a new git worktree for isolated development.",
    permission: ToolPermission::Execute,

    execute(params: WorktreeCreateParams, ctx) {
        let root = git_root(&ctx.cwd)?;
        let wt_path = root.join(".worktrees").join(&params.name);

        let output = std::process::Command::new("git")
            .args(["worktree", "add", "--detach"])
            .arg(&wt_path)
            .arg("-b")
            .arg(&params.branch)
            .arg("HEAD")
            .current_dir(&ctx.cwd)
            .output()
            .map_err(|e| anyhow!("Failed to create worktree: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to create worktree: {}", stderr.trim()));
        }

        Ok(ToolOutput::text(format!(
            "Worktree '{}' created at {}",
            params.name,
            wt_path.display()
        )))
    }
}

define_tool! {
    pub struct WorktreeListTool;

    name: "worktree_list",
    description: "List all active git worktrees.",
    permission: ToolPermission::Read,

    execute(_params: WorktreeListParams, ctx) {
        let output = std::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&ctx.cwd)
            .output()
            .map_err(|e| anyhow!("Failed to list worktrees: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to list worktrees: {}", stderr.trim()));
        }

        let list = String::from_utf8_lossy(&output.stdout);
        let worktrees: Vec<Value> = parse_worktree_list(&list)
            .into_iter()
            .map(|(path, branch)| {
                json!({
                    "path": path.display().to_string(),
                    "branch": branch
                })
            })
            .collect();

        Ok(ToolOutput::with_structured(
            format!("Found {} worktrees", worktrees.len()),
            json!(worktrees),
        ))
    }
}

define_tool! {
    pub struct WorktreeDeleteTool;

    name: "worktree_delete",
    description: "Delete an existing git worktree.",
    permission: ToolPermission::Execute,

    execute(params: WorktreeDeleteParams, ctx) {
        let root = git_root(&ctx.cwd)?;
        let wt_path = root.join(".worktrees").join(&params.name);

        let output = std::process::Command::new("git")
            .args(["worktree", "remove"])
            .arg(&wt_path)
            .current_dir(&ctx.cwd)
            .output()
            .map_err(|e| anyhow!("Failed to remove worktree: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to remove worktree: {}", stderr.trim()));
        }

        Ok(ToolOutput::text(format!("Worktree '{}' removed.", params.name)))
    }
}

// --- Session-scoped enter/exit tools ---

define_tool! {
    pub struct EnterWorktreeTool;

    name: "worktree_enter",
    description: "Create or enter a git worktree and switch the session working directory into it.\n\nUse when: the user explicitly says \"worktree\", \"use a worktree\", \"start a worktree\", or project instructions require worktree isolation.\nDo NOT use when: the user asks to create or switch branches (use git commands instead).\n\nCreates a new worktree from the current HEAD unless `path` is provided to enter an existing one.\nThe session CWD changes to the worktree path. Use `worktree_exit` to return.",
    permission: ToolPermission::Execute,

    execute(params: EnterWorktreeParams, ctx) {
        if in_worktree_session() {
            return Err(anyhow!(
                "Already in a worktree session. Exit the current worktree with worktree_exit first."
            ));
        }

        let original_cwd = ctx.cwd.clone();

        if let Some(path_str) = &params.path {
            // Enter existing worktree by path
            let path = PathBuf::from(path_str);
            if !path.exists() {
                return Err(anyhow!("Path does not exist: {}", path_str));
            }
            // Validate it's a registered git worktree
            let output = std::process::Command::new("git")
                .args(["worktree", "list", "--porcelain"])
                .current_dir(&original_cwd)
                .output()
                .map_err(|e| anyhow!("Failed to run git worktree list: {}", e))?;
            let list = String::from_utf8_lossy(&output.stdout);
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let mut found = false;
            for line in list.lines() {
                if let Some(wt_path) = line.strip_prefix("worktree ") {
                    let wt = PathBuf::from(wt_path);
                    if wt == canonical || wt == path {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                return Err(anyhow!(
                    "Path '{}' is not a registered git worktree. Use `git worktree list` to see valid paths.",
                    path_str
                ));
            }

            set_session_original_cwd(original_cwd);
            Ok(ToolOutput::with_cwd_change(
                format!("Entered existing worktree at {}", path.display()),
                canonical,
            ))
        } else {
            // Create new worktree
            let name = params.name.unwrap_or_else(|| {
                format!("wt-{}", &uuid::Uuid::new_v4().to_string()[..8])
            });

            let root = git_root(&original_cwd)?;
            let wt_path = root.join(".worktrees").join(&name);

            let output = std::process::Command::new("git")
                .args(["worktree", "add", "-b", &name])
                .arg(&wt_path)
                .arg("HEAD")
                .current_dir(&original_cwd)
                .output()
                .map_err(|e| anyhow!("Failed to create worktree: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("Failed to create worktree: {}", stderr.trim()));
            }

            set_session_original_cwd(original_cwd);
            Ok(ToolOutput::with_cwd_change(
                format!(
                    "Created and entered worktree '{}' at {}",
                    name,
                    wt_path.display()
                ),
                wt_path,
            ))
        }
    }
}

define_tool! {
    pub struct ExitWorktreeTool;

    name: "worktree_exit",
    description: "Exit the current worktree session and restore the original working directory.\n\nUse when: the user explicitly asks to \"exit the worktree\", \"leave the worktree\", or end the worktree session.\nDo NOT call proactively — only when the user asks.\n\nThe `action` parameter controls cleanup:\n- \"keep\": Leaves the worktree directory and branch on disk for later use.\n- \"remove\": Deletes the worktree directory and branch. Refuses if there are uncommitted changes unless `discard_changes` is true.",
    permission: ToolPermission::Execute,

    execute(params: ExitWorktreeParams, ctx) {
        let original_cwd = session_original_cwd()
            .ok_or_else(|| anyhow!("Not in a worktree session. Use worktree_enter first."))?;

        if params.action == "remove" {
            // Check for uncommitted changes
            let status_output = std::process::Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(&ctx.cwd)
                .output()
                .map_err(|e| anyhow!("Failed to check git status: {}", e))?;

            let has_changes = !status_output.stdout.is_empty();
            let discard = params.discard_changes;

            if has_changes && !discard {
                let dirty_files = String::from_utf8_lossy(&status_output.stdout);
                let preview: Vec<&str> = dirty_files.lines().take(10).collect();
                return Err(anyhow!(
                    "Worktree has uncommitted changes. Set discard_changes=true to force removal.\nChanges:\n{}",
                    preview.join("\n")
                ));
            }

            // Remove worktree via git command
            let force_flag = if has_changes && discard {
                vec!["--force"]
            } else {
                vec![]
            };

            let mut args = vec!["worktree", "remove"];
            args.extend(force_flag);
            args.push(ctx.cwd.to_str().unwrap_or("."));

            let output = std::process::Command::new("git")
                .args(&args)
                .current_dir(&original_cwd)
                .output()
                .map_err(|e| anyhow!("Failed to remove worktree: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("Failed to remove worktree: {}", stderr.trim()));
            }
        }

        clear_session_original_cwd();

        Ok(ToolOutput::with_cwd_change(
            format!(
                "Exited worktree. {}",
                if params.action == "remove" {
                    "Worktree removed."
                } else {
                    "Worktree kept on disk."
                }
            ),
            original_cwd,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that touch global session state
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn enter_rejects_if_already_in_session() {
        let _lock = TEST_LOCK.lock().unwrap();
        set_session_original_cwd(PathBuf::from("/tmp/test-project"));

        let tool = EnterWorktreeTool;
        let ctx = ToolContext::new("/tmp/test-project");
        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Already in a worktree session"));

        clear_session_original_cwd();
    }

    #[test]
    fn exit_rejects_if_not_in_session() {
        let _lock = TEST_LOCK.lock().unwrap();
        clear_session_original_cwd();

        let tool = ExitWorktreeTool;
        let ctx = ToolContext::new("/tmp/test-project");
        let result = tool.execute(json!({"action": "keep"}), &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Not in a worktree session"));
    }

    #[test]
    fn enter_rejects_nonexistent_path() {
        let _lock = TEST_LOCK.lock().unwrap();
        clear_session_original_cwd();

        let tool = EnterWorktreeTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"path": "/nonexistent/worktree/path"}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn parse_worktree_list_handles_standard_format() {
        let input = "worktree /home/user/project\nbranch refs/heads/main\n\nworktree /home/user/project/.worktrees/feature\nbranch refs/heads/feature\n";
        let result = parse_worktree_list(input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, PathBuf::from("/home/user/project"));
        assert_eq!(result[0].1, Some("main".to_string()));
        assert_eq!(
            result[1].0,
            PathBuf::from("/home/user/project/.worktrees/feature")
        );
        assert_eq!(result[1].1, Some("feature".to_string()));
    }

    #[test]
    fn parse_worktree_list_handles_bare_worktree() {
        let input = "worktree /home/user/project\nbranch refs/heads/main\n";
        let result = parse_worktree_list(input);
        assert_eq!(result.len(), 1);
    }
}
