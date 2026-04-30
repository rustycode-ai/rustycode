//! Tool call formatting for OpenAI-compatible providers

use super::types::OpenAiToolCall;

/// Format tool calls into a markdown code block for inclusion in content.
///
/// This is the standard formatting used across all OpenAI-compatible providers
/// when tool calls are present in the response.
///
/// # Arguments
/// * `tool_calls` - The tool calls extracted from the response
///
/// # Returns
/// A formatted string containing the tool calls as JSON in a markdown block.
///
/// # Example
/// ```rust,ignore
/// let content = format_tool_calls_to_content(&tool_calls);
/// // Returns: "```tool\n[{...}]\n```"
/// ```
pub fn format_tool_calls_to_content(tool_calls: &[OpenAiToolCall]) -> String {
    let tool_calls_json: Vec<serde_json::Value> = tool_calls
        .iter()
        .map(|tc| {
            serde_json::json!({
                "id": tc.id,
                "type": tc.tool_type,
                "function": {
                    "name": tc.function.name,
                    "arguments": tc.function.arguments,
                }
            })
        })
        .collect();

    serde_json::to_string_pretty(&tool_calls_json).unwrap_or_else(|_| "[]".to_string())
}

/// Append tool calls to existing content string.
///
/// Adds a newline separator if content is not empty, then appends the
/// formatted tool calls in a markdown code block.
///
/// # Arguments
/// * `content` - Mutable reference to the content string
/// * `tool_calls` - The tool calls to append
pub fn append_tool_calls_to_content(content: &mut String, tool_calls: &[OpenAiToolCall]) {
    if tool_calls.is_empty() {
        return;
    }

    let formatted = format_tool_calls_to_content(tool_calls);

    if !content.is_empty() {
        content.push('\n');
    }
    content.push_str(&format!("```tool\n{}\n```", formatted));
}

/// Extract tool calls from a response message if present.
///
/// # Arguments
/// * `tool_calls` - Optional tool calls from the response message
///
/// # Returns
/// The formatted content string with tool calls, or None if no tool calls.
pub fn extract_tool_call_content(tool_calls: Option<&[OpenAiToolCall]>) -> Option<String> {
    let tool_calls = tool_calls?;
    if tool_calls.is_empty() {
        return None;
    }

    Some(format_tool_calls_to_content(tool_calls))
}
