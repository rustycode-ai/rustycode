//! OpenAI SSE stream parsing logic.
//!
//! Contains two parallel parsing paths:
//! - `parse_sse_lines` → `SSEEvent` (legacy TUI path)
//! - `parse_sse_lines_stream_events` → `StreamEvent` (runtime stream path)

use crate::provider::{ProviderError, SSEEvent, Usage};
use rustycode_protocol::stream_event::StreamEvent;
use std::collections::{HashMap, HashSet};

/// Parse SSE-formatted lines into SSEEvent results.
///
/// This is the core parsing logic for the legacy TUI streaming path,
/// extracted so it can be tested independently without network calls.
#[allow(dead_code)]
pub fn parse_sse_lines(lines: &str) -> Vec<Result<SSEEvent, ProviderError>> {
    let mut events = Vec::new();
    let mut seen_tool_indices: HashSet<usize> = HashSet::new();
    for line in lines.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line.starts_with("data: ") {
            let json_str = line.trim_start_matches("data: ").trim();
            if json_str == "[DONE]" {
                events.push(Ok(SSEEvent::MessageStop));
                continue;
            }
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                    if let Some(choice) = choices.first() {
                        // Handle content delta (text streaming)
                        if let Some(delta) = choice.get("delta") {
                            if let Some(content) = delta.get("content") {
                                if let Some(content_str) = content.as_str() {
                                    if !content_str.is_empty() {
                                        events.push(Ok(SSEEvent::ContentBlockDelta {
                                            index: 0,
                                            delta: crate::provider::ContentDelta::Text {
                                                text: content_str.to_string(),
                                            },
                                        }));
                                    }
                                }
                            }

                            // Handle reasoning content (GLM-5 via Z.AI official API)
                            if let Some(reasoning) = delta.get("reasoning_content") {
                                if let Some(reasoning_str) = reasoning.as_str() {
                                    if !reasoning_str.is_empty() {
                                        events.push(Ok(SSEEvent::ThinkingDelta {
                                            thinking: reasoning_str.to_string(),
                                        }));
                                    }
                                }
                            }

                            // Handle reasoning content (GLM-5 via vLLM uses "reasoning" key)
                            if let Some(reasoning) = delta.get("reasoning") {
                                if let Some(reasoning_str) = reasoning.as_str() {
                                    if !reasoning_str.is_empty() {
                                        events.push(Ok(SSEEvent::ThinkingDelta {
                                            thinking: reasoning_str.to_string(),
                                        }));
                                    }
                                }
                            }

                            // Handle tool call deltas
                            if let Some(tool_calls) =
                                delta.get("tool_calls").and_then(|tc| tc.as_array())
                            {
                                for tc_delta in tool_calls {
                                    let index =
                                        tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                            as usize;

                                    // Check for tool call start (has id and function.name)
                                    if let Some(id) = tc_delta.get("id").and_then(|i| i.as_str()) {
                                        let name = tc_delta
                                            .get("function")
                                            .and_then(|f| f.get("name"))
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        seen_tool_indices.insert(index);
                                        events.push(Ok(SSEEvent::ContentBlockStart {
                                            index,
                                            content_block:
                                                crate::provider::ContentBlockType::ToolUse {
                                                    id: id.to_string(),
                                                    name,
                                                    input: None,
                                                },
                                        }));
                                    }

                                    // Check for partial function arguments
                                    if let Some(partial) = tc_delta
                                        .get("function")
                                        .and_then(|f| f.get("arguments"))
                                        .and_then(|a| a.as_str())
                                    {
                                        if !partial.is_empty() {
                                            events.push(Ok(SSEEvent::ContentBlockDelta {
                                                index,
                                                delta: crate::provider::ContentDelta::PartialJson {
                                                    partial_json: partial.to_string(),
                                                },
                                            }));
                                        }
                                    }
                                }
                            }
                        }

                        // Handle finish_reason
                        if let Some(finish_reason) =
                            choice.get("finish_reason").and_then(|f| f.as_str())
                        {
                            if finish_reason == "tool_calls" || finish_reason == "stop" {
                                for &idx in &seen_tool_indices {
                                    events.push(Ok(SSEEvent::ContentBlockStop { index: idx }));
                                }
                                if seen_tool_indices.is_empty() {
                                    events.push(Ok(SSEEvent::ContentBlockStop { index: 0 }));
                                }
                            }

                            let usage = data.get("usage").and_then(|u| {
                                let input_tokens = u.get("prompt_tokens")?.as_u64()? as u32;
                                let output_tokens =
                                    u.get("completion_tokens")?.as_u64()? as u32;
                                let cached_tokens: u32 = u
                                    .get("prompt_tokens_details")
                                    .and_then(|d| d.get("cached_tokens"))
                                    .and_then(|t| t.as_u64())
                                    .unwrap_or(0) as u32;
                                let reasoning_tokens: u32 = u
                                    .get("completion_tokens_details")
                                    .and_then(|d| d.get("reasoning_tokens"))
                                    .and_then(|t| t.as_u64())
                                    .unwrap_or(0) as u32;
                                if cached_tokens > 0 {
                                    let hit_pct = (cached_tokens * 100).checked_div(input_tokens).unwrap_or(0);
                                    tracing::info!(
                                        "Cache: {hit_pct}% hit ({cached_tokens}/{input_tokens} prompt tokens)"
                                    );
                                }
                                tracing::info!(
                                    input_tokens,
                                    output_tokens,
                                    reasoning_tokens,
                                    total = input_tokens.saturating_add(output_tokens).saturating_add(reasoning_tokens),
                                    "Usage breakdown (streaming)"
                                );
                                Some(Usage {
                                    input_tokens,
                                    output_tokens,
                                    total_tokens: input_tokens.saturating_add(output_tokens).saturating_add(reasoning_tokens),
                                    cache_read_input_tokens: cached_tokens,
                                    cache_creation_input_tokens: 0,
                                    reasoning_tokens: if reasoning_tokens > 0 { Some(reasoning_tokens) } else { None },
                                })
                            });

                            events.push(Ok(SSEEvent::MessageDelta {
                                stop_reason: crate::provider::normalize_stop_reason(Some(
                                    finish_reason,
                                )),
                                usage,
                            }));
                        }
                    }
                } else if let Some(error) = data.get("error") {
                    let code = error
                        .get("code")
                        .and_then(|c| c.as_str())
                        .unwrap_or("unknown");
                    let message = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown streaming error");
                    events.push(Err(ProviderError::api(format!("{}: {}", code, message))));
                }
            }
        } else if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(error) = data.get("error") {
                let code = error
                    .get("code")
                    .and_then(|c| c.as_str())
                    .unwrap_or("unknown");
                let message = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown streaming error");
                events.push(Err(ProviderError::api(format!("{}: {}", code, message))));
            }
        }
    }

    if events.is_empty() {
        events.push(Ok(SSEEvent::text(String::new())));
    }

    events
}

/// Parse SSE-formatted lines into StreamEvent results for the runtime stream path.
pub fn parse_sse_lines_stream_events(lines: &str) -> Vec<Result<StreamEvent, ProviderError>> {
    let mut events = Vec::new();
    let mut tool_ids_by_index: HashMap<usize, String> = HashMap::new();

    for line in lines.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line.starts_with("data: ") {
            let json_str = line.trim_start_matches("data: ").trim();
            if json_str == "[DONE]" {
                events.push(Ok(StreamEvent::Done));
                continue;
            }
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                dispatch_data_payload(&data, &mut events, &mut tool_ids_by_index);
            }
        } else if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(err) = extract_stream_error(&data) {
                events.push(err);
            }
        }
    }

    if events.is_empty() {
        events.push(Ok(StreamEvent::Done));
    }
    events
}

/// Route a parsed JSON data payload: choices -> delta/finish, or top-level error.
fn dispatch_data_payload(
    data: &serde_json::Value,
    events: &mut Vec<Result<StreamEvent, ProviderError>>,
    tool_ids_by_index: &mut HashMap<usize, String>,
) {
    if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(delta) = choice.get("delta") {
                extract_delta_events(delta, events, tool_ids_by_index);
            }
            if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                extract_finish_events(data, finish_reason, events);
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
    tool_ids_by_index: &mut HashMap<usize, String>,
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

    // Tool call deltas
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
        for tc_delta in tool_calls {
            extract_tool_call_delta(tc_delta, events, tool_ids_by_index);
        }
    }
}

/// Handle a single tool call delta entry within the tool_calls array.
fn extract_tool_call_delta(
    tc_delta: &serde_json::Value,
    events: &mut Vec<Result<StreamEvent, ProviderError>>,
    tool_ids_by_index: &mut HashMap<usize, String>,
) {
    let index = tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

    let resolved_id = tc_delta
        .get("id")
        .and_then(|i| i.as_str())
        .map(ToString::to_string)
        .or_else(|| tool_ids_by_index.get(&index).cloned());

    // Tool call start (id present)
    if let Some(id) = tc_delta.get("id").and_then(|i| i.as_str()) {
        tool_ids_by_index.insert(index, id.to_string());
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
) {
    if let Some(usage) = data.get("usage").and_then(parse_usage) {
        events.push(Ok(StreamEvent::TokenUsage {
            input_tokens: u64::from(usage.input_tokens),
            output_tokens: u64::from(usage.output_tokens),
        }));
    }

    let stop_reason = crate::provider::normalize_stop_reason(Some(finish_reason))
        .unwrap_or_else(|| finish_reason.to_string());
    events.push(Ok(StreamEvent::TurnCompleted { stop_reason }));
}

/// Parse a usage JSON object into a [`Usage`] struct, logging cache hit percentage.
fn parse_usage(u: &serde_json::Value) -> Option<Usage> {
    let input_tokens = u.get("prompt_tokens")?.as_u64()? as u32;
    let output_tokens = u.get("completion_tokens")?.as_u64()? as u32;
    let cached_tokens: u32 = u
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;

    // Parse reasoning tokens from completion_tokens_details
    let reasoning_tokens: u32 = u
        .get("completion_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;

    if cached_tokens > 0 {
        let hit_pct = (cached_tokens * 100).checked_div(input_tokens).unwrap_or(0);
        tracing::info!("Cache: {hit_pct}% hit ({cached_tokens}/{input_tokens} prompt tokens)");
    }
    tracing::info!(
        input_tokens,
        output_tokens,
        reasoning_tokens,
        total = input_tokens
            .saturating_add(output_tokens)
            .saturating_add(reasoning_tokens),
        "Usage breakdown"
    );
    Some(Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens
            .saturating_add(output_tokens)
            .saturating_add(reasoning_tokens),
        cache_read_input_tokens: cached_tokens,
        cache_creation_input_tokens: 0,
        reasoning_tokens: if reasoning_tokens > 0 {
            Some(reasoning_tokens)
        } else {
            None
        },
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
