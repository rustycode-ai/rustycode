//! JSON Repair Utility
//!
//! Fixes common JSON malformations from LLM tool call responses.
//!
//! LLMs often produce malformed JSON when generating tool arguments:
//! - Missing closing braces/brackets
//! - Extra trailing commas
//! - Unquoted object keys
//! - Single quotes instead of double quotes
//! - Python-style booleans (True/False) and None
//! - Markdown code fences around JSON
//! - JSON embedded in explanatory text
//! - Raw control characters (newlines, tabs) inside string values
//! - Double-escaped strings (`\\n` instead of `\n`)
//!
//! This module provides a cascading repair pipeline that attempts to fix
//! these issues before falling back to deserialization errors.
//!
//! Inspired by goose's `safely_parse_json` and `unescape_json_values`.
//!
//! # Example
//!
//! ```rust
//! use rustycode_tools::json_repair;
//!
//! // Repair trailing comma
//! let repaired = json_repair::repair_json(r#"{"key": "value",}"#);
//! assert_eq!(repaired, r#"{"key": "value"}"#);
//!
//! // Parse with automatic repair
//! use serde_json::Value;
//! let result: Value = json_repair::parse_or_repair(r#"{"flag": True,}"#).unwrap();
//! assert_eq!(result["flag"], true);
//! ```

use serde_json::Value;
use std::sync::LazyLock;

static TRAILING_COMMA_BRACE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r",\s*\}").unwrap());
static TRAILING_COMMA_BRACKET: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r",\s*\]").unwrap());
static UNQUOTED_KEY: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([\{,])\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*:").unwrap());

/// Repair potentially malformed JSON from LLM output.
///
/// This function applies a series of repair strategies in order:
/// 1. Remove markdown code fences (\`\`\`json ... \`\`\`)
/// 2. Extract JSON from surrounding text
/// 3. Fix Python-style booleans and None
/// 4. Fix single-quoted strings
/// 5. Fix unquoted object keys
/// 6. Fix trailing commas
/// 7. Fix unclosed brackets/braces
///
/// Returns the repaired JSON string, or the original if no repair was needed.
///
/// # Example
///
/// ```rust
/// use rustycode_tools::json_repair;
///
/// let input = r#"```json
/// {"name": "test",}
/// ```"#;
/// let repaired = json_repair::repair_json(input);
/// assert_eq!(repaired, r#"{"name": "test"}"#);
/// ```
pub fn repair_json(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "{}".to_string();
    }

    let mut result = trimmed.to_string();

    // Apply repairs in order - each transformation receives the output of the previous
    result = remove_json_markdown_fence(&result);
    result = extract_json_from_text(&result);
    result = fix_boolean_null(&result);
    result = fix_single_quotes(&result);
    result = fix_unquoted_keys(&result);
    result = fix_trailing_commas(&result);
    result = fix_unclosed_brackets(&result);

    result
}

/// Try to parse JSON, repairing if needed.
///
/// First attempts to parse the input directly. If that fails, applies
/// repair strategies and tries again. Returns a deserialized value
/// or the original deserialization error.
///
/// # Example
///
/// ```rust
/// use rustycode_tools::json_repair;
/// use serde_json::Value;
///
/// // Valid JSON parses directly
/// let result: Value = json_repair::parse_or_repair(r#"{"a": 1}"#).unwrap();
/// assert_eq!(result["a"], 1);
///
/// // Malformed JSON is repaired and parsed
/// let result: Value = json_repair::parse_or_repair(r#"{"a": 1,}"#).unwrap();
/// assert_eq!(result["a"], 1);
/// ```
pub fn parse_or_repair<T: serde::de::DeserializeOwned>(
    input: &str,
) -> Result<T, serde_json::Error> {
    // Try parsing directly first (fast path for valid JSON)
    if let Ok(value) = serde_json::from_str(input) {
        return Ok(value);
    }

    // Try with repair
    let repaired = repair_json(input);
    serde_json::from_str(&repaired)
}

/// Remove markdown code fences that LLMs sometimes wrap around JSON.
///
/// Handles:
/// - \`\`\`json ... \`\`\`
/// - \`\`\` ... \`\`\`
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::json_repair::remove_json_markdown_fence;
///
/// let input = "```json\n{\"key\": \"value\"}\n```";
/// let result = remove_json_markdown_fence(input);
/// assert_eq!(result, r#"{"key": "value"}"#);
/// ```
fn remove_json_markdown_fence(input: &str) -> String {
    let trimmed = input.trim();

    // Remove ```json ... ``` wrapping
    if trimmed.starts_with("```json") {
        let without_start = trimmed.strip_prefix("```json").unwrap_or(trimmed);
        let without_end = without_start.strip_suffix("```").unwrap_or(without_start);
        return without_end.trim().to_string();
    }

    // Remove generic ``` ... ``` wrapping
    if trimmed.starts_with("```") {
        let without_start = trimmed.strip_prefix("```").unwrap_or(trimmed);
        let without_end = without_start.strip_suffix("```").unwrap_or(without_start);
        return without_end.trim().to_string();
    }

    input.to_string()
}

/// Remove trailing commas before closing brackets/braces.
///
/// JSON doesn't allow trailing commas, but many LLMs generate them
/// (especially when copying Python/JavaScript patterns).
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::json_repair::fix_trailing_commas;
///
/// let input = r#"{"items": [1, 2, 3,], "key": "value",}"#;
/// let result = fix_trailing_commas(input);
/// assert_eq!(result, r#"{"items": [1, 2, 3], "key": "value"}"#);
/// ```
fn fix_trailing_commas(input: &str) -> String {
    // Don't modify valid JSON
    if serde_json::from_str::<Value>(input).is_ok() {
        return input.to_string();
    }

    // Remove trailing commas before } or ]
    // The regex matches ",\s*}" and ",\s*]" patterns
    let result = TRAILING_COMMA_BRACE.replace_all(input, "}").to_string();
    TRAILING_COMMA_BRACKET.replace_all(&result, "]").to_string()
}

/// Fix unclosed brackets and braces by adding the missing closing characters.
///
/// Counts the number of opening vs closing brackets/braces and adds
/// the missing closing characters at the end.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::json_repair::fix_unclosed_brackets;
///
/// let input = r#"{"key": "value""#;
/// let result = fix_unclosed_brackets(input);
/// assert_eq!(result, r#"{"key": "value"}"#);
///
/// let input2 = r#"{"items": [1, 2"#;
/// let result2 = fix_unclosed_brackets(input2);
/// assert_eq!(result2, r#"{"items": [1, 2]}"#);
/// ```
/// Returns `true` if the byte at `byte_index` in `input` is inside a JSON
/// double-quoted string value, accounting for `\"` escape sequences.
///
/// This is the shared helper used by all three string-aware fixes.
fn is_inside_json_string(input: &str, byte_index: usize) -> bool {
    let mut in_string = false;
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < byte_index {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                // Skip escaped character (e.g. \", \\, \n, etc.)
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
        } else if b == b'"' {
            in_string = true;
        }
        i += 1;
    }
    in_string
}

fn fix_unclosed_brackets(input: &str) -> String {
    // Don't modify valid JSON
    if serde_json::from_str::<Value>(input).is_ok() {
        return input.to_string();
    }

    let mut open_braces = 0i32;
    let mut open_brackets = 0i32;

    // Count bracket depth, but only outside JSON string values
    for (byte_idx, ch) in input.char_indices() {
        if is_inside_json_string(input, byte_idx) {
            continue;
        }
        match ch {
            '{' => open_braces += 1,
            '}' => open_braces -= 1,
            '[' => open_brackets += 1,
            ']' => open_brackets -= 1,
            _ => {}
        }
    }

    let mut result = input.to_string();

    // Close unclosed brackets (close inner brackets first)
    for _ in 0..open_brackets {
        result.push(']');
    }
    for _ in 0..open_braces {
        result.push('}');
    }

    result
}

/// Replace single-quoted strings with double-quoted strings.
///
/// This is a simple heuristic that replaces all single quotes with double quotes.
/// It doesn't handle escaped quotes within strings, but it's sufficient for
/// common LLM mistakes where they use JavaScript-style single quotes.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::json_repair::fix_single_quotes;
///
/// let input = r#"{'key': 'value'}"#;
/// let result = fix_single_quotes(input);
/// assert_eq!(result, r#"{"key": "value"}"#);
/// ```
fn fix_single_quotes(input: &str) -> String {
    // Don't modify valid JSON
    if serde_json::from_str::<Value>(input).is_ok() {
        return input.to_string();
    }

    let mut result = String::with_capacity(input.len());
    let mut prev_char: Option<char> = None;

    for (byte_idx, ch) in input.char_indices() {
        match ch {
            '\'' if prev_char != Some('\\') && !is_inside_json_string(input, byte_idx) => {
                result.push('"');
            }
            _ => {
                result.push(ch);
            }
        }
        prev_char = Some(ch);
    }

    result
}

/// Add quotes around unquoted object keys.
///
/// Handles patterns like `{key: value}` -> `{"key": value}`.
/// Only matches keys that start with a letter or underscore and contain
/// alphanumeric characters or underscores (valid `JavaScript` identifiers).
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::json_repair::fix_unquoted_keys;
///
/// let input = r#"{key: "value", another_key: 123}"#;
/// let result = fix_unquoted_keys(input);
/// assert_eq!(result, r#"{"key": "value", "another_key": 123}"#);
/// ```
fn fix_unquoted_keys(input: &str) -> String {
    // Don't modify valid JSON
    if serde_json::from_str::<Value>(input).is_ok() {
        return input.to_string();
    }

    // Add quotes around unquoted keys: {key: value} -> {"key": value}
    // Pattern matches: { or , followed by optional whitespace, then an unquoted key, then colon
    UNQUOTED_KEY
        .replace_all(input, |caps: &regex::Captures| {
            // Use the full match to preserve spacing
            let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let key = &caps[2];

            // Preserve the original spacing
            if full_match.starts_with('{') {
                format!("{{\"{key}\":")
            } else {
                // For comma-separated keys, preserve the original spacing
                format!(", \"{key}\":")
            }
        })
        .to_string()
}

/// Fix Python-style True/False/None to JSON-compatible true/false/null.
///
/// Uses word boundaries to avoid replacing these words inside string values.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::json_repair::fix_boolean_null;
///
/// let input = r#"{"flag": True, "other": False, "nothing": None}"#;
/// let result = fix_boolean_null(input);
/// assert_eq!(result, r#"{"flag": true, "other": false, "nothing": null}"#);
/// ```
fn fix_boolean_null(input: &str) -> String {
    // Don't modify valid JSON
    if serde_json::from_str::<Value>(input).is_ok() {
        return input.to_string();
    }

    let replacements: &[(&str, &str)] = &[("True", "true"), ("False", "false"), ("None", "null")];

    let mut result = input.to_string();

    for (from, to) in replacements {
        let mut new_result = String::with_capacity(result.len());
        let mut last_end = 0;

        while let Some(idx) = result[last_end..].find(from) {
            let abs_idx = last_end + idx;
            let after_kw = abs_idx + from.len();

            let word_boundary_before = abs_idx == 0
                || !result.as_bytes()[abs_idx - 1].is_ascii_alphanumeric()
                    && result.as_bytes()[abs_idx - 1] != b'_';
            let word_boundary_after = after_kw >= result.len()
                || !result.as_bytes()[after_kw].is_ascii_alphanumeric()
                    && result.as_bytes()[after_kw] != b'_';

            if !is_inside_json_string(&result, abs_idx)
                && word_boundary_before
                && word_boundary_after
            {
                new_result.push_str(&result[last_end..abs_idx]);
                new_result.push_str(to);
                last_end = after_kw;
            } else {
                new_result.push_str(&result[last_end..=abs_idx]);
                last_end = abs_idx + 1;
            }
        }
        if last_end > 0 {
            new_result.push_str(&result[last_end..]);
            result = new_result;
        }
    }

    result
}

/// Extract JSON embedded in explanatory text.
///
/// LLMs sometimes wrap JSON in explanatory text like:
/// "Here's the JSON: {\"key\": \"value\"} - hope this helps!"
///
/// This function extracts the first valid JSON object or array from the input.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::json_repair::extract_json_from_text;
///
/// let input = "Here's the result: {\"key\": \"value\"} - done!";
/// let result = extract_json_from_text(input);
/// assert_eq!(result, r#"{"key": "value"}"#);
/// ```
fn extract_json_from_text(input: &str) -> String {
    // If the entire input is valid JSON, return it as-is
    if serde_json::from_str::<Value>(input).is_ok() {
        return input.to_string();
    }

    // Try to find the first { and last } that form valid JSON
    if let Some(start) = input.find('{') {
        if let Some(end) = input.rfind('}') {
            if start < end {
                let extracted = &input[start..=end];
                if serde_json::from_str::<Value>(extracted).is_ok() {
                    return extracted.to_string();
                }
            }
        }
    }

    // Try to find the first [ and last ] that form valid JSON
    if let Some(start) = input.find('[') {
        if let Some(end) = input.rfind(']') {
            if start < end {
                let extracted = &input[start..=end];
                if serde_json::from_str::<Value>(extracted).is_ok() {
                    return extracted.to_string();
                }
            }
        }
    }

    input.to_string()
}

/// Safely parse a JSON string that may contain control characters or be malformed.
///
/// First attempts to parse as-is (fast path for valid JSON). If that fails,
/// escapes any raw control characters (U+0000–U+001F) and retries.
/// If that also fails, applies the full repair pipeline.
///
/// This handles the common LLM error where raw newlines, tabs, and other
/// control characters are emitted inside JSON string values instead of
/// their escaped equivalents (`\n`, `\t`, etc.).
///
/// Inspired by goose's `safely_parse_json` in `providers/utils.rs`.
///
/// # Example
///
/// ```rust
/// use rustycode_tools::json_repair::safely_parse_json;
/// use serde_json::json;
///
/// // Valid JSON parses directly
/// let result = safely_parse_json(r#"{"key": "value"}"#).unwrap();
/// assert_eq!(result["key"], "value");
///
/// // JSON with raw control characters inside strings
/// let broken = "{\"key\": \"line1\nline2\"}";
/// let result = safely_parse_json(broken).unwrap();
/// assert_eq!(result["key"], "line1\nline2");
/// ```
pub fn safely_parse_json(input: &str) -> Result<Value, serde_json::Error> {
    // Fast path: try parsing as-is
    match serde_json::from_str(input) {
        Ok(value) => Ok(value),
        Err(_) => {
            // Try with control character escaping
            let escaped = escape_control_chars(input);
            match serde_json::from_str(&escaped) {
                Ok(value) => Ok(value),
                Err(_) => {
                    // Fall back to full repair pipeline
                    let repaired = repair_json(input);
                    serde_json::from_str(&repaired)
                }
            }
        }
    }
}

/// Escape raw control characters in a string that should be a JSON document.
///
/// Replaces literal control characters (U+0000–U+001F) with their JSON-escaped
/// equivalents. This fixes the common LLM error where raw newlines, tabs, etc.
/// are emitted inside JSON string values.
///
/// Does NOT escape quotes (`"`) or backslashes (`\`) — those are structural
/// JSON characters that must be preserved. This specifically targets the case
/// where an LLM outputs something like:
///
/// ```text
/// {"message": "Hello
/// World"}
/// ```
///
/// instead of:
///
/// ```text
/// {"message": "Hello\nWorld"}
/// ```
///
/// Inspired by goose's `json_escape_control_chars_in_string`.
pub fn escape_control_chars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\u{0000}'..='\u{001F}' => match c {
                '\u{0008}' => result.push_str("\\b"),
                '\u{000C}' => result.push_str("\\f"),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                _ => result.push_str(&format!("\\u{:04x}", c as u32)),
            },
            _ => result.push(c),
        }
    }
    result
}

/// Recursively unescape double-escaped JSON string values.
///
/// Fixes the common LLM error where JSON string values contain double-escaped
/// sequences like `\\n` (two characters: backslash + n) instead of the actual
/// newline character. Walks the entire JSON tree and fixes all string values.
///
/// # What it fixes
///
/// - `\\n` → `\n` (newline)
/// - `\\t` → `\t` (tab)
/// - `\\r` → `\r` (carriage return)
/// - `\\\"` → `"` (quote)
/// - `\\\\n` → `\n` (double-backslash + n → newline)
///
/// # Example
///
/// ```rust
/// use rustycode_tools::json_repair::unescape_json_values;
/// use serde_json::json;
///
/// let input = json!({"text": "Hello\\nWorld"});
/// let result = unescape_json_values(&input);
/// assert_eq!(result, json!({"text": "Hello\nWorld"}));
///
/// let input2 = json!({"items": ["a\\tb", "c\\rd"]});
/// let result2 = unescape_json_values(&input2);
/// assert_eq!(result2, json!({"items": ["a\tb", "c\rd"]}));
/// ```
///
/// Inspired by goose's `unescape_json_values` in `providers/utils.rs`.
pub fn unescape_json_values(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), unescape_json_values(v));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(unescape_json_values).collect()),
        Value::String(s) => {
            if s.contains('\\') {
                Value::String(
                    s.replace("\\\\n", "\n")
                        .replace("\\\\t", "\t")
                        .replace("\\\\r", "\r")
                        .replace("\\\\\"", "\"")
                        .replace("\\n", "\n")
                        .replace("\\t", "\t")
                        .replace("\\r", "\r")
                        .replace("\\\"", "\""),
                )
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_json_passthrough() {
        let input = r#"{"key": "value"}"#;
        assert_eq!(repair_json(input), input);
    }

    #[test]
    fn test_empty_input_returns_empty_object() {
        let input = "";
        assert_eq!(repair_json(input), "{}");
    }

    #[test]
    fn test_whitespace_input_returns_empty_object() {
        let input = "   \n\t  ";
        assert_eq!(repair_json(input), "{}");
    }

    #[test]
    fn test_trailing_comma_fix() {
        let input = r#"{"key": "value",}"#;
        let expected = r#"{"key": "value"}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_trailing_comma_in_array() {
        let input = r#"{"items": [1, 2, 3,]}"#;
        let expected = r#"{"items": [1, 2, 3]}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_markdown_fence_removal() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        let expected = r#"{"key": "value"}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_generic_markdown_fence_removal() {
        let input = "```\n{\"key\": \"value\"}\n```";
        let expected = r#"{"key": "value"}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_unclosed_bracket_fix() {
        let input = r#"{"key": "value""#;
        let expected = r#"{"key": "value"}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_unclosed_array_fix() {
        let input = r#"{"items": [1, 2"#;
        let expected = r#"{"items": [1, 2]}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_nested_unclosed_brackets() {
        let input = r#"{"outer": {"inner": "value""#;
        let expected = r#"{"outer": {"inner": "value"}}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_python_booleans() {
        let input = r#"{"flag": True, "other": False, "nothing": None}"#;
        let expected = r#"{"flag": true, "other": false, "nothing": null}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_single_quotes_to_double_quotes() {
        let input = r#"{'key': 'value'}"#;
        let result = repair_json(input);
        // Single quotes should be replaced
        assert!(result.contains("\"key\": \"value\""));
    }

    #[test]
    fn test_unquoted_keys() {
        let input = r#"{key: "value", another: 123}"#;
        let result = repair_json(input);
        // Keys should be quoted
        assert!(result.contains("\"key\""));
        assert!(result.contains("\"another\""));
    }

    #[test]
    fn test_json_extracted_from_text() {
        let input = "Here's the result: {\"key\": \"value\"} - hope this helps!";
        let expected = r#"{"key": "value"}"#;
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_array_extracted_from_text() {
        let input = "Result: [1, 2, 3] - done!";
        let expected = "[1, 2, 3]";
        assert_eq!(repair_json(input), expected);
    }

    #[test]
    fn test_parse_or_repair_valid() {
        let result: Result<Value, _> = parse_or_repair(r#"{"a": 1}"#);
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["a"], 1);
    }

    #[test]
    fn test_parse_or_repair_with_trailing_comma() {
        let result: Result<Value, _> = parse_or_repair(r#"{"a": 1,}"#);
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["a"], 1);
    }

    #[test]
    fn test_parse_or_repair_python_booleans() {
        let result: Result<Value, _> = parse_or_repair(r#"{"flag": True}"#);
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["flag"], true);
    }

    #[test]
    fn test_parse_or_repair_unclosed() {
        let result: Result<Value, _> = parse_or_repair(r#"{"a": 1"#);
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["a"], 1);
    }

    #[test]
    fn test_parse_or_repair_with_markdown_fence() {
        let result: Result<Value, _> = parse_or_repair("```json\n{\"a\": 1}\n```");
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["a"], 1);
    }

    #[test]
    fn test_parse_or_repair_embedded_in_text() {
        let result: Result<Value, _> = parse_or_repair("Result: {\"a\": 1} - done!");
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["a"], 1);
    }

    #[test]
    fn test_complex_repair_multiple_issues() {
        // Input has multiple issues: trailing comma, Python boolean, unclosed brace
        let input = r#"```json
{"flag": True, "count": 5,}"#;
        let result: Value = parse_or_repair(input).unwrap();
        assert_eq!(result["flag"], true);
        assert_eq!(result["count"], 5);
    }

    #[test]
    fn test_repair_does_not_modify_valid_json() {
        let input = r#"{"key": "value", "nested": {"a": 1, "b": 2}, "arr": [1, 2, 3]}"#;
        assert_eq!(repair_json(input), input);
    }

    #[test]
    fn test_remove_json_markdown_fence_direct() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        let result = remove_json_markdown_fence(input);
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_remove_generic_markdown_fence_direct() {
        let input = "```\n{\"key\": \"value\"}\n```";
        let result = remove_json_markdown_fence(input);
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_fix_trailing_commas_direct() {
        let input = r#"{"key": "value", "items": [1, 2, 3,]}"#;
        let result = fix_trailing_commas(input);
        assert_eq!(result, r#"{"key": "value", "items": [1, 2, 3]}"#);
    }

    #[test]
    fn test_fix_unclosed_brackets_direct() {
        let input = r#"{"key": {"nested": "value""#;
        let result = fix_unclosed_brackets(input);
        assert_eq!(result, r#"{"key": {"nested": "value"}}"#);
    }

    #[test]
    fn test_fix_boolean_null_direct() {
        let input = r#"{"active": True, "deleted": False, "data": None}"#;
        let result = fix_boolean_null(input);
        assert_eq!(
            result,
            r#"{"active": true, "deleted": false, "data": null}"#
        );
    }

    #[test]
    fn test_fix_single_quotes_direct() {
        let input = r#"{'key': 'value', 'number': 123}"#;
        let result = fix_single_quotes(input);
        assert_eq!(result, r#"{"key": "value", "number": 123}"#);
    }

    #[test]
    fn test_fix_unquoted_keys_direct() {
        let input = r#"{key: "value", another_key: 123}"#;
        let result = fix_unquoted_keys(input);
        assert_eq!(result, r#"{"key": "value", "another_key": 123}"#);
    }

    #[test]
    fn test_extract_json_from_text_direct() {
        let input = "The result is: {\"status\": \"ok\", \"code\": 200} - thanks!";
        let result = extract_json_from_text(input);
        assert_eq!(result, r#"{"status": "ok", "code": 200}"#);
    }

    #[test]
    fn test_extract_array_from_text_direct() {
        let input = "Items: [1, 2, 3, 4, 5] - that's all!";
        let result = extract_json_from_text(input);
        assert_eq!(result, r#"[1, 2, 3, 4, 5]"#);
    }

    // ── Safe JSON Parsing Tests ──────────────────────────────────────────────

    #[test]
    fn test_safely_parse_valid_json() {
        let result = safely_parse_json(r#"{"key": "value"}"#).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn test_safely_parse_with_raw_newlines() {
        // JSON with actual unescaped newlines inside string values
        let broken = "{\"key\": \"line1\nline2\"}";
        let result = safely_parse_json(broken).unwrap();
        assert_eq!(result["key"], "line1\nline2");
    }

    #[test]
    fn test_safely_parse_with_raw_tabs() {
        let broken = "{\"key\": \"col1\tcol2\"}";
        let result = safely_parse_json(broken).unwrap();
        assert_eq!(result["key"], "col1\tcol2");
    }

    #[test]
    fn test_safely_parse_with_multiple_control_chars() {
        let broken = "{\"msg\": \"line1\nline2\ttab\rcr\"}";
        let result = safely_parse_json(broken).unwrap();
        assert_eq!(result["msg"], "line1\nline2\ttab\rcr");
    }

    #[test]
    fn test_safely_parse_with_already_escaped_stays_valid() {
        let valid = r#"{"key": "value with\nnewline"}"#;
        let result = safely_parse_json(valid).unwrap();
        assert_eq!(result["key"], "value with\nnewline");
    }

    #[test]
    fn test_safely_parse_empty_object() {
        let result = safely_parse_json("{}").unwrap();
        assert!(result.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_safely_parse_completely_broken_fails() {
        let broken = r#"{"key": "unclosed_string"#;
        assert!(safely_parse_json(broken).is_err());
    }

    #[test]
    fn test_safely_parse_falls_back_to_repair() {
        // Trailing comma - control char escape won't fix it, but repair pipeline will
        let input = r#"{"key": "value",}"#;
        let result = safely_parse_json(input).unwrap();
        assert_eq!(result["key"], "value");
    }

    // ── Control Character Escaping Tests ─────────────────────────────────────

    #[test]
    fn test_escape_control_chars_basic() {
        assert_eq!(escape_control_chars("Hello\nWorld"), "Hello\\nWorld");
        assert_eq!(escape_control_chars("Hello\tWorld"), "Hello\\tWorld");
        assert_eq!(escape_control_chars("Hello\rWorld"), "Hello\\rWorld");
    }

    #[test]
    fn test_escape_control_chars_multiple() {
        assert_eq!(
            escape_control_chars("Hello\n\tWorld\r"),
            "Hello\\n\\tWorld\\r"
        );
    }

    #[test]
    fn test_escape_control_chars_preserves_quotes() {
        assert_eq!(escape_control_chars("Hello \"World\""), "Hello \"World\"");
    }

    #[test]
    fn test_escape_control_chars_preserves_backslashes() {
        assert_eq!(escape_control_chars("Hello\\World"), "Hello\\World");
    }

    #[test]
    fn test_escape_control_chars_no_changes_for_normal() {
        assert_eq!(escape_control_chars("Hello World"), "Hello World");
    }

    #[test]
    fn test_escape_control_chars_unicode_for_unknown() {
        assert_eq!(
            escape_control_chars("Hello\u{0001}World"),
            "Hello\\u0001World"
        );
    }

    // ── JSON Value Unescaping Tests ──────────────────────────────────────────

    #[test]
    fn test_unescape_json_values_object() {
        let value = json!({"text": "Hello\\nWorld"});
        let unescaped = unescape_json_values(&value);
        assert_eq!(unescaped, json!({"text": "Hello\nWorld"}));
    }

    #[test]
    fn test_unescape_json_values_array() {
        let value = json!(["Hello\\nWorld", "Goodbye\\tWorld"]);
        let unescaped = unescape_json_values(&value);
        assert_eq!(unescaped, json!(["Hello\nWorld", "Goodbye\tWorld"]));
    }

    #[test]
    fn test_unescape_json_values_nested() {
        let value = json!({
            "text": "Hello\\nWorld",
            "array": ["a\\tb", "c\\rd"],
            "nested": {
                "inner": "quote\\\"test\\\""
            }
        });
        let unescaped = unescape_json_values(&value);
        assert_eq!(
            unescaped,
            json!({
                "text": "Hello\nWorld",
                "array": ["a\tb", "c\rd"],
                "nested": {
                    "inner": "quote\"test\""
                }
            })
        );
    }

    #[test]
    fn test_unescape_json_values_no_escapes() {
        let value = json!({"text": "Hello World"});
        let unescaped = unescape_json_values(&value);
        assert_eq!(unescaped, json!({"text": "Hello World"}));
    }

    #[test]
    fn test_unescape_json_values_non_string_types_unchanged() {
        let value = json!({"num": 42, "bool": true, "null": null});
        let unescaped = unescape_json_values(&value);
        assert_eq!(unescaped, value);
    }

    #[test]
    fn test_unescape_json_values_double_backslash_n() {
        // \\n (two chars) should become \n (newline)
        let value = json!({"text": "Hello\\\\nWorld"});
        let unescaped = unescape_json_values(&value);
        assert_eq!(unescaped, json!({"text": "Hello\nWorld"}));
    }

    // ── String-State Tracking Regression Tests ────────────────────────────────

    #[test]
    fn test_brackets_inside_strings_not_counted() {
        let input = r#"{"text": "some { brackets [ here"}"#;
        let result = fix_unclosed_brackets(input);
        assert_eq!(result, input, "should not add spurious closing brackets");
    }

    #[test]
    fn test_brackets_inside_strings_with_unclosed_outer() {
        let input = r#"{"text": "some { brackets [ here", "nested": {"a": 1"#;
        let result = fix_unclosed_brackets(input);
        assert_eq!(
            result,
            r#"{"text": "some { brackets [ here", "nested": {"a": 1}}"#
        );
    }

    #[test]
    fn test_brackets_escaped_quote_in_string() {
        let input = r#"{"text": "quote\" inside { bracket"}"#;
        let result = fix_unclosed_brackets(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_boolean_null_not_replaced_inside_strings() {
        let input = r#"{"title": "This is True", "flag": True}"#;
        let result = fix_boolean_null(input);
        assert_eq!(result, r#"{"title": "This is True", "flag": true}"#);
    }

    #[test]
    fn test_boolean_false_and_none_inside_strings_preserved() {
        let input = r#"{"a": "False alarm", "b": False, "c": "None of the above", "d": None}"#;
        let result = fix_boolean_null(input);
        assert_eq!(
            result,
            r#"{"a": "False alarm", "b": false, "c": "None of the above", "d": null}"#
        );
    }

    #[test]
    fn test_boolean_inside_string_with_escaped_quotes() {
        let input = r#"{"t": "say \"True\" loudly", "flag": True}"#;
        let result = fix_boolean_null(input);
        assert_eq!(result, r#"{"t": "say \"True\" loudly", "flag": true}"#);
    }

    #[test]
    fn test_single_quotes_inside_double_quoted_strings_preserved() {
        let input = r#"{"text": "it's working"}"#;
        let result = fix_single_quotes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_single_quotes_replaced_only_outside_strings() {
        let input = r#"{'key': "it's fine"}"#;
        let result = fix_single_quotes(input);
        assert_eq!(result, r#"{"key": "it's fine"}"#);
    }

    #[test]
    fn test_apostrophe_with_escaped_quotes() {
        let input = r#"{"text": "it's \"quoted\" here"}"#;
        let result = fix_single_quotes(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_repair_pipeline_brackets_in_string() {
        let input = r#"{"text": "{ [ brackets ]"}"#;
        assert_eq!(repair_json(input), input);
    }

    #[test]
    fn test_repair_pipeline_boolean_in_string() {
        let input = r#"{"title": "True story"}"#;
        assert_eq!(repair_json(input), input);
    }

    #[test]
    fn test_repair_pipeline_apostrophe_in_string() {
        let input = r#"{"msg": "it's fine"}"#;
        assert_eq!(repair_json(input), input);
    }
}
