//! Wire protocol for Ollama `/api/chat` format.
//!
//! Ollama uses a distinct wire format from OpenAI:
//! - Parameters go in an `options` object (not top-level)
//! - Messages use `{ role, content, images }` (flat content, separate images array)
//! - Response uses `{ message, done, prompt_eval_count, eval_count }` (no `choices[]`)
//! - Streaming is NDJSON (newline-delimited JSON), not SSE

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

/// Ollama `/api/chat` wire format protocol.
///
/// Handles the differences between Ollama and OpenAI:
/// - `options` object for generation parameters
/// - Flat message content with `images` array for vision
/// - `done` flag instead of `choices[].finish_reason`
/// - NDJSON streaming (each line is a complete JSON object)
pub struct OllamaChatProtocol;

impl Protocol for OllamaChatProtocol {
    fn format(&self) -> WireFormat {
        // Ollama tool schemas are OpenAI-compatible; reuse the format tag
        WireFormat::OpenAIChat
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        Box::new(Self)
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let mut messages = Vec::new();

        // System prompt as first message
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(json!({
                "role": "system",
                "content": system_prompt,
            }));
        }

        // Convert chat messages
        messages.extend(convert_messages(&request.messages));

        // Build options object
        let mut options = json!({
            "temperature": request.temperature.unwrap_or(0.7),
        });
        if let Some(max_tokens) = request.max_tokens {
            options["num_predict"] = json!(max_tokens);
        }

        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "stream": request.stream,
            "options": options,
        });

        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.serialize_tools(tools));
            }
        } else if let Some(ref tools_val) = request.tools {
            // Fallback: raw tool JSON from the request (legacy path)
            if !tools_val.is_empty() {
                body["tools"] = json!(tools_val);
            }
        }

        Ok(body)
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let message = body
            .get("message")
            .ok_or_else(|| anyhow::anyhow!("no message in Ollama response"))?;

        let content_text = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let model = body
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("ollama")
            .to_string();

        // Build content, including tool calls if present
        let content =
            if let Some(tool_calls) = message.get("tool_calls").and_then(|tc| tc.as_array()) {
                if !tool_calls.is_empty() {
                    let tc_json: Vec<Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            let name = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or_default();
                            let args = tc
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .cloned()
                                .unwrap_or(json!({}));
                            json!({
                                "id": format!("call_{}", name),
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(&args).unwrap_or_default(),
                                }
                            })
                        })
                        .collect();
                    if content_text.is_empty() {
                        serde_json::to_string(&tc_json).unwrap_or_default()
                    } else {
                        format!(
                            "{content_text}\n[TOOL_CALLS:{}]",
                            serde_json::to_string(&tc_json).unwrap_or_default()
                        )
                    }
                } else {
                    content_text
                }
            } else {
                content_text
            };

        let prompt_eval = body
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let eval = body.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        let done = body.get("done").and_then(|d| d.as_bool()).unwrap_or(true);

        Ok(CompletionResponse {
            content,
            model,
            usage: Some(crate::types::response::Usage {
                input_tokens: prompt_eval,
                output_tokens: eval,
                total_tokens: prompt_eval.saturating_add(eval),
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: if done {
                Some("end_turn".to_string())
            } else {
                None
            },
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>> {
        // Ollama streaming uses NDJSON: each line is a bare JSON object.
        // Strip SSE "data: " prefix if present (some proxies add it).
        let line = data.strip_prefix("data: ").unwrap_or(data).trim();
        if line.is_empty() || line == "[DONE]" {
            return Ok(None);
        }

        let val: Value = serde_json::from_str(line)?;

        // Extract token counts from any chunk that has them
        let prompt_eval = val
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let eval = val.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);

        // Content delta
        if let Some(message) = val.get("message") {
            if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                if !content.is_empty() {
                    return Ok(Some(StreamEvent::TextDelta {
                        content: content.to_string(),
                    }));
                }
            }
        }

        // Done flag
        let done = val.get("done").and_then(|d| d.as_bool()).unwrap_or(false);
        if done {
            // Emit token usage if we have counts
            if prompt_eval > 0 || eval > 0 {
                // We need to emit usage but we may have already emitted content
                // The important event is TurnCompleted
            }
            return Ok(Some(StreamEvent::TurnCompleted {
                stop_reason: "end_turn".to_string(),
            }));
        }

        Ok(None)
    }

    fn serialize_tools(&self, tools: &[ToolSchema]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema.to_value()
                    }
                })
            })
            .collect()
    }
}

/// Convert `ChatMessage` values into Ollama's flat message format.
///
/// Ollama uses `{ role, content, images }` where `content` is a plain string
/// and `images` is an optional array of base64 strings. Tool messages are
/// filtered out because Ollama has no native tool-calling wire format (tools
/// are passed via the `tools` field on the request, and tool results are
/// flattened to text).
pub fn convert_messages(messages: &[crate::provider::ChatMessage]) -> Vec<Value> {
    use crate::provider::MessageRole;
    use rustycode_protocol::MessageContent;

    messages
        .iter()
        .filter_map(|msg| {
            // Skip tool-role messages
            if matches!(msg.role, MessageRole::Tool(_)) {
                return None;
            }

            let role_str = match &msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool(_) => unreachable!(),
            };

            // Extract images from block content (vision support)
            let (content_text, images) = match &msg.content {
                MessageContent::Blocks(blocks) => {
                    let mut texts = Vec::new();
                    let mut imgs = Vec::new();
                    for block in blocks {
                        match block {
                            rustycode_protocol::ContentBlock::Text { text, .. } => {
                                texts.push(text.clone());
                            }
                            rustycode_protocol::ContentBlock::Image { source, .. } => {
                                match source.source_type.as_str() {
                                    "base64" => {
                                        imgs.push(source.data.clone());
                                    }
                                    "file" => {
                                        if let Some((_, data)) =
                                            crate::provider::resolve_image_to_base64(source)
                                        {
                                            imgs.push(data);
                                        }
                                    }
                                    _ => {}
                                }
                                texts.push("[Image]".to_string());
                            }
                            rustycode_protocol::ContentBlock::ToolUse { name, .. } => {
                                texts.push(format!("[Tool use: {}]", name));
                            }
                            rustycode_protocol::ContentBlock::ToolResult { content, .. } => {
                                texts.push(content.clone());
                            }
                            rustycode_protocol::ContentBlock::Thinking { thinking, .. } => {
                                texts.push(thinking.clone());
                            }
                            _ => {}
                        }
                    }
                    (
                        texts.join("\n"),
                        if imgs.is_empty() {
                            None
                        } else {
                            Some(imgs.into_iter().map(Value::String).collect::<Vec<_>>())
                        },
                    )
                }
                _ => (msg.content.to_text(), None),
            };

            let mut msg_val = json!({
                "role": role_str,
                "content": content_text,
            });
            if let Some(images) = images {
                msg_val["images"] = Value::Array(images);
            }

            Some(msg_val)
        })
        .collect()
}
