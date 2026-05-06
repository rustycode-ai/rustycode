//! Streaming LLM response handler with SSE parsing, tool detection, and execution.

pub mod adapter;
pub mod events;
pub mod response;
pub mod tool_detection;
pub mod tool_execution;

pub use response::stream_llm_response;
pub use response::StreamConfig;
pub use tool_execution::execute_tool_with_hooks;
pub use tool_execution::snapshot_files_for_undo;

/// Re-export ToolAccumulator from streaming module as ActiveToolUse for compatibility
pub use rustycode_core::streaming::ToolAccumulator as ActiveToolUse;

/// Result of executing a tool
///
/// Stores the outcome of a tool execution for later use in conversation continuation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolExecutionResult {
    /// The unique ID matching the original tool use request
    pub tool_use_id: String,
    /// Name of the tool that was executed
    pub tool_name: String,
    /// The output/result content from the tool execution
    pub result_content: String,
}

/// Action to take after processing message delta
///
/// Determined by the `stop_reason` field in the LLM response, this enum
/// controls whether to continue the conversation, execute tools, or stop.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ToolUseAction {
    /// LLM wants to use tools - execute them and continue the conversation
    ExecuteTools,
    /// LLM is done - no more tool use needed
    Stop,
    /// Continue with server-side tools (pause_turn stop reason)
    ContinueServerTools,
    /// No specific action required
    None,
}

/// Helper function to parse tool parameters with JSON repair fallback.
///
/// LLMs often produce malformed JSON in tool arguments (missing closing braces,
/// trailing commas, Python-style booleans, etc.). This function attempts to parse
/// the JSON directly first, and if that fails, applies repair strategies before
/// trying again.
pub fn parse_tool_parameters(parameters_json: &str) -> serde_json::Value {
    serde_json::from_str(parameters_json)
        .or_else(|_| rustycode_tools::json_repair::parse_or_repair(parameters_json))
        .unwrap_or_else(|e| {
            tracing::error!(
                error = %e,
                parameters = %parameters_json,
                "Failed to parse tool parameters even after repair attempt"
            );
            serde_json::json!({})
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_tool_use_creation() {
        let tool = ActiveToolUse::new(
            "toolu_123".to_string(),
            "read_file".to_string(),
            String::new(),
        );

        assert_eq!(tool.id, "toolu_123");
        assert_eq!(tool.name, "read_file");
        assert!(tool.partial_json.is_empty());
    }

    #[test]
    fn test_active_tool_use_accumulation() {
        let mut tool = ActiveToolUse::new(
            "toolu_123".to_string(),
            "read_file".to_string(),
            String::new(),
        );

        tool.partial_json.push_str("{\"path\"");
        tool.partial_json.push_str(": \"");
        tool.partial_json.push_str("/tmp/test");
        tool.partial_json.push_str("\"}");

        assert_eq!(tool.partial_json, "{\"path\": \"/tmp/test\"}");
    }

    #[test]
    fn test_tool_execution_result_creation() {
        let result = ToolExecutionResult {
            tool_use_id: "toolu_123".to_string(),
            tool_name: "read_file".to_string(),
            result_content: "File contents here".to_string(),
        };

        assert_eq!(result.tool_use_id, "toolu_123");
        assert_eq!(result.tool_name, "read_file");
        assert_eq!(result.result_content, "File contents here");
    }

    #[test]
    fn test_tool_use_action_from_stop_reason() {
        use super::tool_detection::handle_message_delta;

        let action = handle_message_delta(Some("tool_use"));
        assert_eq!(action, ToolUseAction::ExecuteTools);

        let action = handle_message_delta(Some("stop"));
        assert_eq!(action, ToolUseAction::Stop);

        let action = handle_message_delta(Some("pause_turn"));
        assert_eq!(action, ToolUseAction::ContinueServerTools);

        let action = handle_message_delta(Some("unknown"));
        assert_eq!(action, ToolUseAction::None);

        let action = handle_message_delta(None);
        assert_eq!(action, ToolUseAction::None);
    }
}
