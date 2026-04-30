//! SSE stream parsing for OpenAI-compatible providers

use super::types::OpenAiCompatibleUsage;
use crate::provider::ProviderError;
use rustycode_protocol::stream_event::StreamEvent;
use serde_json::Value;

/// Configuration for SSE parsing behavior.
///
/// Controls which features to extract from SSE events (tool calls, thinking,
/// usage, etc.).
#[derive(Debug, Clone, Copy, Default)]
pub struct SseParseConfig {
    /// Enable extraction of tool call deltas
    pub enable_tool_calls: bool,
    /// Enable extraction of thinking/reasoning content
    pub enable_thinking: bool,
    /// Enable extraction of usage information
    pub enable_usage: bool,
    /// Enable extraction of refusal content
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

    /// Minimal config (text only).
    pub fn minimal() -> Self {
        Self::default()
    }
}

/// Parse SSE lines from an OpenAI-compatible stream.
///
/// # Arguments
/// * `text` - The raw SSE text (may contain multiple lines)
/// * `config` - Parsing configuration
///
/// # Returns
/// A vector of SSE events extracted from the lines.
///
/// # Example
/// ```rust,ignore
/// let events = parse_openai_sse_lines(&text, SseParseConfig::minimal());
/// ```
pub fn parse_openai_sse_lines(
    text: &str,
    config: SseParseConfig,
) -> Vec<Result<StreamEvent, ProviderError>> {
    let mut events = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            continue;
        }

        // Skip non-data lines (e.g., "event: message", "id: ...")
        if !line.starts_with("data: ") {
            continue;
        }

        let json_str = line.trim_start_matches("data: ").trim();

        // Handle stream termination
        if json_str == "[DONE]" {
            continue;
        }

        // Parse the JSON payload
        let data: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => continue, // Skip malformed JSON lines
        };

        // Extract content from choices[0].delta
        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
            if let Some(choice) = choices.first() {
                if let Some(delta) = choice.get("delta") {
                    // Extract text content
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            events.push(Ok(StreamEvent::TextDelta {
                                content: content.to_string(),
                            }));
                        }
                    }

                    // Extract tool call deltas
                    if config.enable_tool_calls {
                        if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array())
                        {
                            for tc in tool_calls {
                                if let Some(function) = tc.get("function") {
                                    if let Some(args) =
                                        function.get("arguments").and_then(|a| a.as_str())
                                    {
                                        events.push(Ok(StreamEvent::TextDelta {
                                            content: args.to_string(),
                                        }));
                                    }
                                }
                            }
                        }
                    }

                    // Extract thinking/reasoning content
                    if config.enable_thinking {
                        if let Some(reasoning) =
                            delta.get("reasoning_content").and_then(|r| r.as_str())
                        {
                            events.push(Ok(StreamEvent::ThinkingDelta {
                                content: reasoning.to_string(),
                            }));
                        }
                    }

                    // Extract refusal
                    if config.enable_refusal {
                        if let Some(refusal) = delta.get("refusal").and_then(|r| r.as_str()) {
                            events.push(Ok(StreamEvent::TextDelta {
                                content: refusal.to_string(),
                            }));
                        }
                    }
                }

                // Extract finish reason
                if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    events.push(Ok(StreamEvent::TurnCompleted {
                        stop_reason: finish_reason.to_string(),
                    }));
                }
            }
        }

        // Extract usage information
        if config.enable_usage {
            if let Some(usage) = data.get("usage") {
                if let Ok(parsed) = serde_json::from_value::<OpenAiCompatibleUsage>(usage.clone()) {
                    events.push(Ok(StreamEvent::TokenUsage {
                        input_tokens: u64::from(parsed.prompt_tokens),
                        output_tokens: u64::from(parsed.completion_tokens),
                    }));
                    let cached = parsed
                        .prompt_tokens_details
                        .as_ref()
                        .and_then(|d| d.cached_tokens)
                        .unwrap_or(0);
                    if cached > 0 {
                        events.push(Ok(StreamEvent::CacheUsage {
                            cache_read_tokens: u64::from(cached),
                            cache_creation_tokens: 0,
                        }));
                    }
                }
            }
        }
    }

    events
}

/// Aggregate SSE text events into a single content string.
///
/// Filters out non-text events and concatenates text content.
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
