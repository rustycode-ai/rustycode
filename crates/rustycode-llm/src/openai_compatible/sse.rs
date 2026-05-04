//! SSE stream parsing for OpenAI-compatible providers.
//!
//! Parses `choices[0].delta` SSE events into protocol-level `StreamEvent` values,
//! with proper tool call state tracking (ToolCallStarted + ToolInputDelta),
//! thinking/reasoning support, usage extraction, and error handling.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rustycode_protocol::stream_event::StreamEvent;

use crate::provider::{normalize_stop_reason, ProviderError, Usage};

/// Configuration for SSE parsing behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct SseParseConfig {
    /// Enable extraction of tool call deltas (ToolCallStarted, ToolInputDelta).
    pub enable_tool_calls: bool,
    /// Enable extraction of thinking/reasoning content.
    pub enable_thinking: bool,
    /// Enable extraction of usage information (TokenUsage).
    pub enable_usage: bool,
    /// Enable extraction of refusal content.
    pub enable_refusal: bool,
}

impl SseParseConfig {
    /// Enable all features.
    pub fn all() -> Self {
        Self {
            enable_tool_calls: true,
            enable_thinking: true,
            enable_usage: true,
            enable_refusal: true,
        }
    }

    /// Minimal config (text only, no tool calls, no thinking, no usage).
    pub fn minimal() -> Self {
        Self::default()
    }
}

/// State tracked across SSE lines within a single streaming response.
///
/// Required for tool call state management: the OpenAI API streams tool call
/// deltas incrementally (first chunk has `id` + `function.name`, subsequent
/// chunks have `function.arguments` fragments). This struct tracks which tool
/// call IDs have been seen so that `ToolInputDelta` events can reference the
/// correct tool call even when the `id` field is absent from later chunks.
///
/// Uses `Arc<Mutex<>>` internally so it can be cloned and shared across
/// stream combinator closures (same pattern as `SseByteBuffer`).
#[derive(Debug, Default)]
pub struct SseParseState(Arc<Mutex<HashMap<usize, String>>>);

impl Clone for SseParseState {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Parse SSE lines from an OpenAI-compatible stream into `StreamEvent` values.
///
/// Callers should feed complete lines (e.g., from `SseByteBuffer::feed_chunk`)
/// joined by `\n`. Returns both the parsed events and the updated parse state.
///
/// # Arguments
/// * `lines` - Complete SSE lines (may span multiple `\n`-separated lines)
/// * `config` - Feature toggles for what to extract
/// * `state` - Mutable state for tracking tool call IDs across chunks
pub fn parse_openai_sse_lines(
    lines: &str,
    config: SseParseConfig,
    state: &SseParseState,
) -> Vec<Result<StreamEvent, ProviderError>> {
    let mut events = Vec::new();

    for line in lines.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        // Only process `data:` lines
        if line.starts_with("data: ") {
            let json_str = line.trim_start_matches("data: ").trim();

            if json_str == "[DONE]" {
                events.push(Ok(StreamEvent::Done));
                continue;
            }

            if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                dispatch_data_payload(&data, &mut events, state, config);
            }
        } else if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
            // Handle non-SSE JSON lines (some providers send raw JSON errors)
            if let Some(err) = extract_stream_error(&data) {
                events.push(err);
            }
        }
    }

    events
}

/// Route a parsed JSON payload: choices → delta/finish, or top-level error.
fn dispatch_data_payload(
    data: &serde_json::Value,
    events: &mut Vec<Result<StreamEvent, ProviderError>>,
    state: &SseParseState,
    config: SseParseConfig,
) {
    if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(delta) = choice.get("delta") {
                extract_delta_events(delta, events, state, config);
            }
            if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                extract_finish_events(data, finish_reason, events, config);
            }
        }
    } else if let Some(err) = extract_stream_error(data) {
        events.push(err);
    }
}

/// Extract TextDelta, ThinkingDelta, ToolCallStarted, and ToolInputDelta from a delta object.
fn extract_delta_events(
    delta: &serde_json::Value,
    events: &mut Vec<Result<StreamEvent, ProviderError>>,
    state: &SseParseState,
    config: SseParseConfig,
) {
    // Text content
    if let Some(content_str) = delta
        .get("content")
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
    {
        events.push(Ok(StreamEvent::TextDelta {
            content: content_str.to_string(),
        }));
    }

    // Reasoning content (two key variants used by different providers)
    if config.enable_thinking {
        for key in ["reasoning_content", "reasoning"] {
            if let Some(reasoning_str) = delta
                .get(key)
                .and_then(|r| r.as_str())
                .filter(|s| !s.is_empty())
            {
                events.push(Ok(StreamEvent::ThinkingDelta {
                    content: reasoning_str.to_string(),
                }));
            }
        }
    }

    // Tool call deltas
    if config.enable_tool_calls {
        if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
            for tc_delta in tool_calls {
                extract_tool_call_delta(tc_delta, events, state);
            }
        }
    }

    // Refusal content
    if config.enable_refusal {
        if let Some(refusal_str) = delta
            .get("refusal")
            .and_then(|r| r.as_str())
            .filter(|s| !s.is_empty())
        {
            events.push(Ok(StreamEvent::TextDelta {
                content: refusal_str.to_string(),
            }));
        }
    }
}

/// Handle a single tool call delta entry within the tool_calls array.
fn extract_tool_call_delta(
    tc_delta: &serde_json::Value,
    events: &mut Vec<Result<StreamEvent, ProviderError>>,
    state: &SseParseState,
) {
    let index = tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

    let resolved_id = tc_delta
        .get("id")
        .and_then(|i| i.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            let map = state.0.lock().unwrap_or_else(|e| e.into_inner());
            map.get(&index).cloned()
        });

    // Tool call start (id present)
    if let Some(id) = tc_delta.get("id").and_then(|i| i.as_str()) {
        {
            let mut map = state.0.lock().unwrap_or_else(|e| e.into_inner());
            map.insert(index, id.to_string());
        }
        let name = tc_delta
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        events.push(Ok(StreamEvent::ToolCallStarted {
            id: id.to_string(),
            name,
        }));
    }

    // Partial function arguments
    if let Some(partial) = tc_delta
        .get("function")
        .and_then(|f| f.get("arguments"))
        .and_then(|a| a.as_str())
        .filter(|s| !s.is_empty())
    {
        if let Some(id) = resolved_id {
            events.push(Ok(StreamEvent::ToolInputDelta {
                id,
                chunk: partial.to_string(),
            }));
        }
    }
}

/// Extract TokenUsage and TurnCompleted events when a finish_reason is present.
fn extract_finish_events(
    data: &serde_json::Value,
    finish_reason: &str,
    events: &mut Vec<Result<StreamEvent, ProviderError>>,
    config: SseParseConfig,
) {
    if config.enable_usage {
        if let Some(usage) = data.get("usage").and_then(parse_usage) {
            events.push(Ok(StreamEvent::TokenUsage {
                input_tokens: u64::from(usage.input_tokens),
                output_tokens: u64::from(usage.output_tokens),
            }));
        }
    }

    let stop_reason =
        normalize_stop_reason(Some(finish_reason)).unwrap_or_else(|| finish_reason.to_string());
    events.push(Ok(StreamEvent::TurnCompleted { stop_reason }));
}

/// Parse a usage JSON object into a [`Usage`] struct.
fn parse_usage(u: &serde_json::Value) -> Option<Usage> {
    let input_tokens = u.get("prompt_tokens")?.as_u64()? as u32;
    let output_tokens = u.get("completion_tokens")?.as_u64()? as u32;
    let cached_tokens: u32 = u
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    if cached_tokens > 0 {
        let hit_pct = (cached_tokens * 100).checked_div(input_tokens).unwrap_or(0);
        tracing::info!("Cache: {hit_pct}% hit ({cached_tokens}/{input_tokens} prompt tokens)");
    }
    Some(Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        cache_read_input_tokens: cached_tokens,
        cache_creation_input_tokens: 0,
        reasoning_tokens: None,
    })
}

/// Extract an error from a JSON value that has a top-level "error" object.
fn extract_stream_error(data: &serde_json::Value) -> Option<Result<StreamEvent, ProviderError>> {
    let error = data.get("error")?;
    let code = error
        .get("code")
        .and_then(|c| c.as_str())
        .unwrap_or("unknown");
    let message = error
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Unknown streaming error");
    Some(Err(ProviderError::api(format!("{}: {}", code, message))))
}

/// Aggregate SSE text events into a single content string.
pub fn aggregate_sse_text(events: &[Result<StreamEvent, ProviderError>]) -> String {
    events
        .iter()
        .filter_map(|e| e.as_ref().ok())
        .filter_map(|e| match e {
            StreamEvent::TextDelta { content } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &str, config: SseParseConfig) -> Vec<Result<StreamEvent, ProviderError>> {
        let state = SseParseState::default();
        parse_openai_sse_lines(lines, config, &state)
    }

    #[test]
    fn text_delta_extraction() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n";
        let events = parse(input, SseParseConfig::minimal());
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::TextDelta { content }) if content == "hello"
        ));
    }

    #[test]
    fn empty_content_skipped() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n";
        let events = parse(input, SseParseConfig::minimal());
        assert!(events.is_empty());
    }

    #[test]
    fn done_event() {
        let input = "data: [DONE]\n";
        let events = parse(input, SseParseConfig::minimal());
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Ok(StreamEvent::Done)));
    }

    #[test]
    fn thinking_delta_reasoning_content() {
        let input = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking...\"}}]}\n";
        let events = parse(input, SseParseConfig::all());
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::ThinkingDelta { content }) if content == "thinking..."
        ));
    }

    #[test]
    fn thinking_delta_reasoning_key() {
        let input = "data: {\"choices\":[{\"delta\":{\"reasoning\":\"pondering...\"}}]}\n";
        let events = parse(input, SseParseConfig::all());
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::ThinkingDelta { content }) if content == "pondering..."
        ));
    }

    #[test]
    fn thinking_disabled() {
        let input = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking...\"}}]}\n";
        let events = parse(input, SseParseConfig::minimal());
        assert!(events.is_empty());
    }

    #[test]
    fn tool_call_start_and_input() {
        let line1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file"}}]}}]}"#;
        let line2 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\"}"}}]}}]}"#;
        let input = format!("{line1}\n{line2}\n");
        let state = SseParseState::default();
        let events = parse_openai_sse_lines(&input, SseParseConfig::all(), &state);

        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::ToolCallStarted { id, name }) if id == "call_1" && name == "read_file"
        ));
        assert!(matches!(
            &events[1],
            Ok(StreamEvent::ToolInputDelta { id, chunk }) if id == "call_1" && chunk == "{\"path\"}"
        ));
        let map = state.0.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(map.get(&0), Some(&"call_1".to_string()));
    }

    #[test]
    fn tool_call_args_without_id_uses_state() {
        let input1 = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_42\",\"function\":{\"name\":\"bash\"}}]}}]}\n";
        let input2 = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"cmd\"}}]}}]}\n";

        let state = SseParseState::default();
        let mut events = Vec::new();
        events.extend(parse_openai_sse_lines(
            input1,
            SseParseConfig::all(),
            &state,
        ));
        events.extend(parse_openai_sse_lines(
            input2,
            SseParseConfig::all(),
            &state,
        ));

        assert!(matches!(
            &events[1],
            Ok(StreamEvent::ToolInputDelta { id, chunk }) if id == "call_42" && chunk == "cmd"
        ));
    }

    #[test]
    fn multiple_tool_calls() {
        let line1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"tool_a"}}]}}]}"#;
        let line2 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"c2","function":{"name":"tool_b"}}]}}]}"#;
        let input = format!("{line1}\n{line2}\n");
        let events = parse(&input, SseParseConfig::all());

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], Ok(StreamEvent::ToolCallStarted { id, .. }) if id == "c1"));
        assert!(matches!(&events[1], Ok(StreamEvent::ToolCallStarted { id, .. }) if id == "c2"));
    }

    #[test]
    fn tool_calls_disabled() {
        let input = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"t\"}}]}}]}\n";
        let events = parse(input, SseParseConfig::minimal());
        assert!(events.is_empty());
    }

    #[test]
    fn finish_reason_with_usage() {
        let input = r#"data: {"choices":[{"finish_reason":"stop", "delta":{}}], "usage":{"prompt_tokens":10,"completion_tokens":20}}"#;
        let events = parse(&format!("{input}\n"), SseParseConfig::all());

        // Should have TokenUsage + TurnCompleted
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::TokenUsage {
                input_tokens: 10,
                output_tokens: 20
            })
        ));
        assert!(matches!(
            &events[1],
            Ok(StreamEvent::TurnCompleted { stop_reason }) if stop_reason == "end_turn"
        ));
    }

    #[test]
    fn finish_reason_tool_calls_normalized() {
        let input = "data: {\"choices\":[{\"finish_reason\":\"tool_calls\", \"delta\":{}}]}\n";
        let events = parse(input, SseParseConfig::minimal());

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::TurnCompleted { stop_reason }) if stop_reason == "tool_use"
        ));
    }

    #[test]
    fn usage_disabled() {
        let input = r#"data: {"choices":[{"finish_reason":"stop", "delta":{}}], "usage":{"prompt_tokens":10,"completion_tokens":20}}"#;
        let events = parse(&format!("{input}\n"), SseParseConfig::minimal());

        // Only TurnCompleted, no TokenUsage
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Ok(StreamEvent::TurnCompleted { .. })));
    }

    #[test]
    fn error_object_extraction() {
        let input =
            "data: {\"error\":{\"code\":\"rate_limit\",\"message\":\"Too many requests\"}}\n";
        let events = parse(input, SseParseConfig::minimal());
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
        assert!(events[0]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("rate_limit: Too many requests"));
    }

    #[test]
    fn non_sse_json_error() {
        let input = "{\"error\":{\"code\":\"auth\",\"message\":\"Invalid key\"}}\n";
        let events = parse(input, SseParseConfig::minimal());
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
    }

    #[test]
    fn refusal_content() {
        let input = "data: {\"choices\":[{\"delta\":{\"refusal\":\"I cannot help with that\"}}]}\n";
        let events = parse(input, SseParseConfig::all());
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::TextDelta { content }) if content == "I cannot help with that"
        ));
    }

    #[test]
    fn mixed_events_full_stream() {
        let lines = [
            r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":" world"}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"bash"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ls"}}]}}]}"#,
            r#"data: {"choices":[{"finish_reason":"tool_calls", "delta":{}}]}"#,
            "data: [DONE]",
        ];
        let input = lines.join("\n") + "\n";
        let events = parse(&input, SseParseConfig::all());

        // 2 text + 1 tool start + 1 tool input + 1 turn completed + 1 done = 6
        assert_eq!(events.len(), 6);
    }

    #[test]
    fn aggregate_text() {
        let events = vec![
            Ok(StreamEvent::TextDelta {
                content: "hello ".to_string(),
            }),
            Ok(StreamEvent::ThinkingDelta {
                content: "thinking".to_string(),
            }),
            Ok(StreamEvent::TextDelta {
                content: "world".to_string(),
            }),
        ];
        assert_eq!(aggregate_sse_text(&events), "hello world");
    }

    #[test]
    fn empty_lines_skipped() {
        let input = "\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n";
        let events = parse(input, SseParseConfig::minimal());
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn cache_hit_logging() {
        let input = "\
            data: {\"choices\":[{\"finish_reason\":\"stop\", \"delta\":{}}], \
            \"usage\":{\"prompt_tokens\":100,\"completion_tokens\":50, \
            \"prompt_tokens_details\":{\"cached_tokens\":80}}}\n";
        let events = parse(input, SseParseConfig::all());

        // Should still parse usage correctly
        assert!(matches!(
            &events[0],
            Ok(StreamEvent::TokenUsage {
                input_tokens: 100,
                output_tokens: 50
            })
        ));
    }
}
