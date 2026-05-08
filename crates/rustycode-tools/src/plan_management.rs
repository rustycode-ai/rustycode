//! Plan Management Tools - Create, save, load, and manage plans
//!
//! This module provides tools for managing plans throughout their lifecycle,
//! including creating plans from templates, saving/loading plans, and approving
//! plans for execution.

use super::plan_templates::PlanTemplate;
use crate::security::{validate_list_path, validate_read_path, validate_write_path};
use crate::{ToolOutput, ToolPermission};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rustycode_protocol::SessionId;
use schemars::JsonSchema;
use serde_json::{json, Value};
use std::fs;

use std::time::SystemTime;

/// Check if a plan ID contains path traversal characters.
fn validate_plan_id(plan_id: &str) -> Result<()> {
    if plan_id.is_empty() {
        return Err(anyhow!("plan_id cannot be empty"));
    }
    if plan_id.contains('/') || plan_id.contains('\\') || plan_id.contains("..") {
        return Err(anyhow!(
            "plan_id contains invalid characters (path separators or traversal)"
        ));
    }
    Ok(())
}

// ── Params structs ──────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct CreatePlanParams {
    /// Type of plan template to use
    pub template: String,
    /// Description of the specific task
    pub task: String,
    /// One-line summary of the plan
    pub summary: String,
    /// Files that will be modified (optional)
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct SavePlanParams {
    /// Plan ID to save
    pub plan_id: String,
    /// Where to save the plan (optional)
    pub file_path: Option<String>,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct LoadPlanParams {
    /// Path to the plan file
    pub file_path: String,
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ListPlansParams {
    /// Directory to search for plans (default: .rustycode/plans)
    #[serde(default = "default_directory")]
    pub directory: String,
}

fn default_directory() -> String {
    ".rustycode/plans".to_string()
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct ApprovePlanParams {
    /// Plan ID to approve
    pub plan_id: String,
}

// ── Tool definitions ────────────────────────────────────────────────────────

rustycode_tools_api::define_tool! {
    pub struct CreatePlanFromTemplateTool;

    name: "create_plan_from_template",
    description: r#"Create a new plan from a predefined template.

**Use cases:**
- Quickly create plans for common development tasks
- Use proven plan structures
- Save time on planning

**Parameters:**
- `template`: Template type (new_feature, bug_fix, refactor, add_tests, performance, documentation, security_fix, dependency_update)
- `task`: Description of the specific task
- `summary`: One-line summary of the plan
- `files`: Array of files that will be modified (optional)

**Returns:**
- Plan ID and summary of the created plan

**Example:**
```json
{
  "template": "bug_fix",
  "task": "Fix authentication bug in login flow",
  "summary": "Fix the bug where users cannot login with valid credentials",
  "files": ["src/auth.rs", "src/login.rs"]
}
```

**Available Templates:**
- `new_feature`: Implement a new feature from scratch
- `bug_fix`: Fix a reported bug
- `refactor`: Refactor code for better structure
- `add_tests`: Add test coverage
- `performance`: Optimize performance
- `documentation`: Add or update documentation
- `security_fix`: Fix security vulnerability
- `dependency_update`: Update dependencies"#,
    permission: ToolPermission::Read,

    execute(params: CreatePlanParams, _ctx) {
        let template_str = params.template.as_str();
        let task = &params.task;
        let summary = &params.summary;
        let files = params.files;

        // Parse template type
        let template = match template_str {
            "new_feature" => PlanTemplate::NewFeature,
            "bug_fix" => PlanTemplate::BugFix,
            "refactor" => PlanTemplate::Refactor,
            "add_tests" => PlanTemplate::AddTests,
            "performance" => PlanTemplate::Performance,
            "documentation" => PlanTemplate::Documentation,
            "security_fix" => PlanTemplate::SecurityFix,
            "dependency_update" => PlanTemplate::DependencyUpdate,
            _ => return Err(anyhow!("unknown template type: {template_str}")),
        };

        // Create plan from template
        let plan = template.create_plan(
            SessionId::new(),
            task.to_string(),
            summary.to_string(),
            files,
        );

        // Format output
        let mut output = String::new();
        output.push_str("**Plan Created from Template**\n\n");
        output.push_str(&format!("**Plan ID:** {}\n", plan.id));
        output.push_str(&format!("**Template:** {template_str}\n"));
        output.push_str(&format!("**Summary:** {}\n\n", plan.summary));
        output.push_str(&format!("**Steps:** {} steps\n", plan.steps.len()));
        output.push_str(&format!(
            "**Risks:** {} potential risks identified\n\n",
            plan.risks.len()
        ));

        output.push_str("**Plan Overview:**\n```\n");
        for (i, step) in plan.steps.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, step.title));
        }
        output.push_str("```\n\n");

        output.push_str("**Next Steps:**\n");
        output.push_str("1. Review the plan steps\n");
        output.push_str("2. Customize steps if needed\n");
        output.push_str("3. Save the plan with save_plan tool\n");
        output.push_str("4. Execute the plan when ready\n");

        let metadata = json!({
            "plan_id": plan.id.to_string(),
            "template": template_str,
            "summary": plan.summary,
            "steps_count": plan.steps.len(),
            "risks_count": plan.risks.len()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

rustycode_tools_api::define_tool! {
    pub struct SavePlanTool;

    name: "save_plan",
    description: r#"Save a plan to disk for later use.

**Use cases:**
- Persist a plan for later execution
- Share plans with team members
- Maintain plan history

**Parameters:**
- `plan_id`: Plan ID (from create_plan_from_template)
- `file_path`: Where to save the plan (default: .rustycode/plans/<plan_id>.json)

**Note:** Currently plans are saved in JSON format. The plan_id parameter
is used to track the plan but in a real implementation, you would pass
the full plan object. This is a simplified version.

**Example:**
```json
{
  "plan_id": "plan-abc123",
  "file_path": "plans/feature-auth.json"
}
```"#,
    permission: ToolPermission::Write,

    execute(params: SavePlanParams, ctx) {
        let plan_id_str = &params.plan_id;
        validate_plan_id(plan_id_str)?;

        let default_path = format!(".rustycode/plans/{plan_id_str}.json");
        let file_path = params
            .file_path
            .as_deref()
            .unwrap_or(&default_path);

        // Validate path stays within workspace
        let plan_json = serde_json::to_string_pretty(&json!({
            "id": plan_id_str,
            "saved_at": Utc::now().to_rfc3339(),
            "note": "This is a placeholder. In a real implementation, the plan would be retrieved from state."
        }))?;
        let path = validate_write_path(
            file_path,
            &ctx.cwd,
            plan_json.len(),
            !ctx.allow_outside_workspace,
        )?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        // Write plan to file atomically (plan_json computed above for size validation)
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &plan_json)
            .with_context(|| format!("failed to write plan to: {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &path)
            .with_context(|| format!("failed to rename plan to: {}", path.display()))?;

        let output = format!(
            "**Plan Saved**\n\n✅ Plan ID `{}` saved to: `{}`\n\n\
            **Next Steps:**\n\
            - Load the plan later with load_plan\n\
            - Execute the plan when ready",
            plan_id_str,
            path.display()
        );

        let metadata = json!({
            "plan_id": plan_id_str,
            "file_path": path.to_string_lossy().to_string(),
            "saved_at": Utc::now().to_rfc3339()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

rustycode_tools_api::define_tool! {
    pub struct LoadPlanTool;

    name: "load_plan",
    description: r#"Load a saved plan from disk.

**Use cases:**
- Load a previously saved plan
- Resume planning from earlier session
- Review existing plans

**Parameters:**
- `file_path`: Path to the plan file

**Example:**
```json
{
  "file_path": "plans/feature-auth.json"
}
```"#,
    permission: ToolPermission::Read,

    execute(params: LoadPlanParams, ctx) {
        let file_path_str = &params.file_path;

        // Validate path stays within workspace
        let path = validate_read_path(file_path_str, &ctx.cwd, !ctx.allow_outside_workspace)?;

        // Read plan file
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read plan from: {}", path.display()))?;

        // Parse JSON
        let plan_data: Value = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse plan JSON from: {}", path.display()))?;

        let plan_id = plan_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let saved_at = plan_data
            .get("saved_at")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let output = format!(
            "**Plan Loaded**\n\n✅ Plan loaded from: `{}`\n\n\
            **Plan ID:** `{}`\n\
            **Saved At:** `{}`\n\n\
            **Next Steps:**\n\
            - Review the plan contents\n\
            - Execute the plan when ready",
            path.display(),
            plan_id,
            saved_at
        );

        let metadata = json!({
            "file_path": path.to_string_lossy().to_string(),
            "plan_data": plan_data
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

rustycode_tools_api::define_tool! {
    pub struct ListPlansTool;

    name: "list_plans",
    description: r#"List all available saved plans.

**Use cases:**
- See what plans are available
- Find plans to load or execute
- Manage plan collection

**Parameters:**
- `directory`: Directory to search for plans (default: .rustycode/plans)

**Example:**
```json
{
  "directory": "plans"
}
```

**Returns:**
- List of plan files with metadata"#,
    permission: ToolPermission::Read,

    execute(params: ListPlansParams, ctx) {
        let directory = &params.directory;

        // Validate path stays within workspace
        let dir_path = validate_list_path(directory, &ctx.cwd, !ctx.allow_outside_workspace)?;

        // Check if directory exists
        if !dir_path.exists() {
            return Ok(ToolOutput::text(
                "**No Plans Found**\n\nNo plans directory found. Create a plan first with create_plan_from_template."
            ));
        }

        // Find all plan files
        let mut plans = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(name) =
                            path.strip_prefix(&ctx.cwd).map(|p| p.display().to_string())
                        {
                            plans.push((name, metadata.modified().ok()));
                        }
                    }
                }
            }
        }

        if plans.is_empty() {
            return Ok(ToolOutput::text(
                "**No Plans Found**\n\nNo plan files found in the plans directory.",
            ));
        }

        // Sort by modification time (newest first)
        plans.sort_by(|a, b| {
            b.1.unwrap_or_else(SystemTime::now)
                .cmp(&a.1.unwrap_or_else(SystemTime::now))
        });

        // Format output
        let mut output = String::new();
        output.push_str("**Available Plans**\n\n");
        output.push_str(&format!("Found {} plan(s):\n\n", plans.len()));

        for (i, (path, modified)) in plans.iter().enumerate() {
            output.push_str(&format!("{}. `{}`\n", i + 1, path));
            if let Some(mtime) = modified {
                let datetime: DateTime<Utc> = (*mtime).into();
                let formatted_time = datetime.format("%Y-%m-%d %H:%M");
                output.push_str(&format!("   Modified: {formatted_time}\n"));
            }
            output.push('\n');
        }

        output.push_str("**Next Steps:**\n");
        output.push_str("- Load a plan with load_plan\n");
        output.push_str("- Execute a plan directly");

        let metadata = json!({
            "plans_count": plans.len(),
            "plans": plans.iter().map(|(p, _)| p).collect::<Vec<_>>()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

rustycode_tools_api::define_tool! {
    pub struct ApprovePlanTool;

    name: "approve_plan",
    description: r#"Approve a plan for execution.

**Use cases:**
- Mark a plan as ready to execute
- User confirmation before execution
- Prevent accidental plan execution

**Parameters:**
- `plan_id`: Plan ID to approve

**Example:**
```json
{
  "plan_id": "plan-abc123"
}
```

**Returns:**
- Confirmation of plan approval

**Note:** This is a planning tool. In a real implementation, this would
update the plan status in a plan store and potentially trigger user
confirmation prompts."#,
    permission: ToolPermission::Read,

    execute(params: ApprovePlanParams, _ctx) {
        let plan_id = &params.plan_id;

        let output = format!(
            "**Plan Approved**\n\n✅ Plan `{plan_id}` has been approved for execution.\n\n\
            **Status:** Ready to execute\n\
            **Next Steps:**\n\
            - Execute the plan when ready\n\
            - Or make additional changes before execution"
        );

        let metadata = json!({
            "plan_id": plan_id,
            "status": "approved",
            "approved_at": Utc::now().to_rfc3339()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;

    #[test]
    fn test_create_plan_tool_metadata() {
        let tool = CreatePlanFromTemplateTool;
        assert_eq!(tool.name(), "create_plan_from_template");
        assert!(tool.description().contains("template"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_create_plan_parameters_schema() {
        let tool = CreatePlanFromTemplateTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        // Macro-generated schema uses schemars; check required fields exist
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("template")));
        assert!(required.contains(&json!("task")));
        assert!(required.contains(&json!("summary")));
    }

    #[test]
    fn test_save_plan_tool_metadata() {
        let tool = SavePlanTool;
        assert_eq!(tool.name(), "save_plan");
        assert!(tool.description().contains("save"));
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_load_plan_tool_metadata() {
        let tool = LoadPlanTool;
        assert_eq!(tool.name(), "load_plan");
        assert!(tool.description().contains("Load"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_list_plans_tool_metadata() {
        let tool = ListPlansTool;
        assert_eq!(tool.name(), "list_plans");
        assert!(tool.description().contains("List"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_approve_plan_tool_metadata() {
        let tool = ApprovePlanTool;
        assert_eq!(tool.name(), "approve_plan");
        assert!(tool.description().contains("approve"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    // ── Path traversal tests ───────────────────────────────────────────

    #[test]
    fn test_plan_id_rejects_traversal() {
        assert!(validate_plan_id("../etc/passwd").is_err());
        assert!(validate_plan_id("foo/bar").is_err());
        assert!(validate_plan_id("foo\\bar").is_err());
        assert!(validate_plan_id("").is_err());
        assert!(validate_plan_id("valid-plan-id").is_ok());
        assert!(validate_plan_id("plan_123").is_ok());
    }

    #[test]
    fn test_save_plan_rejects_traversal_path() {
        let tool = SavePlanTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(
            json!({
                "plan_id": "valid-id",
                "file_path": "/etc/passwd"
            }),
            &ctx,
        );
        assert!(
            result.is_err(),
            "absolute path outside workspace should be rejected"
        );
    }

    #[test]
    fn test_save_plan_rejects_traversal_plan_id() {
        let tool = SavePlanTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(
            json!({
                "plan_id": "../../etc/passwd"
            }),
            &ctx,
        );
        assert!(result.is_err(), "plan_id with .. should be rejected");
    }

    #[test]
    fn test_load_plan_rejects_traversal_path() {
        let tool = LoadPlanTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(
            json!({
                "file_path": "/etc/shadow"
            }),
            &ctx,
        );
        assert!(
            result.is_err(),
            "absolute path outside workspace should be rejected"
        );
    }

    #[test]
    fn test_list_plans_rejects_traversal_directory() {
        let tool = ListPlansTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(
            json!({
                "directory": "/etc"
            }),
            &ctx,
        );
        // validate_list_path checks that it's within workspace and is a directory
        // /etc is not within /tmp workspace, so should be rejected
        assert!(
            result.is_err(),
            "directory outside workspace should be rejected"
        );
    }
}
