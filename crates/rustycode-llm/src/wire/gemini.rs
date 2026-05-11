//! Wire protocol for Google Gemini format.

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::config::OutputFormatType;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

pub struct GeminiProtocol;

impl Protocol for GeminiProtocol {
    fn format(&self) -> WireFormat {
        WireFormat::Gemini
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        Box::new(Self)
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let system_instruction = request.system_prompt.as_ref().map(|prompt| {
            json!({
                "parts": [{"text": prompt}]
            })
        });

        let contents = self.convert_messages(&request.messages);

        let tools_blocks = tools.map(|t| {
            json!([{
                "functionDeclarations": self.serialize_tools(t)
            }])
        });

        let tool_config = request.tool_choice.as_ref().and_then(|choice| {
            match choice {
                Value::String(s) => match s.as_str() {
                    "auto" => Some(json!({"functionCallingConfig": {"mode": "AUTO"}})),
                    "none" => Some(json!({"functionCallingConfig": {"mode": "NONE"}})),
                    "required" => Some(json!({"functionCallingConfig": {"mode": "ANY"}})),
                    _ => None,
                },
                Value::Object(map) => {
                    map.get("name")
                        .and_then(|v| v.as_str())
                        .map(|name| json!({"functionCallingConfig": {"mode": "ANY", "allowedFunctionNames": [name]}}))
                }
                _ => None,
            }
        });

        // Build generation config with optional structured output support
        let mut generation_config = json!({
            "temperature": request.temperature.unwrap_or(0.7),
            "maxOutputTokens": request.max_tokens,
        });

        if let Some(output_config) = &request.output_config {
            if let Some(format) = &output_config.format {
                if matches!(format.format_type, OutputFormatType::JsonSchema) {
                    let schema = format.json_schema.clone().unwrap_or(Value::Null);
                    // Sanitize schema for Gemini compatibility
                    let mut sanitized = schema;
                    sanitize_schema_recursive(&mut sanitized);
                    generation_config
                        .as_object_mut()
                        .expect("generation_config is an object")
                        .insert("responseMimeType".to_string(), json!("application/json"));
                    generation_config
                        .as_object_mut()
                        .expect("generation_config is an object")
                        .insert("responseSchema".to_string(), sanitized);
                }
            }
        }

        // Only include toolConfig when tools are present — Gemini rejects
        // "function calling config without function_declarations" (HTTP 400).
        let tool_config = if tools_blocks.is_some() {
            tool_config
        } else {
            None
        };

        let body = json!({
            "contents": contents,
            "generationConfig": generation_config,
            "systemInstruction": system_instruction,
            "tools": tools_blocks,
            "toolConfig": tool_config,
        });

        Ok(body)
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let candidates = body
            .get("candidates")
            .and_then(|c| c.as_array())
            .ok_or_else(|| anyhow::anyhow!("no candidates"))?;
        let candidate = candidates
            .first()
            .ok_or_else(|| anyhow::anyhow!("empty candidates"))?;

        let parts = candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .ok_or_else(|| anyhow::anyhow!("no parts"))?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for part in parts {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    text_parts.push(text.to_string());
                }
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                let args = fc.get("args").cloned().unwrap_or(json!({}));
                tool_calls.push(json!({
                    "id": format!("call_{}", name),
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(&args).unwrap_or_default(),
                    }
                }));
            }
        }

        let content = if !tool_calls.is_empty() {
            if text_parts.is_empty() {
                serde_json::to_string(&tool_calls).unwrap_or_default()
            } else {
                let mut result = text_parts.join("\n");
                result.push_str(&format!(
                    "\n[TOOL_CALLS:{}]",
                    serde_json::to_string(&tool_calls).unwrap_or_default()
                ));
                result
            }
        } else {
            text_parts.join("\n")
        };

        let usage = body.get("usageMetadata").map(|u| {
            let input = u
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let output = u
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let cached = u
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let total = u
                .get("totalTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            crate::types::streaming::Usage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: total,
                cache_read_input_tokens: cached,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }
        });

        let finish_reason = candidate.get("finishReason").and_then(|f| f.as_str());

        Ok(CompletionResponse {
            content,
            model: String::new(), // Model name not always in response body
            usage,
            stop_reason: crate::provider::normalize_stop_reason(finish_reason),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>> {
        // Gemini SSE sends JSON objects directly
        let val: Value = serde_json::from_str(data)?;

        if let Some(candidates) = val.get("candidates").and_then(|c| c.as_array()) {
            if let Some(candidate) = candidates.first() {
                if let Some(parts) = candidate
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                return Ok(Some(StreamEvent::TextDelta {
                                    content: text.to_string(),
                                }));
                            }
                        }
                        if let Some(fc) = part.get("functionCall") {
                            let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                            return Ok(Some(StreamEvent::ToolCallStarted {
                                id: format!("call_{}", name),
                                name: name.to_string(),
                            }));
                        }
                    }
                }

                if let Some(finish_reason) = candidate.get("finishReason").and_then(|f| f.as_str())
                {
                    return Ok(Some(StreamEvent::TurnCompleted {
                        stop_reason: finish_reason.to_string(),
                    }));
                }
            }
        }

        if let Some(usage) = val.get("usageMetadata") {
            let input = usage
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
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
                let mut parameters = t.input_schema.to_value();
                self.sanitize_schema(&mut parameters);
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": parameters
                })
            })
            .collect()
    }
}

impl GeminiProtocol {
    fn convert_messages(&self, messages: &[crate::provider::ChatMessage]) -> Vec<Value> {
        use crate::provider::MessageRole;
        use rustycode_protocol::message::{ContentBlock, MessageContent};

        let mut contents = Vec::new();
        for msg in messages {
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "model",
                MessageRole::System => continue,
                MessageRole::Tool(_) => "user",
            };

            let mut parts = Vec::new();
            match &msg.content {
                MessageContent::Simple(text) if !text.is_empty() => {
                    parts.push(json!({"text": text}));
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } if !text.is_empty() => {
                                parts.push(json!({"text": text}));
                            }
                            ContentBlock::ToolUse {
                                id: _, name, input, ..
                            } => {
                                parts.push(json!({
                                    "functionCall": {
                                        "name": name,
                                        "args": input
                                    }
                                }));
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } => {
                                parts.push(json!({
                                    "functionResponse": {
                                        "name": tool_use_id, // Gemini expects the name used in functionCall
                                        "response": {"result": content}
                                    }
                                }));
                            }
                            ContentBlock::Thinking { thinking, .. } if !thinking.is_empty() => {
                                parts.push(json!({"text": format!("[thinking] {thinking}")}));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }

            if !parts.is_empty() {
                contents.push(json!({
                    "role": role,
                    "parts": parts
                }));
            }
        }
        contents
    }

    fn sanitize_schema(&self, value: &mut Value) {
        sanitize_schema_recursive(value);
    }
}

fn sanitize_schema_recursive(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    obj.remove("$schema");
    obj.remove("$defs");
    obj.remove("$ref");

    if let Some(type_val) = obj.get_mut("type") {
        if let Some(arr) = type_val.as_array() {
            let first_non_null = arr
                .iter()
                .find(|v| v.as_str().is_some_and(|s| s != "null"))
                .cloned()
                .unwrap_or(json!("string"));
            *type_val = first_non_null;
        }
    }

    if let Some(default) = obj.get("default") {
        if default.is_null() {
            obj.remove("default");
        }
    }

    if let Some(items) = obj.get_mut("items") {
        if items.is_boolean() {
            *items = json!({});
        }
        sanitize_schema_recursive(items);
    }

    if let Some(props) = obj.get_mut("properties") {
        if let Some(props_obj) = props.as_object_mut() {
            for (_key, prop) in props_obj.iter_mut() {
                sanitize_schema_recursive(prop);
            }
        }
    }

    for keyword in &["anyOf", "oneOf", "allOf"] {
        if let Some(arr) = obj.get_mut(*keyword).and_then(|v| v.as_array_mut()) {
            for item in arr.iter_mut() {
                sanitize_schema_recursive(item);
            }
        }
    }
}
