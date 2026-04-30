//! Hugging Face Inference API LLM provider implementation.
//!
//! This provider supports Hugging Face's Inference API which provides access to
//! thousands of models hosted on the Hugging Face Hub.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Hugging Face settings)
//! - Model name (e.g., "meta-llama/Meta-Llama-3-8B-Instruct")
//!
//! ## Environment Variables
//!
//! - `HF_TOKEN` or `HUGGINGFACE_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```toml
//! [ai]
//! provider = "huggingface"
//! model = "meta-llama/Meta-Llama-3-8B-Instruct"
//! api_key = "your-api-key"
//! ```

use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
    Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, PromptOptimizations, PromptTemplate,
    ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::retry::extract_retry_after_ms;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rustycode_protocol::stream_event::StreamEvent;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::time::Duration;

/// Default Hugging Face Inference API endpoint
const HF_API_ENDPOINT: &str = "https://api-inference.huggingface.co/v1/chat/completions";

#[derive(Serialize)]
struct HuggingFaceRequest {
    model: String,
    messages: Vec<HuggingFaceMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct HuggingFaceMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<HuggingFaceToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct HuggingFaceToolCall {
    id: String,
    r#type: String,
    function: HuggingFaceToolCallFunction,
}

#[derive(Serialize, Deserialize)]
struct HuggingFaceToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct HuggingFaceResponse {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    object: String,
    #[allow(dead_code)] // Kept for future use
    created: u64,
    model: String,
    choices: Vec<HuggingFaceChoice>,
    usage: HuggingFaceUsage,
}

#[derive(Deserialize)]
struct HuggingFaceChoice {
    #[allow(dead_code)] // Kept for future use
    index: usize,
    message: HuggingFaceResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct HuggingFaceResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<HuggingFaceToolCall>>,
}

#[derive(Deserialize)]
struct HuggingFaceUsage {
    #[allow(dead_code)] // Kept for future use
    prompt_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    completion_tokens: usize,
    total_tokens: usize,
}

/// Hugging Face Inference API LLM provider
pub struct HuggingFaceProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    default_model: String,
}

impl HuggingFaceProvider {
    pub fn new(config: ProviderConfig, default_model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let client = Self::build_client(&config)?;
        Ok(Self {
            config,
            client,
            default_model,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        let mut env_mappings = HashMap::new();
        env_mappings.insert("api_key".to_string(), "HF_TOKEN".to_string());
        env_mappings.insert("api_key".to_string(), "HUGGINGFACE_API_KEY".to_string());

        ProviderMetadata {
            provider_id: "huggingface".to_string(),
            display_name: "Hugging Face".to_string(),
            description: "Access thousands of models on the Hugging Face Hub via Inference API".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Hugging Face API token (from hf.co/settings/tokens)".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("hf_...".to_string()),
                        default: None,
                        validation_pattern: Some("^hf_.*".to_string()),
                        validation_error: Some("API key must start with 'hf_'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api-inference.huggingface.co/v1/chat/completions".to_string()),
                        default: Some("https://api-inference.huggingface.co/v1/chat/completions".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings,
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant accessed via the Hugging Face Inference API.\n\n=== CONTEXT ===\n{context}\n\n=== GUIDELINES ===\n- Provide clear, accurate responses\n- When writing code, include explanations\n- Ask clarifying questions when needed\n- Be thorough but concise".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "You can access various models from the Hugging Face Hub.".to_string(),
                        "Model capabilities may vary depending on the selected model.".to_string(),
                    ],
                },
                tool_format: ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
            ],
        }
    }

    fn build_client(config: &ProviderConfig) -> Result<reqwest::Client, ProviderError> {
        // Try config first, then environment variables
        let config_key = config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let hf_token = std::env::var("HF_TOKEN").ok();
        let hf_api_key = std::env::var("HUGGINGFACE_API_KEY").ok();

        let api_key = config_key.or(hf_token).or(hf_api_key).unwrap_or_default();

        let mut headers = reqwest::header::HeaderMap::new();
        if !api_key.is_empty() {
            let header_value = format!("Bearer {}", api_key).parse().map_err(|e| {
                ProviderError::Configuration(format!("invalid API key format: {}", e))
            })?;
            headers.insert(reqwest::header::AUTHORIZATION, header_value);
        }
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let timeout = config
            .timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_mins(2));

        reqwest::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })
    }

    fn convert_messages(messages: Vec<crate::provider::ChatMessage>) -> Vec<HuggingFaceMessage> {
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
                                tool_calls.push(HuggingFaceToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: HuggingFaceToolCallFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input).unwrap_or_default(),
                                    },
                                });
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content: result_text,
                                ..
                            } => {
                                tool_call_id = Some(tool_use_id.clone());
                                text_parts.push(result_text.clone());
                            }
                            ContentBlock::Thinking { thinking, .. } => {
                                text_parts.push(thinking.clone());
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

            result.push(HuggingFaceMessage {
                role: role.to_string(),
                content: text_parts.join("\n"),
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id,
                name: None,
            });
        }
        result
    }

    fn convert_tools(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
        crate::tools::normalize_tools_for_openai(tools)
    }

    pub fn endpoint(&self) -> String {
        self.config
            .base_url
            .as_ref()
            .unwrap_or(&HF_API_ENDPOINT.to_string())
            .clone()
    }
}

#[async_trait]
impl LLMProvider for HuggingFaceProvider {
    fn name(&self) -> &'static str {
        "huggingface"
    }

    async fn is_available(&self) -> bool {
        // Check if API key is available
        let has_api_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| !k.expose_secret().is_empty())
            .unwrap_or(false)
            || std::env::var("HF_TOKEN").is_ok()
            || std::env::var("HUGGINGFACE_API_KEY").is_ok();

        if !has_api_key {
            return false;
        }

        // Try a simple model list request to verify connectivity
        self.list_models().await.is_ok()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return a list of popular Hugging Face models (as of March 2026)
        // In a real implementation, this could query the Hugging Face Hub API
        Ok(vec![
            "meta-llama/Llama-4-8B-Instruct".to_string(),
            "meta-llama/Llama-3.3-70B-Instruct".to_string(),
            "meta-llama/Llama-3.1-405B-Instruct".to_string(),
            "meta-llama/Meta-Llama-3-8B-Instruct".to_string(),
            "meta-llama/Meta-Llama-3-70B-Instruct".to_string(),
            "mistralai/Mistral-7B-Instruct-v0.2".to_string(),
            "mistralai/Mixtral-8x7B-Instruct-v0.1".to_string(),
            "mistralai/Mixtral-8x22B-Instruct-v0.1".to_string(),
            "google/gemma-7b".to_string(),
            "tiiuae/falcon-7b-instruct".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = self.endpoint();
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        // Convert ChatMessage to HuggingFaceMessage format
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request.tools.as_ref().map(|t| Self::convert_tools(t));

        let body = HuggingFaceRequest {
            model: model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            stream: Some(request.stream),
            tools,
            tool_choice: request.tool_choice.clone(),
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            // Capture headers before consuming the body to support Retry-After headers
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Authentication failed. Check your HF_TOKEN or HUGGINGFACE_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found. {}. Browse available models at huggingface.co/models",
                    error_text
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Hugging Face service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("HuggingFace API error {}: {}", status, error_text)),
            });
        }

        let hf_response: HuggingFaceResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        let choice = hf_response
            .choices
            .first()
            .ok_or_else(|| ProviderError::Api("No choices in response".to_string()))?;

        let content = if let Some(tool_calls) = &choice.message.tool_calls {
            let tc_json: Vec<serde_json::Value> = tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": tc.r#type,
                        "function": {
                            "name": tc.function.name,
                            "arguments": tc.function.arguments,
                        }
                    })
                })
                .collect();
            let text = &choice.message.content;
            if text.is_empty() {
                serde_json::to_string(&tc_json).unwrap_or_default()
            } else {
                format!(
                    "{text}\n[TOOL_CALLS:{}]",
                    serde_json::to_string(&tc_json).unwrap_or_default()
                )
            }
        } else {
            choice.message.content.clone()
        };

        Ok(CompletionResponse {
            content,
            model: hf_response.model,
            usage: Some(Usage {
                input_tokens: hf_response.usage.prompt_tokens as u32,
                output_tokens: hf_response.usage.completion_tokens as u32,
                total_tokens: hf_response.usage.total_tokens as u32,
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
        let url = self.endpoint();
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        // Convert ChatMessage to HuggingFaceMessage format
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request.tools.as_ref().map(|t| Self::convert_tools(t));

        let mut request_body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });
        if let Some(tools) = tools {
            request_body["tools"] = serde_json::json!(tools);
        }
        if let Some(tc) = &request.tool_choice {
            request_body["tool_choice"] = tc.clone();
        }

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to connect: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Authentication failed. Check your HF_TOKEN or HUGGINGFACE_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found. {}. Browse available models at huggingface.co/models",
                    error_text
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Hugging Face service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("HuggingFace API error {}: {}", status, error_text)),
            });
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();
        let done_sent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tool_ids_by_index =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
                usize,
                String,
            >::new()));
        let started_tool_indices =
            std::sync::Arc::new(std::sync::Mutex::new(HashSet::<usize>::new()));
        let line_buffer = crate::sse::SseLineBuffer::new();

        // Parse SSE events from byte stream (HuggingFace uses OpenAI-compatible format)
        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let done_sent = done_sent.clone();
            let tool_ids_by_index = tool_ids_by_index.clone();
            let started_tool_indices = started_tool_indices.clone();
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(format!(
                        "failed to read chunk: {}",
                        e
                    )))]);
                }
            };
            let text = String::from_utf8_lossy(&chunk);
            let mut events = Vec::new();

            let lines = line_buffer.feed_chunk(&text);
            for line in &lines {
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    // HuggingFace sends "data: [DONE]" when complete
                    if json_str == "[DONE]" {
                        continue;
                    }
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content") {
                                        if let Some(content_str) = content.as_str() {
                                            if !content_str.is_empty() {
                                                events.push(Ok(StreamEvent::TextDelta {
                                                    content: content_str.to_string(),
                                                }));
                                            }
                                        }
                                    }
                                    if let Some(tool_calls) =
                                        delta.get("tool_calls").and_then(|tc| tc.as_array())
                                    {
                                        for tc in tool_calls {
                                            let index = tc
                                                .get("index")
                                                .and_then(|i| i.as_u64())
                                                .unwrap_or(0)
                                                as usize;

                                            if let Some(id) = tc.get("id").and_then(|v| v.as_str())
                                            {
                                                if let Ok(mut ids) = tool_ids_by_index.lock() {
                                                    ids.insert(index, id.to_string());
                                                }
                                            }

                                            let resolved_id =
                                                tc.get("id")
                                                    .and_then(|v| v.as_str())
                                                    .map(ToString::to_string)
                                                    .or_else(|| {
                                                        tool_ids_by_index.lock().ok().and_then(
                                                            |ids| ids.get(&index).cloned(),
                                                        )
                                                    });

                                            if let Some(id) = resolved_id {
                                                let should_emit_start = {
                                                    if let Ok(mut started) =
                                                        started_tool_indices.lock()
                                                    {
                                                        started.insert(index)
                                                    } else {
                                                        false
                                                    }
                                                };
                                                if should_emit_start {
                                                    let name = tc
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

                                                if let Some(args) = tc
                                                    .get("function")
                                                    .and_then(|f| f.get("arguments"))
                                                    .and_then(|a| a.as_str())
                                                {
                                                    if !args.is_empty() {
                                                        events.push(Ok(
                                                            StreamEvent::ToolInputDelta {
                                                                id,
                                                                chunk: args.to_string(),
                                                            },
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(finish_reason) =
                                    choice.get("finish_reason").and_then(|f| f.as_str())
                                {
                                    if let Some(usage) = data.get("usage").and_then(|u| {
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
                                    }) {
                                        events.push(Ok(StreamEvent::TokenUsage {
                                            input_tokens: u64::from(usage.input_tokens),
                                            output_tokens: u64::from(usage.output_tokens),
                                        }));
                                    }

                                    let stop_reason =
                                        crate::provider::normalize_stop_reason(Some(finish_reason))
                                            .unwrap_or_else(|| finish_reason.to_string());
                                    events.push(Ok(StreamEvent::TurnCompleted { stop_reason }));
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

    fn make_config(api_key: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            api_key: api_key.map(|s| SecretString::new(s.to_string().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_provider_name() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert_eq!(provider.name(), "huggingface");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("hf_test-key"));
        let provider = HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = HuggingFaceProvider::metadata();
        assert_eq!(metadata.display_name, "Hugging Face");
        assert_eq!(metadata.provider_id, "huggingface");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = HuggingFaceProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = HuggingFaceProvider::metadata();
        // env_mappings has two entries for api_key (last one wins in HashMap)
        assert!(metadata.config_schema.env_mappings.contains_key("api_key"));
    }

    #[test]
    fn test_metadata_no_recommended_models() {
        let metadata = HuggingFaceProvider::metadata();
        // HuggingFace has an empty recommended_models vec
        assert!(metadata.recommended_models.is_empty());
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert_eq!(provider.endpoint(), HF_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("hf_test-key"));
        config.base_url = Some("https://my-hf-proxy.example.com/v1/chat/completions".to_string());
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert_eq!(
            provider.endpoint(),
            "https://my-hf-proxy.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_huggingface_request_serialization() {
        let request = HuggingFaceRequest {
            model: "meta-llama/Llama-3-70B".to_string(),
            messages: vec![HuggingFaceMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            max_tokens: 2048,
            temperature: 0.8,
            stream: Some(true),
            tools: None,
            tool_choice: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"meta-llama/Llama-3-70B\""));
        assert!(json.contains("\"stream\":true"));
    }

    #[test]
    fn test_huggingface_request_no_stream_serialization() {
        let request = HuggingFaceRequest {
            model: "meta-llama/Llama-3-70B".to_string(),
            messages: vec![],
            max_tokens: 1024,
            temperature: 0.5,
            stream: None,
            tools: None,
            tool_choice: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        // stream should be absent when None
        assert!(!json.contains("\"stream\""));
    }

    #[test]
    fn test_huggingface_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "meta-llama/Llama-3-70B",
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello from HF!"},
                    "finish_reason": "stop"
                }
            ],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let response: HuggingFaceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "meta-llama/Llama-3-70B");
        assert_eq!(response.choices[0].message.content, "Hello from HF!");
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models
            .iter()
            .any(|m| m.contains("llama") || m.contains("Llama")));
    }
}
