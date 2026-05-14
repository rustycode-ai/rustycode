//! Wire protocol for OpenAI Chat format.

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::{CompletionRequest, ToolChoice};
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
            request.tool_choice.as_ref().map(|tc| match tc {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::Required => json!("required"),
                ToolChoice::None => json!("none"),
                ToolChoice::Named(name) => {
                    json!({"type": "function", "function": {"name": name}})
                }
            }),
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

    fn parse_sse_event(&self, data: &str) -> Result<Vec<StreamEvent>> {
        if data == "[DONE]" {
            return Ok(vec![StreamEvent::Done]);
        }

        let val: Value = serde_json::from_str(data)?;

        if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
            if let Some(choice) = choices.first() {
                if let Some(delta) = choice.get("delta") {
                    // Content
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            return Ok(vec![StreamEvent::TextDelta {
                                content: content.to_string(),
                            }]);
                        }
                    }

                    // Reasoning
                    for key in ["reasoning_content", "reasoning"] {
                        if let Some(reasoning) = delta.get(key).and_then(|r| r.as_str()) {
                            if !reasoning.is_empty() {
                                return Ok(vec![StreamEvent::ThinkingDelta {
                                    content: reasoning.to_string(),
                                }]);
                            }
                        }
                    }

                    // Tool calls — providers may send multiple tool calls per chunk
                    // (z.ai bundles id+name+arguments; OpenAI sends incremental deltas).
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                        let mut events = Vec::new();
                        for tc_delta in tool_calls {
                            let has_id = tc_delta.get("id").and_then(|i| i.as_str());
                            let arguments = tc_delta
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                                .unwrap_or("");

                            if let Some(id) = has_id {
                                let name = tc_delta
                                    .get("function")
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or_default();

                                events.push(StreamEvent::ToolCallStarted {
                                    id: id.to_string(),
                                    name: name.to_string(),
                                });
                                // z.ai: id + non-empty arguments in one chunk
                                if !arguments.is_empty() {
                                    events.push(StreamEvent::ToolInputDelta {
                                        id: id.to_string(),
                                        chunk: arguments.to_string(),
                                    });
                                }
                            } else if !arguments.is_empty() {
                                // Subsequent chunks: incremental arguments via index
                                let index =
                                    tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                                events.push(StreamEvent::ToolInputDelta {
                                    id: index.to_string(),
                                    chunk: arguments.to_string(),
                                });
                            }
                        }
                        if !events.is_empty() {
                            return Ok(events);
                        }
                    }
                }

                if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    let reason = crate::provider::normalize_stop_reason(Some(finish_reason))
                        .unwrap_or_else(|| finish_reason.to_string());
                    return Ok(vec![StreamEvent::TurnCompleted {
                        stop_reason: reason,
                    }]);
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
            return Ok(vec![StreamEvent::TokenUsage {
                input_tokens: input,
                output_tokens: output,
            }]);
        }

        Ok(vec![])
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

        let has_tools = !tools.is_empty();

        OpenAiRequest {
            model,
            messages,
            temperature,
            max_tokens: max_tokens_val,
            max_completion_tokens,
            stream,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_stream: if stream == Some(true) && has_tools {
                Some(true)
            } else {
                None
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::stream_event::StreamEvent;

    #[test]
    fn single_chunk_tool_call_with_arguments() {
        // z.ai sends id + name + arguments all in one chunk
        let payload = r#"{"choices":[{"delta":{"tool_calls":[{"id":"call_123","index":0,"type":"function","function":{"name":"read_file","arguments":"{\"path\":\"/tmp/test.txt\"}"}}]}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            StreamEvent::ToolCallStarted { id, name }
            if id == "call_123" && name == "read_file"
        ));
        assert!(matches!(
            &events[1],
            StreamEvent::ToolInputDelta { id, chunk }
            if id == "call_123" && chunk == r#"{"path":"/tmp/test.txt"}"#
        ));
    }

    #[test]
    fn multiple_tool_calls_in_one_chunk() {
        // z.ai can send multiple tool calls in a single chunk
        let payload = r#"{"choices":[{"delta":{"tool_calls":[{"id":"call_1","index":0,"type":"function","function":{"name":"read_file","arguments":"{\"path\":\"/tmp/a.txt\"}"}},{"id":"call_2","index":1,"type":"function","function":{"name":"bash","arguments":"{\"command\":\"ls\"}"}}]}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 4);
        // Tool call 1: started + delta
        assert!(matches!(
            &events[0],
            StreamEvent::ToolCallStarted { id, name }
            if id == "call_1" && name == "read_file"
        ));
        assert!(matches!(
            &events[1],
            StreamEvent::ToolInputDelta { id, chunk }
            if id == "call_1" && chunk == r#"{"path":"/tmp/a.txt"}"#
        ));
        // Tool call 2: started + delta
        assert!(matches!(
            &events[2],
            StreamEvent::ToolCallStarted { id, name }
            if id == "call_2" && name == "bash"
        ));
        assert!(matches!(
            &events[3],
            StreamEvent::ToolInputDelta { id, chunk }
            if id == "call_2" && chunk == r#"{"command":"ls"}"#
        ));
    }

    #[test]
    fn openai_style_tool_call_first_chunk_empty_args() {
        // OpenAI sends id + name with empty arguments first
        let payload = r#"{"choices":[{"delta":{"tool_calls":[{"id":"call_456","index":0,"type":"function","function":{"name":"bash","arguments":""}}]}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::ToolCallStarted { id, name }
            if id == "call_456" && name == "bash"
        ));
    }

    #[test]
    fn openai_style_tool_call_incremental_args() {
        // Subsequent chunks have only index + arguments
        let payload = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"pa"}}]}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::ToolInputDelta { id, chunk }
            if id == "0" && chunk == r#"{"pa"#
        ));
    }

    #[test]
    fn text_delta_returns_single_event() {
        let payload = r#"{"choices":[{"delta":{"content":"hello "}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::TextDelta { content } if content == "hello "
        ));
    }

    #[test]
    fn done_marker_returns_done_event() {
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event("[DONE]").unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], StreamEvent::Done));
    }

    #[test]
    fn unrecognized_json_returns_empty_vec() {
        let payload = r#"{"id":"chatcmpl-123","object":"chat.completion.chunk"}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert!(events.is_empty());
    }

    // --- UTF-8 and special character tests ---

    #[test]
    fn text_delta_with_cjk_characters() {
        let payload = r#"{"choices":[{"delta":{"content":"日本語テスト 🎉"}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::TextDelta { content }
            if content == "日本語テスト 🎉"
        ));
    }

    #[test]
    fn text_delta_with_emoji_and_zwj_sequences() {
        // 👨‍👩‍👧‍👦 is a ZWJ sequence (family emoji, 7 codepoints)
        let payload = r#"{"choices":[{"delta":{"content":"Hello 👨‍👩‍👧‍👦 world 🦀"}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::TextDelta { content }
            if content == "Hello 👨‍👩‍👧‍👦 world 🦀"
        ));
    }

    #[test]
    fn tool_call_args_with_unicode_path() {
        let payload = serde_json::json!({
            "choices": [{"delta": {"tool_calls": [{"id": "call_abc", "index": 0, "type": "function", "function": {"name": "read_file", "arguments": "{\"path\":\"/用户/文档/文件.txt\"}"}}]}}]
        });
        let payload_str = serde_json::to_string(&payload).unwrap();
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(&payload_str).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            StreamEvent::ToolCallStarted { id, name }
            if id == "call_abc" && name == "read_file"
        ));
        assert!(matches!(
            &events[1],
            StreamEvent::ToolInputDelta { id, chunk }
            if id == "call_abc" && chunk.contains("用户")
        ));
    }

    #[test]
    fn tool_call_args_with_special_json_chars() {
        let payload = serde_json::json!({
            "choices": [{"delta": {"tool_calls": [{"id": "call_xyz", "index": 0, "type": "function", "function": {"name": "write_file", "arguments": "{\"content\":\"line1\\nline2\\ttab\\\"quote\\\\backslash\"}"}}]}}]
        });
        let payload_str = serde_json::to_string(&payload).unwrap();
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(&payload_str).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1],
            StreamEvent::ToolInputDelta { chunk, .. }
            if chunk.contains("line1") && chunk.contains("line2")
        ));
    }

    #[test]
    fn reasoning_delta_with_unicode() {
        let payload = r#"{"choices":[{"delta":{"reasoning_content":"思考中... 继续思考 💭"}}]}"#;
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(payload).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::ThinkingDelta { content }
            if content == "思考中... 继续思考 💭"
        ));
    }

    #[test]
    fn text_delta_with_null_bytes_and_control_chars() {
        // Ensure control chars pass through without panicking
        let content = "before\x00after\ttab\rcarriage\nnewline";
        let payload = format!(
            r#"{{"choices":[{{"delta":{{"content":{}}}}}]}}"#,
            serde_json::to_string(content).unwrap()
        );
        let protocol = OpenAIChatProtocol;
        let events = protocol.parse_sse_event(&payload).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            StreamEvent::TextDelta { content: c }
            if c.contains("before") && c.contains("after")
        ));
    }
}
