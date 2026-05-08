//! OpenAI Responses API integration (`POST /v1/responses`).
//!
//! Contains the non-streaming and streaming Responses API methods,
//! plus the WebSocket streaming path (feature-gated).

use crate::provider::{CompletionRequest, CompletionResponse, ProviderError, StreamChunk};
use crate::{build_request, get_api_key};
use secrecy::ExposeSecret;
use std::pin::Pin;

use super::OpenAiProvider;

impl OpenAiProvider {
    /// Complete a request using the Responses API (`POST /v1/responses`).
    pub(super) async fn complete_responses(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let retry_config = self.config.retry_config.clone().unwrap_or_default();

        crate::retry::retry_with_backoff(retry_config, || {
            let request = request.clone();
            async move {
                self.complete_responses_inner(request)
                    .await
                    .map_err(anyhow::Error::from)
            }
        })
        .await
        .map_err(|e: anyhow::Error| {
            if let Some(provider_err) = e.downcast_ref::<ProviderError>() {
                provider_err.clone()
            } else {
                ProviderError::Api(e.to_string())
            }
        })
    }

    /// Inner implementation for `complete_responses` (called by retry wrapper).
    async fn complete_responses_inner(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = get_api_key!(self, "OPENAI_API_KEY")?;

        let url = format!("{}/responses", self.endpoint());

        let (instructions, input_items) =
            crate::openai_compatible::convert_messages_to_responses_input(&request);

        let tools = request
            .tools
            .as_ref()
            .map(|t| crate::tools::normalize_tools_for_responses(t))
            .unwrap_or_default();
        let tools_opt = if tools.is_empty() {
            None
        } else {
            let parsed: Vec<crate::openai_compatible::ResponsesApiTool> = tools
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        };

        let prev_id = self
            .last_response_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let is_reasoning = Self::is_reasoning_model(&request.model);
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
                    crate::provider::EffortLevel::Max => "xhigh", // Responses API caps at xhigh
                };
                crate::openai_compatible::ResponsesApiReasoning {
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

        let body = crate::openai_compatible::ResponsesApiRequest {
            model: request.model.clone(),
            input: input_items,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            top_p: None,
            max_output_tokens: request.max_tokens,
            stream: Some(false),
            previous_response_id: prev_id,
            tool_choice: request.tool_choice,
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning,
            include,
            store: Some(true),
            prompt_cache_key: request.session_id.clone(),
        };

        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(crate::openai_compatible::map_http_error(
                status,
                text,
                &headers,
                "OpenAI",
                "OPENAI_API_KEY",
            ));
        }
        let resp: crate::openai_compatible::ResponsesApiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::api(format!("failed to parse response: {}", e)))?;

        // Store response ID for server-side conversation state
        if let Ok(mut guard) = self.last_response_id.lock() {
            *guard = Some(resp.id.clone());
        }

        crate::openai_compatible::build_responses_completion_response(&resp)
    }

    /// Stream a request using the Responses API (`POST /v1/responses` with `stream: true`).
    pub(super) async fn complete_responses_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>, ProviderError> {
        use futures::StreamExt;

        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                ProviderError::auth(
                    "OpenAI API key is required. Set api_key in config or OPENAI_API_KEY env var",
                )
            })?
            .expose_secret();

        let url = format!("{}/responses", self.endpoint());

        let (instructions, input_items) =
            crate::openai_compatible::convert_messages_to_responses_input(&request);

        let tools = request
            .tools
            .as_ref()
            .map(|t| crate::tools::normalize_tools_for_responses(t))
            .unwrap_or_default();
        let tools_opt = if tools.is_empty() {
            None
        } else {
            let parsed: Vec<crate::openai_compatible::ResponsesApiTool> = tools
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        };

        let prev_id = self
            .last_response_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let is_reasoning = Self::is_reasoning_model(&request.model);
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
                crate::openai_compatible::ResponsesApiReasoning {
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

        let body = crate::openai_compatible::ResponsesApiRequest {
            model: request.model.clone(),
            input: input_items,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            top_p: None,
            max_output_tokens: request.max_tokens,
            stream: Some(true),
            previous_response_id: prev_id,
            tool_choice: request.tool_choice,
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning,
            include,
            store: Some(true),
            prompt_cache_key: request.session_id.clone(),
        };

        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(crate::openai_compatible::map_http_error(
                status,
                error_text,
                &headers,
                "OpenAI",
                "OPENAI_API_KEY",
            ));
        }

        let bytes_stream = response.bytes_stream();
        let line_buffer = crate::sse::SseByteBuffer::new();
        let state = crate::openai_compatible::ResponsesSseState::default();

        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let state = state.clone();

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(e.to_string()))]);
                }
            };

            let lines = line_buffer.feed_chunk(&chunk);
            let joined = lines.join("\n");
            let events = crate::openai_compatible::parse_responses_sse_lines(&joined, &state);

            futures::stream::iter(events)
        });

        Ok(Box::pin(sse_stream))
    }

    /// Stream a Responses API request over WebSocket (feature-gated).
    ///
    /// Converts the HTTP endpoint to a WebSocket URL and uses the WS transport.
    #[cfg(feature = "ws")]
    pub(super) async fn complete_responses_ws(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>, ProviderError> {
        use secrecy::ExposeSecret;

        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                ProviderError::auth(
                    "OpenAI API key is required. Set api_key in config or OPENAI_API_KEY env var",
                )
            })?
            .expose_secret();

        // Convert HTTP endpoint to WebSocket URL
        let endpoint = self.endpoint();
        let base = endpoint.trim_end_matches('/');
        let ws_url = base
            .replace("https://", "wss://")
            .replace("http://", "ws://")
            + "/responses";

        let (instructions, input_items) =
            crate::openai_compatible::convert_messages_to_responses_input(&request);

        let tools = request
            .tools
            .as_ref()
            .map(|t| crate::tools::normalize_tools_for_responses(t))
            .unwrap_or_default();
        let tools_opt = if tools.is_empty() {
            None
        } else {
            let parsed: Vec<crate::openai_compatible::ResponsesApiTool> = tools
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        };

        let prev_id = self
            .last_response_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let is_reasoning = Self::is_reasoning_model(&request.model);
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
                crate::openai_compatible::ResponsesApiReasoning {
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

        let body = crate::openai_compatible::ResponsesApiRequest {
            model: request.model.clone(),
            input: input_items,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            top_p: None,
            max_output_tokens: request.max_tokens,
            stream: Some(true),
            previous_response_id: prev_id,
            tool_choice: request.tool_choice,
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning,
            include,
            store: Some(true),
            prompt_cache_key: request.session_id.clone(),
        };

        let body_json =
            serde_json::to_value(&body).map_err(|e| ProviderError::Serialization(e.to_string()))?;

        crate::openai_compatible::responses_ws::stream_responses_ws(&ws_url, api_key, body_json)
            .await
    }

    /// Check if an error from the Responses API indicates the endpoint is unavailable,
    /// signalling that Auto mode should fall back to Chat Completions.
    pub(super) fn is_responses_unsupported_error(err: &ProviderError) -> bool {
        matches!(err, ProviderError::InvalidModel(_))
            || matches!(err, ProviderError::Api(msg) if msg.starts_with("404"))
    }
}
