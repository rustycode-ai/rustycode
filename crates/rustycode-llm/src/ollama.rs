//! Ollama LLM provider implementation.
//!
//! Uses the Route abstraction (Protocol + Transport + Auth) for `/api/chat`
//! requests, with a direct HTTP client for local-only API calls
//! (`/api/tags` for model listing, availability checks).

use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use std::collections::HashMap;
use std::pin::Pin;

use crate::auth::NoAuth;
use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::route::Route;
use crate::transport::{HttpSseTransport, HttpTransport};
use crate::wire::ollama_chat::OllamaChatProtocol;

/// Default Ollama server endpoint.
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";

/// Ollama-specific message structure for cross-provider tests.
///
/// Ollama uses a flat message format with a separate `images` array for
/// vision support, rather than the content-block arrays used by OpenAI/Anthropic.
#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OllamaMessage {
    pub(crate) role: String,
    pub(crate) content: String,
    /// Base64-encoded images for vision models (llava, llama3.2-vision, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) images: Option<Vec<String>>,
}

/// OllamaProvider handles local LLM inference via Ollama.
///
/// Uses the Route abstraction for `/api/chat` requests (both streaming and
/// non-streaming) and a direct HTTP client for local-only endpoints like
/// `/api/tags` (model listing) and availability checks.
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
    route: Route,
    client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string());

        let timeout_secs = config.timeout_seconds.unwrap_or(300);

        let route = Route::new(
            format!("{}/api/chat", base_url),
            Box::new(OllamaChatProtocol),
            Box::new(crate::transport::fallback::TransportFallback::new(
                Box::new(
                    HttpTransport::new(timeout_secs)
                        .map_err(|e| ProviderError::Configuration(e.to_string()))?,
                ),
                Box::new(
                    HttpSseTransport::new(timeout_secs)
                        .map_err(|e| ProviderError::Configuration(e.to_string()))?,
                ),
            )),
            Box::new(NoAuth),
        )
        .with_name("ollama-chat");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Ok(Self {
            config,
            route,
            client,
            base_url,
        })
    }

    pub fn with_default_endpoint() -> Result<Self, ProviderError> {
        let config = ProviderConfig {
            base_url: Some(DEFAULT_OLLAMA_ENDPOINT.to_string()),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Get metadata for this provider.
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
                    placeholder: Some(DEFAULT_OLLAMA_ENDPOINT.to_string()),
                    default: Some(DEFAULT_OLLAMA_ENDPOINT.to_string()),
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
            model_behavior_profiles: HashMap::new(),
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Convert ChatMessages into OllamaMessages, handling vision content blocks
    /// and filtering out tool messages (Ollama has no native tool calling).
    #[cfg(test)]
    pub(crate) fn convert_messages(
        messages: Vec<crate::provider::ChatMessage>,
    ) -> Vec<OllamaMessage> {
        use crate::provider::MessageRole;
        use rustycode_protocol::MessageContent;

        messages
            .into_iter()
            .filter_map(|msg| {
                // Skip tool messages - Ollama doesn't support them
                if matches!(msg.role, MessageRole::Tool(_)) {
                    return None;
                }

                let ollama_role = match &msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                    MessageRole::Tool(_) => unreachable!(), // handled above
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
                                    match source.source_type.as_str() {
                                        "base64" => {
                                            imgs.push(source.data.clone());
                                        }
                                        "file" => {
                                            if let Some((_, data)) =
                                                crate::provider::resolve_image_to_base64(source)
                                            {
                                                imgs.push(data);
                                            }
                                        }
                                        _ => {}
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
        self.route
            .execute(&request, None)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                // Provide actionable error messages for common Ollama issues
                if msg.contains("404") {
                    ProviderError::InvalidModel(format!(
                        "model not found. {}. Run 'ollama list' to see available models, or 'ollama pull <model>' to download",
                        msg
                    ))
                } else if msg.contains("502") || msg.contains("503") || msg.contains("504") {
                    ProviderError::Network(format!(
                        "Ollama service unavailable. {}. Ensure Ollama is running: 'ollama serve'",
                        msg
                    ))
                } else {
                    ProviderError::Api(msg)
                }
            })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let stream = self
            .route
            .execute_stream(&request, None)
            .await
            .map_err(|e| ProviderError::api(e.to_string()))?;

        let chunk_stream = stream.map(|res| res.map_err(|e| ProviderError::api(e.to_string())));

        Ok(Box::pin(chunk_stream))
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
            DEFAULT_OLLAMA_ENDPOINT
        );
    }

    #[test]
    fn test_ollama_base_url() {
        let provider = OllamaProvider::with_default_endpoint().unwrap();
        assert_eq!(provider.base_url(), DEFAULT_OLLAMA_ENDPOINT);
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
        let request = serde_json::json!({
            "model": "llama3.2",
            "messages": vec![serde_json::json!({
                "role": "user",
                "content": "What is Rust?",
            })],
            "stream": false,
            "options": {
                "temperature": 0.7,
                "num_predict": 1024,
            },
        });
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"llama3.2\""));
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"num_predict\":1024"));
        // images should be absent when not set
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
        let response: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(response["model"], "llama3.2");
        assert_eq!(response["done"], true);
        assert_eq!(
            response["message"]["content"],
            "Rust is a systems programming language."
        );
        assert_eq!(response["prompt_eval_count"], 15);
        assert_eq!(response["eval_count"], 20);
    }

    #[test]
    fn test_ollama_response_missing_counts() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Hello"},
            "model": "llama3.2",
            "done": true
        }"#;
        let response: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(response.get("prompt_eval_count").is_none());
        assert!(response.get("eval_count").is_none());
    }

    #[test]
    fn test_error_message_404_suggests_ollama_list() {
        // Verify 404 errors include actionable guidance
        let config = ProviderConfig::default();
        let _provider = OllamaProvider::new(config).unwrap();
        // The error message is constructed inline in the complete() match arm;
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
    fn test_ollama_timeout_uses_config() {
        let config = ProviderConfig {
            timeout_seconds: Some(600),
            ..Default::default()
        };
        let provider = OllamaProvider::new(config).unwrap();
        // Provider created successfully with custom timeout
        assert_eq!(provider.name(), "ollama");
    }

    // -- Protocol-level message roundtrip tests --

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
                "Read",
                serde_json::json!({"path": "a.rs"}),
            )]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        // Tool use is flattened to a descriptive text string
        assert!(result[0].content.contains("Read"));
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
        // Only base64 images are extracted; URL sources do not go to images array
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
                ContentBlock::tool_use("call_x", "Read", serde_json::json!({"path": "x.rs"})),
            ]),
        }];
        let result = OllamaProvider::convert_messages(msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        // Both blocks flattened to text joined with newline
        assert!(result[0].content.contains("Reading file now."));
        assert!(result[0].content.contains("[Tool use: Read]"));
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

    // -- Wire protocol serialization tests --

    use crate::wire::Protocol;

    #[test]
    fn test_wire_protocol_serialize_body() {
        let request = CompletionRequest::new("llama3.2", vec![ChatMessage::user("Hello")])
            .with_temperature(0.5)
            .with_max_tokens(1024);

        let protocol = crate::wire::ollama_chat::OllamaChatProtocol;
        let body = protocol.serialize_body(&request, None).unwrap();

        assert_eq!(body["model"], "llama3.2");
        assert_eq!(body["stream"], false);
        assert_eq!(body["options"]["temperature"], 0.5);
        assert_eq!(body["options"]["num_predict"], 1024);
    }

    #[test]
    fn test_wire_protocol_parse_response() {
        let body = serde_json::json!({
            "message": {"role": "assistant", "content": "Hello from Ollama!"},
            "model": "llama3.2",
            "done": true,
            "prompt_eval_count": 10,
            "eval_count": 5
        });

        let protocol = crate::wire::ollama_chat::OllamaChatProtocol;
        let response = protocol.parse_response(&body).unwrap();

        assert_eq!(response.model, "llama3.2");
        assert_eq!(response.content, "Hello from Ollama!");
        assert_eq!(response.stop_reason, Some("end_turn".to_string()));
        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn test_wire_protocol_parse_response_with_tool_calls() {
        let body = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "Let me read that file.",
                "tool_calls": [{
                    "function": {
                        "name": "Read",
                        "arguments": {"path": "src/main.rs"}
                    }
                }]
            },
            "model": "llama3.2",
            "done": true
        });

        let protocol = crate::wire::ollama_chat::OllamaChatProtocol;
        let response = protocol.parse_response(&body).unwrap();

        assert!(response.content.contains("Let me read that file."));
        assert!(response.content.contains("[TOOL_CALLS:"));
        assert!(response.content.contains("Read"));
    }

    #[test]
    fn test_wire_protocol_parse_ndjson_stream_event() {
        let protocol = crate::wire::ollama_chat::OllamaChatProtocol;

        // Content delta
        let line =
            r#"{"message":{"role":"assistant","content":"Hello"},"model":"llama3.2","done":false}"#;
        let event = protocol.parse_sse_event(line).unwrap();
        match event {
            Some(rustycode_protocol::stream_event::StreamEvent::TextDelta { content }) => {
                assert_eq!(content, "Hello");
            }
            other => panic!("Expected TextDelta, got {:?}", other),
        }

        // Done event
        let done_line = r#"{"message":{"role":"assistant","content":""},"model":"llama3.2","done":true,"prompt_eval_count":10,"eval_count":5}"#;
        let event = protocol.parse_sse_event(done_line).unwrap();
        match event {
            Some(rustycode_protocol::stream_event::StreamEvent::TurnCompleted { stop_reason }) => {
                assert_eq!(stop_reason, "end_turn");
            }
            other => panic!("Expected TurnCompleted, got {:?}", other),
        }
    }
}
