//! Mistral AI LLM provider implementation.
//!
//! This provider supports Mistral AI's API which provides access to
//! language models like Mistral 7B, Mixtral 8x7B, Mistral Large, etc.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Mistral AI dashboard)
//! - Model name (e.g., "mistral-large-latest", "mixtral-8x7b-2707")
//!
//! ## Environment Variables
//!
//! - `MISTRAL_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```toml
//! [ai]
//! provider = "mistral"
//! model = "mistral-large-latest"
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
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default Mistral AI API endpoint
const MISTRAL_API_ENDPOINT: &str = "https://api.mistral.ai/v1/chat/completions";

#[derive(Serialize)]
struct MistralRequest {
    model: String,
    messages: Vec<MistralMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct MistralMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<MistralToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct MistralToolCall {
    id: String,
    r#type: String,
    function: MistralToolCallFunction,
}

#[derive(Serialize, Deserialize)]
struct MistralToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct MistralResponse {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    object: String,
    #[allow(dead_code)] // Kept for future use
    created: u64,
    model: String,
    choices: Vec<MistralChoice>,
    usage: MistralUsage,
}

#[derive(Deserialize)]
struct MistralChoice {
    #[allow(dead_code)] // Kept for future use
    index: usize,
    message: MistralResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct MistralResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<MistralToolCall>>,
}

#[derive(Deserialize)]
struct MistralUsage {
    #[allow(dead_code)] // Kept for future use
    prompt_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    completion_tokens: usize,
    total_tokens: usize,
}

/// Mistral AI LLM provider
pub struct MistralProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    default_model: String,
}

impl MistralProvider {
    pub fn new(config: ProviderConfig, model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        // Try config first, then environment variable
        let config_key = config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("MISTRAL_API_KEY").ok();

        let api_key = config_key.or(env_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Mistral API key is required. Set api_key in config or MISTRAL_API_KEY env var"
                    .to_string(),
            )
        })?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", api_key).parse().map_err(|e| {
                ProviderError::Configuration(format!("invalid API key format: {}", e))
            })?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(120));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .map_err(|e| ProviderError::Network(format!("failed to build HTTP client: {}", e)))?;

        Ok(Self {
            config,
            client,
            default_model: model,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        let mut env_mappings = HashMap::new();
        env_mappings.insert("api_key".to_string(), "MISTRAL_API_KEY".to_string());

        ProviderMetadata {
            provider_id: "mistral".to_string(),
            display_name: "Mistral AI".to_string(),
            description: "Advanced AI models including Mistral Large, Mixtral, and Codestral".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Mistral API key from console.mistral.ai".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("Your API key".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        default: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings,
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant provided by Mistral AI.\n\n=== CONTEXT ===\n{context}\n\n=== GUIDELINES ===\n- Provide clear, accurate, and well-structured responses\n- When writing code, include explanations and best practices\n- Ask clarifying questions when requirements are ambiguous\n- Be thorough but concise in your answers".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Mistral models excel at reasoning and code generation.".to_string(),
                        "Use structured formatting for complex responses.".to_string(),
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

    fn convert_messages(messages: Vec<crate::provider::ChatMessage>) -> Vec<MistralMessage> {
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
            let tool_name = None;

            match &msg.content {
                MessageContent::Simple(t) => text_parts.push(t.clone()),
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse {
                                id, name, input, ..
                            } => {
                                tool_calls.push(MistralToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: MistralToolCallFunction {
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

            result.push(MistralMessage {
                role: role.to_string(),
                content: text_parts.join("\n"),
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id,
                name: tool_name,
            });
        }
        result
    }

    fn convert_tools(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
        crate::tools::normalize_tools_for_openai(tools)
    }

    fn endpoint(&self) -> String {
        self.config
            .base_url
            .as_ref()
            .unwrap_or(&MISTRAL_API_ENDPOINT.to_string())
            .clone()
    }

    fn get_api_key(&self) -> Result<String, ProviderError> {
        let config_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("MISTRAL_API_KEY").ok();
        config_key.or(env_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Mistral API key is required. Set api_key in config or MISTRAL_API_KEY env var"
                    .to_string(),
            )
        })
    }
}

#[async_trait]
impl LLMProvider for MistralProvider {
    fn name(&self) -> &'static str {
        "mistral"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return known Mistral models (as of March 2026)
        Ok(vec![
            "mistral-large-2407".to_string(),
            "mixtral-8x22b-2407".to_string(),
            "mixtral-8x7b-2407".to_string(),
            "mistral-medium-2312".to_string(),
            "mistral-small-2409".to_string(),
            "codestral-2405".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let messages = Self::convert_messages(request.messages.clone());

        let tools = request.tools.as_ref().map(|t| Self::convert_tools(t));
        let body = MistralRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            tools,
            tool_choice: request.tool_choice.clone(),
        };

        let api_key = self.get_api_key()?;

        let response = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to send request: {}", e)))?;

        // Clone headers for potential retry logic (e.g., 429 with Retry-After)
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your MISTRAL_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(error_text.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Mistral service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let mistral_response: MistralResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        let choice = mistral_response
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
            model: mistral_response.model,
            usage: Some(Usage {
                input_tokens: mistral_response.usage.prompt_tokens as u32, // usize→u32 safe: token counts never exceed 2^32
                output_tokens: mistral_response.usage.completion_tokens as u32,
                total_tokens: mistral_response.usage.total_tokens as u32,
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
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request.tools.as_ref().map(|t| Self::convert_tools(t));

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });
        if let Some(tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }
        if let Some(tc) = &request.tool_choice {
            body["tool_choice"] = tc.clone();
        }

        let api_key = self.get_api_key()?;

        let response = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to connect to Mistral: {}", e)))?;
        // Clone headers for potential retry logic (e.g., 429 with Retry-After)
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your MISTRAL_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(error_text.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Mistral service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let bytes_stream = response.bytes_stream();
        let line_buffer = crate::sse::SseLineBuffer::new();

        let sse_stream = bytes_stream.map(move |chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("Failed to read chunk: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);
            let mut chunks = Vec::new();

            let lines = line_buffer.feed_chunk(&text);
            for line in &lines {
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        continue;
                    }
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content") {
                                        if let Some(content_str) = content.as_str() {
                                            chunks.push(content_str.to_string());
                                        }
                                    }
                                    // Handle tool_calls in streaming delta
                                    if let Some(tool_calls) =
                                        delta.get("tool_calls").and_then(|tc| tc.as_array())
                                    {
                                        for tc in tool_calls {
                                            let tc_id =
                                                tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                            let func = tc.get("function");
                                            let name = func
                                                .and_then(|f| f.get("name"))
                                                .and_then(|n| n.as_str())
                                                .unwrap_or("");
                                            let args = func
                                                .and_then(|f| f.get("arguments"))
                                                .and_then(|a| a.as_str())
                                                .unwrap_or("");
                                            if !name.is_empty() {
                                                chunks.push(format!(
                                                    "[TOOL_CALL:{}]",
                                                    serde_json::to_string(&serde_json::json!({
                                                        "id": tc_id,
                                                        "type": "function",
                                                        "function": {
                                                            "name": name,
                                                            "arguments": args,
                                                        }
                                                    }))
                                                    .unwrap_or_default()
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta {
                content: chunks.join(""),
            })
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
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.name(), "mistral");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(result.is_err());
    }

    // --- Metadata ---

    #[test]
    fn test_metadata_fields() {
        let meta = MistralProvider::metadata();
        assert_eq!(meta.provider_id, "mistral");
        assert_eq!(meta.display_name, "Mistral AI");
        assert!(!meta.config_schema.required_fields.is_empty());
        assert!(
            meta.config_schema
                .required_fields
                .iter()
                .any(|f| f.name == "api_key"),
            "api_key should be required"
        );
    }

    #[test]
    fn test_metadata_env_mapping() {
        let meta = MistralProvider::metadata();
        assert_eq!(
            meta.config_schema.env_mappings.get("api_key"),
            Some(&"MISTRAL_API_KEY".to_string())
        );
    }

    // --- Endpoint ---

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.endpoint(), MISTRAL_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://custom.mistral.example.com/v1".to_string());
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.endpoint(), "https://custom.mistral.example.com/v1");
    }

    // --- Serialization ---

    #[test]
    fn test_request_serialization() {
        let req = MistralRequest {
            model: "mistral-large-latest".to_string(),
            messages: vec![MistralMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            max_tokens: 1024,
            temperature: 0.5,
            tools: None,
            tool_choice: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "mistral-large-latest");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["temperature"], 0.5);
        assert_eq!(json["messages"][0]["role"], "user");
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "mistral-large-latest",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let resp: MistralResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model, "mistral-large-latest");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content, "Hello!");
        assert_eq!(resp.usage.total_tokens, 15);
    }

    // --- Availability ---

    #[tokio::test]
    async fn test_is_available_with_key() {
        let config = make_config(Some("valid-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert!(provider.is_available().await);
    }

    // --- Model listing ---

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("mistral-large")));
    }

    // --- Config access ---

    #[test]
    fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert!(provider.config().is_some());
    }
}
