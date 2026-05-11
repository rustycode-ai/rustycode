//! Wire protocol for Cohere v2 format.

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

pub struct CohereProtocol;

impl Protocol for CohereProtocol {
    fn format(&self) -> WireFormat {
        WireFormat::Cohere
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        Box::new(Self)
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let messages = self.convert_messages(&request.messages);
        let tools_blocks = tools.map(|t| self.serialize_tools(t));

        let body = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "preamble": request.system_prompt,
            "tools": tools_blocks,
            "tool_choice": request.tool_choice,
            "stream": request.stream,
        });

        Ok(body)
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let message = body
            .get("message")
            .ok_or_else(|| anyhow::anyhow!("no message"))?;
        let content_blocks = message
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| anyhow::anyhow!("no content blocks"))?;

        let mut content = String::new();
        for block in content_blocks {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(text);
            }
        }

        if let Some(plan) = message.get("tool_plan").and_then(|p| p.as_str()) {
            if !plan.is_empty() {
                if content.is_empty() {
                    content = plan.to_string();
                } else {
                    content = format!("{}\n\n{}", plan, content);
                }
            }
        }

        if let Some(calls) = message.get("tool_calls").and_then(|c| c.as_array()) {
            if !calls.is_empty() {
                let mut tc_json = Vec::new();
                for tc in calls {
                    let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("unknown");
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    let args = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("{}");
                    tc_json.push(json!({
                        "id": id,
                        "type": "function",
                        "function": { "name": name, "arguments": args }
                    }));
                }
                let tc_str = serde_json::to_string(&tc_json).unwrap_or_default();
                if content.is_empty() {
                    content = tc_str;
                } else {
                    content = format!("{content}\n[TOOL_CALLS:{tc_str}]");
                }
            }
        }

        let usage = body
            .get("meta")
            .and_then(|m| m.get("usage"))
            .and_then(|u| u.get("tokens"))
            .map(|t| {
                let input = t.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let output = t.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                crate::types::streaming::Usage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: input.saturating_add(output),
                    cache_read_input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    reasoning_tokens: None,
                }
            });

        let finish_reason = body.get("finish_reason").and_then(|f| f.as_str());

        Ok(CompletionResponse {
            content,
            model: String::new(),
            usage,
            stop_reason: match finish_reason {
                Some("COMPLETE") => Some("end_turn".to_string()),
                Some("TOOL_CALL") => Some("tool_use".to_string()),
                Some("MAX_TOKENS") => Some("max_tokens".to_string()),
                other => other.map(|s| s.to_string()),
            },
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>> {
        let val: Value = serde_json::from_str(data)?;
        let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "content-delta" => {
                if let Some(text) = val
                    .get("delta")
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.get("text"))
                    .and_then(|t| t.as_str())
                {
                    return Ok(Some(StreamEvent::TextDelta {
                        content: text.to_string(),
                    }));
                }
            }
            "tool-plan-delta" => {
                if let Some(plan) = val
                    .get("delta")
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.get("tool_plan"))
                    .and_then(|t| t.as_str())
                {
                    return Ok(Some(StreamEvent::TextDelta {
                        content: plan.to_string(),
                    }));
                }
            }
            "tool-call-delta" => {
                if let Some(args) = val
                    .get("delta")
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.get("tool_calls"))
                    .and_then(|tc| tc.get(0))
                    .and_then(|tc| tc.get("function"))
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                {
                    // id and name might be in a separate event (tool-call-start) in some APIs,
                    // but Cohere v2 streaming for tool calls is often grouped.
                    // For now, mapping to index "0".
                    return Ok(Some(StreamEvent::ToolInputDelta {
                        id: "0".to_string(),
                        chunk: args.to_string(),
                    }));
                }
            }
            "message-end" => {
                let finish_reason = val
                    .get("delta")
                    .and_then(|d| d.get("finish_reason"))
                    .and_then(|f| f.as_str())
                    .unwrap_or("COMPLETE");
                return Ok(Some(StreamEvent::TurnCompleted {
                    stop_reason: finish_reason.to_string(),
                }));
            }
            _ => {}
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

impl CohereProtocol {
    fn convert_messages(&self, messages: &[crate::provider::ChatMessage]) -> Vec<Value> {
        use crate::provider::MessageRole;
        use rustycode_protocol::message::{ContentBlock, MessageContent};

        let mut result = Vec::new();
        for msg in messages {
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool(_) => "tool",
            };

            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();
            let mut tool_call_id = None;

            match &msg.content {
                MessageContent::Simple(t) => text_parts.push(t.clone()),
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse {
                                id, name, input, ..
                            } => {
                                tool_calls.push(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": serde_json::to_string(input).unwrap_or_default()
                                    }
                                }));
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } => {
                                tool_call_id = Some(tool_use_id.clone());
                                text_parts.push(content.clone());
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }

            if let MessageRole::Tool(ref id) = msg.role {
                tool_call_id = Some(id.clone());
            }

            let content = if tool_call_id.is_some() {
                let data = text_parts.join("\n");
                Some(json!([{"type": "document", "document": {"data": data}}]))
            } else if text_parts.is_empty() && !tool_calls.is_empty() {
                None
            } else {
                Some(json!(text_parts.join("\n")))
            };

            result.push(json!({
                "role": role,
                "content": content,
                "tool_calls": if tool_calls.is_empty() { None } else { Some(tool_calls) },
                "tool_call_id": tool_call_id,
            }));
        }
        result
    }
}
