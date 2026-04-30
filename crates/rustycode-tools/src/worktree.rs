use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use rustycode_runtime::git_worktree::{WorktreeManager, WorktreeType};
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct WorktreeCreateTool;
pub struct WorktreeListTool;
pub struct WorktreeDeleteTool;

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
                "branch": { "type": "string" },
                "type": { "type": "string", "enum": ["session", "feature", "bugfix", "experiment"] }
            },
            "required": ["name", "branch", "type"]
        })
    }
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params["name"].as_str().ok_or(anyhow!("Missing name"))?;
        let branch = params["branch"].as_str().ok_or(anyhow!("Missing branch"))?;
        let wt_type = match params["type"].as_str().unwrap_or("feature") {
            "session" => WorktreeType::Session,
            "feature" => WorktreeType::Feature,
            "bugfix" => WorktreeType::Bugfix,
            "experiment" => WorktreeType::Experiment,
            _ => WorktreeType::Feature,
        };

        let manager = WorktreeManager::new(ctx.cwd.clone(), Default::default())
            .map_err(|e| anyhow!("WorktreeManager init failed: {}", e))?;

        let wt = manager
            .create_worktree(name.to_string(), branch.to_string(), wt_type)
            .map_err(|e| anyhow!("Failed to create worktree: {}", e))?;

        Ok(ToolOutput::text(format!(
            "Worktree '{}' created at {}",
            name,
            wt.path.display()
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
        let manager = WorktreeManager::new(ctx.cwd.clone(), Default::default())
            .map_err(|e| anyhow!("WorktreeManager init failed: {}", e))?;

        // Blocking block for async manager list
        let worktrees = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(manager.list_worktrees())
        });

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
        let manager = WorktreeManager::new(ctx.cwd.clone(), Default::default())
            .map_err(|e| anyhow!("WorktreeManager init failed: {}", e))?;

        manager
            .remove_worktree(name)
            .map_err(|e| anyhow!("Failed to remove worktree: {}", e))?;

        Ok(ToolOutput::text(format!("Worktree '{}' removed.", name)))
    }
}
