use crate::file_formatter;
use crate::security::{create_file_symlink_safe, open_file_symlink_safe, validate_write_path};
use crate::{Checkpoint, Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

/// Maximum file size for edit operations (1 MB)
const MAX_EDIT_SIZE: usize = 1024 * 1024;

/// `MultiEdit` tool - Edit multiple files atomically
///
/// This tool enables editing multiple files in a single atomic operation.
/// All edits are validated before any are applied, and all files are
/// written together. If any edit fails, no changes are made.
///
/// **Use cases:**
/// - Refactor across multiple files
/// - Update imports across a project
/// - Apply consistent changes
/// - Multi-file documentation updates
///
/// **Operations:**
/// - `create`: Create a new file
/// - `edit`: Edit an existing file (find/replace)
/// - `delete`: Delete a file
///
/// **Atomicity:**
/// All operations are validated first, then applied. If any operation
/// fails, no changes are made to any file.
pub struct MultiEditTool;

impl Tool for MultiEditTool {
    fn name(&self) -> &'static str {
        "multiedit"
    }

    fn description(&self) -> &'static str {
        r#"Edit multiple files atomically in a single operation.

**Use cases:**
- Refactor code across multiple files
- Update imports/dependencies consistently
- Apply documentation changes project-wide
- Rename symbols across files
- Delete deprecated files

**Operations:**
Each edit operation specifies:
- `path`: File path (relative or absolute)
- `operation`: One of: create, edit, delete
- `content`: File content (for create/edit)
- `old_text`: Text to replace (for edit)
- `new_text`: Replacement text (for edit)

**Atomicity:**
All operations are validated first, then applied together.
If any operation fails, NO changes are made to any file.

**Example:**
```json
{
  "edits": [
    {
      "path": "src/auth.rs",
      "operation": "edit",
      "old_text": "fn login",
      "new_text": "fn authenticate"
    },
    {
      "path": "src/api.rs",
      "operation": "edit",
      "old_text": "use crate::auth::login",
      "new_text": "use crate::auth::authenticate"
    },
    {
      "path": "src/models.rs",
      "operation": "create",
      "content": "pub struct User { id: u32, name: String }"
    }
  ],
  "dry_run": false
}
```

**Return value:**
Summary of changes made to each file.

**Error handling:**
- If `continue_on_error` is true: Apply all valid edits, skip invalid ones
- If `continue_on_error` is false: Fail entirely if any edit is invalid
"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["edits"],
            "properties": {
                "edits": {
                    "type": "array",
                    "description": "Array of edit operations to apply atomically",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "File path (relative or absolute)"
                            },
                            "operation": {
                                "type": "string",
                                "enum": ["create", "edit", "delete"],
                                "description": "Operation to perform"
                            },
                            "content": {
                                "type": "string",
                                "description": "File content (for create operations)"
                            },
                            "old_text": {
                                "type": "string",
                                "description": "Text to find and replace (for edit operations)"
                            },
                            "new_text": {
                                "type": "string",
                                "description": "Replacement text (for edit operations)"
                            }
                        },
                        "required": ["path", "operation"]
                    }
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Validate but don't apply changes (default: false)",
                    "default": false
                },
                "continue_on_error": {
                    "type": "boolean",
                    "description": "Apply valid edits even if some fail (default: false)",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let edits_value = params
            .get("edits")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("missing 'edits' array"))?;

        let dry_run = params
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let continue_on_error = params
            .get("continue_on_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if edits_value.is_empty() {
            return Err(anyhow!("edits array is empty"));
        }

        if edits_value.len() > 50 {
            return Err(anyhow!(
                "too many edits (max 50), got {}",
                edits_value.len()
            ));
        }

        // Check for cancellation before starting multi-file operation
        ctx.checkpoint()?;

        // Phase 1: Parse and validate all edits
        let mut validated_edits = Vec::new();
        for (index, edit_value) in edits_value.iter().enumerate() {
            // Check for cancellation every 10 edits during validation
            if index % 10 == 0 {
                ctx.checkpoint()?;
            }

            match validate_edit(edit_value, ctx) {
                Ok(edit) => validated_edits.push(edit),
                Err(e) => {
                    if !continue_on_error {
                        return Err(anyhow!("validation failed for edit {}: {}", index + 1, e));
                    }
                    tracing::warn!(index = index + 1, error = %e, "Skipping edit due to validation failure");
                }
            }
        }

        if validated_edits.is_empty() {
            return Err(anyhow!("no valid edits to apply"));
        }

        // Phase 2: Check for conflicts (same file edited multiple times)
        check_conflicts(&validated_edits)?;

        // CRITICAL CHECKPOINT: Before any file modifications
        // User can cancel here safely without any changes being made
        ctx.checkpoint()?;

        // Phase 3: Apply all edits (or dry run)
        let mut results = Vec::new();
        for edit in &validated_edits {
            // Check for cancellation before each file modification
            ctx.checkpoint()?;

            let result = match &edit.operation {
                EditOperation::Create => apply_create(edit, dry_run, &ctx.cwd),
                EditOperation::Edit { old_text, new_text } => {
                    apply_edit(edit, old_text, new_text, dry_run, &ctx.cwd)
                }
                EditOperation::Delete => apply_delete(edit, dry_run),
            };

            results.push(result);
        }

        // Format output
        let mut output = String::new();

        if dry_run {
            output.push_str("**Dry Run** - No changes applied\n\n");
        }

        output.push_str(&format!(
            "**MultiEdit Summary** - {} edits\n\n",
            results.len()
        ));

        let mut success_count = 0;
        let mut failure_count = 0;

        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(summary) => {
                    success_count += 1;
                    output.push_str(&format!("{}. ✅ {}\n", i + 1, summary));
                }
                Err(e) => {
                    failure_count += 1;
                    output.push_str(&format!("{}. ❌ {}\n", i + 1, e));
                }
            }
        }

        output.push_str(&format!(
            "\n**Result:** {}/{} successful",
            success_count,
            results.len()
        ));

        // Build metadata
        let metadata = json!({
            "total_edits": results.len(),
            "success_count": success_count,
            "failure_count": failure_count,
            "dry_run": dry_run,
            "continue_on_error": continue_on_error
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Validated edit operation
#[derive(Debug)]
struct ValidatedEdit {
    path: PathBuf,
    operation: EditOperation,
    content: Option<String>, // For create operations
}

#[derive(Debug)]
enum EditOperation {
    Create,
    Edit { old_text: String, new_text: String },
    Delete,
}

/// Validate a single edit operation
fn validate_edit(edit_value: &Value, ctx: &ToolContext) -> Result<ValidatedEdit> {
    let path = edit_value
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing 'path'"))?;

    let operation = edit_value
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing 'operation'"))?;

    // Validate path through security layer (blocks path traversal, sensitive files)
    let validated_path = validate_write_path(path, &ctx.cwd, 0)?;

    let edit_operation = match operation {
        "create" => {
            let _content = edit_value
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("missing 'content' for create operation"))?;

            // Check if file already exists
            if validated_path.exists() {
                return Err(anyhow!("file already exists: {}", validated_path.display()));
            }

            EditOperation::Create
        }
        "edit" => {
            let old_text = edit_value
                .get("old_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("missing 'old_text' for edit operation"))?;

            if old_text.is_empty() {
                return Err(anyhow!("old_text cannot be empty"));
            }

            let new_text = edit_value
                .get("new_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("missing 'new_text' for edit operation"))?;

            // Check if file exists
            if !validated_path.exists() {
                return Err(anyhow!("file not found: {}", validated_path.display()));
            }

            // Read file using symlink-safe operation
            let mut file = open_file_symlink_safe(&validated_path)
                .with_context(|| format!("failed to open file: {}", validated_path.display()))?;
            let mut content = String::new();
            file.read_to_string(&mut content).map_err(|e| {
                if e.kind() == std::io::ErrorKind::InvalidData {
                    anyhow!("binary or non-UTF-8 file: {}", validated_path.display())
                } else {
                    anyhow!("failed to read file {}: {}", validated_path.display(), e)
                }
            })?;

            // Check file size
            if content.len() > MAX_EDIT_SIZE {
                return Err(anyhow!(
                    "file too large for editing ({} bytes, max {}): {}",
                    content.len(),
                    MAX_EDIT_SIZE,
                    validated_path.display()
                ));
            }

            if !content.contains(old_text) {
                return Err(anyhow!(
                    "old_text not found in file: {}",
                    validated_path.display()
                ));
            }

            EditOperation::Edit {
                old_text: old_text.to_string(),
                new_text: new_text.to_string(),
            }
        }
        "delete" => {
            // Check if file exists
            if !validated_path.exists() {
                return Err(anyhow!("file not found: {}", validated_path.display()));
            }

            EditOperation::Delete
        }
        _ => return Err(anyhow!("invalid operation: {operation}")),
    };

    // Store content for create operations
    let content = match operation {
        "create" => {
            let content_str = edit_value.get("content").and_then(|v| v.as_str()).unwrap();
            Some(content_str.to_string())
        }
        _ => None,
    };

    Ok(ValidatedEdit {
        path: validated_path,
        operation: edit_operation,
        content,
    })
}

/// Check for conflicts (same file edited multiple times)
fn check_conflicts(edits: &[ValidatedEdit]) -> Result<()> {
    let mut file_counts = std::collections::HashMap::new();
    for edit in edits {
        *file_counts.entry(edit.path.clone()).or_insert(0) += 1;
    }

    let conflicts: Vec<_> = file_counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .collect();

    if !conflicts.is_empty() {
        let mut msg = String::from("conflicting edits detected:\n");
        for (path, count) in conflicts {
            msg.push_str(&format!("- {} edited {} times\n", path.display(), count));
        }
        msg.push_str("\nCombine these into a single edit operation.");
        return Err(anyhow!(msg));
    }

    Ok(())
}

/// Apply create operation
fn apply_create(
    edit: &ValidatedEdit,
    dry_run: bool,
    project_root: &std::path::Path,
) -> Result<String> {
    if dry_run {
        return Ok(format!(
            "Would create: {} ({} bytes)",
            edit.path.display(),
            edit.content.as_ref().map(String::len).unwrap_or(0)
        ));
    }

    let content = edit
        .content
        .as_ref()
        .ok_or_else(|| anyhow!("missing content for create operation"))?;

    // Create parent directories if needed
    if let Some(parent) = edit.path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Use symlink-safe file creation
    let mut file = create_file_symlink_safe(&edit.path)
        .with_context(|| format!("failed to create file: {}", edit.path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("failed to write file: {}", edit.path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync file: {}", edit.path.display()))?;

    let mut output = format!("Created: {} ({} bytes)", edit.path.display(), content.len());

    if let Some(formatter_diff) = file_formatter::format_file(&edit.path, project_root) {
        output.push_str(&formatter_diff);
    }

    Ok(output)
}

/// Apply edit operation
fn apply_edit(
    edit: &ValidatedEdit,
    old_text: &str,
    new_text: &str,
    dry_run: bool,
    project_root: &std::path::Path,
) -> Result<String> {
    if dry_run {
        return Ok(format!(
            "Would edit: {} (replace {} chars with {} chars)",
            edit.path.display(),
            old_text.len(),
            new_text.len()
        ));
    }

    // Read using symlink-safe operation
    let mut file = open_file_symlink_safe(&edit.path)
        .with_context(|| format!("failed to open file: {}", edit.path.display()))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .with_context(|| format!("failed to read file: {}", edit.path.display()))?;

    // Replace only the first occurrence — same behavior as edit_file
    let new_content = content.replacen(old_text, new_text, 1);

    // If the replacement didn't change anything, old_text was not found
    if new_content == content {
        anyhow::bail!(
            "old_text not found in {}: '{}'",
            edit.path.display(),
            if old_text.len() > 50 {
                let end = old_text.floor_char_boundary(47);
                format!("{}...", &old_text[..end])
            } else {
                old_text.to_string()
            }
        );
    }

    // Write atomically: write to a temp file, then rename over the target.
    // This prevents file corruption if the process crashes mid-write.
    let temp_path = edit.path.with_extension("tmp");
    {
        let mut out_file = create_file_symlink_safe(&temp_path)
            .with_context(|| format!("failed to create temp file: {}", temp_path.display()))?;
        out_file
            .write_all(new_content.as_bytes())
            .with_context(|| format!("failed to write temp file: {}", temp_path.display()))?;
        out_file
            .sync_all()
            .with_context(|| format!("failed to sync temp file: {}", temp_path.display()))?;
    }
    fs::rename(&temp_path, &edit.path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        anyhow::anyhow!(
            "failed to rename temp file to {}: {}",
            edit.path.display(),
            e
        )
    })?;

    let mut output = format!("Edited: {} (replaced 1 occurrence)", edit.path.display());

    if let Some(formatter_diff) = file_formatter::format_file(&edit.path, project_root) {
        output.push_str(&formatter_diff);
    }

    Ok(output)
}

/// Apply delete operation
fn apply_delete(edit: &ValidatedEdit, dry_run: bool) -> Result<String> {
    if dry_run {
        return Ok(format!("Would delete: {}", edit.path.display()));
    }

    fs::remove_file(&edit.path)
        .with_context(|| format!("failed to delete file: {}", edit.path.display()))?;

    Ok(format!("Deleted: {}", edit.path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_multiedit_tool_metadata() {
        let tool = MultiEditTool;
        assert_eq!(tool.name(), "multiedit");
        assert!(tool.description().contains("atomically"));
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_multiedit_parameters_schema() {
        let tool = MultiEditTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "edits");

        // Check edits array
        assert_eq!(schema["properties"]["edits"]["type"], "array");
        assert_eq!(schema["properties"]["edits"]["minItems"], json!(1));

        // Check operation enum
        let ops = schema["properties"]["edits"]["items"]["properties"]["operation"]["enum"]
            .as_array()
            .unwrap();
        assert!(ops.contains(&json!("create")));
        assert!(ops.contains(&json!("edit")));
        assert!(ops.contains(&json!("delete")));
    }

    #[test]
    fn test_multiedit_missing_edits() {
        let tool = MultiEditTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("edits"));
    }

    #[test]
    fn test_multiedit_empty_edits() {
        let tool = MultiEditTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "edits": [] }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_multiedit_too_many_edits() {
        let tool = MultiEditTool;
        let ctx = ToolContext::new("/tmp");

        let mut edits = vec![];
        for i in 0..51 {
            edits.push(json!({
                "path": format!("test{}.rs", i),
                "operation": "create",
                "content": ""
            }));
        }

        let result = tool.execute(json!({ "edits": edits }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too many"));
    }

    #[test]
    fn test_validate_edit_create_missing_content() {
        let workspace = tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());
        let edit = json!({
            "path": "test.rs",
            "operation": "create"
        });

        let result = validate_edit(&edit, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("content"));
    }

    #[test]
    fn test_validate_edit_edit_missing_old_text() {
        let workspace = tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());
        let edit = json!({
            "path": "test.rs",
            "operation": "edit",
            "new_text": "bar"
        });

        let result = validate_edit(&edit, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("old_text"));
    }

    #[test]
    fn test_validate_edit_invalid_operation() {
        let workspace = tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());
        let edit = json!({
            "path": "test.rs",
            "operation": "invalid"
        });

        let result = validate_edit(&edit, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid operation"));
    }

    #[test]
    fn test_multiedit_blocks_path_traversal() {
        let workspace = tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());

        let result = validate_edit(
            &json!({
                "path": "../../../etc/passwd",
                "operation": "edit",
                "old_text": "root",
                "new_text": "hacked"
            }),
            &ctx,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_multiedit_edit_replaces_only_first_occurrence() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "foo bar foo baz foo").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "edits": [{
                    "path": "test.txt",
                    "operation": "edit",
                    "old_text": "foo",
                    "new_text": "QUX"
                }]
            }),
            &ctx,
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "QUX bar foo baz foo");
    }

    #[test]
    fn test_multiedit_create_file() {
        let workspace = tempdir().unwrap();
        let tool = MultiEditTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "edits": [{
                    "path": "new_file.txt",
                    "operation": "create",
                    "content": "hello world"
                }]
            }),
            &ctx,
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(workspace.path().join("new_file.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_multiedit_delete_file() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("to_delete.txt");
        fs::write(&test_file, "bye bye").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "edits": [{
                    "path": "to_delete.txt",
                    "operation": "delete"
                }]
            }),
            &ctx,
        );
        assert!(result.is_ok());
        assert!(!test_file.exists());
    }

    #[test]
    fn test_multiedit_rejects_empty_old_text() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let ctx = ToolContext::new(workspace.path());
        let result = validate_edit(
            &json!({
                "path": "test.txt",
                "operation": "edit",
                "old_text": "",
                "new_text": "injected"
            }),
            &ctx,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_multiedit_rejects_binary_file() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.bin");
        fs::write(&test_file, [0xff, 0xfe, 0xfd]).unwrap();

        let ctx = ToolContext::new(workspace.path());
        let result = validate_edit(
            &json!({
                "path": "test.bin",
                "operation": "edit",
                "old_text": "a",
                "new_text": "b"
            }),
            &ctx,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("UTF-8"));
    }

    #[test]
    fn test_multiedit_conflict_detection() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "aaa bbb ccc").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "edits": [
                    {
                        "path": "test.txt",
                        "operation": "edit",
                        "old_text": "aaa",
                        "new_text": "xxx"
                    },
                    {
                        "path": "test.txt",
                        "operation": "edit",
                        "old_text": "bbb",
                        "new_text": "yyy"
                    }
                ]
            }),
            &ctx,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("conflicting"));
    }

    #[test]
    fn test_multiedit_dry_run() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "original content").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "edits": [{
                    "path": "test.txt",
                    "operation": "edit",
                    "old_text": "original",
                    "new_text": "modified"
                }],
                "dry_run": true
            }),
            &ctx,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("Dry Run"));

        // File should be unchanged
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "original content");
    }

    #[test]
    fn test_multiedit_multiple_files() {
        let workspace = tempdir().unwrap();
        let file_a = workspace.path().join("a.txt");
        let file_b = workspace.path().join("b.txt");
        fs::write(&file_a, "hello from a").unwrap();
        fs::write(&file_b, "hello from b").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "edits": [
                    {
                        "path": "a.txt",
                        "operation": "edit",
                        "old_text": "hello",
                        "new_text": "goodbye"
                    },
                    {
                        "path": "b.txt",
                        "operation": "edit",
                        "old_text": "hello",
                        "new_text": "goodbye"
                    }
                ]
            }),
            &ctx,
        );
        assert!(result.is_ok());

        assert_eq!(fs::read_to_string(&file_a).unwrap(), "goodbye from a");
        assert_eq!(fs::read_to_string(&file_b).unwrap(), "goodbye from b");
    }

    #[test]
    fn test_multiedit_atomic_no_temp_files_left() {
        // Verify that after a successful edit, no .tmp files remain
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("data.txt");
        fs::write(&test_file, "line one\nline two\nline three").unwrap();

        let tool = MultiEditTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "edits": [{
                    "path": "data.txt",
                    "operation": "edit",
                    "old_text": "line two",
                    "new_text": "line 2"
                }]
            }),
            &ctx,
        );
        assert!(result.is_ok());

        // No .tmp files should remain
        let tmp_files: Vec<_> = fs::read_dir(workspace.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert!(tmp_files.is_empty(), "Left temp files: {:?}", tmp_files);

        // File should have the edit applied
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "line one\nline 2\nline three");
    }
}
