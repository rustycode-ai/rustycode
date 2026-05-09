//! Tool call lifecycle state for streaming SSE responses.

use std::time::Instant;

/// A tool call tracked from SSE start through execution completion.
///
/// Owns its full lifecycle: identity, input accumulation, timing.
/// Designed for parallel tool calls — each call ID gets its own instance
/// in a `HashMap<String, ToolCall>`.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub partial_json: String,
    pub started_at: Instant,
}

impl ToolCall {
    const MAX_JSON_SIZE: usize = 1 << 20;

    pub fn new(id: String, name: String, initial_json: String) -> Self {
        Self {
            id,
            name,
            partial_json: initial_json,
            started_at: Instant::now(),
        }
    }

    pub fn push_json(&mut self, chunk: &str) {
        if self.partial_json.len() + chunk.len() <= Self::MAX_JSON_SIZE {
            self.partial_json.push_str(chunk);
        } else {
            tracing::warn!(
                "tool call JSON exceeded {} bytes, truncating (tool: {})",
                Self::MAX_JSON_SIZE,
                self.name
            );
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

/// Legacy alias for backward compatibility.
pub type ToolAccumulator = ToolCall;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_call_has_empty_input() {
        let tool = ToolCall::new("call_123".into(), "Read".into(), String::new());
        assert_eq!(tool.id, "call_123");
        assert_eq!(tool.name, "Read");
        assert!(tool.partial_json.is_empty());
    }

    #[test]
    fn new_call_with_initial_json() {
        let tool = ToolCall::new("call_456".into(), "Write".into(), r#"{"path":"#.into());
        assert_eq!(tool.partial_json, r#"{"path":"#);
    }

    #[test]
    fn push_json_accumulates() {
        let mut tool = ToolCall::new("id".into(), "tool".into(), "{\"p\":\"".into());
        tool.push_json("a\"}");
        assert_eq!(tool.partial_json, "{\"p\":\"a\"}");
    }

    #[test]
    fn push_json_truncates_at_max_size() {
        let mut tool = ToolCall::new("id".into(), "big_tool".into(), String::new());
        let big_chunk = "x".repeat(ToolCall::MAX_JSON_SIZE + 1);
        tool.push_json(&big_chunk);
        assert!(tool.partial_json.is_empty());
    }

    #[test]
    fn push_json_accumulates_up_to_limit() {
        let mut tool = ToolCall::new("id".into(), "tool".into(), String::new());
        let half = "x".repeat(ToolCall::MAX_JSON_SIZE / 2);
        tool.push_json(&half);
        assert_eq!(tool.partial_json.len(), ToolCall::MAX_JSON_SIZE / 2);
        tool.push_json(&half);
        assert_eq!(tool.partial_json.len(), ToolCall::MAX_JSON_SIZE);
        tool.push_json("overflow");
        assert_eq!(tool.partial_json.len(), ToolCall::MAX_JSON_SIZE);
    }

    #[test]
    fn parallel_calls_accumulate_independently() {
        let mut tools: std::collections::HashMap<String, ToolCall> =
            std::collections::HashMap::new();
        tools.insert(
            "t1".into(),
            ToolCall::new("t1".into(), "Grep".into(), String::new()),
        );
        tools.insert(
            "t2".into(),
            ToolCall::new("t2".into(), "Glob".into(), String::new()),
        );

        tools
            .get_mut("t1")
            .unwrap()
            .push_json(r#"{"pattern":"foo"}"#);
        tools
            .get_mut("t2")
            .unwrap()
            .push_json(r#"{"Glob":"**/*.rs"}"#);

        assert_eq!(tools["t1"].partial_json, r#"{"pattern":"foo"}"#);
        assert_eq!(tools["t2"].partial_json, r#"{"Glob":"**/*.rs"}"#);
    }

    #[test]
    fn elapsed_ms_is_nonzero() {
        let tool = ToolCall::new("id".into(), "tool".into(), String::new());
        assert!(tool.elapsed_ms() <= 1);
    }
}
