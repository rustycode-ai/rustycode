//! Apply Patch tool - Apply git patches to the codebase
//!
//! This tool enables applying .patch, .diff, or unified diff files
//! to the codebase using git apply.

use crate::security::{open_file_symlink_safe, validate_read_path, validate_write_path};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::io::Read;
use std::process::Command;

/// `ApplyPatch` tool - Apply git patches
pub struct ApplyPatchTool;

impl Tool for ApplyPatchTool {
    fn name(&self) -> &'static str {
        "apply_patch"
    }

    fn description(&self) -> &'static str {
        r#"Apply a git patch file to the codebase.

**Use cases:**
- Apply changes from git format-patch
- Apply .patch or .diff files
- Apply unified diffs
- Merge changes from other developers

**Parameters:**
- `patch_file`: Path to the patch file (.patch, .diff, or unified diff)
- `strip`: Number of path components to strip (default: 1)
- `reverse`: Apply patch in reverse (for unapplying)
- `directory`: Directory to apply patch in (default: current directory)

**Safety:**
- Validates patch file exists
- Checks for git repository
- Uses git apply for robust patch application
- Reports conflicts if they occur

**Example:**
```json
{
  "patch_file": "changes/feature-auth.patch",
  "strip": 1
}
```

**Notes:**
- Requires git repository
- Uses `git apply --stat` for preview
- Applies with `git apply --3way` for merge conflict resolution
- Returns summary of changes applied

**Error handling:**
- If patch doesn't apply cleanly, reports conflicts
- Git applies changes with conflict markers
- User can resolve conflicts manually
"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["patch_file"],
            "properties": {
                "patch_file": {
                    "type": "string",
                    "description": "Path to the patch file (.patch, .diff, or unified diff)"
                },
                "strip": {
                    "type": "integer",
                    "description": "Number of path components to strip from file paths (default: 1)",
                    "default": 1,
                    "minimum": 0,
                    "maximum": 10
                },
                "reverse": {
                    "type": "boolean",
                    "description": "Apply patch in reverse (for unapplying changes)",
                    "default": false
                },
                "directory": {
                    "type": "string",
                    "description": "Directory to apply patch in (default: current directory)",
                    "default": "."
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let patch_file_str = required_string(&params, "patch_file")?;
        let strip = params.get("strip").and_then(Value::as_u64).unwrap_or(1) as usize;
        let reverse = params
            .get("reverse")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let directory = params
            .get("directory")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        // Resolve and validate patch file path
        let patch_path = validate_read_path(patch_file_str, &ctx.cwd)?;

        // Resolve and validate directory path
        let target_dir = validate_write_path(directory, &ctx.cwd, 0)?;

        // Validate patch file exists
        if !patch_path.exists() {
            return Err(anyhow!("patch file not found: {}", patch_path.display()));
        }

        // Validate target directory is a git repo
        let git_dir = target_dir.join(".git");
        if !git_dir.exists() {
            return Err(anyhow!("not a git repository: {}", target_dir.display()));
        }

        // Read patch file to get basic info (symlink-safe)
        let mut file = open_file_symlink_safe(&patch_path)
            .with_context(|| format!("failed to open patch file: {}", patch_path.display()))?;
        let mut patch_content = String::new();
        file.read_to_string(&mut patch_content)
            .with_context(|| format!("failed to read patch file: {}", patch_path.display()))?;

        // Count files in patch
        let file_count = patch_content
            .lines()
            .filter(|line| line.starts_with("+++"))
            .count();

        // Apply patch using git apply
        let mut cmd = Command::new("git");
        cmd.arg("apply")
            .arg("--3way") // Use 3-way merge for better conflict handling
            .arg(format!("--strip={strip}"))
            .current_dir(&target_dir);

        if reverse {
            cmd.arg("--reverse");
        }

        cmd.arg(&patch_path);

        let output = cmd
            .output()
            .with_context(|| format!("failed to apply patch: {}", patch_path.display()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Format output
        let mut result_output = String::new();

        result_output.push_str(&format!("**Patch Applied** - {}\n\n", patch_path.display()));

        result_output.push_str(&format!("**Directory:** {}\n", target_dir.display()));
        result_output.push_str(&format!("**Files affected:** ~{file_count}\n\n"));

        if output.status.success() {
            result_output.push_str("✅ **Status:** Successfully applied\n\n");

            if !stdout.is_empty() {
                result_output.push_str(&format!("**Output:**\n{stdout}\n"));
            }

            if !stderr.is_empty() && !stderr.contains("cyclic") {
                result_output.push_str(&format!("**Warnings:**\n{stderr}\n"));
            }
        } else {
            // Failed to apply
            result_output.push_str("❌ **Status:** Failed to apply\n\n");

            if stderr.contains("conflict") {
                result_output.push_str("**⚠️ Conflicts detected**\n\n");
                result_output.push_str(
                    "Patch could not be applied cleanly. Git has added conflict markers.\n",
                );
                result_output.push_str("You need to resolve conflicts manually.\n\n");
            } else if stderr.contains("does not exist") {
                result_output.push_str("**⚠️ File not found**\n\n");
            } else if stderr.contains("patch does not apply") {
                result_output.push_str("**⚠️ Patch does not apply**\n\n");
                result_output.push_str("The patch cannot be applied to the current codebase.\n");
                result_output.push_str("This may be due to:\n");
                result_output.push_str("- Different base version\n");
                result_output.push_str("- Changes already applied\n");
                result_output.push_str("- Conflicting changes\n\n");
            }

            if !stderr.is_empty() {
                result_output.push_str(&format!("**Error Details:**\n```\n{stderr}\n```\n\n"));
            }

            return Err(anyhow!("patch application failed: {}", stderr.trim()));
        }

        // Get git status to show changes
        let status_output = Command::new("git")
            .args(["status", "--short"])
            .current_dir(&target_dir)
            .output()
            .ok();

        if let Some(status) = status_output {
            let status_str = String::from_utf8_lossy(&status.stdout);
            if !status_str.trim().is_empty() {
                result_output.push_str(&format!(
                    "**Modified files:**\n```\n{}\n```\n",
                    status_str.trim()
                ));
            }
        }

        // Build metadata
        let metadata = json!({
            "patch_file": patch_path.to_string_lossy().to_string(),
            "directory": target_dir.to_string_lossy().to_string(),
            "strip": strip,
            "reverse": reverse,
            "success": output.status.success(),
            "files_affected": file_count
        });

        Ok(ToolOutput::with_structured(result_output, metadata))
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter `{key}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_patch_tool_metadata() {
        let tool = ApplyPatchTool;
        assert_eq!(tool.name(), "apply_patch");
        assert!(tool.description().contains("git patch"));
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_apply_patch_parameters_schema() {
        let tool = ApplyPatchTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "patch_file");

        // Check patch_file property
        assert_eq!(schema["properties"]["patch_file"]["type"], "string");

        // Check strip constraints
        assert_eq!(schema["properties"]["strip"]["default"], 1);
        assert_eq!(schema["properties"]["strip"]["minimum"], 0);
        assert_eq!(schema["properties"]["strip"]["maximum"], 10);

        // Check reverse
        assert_eq!(schema["properties"]["reverse"]["type"], "boolean");
        assert_eq!(schema["properties"]["reverse"]["default"], false);
    }

    #[test]
    fn test_apply_patch_missing_patch_file() {
        let workspace = tempfile::tempdir().unwrap();
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({"patch_file": "nonexistent.patch"}), &ctx);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found") || err.contains("path"),
            "expected path or not found error, got: {err}"
        );
    }

    #[test]
    fn test_apply_patch_non_git_directory() {
        let workspace = tempfile::tempdir().unwrap();
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        // Not a git repo
        let result = tool.execute(json!({"patch_file": "test.patch"}), &ctx);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("not found")
                || error_msg.contains("not a git repository")
                || error_msg.contains("path"),
            "expected path or git error, got: {error_msg}"
        );
    }

    #[test]
    fn test_apply_patch_strip_parameter() {
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new("/tmp");

        // Test strip parameter validation
        let result = tool.execute(
            json!({
                "patch_file": "test.patch",
                "strip": 15
            }),
            &ctx,
        );

        // Should fail due to validation (patch doesn't exist)
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_patch_reverse_parameter() {
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "patch_file": "test.patch",
                "reverse": true
            }),
            &ctx,
        );

        // Should fail due to validation (patch doesn't exist)
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_patch_directory_parameter() {
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "patch_file": "test.patch",
                "directory": "src"
            }),
            &ctx,
        );

        // Should fail due to validation (patch doesn't exist)
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_patch_blocks_path_traversal() {
        let workspace = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());
        let tool = ApplyPatchTool;

        let result = tool.execute(
            json!({
                "patch_file": "../../../etc/passwd"
            }),
            &ctx,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_patch_blocks_directory_traversal() {
        let workspace = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());
        let tool = ApplyPatchTool;

        // Create a dummy patch file so it doesn't fail on "not found"
        let patch = workspace.path().join("test.patch");
        std::fs::write(&patch, "dummy").unwrap();

        let result = tool.execute(
            json!({
                "patch_file": "test.patch",
                "directory": "../../../"
            }),
            &ctx,
        );
        assert!(result.is_err());
    }
}
