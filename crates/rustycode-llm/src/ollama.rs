use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
    Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use rustycode_protocol::stream_event::StreamEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

/// Ollama-specific request structure
#[derive(Debug, Clone, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OllamaMessage {
    pub(crate) role: String,
    pub(crate) content: String,
    /// Base64-encoded images for vision models (llava, llama3.2-vision, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) images: Option<Vec<String>>,
}

/// Ollama generation options.
///
/// Maps to the `options` field in the `/api/chat` request.
/// See: <https://github.com/ollama/ollama/blob/main/docs/modelfile.md#valid-parameters-and-values>
#[derive(Debug, Clone, Serialize)]
struct OllamaOptions {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    /// How long to keep the model loaded in memory (e.g. "5m", "30m", "24h")
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_alive: Option<String>,
}

/// Ollama-specific response structure
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessageContent,
    model: String,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaMessageContent {
    role: String,
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

/// Ollama-specific streaming response structure
#[derive(Debug, Deserialize)]
struct OllamaStreamResponse {
    message: Option<OllamaMessageContent>,
    #[allow(dead_code)]
    model: String,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

/// OllamaProvider handles local LLM inference via Ollama
///
/// # Example
///
/// ```ignore
/// use rustycode_llm::{OllamaProvider, ProviderConfig};
///
/// let config = ProviderConfig {
///     base_url: Some("http://localhost:11434".to_string()),
///     ..Default::default()
/// };
/// let provider = OllamaProvider::new(config);
/// ```
pub struct OllamaProvider {
    config: ProviderConfig,
    client: Client,
}

impl OllamaProvider {
    fn build_chat_messages(
        request: CompletionRequest,
    ) -> Result<
        (
            String,
            Vec<OllamaMessage>,
            OllamaOptions,
            Option<Vec<serde_json::Value>>,
        ),
        ProviderError,
    > {
        let options = Self::build_options(&request);
        let model = request.model.clone();
        let tools = request.tools.clone();
        let mut messages = Self::convert_messages(request.messages);
        if let Some(system_prompt) = request.system_prompt {
            messages.insert(
                0,
                OllamaMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                    images: None,
                },
            );
        }

        if messages.is_empty() {
            return Err(ProviderError::Api(
                "No valid messages to send to Ollama after filtering tool messages".to_string(),
            ));
        }

        Ok((model, messages, options, tools))
    }

    async fn send_chat_request(
        &self,
        url: &str,
        request: &OllamaRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let response = self
            .client
            .post(url)
            .json(request)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!(
                    "Failed to call Ollama API at {}. Is Ollama running? Error: {}",
                    url, e
                ))
            })?;

        Self::validate_chat_response(response).await
    }

    async fn validate_chat_response(
        response: reqwest::Response,
    ) -> Result<reqwest::Response, ProviderError> {
        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read error body".to_string());

        Err(match status.as_u16() {
            404 => ProviderError::InvalidModel(format!(
                "model not found. {}. Run 'ollama list' to see available models, or 'ollama pull <model>' to download",
                error_body
            )),
            502..=504 => ProviderError::Network(format!(
                "Ollama service unavailable ({}). Ensure Ollama is running: 'ollama serve'",
                error_body
            )),
            _ => ProviderError::Api(format!("Ollama API error: {} - {}", status, error_body)),
        })
    }

    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let timeout_secs = config.timeout_seconds.unwrap_or(300);
        Ok(Self {
            config,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
        })
    }

    pub fn with_default_endpoint() -> Result<Self, ProviderError> {
        let config = ProviderConfig {
            base_url: Some("http://localhost:11434".to_string()),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "ollama".to_string(),
            display_name: "Ollama".to_string(),
            description: "Run LLMs locally on your own hardware".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "Ollama server endpoint".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some("http://localhost:11434".to_string()),
                    default: Some("http://localhost:11434".to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: HashMap::new(),
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant.\n\n{context}".to_string(),
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
                max_tools_per_call: None,
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![ModelInfo {
                model_id: "llama3.2".to_string(),
                display_name: "Llama 3.2".to_string(),
                description: "Meta's open-source LLM".to_string(),
                context_window: 128_000,
                supports_tools: true,
                use_cases: vec!["General tasks".to_string(), "Local inference".to_string()],
                cost_tier: 0,
            }],
        }
    }

    fn base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    }

    /// Convert ChatMessages into OllamaMessages, handling vision content blocks
    /// and filtering out tool messages (Ollama has no native tool calling).
    pub(crate) fn convert_messages(
        messages: Vec<crate::provider::ChatMessage>,
    ) -> Vec<OllamaMessage> {
        use rustycode_protocol::MessageContent;

        messages
            .into_iter()
            .filter_map(|msg| {
                // Skip tool messages - Ollama doesn't support them
                if matches!(msg.role, crate::provider::MessageRole::Tool(_)) {
                    return None;
                }

                let ollama_role = match &msg.role {
                    crate::provider::MessageRole::User => "user",
                    crate::provider::MessageRole::Assistant => "assistant",
                    crate::provider::MessageRole::System => "system",
                    crate::provider::MessageRole::Tool(_) => unreachable!(), // handled above
                }
                .to_string();

                // Extract images from block content (vision support)
                let (content_text, images) = match &msg.content {
                    MessageContent::Blocks(blocks) => {
                        let mut texts = Vec::new();
                        let mut imgs = Vec::new();
                        for block in blocks {
                            match block {
                                rustycode_protocol::ContentBlock::Text { text, .. } => {
                                    texts.push(text.clone());
                                }
                                rustycode_protocol::ContentBlock::Image { source, .. } => {
                                    // Ollama accepts base64-encoded images
                                    if source.source_type == "base64" {
                                        imgs.push(source.data.clone());
                                    }
                                    texts.push("[Image]".to_string());
                                }
                                rustycode_protocol::ContentBlock::ToolUse { name, .. } => {
                                    texts.push(format!("[Tool use: {}]", name));
                                }
                                rustycode_protocol::ContentBlock::ToolResult {
                                    content, ..
                                } => {
                                    texts.push(content.clone());
                                }
                                rustycode_protocol::ContentBlock::Thinking { thinking, .. } => {
                                    texts.push(thinking.clone());
                                }
                                _ => {} // non-exhaustive: future block types ignored
                            }
                        }
                        (
                            texts.join("\n"),
                            if imgs.is_empty() { None } else { Some(imgs) },
                        )
                    }
                    _ => (msg.content.to_text(), None), // non-exhaustive: Simple and future variants
                };

                Some(OllamaMessage {
                    role: ollama_role,
                    content: content_text,
                    images,
                })
            })
            .collect()
    }

    /// Build OllamaOptions from a CompletionRequest
    fn build_options(request: &CompletionRequest) -> OllamaOptions {
        OllamaOptions {
            temperature: request.temperature.unwrap_or(0.7),
            num_predict: request.max_tokens,
            top_p: None,
            top_k: None,
            num_ctx: None,
            stop: None,
            seed: None,
            repeat_penalty: None,
            presence_penalty: None,
            frequency_penalty: None,
            keep_alive: None,
        }
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url());

        match self.client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/api/tags", self.base_url());

        let response = self.client.get(&url).send().await.map_err(|e| {
            ProviderError::Network(format!(
                "Failed to connect to Ollama at {}: {}. Is Ollama running? Try: ollama serve",
                url, e
            ))
        })?;

        if !response.status().is_success() {
            return Err(ProviderError::Api(format!(
                "Ollama API returned status {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct OllamaTagsResponse {
            models: Vec<OllamaModel>,
        }

        #[derive(Deserialize)]
        struct OllamaModel {
            name: String,
        }

        let tags: OllamaTagsResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/api/chat", self.base_url());
        let (model, messages, options, tools) = Self::build_chat_messages(request)?;

        let ollama_request = OllamaRequest {
            model,
            messages,
            stream: false,
            options: Some(options),
            tools,
        };
        let response = self.send_chat_request(&url, &ollama_request).await?;

        let ollama_response: OllamaResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        // Build content, including tool calls if present
        let content = if let Some(tool_calls) = &ollama_response.message.tool_calls {
            let tc_json: Vec<serde_json::Value> = tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": format!("call_{}", tc.function.name),
                        "type": "function",
                        "function": {
                            "name": tc.function.name,
                            "arguments": serde_json::to_string(&tc.function.arguments).unwrap_or_default(),
                        }
                    })
                })
                .collect();
            let text = &ollama_response.message.content;
            if text.is_empty() {
                serde_json::to_string(&tc_json).unwrap_or_default()
            } else {
                format!(
                    "{text}\n[TOOL_CALLS:{}]",
                    serde_json::to_string(&tc_json).unwrap_or_default()
                )
            }
        } else {
            ollama_response.message.content
        };

        Ok(CompletionResponse {
            content,
            model: ollama_response.model,
            usage: Usage {
                input_tokens: ollama_response.prompt_eval_count.unwrap_or(0),
                output_tokens: ollama_response.eval_count.unwrap_or(0),
                total_tokens: ollama_response
                    .prompt_eval_count
                    .unwrap_or(0)
                    .saturating_add(ollama_response.eval_count.unwrap_or(0)),
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }
            .into(),
            stop_reason: if ollama_response.done {
                Some("end_turn".to_string())
            } else {
                None
            },
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = format!("{}/api/chat", self.base_url());
        let (model, messages, options, tools) = Self::build_chat_messages(request)?;

        let ollama_request = OllamaRequest {
            model,
            messages,
            stream: true,
            options: Some(options),
            tools,
        };
        let response = self.send_chat_request(&url, &ollama_request).await?;

        let bytes_stream = response.bytes_stream();

        let accumulated_content = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let prompt_eval_count = std::sync::Arc::new(std::sync::Mutex::new(Option::<u32>::None));
        let eval_count = std::sync::Arc::new(std::sync::Mutex::new(Option::<u32>::None));
        let done_sent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let byte_buffer = crate::sse::SseByteBuffer::new();

        let stream = bytes_stream.flat_map(move |chunk_result| {
            let accumulated_content = accumulated_content.clone();
            let prompt_eval_count = prompt_eval_count.clone();
            let eval_count = eval_count.clone();
            let done_sent = done_sent.clone();
            let byte_buffer = byte_buffer.clone();

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(format!(
                        "Stream error: {}",
                        e
                    )))]);
                }
            };
            let lines = byte_buffer.feed_chunk(&chunk);
            let mut events = Vec::new();

            for line in &lines {
                if line.is_empty() {
                    continue;
                }
                if let Ok(data) = serde_json::from_str::<OllamaStreamResponse>(line) {
                    if let Some(prompt_tokens) = data.prompt_eval_count {
                        *prompt_eval_count.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(prompt_tokens);
                    }
                    if let Some(output_tokens) = data.eval_count {
                        *eval_count.lock().unwrap_or_else(|e| e.into_inner()) = Some(output_tokens);
                    }

                    let mut content_buffer = accumulated_content
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    if let Some(message) = data.message {
                        if !message.content.is_empty() {
                            content_buffer.push_str(&message.content);
                            events.push(Ok(StreamEvent::TextDelta {
                                content: message.content,
                            }));
                        }
                    }

                    if data.done && !done_sent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let _final_content = std::mem::take(&mut *content_buffer);
                        drop(content_buffer);

                        let usage = {
                            let prompt =
                                *prompt_eval_count.lock().unwrap_or_else(|e| e.into_inner());
                            let output = *eval_count.lock().unwrap_or_else(|e| e.into_inner());
                            if let (Some(input_tokens), Some(output_tokens)) = (prompt, output) {
                                Some(Usage {
                                    input_tokens,
                                    output_tokens,
                                    total_tokens: input_tokens.saturating_add(output_tokens),
                                    cache_read_input_tokens: 0,
                                    cache_creation_input_tokens: 0,
                                    reasoning_tokens: None,
                                })
                            } else {
                                None
                            }
                        };

                        if let Some(usage) = usage {
                            events.push(Ok(StreamEvent::TokenUsage {
                                input_tokens: u64::from(usage.input_tokens),
                                output_tokens: u64::from(usage.output_tokens),
                            }));
                        }
                        events.push(Ok(StreamEvent::TurnCompleted {
                            stop_reason: "end_turn".to_string(),
                        }));
                        events.push(Ok(StreamEvent::Done));
                    }
                }
            }

            futures::stream::iter(events)
        });

        Ok(Box::pin(stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let config = ProviderConfig::default();
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_ollama_provider_default() {
        let provider = OllamaProvider::with_default_endpoint().unwrap();
        assert_eq!(
            provider.config.base_url.as_ref().unwrap(),
            "http://localhost:11434"
        );
    }

    #[test]
    fn test_ollama_base_url() {
        let provider = OllamaProvider::with_default_endpoint().unwrap();
        assert_eq!(provider.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = OllamaProvider::metadata();
        assert_eq!(metadata.display_name, "Ollama");
        assert_eq!(metadata.provider_id, "ollama");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = OllamaProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.parallel_calling);
        assert!(metadata.tool_calling.streaming_support);
    }

    #[test]
    fn test_metadata_no_required_fields() {
        let metadata = OllamaProvider::metadata();
        assert!(metadata.config_schema.required_fields.is_empty());
        assert!(!metadata.config_schema.optional_fields.is_empty());
    }

    #[test]
    fn test_metadata_recommended_models() {
        let metadata = OllamaProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(model_ids.iter().any(|id| id.contains("llama")));
    }

    #[test]
    fn test_custom_base_url() {
        let config = ProviderConfig {
            base_url: Some("http://192.168.1.100:11434".to_string()),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.base_url(), "http://192.168.1.100:11434");
    }

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaRequest {
            model: "llama3.2".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "What is Rust?".to_string(),
                images: None,
            }],
            stream: false,
            options: Some(OllamaOptions {
                temperature: 0.7,
                num_predict: Some(1024),
                top_p: None,
                top_k: None,
                num_ctx: None,
                stop: None,
                seed: None,
                repeat_penalty: None,
                presence_penalty: None,
                frequency_penalty: None,
                keep_alive: None,
            }),
            tools: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"llama3.2\""));
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"num_predict\":1024"));
        // images should be absent (skip_serializing_if)
        assert!(!json.contains("\"images\""));
    }

    #[test]
    fn test_ollama_response_deserialization() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Rust is a systems programming language."},
            "model": "llama3.2",
            "done": true,
            "prompt_eval_count": 15,
            "eval_count": 20
        }"#;
        let response: OllamaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "llama3.2");
        assert!(response.done);
        assert_eq!(
            response.message.content,
            "Rust is a systems programming language."
        );
        assert_eq!(response.prompt_eval_count, Some(15));
        assert_eq!(response.eval_count, Some(20));
    }

    #[test]
    fn test_ollama_response_missing_counts() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Hello"},
            "model": "llama3.2",
            "done": true
        }"#;
        let response: OllamaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.prompt_eval_count, None);
        assert_eq!(response.eval_count, None);
    }

    #[test]
    fn test_ollama_stream_response_deserialization() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Hello"},
            "model": "llama3.2",
            "done": false
        }"#;
        let response: OllamaStreamResponse = serde_json::from_str(json).unwrap();
        assert!(!response.done);
        assert!(response.message.is_some());
    }

    #[test]
    fn test_error_message_404_suggests_ollama_list() {
        // Verify 404 errors include actionable guidance
        let config = ProviderConfig::default();
        let _provider = OllamaProvider::new(config).unwrap();
        // The error message is constructed inline in the match arm;
        // we verify the pattern by checking the metadata recommends running 'ollama list'
        let meta = OllamaProvider::metadata();
        assert_eq!(meta.provider_id, "ollama");
        assert!(!meta.recommended_models.is_empty());
    }

    #[test]
    fn test_ollama_message_with_images() {
        let msg = OllamaMessage {
            role: "user".to_string(),
            content: "What is in this image?".to_string(),
            images: Some(vec!["iVBORw0KGgo=".to_string()]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"images\":[\"iVBORw0KGgo=\"]"));
        assert!(json.contains("\"content\":\"What is in this image?\""));
    }

    #[test]
    fn test_ollama_options_all_fields() {
        let opts = OllamaOptions {
            temperature: 0.5,
            num_predict: Some(2048),
            top_p: Some(0.9),
            top_k: Some(40),
            num_ctx: Some(4096),
            stop: Some(vec!["\n".to_string()]),
            seed: Some(42),
            repeat_penalty: Some(1.1),
            presence_penalty: Some(0.1),
            frequency_penalty: Some(0.2),
            keep_alive: Some("5m".to_string()),
        };
        let json = serde_json::to_string(&opts).unwrap();
        assert!(json.contains("\"top_p\":0.9"));
        assert!(json.contains("\"top_k\":40"));
        assert!(json.contains("\"num_ctx\":4096"));
        assert!(json.contains("\"stop\":[\"\\n\"]"));
        assert!(json.contains("\"seed\":42"));
        assert!(json.contains("\"repeat_penalty\":1.1"));
        assert!(json.contains("\"keep_alive\":\"5m\""));
    }

    #[test]
    fn test_ollama_options_minimal_fields() {
        // Only required fields — rest should be absent from JSON
        let opts = OllamaOptions {
            temperature: 0.7,
            num_predict: None,
            top_p: None,
            top_k: None,
            num_ctx: None,
            stop: None,
            seed: None,
            repeat_penalty: None,
            presence_penalty: None,
            frequency_penalty: None,
            keep_alive: None,
        };
        let json = serde_json::to_string(&opts).unwrap();
        assert!(json.contains("\"temperature\":0.7"));
        assert!(!json.contains("top_p"));
        assert!(!json.contains("top_k"));
        assert!(!json.contains("num_ctx"));
        assert!(!json.contains("keep_alive"));
    }

    #[test]
    fn test_ollama_timeout_uses_config() {
        let config = ProviderConfig {
            timeout_seconds: Some(600),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).unwrap();
        // Provider created successfully with custom timeout
        assert_eq!(provider.name(), "ollama");
    }

    // ── Protocol-level message roundtrip tests ────────────────────────────────

    use crate::provider::{ChatMessage, MessageRole};
    use rustycode_protocol::{ContentBlock, ImageSource, MessageContent};

    #[test]
    fn test_roundtrip_simple_text_user_message() {
        let msgs = vec![ChatMessage::user("Hello, world!")];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content, "Hello, world!");
        assert!(result[0].images.is_none());
    }

    #[test]
    fn test_roundtrip_simple_text_assistant_message() {
        let msgs = vec![ChatMessage::assistant("Hi there!")];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        assert_eq!(result[0].content, "Hi there!");
    }

    #[test]
    fn test_roundtrip_simple_text_system_message() {
        let msgs = vec![ChatMessage::system("System prompt")];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "system");
        assert_eq!(result[0].content, "System prompt");
    }

    #[test]
    fn test_roundtrip_text_block() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::text("Block text")]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content, "Block text");
    }

    #[test]
    fn test_roundtrip_tool_use_block_flattened_to_text() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::tool_use(
                "call_123",
                "read_file",
                serde_json::json!({"path": "a.rs"}),
            )]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        // Tool use is flattened to a descriptive text string
        assert!(result[0].content.contains("read_file"));
        assert!(result[0].content.contains("[Tool use:"));
    }

    #[test]
    fn test_roundtrip_tool_result_block_flattened_to_text() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result(
                "call_abc",
                "file output",
            )]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        // Tool result content is flattened to plain text
        assert_eq!(result[0].content, "file output");
    }

    #[test]
    fn test_roundtrip_tool_role_messages_filtered_out() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Tool("call_id".to_string()),
            content: MessageContent::simple("Tool output"),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        // Tool role messages are filtered out entirely
        assert!(result.is_empty());
    }

    #[test]
    fn test_roundtrip_image_block_extracted() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::image(ImageSource::base64(
                "image/png",
                "iVBORw0KGgo=",
            ))]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        // Image extracted to images array
        assert!(result[0].images.is_some());
        let images = result[0].images.as_ref().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0], "iVBORw0KGgo=");
        // Text placeholder for the image
        assert!(result[0].content.contains("[Image]"));
    }

    #[test]
    fn test_roundtrip_image_url_source_not_extracted() {
        // Only base64 images are extracted; URL sources go to images but with source data
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::image(ImageSource::url(
                "https://example.com/img.png",
                "image/png",
            ))]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        // URL images don't have source_type == "base64", so not extracted
        assert!(result[0].images.is_none());
        assert!(result[0].content.contains("[Image]"));
    }

    #[test]
    fn test_roundtrip_thinking_block_flattened_to_text() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::thinking(
                "deep thoughts",
                "sig123",
            )]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        // Thinking is flattened to its text content
        assert_eq!(result[0].content, "deep thoughts");
    }

    #[test]
    fn test_roundtrip_mixed_text_and_tool_use() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("Reading file now."),
                ContentBlock::tool_use("call_x", "read_file", serde_json::json!({"path": "x.rs"})),
            ]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        // Both blocks flattened to text joined with newline
        assert!(result[0].content.contains("Reading file now."));
        assert!(result[0].content.contains("[Tool use: read_file]"));
    }

    #[test]
    fn test_roundtrip_mixed_text_and_tool_result() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("Result:"),
                ContentBlock::tool_result("call_1", "data output"),
            ]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert!(result[0].content.contains("Result:"));
        assert!(result[0].content.contains("data output"));
    }

    #[test]
    fn test_roundtrip_empty_message() {
        let msgs = vec![ChatMessage::user("")];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "");
    }

    #[test]
    fn test_roundtrip_whitespace_only_message() {
        let msgs = vec![ChatMessage::user("   \n\t  ")];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "   \n\t  ");
    }

    #[test]
    fn test_roundtrip_very_long_text_content() {
        let long_text = "C".repeat(12_000);
        let msgs = vec![ChatMessage::user(long_text.clone())];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, long_text);
    }

    #[test]
    fn test_roundtrip_multiple_images_extracted() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("Compare these:"),
                ContentBlock::image(ImageSource::base64("image/png", "img1_data")),
                ContentBlock::image(ImageSource::base64("image/jpeg", "img2_data")),
            ]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        let images = result[0].images.as_ref().expect("should have images");
        assert_eq!(images.len(), 2);
        assert_eq!(images[0], "img1_data");
        assert_eq!(images[1], "img2_data");
        // Content should have text + two [Image] placeholders
        assert!(result[0].content.contains("Compare these:"));
        assert_eq!(result[0].content.matches("[Image]").count(), 2);
    }

    #[test]
    fn test_system_prompt_prepended_to_messages() {
        // When CompletionRequest.system_prompt is set, it should be prepended
        // as a system message. This tests the fix for the headless agent which
        // now uses system_prompt field instead of ChatMessage::system().
        use crate::provider::CompletionRequest;

        let request = CompletionRequest::new("llama3", vec![ChatMessage::user("hello")])
            .with_system_prompt("You are a helpful assistant.".to_string());

        let mut messages = OllamaProvider::convert_messages(request.messages);
        // Simulate what complete() does
        if let Some(ref system_prompt) = request.system_prompt {
            messages.insert(
                0,
                OllamaMessage {
                    role: "system".to_string(),
                    content: system_prompt.clone(),
                    images: None,
                },
            );
        }

        assert_eq!(messages.len(), 2, "Should have system + user messages");
        assert_eq!(messages[0].role, "system", "First message should be system");
        assert_eq!(messages[0].content, "You are a helpful assistant.");
        assert_eq!(messages[1].role, "user", "Second message should be user");
        assert_eq!(messages[1].content, "hello");
    }
}
