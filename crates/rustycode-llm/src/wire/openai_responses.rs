//! Wire protocol for OpenAI Responses API format.

use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::schema::normalizer::WireFormat;
use crate::schema::tool_schema::ToolSchema;
use crate::types::request::{CompletionRequest, ToolChoice};
use crate::types::response::CompletionResponse;
use crate::types::streaming::StreamEvent;
use crate::wire::Protocol;

use crate::openai::OpenAiProvider;
use crate::openai_compatible::{
    ResponsesApiReasoning, ResponsesApiRequest, ResponsesApiResponse, ResponsesApiTool,
    ResponsesSseState,
};

/// Stateful protocol for the OpenAI Responses API.
///
/// Tracks `response_id` across requests so that `previous_response_id` is
/// automatically included, enabling server-side conversation state.
pub struct OpenAIResponsesProtocol {
    /// Tracks the last response ID for server-side conversation state.
    pub last_response_id: Arc<std::sync::Mutex<Option<String>>>,
}

impl Protocol for OpenAIResponsesProtocol {
    fn format(&self) -> WireFormat {
        WireFormat::OpenAIResponses
    }

    fn clone_box(&self) -> Box<dyn Protocol> {
        Box::new(Self {
            last_response_id: self.last_response_id.clone(),
        })
    }

    fn clone_with_fresh_state(&self) -> Box<dyn Protocol> {
        // Share the same response ID tracker across stream clones
        self.clone_box()
    }

    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<Value> {
        let (instructions, input) =
            crate::openai_compatible::convert_messages_to_responses_input(request);

        let tools_opt = tools
            .map(|t| {
                let normalized = self.serialize_tools(t);
                normalized
                    .into_iter()
                    .filter_map(|v| serde_json::from_value::<ResponsesApiTool>(v).ok())
                    .collect::<Vec<ResponsesApiTool>>()
            })
            .or_else(|| {
                request.tools.as_ref().map(|canonical| {
                    canonical
                        .iter()
                        .map(|t| ResponsesApiTool {
                            tool_type: "function".to_string(),
                            name: t
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            description: t
                                .get("description")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            parameters: t.get("input_schema").cloned(),
                        })
                        .collect::<Vec<ResponsesApiTool>>()
                })
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

        // Read the previous response ID for server-side conversation state
        let prev_id = self
            .last_response_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let body = ResponsesApiRequest {
            model: request.model.clone(),
            input,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            top_p: None,
            max_output_tokens: request.max_tokens,
            stream: Some(request.stream),
            previous_response_id: prev_id,
            tool_choice: request.tool_choice.as_ref().map(|tc| match tc {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::Required => json!("required"),
                ToolChoice::None => json!("none"),
                ToolChoice::Named(name) => {
                    json!({"type": "function", "function": {"name": name}})
                }
            }),
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

        // Store the response ID for the next request in the conversation
        if let Ok(mut guard) = self.last_response_id.lock() {
            *guard = Some(resp.id.clone());
        }

        crate::openai_compatible::build_responses_completion_response(&resp)
            .map_err(|e| anyhow::anyhow!(e))
    }

    fn parse_sse_event(&self, data: &str) -> Result<Vec<StreamEvent>> {
        let state = ResponsesSseState::default();
        let events = crate::openai_compatible::parse_responses_sse_lines(data, &state);
        Ok(events.into_iter().flatten().collect())
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
