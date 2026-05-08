use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use rustycode_tools_api::{
    clear_session_original_cwd, in_worktree_session, session_original_cwd, set_session_original_cwd,
};
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct WorktreeCreateTool;
pub struct WorktreeListTool;
pub struct WorktreeDeleteTool;
pub struct EnterWorktreeTool;
pub struct ExitWorktreeTool;

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

impl Tool for WorktreeCreateTool {
    fn name(&self) -> &str {
        "worktree_create"
    }
    fn description(&self) -> &str {
        "Create a new git worktree for isolated development."
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "branch": { "type": "string" }
            },
            "required": ["name", "branch"]
        })
    }
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params["name"].as_str().ok_or(anyhow!("Missing name"))?;
        let branch = params["branch"].as_str().ok_or(anyhow!("Missing branch"))?;

        let root = git_root(&ctx.cwd)?;
        let wt_path = root.join(".worktrees").join(name);

        let output = std::process::Command::new("git")
            .args(["worktree", "add", "--detach"])
            .arg(&wt_path)
            .arg("-b")
            .arg(branch)
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
            name,
            wt_path.display()
        )))
    }
}

impl Tool for WorktreeListTool {
    fn name(&self) -> &str {
        "worktree_list"
    }
    fn description(&self) -> &str {
        "List all active git worktrees."
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }
    fn parameters_schema(&self) -> Value {
        json!({})
    }
    fn execute(&self, _params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
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

impl Tool for WorktreeDeleteTool {
    fn name(&self) -> &str {
        "worktree_delete"
    }
    fn description(&self) -> &str {
        "Delete an existing git worktree."
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        })
    }
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params["name"].as_str().ok_or(anyhow!("Missing name"))?;
        let root = git_root(&ctx.cwd)?;
        let wt_path = root.join(".worktrees").join(name);

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

        Ok(ToolOutput::text(format!("Worktree '{}' removed.", name)))
    }
}

// --- Session-scoped enter/exit tools ---

impl Tool for EnterWorktreeTool {
    fn name(&self) -> &str {
        "worktree_enter"
    }
    fn description(&self) -> &str {
        r#"Create or enter a git worktree and switch the session working directory into it.

Use when: the user explicitly says "worktree", "use a worktree", "start a worktree", or project instructions require worktree isolation.
Do NOT use when: the user asks to create or switch branches (use git commands instead).

Creates a new worktree from the current HEAD unless `path` is provided to enter an existing one.
The session CWD changes to the worktree path. Use `worktree_exit` to return."#
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name for a new worktree. Random name generated if neither name nor path is provided."
                },
                "path": {
                    "type": "string",
                    "description": "Path to an existing worktree to enter (must appear in `git worktree list`). Mutually exclusive with name."
                }
            }
        })
    }
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        if in_worktree_session() {
            return Err(anyhow!(
                "Already in a worktree session. Exit the current worktree with worktree_exit first."
            ));
        }

        let original_cwd = ctx.cwd.clone();

        if let Some(path_str) = params["path"].as_str() {
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
            let name = params["name"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| format!("wt-{}", &uuid::Uuid::new_v4().to_string()[..8]));

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

impl Tool for ExitWorktreeTool {
    fn name(&self) -> &str {
        "worktree_exit"
    }
    fn description(&self) -> &str {
        r#"Exit the current worktree session and restore the original working directory.

Use when: the user explicitly asks to "exit the worktree", "leave the worktree", or end the worktree session.
Do NOT call proactively — only when the user asks.

The `action` parameter controls cleanup:
- "keep": Leaves the worktree directory and branch on disk for later use.
- "remove": Deletes the worktree directory and branch. Refuses if there are uncommitted changes unless `discard_changes` is true."#
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["keep", "remove"],
                    "description": "'keep' leaves the worktree on disk. 'remove' deletes it."
                },
                "discard_changes": {
                    "type": "boolean",
                    "default": false,
                    "description": "Only meaningful with action='remove'. Must be true to remove a worktree with uncommitted changes."
                }
            }
        })
    }
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let original_cwd = session_original_cwd()
            .ok_or_else(|| anyhow!("Not in a worktree session. Use worktree_enter first."))?;

        let action = params["action"]
            .as_str()
            .ok_or(anyhow!("Missing 'action' parameter"))?;

        if action == "remove" {
            // Check for uncommitted changes
            let status_output = std::process::Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(&ctx.cwd)
                .output()
                .map_err(|e| anyhow!("Failed to check git status: {}", e))?;

            let has_changes = !status_output.stdout.is_empty();
            let discard = params["discard_changes"].as_bool().unwrap_or(false);

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
                if action == "remove" {
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
