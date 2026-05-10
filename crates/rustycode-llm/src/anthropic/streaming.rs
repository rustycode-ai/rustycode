//! Anthropic SSE stream parsing logic for streaming completions.
//!
//! Contains the `complete_stream_internal` method that builds streaming requests
//! and parses SSE events into structured `StreamEvent` results.

use crate::advisor::AdvisorTool;
use crate::provider::{CompletionRequest, ProviderError, StreamChunk, Usage};
use futures::{Stream, StreamExt};
use rustycode_protocol::stream_event::StreamEvent;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use super::helpers::{
    apply_cache_to_last_messages, map_anthropic_error, map_anthropic_structured_error,
    normalize_thinking_for_model,
};
use super::types::{AnthropicRequest, CacheControl, SystemContentBlock, SystemPrompt};

impl super::AnthropicProvider {
    /// Internal implementation of streaming completion without retry logic.
    pub async fn complete_stream_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = self.endpoint();

        // Convert ChatMessage to AnthropicMessage
        let mut messages = self.parse_conversation_messages(&request.messages);

        // Use intelligent tool selection if tools not explicitly provided
        let mut tools = match request.tools {
            Some(tools) => tools, // Use explicitly provided tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
                    .unwrap_or_default()
            }
        };

        // Inject advisor tool if configured via builder or env var
        let advisor_tool = self
            .advisor_config
            .as_ref()
            .map(|c| c.advisor.to_anthropic_tool())
            .or_else(|| {
                std::env::var("RUSTYCODE_ADVISOR_MODEL").ok().map(|model| {
                    let advisor = AdvisorTool::new(model);
                    tracing::info!(
                        "Advisor tool auto-enabled via RUSTYCODE_ADVISOR_MODEL={}",
                        advisor.advisor_model
                    );
                    advisor.to_anthropic_tool()
                })
            });

        if let Some(tool) = &advisor_tool {
            tools.push(tool.clone());
        }

        let has_advisor = advisor_tool.is_some();

        let _wants_structured_output = request
            .output_config
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_some_and(|f| {
                matches!(f.format_type, crate::provider::OutputFormatType::JsonSchema)
            });

        // Enable prompt caching for all Anthropic-compatible endpoints.
        // Most proxies (OpenRouter, z.ai, etc.) support cache_control correctly.
        let use_cache_control = true;

        // Apply cache_control to last 2 conversation messages.
        // Combined with system prompt (1) and tools (1), this uses all 4 allowed breakpoints.
        if use_cache_control {
            apply_cache_to_last_messages(&mut messages, 2);
        }

        let system = request.system_prompt.map(|text| {
            if use_cache_control {
                SystemPrompt::Blocks(vec![SystemContentBlock {
                    block_type: "text",
                    text,
                    cache_control: Some(CacheControl {
                        cache_type: "ephemeral",
                    }),
                }])
            } else {
                SystemPrompt::Text(text)
            }
        });

        let tools = if !tools.is_empty() {
            if use_cache_control {
                if let Some(last_tool) = tools.last_mut().and_then(|t| t.as_object_mut()) {
                    last_tool.insert(
                        "cache_control".to_string(),
                        serde_json::json!({"type": "ephemeral"}),
                    );
                }
            }
            Some(tools)
        } else {
            None
        };

        let anthropic_request = AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system,
            stream: Some(true),
            tools,
            thinking: normalize_thinking_for_model(
                request.thinking,
                request.output_config.as_ref().and_then(|c| c.effort),
                &request.model,
            ),
            output_config: request.output_config,
            container: request.container.clone(),
            tool_choice: request.tool_choice.as_ref().map(|tc| match tc.as_str() {
                Some("required") => serde_json::json!({"type": "any"}),
                Some("auto") => serde_json::json!({"type": "auto"}),
                Some("none") => serde_json::json!({"type": "none"}),
                _ => tc.clone(),
            }),
            parallel_tool_calls: request.parallel_tool_calls,
        };

        // HTTP trace: dump stream request body for debugging
        let trace_dir = std::env::var("RTK_HTTP_TRACE_DIR").unwrap_or_default();
        if !trace_dir.is_empty() {
            let _ = std::fs::create_dir_all(&trace_dir);
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            if let Ok(body) = serde_json::to_string_pretty(&anthropic_request) {
                let _ = std::fs::write(format!("{trace_dir}/{ts}_stream_req.json"), &body);
            }
        }

        // Build the request, adding beta headers for advisor and skills
        let mut request_builder = self.client.post(&url).json(&anthropic_request);

        if has_advisor {
            request_builder = request_builder.header("anthropic-beta", AdvisorTool::beta_header());
        }

        if request.container.is_some() {
            for header in crate::tools::anthropic_skills_beta_headers() {
                request_builder = request_builder.header("anthropic-beta", header);
            }
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to send request: {}", e)))?;

        tracing::info!(
            "[http-trace] Stream response: status={} content-type={:?} url={}",
            response.status(),
            response.headers().get("content-type"),
            url
        );

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            tracing::error!(
                "API error {} (model: {}): {}",
                status,
                self.model,
                error_text
            );

            if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(error_obj) = error_json.get("error").and_then(|e| e.as_object()) {
                    let error_type = error_obj
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown_error");

                    let message = error_obj
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or(&error_text);

                    let param = error_obj.get("param").and_then(|p| p.as_str());

                    return Err(map_anthropic_structured_error(
                        status, error_type, message, param, &headers,
                    ));
                }
            }

            return Err(map_anthropic_error(status, &error_text, &headers));
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        // Parse SSE events and emit structured events
        // Buffer partial lines and event type across chunk boundaries using shared state
        let byte_buffer = crate::sse::SseByteBuffer::new();
        let stream_state = Arc::new(std::sync::Mutex::new((
            None::<String>,
            HashMap::<usize, String>::new(),
            HashMap::<usize, String>::new(),
            HashMap::<usize, String>::new(),
            HashMap::<usize, String>::new(),
        )));
        let event_stream = bytes_stream.flat_map(move |chunk_result| {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(format!(
                        "Failed to read chunk: {}",
                        e
                    )))])
                }
            };

            let lines = byte_buffer.feed_chunk(&chunk);
            let mut state = stream_state.lock().unwrap_or_else(|e| e.into_inner());
            let (
                current_event_type,
                tool_ids_by_index,
                thinking_signatures,
                thinking_block_types,
                redacted_data,
            ) = &mut *state;

            let mut events = Vec::new();

            for line in &lines {
                if line.starts_with("event: ") {
                    *current_event_type = Some(line.trim_start_matches("event: ").to_string());
                    continue;
                }

                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        continue;
                    }

                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        match current_event_type.as_deref() {
                            Some("error") => {
                                if let Some(error_obj) = data.get("error") {
                                    let error_type = error_obj
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("streaming_error")
                                        .to_string();
                                    let message = error_obj
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("streaming error occurred")
                                        .to_string();
                                    events.push(Err(ProviderError::Api(format!(
                                        "{}: {}",
                                        error_type, message
                                    ))));
                                }
                            }
                            Some("message_start") => {}
                            Some("content_block_start") => {
                                if let Some(block_obj) = data.get("content_block") {
                                    let index =
                                        data.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                            as usize;

                                    if let Some(block_type) =
                                        block_obj.get("type").and_then(|t| t.as_str())
                                    {
                                        match block_type {
                                            "tool_use" | "tool_reference" => {
                                                let id = block_obj
                                                    .get("id")
                                                    .and_then(|i| i.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let name = block_obj
                                                    .get("name")
                                                    .and_then(|n| n.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                tool_ids_by_index.insert(index, id.clone());
                                                events.push(Ok(StreamEvent::ToolCallStarted {
                                                    id,
                                                    name,
                                                }));
                                            }
                                            "thinking" => {
                                                thinking_block_types
                                                    .insert(index, "thinking".to_string());
                                            }
                                            "redacted_thinking" => {
                                                thinking_block_types
                                                    .insert(index, "redacted_thinking".to_string());
                                                let data = block_obj
                                                    .get("data")
                                                    .and_then(|d| d.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                redacted_data.insert(index, data);
                                            }
                                            "text" => {}
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            Some("content_block_delta") => {
                                let index = data.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                    as usize;

                                if let Some(delta_obj) = data.get("delta") {
                                    if let Some(delta_type) =
                                        delta_obj.get("type").and_then(|t| t.as_str())
                                    {
                                        match delta_type {
                                            "text_delta" => {
                                                let text = delta_obj
                                                    .get("text")
                                                    .and_then(|t| t.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                if !text.is_empty() {
                                                    events.push(Ok(StreamEvent::TextDelta {
                                                        content: text,
                                                    }));
                                                }
                                            }
                                            "input_json_delta" => {
                                                let partial_json = delta_obj
                                                    .get("partial_json")
                                                    .and_then(|j| j.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                if !partial_json.is_empty() {
                                                    if let Some(id) =
                                                        tool_ids_by_index.get(&index).cloned()
                                                    {
                                                        events.push(Ok(
                                                            StreamEvent::ToolInputDelta {
                                                                id,
                                                                chunk: partial_json,
                                                            },
                                                        ));
                                                    }
                                                }
                                            }
                                            "thinking_delta" => {
                                                let thinking = delta_obj
                                                    .get("thinking")
                                                    .and_then(|t| t.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                if !thinking.is_empty() {
                                                    events.push(Ok(StreamEvent::ThinkingDelta {
                                                        content: thinking,
                                                    }));
                                                }
                                            }
                                            "signature_delta" => {
                                                if let Some(sig) = delta_obj
                                                    .get("signature")
                                                    .and_then(|s| s.as_str())
                                                {
                                                    thinking_signatures
                                                        .entry(index)
                                                        .and_modify(|existing| {
                                                            existing.push_str(sig);
                                                        })
                                                        .or_insert_with(|| sig.to_string());
                                                }
                                            }
                                            "citations_delta" => {}
                                            _ => {
                                                if let Some(text) =
                                                    delta_obj.get("text").and_then(|t| t.as_str())
                                                {
                                                    if !text.is_empty() {
                                                        events.push(Ok(StreamEvent::TextDelta {
                                                            content: text.to_string(),
                                                        }));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Some("content_block_stop") => {
                                let index = data.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                    as usize;
                                tool_ids_by_index.remove(&index);

                                if let Some(block_type) = thinking_block_types.remove(&index) {
                                    let signature =
                                        thinking_signatures.remove(&index).unwrap_or_default();
                                    let data = redacted_data.remove(&index).unwrap_or_default();
                                    events.push(Ok(StreamEvent::ThinkingBlockCompleted {
                                        block_type,
                                        signature,
                                        data,
                                    }));
                                }
                            }
                            Some("message_delta") => {
                                let stop_reason = data
                                    .get("delta")
                                    .and_then(|d| d.get("stop_reason"))
                                    .and_then(|s| s.as_str())
                                    .map(|s| s.to_string());

                                let usage = data.get("usage").and_then(|u| {
                                    let input_tokens: u32 = u
                                        .get("input_tokens")
                                        .and_then(|t| t.as_u64())?
                                        .try_into()
                                        .unwrap_or(u32::MAX);
                                    let output_tokens: u32 = u
                                        .get("output_tokens")
                                        .and_then(|t| t.as_u64())?
                                        .try_into()
                                        .unwrap_or(u32::MAX);
                                    let cache_read_input_tokens: u32 = u
                                        .get("cache_read_input_tokens")
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0)
                                        .try_into()
                                        .unwrap_or(u32::MAX);
                                    let cache_creation_input_tokens: u32 = u
                                        .get("cache_creation_input_tokens")
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0)
                                        .try_into()
                                        .unwrap_or(u32::MAX);

                                    let total_input = cache_read_input_tokens
                                        .saturating_add(cache_creation_input_tokens)
                                        .saturating_add(input_tokens);

                                    Some(Usage {
                                        input_tokens,
                                        output_tokens,
                                        total_tokens: total_input.saturating_add(output_tokens),
                                        cache_read_input_tokens,
                                        cache_creation_input_tokens,
                                        reasoning_tokens: None,
                                    })
                                });

                                if let Some(usage) = usage {
                                    // Only emit CacheUsage when we requested caching —
                                    // proxies may return fake cache fields.
                                    if use_cache_control
                                        && (usage.cache_read_input_tokens > 0
                                            || usage.cache_creation_input_tokens > 0)
                                    {
                                        let cache_read = usage.cache_read_input_tokens;
                                        let cache_creation = usage.cache_creation_input_tokens;
                                        tracing::info!(
                                            "Cache: {} read, {} written, input={}, output={}",
                                            cache_read,
                                            cache_creation,
                                            usage.input_tokens,
                                            usage.output_tokens
                                        );
                                        events.push(Ok(StreamEvent::CacheUsage {
                                            cache_read_tokens: u64::from(
                                                usage.cache_read_input_tokens,
                                            ),
                                            cache_creation_tokens: u64::from(
                                                usage.cache_creation_input_tokens,
                                            ),
                                        }));
                                    }
                                    events.push(Ok(StreamEvent::TokenUsage {
                                        input_tokens: u64::from(usage.input_tokens),
                                        output_tokens: u64::from(usage.output_tokens),
                                    }));
                                }

                                let reason =
                                    crate::provider::normalize_stop_reason(stop_reason.as_deref())
                                        .unwrap_or_else(|| {
                                            stop_reason
                                                .clone()
                                                .unwrap_or_else(|| "end_turn".to_string())
                                        });
                                events.push(Ok(StreamEvent::TurnCompleted {
                                    stop_reason: reason,
                                }));
                            }
                            Some("message_stop") => {
                                events.push(Ok(StreamEvent::Done));
                            }
                            Some("ping") => {}
                            _ => {}
                        }
                    }
                }
            }

            futures::stream::iter(events)
        });

        Ok(Box::pin(event_stream))
    }
}
