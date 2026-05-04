use rustycode_protocol::ToolCall;
use rustycode_tools::{ToolContext, ToolRegistry};
use std::path::Path;

/// Execute a named tool via the shared registry.
pub fn execute_tool(
    cwd: &Path,
    tool_name: &str,
    tool_json: &str,
    tool_registry: &ToolRegistry,
) -> (String, bool) {
    let resolved_name = normalize_tool_name(tool_name);
    let args: serde_json::Value = match serde_json::from_str(tool_json) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("Error: Failed to parse tool arguments: {e}");
            return (msg, true);
        }
    };

    let call = ToolCall {
        call_id: "agent".to_string(),
        name: resolved_name.to_string(),
        arguments: args,
    };

    let ctx = ToolContext::new(cwd);
    let result = tool_registry.execute(&call, &ctx);

    if result.success {
        (result.output, false)
    } else {
        let msg = result
            .error
            .unwrap_or_else(|| "Error executing tool".to_string());
        (msg, true)
    }
}

/// Truncate tool output to fit within budget, preserving pagination info in the tail.
pub fn truncate_tool_output(output: &str, max_bytes: usize) -> String {
    if output.len() <= max_bytes {
        return output.to_string();
    }

    let out_lower = output.to_lowercase();
    let has_errors = out_lower.contains("error")
        || out_lower.contains("traceback")
        || out_lower.contains("failed")
        || out_lower.contains("segmentation fault")
        || out_lower.contains("command not found");

    // When there are errors, preserve more of the tail (error details + pagination).
    // Otherwise, keep the last quarter for pagination/hint lines.
    let (head_bytes, tail_bytes) = if has_errors {
        (max_bytes / 6, max_bytes * 5 / 6)
    } else {
        (max_bytes / 4, max_bytes * 3 / 4)
    };

    let head_end = output
        .char_indices()
        .take_while(|(i, _)| *i < head_bytes)
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());

    let tail_start_offset = output.len().saturating_sub(tail_bytes);
    let tail_start = output
        .char_indices()
        .find(|(i, _)| *i >= tail_start_offset)
        .map_or(output.len(), |(i, _)| i);

    if tail_start > head_end {
        let skipped = tail_start - head_end;
        format!(
            "{}\n\n[...{skipped} bytes truncated...]\n\n{}",
            &output[..head_end],
            &output[tail_start..]
        )
    } else {
        output.to_string()
    }
}

/// Normalize tool names from different providers to our canonical names.
fn normalize_tool_name(name: &str) -> &str {
    match name {
        "Edit" | "edit" | "text_editor_20250728" => "edit_file",
        "Read" | "read" | "view" => "read_file",
        "Write" | "Create" | "create" => "write_file",
        "Bash" | "Shell" | "shell" | "execute" | "run_command" => "bash",
        "PowerShell" | "pwsh" => "powershell",
        "Grep" | "Search" | "search" => "grep",
        "Glob" | "Find" | "find" => "glob",
        "NotebookEdit" | "notebook_edit" => "notebook_edit",
        "WebFetch" | "web_fetch" | "fetch" => "web_fetch",
        "LSP" | "lsp" => "lsp",
        "ApplyPatch" | "apply_patch" | "patch" => "apply_patch",
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_output_unchanged() {
        let output = "hello world";
        assert_eq!(truncate_tool_output(output, 100), output);
    }

    #[test]
    fn truncate_long_output_with_error() {
        let mut lines: Vec<String> = (0..50).map(|i| format!("ok line {i}")).collect();
        lines.insert(5, "error: something failed".to_string());
        let output = lines.join("\n");
        assert!(output.len() > 200);
        let truncated = truncate_tool_output(&output, 100);
        assert!(truncated.contains("truncated"));
        assert!(truncated.len() < output.len());
    }

    #[test]
    fn truncate_long_output_no_error() {
        let output = "normal output line\n".repeat(50);
        assert!(output.len() > 500);
        let truncated = truncate_tool_output(&output, 200);
        assert!(truncated.contains("truncated"));
        assert!(truncated.len() < output.len());
    }

    #[test]
    fn normalize_tool_names() {
        assert_eq!(normalize_tool_name("Edit"), "edit_file");
        assert_eq!(normalize_tool_name("edit"), "edit_file");
        assert_eq!(normalize_tool_name("text_editor_20250728"), "edit_file");
        assert_eq!(normalize_tool_name("Read"), "read_file");
        assert_eq!(normalize_tool_name("view"), "read_file");
        assert_eq!(normalize_tool_name("Write"), "write_file");
        assert_eq!(normalize_tool_name("Create"), "write_file");
        assert_eq!(normalize_tool_name("Bash"), "bash");
        assert_eq!(normalize_tool_name("Shell"), "bash");
        assert_eq!(normalize_tool_name("execute"), "bash");
        assert_eq!(normalize_tool_name("Grep"), "grep");
        assert_eq!(normalize_tool_name("Search"), "grep");
        assert_eq!(normalize_tool_name("Glob"), "glob");
        assert_eq!(normalize_tool_name("Find"), "glob");
        assert_eq!(normalize_tool_name("NotebookEdit"), "notebook_edit");
        assert_eq!(normalize_tool_name("WebFetch"), "web_fetch");
        assert_eq!(normalize_tool_name("fetch"), "web_fetch");
        assert_eq!(normalize_tool_name("LSP"), "lsp");
        assert_eq!(normalize_tool_name("ApplyPatch"), "apply_patch");
        assert_eq!(normalize_tool_name("patch"), "apply_patch");
        assert_eq!(normalize_tool_name("apply_patch"), "apply_patch");
        assert_eq!(normalize_tool_name("unknown_tool"), "unknown_tool");
    }

    #[test]
    fn truncate_empty_output() {
        assert_eq!(truncate_tool_output("", 100), "");
    }

    #[test]
    fn truncate_exact_boundary() {
        let output = "x".repeat(50);
        assert_eq!(truncate_tool_output(&output, 50), output);
    }
}
