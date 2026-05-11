use rustycode_protocol::tool_names as tn;
use rustycode_protocol::ToolCall;
use rustycode_tools::{ToolContext, ToolRegistry};
use rustycode_tools_api::MessageSender;
use std::path::Path;
use std::sync::Arc;

/// Execute a named tool via the shared registry.
pub fn execute_tool(
    cwd: &Path,
    tool_name: &str,
    tool_json: &str,
    tool_registry: &ToolRegistry,
    message_sender: Option<Arc<dyn MessageSender>>,
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

    let mut ctx = ToolContext::new(cwd);
    if let Some(sender) = message_sender {
        ctx = ctx.with_message_sender(sender);
    }
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
    rustycode_protocol::text::truncate_tool_output(output, max_bytes)
}

/// Normalize tool names from different providers to our canonical names.
fn normalize_tool_name(name: &str) -> &str {
    tn::normalize_tool_name(name)
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
        assert_eq!(normalize_tool_name("Edit"), "Edit");
        assert_eq!(normalize_tool_name("edit"), "Edit");
        assert_eq!(normalize_tool_name("text_editor_20250728"), "Edit");
        assert_eq!(normalize_tool_name("Read"), "Read");
        assert_eq!(normalize_tool_name("view"), "Read");
        assert_eq!(normalize_tool_name("Write"), "Write");
        assert_eq!(normalize_tool_name("Create"), "Write");
        assert_eq!(normalize_tool_name("Bash"), "Bash");
        assert_eq!(normalize_tool_name("Shell"), "Bash");
        assert_eq!(normalize_tool_name("execute"), "Bash");
        assert_eq!(normalize_tool_name("Grep"), "Grep");
        assert_eq!(normalize_tool_name("Search"), "Grep");
        assert_eq!(normalize_tool_name("Glob"), "Glob");
        assert_eq!(normalize_tool_name("Find"), "Glob");
        assert_eq!(normalize_tool_name("NotebookEdit"), "NotebookEdit");
        assert_eq!(normalize_tool_name("WebFetch"), "WebFetch");
        assert_eq!(normalize_tool_name("fetch"), "WebFetch");
        assert_eq!(normalize_tool_name("LSP"), "LspDiagnostics");
        assert_eq!(normalize_tool_name("ApplyPatch"), "ApplyPatch");
        assert_eq!(normalize_tool_name("patch"), "ApplyPatch");
        assert_eq!(normalize_tool_name("ApplyPatch"), "ApplyPatch");
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
