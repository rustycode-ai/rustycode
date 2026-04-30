use anyhow::Result;
use futures::StreamExt;
use rustycode_llm::provider::{CompletionResponse, StreamChunk};
use rustycode_protocol::stream_event::StreamEvent;
use std::time::Duration;

use crate::session::AgentEvents;

#[derive(Clone, Debug)]
pub struct PendingTool {
    pub id: String,
    pub name: String,
    pub input_json: String,
}

#[derive(Debug)]
pub struct TurnState {
    pub assistant_text: String,
    pub thinking_text: String,
    pub tools: Vec<PendingTool>,
    pub stop_reason: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
}

impl TurnState {
    pub const fn new() -> Self {
        Self {
            assistant_text: String::new(),
            thinking_text: String::new(),
            tools: Vec::new(),
            stop_reason: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_read_tokens: 0,
            total_cache_creation_tokens: 0,
        }
    }
}

pub async fn collect_stream_turn(
    mut stream: std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>,
    chunk_timeout: Duration,
    events: &mut dyn AgentEvents,
) -> Result<TurnState> {
    let mut state = TurnState::new();

    loop {
        let sse = match tokio::time::timeout(chunk_timeout, stream.next()).await {
            Ok(Some(Ok(ev))) => ev,
            Ok(Some(Err(e))) => {
                tracing::warn!("Mid-stream error: {e}. Ending turn early.");
                break;
            }
            Ok(None) => break,
            Err(_) => {
                tracing::warn!("Stream chunk timeout. Ending turn early.");
                break;
            }
        };

        apply_stream_event(&sse, &mut state, events).await;
    }

    Ok(state)
}

pub async fn collect_completion_turn(
    response: CompletionResponse,
    events: &mut dyn AgentEvents,
) -> Result<TurnState> {
    let mut state = TurnState::new();

    state.stop_reason = response.stop_reason.clone();

    if let Some(usage) = &response.usage {
        state.total_output_tokens = state
            .total_output_tokens
            .saturating_add(u64::from(usage.output_tokens));
        state.total_input_tokens = state
            .total_input_tokens
            .saturating_add(u64::from(usage.input_tokens));
        state.total_cache_read_tokens = state
            .total_cache_read_tokens
            .saturating_add(u64::from(usage.cache_read_input_tokens));
        state.total_cache_creation_tokens = state
            .total_cache_creation_tokens
            .saturating_add(u64::from(usage.cache_creation_input_tokens));
        events
            .on_event(StreamEvent::TokenUsage {
                input_tokens: u64::from(usage.input_tokens),
                output_tokens: u64::from(usage.output_tokens),
            })
            .await;
    }

    if let Some(thinking_blocks) = &response.thinking_blocks {
        for block in thinking_blocks {
            if block.thinking.is_empty() {
                continue;
            }
            state.thinking_text.push_str(&block.thinking);
            events
                .on_event(StreamEvent::ThinkingDelta {
                    content: block.thinking.clone(),
                })
                .await;
        }
    }

    // Parse the completion response content (non-streaming path)
    let mut assistant_text = String::new();
    let mut tool_calls = Vec::new();

    // Try to parse as structured content blocks first
    if let Ok(blocks) = serde_json::from_str::<Vec<serde_json::Value>>(&response.content) {
        for block in blocks {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                assistant_text.push_str(text);
            }
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                    let id = block.get("id").and_then(|i| i.as_str()).map(String::from);
                    let input_json = block
                        .get("input")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    tool_calls.push(ParsedToolCall {
                        id,
                        name: name.to_string(),
                        arguments: input_json,
                    });
                }
            }
        }
    } else {
        // Fallback: treat entire content as text
        assistant_text = response.content.clone();
        // Try to parse tool calls from the text if present
        tool_calls = parse_tool_calls(&response.content);
    }
    if !assistant_text.is_empty() {
        state.assistant_text.push_str(&assistant_text);
        events
            .on_event(StreamEvent::TextDelta {
                content: assistant_text,
            })
            .await;
    }

    for call in tool_calls {
        let tool_id = call
            .id
            .clone()
            .unwrap_or_else(|| format!("tool-{}", state.tools.len() + 1));
        let input_json =
            serde_json::to_string(&call.arguments).unwrap_or_else(|_| call.arguments.to_string());

        state.tools.push(PendingTool {
            id: tool_id.clone(),
            name: call.name.clone(),
            input_json: input_json.clone(),
        });
        events
            .on_event(StreamEvent::ToolCallStarted {
                id: tool_id.clone(),
                name: call.name.clone(),
            })
            .await;
        events
            .on_event(StreamEvent::ToolInputDelta {
                id: tool_id,
                chunk: input_json,
            })
            .await;
    }

    Ok(state)
}

async fn apply_stream_event(
    event: &StreamEvent,
    state: &mut TurnState,
    events: &mut dyn AgentEvents,
) {
    match event {
        StreamEvent::TextDelta { content } => {
            state.assistant_text.push_str(content);
            events
                .on_event(StreamEvent::TextDelta {
                    content: content.clone(),
                })
                .await;
        }
        StreamEvent::ThinkingDelta { content } => {
            state.thinking_text.push_str(content);
            events
                .on_event(StreamEvent::ThinkingDelta {
                    content: content.clone(),
                })
                .await;
        }
        StreamEvent::ToolCallStarted { id, name } => {
            state.tools.push(PendingTool {
                id: id.clone(),
                name: name.clone(),
                input_json: String::new(),
            });
            events
                .on_event(StreamEvent::ToolCallStarted {
                    id: id.clone(),
                    name: name.clone(),
                })
                .await;
        }
        StreamEvent::ToolInputDelta { id, chunk } => {
            if let Some(last) = state.tools.last_mut() {
                last.input_json.push_str(chunk);
            }
            events
                .on_event(StreamEvent::ToolInputDelta {
                    id: id.clone(),
                    chunk: chunk.clone(),
                })
                .await;
        }
        StreamEvent::TokenUsage {
            input_tokens,
            output_tokens,
        } => {
            state.total_input_tokens = state.total_input_tokens.saturating_add(*input_tokens);
            state.total_output_tokens = state.total_output_tokens.saturating_add(*output_tokens);
            events
                .on_event(StreamEvent::TokenUsage {
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                })
                .await;
        }
        StreamEvent::TurnCompleted { stop_reason } => {
            state.stop_reason = Some(stop_reason.clone());
        }
        StreamEvent::CacheUsage {
            cache_read_tokens,
            cache_creation_tokens,
        } => {
            state.total_cache_read_tokens = state
                .total_cache_read_tokens
                .saturating_add(*cache_read_tokens);
            state.total_cache_creation_tokens = state
                .total_cache_creation_tokens
                .saturating_add(*cache_creation_tokens);
        }
        _ => {}
    }
}

#[derive(Clone, Debug)]
struct ParsedToolCall {
    id: Option<String>,
    name: String,
    arguments: serde_json::Value,
}

fn parse_tool_calls(content: &str) -> Vec<ParsedToolCall> {
    let trimmed = content.trim();

    if let Some(payload) = extract_marked_tool_payload(trimmed) {
        let calls = parse_tool_payload(payload);
        if !calls.is_empty() {
            return calls;
        }
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let calls = parse_tool_value(&value);
        if !calls.is_empty() {
            return calls;
        }
    }

    Vec::new()
}

fn parse_tool_payload(payload: &str) -> Vec<ParsedToolCall> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
        return parse_tool_value(&value);
    }

    Vec::new()
}

fn parse_tool_value(value: &serde_json::Value) -> Vec<ParsedToolCall> {
    match value {
        serde_json::Value::Array(items) => items.iter().filter_map(parse_tool_item).collect(),
        serde_json::Value::Object(map) => map
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .map_or_else(
                || parse_tool_item(value).map_or_else(Vec::new, |parsed| vec![parsed]),
                |tool_calls| tool_calls.iter().filter_map(parse_tool_item).collect(),
            ),
        _ => Vec::new(),
    }
}

fn parse_tool_item(item: &serde_json::Value) -> Option<ParsedToolCall> {
    if let Some(function) = item.get("function") {
        let name = function.get("name")?.as_str()?.to_string();
        let arguments_str = function
            .get("arguments")
            .and_then(|a| a.as_str())
            .unwrap_or("{}");
        let arguments = serde_json::from_str::<serde_json::Value>(arguments_str)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::default()));
        let id = item
            .get("id")
            .and_then(|i| i.as_str())
            .map(std::string::ToString::to_string);
        return Some(ParsedToolCall {
            id,
            name,
            arguments,
        });
    }

    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
        let raw_arguments = item
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default()));
        let arguments = match &raw_arguments {
            serde_json::Value::String(s) => {
                serde_json::from_str::<serde_json::Value>(s).unwrap_or(raw_arguments)
            }
            _ => raw_arguments,
        };
        let id = item
            .get("id")
            .and_then(|i| i.as_str())
            .map(std::string::ToString::to_string);
        return Some(ParsedToolCall {
            id,
            name: name.to_string(),
            arguments,
        });
    }

    None
}

fn extract_marked_tool_payload(content: &str) -> Option<&str> {
    if let Some(start) = content.find("```tool") {
        let after_start = start + "```tool".len();
        let json_start = if content[after_start..].starts_with('\n')
            || content[after_start..].starts_with(' ')
        {
            after_start + 1
        } else {
            after_start
        };
        let remaining = &content[json_start..];
        let end = remaining.find("```")?;
        let payload = remaining[..end].trim();
        if payload.is_empty() {
            None
        } else {
            Some(payload)
        }
    } else if let Some(start) = content.find("[TOOL_CALLS:") {
        let after_start = start + "[TOOL_CALLS:".len();
        let remaining = &content[after_start..];
        let end = remaining.rfind(']')?;
        let payload = remaining[..end].trim();
        if payload.is_empty() {
            None
        } else {
            Some(payload)
        }
    } else {
        None
    }
}

#[allow(dead_code)]
fn strip_tool_annotations(content: &str, has_tool_calls: bool) -> String {
    let trimmed = content.trim();
    if has_tool_calls && (trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return String::new();
    }

    let mut cleaned = content.to_string();

    if let Some(start) = cleaned.find("```tool") {
        let after_marker = start + "```tool".len();
        if let Some(end_rel) = cleaned[after_marker..].find("```") {
            let end = after_marker + end_rel + "```".len();
            cleaned.replace_range(start..end, "");
        }
    }

    if let Some(start) = cleaned.find("[TOOL_CALLS:") {
        if let Some(end_rel) = cleaned[start..].rfind(']') {
            let end = start + end_rel + 1;
            cleaned.replace_range(start..end, "");
        }
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rustycode_protocol::stream_event::ApprovalDecision;

    struct TestEventCollector {
        events: Vec<StreamEvent>,
    }

    impl TestEventCollector {
        fn new() -> Self {
            Self { events: Vec::new() }
        }
    }

    #[async_trait]
    impl AgentEvents for TestEventCollector {
        async fn on_event(&mut self, event: StreamEvent) {
            self.events.push(event);
        }

        async fn on_approval_needed(
            &mut self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> ApprovalDecision {
            ApprovalDecision::AutoApproved
        }

        async fn on_question(&mut self, _question: &str, _options: &[String]) -> Option<String> {
            None
        }

        async fn on_done(&mut self, _result: &crate::session::AgentResult) {}
    }

    #[tokio::test]
    async fn text_delta_accumulates_and_forwards() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let event = StreamEvent::TextDelta {
            content: "Hello".to_string(),
        };

        apply_stream_event(&event, &mut state, &mut collector).await;

        assert_eq!(state.assistant_text, "Hello");
        assert_eq!(collector.events.len(), 1);
        match &collector.events[0] {
            StreamEvent::TextDelta { content } => assert_eq!(content, "Hello"),
            _ => panic!("Expected TextDelta event"),
        }
    }

    #[tokio::test]
    async fn consecutive_text_deltas_accumulate() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let events = vec![
            StreamEvent::TextDelta {
                content: "hel".to_string(),
            },
            StreamEvent::TextDelta {
                content: "lo".to_string(),
            },
        ];

        for event in events {
            apply_stream_event(&event, &mut state, &mut collector).await;
        }

        assert_eq!(state.assistant_text, "hello");
        assert_eq!(collector.events.len(), 2);
        match &collector.events[0] {
            StreamEvent::TextDelta { content } => assert_eq!(content, "hel"),
            _ => panic!("Expected TextDelta event"),
        }
        match &collector.events[1] {
            StreamEvent::TextDelta { content } => assert_eq!(content, "lo"),
            _ => panic!("Expected TextDelta event"),
        }
    }

    #[tokio::test]
    async fn consecutive_identical_text_deltas_all_preserved() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let events = vec![
            StreamEvent::TextDelta {
                content: "A".to_string(),
            },
            StreamEvent::TextDelta {
                content: "A".to_string(),
            },
            StreamEvent::TextDelta {
                content: "A".to_string(),
            },
        ];

        for event in events {
            apply_stream_event(&event, &mut state, &mut collector).await;
        }

        assert_eq!(state.assistant_text, "AAA");
        assert_eq!(collector.events.len(), 3);
        for event in &collector.events {
            match event {
                StreamEvent::TextDelta { content } => assert_eq!(content, "A"),
                _ => panic!("Expected TextDelta event"),
            }
        }
    }

    #[tokio::test]
    async fn tool_call_started_creates_pending_tool() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let event = StreamEvent::ToolCallStarted {
            id: "t1".to_string(),
            name: "read_file".to_string(),
        };

        apply_stream_event(&event, &mut state, &mut collector).await;

        assert_eq!(state.tools.len(), 1);
        assert_eq!(state.tools[0].id, "t1");
        assert_eq!(state.tools[0].name, "read_file");
        assert!(state.tools[0].input_json.is_empty());
        assert_eq!(collector.events.len(), 1);
        match &collector.events[0] {
            StreamEvent::ToolCallStarted { id, name } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "read_file");
            }
            _ => panic!("Expected ToolCallStarted event"),
        }
    }

    #[tokio::test]
    async fn tool_input_delta_appends_to_last_tool() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let start = StreamEvent::ToolCallStarted {
            id: "t1".to_string(),
            name: "bash".to_string(),
        };
        apply_stream_event(&start, &mut state, &mut collector).await;

        let delta = StreamEvent::ToolInputDelta {
            id: "t1".to_string(),
            chunk: r#"{"command":"ls"}"#.to_string(),
        };
        apply_stream_event(&delta, &mut state, &mut collector).await;

        assert_eq!(state.tools[0].input_json, r#"{"command":"ls"}"#);
    }

    #[tokio::test]
    async fn turn_completed_sets_stop_reason() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let event = StreamEvent::TurnCompleted {
            stop_reason: "tool_use".to_string(),
        };
        apply_stream_event(&event, &mut state, &mut collector).await;

        assert_eq!(state.stop_reason, Some("tool_use".to_string()));
        // TurnCompleted is not forwarded to collector
        assert!(collector.events.is_empty());
    }

    #[tokio::test]
    async fn token_usage_updates_counters() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let event = StreamEvent::TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        apply_stream_event(&event, &mut state, &mut collector).await;

        assert_eq!(state.total_input_tokens, 100);
        assert_eq!(state.total_output_tokens, 50);
        assert_eq!(collector.events.len(), 1);
    }

    #[tokio::test]
    async fn cache_usage_updates_counters() {
        let mut state = TurnState::new();
        let mut collector = TestEventCollector::new();

        let event = StreamEvent::CacheUsage {
            cache_read_tokens: 500,
            cache_creation_tokens: 200,
        };
        apply_stream_event(&event, &mut state, &mut collector).await;

        assert_eq!(state.total_cache_read_tokens, 500);
        assert_eq!(state.total_cache_creation_tokens, 200);
        // CacheUsage is not forwarded to collector
        assert!(collector.events.is_empty());
    }

    #[tokio::test]
    async fn full_stream_turn_e2e() {
        let events: Vec<Result<StreamEvent, rustycode_llm::provider::ProviderError>> = vec![
            Ok(StreamEvent::TextDelta {
                content: "Hello".into(),
            }),
            Ok(StreamEvent::ToolCallStarted {
                id: "t1".into(),
                name: "bash".into(),
            }),
            Ok(StreamEvent::ToolInputDelta {
                id: "t1".into(),
                chunk: r#"{"command":"ls"}"#.into(),
            }),
            Ok(StreamEvent::TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            }),
            Ok(StreamEvent::TurnCompleted {
                stop_reason: "tool_use".into(),
            }),
        ];
        let stream = futures::stream::iter(events);
        let mut collector = TestEventCollector::new();
        let state = collect_stream_turn(Box::pin(stream), Duration::from_secs(30), &mut collector)
            .await
            .unwrap();

        assert_eq!(state.assistant_text, "Hello");
        assert_eq!(state.tools.len(), 1);
        assert_eq!(state.tools[0].name, "bash");
        assert_eq!(state.tools[0].input_json, r#"{"command":"ls"}"#);
        assert_eq!(state.stop_reason, Some("tool_use".to_string()));
        assert_eq!(state.total_input_tokens, 100);
        assert_eq!(state.total_output_tokens, 50);
    }

    #[test]
    fn strip_tool_annotations_removes_tool_code_block() {
        let input = "Here is the plan ```tool\n{\"name\":\"read_file\"}\n``` done";
        let result = strip_tool_annotations(input, true);
        assert_eq!(result, "Here is the plan  done");
    }

    #[test]
    fn strip_tool_annotations_removes_tool_code_block_only() {
        let input = "```tool\n{\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"}}\n```";
        let result = strip_tool_annotations(input, true);
        assert_eq!(result, "");
    }

    #[test]
    fn strip_tool_annotations_preserves_text_when_no_tools() {
        let input = "Just some regular text with ```rust\ncode\n``` blocks";
        let result = strip_tool_annotations(input, false);
        assert_eq!(result, input.trim());
    }

    #[test]
    fn strip_tool_annotations_removes_tool_calls_bracket() {
        let input = "text before [TOOL_CALLS: {\"name\":\"read\"}] text after";
        let result = strip_tool_annotations(input, true);
        assert_eq!(result, "text before  text after");
    }

    #[test]
    fn strip_tool_annotations_removes_both_formats() {
        let input = "```tool\n{\"name\":\"read\"}\n``` and [TOOL_CALLS: {\"name\":\"write\"}] done";
        let result = strip_tool_annotations(input, true);
        assert_eq!(result, "and  done");
    }

    #[test]
    fn token_counters_use_saturating_add() {
        let mut state = TurnState::new();
        state.total_input_tokens = u64::MAX - 10;
        // This would panic with +=, but saturating_add clamps to u64::MAX
        state.total_input_tokens = state.total_input_tokens.saturating_add(100);
        assert_eq!(state.total_input_tokens, u64::MAX);
    }
}
