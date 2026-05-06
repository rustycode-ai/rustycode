use crate::security::validate_list_path;
use crate::truncation::{truncate_items, LIST_MAX_ITEMS};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use walkdir::WalkDir;

pub struct ListDirTool;

impl Tool for ListDirTool {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "List all files and directories in a specified path. Use this to explore the codebase structure, find files in a directory, or see what's in a folder. Supports recursive listing and filtering by file type or extension."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path relative to current workspace (alias: file_path)"
                },
                "file_path": {
                    "type": "string",
                    "description": "Alias for path"
                },
                "recursive": { "type": "boolean", "description": "List directories recursively" },
                "max_depth": { "type": "integer", "description": "Maximum depth for recursive listing" },
                "filter": {
                    "type": "string",
                    "description": "Filter entries by type (file/dir/all) or extension (e.g., '.rs', '.md')"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }
        let path_str = params
            .get("path")
            .and_then(Value::as_str)
            .or_else(|| params.get("file_path").and_then(Value::as_str))
            .unwrap_or(".");

        let path = validate_list_path(path_str, &ctx.cwd, !ctx.allow_outside_workspace)?;

        let path_display = path.display().to_string();
        let recursive = params
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let max_depth = params.get("max_depth").and_then(Value::as_u64).unwrap_or(3) as usize;
        let filter = params.get("filter").and_then(|v| v.as_str());

        let mut entries = Vec::new();

        if recursive {
            for entry in WalkDir::new(&path)
                .max_depth(max_depth)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                let file_type = entry.file_type();
                let kind = if file_type.is_dir() {
                    "dir"
                } else if file_type.is_file() {
                    "file"
                } else {
                    "other"
                };

                if let Some(filter_str) = filter {
                    if matches!(filter_str.to_lowercase().as_str(), "file" | "dir" | "all") {
                        if filter_str != "all" && kind != filter_str {
                            continue;
                        }
                    }
                    else if filter_str.starts_with('.')
                        && !entry.path().to_string_lossy().ends_with(filter_str)
                    {
                        continue;
                    }
                }

                let relative_path = entry
                    .path()
                    .strip_prefix(&path)
                    .unwrap_or(entry.path())
                    .display()
                    .to_string();
                entries.push(format!("{relative_path}: {kind}"));
            }
        } else {
            let mut dir_entries = fs::read_dir(&path)?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let file_type = entry.file_type().ok();
                    let kind = match file_type {
                        Some(ft) if ft.is_dir() => "dir",
                        Some(ft) if ft.is_file() => "file",
                        _ => "other",
                    };

                    if let Some(filter_str) = filter {
                        if matches!(filter_str.to_lowercase().as_str(), "file" | "dir" | "all") {
                            if filter_str != "all" && kind != filter_str {
                                return None;
                            }
                        }
                        else if filter_str.starts_with('.')
                            && !entry.file_name().to_string_lossy().ends_with(filter_str)
                        {
                            return None;
                        }
                    }

                    Some(format!("{}: {}", entry.file_name().to_string_lossy(), kind))
                })
                .collect::<Vec<_>>();
            entries.append(&mut dir_entries);
        }

        let total_count = entries.len();
        entries.sort();

        let truncated = truncate_items(entries, LIST_MAX_ITEMS, &path_display);

        let output_text = format!(
            "**{}** ({} items{})\n\n{}",
            path_display,
            total_count,
            if recursive {
                format!(", recursive (depth={max_depth})")
            } else {
                String::new()
            },
            truncated.as_str()
        );

        let mut metadata = truncated.into_metadata();
        metadata["path"] = json!(path_display);
        metadata["total_items"] = json!(total_count);
        metadata["recursive"] = json!(recursive);
        if recursive {
            metadata["max_depth"] = json!(max_depth);
        }
        if let Some(filter_str) = filter {
            metadata["filter"] = json!(filter_str);
        }

        Ok(ToolOutput::with_structured(output_text, metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_list_dir_tool_metadata() {
        let tool = ListDirTool;
        assert_eq!(tool.name(), "list_dir");
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_list_dir_parameters_schema() {
        let tool = ListDirTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["required"].is_array());
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "path");

        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["properties"]["recursive"]["type"], "boolean");
        assert_eq!(schema["properties"]["max_depth"]["type"], "integer");
        assert_eq!(schema["properties"]["filter"]["type"], "string");
    }

    #[test]
    fn test_list_dir_lists_current_directory() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(json!({ "path": "." }), &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        let text = output.text;
        assert!(text.contains("items"));
    }

    #[test]
    fn test_list_dir_nonexistent_path() {
        let tool = ListDirTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "path": "/nonexistent_dir_xyz" }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_dir_with_filter_type() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(json!({ "path": ".", "filter": "file" }), &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_dir_with_filter_extension() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(json!({ "path": ".", "filter": ".rs" }), &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_dir_recursive() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(
            json!({ "path": ".", "recursive": true, "max_depth": 1 }),
            &ctx,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_dir_file_path_alias() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(json!({ "file_path": "." }), &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_dir_default_path() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_dir_metadata_fields() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(json!({ "path": "." }), &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        let metadata = output.structured.as_ref().unwrap();
        assert!(metadata.get("path").is_some());
        assert!(metadata.get("total_items").is_some());
        assert!(metadata.get("recursive").is_some());
    }

    #[test]
    fn test_list_dir_recursive_metadata() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(
            json!({ "path": ".", "recursive": true }),
            &ctx,
        );
        assert!(result.is_ok());

        let output = result.unwrap();
        let metadata = output.structured.as_ref().unwrap();
        assert_eq!(metadata["recursive"], true);
        assert!(metadata.get("max_depth").is_some());
    }

    #[test]
    fn test_list_dir_filter_metadata() {
        let tool = ListDirTool;
        let ctx = ToolContext::new(".");

        let result = tool.execute(json!({ "path": ".", "filter": ".rs" }), &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        let metadata = output.structured.as_ref().unwrap();
        assert_eq!(metadata["filter"], ".rs");
    }
}
