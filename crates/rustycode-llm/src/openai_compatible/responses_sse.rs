//! SSE stream parsing for the OpenAI Responses API.
//!
//! The Responses API uses typed SSE events (e.g. `response.output_text.delta`,
//! `response.function_call_arguments.delta`) instead of the Chat Completions
//! `choices[0].delta` format. This parser translates those events into
//! protocol-level `StreamEvent` values.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rustycode_protocol::stream_event::StreamEvent;

use crate::provider::{normalize_stop_reason, ProviderError};

/// State tracked across SSE lines for a single Responses API stream.
///
/// Tracks `call_id` → `(id, name)` mappings so `ToolInputDelta` events can
/// reference the correct tool call even when later chunks only contain the
/// `arguments` delta without repeating the name.
#[derive(Debug, Default)]
pub struct ResponsesSseState {
    /// `call_id` → `(item_id, tool_name)`
    calls: Arc<Mutex<HashMap<String, (String, String)>>>,
}

impl Clone for ResponsesSseState {
    fn clone(&self) -> Self {
        Self {
            calls: Arc::clone(&self.calls),
        }
    }
}

impl ResponsesSseState {
    /// Record a new tool call when `output_item.added` fires.
    pub fn register_call(&self, call_id: String, item_id: String, name: String) {
        let mut map = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(call_id, (item_id, name));
    }

    /// Look up the tool name for a given `call_id`.
    pub fn name(&self, call_id: &str) -> Option<String> {
        let map = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        map.get(call_id).map(|(_, name)| name.clone())
    }
}

/// Parse SSE lines from a Responses API stream into `StreamEvent` values.
///
/// Callers should feed complete lines (e.g. from `SseByteBuffer::feed_chunk`)
/// joined by `\n`. The Responses API sends events like:
///
/// ```text
/// event: response.output_text.delta
/// data: {"type":"response.output_text.delta","delta":"Hello"}
///
/// event: response.function_call_arguments.delta
/// data: {"type":"response.function_call_arguments.delta","call_id":"call_1","delta":"{\""}
/// ```
///
pub fn parse_responses_sse_lines(
    lines: &str,
    state: &ResponsesSseState,
) -> Vec<Result<StreamEvent, ProviderError>> {
    let mut events = Vec::new();
    let mut current_event_type: Option<String> = None;

    for line in lines.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            current_event_type = None;
            continue;
        }

        // Track event type
        if let Some(evt_type) = line.strip_prefix("event: ") {
            current_event_type = Some(evt_type.trim().to_string());
            continue;
        }

        // Only process `data:` lines
        let data_str = if let Some(d) = line.strip_prefix("data: ") {
            d.trim()
        } else if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
            // Non-SSE raw JSON (some providers send errors this way)
            if let Some(err) = extract_responses_error(&data) {
                events.push(err);
            }
            current_event_type = None;
            continue;
        } else {
            continue;
        };

        if data_str == "[DONE]" {
            events.push(Ok(StreamEvent::Done));
            current_event_type = None;
            continue;
        }

        let data = match serde_json::from_str::<serde_json::Value>(data_str) {
            Ok(d) => d,
            Err(_) => {
                current_event_type = None;
                continue;
            }
        };

        let evt_type = match (
            &current_event_type,
            data.get("type").and_then(|t| t.as_str()),
        ) {
            (Some(t), _) => t.clone(),
            (_, Some(t)) => t.to_string(),
            _ => {
                current_event_type = None;
                continue;
            }
        };

        events.extend(dispatch_responses_event(&data, &evt_type, state));
        current_event_type = None;
    }

    events
}

/// Dispatch a single Responses API event (by type name + JSON payload) into stream events.
///
/// Shared by both SSE and WebSocket transports since the event JSON shapes are identical.
pub fn dispatch_responses_event(
    data: &serde_json::Value,
    evt_type: &str,
    state: &ResponsesSseState,
) -> Vec<Result<StreamEvent, ProviderError>> {
    let mut events = Vec::new();

    match evt_type {
        // Reasoning streaming
        "response.reasoning.delta" => {
            if let Some(delta) = data.get("delta") {
                if let Some(summary) = delta.get("summary").and_then(|s| s.as_array()) {
                    for part in summary {
                        if part.get("type").and_then(|t| t.as_str()) == Some("summary_text") {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                if !text.is_empty() {
                                    events.push(Ok(StreamEvent::ThinkingDelta {
                                        content: text.to_string(),
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Reasoning completed
        "response.reasoning.done" => {
            let reasoning_data = data.get("data").unwrap_or(data);
            let encrypted = reasoning_data
                .get("encrypted_content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !encrypted.is_empty() {
                events.push(Ok(StreamEvent::ThinkingBlockCompleted {
                    block_type: "reasoning".to_string(),
                    signature: String::new(),
                    data: encrypted.to_string(),
                }));
            }
        }

        // Text streaming
        "response.output_text.delta" => {
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                if !delta.is_empty() {
                    events.push(Ok(StreamEvent::TextDelta {
                        content: delta.to_string(),
                    }));
                }
            }
        }

        // Tool call started
        "response.output_item.added" => {
            if let Some(item) = data.get("item") {
                if item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                    let call_id = item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let item_id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    if call_id.is_empty() {
                        tracing::warn!("skipping tool call with empty call_id in SSE stream");
                    } else {
                        state.register_call(call_id.clone(), item_id, name.clone());

                        events.push(Ok(StreamEvent::ToolCallStarted { id: call_id, name }));
                    }
                } else {
                    // Message item added — treat as turn start
                    events.push(Ok(StreamEvent::TurnStarted { turn: 0 }));
                }
            }
        }

        // Tool call argument streaming
        "response.function_call_arguments.delta" => {
            let call_id = data
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let delta = data.get("delta").and_then(|v| v.as_str()).unwrap_or("");

            if !call_id.is_empty() && !delta.is_empty() {
                events.push(Ok(StreamEvent::ToolInputDelta {
                    id: call_id,
                    chunk: delta.to_string(),
                }));
            }
        }

        // Tool call arguments complete — no event needed, deltas already sent
        "response.function_call_arguments.done" => {}

        // Text done — no event needed, deltas already sent
        "response.output_text.done" | "response.content_part.done" => {}

        // Response completed
        "response.completed" => {
            if let Some(response) = data.get("response") {
                // Extract usage
                if let Some(usage) = response.get("usage") {
                    let input = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let output = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    events.push(Ok(StreamEvent::TokenUsage {
                        input_tokens: input,
                        output_tokens: output,
                    }));
                }

                let status = response
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed");

                let reason = match status {
                    "completed" => Some("stop"),
                    "incomplete" => Some("max_tokens"),
                    "failed" => Some("error"),
                    _ => Some("stop"),
                };
                events.push(Ok(StreamEvent::TurnCompleted {
                    stop_reason: normalize_stop_reason(reason)
                        .unwrap_or_else(|| "end_turn".to_string()),
                }));
            }
        }

        // Informational events — skip
        "response.created"
        | "response.in_progress"
        | "response.output_item.done"
        | "response.content_part.added" => {}

        // Error
        "error" => {
            if let Some(err) = extract_responses_error(data) {
                events.push(err);
            }
        }

        // Unknown event type — ignore
        _ => {}
    }

    events
}

/// Extract an error from a Responses API event payload.
pub fn extract_responses_error(
    data: &serde_json::Value,
) -> Option<Result<StreamEvent, ProviderError>> {
    // Responses API errors: {"type":"error","code":"...","message":"..."}
    // Or nested: {"error":{"type":"...","code":"...","message":"..."}}
    let err_obj = data.get("error").cloned().unwrap_or_else(|| data.clone());

    if err_obj.get("type").and_then(|t| t.as_str()) == Some("error")
        || err_obj.get("code").is_some()
        || err_obj.get("message").is_some()
    {
        let code = err_obj
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let message = err_obj
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        Some(Err(ProviderError::Api(format!("[{}] {}", code, message))))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_delta() {
        let input = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::TextDelta { content }) => assert_eq!(content, "Hello"),
            other => panic!("expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_call_started() {
        let input = "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"Read\"}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ToolCallStarted { id, name }) => {
                assert_eq!(id, "call_1");
                assert_eq!(name, "Read");
            }
            other => panic!("expected ToolCallStarted, got {:?}", other),
        }
    }

    #[test]
    fn parse_tool_argument_delta_with_state() {
        let state = ResponsesSseState::default();
        state.register_call("call_1".to_string(), "fc_1".to_string(), "Read".to_string());

        let input = "event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"call_id\":\"call_1\",\"delta\":\"{\\\"path\\\":\"}\n\n";
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ToolInputDelta { id, chunk }) => {
                assert_eq!(id, "call_1");
                assert!(chunk.contains("path"));
            }
            other => panic!("expected ToolInputDelta, got {:?}", other),
        }
    }

    #[test]
    fn parse_response_completed() {
        let input = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 2); // TokenUsage + TurnCompleted

        match &events[0] {
            Ok(StreamEvent::TokenUsage {
                input_tokens,
                output_tokens,
            }) => {
                assert_eq!(*input_tokens, 10);
                assert_eq!(*output_tokens, 5);
            }
            other => panic!("expected TokenUsage, got {:?}", other),
        }
        match &events[1] {
            Ok(StreamEvent::TurnCompleted { stop_reason }) => {
                assert_eq!(stop_reason, "end_turn");
            }
            other => panic!("expected TurnCompleted, got {:?}", other),
        }
    }

    #[test]
    fn parse_done_signal() {
        let input = "data: [DONE]\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Ok(StreamEvent::Done)));
    }

    #[test]
    fn parse_error_event() {
        let input = "event: error\ndata: {\"type\":\"error\",\"code\":\"rate_limit\",\"message\":\"Too many requests\"}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Err(ProviderError::Api(msg)) => {
                assert!(msg.contains("rate_limit"));
            }
            other => panic!("expected Api error, got {:?}", other),
        }
    }

    #[test]
    fn parse_full_conversation_flow() {
        let input = "\
event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n\
event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n\
event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\n\
event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\" there\"}\n\n\
event: response.output_text.done\ndata: {\"type\":\"response.output_text.done\",\"text\":\"Hi there\"}\n\n\
event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":5,\"output_tokens\":2}}}\n\n\
data: [DONE]\n\n";

        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);

        // TurnStarted + 2x TextDelta + TokenUsage + TurnCompleted + Done = 6
        assert_eq!(events.len(), 6);
    }

    #[test]
    fn state_tracks_multiple_calls() {
        let state = ResponsesSseState::default();
        state.register_call("c1".into(), "fc_1".into(), "tool_a".into());
        state.register_call("c2".into(), "fc_2".into(), "tool_b".into());

        assert_eq!(state.name("c1"), Some("tool_a".to_string()));
        assert_eq!(state.name("c2"), Some("tool_b".to_string()));
        assert_eq!(state.name("c3"), None);
    }

    #[test]
    fn incomplete_status_maps_to_max_tokens() {
        let input = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"incomplete\"}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        match &events[0] {
            Ok(StreamEvent::TurnCompleted { stop_reason }) => {
                assert_eq!(stop_reason, "max_tokens");
            }
            other => panic!("expected TurnCompleted, got {:?}", other),
        }
    }

    #[test]
    fn parse_reasoning_delta_emits_thinking_delta() {
        let input = "event: response.reasoning.delta\ndata: {\"type\":\"response.reasoning.delta\",\"delta\":{\"id\":\"rs_abc123\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"Let me think about\"}]}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ThinkingDelta { content }) => {
                assert_eq!(content, "Let me think about");
            }
            other => panic!("expected ThinkingDelta, got {:?}", other),
        }
    }

    #[test]
    fn parse_reasoning_delta_multiple_summary_parts() {
        let input = "event: response.reasoning.delta\ndata: {\"type\":\"response.reasoning.delta\",\"delta\":{\"id\":\"rs_abc123\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"First part\"},{\"type\":\"summary_text\",\"text\":\" second part\"}]}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 2);
        match &events[0] {
            Ok(StreamEvent::ThinkingDelta { content }) => {
                assert_eq!(content, "First part");
            }
            other => panic!("expected first ThinkingDelta, got {:?}", other),
        }
        match &events[1] {
            Ok(StreamEvent::ThinkingDelta { content }) => {
                assert_eq!(content, " second part");
            }
            other => panic!("expected second ThinkingDelta, got {:?}", other),
        }
    }

    #[test]
    fn parse_reasoning_delta_skips_empty_text() {
        let input = "event: response.reasoning.delta\ndata: {\"type\":\"response.reasoning.delta\",\"delta\":{\"id\":\"rs_abc123\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"\"}]}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert!(events.is_empty());
    }

    #[test]
    fn parse_reasoning_done_emits_thinking_block_completed() {
        let input = "event: response.reasoning.done\ndata: {\"type\":\"response.reasoning.done\",\"data\":{\"id\":\"rs_abc123\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"full reasoning\"}],\"encrypted_content\":\"enc_abc123xyz\"}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ThinkingBlockCompleted {
                block_type,
                signature,
                data,
            }) => {
                assert_eq!(block_type, "reasoning");
                assert!(signature.is_empty());
                assert_eq!(data, "enc_abc123xyz");
            }
            other => panic!("expected ThinkingBlockCompleted, got {:?}", other),
        }
    }

    #[test]
    fn parse_reasoning_done_skips_empty_encrypted_content() {
        let input = "event: response.reasoning.done\ndata: {\"type\":\"response.reasoning.done\",\"data\":{\"id\":\"rs_abc123\",\"summary\":[]}}\n\n";
        let state = ResponsesSseState::default();
        let events = parse_responses_sse_lines(input, &state);
        assert!(events.is_empty());
    }
}
