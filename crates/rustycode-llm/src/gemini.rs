//! Google Gemini LLM provider implementation.
//!
//! This provider supports Google's Gemini API which provides access to
//! language models like Gemini 2.5 Pro, Gemini 2.0 Flash, and more.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Google AI Studio)
//! - Model name (e.g., "gemini-2.5-pro", "gemini-2.0-flash")
//!
//! ## Environment Variables
//!
//! - `GOOGLE_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```rust
//! use rustycode_llm::{GeminiProvider, ProviderConfig};
//! use secrecy::SecretString;
//!
//! let config = ProviderConfig {
//!     api_key: Some(SecretString::new("AIza-your-api-key".to_string().into())),
//!     base_url: Some("https://generativelanguage.googleapis.com".to_string()),
//!     timeout_seconds: Some(180),
//!     extra_headers: None,
//!     retry_config: None,
//! };
//! let provider = GeminiProvider::new(config).unwrap();
//! ```
//!
//! ## Streaming
//!
//! Gemini uses a streaming-specific endpoint (`streamGenerateContent`) that
//! returns Server-Sent Events (SSE) with real-time text generation.

use crate::provider::{
    build_gemini_response_schema, ChatMessage, CompletionRequest, CompletionResponse, LLMProvider,
    MessageRole, ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::retry::extract_retry_after_ms;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiToolsBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<GeminiToolConfig>,
}

#[derive(Serialize, Clone)]
struct GeminiToolsBlock {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDecl>,
}

#[derive(Serialize, Clone)]
struct GeminiFunctionDecl {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiToolConfig {
    function_calling_config: GeminiFunctionCallingConfig,
}

#[derive(Serialize)]
struct GeminiFunctionCallingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_function_names: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct GeminiContent {
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize, Clone)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "functionCall", skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(rename = "functionResponse", skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Serialize, Deserialize, Clone)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_schema: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiUsageMetadata {
    total_token_count: usize,
}

/// Google Gemini LLM provider
pub struct GeminiProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(config: ProviderConfig) -> Result<Self> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let api_key = config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Google API key is required. Set api_key in config or GOOGLE_API_KEY env var"
                )
            })?
            .expose_secret();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("x-goog-api-key"),
            reqwest::header::HeaderValue::from_str(api_key).context("invalid API key format")?,
        );

        let timeout = config
            .timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_mins(3));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { config, client })
    }

    /// Create provider without config validation (for custom endpoints/proxies)
    pub fn new_without_validation(config: ProviderConfig) -> Result<Self> {
        // Skip validation - but we still need the API key for the header
        let api_key = config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Google API key is required. Set api_key in config or GOOGLE_API_KEY env var"
                )
            })?
            .expose_secret();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("x-goog-api-key"),
            reqwest::header::HeaderValue::from_str(api_key).context("invalid API key format")?,
        );

        let timeout = config
            .timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_mins(3));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { config, client })
    }

    /// Convert internal messages to Gemini content format
    fn convert_messages(messages: &[ChatMessage]) -> Vec<GeminiContent> {
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
                MessageContent::Simple(text) => {
                    if !text.is_empty() {
                        parts.push(GeminiPart {
                            text: Some(text.clone()),
                            function_call: None,
                            function_response: None,
                        });
                    }
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } => {
                                if !text.is_empty() {
                                    parts.push(GeminiPart {
                                        text: Some(text.clone()),
                                        function_call: None,
                                        function_response: None,
                                    });
                                }
                            }
                            ContentBlock::ToolUse { name, input, .. } => {
                                if matches!(msg.role, MessageRole::Assistant) {
                                    parts.push(GeminiPart {
                                        text: None,
                                        function_call: Some(GeminiFunctionCall {
                                            name: name.clone(),
                                            args: input.clone(),
                                        }),
                                        function_response: None,
                                    });
                                }
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content: result_text,
                                ..
                            } => {
                                let response_value = serde_json::json!({"result": result_text});
                                parts.push(GeminiPart {
                                    text: None,
                                    function_call: None,
                                    function_response: Some(GeminiFunctionResponse {
                                        name: tool_use_id.clone(),
                                        response: response_value,
                                    }),
                                });
                            }
                            ContentBlock::Thinking { thinking, .. } => {
                                if !thinking.is_empty() {
                                    parts.push(GeminiPart {
                                        text: Some(format!("[thinking] {thinking}")),
                                        function_call: None,
                                        function_response: None,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }

            if !parts.is_empty() {
                contents.push(GeminiContent {
                    role: Some(role.to_string()),
                    parts,
                });
            }
        }

        contents
    }

    /// Convert Anthropic-format tools to Gemini functionDeclarations format
    fn convert_tools(tools: &[serde_json::Value]) -> Vec<GeminiToolsBlock> {
        let decls: Vec<GeminiFunctionDecl> = tools
            .iter()
            .map(|tool| {
                let name = tool
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let description = tool
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let parameters = tool
                    .get("input_schema")
                    .or_else(|| tool.get("parameters"))
                    .cloned();
                GeminiFunctionDecl {
                    name: name.to_string(),
                    description,
                    parameters,
                }
            })
            .collect();

        if decls.is_empty() {
            vec![]
        } else {
            vec![GeminiToolsBlock {
                function_declarations: decls,
            }]
        }
    }

    /// Convert tool_choice to Gemini's functionCallingConfig
    fn convert_tool_choice(tool_choice: &serde_json::Value) -> Option<GeminiToolConfig> {
        let config = match tool_choice {
            serde_json::Value::String(s) => match s.as_str() {
                "auto" => GeminiFunctionCallingConfig {
                    mode: Some("AUTO".to_string()),
                    allowed_function_names: None,
                },
                "none" => GeminiFunctionCallingConfig {
                    mode: Some("NONE".to_string()),
                    allowed_function_names: None,
                },
                _ => return None,
            },
            serde_json::Value::Object(map) => {
                if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                    GeminiFunctionCallingConfig {
                        mode: Some("ANY".to_string()),
                        allowed_function_names: Some(vec![name.to_string()]),
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        Some(GeminiToolConfig {
            function_calling_config: config,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "gemini".to_string(),
            display_name: "Google Gemini".to_string(),
            description: "Multimodal AI assistant with strong reasoning and creative capabilities".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Google API key from console.cloud.google.com".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("AIza...".to_string()),
                        default: None,
                        validation_pattern: Some("^AIza.*".to_string()),
                        validation_error: Some("API key must start with 'AIza'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "GEMINI_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are RustyCode, a coding assistant.\n\n{context}\n\n## Gemini Guidance\n- Order reasoning: Context → Task → Constraints\n- Place critical instructions at the END for strongest attention\n- State assumptions and proceed rather than asking for clarification\n- Separate expected vs actual results when verifying".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Keep function descriptions concise; include enum constraints for enum parameters.".to_string(),
                        "For large contexts, re-read key requirements before finalizing output.".to_string(),
                    ],
                },
                tool_format: ToolFormat::GeminiTools,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "gemini-2.5-pro".to_string(),
                    display_name: "Gemini 2.5 Pro".to_string(),
                    description: "Latest model with advanced reasoning".to_string(),
                    context_window: 1_000_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Large context analysis".to_string()],
                    cost_tier: 4,
                },
            ],
                    model_behavior_profiles: HashMap::new(),
        }
    }

    pub fn endpoint(&self, model: &str) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com");
        format!("{}/v1beta/models/{}:generateContent", base, model)
    }

    pub fn stream_endpoint(&self, model: &str) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com");
        format!("{}/v1beta/models/{}:streamGenerateContent", base, model)
    }

    async fn complete_internal(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = self.endpoint(&request.model);

        let response_schema = build_gemini_response_schema(&request.output_config);
        let generation_config = GeminiGenerationConfig {
            temperature: request.temperature.unwrap_or(0.7),
            max_output_tokens: request.max_tokens,
            response_mime_type: response_schema
                .as_ref()
                .map(|_| "application/json".to_string()),
            response_schema: response_schema.and_then(|v| v.get("responseSchema").cloned()),
        };

        let system_instruction =
            request
                .system_prompt
                .as_ref()
                .map(|prompt| GeminiSystemInstruction {
                    parts: vec![GeminiPart {
                        text: Some(prompt.clone()),
                        function_call: None,
                        function_response: None,
                    }],
                });

        let contents = Self::convert_messages(&request.messages);

        // Convert tools to Gemini functionDeclarations format
        let tools_blocks = match &request.tools {
            Some(tools) if !tools.is_empty() => Some(Self::convert_tools(tools)),
            _ => None,
        };

        let tool_config = request
            .tool_choice
            .as_ref()
            .and_then(Self::convert_tool_choice);

        let gemini_request = GeminiRequest {
            contents,
            generation_config: Some(generation_config),
            system_instruction,
            tools: tools_blocks,
            tool_config,
        };

        let response = self
            .client
            .post(&url)
            .json(&gemini_request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            // Capture headers before consuming the body to support Retry-After headers
            let headers = response.headers().clone();
            let error_body = response.text().await.ok();
            let error_msg = error_body.unwrap_or_else(|| format!("HTTP {}", status.as_u16()));

            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your GEMINI_API_KEY env var. {}",
                    error_msg
                )),
                404 => ProviderError::InvalidModel(error_msg.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Gemini service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_msg
                )),
                _ => ProviderError::Api(error_msg),
            });
        }

        let resp: GeminiResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        let candidate =
            resp.candidates.into_iter().next().ok_or_else(|| {
                ProviderError::Api("no candidates in Gemini response".to_string())
            })?;

        // Extract text and function calls from parts
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for part in &candidate.content.parts {
            if let Some(ref text) = part.text {
                if !text.is_empty() {
                    text_parts.push(text.clone());
                }
            }
            if let Some(ref fc) = part.function_call {
                tool_calls.push(serde_json::json!({
                    "id": format!("call_{}", fc.name),
                    "type": "function",
                    "function": {
                        "name": fc.name,
                        "arguments": serde_json::to_string(&fc.args).unwrap_or_default(),
                    }
                }));
            }
        }

        // If tool calls present, format as JSON for tool routing
        let content = if !tool_calls.is_empty() {
            if text_parts.is_empty() {
                serde_json::to_string(&tool_calls).unwrap_or_default()
            } else {
                // Mix of text and tool calls
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

        let usage = resp.usage_metadata.map(|u| {
            let total = u.total_token_count.try_into().unwrap_or(u32::MAX);
            crate::provider::Usage {
                input_tokens: 0,
                output_tokens: total,
                total_tokens: total,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }
        });

        Ok(CompletionResponse {
            content,
            model: request.model.clone(),
            usage,
            stop_reason: crate::provider::normalize_stop_reason(candidate.finish_reason.as_deref()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return common Gemini models (as of March 2026)
        Ok(vec![
            "gemini-2.5-pro".to_string(),
            "gemini-2.5-flash".to_string(),
            "gemini-2.0-flash".to_string(),
            "gemini-1.5-pro".to_string(),
            "gemini-1.5-flash".to_string(),
            "gemini-1.5-flash-8b".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.complete_internal(&request).await
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = self.stream_endpoint(&request.model);

        let response_schema = build_gemini_response_schema(&request.output_config);
        let generation_config = GeminiGenerationConfig {
            temperature: request.temperature.unwrap_or(0.7),
            max_output_tokens: request.max_tokens,
            response_mime_type: response_schema
                .as_ref()
                .map(|_| "application/json".to_string()),
            response_schema: response_schema.and_then(|v| v.get("responseSchema").cloned()),
        };

        let system_instruction =
            request
                .system_prompt
                .as_ref()
                .map(|prompt| GeminiSystemInstruction {
                    parts: vec![GeminiPart {
                        text: Some(prompt.clone()),
                        function_call: None,
                        function_response: None,
                    }],
                });

        let contents = Self::convert_messages(&request.messages);

        let tools_blocks = match &request.tools {
            Some(tools) if !tools.is_empty() => Some(Self::convert_tools(tools)),
            _ => None,
        };

        let tool_config = request
            .tool_choice
            .as_ref()
            .and_then(Self::convert_tool_choice);

        let gemini_request = GeminiRequest {
            contents,
            generation_config: Some(generation_config),
            system_instruction,
            tools: tools_blocks,
            tool_config,
        };

        let response = self
            .client
            .post(&url)
            .json(&gemini_request)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("failed to connect to Gemini API: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            // Capture headers before consuming the body to support Retry-After headers
            let headers = response.headers().clone();
            let error_body = response.text().await.ok();
            let error_msg = error_body.unwrap_or_else(|| format!("HTTP {}", status.as_u16()));

            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your GEMINI_API_KEY env var. {}",
                    error_msg
                )),
                404 => ProviderError::InvalidModel(error_msg.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Gemini service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_msg
                )),
                _ => ProviderError::Api(error_msg),
            });
        }

        // Gemini returns SSE-style streaming responses
        let bytes_stream = response.bytes_stream();
        let byte_buffer = crate::sse::SseByteBuffer::new();

        let sse_stream = bytes_stream.map(move |chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("failed to read chunk: {}", e)))?;
            let lines = byte_buffer.feed_chunk(&chunk);
            let mut chunks = Vec::new();

            for line in &lines {
                if line.is_empty() || line == "," {
                    continue;
                }
                let line = line.trim_end_matches(',');
                // Gemini sends newline-delimited JSON objects
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(candidates) = data.get("candidates").and_then(|c| c.as_array()) {
                        if let Some(candidate) = candidates.first() {
                            if let Some(content) = candidate.get("content") {
                                if let Some(parts) = content.get("parts").and_then(|p| p.as_array())
                                {
                                    for part in parts {
                                        // Extract text chunks
                                        if let Some(text) =
                                            part.get("text").and_then(|t| t.as_str())
                                        {
                                            if !text.is_empty() {
                                                chunks.push(text.to_string());
                                            }
                                        }
                                        // Extract function call chunks
                                        if let Some(fc) = part.get("functionCall") {
                                            let name = fc
                                                .get("name")
                                                .and_then(|n| n.as_str())
                                                .unwrap_or("unknown");
                                            let args = fc.get("args").cloned().unwrap_or(serde_json::json!({}));
                                            chunks.push(format!(
                                                "[TOOL_CALL:{}]",
                                                serde_json::to_string(&serde_json::json!({
                                                    "id": format!("call_{}", name),
                                                    "type": "function",
                                                    "function": {
                                                        "name": name,
                                                        "arguments": serde_json::to_string(&args).unwrap_or_default(),
                                                    }
                                                })).unwrap_or_default()
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta { content: chunks.join("") })
        });

        Ok(Box::pin(sse_stream))
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
            timeout_seconds: None,
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_requires_api_key() {
        assert!(GeminiProvider::new(make_config(None)).is_err());
    }

    #[test]
    fn test_creates_with_api_key() {
        assert!(GeminiProvider::new(make_config(Some("AIzaTest123"))).is_ok());
    }

    #[test]
    fn test_name() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        assert_eq!(p.name(), "gemini");
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("AIzaTest123"));
        config.base_url = Some("https://proxy.example.com".to_string());
        let p = GeminiProvider::new(config).unwrap();
        let endpoint = p.endpoint("gemini-pro");
        assert!(endpoint.contains("proxy.example.com"));
        assert!(!endpoint.contains("test-key"));
    }

    #[test]
    fn test_stream_endpoint() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let endpoint = p.stream_endpoint("gemini-pro");
        assert!(endpoint.contains("streamGenerateContent"));
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = GeminiProvider::metadata();
        assert_eq!(metadata.display_name, "Google Gemini");
        assert_eq!(metadata.provider_id, "gemini");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = GeminiProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = GeminiProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"GEMINI_API_KEY".to_string())
        );
    }

    #[test]
    fn test_metadata_tool_format() {
        let metadata = GeminiProvider::metadata();
        assert!(matches!(
            metadata.prompt_template.tool_format,
            crate::provider_metadata::ToolFormat::GeminiTools
        ));
    }

    #[test]
    fn test_default_endpoint() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let endpoint = p.endpoint("gemini-2.5-pro");
        assert!(endpoint.starts_with("https://generativelanguage.googleapis.com"));
        assert!(endpoint.contains("gemini-2.5-pro"));
        assert!(endpoint.contains("generateContent"));
    }

    #[test]
    fn test_custom_endpoint_used() {
        let mut config = make_config(Some("AIzaTest123"));
        config.base_url = Some("https://my-gemini-proxy.example.com".to_string());
        let p = GeminiProvider::new(config).unwrap();
        let endpoint = p.endpoint("gemini-pro");
        assert!(endpoint.starts_with("https://my-gemini-proxy.example.com"));
    }

    #[test]
    fn test_gemini_content_deserialization() {
        let json = r#"{"role": "user", "parts": [{"text": "Hello world"}]}"#;
        let content: GeminiContent = serde_json::from_str(json).unwrap();
        assert_eq!(content.role, Some("user".to_string()));
        assert_eq!(content.parts.len(), 1);
        assert_eq!(content.parts[0].text, Some("Hello world".to_string()));
    }

    #[test]
    fn test_gemini_response_deserialization() {
        let json = r#"{
            "candidates": [
                {
                    "content": {"parts": [{"text": "The answer is 42"}]},
                    "finish_reason": "STOP"
                }
            ],
            "usage_metadata": {"total_token_count": 100}
        }"#;
        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(
            response.candidates[0].content.parts[0].text,
            Some("The answer is 42".to_string())
        );
        assert_eq!(response.usage_metadata.unwrap().total_token_count, 100);
    }

    #[test]
    fn test_gemini_usage_metadata_missing() {
        let json = r#"{
            "candidates": [
                {
                    "content": {"parts": [{"text": "Hello"}]},
                    "finish_reason": "STOP"
                }
            ]
        }"#;
        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage_metadata.is_none());
    }

    #[test]
    fn test_gemini_request_serialization() {
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".to_string()),
                parts: vec![GeminiPart {
                    text: Some("What is Rust?".to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }],
            generation_config: Some(GeminiGenerationConfig {
                temperature: 0.5,
                max_output_tokens: Some(1024),
                response_mime_type: None,
                response_schema: None,
            }),
            system_instruction: Some(GeminiSystemInstruction {
                parts: vec![GeminiPart {
                    text: Some("Be helpful".to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }),
            tools: None,
            tool_config: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"temperature\":0.5"));
        assert!(json.contains("\"max_output_tokens\":1024"));
        assert!(!json.contains("\"tools\""));
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let models = p.list_models().await.unwrap();
        assert!(models.iter().any(|m| m == "gemini-2.5-pro"));
        assert!(models.iter().any(|m| m == "gemini-2.0-flash"));
    }

    #[test]
    fn test_new_without_validation() {
        let config = make_config(Some("AIzaTest123"));
        let provider = GeminiProvider::new_without_validation(config).unwrap();
        assert_eq!(provider.name(), "gemini");
    }

    #[test]
    fn test_convert_tools_wraps_in_function_declarations() {
        let tools = vec![serde_json::json!({
            "name": "bash",
            "description": "Run a command",
            "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}}
        })];
        let blocks = GeminiProvider::convert_tools(&tools);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].function_declarations.len(), 1);
        assert_eq!(blocks[0].function_declarations[0].name, "bash");
    }

    #[test]
    fn test_convert_messages_assigns_roles() {
        use crate::provider::MessageRole;
        use rustycode_protocol::message::MessageContent;

        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Simple("Hello".into()),
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: MessageContent::Simple("Hi".into()),
            },
        ];
        let contents = GeminiProvider::convert_messages(&messages);
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].role, Some("user".to_string()));
        assert_eq!(contents[1].role, Some("model".to_string()));
    }

    #[test]
    fn test_convert_tool_choice_auto() {
        let tc = serde_json::json!("auto");
        let config = GeminiProvider::convert_tool_choice(&tc);
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(
            config.function_calling_config.mode,
            Some("AUTO".to_string())
        );
    }

    #[test]
    fn test_convert_tool_choice_none_mode() {
        let tc = serde_json::json!("none");
        let config = GeminiProvider::convert_tool_choice(&tc);
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(
            config.function_calling_config.mode,
            Some("NONE".to_string())
        );
    }

    #[test]
    fn test_convert_tool_choice_named() {
        let tc = serde_json::json!({"name": "bash"});
        let config = GeminiProvider::convert_tool_choice(&tc);
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.function_calling_config.mode, Some("ANY".to_string()));
        assert_eq!(
            config.function_calling_config.allowed_function_names,
            Some(vec!["bash".to_string()])
        );
    }

    #[test]
    fn test_gemini_function_call_deserialization() {
        let json = r#"{"name": "bash", "args": {"command": "ls -la"}}"#;
        let fc: GeminiFunctionCall = serde_json::from_str(json).unwrap();
        assert_eq!(fc.name, "bash");
        assert_eq!(fc.args["command"], "ls -la");
    }

    #[test]
    fn test_gemini_part_with_function_call() {
        let json = r#"{"functionCall": {"name": "bash", "args": {"command": "ls"}}}"#;
        let part: GeminiPart = serde_json::from_str(json).unwrap();
        assert!(part.text.is_none());
        assert!(part.function_call.is_some());
        assert_eq!(part.function_call.unwrap().name, "bash");
    }

    #[test]
    fn test_gemini_request_with_tools() {
        let tools = vec![serde_json::json!({
            "name": "bash",
            "description": "Run command",
            "input_schema": {"type": "object"}
        })];
        let blocks = GeminiProvider::convert_tools(&tools);
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".to_string()),
                parts: vec![GeminiPart {
                    text: Some("hello".to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }],
            generation_config: Some(GeminiGenerationConfig {
                temperature: 0.7,
                max_output_tokens: None,
                response_mime_type: None,
                response_schema: None,
            }),
            system_instruction: None,
            tools: Some(blocks),
            tool_config: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("functionDeclarations"));
        assert!(json.contains("bash"));
    }
}
