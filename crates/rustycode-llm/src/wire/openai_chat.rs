//! Wire protocol for OpenAI Chat format.

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::CompletionRequest;
use crate::types::response::{CompletionResponse, ThinkingBlock};
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

use crate::openai::types::{
    OpenAiContentPart, OpenAiFunction, OpenAiImageUrl, OpenAiMessage, OpenAiRequest,
    OpenAiResponse, OpenAiToolCall,
};
use crate::openai::OpenAiProvider;
use crate::provider::MessageRole;

pub struct OpenAIChatProtocol;

impl Protocol for OpenAIChatProtocol {
    fn format(&self) -> WireFormat {
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
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenAiMessage {
                role: "system".to_string(),
                content: Some(json!(system_prompt)),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            });
        }
        messages.extend(self.convert_messages(&request.messages));

        let body = self.build_request_body(
            request.model.clone(),
            messages,
            tools.map(|t| self.serialize_tools(t)).unwrap_or_default(),
            request.max_tokens,
            request.temperature,
            request
                .output_config
                .as_ref()
                .and_then(|c| c.effort.as_ref()),
            Some(request.stream),
            request.output_config.as_ref(),
            request.tool_choice.clone(),
            request.parallel_tool_calls,
            request.session_id.as_ref(),
            request.thinking.as_ref(),
        );

        Ok(json!(body))
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let resp: OpenAiResponse = serde_json::from_value(body.clone())?;

        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no choices in response"))?;

        let mut content = choice.message.content.unwrap_or_default();

        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let tool_calls_json: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "type": tc.r#type,
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            }
                        })
                    })
                    .collect();
                let formatted = serde_json::to_string_pretty(&tool_calls_json)?;
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&format!("```tool\n{}\n```", formatted));
            }
        }

        let usage = resp.usage.map(|u| {
            let cached = u
                .prompt_tokens_details
                .as_ref()
                .map_or(0, |d| d.cached_tokens);
            crate::types::streaming::Usage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                cache_read_input_tokens: cached,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }
        });

        Ok(CompletionResponse {
            content,
            model: resp.model,
            usage,
            stop_reason: crate::provider::normalize_stop_reason(choice.finish_reason.as_deref()),
            citations: None,
            thinking_blocks: choice.message.reasoning_content.map(|rc| {
                vec![ThinkingBlock {
                    block_type: "thinking".to_string(),
                    thinking: rc,
                    signature: String::new(),
                    data: String::new(),
                    display: None,
                }]
            }),
            structured_output: None,
        })
    }

    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>> {
        if data == "[DONE]" {
            return Ok(Some(StreamEvent::Done));
        }

        let val: Value = serde_json::from_str(data)?;

        // Use existing helper if possible, but it returns a Vec<Result<StreamEvent, ProviderError>>
        // which might be overkill for a single SSE event.
        // For now, I'll just implement the logic directly or wrap the helper.

        if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
            if let Some(choice) = choices.first() {
                if let Some(delta) = choice.get("delta") {
                    // Content
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            return Ok(Some(StreamEvent::TextDelta {
                                content: content.to_string(),
                            }));
                        }
                    }

                    // Reasoning
                    for key in ["reasoning_content", "reasoning"] {
                        if let Some(reasoning) = delta.get(key).and_then(|r| r.as_str()) {
                            if !reasoning.is_empty() {
                                return Ok(Some(StreamEvent::ThinkingDelta {
                                    content: reasoning.to_string(),
                                }));
                            }
                        }
                    }

                    // Tool calls
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                        if let Some(tc_delta) = tool_calls.first() {
                            if let Some(id) = tc_delta.get("id").and_then(|i| i.as_str()) {
                                let name = tc_delta
                                    .get("function")
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or_default();
                                return Ok(Some(StreamEvent::ToolCallStarted {
                                    id: id.to_string(),
                                    name: name.to_string(),
                                }));
                            }
                            if let Some(partial) = tc_delta
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                            {
                                if !partial.is_empty() {
                                    // Id is not in the delta if it's a subsequent chunk.
                                    // High-level caller handles mapping index to id.
                                    // We use the index as a placeholder.
                                    let index =
                                        tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                                    return Ok(Some(StreamEvent::ToolInputDelta {
                                        id: index.to_string(),
                                        chunk: partial.to_string(),
                                    }));
                                }
                            }
                        }
                    }
                }

                if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    let reason = crate::provider::normalize_stop_reason(Some(finish_reason))
                        .unwrap_or_else(|| finish_reason.to_string());
                    return Ok(Some(StreamEvent::TurnCompleted {
                        stop_reason: reason,
                    }));
                }
            }
        }

        if let Some(usage) = val.get("usage") {
            let input = usage
                .get("prompt_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("completion_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            return Ok(Some(StreamEvent::TokenUsage {
                input_tokens: input,
                output_tokens: output,
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

impl OpenAIChatProtocol {
    fn convert_messages(&self, messages: &[crate::provider::ChatMessage]) -> Vec<OpenAiMessage> {
        use rustycode_protocol::{ContentBlock, MessageContent};

        messages
            .iter()
            .flat_map(|msg| {
                let role_str = match &msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                    MessageRole::Tool(_) => "tool",
                };

                match &msg.content {
                    MessageContent::Blocks(blocks) => {
                        let mut tool_results: Vec<OpenAiMessage> = Vec::new();
                        let mut other_parts: Vec<OpenAiContentPart> = Vec::new();
                        let mut tool_calls: Vec<OpenAiToolCall> = Vec::new();
                        let mut reasoning_content: Option<String> = None;

                        for block in blocks {
                            match block {
                                ContentBlock::Text { text, .. } => {
                                    other_parts
                                        .push(OpenAiContentPart::Text { text: text.clone() });
                                }
                                ContentBlock::Image { source, .. } => {
                                    if source.source_type == "url" {
                                        other_parts.push(OpenAiContentPart::ImageUrl {
                                            image_url: OpenAiImageUrl {
                                                url: source.data.clone(),
                                                detail: None,
                                            },
                                        });
                                    } else if let Some((mime, data)) =
                                        crate::provider::resolve_image_to_base64(source)
                                    {
                                        other_parts.push(OpenAiContentPart::ImageUrl {
                                            image_url: OpenAiImageUrl {
                                                url: format!("data:{mime};base64,{data}"),
                                                detail: None,
                                            },
                                        });
                                    }
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => {
                                    let display_content = if *is_error {
                                        format!("Error: {content}")
                                    } else {
                                        content.clone()
                                    };
                                    tool_results.push(OpenAiMessage {
                                        role: "tool".to_string(),
                                        content: Some(json!(display_content)),
                                        tool_calls: None,
                                        tool_call_id: Some(tool_use_id.clone()),
                                        name: None,
                                        reasoning_content: None,
                                    });
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    tool_calls.push(OpenAiToolCall {
                                        id: id.clone(),
                                        r#type: "function".to_string(),
                                        function: OpenAiFunction {
                                            name: name.clone(),
                                            arguments: serde_json::to_string(input)
                                                .unwrap_or_else(|_| "{}".to_string()),
                                        },
                                    });
                                }
                                ContentBlock::Thinking { thinking, .. } if !thinking.is_empty() => {
                                    reasoning_content = Some(thinking.clone());
                                }
                                _ => {}
                            }
                        }

                        let mut result = Vec::new();
                        if !other_parts.is_empty()
                            || !tool_calls.is_empty()
                            || reasoning_content.is_some()
                        {
                            let content_val = if other_parts.is_empty() {
                                None
                            } else if other_parts.len() == 1 {
                                match &other_parts[0] {
                                    OpenAiContentPart::Text { text } => Some(json!(text)),
                                    _ => Some(json!(other_parts)),
                                }
                            } else {
                                Some(json!(other_parts))
                            };

                            result.push(OpenAiMessage {
                                role: if tool_calls.is_empty() {
                                    role_str.to_string()
                                } else {
                                    "assistant".to_string()
                                },
                                content: content_val,
                                tool_calls: if tool_calls.is_empty() {
                                    None
                                } else {
                                    Some(tool_calls)
                                },
                                tool_call_id: None,
                                name: None,
                                reasoning_content,
                            });
                        }
                        result.extend(tool_results);
                        result
                    }
                    _ => {
                        vec![OpenAiMessage {
                            role: role_str.to_string(),
                            content: Some(json!(msg.content.to_text())),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                            reasoning_content: None,
                        }]
                    }
                }
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn build_request_body(
        &self,
        model: String,
        messages: Vec<OpenAiMessage>,
        tools: Vec<Value>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        effort: Option<&crate::provider::EffortLevel>,
        stream: Option<bool>,
        output_config: Option<&crate::provider::OutputConfig>,
        tool_choice: Option<Value>,
        parallel_tool_calls: Option<bool>,
        session_id: Option<&String>,
        thinking_config: Option<&crate::provider::ThinkingConfig>,
    ) -> OpenAiRequest {
        let (max_tokens_val, max_completion_tokens) = if OpenAiProvider::is_reasoning_model(&model)
        {
            (None, max_tokens)
        } else {
            (max_tokens, None)
        };

        let reasoning_effort = if OpenAiProvider::is_reasoning_model(&model) {
            effort.map(|e| match e {
                crate::provider::EffortLevel::Low => "low".to_string(),
                crate::provider::EffortLevel::Medium => "medium".to_string(),
                crate::provider::EffortLevel::High => "high".to_string(),
                crate::provider::EffortLevel::Xhigh => "xhigh".to_string(),
                crate::provider::EffortLevel::Max => "xhigh".to_string(),
            })
        } else {
            None
        };

        let temperature = if OpenAiProvider::is_reasoning_model(&model) {
            None
        } else {
            temperature
        };

        let response_format = output_config.and_then(|cfg| {
            cfg.format.as_ref().map(|fmt| match fmt.format_type {
                crate::provider::OutputFormatType::JsonSchema => {
                    json!({
                        "type": "json_schema",
                        "json_schema": fmt.json_schema.as_ref().unwrap_or(&json!({}))
                    })
                }
            })
        });

        let thinking = match thinking_config {
            Some(cfg) => match cfg.thinking_type {
                crate::provider::ThinkingType::Disabled => None,
                crate::provider::ThinkingType::Enabled => {
                    let mut obj = json!({"type": "enabled"});
                    if let Some(budget) = cfg.budget_tokens {
                        obj["budget_tokens"] = json!(budget);
                    }
                    Some(obj)
                }
                crate::provider::ThinkingType::Adaptive => Some(json!({"type": "enabled"})),
            },
            None if model.starts_with("glm-5")
                || model.starts_with("glm-4.5")
                || model.starts_with("glm-4.6")
                || model.starts_with("glm-4.7") =>
            {
                Some(json!({"type": "enabled"}))
            }
            None => None,
        };

        let stream_options = if stream == Some(true) {
            Some(json!({"include_usage": true}))
        } else {
            None
        };

        OpenAiRequest {
            model,
            messages,
            temperature,
            max_tokens: max_tokens_val,
            max_completion_tokens,
            stream,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_choice,
            parallel_tool_calls,
            reasoning_effort,
            response_format,
            thinking,
            stream_options,
            prompt_cache_key: session_id.cloned(),
        }
    }
}
