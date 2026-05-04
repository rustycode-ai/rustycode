//! Tool execution integration for LLM providers
//!
//! This module bridges LLM provider tool calling with the rustycode-tools execution system.
//! It handles:
//! - Converting tool calls from LLM responses to ToolCall format
//! - Executing tools via ToolExecutor
//! - Converting tool results back to LLM message format
//! - Error handling and retries

use crate::provider::{ChatMessage, MessageRole};
use crate::tool_annotations::anthropic_annotations_for_tool_info;
use anyhow::Result;
use rustycode_protocol::ToolCall;
use rustycode_tool_integration::ToolExecutorApi;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Parsed tool call from LLM response
#[derive(Debug, Clone)]
pub struct ParsedToolCall {
    pub name: String,
    pub arguments: Value,
    pub id: Option<String>,
}

/// Result of tool execution
#[derive(Debug)]
pub struct ToolExecutionResult {
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Tool execution manager for LLM providers
pub struct LLMToolExecutor {
    executor: Arc<dyn ToolExecutorApi>,
}

impl LLMToolExecutor {
    /// Create a new tool executor with a custom backend.
    pub fn with_executor<E>(executor: E) -> Self
    where
        E: ToolExecutorApi + 'static,
    {
        Self {
            executor: Arc::new(executor),
        }
    }

    /// Get the underlying tool executor
    pub fn executor(&self) -> &dyn ToolExecutorApi {
        self.executor.as_ref()
    }

    /// Parse tool calls from Anthropic response content
    ///
    /// Anthropic returns tool_use blocks in content array:
    /// {"type": "tool_use", "id": "...", "name": "bash", "input": {...}}
    pub fn parse_anthropic_tool_calls(&self, content: &str) -> Result<Vec<ParsedToolCall>> {
        let mut tool_calls = Vec::new();

        // Try to parse as JSON array first (structured content)
        if let Ok(json_value) = serde_json::from_str::<Value>(content) {
            if let Some(blocks) = json_value.as_array() {
                for block in blocks {
                    if let Some(content_type) = block.get("type").and_then(|t| t.as_str()) {
                        if content_type == "tool_use" {
                            let name = block
                                .get("name")
                                .and_then(|n| n.as_str())
                                .ok_or_else(|| anyhow::anyhow!("tool_use missing 'name'"))?
                                .to_string();

                            let arguments = block
                                .get("input")
                                .cloned()
                                .unwrap_or_else(|| Value::Object(Default::default()));

                            let id = block
                                .get("id")
                                .and_then(|i| i.as_str())
                                .map(|s| s.to_string());

                            tool_calls.push(ParsedToolCall {
                                name,
                                arguments,
                                id,
                            });
                        }
                    }
                }
            }
        }

        // Extract from ```tool ... ``` code blocks (multi-line)
        // Handles format from openai.rs: ```tool\n[{...}]\n```
        if let Some(extracted) = extract_tool_code_block(content) {
            if let Ok(json_value) = serde_json::from_str::<Value>(&extracted) {
                if let Some(array) = json_value.as_array() {
                    for item in array {
                        if let Some(parsed) = parse_tool_call_item(item) {
                            tool_calls.push(parsed);
                        }
                    }
                } else if let Some(parsed) = parse_tool_call_item(&json_value) {
                    tool_calls.push(parsed);
                }
            }
        }

        // Fallback: single-line tool: prefix
        for line in content.lines() {
            if let Some(json_str) = line.strip_prefix("tool:") {
                let json_str = json_str.trim();
                if let Ok(json_value) = serde_json::from_str::<Value>(json_str) {
                    if let Some(parsed) = parse_tool_call_item(&json_value) {
                        tool_calls.push(parsed);
                    }
                }
            }
        }

        Ok(tool_calls)
    }

    /// Parse tool calls from OpenAI function calling response.
    ///
    /// Handles two formats:
    /// 1. Pure JSON: `{"tool_calls": [{"id": "...", "function": {"name": "bash", "arguments": "{...}"}}]}`
    /// 2. Markdown-wrapped: ```` ```tool\n[{"name": "...", "arguments": {...}}]\n``` ````
    ///
    /// Some providers (e.g. GLM-5.1 via OpenAI-compatible API) embed tool calls inside
    /// ```tool fenced code blocks mixed with regular text, so we extract and parse those
    /// as a fallback when pure-JSON parsing yields nothing.
    pub fn parse_openai_tool_calls(&self, content: &str) -> Result<Vec<ParsedToolCall>> {
        let mut tool_calls = Vec::new();

        // Strategy 1: Try pure JSON first (standard OpenAI format)
        if let Ok(json_value) = serde_json::from_str::<Value>(content) {
            if let Some(tool_calls_array) = json_value.get("tool_calls").and_then(|t| t.as_array())
            {
                for tool_call in tool_calls_array {
                    let id = tool_call
                        .get("id")
                        .and_then(|i| i.as_str())
                        .map(|s| s.to_string());

                    if let Some(function) = tool_call.get("function") {
                        let name = function
                            .get("name")
                            .and_then(|n| n.as_str())
                            .ok_or_else(|| anyhow::anyhow!("function call missing 'name'"))?
                            .to_string();

                        let arguments_str = function
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");

                        let arguments = serde_json::from_str::<Value>(arguments_str)
                            .unwrap_or_else(|_| Value::Object(Default::default()));

                        tool_calls.push(ParsedToolCall {
                            name,
                            arguments,
                            id,
                        });
                    }
                }
                return Ok(tool_calls);
            }
        }

        // Strategy 2: Extract from ```tool code blocks (GLM-5.1 / Anthropic-style)
        if let Some(json_str) = extract_tool_code_block(content) {
            // The extracted block may be a JSON array of tool calls
            if let Ok(items) = serde_json::from_str::<Vec<Value>>(&json_str) {
                for item in &items {
                    if let Some(tc) = parse_tool_call_item(item) {
                        tool_calls.push(tc);
                    }
                }
                return Ok(tool_calls);
            }
            // Single object (not wrapped in array)
            if let Ok(obj) = serde_json::from_str::<Value>(&json_str) {
                if let Some(tc) = parse_tool_call_item(&obj) {
                    tool_calls.push(tc);
                    return Ok(tool_calls);
                }
            }
        }

        Ok(tool_calls)
    }

    /// Execute a parsed tool call
    #[instrument(skip(self, tool_call), fields(tool_name = %tool_call.name))]
    pub async fn execute_tool_call(
        &self,
        tool_call: &ParsedToolCall,
    ) -> Result<ToolExecutionResult> {
        debug!("Executing tool: {}", tool_call.name);

        let call_id = tool_call
            .id
            .clone()
            .unwrap_or_else(|| format!("tool-{}", uuid::Uuid::new_v4()));

        let tool_call = ToolCall {
            call_id,
            name: tool_call.name.clone(),
            arguments: tool_call.arguments.clone(),
        };

        let result = self.executor.execute(&tool_call);

        Ok(ToolExecutionResult {
            tool_name: tool_call.name.clone(),
            success: result.success,
            output: result.output,
            error: result.error,
        })
    }

    /// Execute multiple tool calls concurrently
    pub async fn execute_tool_calls(
        &self,
        tool_calls: &[ParsedToolCall],
    ) -> Result<Vec<ToolExecutionResult>> {
        let mut results = Vec::new();

        for tool_call in tool_calls {
            let result = self.execute_tool_call(tool_call).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Convert tool execution result to ChatMessage for Anthropic
    pub fn result_to_anthropic_message(
        &self,
        result: &ToolExecutionResult,
        tool_use_id: Option<String>,
    ) -> ChatMessage {
        let content = if let Some(id) = tool_use_id {
            // Anthropic tool result format
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": result.output
            })
            .to_string()
        } else {
            // Fallback: just output text
            result.output.clone()
        };

        ChatMessage {
            role: MessageRole::User, // Tool results sent as user role in Anthropic
            content: rustycode_protocol::MessageContent::simple(content),
        }
    }

    /// Convert tool execution result to ChatMessage for OpenAI
    pub fn result_to_openai_message(
        &self,
        result: &ToolExecutionResult,
        tool_call_id: Option<String>,
    ) -> ChatMessage {
        let content = if let Some(id) = tool_call_id {
            // OpenAI tool message format - use structured content
            let tool_result = serde_json::json!({
                "tool_call_id": id,
                "content": result.output
            });
            tool_result.to_string()
        } else {
            // Fallback: just output text
            result.output.clone()
        };

        ChatMessage {
            role: MessageRole::Tool(result.tool_name.clone()),
            content: rustycode_protocol::MessageContent::simple(content),
        }
    }

    /// Execute tool calls and convert results to messages (Anthropic format)
    pub async fn execute_and_format_anthropic(
        &self,
        tool_calls: &[ParsedToolCall],
    ) -> Result<Vec<ChatMessage>> {
        let results = self.execute_tool_calls(tool_calls).await?;
        let messages = tool_calls
            .iter()
            .zip(results.iter())
            .map(|(call, result)| self.result_to_anthropic_message(result, call.id.clone()))
            .collect();

        Ok(messages)
    }

    /// Execute tool calls and convert results to messages (OpenAI format)
    pub async fn execute_and_format_openai(
        &self,
        tool_calls: &[ParsedToolCall],
    ) -> Result<Vec<ChatMessage>> {
        let results = self.execute_tool_calls(tool_calls).await?;
        let messages = tool_calls
            .iter()
            .zip(results.iter())
            .map(|(call, result)| self.result_to_openai_message(result, call.id.clone()))
            .collect();

        Ok(messages)
    }

    /// Get tool definitions for Anthropic format.
    /// Deferred tools emit stubs using the canonical schema from `formatters`.
    pub fn get_anthropic_tool_definitions(&self) -> Vec<Value> {
        let stub_schema = crate::tool_selection_helper::formatters::deferred_stub_schema();
        self.executor
            .list()
            .into_iter()
            .map(|tool| {
                if tool.defer_loading == Some(true) {
                    let desc = tool
                        .description
                        .lines()
                        .next()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("(no description)");
                    let stub_desc = format!(
                        "{} [DEFERRED: call tool_search with name=\"{}\" to load full schema]",
                        desc, tool.name
                    );
                    serde_json::json!({
                        "name": tool.name,
                        "description": stub_desc,
                        "input_schema": stub_schema
                    })
                } else {
                    let mut tool_json = serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.parameters_schema
                    });
                    if let Some(annotations) = anthropic_annotations_for_tool_info(
                        &tool.name,
                        matches!(tool.permission, rustycode_protocol::ToolPermission::Read),
                    ) {
                        tool_json["annotations"] = annotations;
                    }
                    tool_json
                }
            })
            .collect()
    }

    /// Get tool definitions for OpenAI format.
    /// Deferred tools emit stubs using the canonical schema from `formatters`.
    pub fn get_openai_tool_definitions(&self) -> Vec<Value> {
        let stub_schema = crate::tool_selection_helper::formatters::deferred_stub_schema();
        self.executor
            .list()
            .into_iter()
            .map(|tool| {
                if tool.defer_loading == Some(true) {
                    let desc = tool
                        .description
                        .lines()
                        .next()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("(no description)");
                    let stub_desc = format!(
                        "{} [DEFERRED: call tool_search with name=\"{}\" to load full schema]",
                        desc, tool.name
                    );
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": stub_desc,
                            "parameters": stub_schema
                        }
                    })
                } else {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters_schema
                        }
                    })
                }
            })
            .collect()
    }
}

/// Extract the JSON content from a ```tool ... ``` fenced code block.
fn extract_tool_code_block(content: &str) -> Option<String> {
    let start_marker = "```tool";
    let end_marker = "```";

    let start_idx = content.find(start_marker)?;
    let after_start = start_idx + start_marker.len();

    let json_start =
        if content[after_start..].starts_with('\n') || content[after_start..].starts_with(' ') {
            after_start + 1
        } else {
            after_start
        };

    let remaining = &content[json_start..];
    let end_idx = remaining.find(end_marker)?;
    let json_str = remaining[..end_idx].trim();

    if json_str.is_empty() {
        return None;
    }

    Some(json_str.to_string())
}

/// Parse a single tool call item that may be in OpenAI format or Anthropic format.
///
/// OpenAI: {"id": "...", "type": "function", "function": {"name": "...", "arguments": "{...}"}}
/// Anthropic/flat: {"name": "...", "arguments": {...}}
fn parse_tool_call_item(item: &Value) -> Option<ParsedToolCall> {
    // OpenAI format: nested under "function"
    if let Some(func) = item.get("function") {
        let name = func.get("name")?.as_str()?.to_string();
        let arguments_str = func
            .get("arguments")
            .and_then(|a| a.as_str())
            .unwrap_or("{}");
        let arguments = serde_json::from_str::<Value>(arguments_str)
            .unwrap_or_else(|_| Value::Object(Default::default()));
        let id = item
            .get("id")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string());
        return Some(ParsedToolCall {
            name,
            arguments,
            id,
        });
    }

    // Anthropic/flat format: {"name": "...", "arguments": {...}}
    // Also handles: {"name": "...", "arguments": "{...}"} (string-encoded JSON)
    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
        let raw_arguments = item
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        // If arguments is a string, try to parse it as JSON
        let arguments = match &raw_arguments {
            Value::String(s) => serde_json::from_str::<Value>(s).unwrap_or(raw_arguments),
            _ => raw_arguments,
        };

        let id = item
            .get("id")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string());
        return Some(ParsedToolCall {
            name: name.to_string(),
            arguments,
            id,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::{ToolPermission, ToolResult};
    use rustycode_tool_integration::ToolInfo;

    struct FakeToolExecutor;

    impl ToolExecutorApi for FakeToolExecutor {
        fn list(&self) -> Vec<ToolInfo> {
            vec![
                ToolInfo {
                    name: "bash".to_string(),
                    description: "Execute a shell command".to_string(),
                    parameters_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "The command"
                            }
                        }
                    }),
                    permission: ToolPermission::Execute,
                    defer_loading: None,
                },
                ToolInfo {
                    name: "list_dir".to_string(),
                    description: "List directory contents".to_string(),
                    parameters_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "Directory path"
                            }
                        }
                    }),
                    permission: ToolPermission::Read,
                    defer_loading: None,
                },
                ToolInfo {
                    name: "read_file".to_string(),
                    description: "Read a file".to_string(),
                    parameters_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {
                                "type": "string",
                                "description": "File path"
                            }
                        }
                    }),
                    permission: ToolPermission::Read,
                    defer_loading: None,
                },
            ]
        }

        fn execute(&self, call: &ToolCall) -> ToolResult {
            match call.name.as_str() {
                "list_dir" => ToolResult::success(
                    call.call_id.clone(),
                    format!(
                        "Fake directory listing for {}",
                        call.arguments["path"].as_str().unwrap_or(".")
                    ),
                ),
                "read_file" => {
                    if call.arguments["path"].as_str() == Some("/nonexistent/file.txt") {
                        ToolResult::error(call.call_id.clone(), "File not found")
                    } else {
                        ToolResult::success(call.call_id.clone(), "Fake file contents")
                    }
                }
                "bash" => ToolResult::success(call.call_id.clone(), "Fake bash output"),
                other => ToolResult::error(call.call_id.clone(), format!("unknown tool '{other}'")),
            }
        }
    }

    fn create_executor() -> LLMToolExecutor {
        LLMToolExecutor::with_executor(FakeToolExecutor)
    }

    #[test]
    fn test_parse_anthropic_tool_calls() {
        let executor = create_executor();

        let content = r#"[
            {"type": "text", "text": "I'll help you with that."},
            {"type": "tool_use", "id": "toolu_123", "name": "bash", "input": {"command": "ls"}}
        ]"#;

        let tool_calls = executor.parse_anthropic_tool_calls(content).unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "bash");
        assert_eq!(tool_calls[0].id.as_ref().unwrap(), "toolu_123");
    }

    #[test]
    fn test_parse_openai_tool_calls() {
        let executor = create_executor();

        let content = r#"{
            "tool_calls": [
                {
                    "id": "call_123",
                    "function": {
                        "name": "bash",
                        "arguments": "{\"command\": \"ls\"}"
                    }
                }
            ]
        }"#;

        let tool_calls = executor.parse_openai_tool_calls(content).unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "bash");
        assert_eq!(tool_calls[0].id.as_ref().unwrap(), "call_123");
    }

    #[test]
    fn test_get_anthropic_tool_definitions() {
        let executor = create_executor();
        let tools = executor.get_anthropic_tool_definitions();

        assert!(!tools.is_empty());
        let tool = &tools[0];
        assert!(tool.get("name").is_some());
        assert!(tool.get("description").is_some());
        assert!(tool.get("input_schema").is_some());
    }

    #[test]
    fn test_get_openai_tool_definitions() {
        let executor = create_executor();
        let tools = executor.get_openai_tool_definitions();

        assert!(!tools.is_empty());
        let tool = &tools[0];
        assert_eq!(tool.get("type").unwrap().as_str().unwrap(), "function");
        assert!(tool.get("function").is_some());
    }

    // Regression: openai.rs produces ```tool\n[{...}]\n``` format
    #[test]
    fn test_parse_openai_format_in_tool_code_block() {
        let executor = create_executor();

        let content = "```tool\n[\n  {\n    \"id\": \"call_-7703117166425406227\",\n    \"type\": \"function\",\n    \"function\": {\n      \"name\": \"bash\",\n      \"arguments\": \"{\\\"command\\\": \\\"ls\\\"}\"\n    }\n  }\n]\n```";

        let tool_calls = executor.parse_anthropic_tool_calls(content).unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "bash");
        assert_eq!(
            tool_calls[0].id.as_ref().unwrap(),
            "call_-7703117166425406227"
        );
        assert_eq!(tool_calls[0].arguments["command"], "ls");
    }

    #[test]
    fn test_extract_tool_code_block_multiline() {
        let content = "some text\n```tool\n[{\"id\": \"call_1\", \"type\": \"function\", \"function\": {\"name\": \"bash\", \"arguments\": \"{}\"}}]\n```\nmore text";
        let extracted = extract_tool_code_block(content).unwrap();
        assert!(extracted.contains("\"bash\""));
    }

    #[test]
    fn test_extract_tool_code_block_none() {
        let content = "just regular text, no tool blocks here";
        assert!(extract_tool_code_block(content).is_none());
    }

    #[test]
    fn test_parse_tool_call_item_openai_format() {
        let item = serde_json::json!({
            "id": "call_xyz",
            "type": "function",
            "function": {
                "name": "read_file",
                "arguments": "{\"path\": \"main.rs\"}"
            }
        });
        let parsed = parse_tool_call_item(&item).unwrap();
        assert_eq!(parsed.name, "read_file");
        assert_eq!(parsed.id.as_ref().unwrap(), "call_xyz");
        assert_eq!(parsed.arguments["path"], "main.rs");
    }

    #[test]
    fn test_parse_tool_call_item_flat_format() {
        let item = serde_json::json!({
            "name": "bash",
            "arguments": {"command": "ls"}
        });
        let parsed = parse_tool_call_item(&item).unwrap();
        assert_eq!(parsed.name, "bash");
        assert_eq!(parsed.arguments["command"], "ls");
    }

    #[test]
    fn test_parse_tool_call_item_string_arguments() {
        // Some providers return arguments as a JSON string instead of parsed object
        let item = serde_json::json!({
            "name": "bash",
            "arguments": "{\"command\": \"ls -la\"}"
        });
        let parsed = parse_tool_call_item(&item).unwrap();
        assert_eq!(parsed.name, "bash");
        assert_eq!(parsed.arguments["command"], "ls -la");
    }

    #[test]
    fn test_parse_tool_call_item_invalid_string_arguments_falls_back() {
        // If string arguments aren't valid JSON, use the raw string value
        let item = serde_json::json!({
            "name": "bash",
            "arguments": "not valid json"
        });
        let parsed = parse_tool_call_item(&item).unwrap();
        assert_eq!(parsed.name, "bash");
        // Should fall back to the raw string value
        assert!(parsed.arguments.is_string());
    }

    #[test]
    fn test_parse_tool_call_item_no_arguments() {
        let item = serde_json::json!({
            "name": "list_tools"
        });
        let parsed = parse_tool_call_item(&item).unwrap();
        assert_eq!(parsed.name, "list_tools");
        assert!(parsed.arguments.is_object());
        assert!(parsed.arguments.as_object().unwrap().is_empty());
    }
}
