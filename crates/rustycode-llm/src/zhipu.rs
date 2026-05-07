use crate::provider::{
    build_openai_response_format, CompletionRequest, CompletionResponse, LLMProvider,
    ProviderConfig, ProviderError, StreamChunk, Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::retry::extract_retry_after_ms;
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use rustycode_protocol::stream_event::StreamEvent;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::time::Duration;

const ZHIPU_DEFAULT_ENDPOINT: &str = "https://api.z.ai/api/coding/paas/v4";

#[derive(Serialize)]
struct ZhipuRequest {
    model: String,
    messages: Vec<ZhipuMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ZhipuMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ZhipuResponse {
    choices: Vec<ZhipuChoice>,
    usage: Option<ZhipuUsage>,
    model: String,
}

#[derive(Deserialize)]
struct ZhipuChoice {
    message: ZhipuResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ZhipuResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ZhipuToolCall>>,
}

#[derive(Deserialize, Serialize, Clone)]
struct ZhipuToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: ZhipuFunction,
}

#[derive(Deserialize, Serialize, Clone)]
struct ZhipuFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ZhipuUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: Option<ZhipuPromptTokensDetails>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ZhipuPromptTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

pub struct ZhipuProvider {
    config: ProviderConfig,
    client: Client,
}

impl ZhipuProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let timeout_secs = config.timeout_seconds.unwrap_or(300);
        Ok(Self {
            config,
            client: Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
        })
    }

    fn endpoint(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| ZHIPU_DEFAULT_ENDPOINT.to_string())
    }

    fn api_key(&self) -> Result<String, ProviderError> {
        self.config
            .api_key
            .as_ref()
            .ok_or_else(|| ProviderError::auth("ZHIPU_API_KEY is required"))
            .map(|k| k.expose_secret().to_string())
    }

    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "zhipu".to_string(),
            display_name: "Zhipu AI".to_string(),
            description: "GLM models from Zhipu AI".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    description: "Your Zhipu AI API key from z.ai".to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("...".to_string()),
                    default: None,
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "API endpoint (defaults to https://api.z.ai/api/paas/v4)"
                        .to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some(ZHIPU_DEFAULT_ENDPOINT.to_string()),
                    default: Some(ZHIPU_DEFAULT_ENDPOINT.to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "ZHIPU_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant powered by GLM.".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![],
                },
                tool_format: ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: Some(128),
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "glm-5".to_string(),
                    display_name: "GLM-5".to_string(),
                    description: "Latest flagship model with agentic capabilities".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Complex reasoning".to_string(),
                        "Coding".to_string(),
                        "Agent workflows".to_string(),
                    ],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "glm-4-plus".to_string(),
                    display_name: "GLM-4 Plus".to_string(),
                    description: "High-capability GLM-4 model".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["General tasks".to_string(), "Coding".to_string()],
                    cost_tier: 2,
                },
                ModelInfo {
                    model_id: "glm-4-flash".to_string(),
                    display_name: "GLM-4 Flash".to_string(),
                    description: "Fast, cost-effective GLM-4 model".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Quick tasks".to_string(),
                        "High-volume workloads".to_string(),
                    ],
                    cost_tier: 1,
                },
            ],
        }
    }
}

#[async_trait]
impl LLMProvider for ZhipuProvider {
    fn name(&self) -> &'static str {
        "zhipu"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/models", self.endpoint());
        match self
            .client
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key().unwrap_or_default()),
            )
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/models", self.endpoint());
        let req = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key()?));
        let response = req
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to connect to Zhipu: {}", e)))?;
        if !response.status().is_success() {
            return Err(ProviderError::Api(format!(
                "Zhipu API returned status {}",
                response.status()
            )));
        }
        #[derive(Deserialize)]
        struct ZhipuModelsResponse {
            data: Vec<ZhipuModel>,
        }
        #[derive(Deserialize)]
        struct ZhipuModel {
            id: String,
        }
        let models: ZhipuModelsResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;
        Ok(models.data.into_iter().map(|m| m.id).collect())
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = self.api_key()?;
        let url = format!("{}/chat/completions", self.endpoint());
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(ZhipuMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }
        for msg in &request.messages {
            let role = match msg.role.as_ref() {
                "user" => "user",
                "assistant" => "assistant",
                "system" => "system",
                "tool" => "tool",
                _ => "user",
            };
            messages.push(ZhipuMessage {
                role: role.to_string(),
                content: msg.content.to_text(),
            });
        }
        let thinking = if request.model.starts_with("glm-5") {
            Some(serde_json::json!({"type": "enabled"}))
        } else {
            None
        };
        let body = ZhipuRequest {
            model: request.model.clone(),
            messages,
            stream: false,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: request.tools.as_ref().map(|t| {
                let normalized = crate::tools::normalize_tools_for_openai(t);
                let sanitized = crate::tools::sanitize_tools_for_strict_providers(&normalized);
                serde_json::json!(sanitized)
            }),
            response_format: build_openai_response_format(&request.output_config),
            thinking,
        };
        let req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json");
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
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::auth(format!("Authentication failed: {}", error_text)),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => {
                    ProviderError::Network(format!("Zhipu service unavailable: {}", error_text))
                }
                _ => ProviderError::api(format!("{}: {}", status, error_text)),
            });
        }
        let resp: ZhipuResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;
        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::api("no choices in response"))?;
        let mut content = choice.message.content.unwrap_or_default();
        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let tool_calls_json: Vec<serde_json::Value> = tool_calls.iter().map(|tc| {
                    serde_json::json!({"id": tc.id, "type": tc.tool_type, "function": {"name": tc.function.name, "arguments": tc.function.arguments}})
                }).collect();
                let formatted = serde_json::to_string_pretty(&tool_calls_json)
                    .unwrap_or_else(|_| "[]".to_string());
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&format!("```tool\n{}\n```", formatted));
            }
        }
        Ok(CompletionResponse {
            content,
            model: resp.model,
            usage: resp.usage.map(|u| Usage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: crate::provider::normalize_stop_reason(choice.finish_reason.as_deref()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self.api_key()?;
        let url = format!("{}/chat/completions", self.endpoint());
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(ZhipuMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }
        for msg in &request.messages {
            let role = match msg.role.as_ref() {
                "user" => "user",
                "assistant" => "assistant",
                "system" => "system",
                "tool" => "tool",
                _ => "user",
            };
            messages.push(ZhipuMessage {
                role: role.to_string(),
                content: msg.content.to_text(),
            });
        }
        let thinking = if request.model.starts_with("glm-5") {
            Some(serde_json::json!({"type": "enabled"}))
        } else {
            None
        };
        let body = ZhipuRequest {
            model: request.model.clone(),
            messages,
            stream: true,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: request.tools.as_ref().map(|t| {
                let normalized = crate::tools::normalize_tools_for_openai(t);
                let sanitized = crate::tools::sanitize_tools_for_strict_providers(&normalized);
                serde_json::json!(sanitized)
            }),
            response_format: build_openai_response_format(&request.output_config),
            thinking,
        };
        let req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json");
        // Debug: log the request body (truncated) to inspect what's being sent
        if tracing::enabled!(tracing::Level::DEBUG) {
            let body_json = serde_json::to_string(&body).unwrap_or_default();
            let preview = if body_json.len() > 2000 {
                let end = body_json.floor_char_boundary(2000);
                format!(
                    "{}... ({} bytes total)",
                    &body_json[..end],
                    body_json.len()
                )
            } else {
                body_json
            };
            tracing::debug!(model = %request.model, url = %url, "Zhipu streaming request body: {}", preview);
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;
        let status = response.status();
        tracing::debug!(status = %status, "Zhipu streaming response status");
        if !status.is_success() {
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            tracing::warn!(status = %status, error = %error_text, "Zhipu streaming error response");
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::auth(format!("Authentication failed: {}", error_text)),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => {
                    ProviderError::Network(format!("Zhipu service unavailable: {}", error_text))
                }
                _ => ProviderError::api(format!("{}: {}", status, error_text)),
            });
        }
        let bytes_stream = response.bytes_stream();
        let done_sent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tool_ids_by_index =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
                usize,
                String,
            >::new()));
        let started_tool_indices =
            std::sync::Arc::new(std::sync::Mutex::new(HashSet::<usize>::new()));
        let line_buffer = crate::sse::SseByteBuffer::new();
        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let done_sent = done_sent.clone();
            let tool_ids_by_index = tool_ids_by_index.clone();
            let started_tool_indices = started_tool_indices.clone();
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(e.to_string()))])
                }
            };
            let mut events = Vec::new();
            let lines = line_buffer.feed_chunk(&chunk);
            // Debug: log each raw SSE line to inspect server response
            if tracing::enabled!(tracing::Level::DEBUG) {
                for line in &lines {
                    if !line.is_empty() {
                        tracing::debug!(raw_sse = %line, "Zhipu SSE line");
                    }
                }
            }
            for line in &lines {
                if line.is_empty() {
                    continue;
                }
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        if !done_sent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            events.push(Ok(StreamEvent::Done));
                        }
                        continue;
                    }
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content") {
                                        if let Some(content_str) = content.as_str() {
                                            if !content_str.is_empty() {
                                                tracing::trace!(len = content_str.len(), content = %content_str, "Zhipu text delta");
                                                events.push(Ok(StreamEvent::TextDelta {
                                                    content: content_str.to_string(),
                                                }));
                                            }
                                        }
                                    }

                                    if let Some(reasoning) = delta.get("reasoning_content") {
                                        if let Some(reasoning_str) = reasoning.as_str() {
                                            if !reasoning_str.is_empty() {
                                                events.push(Ok(StreamEvent::ThinkingDelta {
                                                    content: reasoning_str.to_string(),
                                                }));
                                            }
                                        }
                                    }

                                    if let Some(reasoning) = delta.get("reasoning") {
                                        if let Some(reasoning_str) = reasoning.as_str() {
                                            if !reasoning_str.is_empty() {
                                                events.push(Ok(StreamEvent::ThinkingDelta {
                                                    content: reasoning_str.to_string(),
                                                }));
                                            }
                                        }
                                    }

                                    if let Some(tool_calls) =
                                        delta.get("tool_calls").and_then(|tc| tc.as_array())
                                    {
                                        for tc_delta in tool_calls {
                                            let index = tc_delta
                                                .get("index")
                                                .and_then(|i| i.as_u64())
                                                .unwrap_or(0) as usize;

                                            if let Some(id) = tc_delta.get("id").and_then(|i| i.as_str()) {
                                                if let Ok(mut ids) = tool_ids_by_index.lock() {
                                                    ids.insert(index, id.to_string());
                                                }
                                            }

                                            let resolved_id = tc_delta
                                                .get("id")
                                                .and_then(|i| i.as_str())
                                                .map(ToString::to_string)
                                                .or_else(|| {
                                                    tool_ids_by_index
                                                        .lock()
                                                        .ok()
                                                        .and_then(|ids| ids.get(&index).cloned())
                                                });

                                            if let Some(id) = resolved_id {
                                                let should_emit_start = {
                                                    if let Ok(mut started) = started_tool_indices.lock() {
                                                        started.insert(index)
                                                    } else {
                                                        false
                                                    }
                                                };
                                                if should_emit_start {
                                                    let name = tc_delta
                                                        .get("function")
                                                        .and_then(|f| f.get("name"))
                                                        .and_then(|n| n.as_str())
                                                        .unwrap_or("")
                                                        .to_string();
                                                    events.push(Ok(StreamEvent::ToolCallStarted {
                                                        id: id.clone(),
                                                        name,
                                                    }));
                                                }
                                            }

                                            if let Some(partial) = tc_delta
                                                .get("function")
                                                .and_then(|f| f.get("arguments"))
                                                .and_then(|a| a.as_str())
                                            {
                                                if !partial.is_empty() {
                                                    if let Some(id) = tc_delta
                                                        .get("id")
                                                        .and_then(|i| i.as_str())
                                                        .map(ToString::to_string)
                                                        .or_else(|| {
                                                            tool_ids_by_index
                                                                .lock()
                                                                .ok()
                                                                .and_then(|ids| ids.get(&index).cloned())
                                                        })
                                                    {
                                                        events.push(Ok(StreamEvent::ToolInputDelta {
                                                            id,
                                                            chunk: partial.to_string(),
                                                        }));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Some(finish_reason) =
                                    choice.get("finish_reason").and_then(|f| f.as_str())
                                {
                                    let usage = data.get("usage").and_then(|u| {
                                        let input_tokens = u.get("prompt_tokens")?.as_u64()? as u32;
                                        let output_tokens =
                                            u.get("completion_tokens")?.as_u64()? as u32;
                                        Some(Usage {
                                            input_tokens,
                                            output_tokens,
                                            total_tokens: input_tokens
                                                .saturating_add(output_tokens),
                                            cache_read_input_tokens: u
                                                .get("prompt_tokens_details")
                                                .and_then(|d| d.get("cached_tokens"))
                                                .and_then(|t| t.as_u64())
                                                .unwrap_or(0)
                                                as u32,
                                            cache_creation_input_tokens: 0,
                                            reasoning_tokens: None,
                                        })
                                    });
                                    if let Some(usage) = usage {
                                        events.push(Ok(StreamEvent::TokenUsage {
                                            input_tokens: u64::from(usage.input_tokens),
                                            output_tokens: u64::from(usage.output_tokens),
                                        }));
                                    }
                                    let stop_reason = crate::provider::normalize_stop_reason(Some(
                                        finish_reason,
                                    ))
                                    .unwrap_or_else(|| finish_reason.to_string());
                                    events.push(Ok(StreamEvent::TurnCompleted {
                                        stop_reason,
                                    }));
                                    if !done_sent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                                        events.push(Ok(StreamEvent::Done));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            futures::stream::iter(events)
        });
        Ok(Box::pin(sse_stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;
    #[test]
    fn test_zhipu_provider_creation() {
        let config = ProviderConfig {
            api_key: Some(SecretString::new("test-key".into())),
            ..Default::default()
        };
        let provider = ZhipuProvider::new(config);
        assert!(provider.is_ok());
    }
    #[test]
    fn test_metadata_display_name() {
        let metadata = ZhipuProvider::metadata();
        assert_eq!(metadata.display_name, "Zhipu AI");
        assert_eq!(metadata.provider_id, "zhipu");
    }
    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = ZhipuProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
    }

    // --- Config validation ---

    #[test]
    fn test_missing_api_key_fails() {
        let config = ProviderConfig {
            api_key: None,
            ..Default::default()
        };
        let result = ZhipuProvider::new(config);
        assert!(result.is_err());
    }

    // --- Availability (config-level only; network calls need mocks) ---

    #[test]
    fn test_provider_creation_with_valid_key() {
        let config = ProviderConfig {
            api_key: Some(SecretString::new("valid-key".into())),
            ..Default::default()
        };
        let provider = ZhipuProvider::new(config);
        assert!(provider.is_ok());
    }

    // --- Metadata required fields ---

    #[test]
    fn test_metadata_has_api_key_field() {
        let meta = ZhipuProvider::metadata();
        assert!(
            meta.config_schema
                .required_fields
                .iter()
                .any(|f| f.name == "api_key"),
            "api_key should be required"
        );
    }

    // --- Request serialization ---

    #[test]
    fn test_zhipu_request_serialization() {
        let req = ZhipuRequest {
            model: "glm-4".to_string(),
            messages: vec![ZhipuMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            stream: false,
            temperature: Some(0.7),
            max_tokens: Some(2048),
            tools: None,
            response_format: None,
            thinking: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "glm-4");
        assert_eq!(json["max_tokens"], 2048);
        assert!((json["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
        assert_eq!(json["stream"], false);
    }

    // --- Response deserialization ---

    #[test]
    fn test_zhipu_response_deserialization() {
        let json = r#"{
            "id": "test-123",
            "model": "glm-4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hi there!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let resp: ZhipuResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model, "glm-4");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(
            resp.choices[0].message.content,
            Some("Hi there!".to_string())
        );
        assert_eq!(resp.usage.as_ref().unwrap().total_tokens, 15);
    }

    // --- Config access ---

    #[test]
    fn test_config_returns_some() {
        let config = ProviderConfig {
            api_key: Some(SecretString::new("test-key".into())),
            ..Default::default()
        };
        let provider = ZhipuProvider::new(config).unwrap();
        assert!(provider.config().is_some());
    }
}
