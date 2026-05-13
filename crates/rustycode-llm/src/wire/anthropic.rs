//! Wire protocol for Anthropic format.

use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::provider::{
    Citation, CompletionRequest, CompletionResponse, MessageRole, ThinkingBlock, Usage,
};
use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::wire::Protocol;
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_tools_api::{ToolMetadataProvider, ToolRegistry};

use crate::anthropic::helpers::{normalize_thinking_for_model, parse_tool_result_content};
use crate::anthropic::types::{
    AnthropicMessage, AnthropicRequest, AnthropicRequestContent, AnthropicResponse,
    ImageSource as AnthropicImageSource, SystemContentBlock, SystemPrompt,
};
use crate::anthropic::ContentBlock;

#[derive(Default)]
pub struct AnthropicStreamState {
    pub tool_ids_by_index: HashMap<usize, String>,
    pub thinking_signatures: HashMap<usize, String>,
    pub thinking_block_types: HashMap<usize, String>,
    pub redacted_data: HashMap<usize, String>,
}

pub struct AnthropicProtocol {
    pub registry: Option<Arc<ToolRegistry>>,
    pub state: Arc<Mutex<AnthropicStreamState>>,
}

impl Protocol for AnthropicProtocol {
    fn format(&self) -> WireFormat {
        WireFormat::Anthropic
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        // Shared state clones (for events within a stream)
        Box::new(Self {
            registry: self.registry.clone(),
            state: self.state.clone(),
        })
    }

    fn clone_with_fresh_state(&self) -> Box<dyn Protocol> {
        // Fresh state clone (for a new stream)
        Box::new(Self {
            registry: self.registry.clone(),
            state: Arc::new(Mutex::new(Default::default())),
        })
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let mut messages = self.parse_conversation_messages(&request.messages);

        // Filter out empty or invalid messages
        messages.retain(|msg| match &msg.content {
            AnthropicRequestContent::Text(text) => !text.trim().is_empty(),
            AnthropicRequestContent::Blocks(blocks) => !blocks.is_empty(),
        });

        if messages.is_empty() {
            anyhow::bail!("No valid messages to send to Anthropic API after filtering");
        }

        // Enable prompt caching for all Anthropic-compatible endpoints.
        let use_cache_control = true;

        if use_cache_control {
            // Apply cache_control to last 2 conversation messages.
            let start = messages.len().saturating_sub(2);
            for msg in &mut messages[start..] {
                match &mut msg.content {
                    AnthropicRequestContent::Text(text) => {
                        let text_val = text.clone();
                        msg.content = AnthropicRequestContent::Blocks(vec![ContentBlock::Text {
                            content_type: "text",
                            text: text_val,
                            cache_control: Some(crate::anthropic::types::CacheControl {
                                cache_type: "ephemeral",
                            }),
                        }]);
                    }
                    AnthropicRequestContent::Blocks(blocks) => {
                        if let Some(last_block) = blocks.last_mut() {
                            match last_block {
                                ContentBlock::Text { cache_control, .. }
                                | ContentBlock::ToolUse { cache_control, .. }
                                | ContentBlock::ToolResult { cache_control, .. } => {
                                    *cache_control = Some(crate::anthropic::types::CacheControl {
                                        cache_type: "ephemeral",
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        let system = request.system_prompt.as_ref().map(|text| {
            if use_cache_control {
                SystemPrompt::Blocks(vec![SystemContentBlock {
                    block_type: "text",
                    text: text.clone(),
                    cache_control: Some(crate::anthropic::types::CacheControl {
                        cache_type: "ephemeral",
                    }),
                }])
            } else {
                SystemPrompt::Text(text.clone())
            }
        });

        let mut anthropic_tools = match tools {
            Some(t) => Some(self.serialize_tools(t)),
            None => request.tools.clone(),
        };

        if let Some(ref mut t_list) = anthropic_tools {
            if use_cache_control {
                // Apply cache_control to the last tool if it's a standard tool
                if let Some(last_tool) = t_list.last_mut().and_then(|t| t.as_object_mut()) {
                    // Don't add cache_control to deferred-only tools or special advisor tools
                    // Standard tools have input_schema or description.
                    if last_tool.contains_key("description")
                        || last_tool.contains_key("input_schema")
                    {
                        last_tool.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
                    }
                }
            }
        }

        let anthropic_request = AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system,
            stream: Some(request.stream),
            tools: anthropic_tools,
            thinking: normalize_thinking_for_model(
                request.thinking.clone(),
                request.output_config.as_ref().and_then(|c| c.effort),
                &request.model,
            ),
            output_config: request.output_config.clone(),
            container: request.container.clone(),
            tool_choice: request.tool_choice.as_ref().map(|tc| match tc.as_str() {
                Some("required") => json!({"type": "any"}),
                Some("auto") => json!({"type": "auto"}),
                Some("none") => json!({"type": "none"}),
                _ => tc.clone(),
            }),
            parallel_tool_calls: request.parallel_tool_calls,
        };

        Ok(serde_json::to_value(anthropic_request)?)
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let response: AnthropicResponse = serde_json::from_value(body.clone())?;

        let mut content_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<Value> = Vec::new();
        let mut all_citations: Vec<Citation> = Vec::new();
        let mut thinking_blocks: Vec<ThinkingBlock> = Vec::new();
        let mut refused = false;

        for block in &response.content {
            match block.content_type.as_str() {
                "json" | "text" => {
                    if !block.text.is_empty() {
                        content_parts.push(block.text.clone());
                    }
                    if let Some(ref citations) = block.citations {
                        for c in citations {
                            all_citations.push(Citation {
                                source: c.url.clone(),
                                title: c.title.clone(),
                                cited_text: c.cited_text.clone(),
                                index: c.search_result_index.unwrap_or(0),
                            });
                        }
                    }
                }
                "tool_use" => {
                    tool_calls.push(json!({
                        "id": block.id,
                        "name": block.name,
                        "arguments": block.input
                    }));
                }
                "tool_reference" => {
                    // Native deferred tool resolution: the model wants to use a
                    // deferred tool. Resolve by looking up the full schema from the
                    // registry so downstream code can treat this as a normal tool_use.
                    let tool_name = &block.name;
                    let resolved = self.registry.as_ref().and_then(|r| {
                        r.tool_info(tool_name).map(|info| {
                            json!({
                                "id": block.id,
                                "name": tool_name,
                                "arguments": info.parameters_schema
                            })
                        })
                    });
                    if let Some(tool_call) = resolved {
                        tracing::debug!("Resolved tool_reference '{}' to tool_use", tool_name);
                        tool_calls.push(tool_call);
                    } else {
                        tracing::warn!(
                            "tool_reference '{}' could not be resolved (registry missing or tool not found)",
                            tool_name
                        );
                    }
                }
                "thinking" => {
                    thinking_blocks.push(ThinkingBlock {
                        block_type: "thinking".to_string(),
                        thinking: block.thinking.clone(),
                        signature: block.signature.clone(),
                        data: String::new(),
                        display: None,
                    });
                }
                "redacted_thinking" => {
                    thinking_blocks.push(ThinkingBlock {
                        block_type: "redacted_thinking".to_string(),
                        thinking: String::new(),
                        signature: String::new(),
                        data: block.data.clone(),
                        display: None,
                    });
                }
                "refusal" => {
                    refused = true;
                    if !block.text.is_empty() {
                        content_parts.push(format!("[REFUSAL] {}", block.text));
                    } else {
                        content_parts.push("[REFUSAL]".to_string());
                    }
                }
                _ => {}
            }
        }

        if !tool_calls.is_empty() {
            let tool_calls_json = serde_json::to_string_pretty(&tool_calls)?;
            content_parts.push(format!("```tool\n{}\n```", tool_calls_json));
        }

        let input_tokens = response.usage.input_tokens as u32;
        let output_tokens = response.usage.output_tokens as u32;
        let cache_read = response.usage.cache_read_input_tokens as u32;
        let cache_creation = response.usage.cache_creation_input_tokens as u32;

        let usage = if cache_read > 0 || cache_creation > 0 {
            Usage::with_cache(input_tokens, output_tokens, cache_read, cache_creation)
        } else {
            Usage::new(input_tokens, output_tokens)
        };

        Ok(CompletionResponse {
            content: content_parts.join("\n"),
            model: response.model,
            usage: Some(usage),
            stop_reason: response.stop_reason.or_else(|| {
                if refused {
                    Some("refusal".to_string())
                } else if !tool_calls.is_empty() {
                    Some("tool_use".to_string())
                } else {
                    Some("end_turn".to_string())
                }
            }),
            citations: if all_citations.is_empty() {
                None
            } else {
                Some(all_citations)
            },
            thinking_blocks: if thinking_blocks.is_empty() {
                None
            } else {
                Some(thinking_blocks)
            },
            structured_output: None, // Will be handled by caller if needed
        })
    }

    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>> {
        if data == "[DONE]" {
            return Ok(Some(StreamEvent::Done));
        }

        let val: Value = serde_json::from_str(data)?;
        let event_type = val.get("type").and_then(|t| t.as_str());

        let mut state = self.state.lock().map_err(|e| {
            anyhow::anyhow!("Anthropic stream state lock poisoned: {e}")
        })?;

        match event_type {
            Some("message_start") => Ok(None),
            Some("content_block_start") => {
                if let Some(block_obj) = val.get("content_block") {
                    let index = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let block_type = block_obj.get("type").and_then(|t| t.as_str());
                    match block_type {
                        Some("tool_use") | Some("tool_reference") => {
                            let id = block_obj
                                .get("id")
                                .and_then(|i| i.as_str())
                                .unwrap_or_default();
                            let name = block_obj
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or_default();
                            state.tool_ids_by_index.insert(index, id.to_string());
                            Ok(Some(StreamEvent::ToolCallStarted {
                                id: id.to_string(),
                                name: name.to_string(),
                            }))
                        }
                        Some("thinking") => {
                            state
                                .thinking_block_types
                                .insert(index, "thinking".to_string());
                            Ok(None)
                        }
                        Some("redacted_thinking") => {
                            state
                                .thinking_block_types
                                .insert(index, "redacted_thinking".to_string());
                            let data = block_obj
                                .get("data")
                                .and_then(|d| d.as_str())
                                .unwrap_or_default()
                                .to_string();
                            state.redacted_data.insert(index, data);
                            Ok(None)
                        }
                        _ => Ok(None),
                    }
                } else {
                    Ok(None)
                }
            }
            Some("content_block_delta") => {
                let index = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                if let Some(delta) = val.get("delta") {
                    let delta_type = delta.get("type").and_then(|t| t.as_str());
                    match delta_type {
                        Some("text_delta") => {
                            let text = delta
                                .get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or_default();
                            Ok(Some(StreamEvent::TextDelta {
                                content: text.to_string(),
                            }))
                        }
                        Some("input_json_delta") => {
                            let partial = delta
                                .get("partial_json")
                                .and_then(|j| j.as_str())
                                .unwrap_or_default();
                            if let Some(id) = state.tool_ids_by_index.get(&index) {
                                Ok(Some(StreamEvent::ToolInputDelta {
                                    id: id.clone(),
                                    chunk: partial.to_string(),
                                }))
                            } else {
                                Ok(None)
                            }
                        }
                        Some("thinking_delta") => {
                            let thinking = delta
                                .get("thinking")
                                .and_then(|t| t.as_str())
                                .unwrap_or_default();
                            Ok(Some(StreamEvent::ThinkingDelta {
                                content: thinking.to_string(),
                            }))
                        }
                        Some("signature_delta") => {
                            if let Some(sig) = delta.get("signature").and_then(|s| s.as_str()) {
                                state
                                    .thinking_signatures
                                    .entry(index)
                                    .and_modify(|existing| existing.push_str(sig))
                                    .or_insert_with(|| sig.to_string());
                            }
                            Ok(None)
                        }
                        _ => Ok(None),
                    }
                } else {
                    Ok(None)
                }
            }
            Some("content_block_stop") => {
                let index = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                state.tool_ids_by_index.remove(&index);
                if let Some(block_type) = state.thinking_block_types.remove(&index) {
                    let signature = state.thinking_signatures.remove(&index).unwrap_or_default();
                    let data = state.redacted_data.remove(&index).unwrap_or_default();
                    Ok(Some(StreamEvent::ThinkingBlockCompleted {
                        block_type,
                        signature,
                        data,
                    }))
                } else {
                    Ok(None)
                }
            }
            Some("message_delta") => {
                if let Some(usage_val) = val.get("usage") {
                    let input_tokens = usage_val
                        .get("input_tokens")
                        .and_then(|i| i.as_u64())
                        .unwrap_or(0);
                    let output_tokens = usage_val
                        .get("output_tokens")
                        .and_then(|i| i.as_u64())
                        .unwrap_or(0);

                    if let Some(stop_reason) = val
                        .get("delta")
                        .and_then(|d| d.get("stop_reason"))
                        .and_then(|s| s.as_str())
                    {
                        let normalized = crate::provider::normalize_stop_reason(Some(stop_reason))
                            .unwrap_or_else(|| stop_reason.to_string());
                        return Ok(Some(StreamEvent::TurnCompleted {
                            stop_reason: normalized,
                        }));
                    }

                    Ok(Some(StreamEvent::TokenUsage {
                        input_tokens,
                        output_tokens,
                    }))
                } else {
                    Ok(None)
                }
            }
            Some("message_stop") => Ok(Some(StreamEvent::Done)),
            _ => Ok(None),
        }
    }

    fn serialize_tools(&self, tools: &[ToolSchema]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                if t.description.contains("[DEFERRED:") {
                    json!({
                        "name": t.name,
                        "defer_loading": true
                    })
                } else {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema.to_value()
                    })
                }
            })
            .collect()
    }

    fn extra_headers(&self, request: &CompletionRequest) -> Vec<(String, String)> {
        let mut headers = Vec::new();

        // Anthropic requires a version header
        headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));

        // Add beta headers for skills if container is present
        if request.container.is_some() {
            for header in crate::tools::anthropic_skills_beta_headers() {
                headers.push(("anthropic-beta".to_string(), header.to_string()));
            }
        }

        // Add beta header for prompt caching if deferred tools are present or if requested
        let mut has_deferred = false;
        let mut has_advisor = false;

        if let Some(tools) = &request.tools {
            for t in tools {
                // Check for advisor tool (type: advisor_20260301)
                if t.get("type").and_then(|v| v.as_str()) == Some("advisor_20260301") {
                    has_advisor = true;
                }
                // Check for deferred tools ([DEFERRED: in description)
                if t.get("description")
                    .and_then(|v| v.as_str())
                    .is_some_and(|d| d.contains("[DEFERRED:"))
                {
                    has_deferred = true;
                }
            }
        }

        if has_advisor {
            headers.push((
                "anthropic-beta".to_string(),
                "advisor-tool-2026-03-01".to_string(),
            ));
        }

        if has_deferred {
            headers.push((
                "anthropic-beta".to_string(),
                "prompt-caching-2024-07-31".to_string(),
            ));
        }

        headers
    }
}

impl AnthropicProtocol {
    fn parse_conversation_messages(
        &self,
        messages: &[crate::provider::ChatMessage],
    ) -> Vec<AnthropicMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match &msg.role {
                    MessageRole::System => "user",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool(_) => "user",
                };

                if let rustycode_protocol::MessageContent::Blocks(blocks) = &msg.content {
                    let anthropic_blocks: Vec<ContentBlock> = blocks
                        .iter()
                        .map(|b| match b {
                            rustycode_protocol::ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => ContentBlock::ToolResult {
                                content_type: "tool_result",
                                tool_use_id: tool_use_id.clone(),
                                content: parse_tool_result_content(
                                    &serde_json::from_str::<Value>(content)
                                        .unwrap_or(json!(content)),
                                ),
                                is_error: if *is_error { Some(true) } else { None },
                                cache_control: None,
                            },
                            rustycode_protocol::ContentBlock::ToolUse { id, name, input } => {
                                ContentBlock::ToolUse {
                                    content_type: "tool_use",
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                    cache_control: None,
                                }
                            }
                            rustycode_protocol::ContentBlock::Text { text, .. } => {
                                ContentBlock::Text {
                                    content_type: "text",
                                    text: text.clone(),
                                    cache_control: None,
                                }
                            }
                            rustycode_protocol::ContentBlock::Image { source, .. } => {
                                ContentBlock::Image {
                                    content_type: "image",
                                    source: AnthropicImageSource {
                                        source_type: source.source_type.clone(),
                                        media_type: source.media_type.clone(),
                                        data: source.data.clone(),
                                    },
                                }
                            }
                            rustycode_protocol::ContentBlock::Thinking {
                                thinking,
                                signature,
                            } => ContentBlock::Thinking {
                                content_type: "thinking",
                                thinking: thinking.clone(),
                                signature: signature.clone(),
                            },
                            rustycode_protocol::ContentBlock::RedactedThinking { data } => {
                                ContentBlock::RedactedThinking {
                                    content_type: "redacted_thinking",
                                    data: data.clone(),
                                }
                            }
                            _ => ContentBlock::Text {
                                content_type: "text",
                                text: "[unsupported block]".to_string(),
                                cache_control: None,
                            },
                        })
                        .collect();

                    let effective_role = if anthropic_blocks
                        .iter()
                        .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
                    {
                        "user"
                    } else {
                        role
                    };
                    AnthropicMessage {
                        role: effective_role,
                        content: AnthropicRequestContent::Blocks(anthropic_blocks),
                    }
                } else {
                    AnthropicMessage {
                        role,
                        content: AnthropicRequestContent::Text(msg.content.as_text()),
                    }
                }
            })
            .collect()
    }
}
