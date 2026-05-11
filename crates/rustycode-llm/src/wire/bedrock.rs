//! Wire protocol for AWS Bedrock format.

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

pub struct BedrockProtocol;

impl Protocol for BedrockProtocol {
    fn format(&self) -> WireFormat {
        WireFormat::Bedrock
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        Box::new(Self)
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let system = request
            .system_prompt
            .as_ref()
            .map(|s| vec![json!({ "text": s })]);

        let messages = self.convert_messages(&request.messages);

        // Use typed ToolSchema if provided, otherwise fall back to raw JSON from request
        let serialized_tools: Option<Vec<Value>> = match tools {
            Some(t) if !t.is_empty() => Some(self.serialize_tools(t)),
            _ => request
                .tools
                .as_ref()
                .map(|raw| self.convert_raw_tools(raw)),
        };

        let tool_config = serialized_tools.filter(|t| !t.is_empty()).map(|t| {
            json!({
                "tools": t,
                "toolChoice": match &request.tool_choice {
                    Some(Value::String(s)) => match s.as_str() {
                        "auto" => json!({"auto": {}}),
                        "required" => json!({"any": {}}),
                        name => json!({"tool": {"name": name}}),
                    },
                    _ => json!({"auto": {}}),
                }
            })
        });

        let mut body = json!({
            "messages": messages,
            "system": system,
            "inferenceConfig": {
                "maxTokens": request.max_tokens.unwrap_or(4096),
                "temperature": request.temperature.unwrap_or(0.7),
            },
        });

        if let Some(tc) = tool_config {
            body["toolConfig"] = tc;
        }

        Ok(body)
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let output = body
            .get("output")
            .and_then(|o| o.get("message"))
            .ok_or_else(|| anyhow::anyhow!("no output message"))?;
        let content_parts = output
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| anyhow::anyhow!("no content parts"))?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for part in content_parts {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    text_parts.push(text.to_string());
                }
            }
            if let Some(tu) = part.get("toolUse") {
                let id = tu
                    .get("toolUseId")
                    .and_then(|i| i.as_str())
                    .unwrap_or("unknown");
                let name = tu.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                let input = tu.get("input").cloned().unwrap_or(json!({}));
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(&input).unwrap_or_default(),
                    }
                }));
            }
        }

        let mut content = text_parts.join("\n");
        if !tool_calls.is_empty() {
            let fmt =
                serde_json::to_string_pretty(&tool_calls).unwrap_or_else(|_| "[]".to_string());
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&format!("```tool\n{}\n```", fmt));
        }

        let usage = body.get("usage").map(|u| {
            let input = u.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let output = u.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let total = u.get("totalTokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            crate::types::streaming::Usage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: total,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }
        });

        let stop_reason = body.get("stopReason").and_then(|f| f.as_str());

        Ok(CompletionResponse {
            content,
            model: String::new(),
            usage,
            stop_reason: crate::provider::normalize_stop_reason(stop_reason),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>> {
        let val: Value = serde_json::from_str(data)?;

        if let Some(delta) = val.get("contentBlockDelta").and_then(|d| d.get("delta")) {
            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                return Ok(Some(StreamEvent::TextDelta {
                    content: text.to_string(),
                }));
            }
            if let Some(tool_use) = delta.get("toolUse") {
                if let Some(id) = tool_use.get("toolUseId").and_then(|i| i.as_str()) {
                    let name = tool_use
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or_default();
                    return Ok(Some(StreamEvent::ToolCallStarted {
                        id: id.to_string(),
                        name: name.to_string(),
                    }));
                }
                if let Some(input) = tool_use.get("input").and_then(|i| i.as_str()) {
                    // Bedrock streaming for tools is a bit different, but we'll try to map it
                    return Ok(Some(StreamEvent::ToolInputDelta {
                        id: "0".to_string(),
                        chunk: input.to_string(),
                    }));
                }
            }
        }

        if let Some(usage) = val.get("messageStop").and_then(|m| m.get("usage")) {
            let input = usage
                .get("inputTokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("outputTokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            return Ok(Some(StreamEvent::TokenUsage {
                input_tokens: input,
                output_tokens: output,
            }));
        }

        if let Some(stop) = val.get("messageStop").and_then(|m| m.get("stopReason")) {
            if let Some(reason) = stop.as_str() {
                return Ok(Some(StreamEvent::TurnCompleted {
                    stop_reason: reason.to_string(),
                }));
            }
        }

        Ok(None)
    }

    fn serialize_tools(&self, tools: &[ToolSchema]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "toolSpec": {
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": {
                            "json": t.input_schema.to_value()
                        }
                    }
                })
            })
            .collect()
    }
}

impl BedrockProtocol {
    /// Convert raw JSON tool definitions (OpenAI/Anthropic format) to Bedrock toolSpec format.
    fn convert_raw_tools(&self, tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .map(|tool| {
                let name = tool
                    .get("name")
                    .or_else(|| tool.get("function").and_then(|f| f.get("name")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let description = tool
                    .get("description")
                    .or_else(|| tool.get("function").and_then(|f| f.get("description")))
                    .and_then(|v| v.as_str());
                let parameters = tool
                    .get("parameters")
                    .or_else(|| tool.get("input_schema"))
                    .cloned()
                    .unwrap_or(json!({"type": "object", "properties": {}}));

                let mut spec = json!({
                    "name": name,
                    "inputSchema": { "json": parameters }
                });
                if let Some(desc) = description {
                    spec["description"] = json!(desc);
                }
                json!({ "toolSpec": spec })
            })
            .collect()
    }

    fn convert_messages(&self, messages: &[crate::provider::ChatMessage]) -> Vec<Value> {
        use crate::provider::MessageRole;
        use rustycode_protocol::message::{ContentBlock, MessageContent};

        let mut converse_messages = Vec::new();
        for msg in messages {
            let role = match msg.role {
                MessageRole::User | MessageRole::Tool(_) => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "user",
            };

            let mut content = Vec::new();
            match &msg.content {
                MessageContent::Simple(text) if !text.is_empty() => {
                    content.push(json!({"text": text}));
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } if !text.is_empty() => {
                                content.push(json!({"text": text}));
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                content.push(json!({
                                    "toolUse": { "toolUseId": id, "name": name, "input": input }
                                }));
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content: result_text,
                                is_error,
                            } => {
                                let status = if *is_error { "error" } else { "success" };
                                content.push(json!({
                                    "toolResult": {
                                        "toolUseId": tool_use_id,
                                        "content": [{ "text": result_text }],
                                        "status": status
                                    }
                                }));
                            }
                            ContentBlock::Thinking { thinking, .. } if !thinking.is_empty() => {
                                content.push(json!({
                                    "text": format!("[prior-reasoning]\n{}\n[/prior-reasoning]", thinking)
                                }));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }

            if !content.is_empty() {
                converse_messages.push(json!({
                    "role": role,
                    "content": content
                }));
            }
        }
        converse_messages
    }
}
