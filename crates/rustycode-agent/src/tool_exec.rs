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

/// Truncate tool output to fit within budget, preserving error context in the tail.
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

    let tail_start = output
        .char_indices()
        .rev()
        .skip_while(|(i, _)| output.len() - *i > tail_bytes)
        .last()
        .map_or(0, |(i, _)| i);

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
        "Edit" | "edit" | "text_editor_20250728" | "search_replace" => "edit_file",
        "Read" | "read" | "view" => "read_file",
        "Write" | "Create" | "create" => "write_file",
        "Bash" | "Shell" | "shell" | "execute" | "run_command" => "bash",
        "Grep" | "Search" | "search" => "grep",
        "Glob" | "Find" | "find" => "glob",
        "NotebookEdit" | "notebook_edit" => "notebook_edit",
        "WebFetch" | "web_fetch" | "fetch" => "web_fetch",
        "LSP" | "lsp" => "lsp",
        _ => name,
    }
}

