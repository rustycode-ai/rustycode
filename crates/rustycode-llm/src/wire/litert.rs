//! Wire protocol for LiteRT (Local LLM) format.

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

pub struct LiteRTProtocol;

impl Protocol for LiteRTProtocol {
    fn format(&self) -> WireFormat {
        WireFormat::LiteRT
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        Box::new(Self)
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        _tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let mut sections = Vec::new();

        if let Some(system) = &request.system_prompt {
            if !system.trim().is_empty() {
                sections.push(format!("System:\n{}", system.trim()));
            }
        }

        for message in &request.messages {
            let role = match &message.role {
                crate::provider::MessageRole::User => "User",
                crate::provider::MessageRole::Assistant => "Assistant",
                crate::provider::MessageRole::System => "System",
                crate::provider::MessageRole::Tool(tool_name) => {
                    sections.push(format!(
                        "Tool {}:\n{}",
                        tool_name,
                        message.content.to_text()
                    ));
                    continue;
                }
            };
            sections.push(format!("{}:\n{}", role, message.content.to_text()));
        }

        let prompt = sections.join("\n\n");
        Ok(json!({ "prompt": prompt }))
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let content = body
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(CompletionResponse {
            content,
            model: String::new(),
            usage: Some(crate::types::streaming::Usage::new(0, 0)),
            stop_reason: Some("stop".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    fn parse_sse_event(&self, data: &str) -> Result<Vec<StreamEvent>> {
        // LiteRT streaming is just raw text tokens
        Ok(vec![StreamEvent::TextDelta {
            content: data.to_string(),
        }])
    }

    fn serialize_tools(&self, _tools: &[ToolSchema]) -> Vec<Value> {
        vec![]
    }
}
