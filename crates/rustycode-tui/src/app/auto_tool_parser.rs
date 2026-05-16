use anyhow::Context;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToolCall {
    pub name: String,
    pub arguments: Value,
}

pub fn extract_tool_payloads(response: &str) -> Vec<String> {
    let mut in_tool_block = false;
    let mut tool_block_lines: Vec<String> = Vec::new();
    let mut tool_payloads: Vec<String> = Vec::new();
    let fence_state = build_fence_state(response);

    for (idx, line) in response.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "```tool" || trimmed == "```tools" {
            in_tool_block = true;
            tool_block_lines.clear();
            continue;
        }
        if trimmed == "```" && in_tool_block {
            in_tool_block = false;
            let payload = tool_block_lines.join("\n");
            if !payload.trim().is_empty() {
                tool_payloads.push(payload);
            }
            continue;
        }
        if in_tool_block {
            tool_block_lines.push(line.to_string());
            continue;
        }

        // Also extract inline tool JSON (not in ``` blocks)
        if looks_like_tool_json_payload(line) && !fence_state[idx] {
            tool_payloads.push(line.to_string());
        }
    }

    tool_payloads
}

pub fn parse_tool_calls_payload(payload: &str) -> anyhow::Result<Vec<ParsedToolCall>> {
    let parsed =
        serde_json::from_str::<Value>(payload).context("Invalid JSON in tool call payload")?;

    let mut calls: Vec<Value> = Vec::new();
    if parsed.is_array() {
        calls.extend(parsed.as_array().cloned().unwrap_or_default());
    } else if let Some(arr) = parsed.get("calls").and_then(Value::as_array) {
        calls.extend(arr.clone());
    } else if parsed.is_object() {
        calls.push(parsed);
    }

    let mut out = Vec::new();
    for call_json in calls {
        let name = call_json
            .get("name")
            .or_else(|| call_json.get("tool"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let arguments = call_json
            .get("input") // Anthropic API format (preferred)
            .or_else(|| call_json.get("arguments"))
            .or_else(|| call_json.get("args"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        out.push(ParsedToolCall { name, arguments });
    }

    Ok(out)
}

/// Pre-compute whether each line is inside a fenced code block (O(n) instead of O(n²)).
fn build_fence_state(text: &str) -> Vec<bool> {
    let lines: Vec<&str> = text.lines().collect();
    let mut state = vec![false; lines.len()];
    let mut in_fence = false;
    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
        }
        state[i] = in_fence;
    }
    state
}

fn looks_like_tool_json_payload(line: &str) -> bool {
    let s = line.trim();
    if s.is_empty() {
        return false;
    }

    let starts_like_json = s.starts_with('{') || s.starts_with('[');
    if !starts_like_json {
        return false;
    }

    (s.contains("\"calls\"") && (s.contains("\"name\"") || s.contains("\"tool\"")))
        || (s.contains("\"name\"") && (s.contains("\"arguments\"") || s.contains("\"args\"")))
}

/// Remove raw tool payload artifacts from assistant display text.
/// Tool payloads are still parsed/executed separately; this only cleans UI output.
pub fn sanitize_tool_artifacts_for_display(response: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_tool_block = false;
    let mut removed_any = false;
    let fence_state = build_fence_state(response);

    for (idx, raw) in response.lines().enumerate() {
        let line = raw.trim();
        if line == "```tool" || line == "```tools" {
            in_tool_block = true;
            removed_any = true;
            continue;
        }
        if in_tool_block && line == "```" {
            in_tool_block = false;
            continue;
        }
        if in_tool_block {
            continue;
        }

        if looks_like_tool_json_payload(line) && !fence_state[idx] {
            removed_any = true;
            continue;
        }

        out.push(raw);
    }

    let cleaned = out
        .join("\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if cleaned.is_empty() && removed_any {
        "Tool calls prepared and executed.".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tool_blocks() {
        let text = "before\n```tool\n{\"name\":\"read\",\"input\":{}}\n```\nafter";
        let payloads = extract_tool_payloads(text);
        assert_eq!(payloads.len(), 1);
        assert!(payloads[0].contains("\"name\":\"read\""));
    }

    #[test]
    fn parses_single_object() {
        let calls = parse_tool_calls_payload("{\"name\":\"read\",\"input\":{\"path\":\"a\"}}")
            .expect("parses object");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read");
    }

    #[test]
    fn parses_calls_wrapper() {
        let calls = parse_tool_calls_payload(
            "{\"calls\":[{\"name\":\"read\",\"input\":{}},{\"tool\":\"glob\",\"input\":{}}]}",
        )
        .expect("parses calls wrapper");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[1].name, "glob");
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_tool_calls_payload("{").expect_err("should fail");
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn sanitizes_tool_block_for_display() {
        let text = "Plan\n```tool\n{\"name\":\"read_file\",\"input\":{}}\n```\nDone";
        let cleaned = sanitize_tool_artifacts_for_display(text);
        assert_eq!(cleaned, "Plan\nDone");
    }

    #[test]
    fn sanitizes_inline_tool_json_for_display() {
        let text =
            "I will run tools\n{\"calls\":[{\"name\":\"grep\",\"input\":{}}]}\nThen summarize";
        let cleaned = sanitize_tool_artifacts_for_display(text);
        assert_eq!(cleaned, "I will run tools\nThen summarize");
    }

    #[test]
    fn parses_anthropic_tool_format() {
        // This is the format generated by rustycode-llm/src/anthropic.rs
        let text = "```tool\n{\"name\":\"bash\",\"input\":{\"command\":\"ls -la\"}}\n```";
        let payloads = extract_tool_payloads(text);
        assert_eq!(payloads.len(), 1);
        let calls = parse_tool_calls_payload(&payloads[0]).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "bash");
        assert_eq!(calls[0].arguments["command"], "ls -la");
    }

    #[test]
    fn parses_anthropic_web_search_format() {
        // Test with web_search server tool format
        let text = "```tool\n{\"name\":\"web_search\",\"input\":{\"query\":\"Rust async\"}}\n```";
        let payloads = extract_tool_payloads(text);
        assert_eq!(payloads.len(), 1);
        let calls = parse_tool_calls_payload(&payloads[0]).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[0].arguments["query"], "Rust async");
    }
}
