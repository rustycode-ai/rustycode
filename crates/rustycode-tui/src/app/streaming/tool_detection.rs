//! Tool use detection for LLM streaming
//!
//! This module handles stop-reason classification and tool-call detection
//! from LLM stream output.

use super::ToolUseAction;

/// Handle message metadata (stop reason, usage) and return appropriate action
///
/// Determines what action to take based on the LLM's stop reason.
/// This controls conversation continuation flow.
///
/// # Arguments
/// * `stop_reason` - The stop reason from the LLM response
///
/// # Returns
/// * `ToolUseAction::ExecuteTools` - Execute tools and continue conversation
/// * `ToolUseAction::Stop` - Conversation is complete
/// * `ToolUseAction::ContinueServerTools` - Server-side tools requested
/// * `ToolUseAction::None` - No specific action
pub fn handle_message_delta(stop_reason: Option<&str>) -> ToolUseAction {
    match stop_reason {
        Some(reason @ ("tool_use" | "tool_calls")) => {
            tracing::info!(
                "Stream stopped with reason: {} - will execute tools and continue",
                reason
            );
            ToolUseAction::ExecuteTools
        }
        Some("stop") => {
            tracing::info!("Stream stopped with reason: stop - conversation complete");
            ToolUseAction::Stop
        }
        Some("pause_turn") => {
            tracing::info!("Stream stopped with reason: pause_turn - server-side tools");
            ToolUseAction::ContinueServerTools
        }
        Some(other) => {
            tracing::debug!("Stream stopped with reason: {}", other);
            ToolUseAction::None
        }
        None => {
            tracing::debug!("Stream stopped without stop_reason");
            ToolUseAction::None
        }
    }
}

/// Check if text looks like a tool call (JSON patterns or XML-style tags)
///
/// This is a safety filter to prevent raw tool call JSON or XML from appearing
/// in the UI. It detects common patterns used by various LLM providers for
/// tool calls.
///
/// # Arguments
/// * `text` - The text to check
///
/// # Returns
/// * `true` if the text appears to be a tool call
/// * `false` otherwise
pub fn looks_like_tool_call(text: &str) -> bool {
    // Fast path: tool calls always contain structural markers ({, ", <).
    // Normal prose rarely has all three. Skip the expensive to_lowercase()
    // allocation for the ~95% of chunks that are just prose.
    let has_quote = text.contains('"');
    let has_angle = text.contains('<');
    let trimmed = text.trim();
    let starts_brace = trimmed.starts_with('{');
    if !starts_brace && !has_quote && !has_angle {
        return false;
    }

    // Single allocation for case-insensitive matching
    let text_lower = text.to_lowercase();
    let trimmed_lower = trimmed.to_lowercase();

    // Structural JSON patterns — must start with { or contain clear tool-use markers
    trimmed_lower.starts_with('{')
        || trimmed_lower.starts_with("\"function\":")
        || trimmed_lower.starts_with("\"tool\":")
        || text_lower.contains("\"tool_use\"")
        || text_lower.contains("\"tooluse\"")
        // XML-style tool calls — unambiguous
        || text_lower.contains("<read_file>")
        || text_lower.contains("<bash>")
        || text_lower.contains("<write_file>")
        || text_lower.contains("<grep>")
        || text_lower.contains("<edit>")
        // GLM/Zhipu XML tool calls: <tool_call xmlns="..." ...>
        || text_lower.contains("<tool_call")
        // Generic tool_response tags (prevent echo of results)
        || text_lower.contains("<tool_response")
        // Anthropic-style: starts with { AND contains "name"
        || (trimmed_lower.starts_with('{') && text_lower.contains("\"name\""))
        // OpenAI-style: contains BOTH "function" AND "name" together
        || (text_lower.contains("\"function\"") && text_lower.contains("\"name\""))
        // JSON with tool parameters — require "parameters"/"argument"/"input" to
        // appear alongside a structural JSON indicator ("name" or "function")
        // to avoid false positives on normal text like: Set the "input" field...
        || (text_lower.contains("\"parameters\"") && text_lower.contains("\"name\""))
        || (text_lower.contains("\"argument\"") && text_lower.contains("\"name\""))
        || (text_lower.contains("\"input\"") && text_lower.contains("\"name\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_tool_call() {
        assert!(looks_like_tool_call("{"));
        assert!(looks_like_tool_call("{\"name\": \"bash\""));
        assert!(looks_like_tool_call("\"tool_use\":"));
        assert!(looks_like_tool_call("\"function\":"));
        assert!(looks_like_tool_call("\"function\": {\"name\":"));
        // "parameters"/"argument"/"input" alone should NOT trigger — need "name" too
        assert!(!looks_like_tool_call("\"parameters\":"));
        assert!(!looks_like_tool_call("\"argument\":"));
        assert!(!looks_like_tool_call("\"input\":"));
        // But with "name" they should trigger
        assert!(looks_like_tool_call(
            "{\"name\": \"bash\", \"parameters\": {}}"
        ));
        assert!(looks_like_tool_call(
            "{\"name\": \"bash\", \"argument\": {}}"
        ));
        assert!(looks_like_tool_call("{\"name\": \"bash\", \"input\": {}}"));
        assert!(!looks_like_tool_call("Hello, world!"));
        assert!(!looks_like_tool_call("This is text"));
        assert!(!looks_like_tool_call("```code```"));
        assert!(!looks_like_tool_call(
            "Set the \"input\" field to the value"
        ));
        assert!(!looks_like_tool_call("The \"parameters\" are as follows:"));
        // GLM/Zhipu XML tool call format
        assert!(looks_like_tool_call(
            "<tool_call xmlns=\"http://example.com/tool_call\">"
        ));
        assert!(looks_like_tool_call(
            "<tool_call name=\"bash\" arguments='{}' />"
        ));
        assert!(looks_like_tool_call("<TOOL_CALL xmlns=\"...\">"));
        assert!(looks_like_tool_call(
            "<tool_response request_id=\"t1\">ok</tool_response>"
        ));
    }

    #[test]
    fn test_looks_like_tool_call_case_insensitive() {
        assert!(looks_like_tool_call("\"TOOL_USE\":"));
        assert!(looks_like_tool_call("\"Function\":"));
        // "PARAMETERS" alone should not trigger without "name"
        assert!(!looks_like_tool_call("\"PARAMETERS\":"));
        // With "name" it should trigger
        assert!(looks_like_tool_call("\"NAME\": \"x\", \"PARAMETERS\": {}"));
    }

    #[test]
    fn test_handle_message_delta_tool_use() {
        assert!(matches!(
            handle_message_delta(Some("tool_use")),
            ToolUseAction::ExecuteTools
        ));
        assert!(matches!(
            handle_message_delta(Some("tool_calls")),
            ToolUseAction::ExecuteTools
        ));
    }

    #[test]
    fn test_handle_message_delta_stop() {
        assert!(matches!(
            handle_message_delta(Some("stop")),
            ToolUseAction::Stop
        ));
    }

    #[test]
    fn test_handle_message_delta_pause_turn() {
        assert!(matches!(
            handle_message_delta(Some("pause_turn")),
            ToolUseAction::ContinueServerTools
        ));
    }

    #[test]
    fn test_handle_message_delta_unknown_reason() {
        assert!(matches!(
            handle_message_delta(Some("end_turn")),
            ToolUseAction::None
        ));
        assert!(matches!(
            handle_message_delta(Some("max_tokens")),
            ToolUseAction::None
        ));
    }

    #[test]
    fn test_handle_message_delta_none() {
        assert!(matches!(handle_message_delta(None), ToolUseAction::None));
    }
}
