//! Wire protocol for OpenAI Responses API format.

use anyhow::Result;
use serde_json::{json, Value};

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::CompletionRequest;
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

use crate::openai::OpenAiProvider;
use crate::openai_compatible::{
    ResponsesApiReasoning, ResponsesApiRequest, ResponsesApiResponse, ResponsesApiTool,
    ResponsesSseState,
};

pub struct OpenAIResponsesProtocol;

impl Protocol for OpenAIResponsesProtocol {
    fn format(&self) -> WireFormat {
        WireFormat::OpenAIResponses
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        Box::new(Self)
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let (instructions, input) =
            crate::openai_compatible::convert_messages_to_responses_input(request);

        let tools_opt = tools.map(|t| {
            let normalized = self.serialize_tools(t);
            normalized
                .into_iter()
                .filter_map(|v| serde_json::from_value::<ResponsesApiTool>(v).ok())
                .collect::<Vec<ResponsesApiTool>>()
        });

        let is_reasoning = OpenAiProvider::is_reasoning_model(&request.model);
        let effort = request
            .output_config
            .as_ref()
            .and_then(|c| c.effort.as_ref());

        let reasoning = if is_reasoning {
            effort.map(|e| {
                let effort_str = match e {
                    crate::provider::EffortLevel::Low => "low",
                    crate::provider::EffortLevel::Medium => "medium",
                    crate::provider::EffortLevel::High => "high",
                    crate::provider::EffortLevel::Xhigh => "xhigh",
                    crate::provider::EffortLevel::Max => "xhigh",
                };
                ResponsesApiReasoning {
                    effort: Some(effort_str.to_string()),
                    summary: Some("auto".to_string()),
                    encrypted_content: None,
                }
            })
        } else {
            None
        };

        let include = if reasoning.is_some() {
            Some(vec!["reasoning_encrypted_content".to_string()])
        } else {
            None
        };

        let body = ResponsesApiRequest {
            model: request.model.clone(),
            input,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            top_p: None,
            max_output_tokens: request.max_tokens,
            stream: Some(request.stream),
            previous_response_id: None, // Handle state at higher level if needed
            tool_choice: request.tool_choice.clone(),
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning,
            include,
            store: Some(true),
            prompt_cache_key: request.session_id.clone(),
        };

        Ok(json!(body))
    }

    fn parse_response(&self, body: &Value) -> Result<CompletionResponse> {
        let resp: ResponsesApiResponse = serde_json::from_value(body.clone())?;
        crate::openai_compatible::build_responses_completion_response(&resp)
            .map_err(|e| anyhow::anyhow!(e))
    }

    fn parse_sse_event(&self, data: &str) -> Result<Option<StreamEvent>> {
        // Responses API SSE format is different from Chat Completions.
        // It uses `event: ...` lines.
        // The existing `parse_responses_sse_lines` handles a batch of lines.
        // For a single event string, we can try to wrap it.

        let state = ResponsesSseState::default();
        let events = crate::openai_compatible::parse_responses_sse_lines(data, &state);

        // Return the first successful event
        if let Some(ev) = events.into_iter().flatten().next() {
            return Ok(Some(ev));
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
