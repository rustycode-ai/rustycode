//! `ToolShim` — Extract tool calls from plain-text LLM responses.
//!
//! Enables tool use with models that don't natively support function calling
//! by extracting structured tool invocations from text using pattern matching.
//!
//! Inspired by goose's `providers/toolshim.rs`, but uses regex-based extraction
//! instead of a separate LLM call, making it zero-cost and instant.
//!
//! # Supported Formats
//!
//! The extractor handles multiple common formats that LLMs use to express
//! tool calls in text:
//!
//! - **XML-style**: `<tool_call name="Bash" arguments='{"command":"ls"}' />`
//! - **JSON blocks**: `{"name": "Bash", "arguments": {"command": "ls"}}`
//! - **Function-call style**: `bash(command="ls")`
//! - **Markdown code blocks**: ```json\n{"name": "Bash", ...}\n```
//!
//! # Example
//!
//! ```
//! use rustycode_tools::{ToolCallExtractor, ExtractedToolCall};
//!
//! let text = r#"I'll check the files for you.
//! {"name": "Bash", "arguments": {"command": "ls -la"}}"#;
//!
//! let calls = ToolCallExtractor::extract(text);
//! assert_eq!(calls.len(), 1);
//! assert_eq!(calls[0].name, "Bash");
//! assert_eq!(calls[0].arguments["command"], "ls -la");
//! ```

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An extracted tool call from LLM text output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedToolCall {
    /// Tool name (e.g., "Bash", "`read_file`")
    pub name: String,
    /// Tool arguments as a JSON object
    pub arguments: Value,
    /// The extraction method used
    pub source: ExtractionSource,
    /// Confidence score (0.0-1.0) based on format quality
    pub confidence: f32,
}

/// How the tool call was extracted from text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum ExtractionSource {
    /// JSON object with name/arguments fields
    JsonBlock,
    /// XML-style tag
    XmlTag,
    /// Function-call syntax: name(key="value")
    FunctionCall,
    /// JSON inside a markdown code block
    CodeBlock,
}

/// Configuration for tool call extraction.
#[derive(Debug, Clone)]
pub struct ExtractorConfig {
    /// Known tool names to validate against (empty = accept any)
    pub known_tools: Vec<String>,
    /// Maximum number of tool calls to extract from a single text
    pub max_calls: usize,
    /// Whether to validate tool names against `known_tools`
    pub validate_names: bool,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            known_tools: Vec::new(),
            max_calls: 10,
            validate_names: false,
        }
    }
}

impl ExtractorConfig {
    /// Create config with known tool names for validation.
    pub fn with_known_tools(tools: Vec<String>) -> Self {
        Self {
            validate_names: true,
            known_tools: tools,
            ..Default::default()
        }
    }
}

// ── Compiled Patterns ──────────────────────────────────────────────────────

/// Pattern for JSON tool call opener: finds the start `{"name": "tool", "arguments":` part.
/// The actual argument object is parsed with brace-counting (see `extract_balanced_json`)
/// since a regex cannot handle nested `{}`.
static JSON_TOOL_CALL_START_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"\{\s*"name"\s*:\s*"([^"]+)"\s*,\s*"arguments"\s*:\s*\{"#).unwrap()
});

/// Pattern for JSON tool call array: `[{"name": "...", "arguments": {...}}]`
#[allow(dead_code)] // Kept for future use
static JSON_ARRAY_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(?s)\[\s*\{[^]]*\]\s*").unwrap());

/// Pattern for XML-style tool call: `<tool_call name="..." arguments='...'/>`
/// Handles both single and double-quoted attributes with inner JSON
static XML_TOOL_CALL_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"<tool_call\s+name\s*=\s*["']([^"']+)["']\s+arguments\s*=\s*'(.*?)'\s*/?>"#)
        .unwrap()
});

/// Pattern for function-call syntax: `tool_name(arg="value", arg2="value2")`
static FUNC_CALL_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(\w+)\s*\(([^)]*)\)").unwrap());

/// Pattern for markdown code blocks containing JSON
static CODE_BLOCK_JSON_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"```(?:json)?\s*\n(\{.*?\})\s*\n```").unwrap());

/// Tool call extractor — the main API.
pub struct ToolCallExtractor;

impl ToolCallExtractor {
    /// Extract all tool calls from LLM text output.
    ///
    /// Tries multiple extraction patterns in priority order:
    /// 1. XML tags (highest confidence)
    /// 2. JSON objects with name/arguments
    /// 3. JSON in code blocks
    /// 4. Function-call syntax (lowest confidence)
    pub fn extract(text: &str) -> Vec<ExtractedToolCall> {
        Self::extract_with_config(text, &ExtractorConfig::default())
    }

    /// Extract tool calls with custom configuration.
    pub fn extract_with_config(text: &str, config: &ExtractorConfig) -> Vec<ExtractedToolCall> {
        let mut calls = Vec::new();

        // 1. Try XML-style tags first (highest confidence)
        Self::extract_xml_tags(text, config, &mut calls);

        // 2. Try JSON objects
        if calls.len() < config.max_calls {
            Self::extract_json_objects(text, config, &mut calls);
        }

        // 3. Try JSON in code blocks (only if no JSON match found — code blocks are a subset)
        if calls.len() < config.max_calls {
            Self::extract_code_blocks(text, config, &mut calls);
        }

        // 4. Try function-call syntax (lowest priority)
        if calls.len() < config.max_calls {
            Self::extract_function_calls(text, config, &mut calls);
        }

        // Deduplicate by (name, arguments) identity
        let mut seen = std::collections::HashSet::new();
        calls.retain(|c| {
            let key = format!("{}:{}", c.name, c.arguments);
            seen.insert(key)
        });

        // Truncate to max
        calls.truncate(config.max_calls);
        calls
    }

    /// Check if text likely contains a tool call without full extraction.
    pub fn contains_tool_call(text: &str) -> bool {
        XML_TOOL_CALL_RE.is_match(text)
            || JSON_TOOL_CALL_START_RE.is_match(text)
            || CODE_BLOCK_JSON_RE.is_match(text)
            || FUNC_CALL_RE.is_match(text)
    }

    /// Extract tool calls from XML-style tags.
    fn extract_xml_tags(text: &str, config: &ExtractorConfig, calls: &mut Vec<ExtractedToolCall>) {
        for cap in XML_TOOL_CALL_RE.captures_iter(text) {
            if calls.len() >= config.max_calls {
                break;
            }
            let name = cap[1].to_string();
            if config.validate_names && !Self::is_known_tool(&name, config) {
                continue;
            }
            if let Ok(args) = serde_json::from_str::<Value>(&cap[2]) {
                calls.push(ExtractedToolCall {
                    name,
                    arguments: args,
                    source: ExtractionSource::XmlTag,
                    confidence: 0.95,
                });
            }
        }
    }

    /// Extract tool calls from JSON objects.
    fn extract_json_objects(
        text: &str,
        config: &ExtractorConfig,
        calls: &mut Vec<ExtractedToolCall>,
    ) {
        for cap in JSON_TOOL_CALL_START_RE.captures_iter(text) {
            if calls.len() >= config.max_calls {
                break;
            }
            let name = cap[1].to_string();
            if config.validate_names && !Self::is_known_tool(&name, config) {
                continue;
            }
            // `cap[0]` is the full match including the opening `{` of arguments.
            // We need the arguments object starting at the `{` after `"arguments":`.
            let match_start = cap.get(0).unwrap().start();
            let full_outer_start = match text[match_start..].find('{') {
                Some(offset) => match_start + offset,
                None => continue,
            };
            // Find the end of the outermost `{...}` by brace-counting.
            let Some(full_json) = Self::extract_balanced_json(text, full_outer_start) else {
                continue;
            };
            // Parse the full object, then extract the "arguments" value.
            if let Ok(parsed) = serde_json::from_str::<Value>(&full_json) {
                if let Some(args) = parsed.get("arguments").cloned() {
                    calls.push(ExtractedToolCall {
                        name,
                        arguments: args,
                        source: ExtractionSource::JsonBlock,
                        confidence: 0.90,
                    });
                }
            }
        }
    }

    /// Extract a balanced `{...}` substring starting at `start` using brace counting.
    /// Handles nested braces and respects JSON string escaping.
    fn extract_balanced_json(text: &str, start: usize) -> Option<String> {
        let bytes = text.as_bytes();
        if start >= bytes.len() || bytes[start] != b'{' {
            return None;
        }
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        for i in start..bytes.len() {
            let ch = bytes[i];
            if escape {
                escape = false;
                continue;
            }
            if ch == b'\\' && in_string {
                escape = true;
                continue;
            }
            if ch == b'"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            match ch {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[start..=i].to_string());
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Extract tool calls from markdown code blocks.
    fn extract_code_blocks(
        text: &str,
        config: &ExtractorConfig,
        calls: &mut Vec<ExtractedToolCall>,
    ) {
        for cap in CODE_BLOCK_JSON_RE.captures_iter(text) {
            if calls.len() >= config.max_calls {
                break;
            }
            if let Ok(json) = serde_json::from_str::<Value>(&cap[1]) {
                if let Some(tool_call) = Self::parse_tool_call_from_json(&json, config) {
                    calls.push(ExtractedToolCall {
                        source: ExtractionSource::CodeBlock,
                        ..tool_call
                    });
                }
            }
        }
    }

    /// Extract tool calls from function-call syntax.
    fn extract_function_calls(
        text: &str,
        config: &ExtractorConfig,
        calls: &mut Vec<ExtractedToolCall>,
    ) {
        // Known tool names as function-call patterns
        let tool_names = if config.validate_names && !config.known_tools.is_empty() {
            config.known_tools.clone()
        } else {
            vec![
                "Bash".to_string(),
                "Read".to_string(),
                "Write".to_string(),
                "Edit".to_string(),
                "list_dir".to_string(),
                "Glob".to_string(),
                "Grep".to_string(),
                "git_status".to_string(),
                "git_diff".to_string(),
                "git_log".to_string(),
                "git_commit".to_string(),
                "WebFetch".to_string(),
                "web_search".to_string(),
                "run_tests".to_string(),
                "multi_edit".to_string(),
            ]
        };

        for tool_name in &tool_names {
            if calls.len() >= config.max_calls {
                break;
            }
            // Match: tool_name(key="value", key2="value2")
            let pattern = format!(r"(?i)\b{}\s*\(([^)]*)\)", regex::escape(tool_name));
            if let Ok(re) = Regex::new(&pattern) {
                for cap in re.captures_iter(text) {
                    if calls.len() >= config.max_calls {
                        break;
                    }
                    let args_str = &cap[1];
                    if let Some(args) = Self::parse_function_args(args_str) {
                        calls.push(ExtractedToolCall {
                            name: tool_name.clone(),
                            arguments: args,
                            source: ExtractionSource::FunctionCall,
                            confidence: 0.7,
                        });
                    }
                }
            }
        }
    }

    /// Parse a JSON value as a tool call (must have "name" field).
    fn parse_tool_call_from_json(
        json: &Value,
        config: &ExtractorConfig,
    ) -> Option<ExtractedToolCall> {
        let name = json.get("name")?.as_str()?.to_string();
        if config.validate_names && !Self::is_known_tool(&name, config) {
            return None;
        }
        let arguments = json
            .get("arguments")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));
        Some(ExtractedToolCall {
            name,
            arguments,
            source: ExtractionSource::JsonBlock,
            confidence: 0.85,
        })
    }

    /// Parse function-call arguments: `key="value", key2="value2"`
    fn parse_function_args(args_str: &str) -> Option<Value> {
        if args_str.trim().is_empty() {
            return Some(Value::Object(serde_json::Map::new()));
        }

        let mut map = serde_json::Map::new();
        for pair in Self::split_function_args(args_str) {
            let pair = pair.trim();
            if let Some(eq_pos) = pair.find('=') {
                let key = pair[..eq_pos].trim().to_string();
                let value = pair[eq_pos + 1..].trim();
                // Remove surrounding quotes
                let value = value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                    .unwrap_or(value)
                    .to_string();
                map.insert(key, Value::String(value));
            }
        }
        Some(Value::Object(map))
    }

    /// Split function-call arguments on commas that are not inside quotes/brackets.
    fn split_function_args(args_str: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0usize;
        let mut depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;

        for (idx, ch) in args_str.char_indices() {
            if escape {
                escape = false;
                continue;
            }

            match ch {
                '\\' if in_single || in_double => {
                    escape = true;
                }
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '(' | '[' | '{' if !in_single && !in_double => depth += 1,
                ')' | ']' | '}' if !in_single && !in_double && depth > 0 => depth -= 1,
                ',' if !in_single && !in_double && depth == 0 => {
                    parts.push(&args_str[start..idx]);
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }

        if start <= args_str.len() {
            parts.push(&args_str[start..]);
        }

        parts
    }

    /// Check if a tool name is in the known tools list.
    fn is_known_tool(name: &str, config: &ExtractorConfig) -> bool {
        config
            .known_tools
            .iter()
            .any(|t| t.eq_ignore_ascii_case(name))
    }
}

/// Sanitize a tool/function name to be API-compatible.
///
/// LLMs sometimes generate tool names with characters that are not valid
/// in function calling APIs (e.g., spaces, `@`, `.`). This replaces
/// any character that is not alphanumeric, underscore, or hyphen
/// with an underscore.
///
/// Inspired by goose's `sanitize_function_name` in `providers/utils.rs`.
///
/// # Example
///
/// ```
/// use rustycode_tools::sanitize_function_name;
///
/// assert_eq!(sanitize_function_name("Read"), "Read");
/// assert_eq!(sanitize_function_name("read file"), "read_file");
/// assert_eq!(sanitize_function_name("tool@v2"), "tool_v2");
/// assert_eq!(sanitize_function_name("my.tool"), "my_tool");
/// ```
pub fn sanitize_function_name(name: &str) -> String {
    use regex::Regex;

    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"[^a-zA-Z0-9_-]").unwrap());
    RE.replace_all(name, "_").to_string()
}

/// Check if a tool/function name is valid for API use.
///
/// A valid function name contains only alphanumeric characters,
/// underscores, and hyphens.
///
/// # Example
///
/// ```
/// use rustycode_tools::is_valid_function_name;
///
/// assert!(is_valid_function_name("Read"));
/// assert!(is_valid_function_name("Bash"));
/// assert!(!is_valid_function_name("read file"));
/// assert!(!is_valid_function_name("tool@v2"));
/// ```
pub fn is_valid_function_name(name: &str) -> bool {
    use regex::Regex;

    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());
    RE.is_match(name)
}

/// Augment a text response with extracted tool calls.
///
/// Given an LLM text response, extracts any tool calls and returns
/// a list of `ExtractedToolCall`. This is the main entry point for
/// the `ToolShim` pipeline.
///
/// # Example
///
/// ```
/// use rustycode_tools::extract_tool_calls;
///
/// let text = r#"Let me check the files:
/// {"name": "Bash", "arguments": {"command": "ls -la"}}"#;
///
/// let calls = extract_tool_calls(text);
/// assert_eq!(calls.len(), 1);
/// ```
pub fn extract_tool_calls(text: &str) -> Vec<ExtractedToolCall> {
    ToolCallExtractor::extract(text)
}

/// Extract tool calls with configuration.
pub fn extract_tool_calls_with_config(
    text: &str,
    config: &ExtractorConfig,
) -> Vec<ExtractedToolCall> {
    ToolCallExtractor::extract_with_config(text, config)
}

/// Format tool definitions for injection into system prompt.
///
/// Creates a text description of available tools that can be appended
/// to a system prompt when using models without native tool support.
pub fn format_tools_for_prompt(tools: &[(String, String, Value)]) -> String {
    let mut output = String::from("Available tools:\n\n");
    for (name, description, schema) in tools {
        output.push_str(&format!("### {name}\n"));
        output.push_str(&format!("{description}\n"));
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            output.push_str("Parameters:\n");
            for (key, prop) in properties {
                let type_str = prop.get("type").and_then(|t| t.as_str()).unwrap_or("any");
                let desc = prop
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                output.push_str(&format!("  - `{key}` ({type_str}): {desc}\n"));
            }
        }
        output.push('\n');
    }
    output.push_str(
        "To use a tool, include a JSON object in your response:\n\
         {\"name\": \"tool_name\", \"arguments\": {\"param\": \"value\"}}\n",
    );
    output
}

/// Convert extracted tool calls back to text format for conversation history.
///
/// Used when a conversation needs to be converted from tool-calling format
/// to plain text (e.g., for models without tool support).
pub fn tool_calls_to_text(calls: &[ExtractedToolCall]) -> String {
    let mut output = String::new();
    for call in calls {
        output.push_str(&format!(
            "Tool call: {}\nArguments: {}\n",
            call.name,
            serde_json::to_string_pretty(&call.arguments).unwrap_or_default()
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_json_tool_call() {
        let text = r#"I'll check the files.
{"name": "Bash", "arguments": {"command": "ls -la"}}"#;

        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Bash");
        assert_eq!(calls[0].arguments["command"], "ls -la");
        assert_eq!(calls[0].source, ExtractionSource::JsonBlock);
        assert!(calls[0].confidence > 0.8);
    }

    #[test]
    fn test_extract_xml_tool_call() {
        let text = r#"Let me read that file.
<tool_call name="Read" arguments='{"file_path": "/tmp/test.txt"}' />"#;

        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Read");
        assert_eq!(calls[0].arguments["file_path"], "/tmp/test.txt");
        assert_eq!(calls[0].source, ExtractionSource::XmlTag);
    }

    #[test]
    fn test_extract_function_call_syntax() {
        let text = r#"I'll list the directory:
bash(command="ls -la")"#;

        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Bash");
        assert_eq!(calls[0].arguments["command"], "ls -la");
        assert_eq!(calls[0].source, ExtractionSource::FunctionCall);
    }

    #[test]
    fn test_extract_code_block_json() {
        let text = r#"Here's the tool call:
```json
{"name": "Grep", "arguments": {"pattern": "TODO"}}
```"#;

        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Grep");
        assert_eq!(calls[0].arguments["pattern"], "TODO");
        // May be extracted as JsonBlock or CodeBlock depending on which regex matches first
        assert!(matches!(
            calls[0].source,
            ExtractionSource::CodeBlock | ExtractionSource::JsonBlock
        ));
    }

    #[test]
    fn test_extract_multiple_calls() {
        let text = r#"First, let me check the directory.
{"name": "Bash", "arguments": {"command": "ls"}}
Then read a file.
{"name": "Read", "arguments": {"file_path": "/tmp/test.txt"}}"#;

        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "Bash");
        assert_eq!(calls[1].name, "Read");
    }

    #[test]
    fn test_extract_with_known_tools_filter() {
        let text = r#"{"name": "Bash", "arguments": {"command": "ls"}}
{"name": "unknown_tool", "arguments": {}}"#;

        let config = ExtractorConfig::with_known_tools(vec!["Bash".to_string()]);
        let calls = ToolCallExtractor::extract_with_config(text, &config);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Bash");
    }

    #[test]
    fn test_extract_respects_max_calls() {
        let text = r#"
{"name": "Bash", "arguments": {"command": "ls"}}
{"name": "Bash", "arguments": {"command": "pwd"}}
{"name": "Bash", "arguments": {"command": "whoami"}}
{"name": "Bash", "arguments": {"command": "date"}}
"#;

        let config = ExtractorConfig {
            max_calls: 2,
            ..Default::default()
        };
        let calls = ToolCallExtractor::extract_with_config(text, &config);
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn test_extract_empty_text() {
        let calls = ToolCallExtractor::extract("");
        assert!(calls.is_empty());
    }

    #[test]
    fn test_extract_no_tool_calls() {
        let text = "This is just a regular response with no tool calls.";
        let calls = ToolCallExtractor::extract(text);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_extract_nested_json_arguments() {
        // Nested JSON in arguments — the old regex [^}]* couldn't handle this.
        let text = r#"{"name": "Edit", "arguments": {"old_string": "fn foo() {\n  bar()\n}", "new_string": "fn foo() {\n  baz()\n}"}}"#;

        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Edit");
        assert_eq!(calls[0].arguments["old_string"], "fn foo() {\n  bar()\n}");
        assert_eq!(calls[0].arguments["new_string"], "fn foo() {\n  baz()\n}");
    }

    #[test]
    fn test_extract_balanced_json_handles_escaping() {
        let text = r#"{"name": "Write", "arguments": {"content": "line1\n\"quoted\"\nline3"}}"#;
        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Write");
        assert_eq!(calls[0].arguments["content"], "line1\n\"quoted\"\nline3");
    }

    #[test]
    fn test_contains_tool_call() {
        assert!(ToolCallExtractor::contains_tool_call(
            r#"{"name": "Bash", "arguments": {"command": "ls"}}"#
        ));
        assert!(ToolCallExtractor::contains_tool_call(
            r#"<tool_call name="Bash" arguments='{}' />"#
        ));
        assert!(!ToolCallExtractor::contains_tool_call("Just text"));
    }

    #[test]
    fn test_parse_function_args() {
        let args = ToolCallExtractor::parse_function_args(r#"command="ls -la""#);
        assert_eq!(args, Some(json!({"command": "ls -la"})));

        let args = ToolCallExtractor::parse_function_args(r#"path="/tmp", recursive="true""#);
        assert_eq!(args, Some(json!({"path": "/tmp", "recursive": "true"})));

        let args = ToolCallExtractor::parse_function_args("");
        assert_eq!(args, Some(json!({})));
    }

    #[test]
    fn test_format_tools_for_prompt() {
        let tools = vec![(
            "Bash".to_string(),
            "Execute a bash command".to_string(),
            json!({
                "properties": {
                    "command": {"type": "string", "description": "Command to execute"}
                }
            }),
        )];

        let formatted = format_tools_for_prompt(&tools);
        assert!(formatted.contains("Bash"));
        assert!(formatted.contains("Execute a bash command"));
        assert!(formatted.contains("command"));
    }

    #[test]
    fn test_tool_calls_to_text() {
        let calls = vec![ExtractedToolCall {
            name: "Bash".to_string(),
            arguments: json!({"command": "ls"}),
            source: ExtractionSource::JsonBlock,
            confidence: 0.9,
        }];

        let text = tool_calls_to_text(&calls);
        assert!(text.contains("Bash"));
        assert!(text.contains("ls"));
    }

    #[test]
    fn test_xml_with_double_quotes() {
        let text = r#"<tool_call name="Bash" arguments='{"command": "ls"}' />"#;
        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Bash");
    }

    #[test]
    fn test_json_with_extra_whitespace() {
        let text = r#"{
            "name": "Read",
            "arguments": {
                "file_path": "/etc/hosts"
            }
        }"#;
        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Read");
    }

    #[test]
    fn test_function_call_with_multiple_args() {
        let text = r#"glob(pattern="**/*.rs", path="/tmp")"#;
        let calls = ToolCallExtractor::extract(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "Glob");
        assert_eq!(calls[0].arguments["pattern"], "**/*.rs");
    }

    #[test]
    fn test_extraction_source_confidence_ordering() {
        let xml_text = r#"<tool_call name="Bash" arguments='{}' />"#;
        let json_text = r#"{"name": "Bash", "arguments": {}}"#;
        let func_text = r#"bash()"#;

        let xml_calls = ToolCallExtractor::extract(xml_text);
        let json_calls = ToolCallExtractor::extract(json_text);
        let func_calls = ToolCallExtractor::extract(func_text);

        assert_eq!(xml_calls[0].source, ExtractionSource::XmlTag);
        assert!(xml_calls[0].confidence >= json_calls[0].confidence);
        assert!(json_calls[0].confidence >= func_calls[0].confidence);
    }

    #[test]
    fn test_sanitize_function_name() {
        assert_eq!(sanitize_function_name("Read"), "Read");
        assert_eq!(sanitize_function_name("read file"), "read_file");
        assert_eq!(sanitize_function_name("tool@v2"), "tool_v2");
        assert_eq!(sanitize_function_name("my.tool"), "my_tool");
        assert_eq!(sanitize_function_name("Bash"), "Bash");
        assert_eq!(
            sanitize_function_name("has/hyphens-and_underscores"),
            "has_hyphens-and_underscores"
        );
    }

    #[test]
    fn test_is_valid_function_name() {
        assert!(is_valid_function_name("Read"));
        assert!(is_valid_function_name("Bash"));
        assert!(is_valid_function_name("my-tool"));
        assert!(is_valid_function_name("Tool123"));
        assert!(!is_valid_function_name("read file"));
        assert!(!is_valid_function_name("tool@v2"));
        assert!(!is_valid_function_name("my.tool"));
        assert!(!is_valid_function_name(""));
    }

    #[test]
    fn test_parse_function_args_handles_commas_in_quotes() {
        let args = ToolCallExtractor::parse_function_args(
            r#"command="echo a,b", path="/tmp/x", note='hello, world'"#,
        )
        .unwrap();

        assert_eq!(args["command"], "echo a,b");
        assert_eq!(args["path"], "/tmp/x");
        assert_eq!(args["note"], "hello, world");
    }
}
