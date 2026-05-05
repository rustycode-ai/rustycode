//! Cohere LLM provider implementation.
//!
//! This provider supports Cohere's v2 Chat API which provides access to
//! models like Command R, Command R+, and Command A with tool-use support.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Cohere dashboard)
//! - Model name (e.g., "command-r-plus-08-2024", "command-a-03-2025")
//!
//! ## Environment Variables
//!
//! - `COHERE_API_KEY` - API key for authentication
//!
//! ## Tool Calling
//!
//! Cohere's v2 Chat API supports function calling with parallel tool use.
//! Tools are sent as `{ type: "function", function: { name, description, parameters } }`.
//! Responses include `tool_calls` with `id`, `type`, and `function` fields.
//! Tool results are sent as `{ role: "tool", tool_call_id, content }` messages.

use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::retry::extract_retry_after_ms;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default Cohere API endpoint (v2 Chat API)
const COHERE_API_ENDPOINT: &str = "https://api.cohere.ai/v2/chat";

// ── Request types ──

#[derive(Serialize)]
struct CohereV2Request {
    model: String,
    messages: Vec<CohereV2Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preamble: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct CohereV2Message {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<CohereV2ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct CohereV2ToolCall {
    id: String,
    r#type: String,
    function: CohereV2ToolCallFunction,
}

#[derive(Serialize, Deserialize, Clone)]
struct CohereV2ToolCallFunction {
    name: String,
    arguments: String,
}

// ── Response types ──

#[derive(Deserialize)]
struct CohereV2Response {
    #[serde(default)]
    message: Option<CohereV2AssistantMessage>,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    meta: Option<CohereV2Meta>,
}

#[derive(Deserialize)]
struct CohereV2AssistantMessage {
    #[allow(dead_code)] // Deserialized from API but not directly read
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Vec<CohereV2ContentBlock>,
    #[serde(default)]
    tool_plan: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<CohereV2ToolCall>>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum CohereV2ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "document")]
    Document { document: CohereV2Document },
}

#[derive(Deserialize)]
struct CohereV2Document {
    data: String,
}

#[derive(Deserialize)]
struct CohereV2Meta {
    #[serde(default)]
    usage: Option<CohereV2Usage>,
    #[allow(dead_code)]
    #[serde(default)]
    api_version: Option<CohereV2ApiVersion>,
}

#[derive(Deserialize)]
struct CohereV2Usage {
    #[allow(dead_code)]
    #[serde(default)]
    billed_units: Option<serde_json::Value>,
    #[serde(default)]
    tokens: Option<CohereV2TokenUsage>,
}

#[derive(Deserialize)]
struct CohereV2TokenUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct CohereV2ApiVersion {
    #[allow(dead_code)]
    version: String,
}

/// Cohere LLM provider
pub struct CohereProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl CohereProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let timeout_secs = config.timeout_seconds.unwrap_or(180);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        use crate::provider::validate_extra_headers;
        let validated_headers = validate_extra_headers(&config.extra_headers)?;
        for (header_name, header_value) in validated_headers {
            headers.insert(header_name, header_value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })?;

        Ok(Self { config, client })
    }

    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "cohere".to_string(),
            display_name: "Cohere".to_string(),
            description: "Enterprise AI platform with Command R, Command R+, and Command A models"
                .to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    description: "Your Cohere API key from dashboard.cohere.com".to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("your-api-key".to_string()),
                    default: None,
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "Custom API endpoint (optional)".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some(COHERE_API_ENDPOINT.to_string()),
                    default: Some(COHERE_API_ENDPOINT.to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "COHERE_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant powered by Cohere. {context}"
                    .to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Cohere models perform well with clear, direct instructions.".to_string(),
                        "Use conversational language when appropriate.".to_string(),
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
                ModelInfo {
                    model_id: "command-a-03-2025".to_string(),
                    display_name: "Command A".to_string(),
                    description:
                        "Cohere's latest flagship model with strong tool use and reasoning"
                            .to_string(),
                    context_window: 256000,
                    supports_tools: true,
                    use_cases: vec![
                        "Tool calling".to_string(),
                        "Complex reasoning".to_string(),
                        "RAG applications".to_string(),
                    ],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "command-r-plus-08-2024".to_string(),
                    display_name: "Command R+".to_string(),
                    description:
                        "Cohere's flagship model with 128k context and strong RAG capabilities"
                            .to_string(),
                    context_window: 128000,
                    supports_tools: true,
                    use_cases: vec![
                        "Document analysis".to_string(),
                        "RAG applications".to_string(),
                        "Tool calling".to_string(),
                    ],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "command-r-08-2024".to_string(),
                    display_name: "Command R".to_string(),
                    description: "Balanced model with good performance and lower cost".to_string(),
                    context_window: 128000,
                    supports_tools: true,
                    use_cases: vec![
                        "Chat applications".to_string(),
                        "Tool calling".to_string(),
                        "Content generation".to_string(),
                    ],
                    cost_tier: 3,
                },
            ],
        }
    }

    fn endpoint(&self) -> String {
        self.config
            .base_url
            .as_ref()
            .unwrap_or(&COHERE_API_ENDPOINT.to_string())
            .clone()
    }

    fn api_key(&self) -> Result<String, ProviderError> {
        let config_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("COHERE_API_KEY").ok();

        config_key.or(env_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Cohere API key is required. Set api_key in config or COHERE_API_KEY env var"
                    .to_string(),
            )
        })
    }

    /// Convert internal messages to Cohere v2 format.
    fn convert_messages(messages: Vec<crate::provider::ChatMessage>) -> Vec<CohereV2Message> {
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
                MessageContent::Simple(t) => {
                    text_parts.push(t.clone());
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse {
                                id, name, input, ..
                            } => {
                                tool_calls.push(CohereV2ToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: CohereV2ToolCallFunction {
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

            // Build content based on role and available data
            let content = if tool_call_id.is_some() {
                // Tool results must use document format per Cohere v2 API spec:
                // { role: "tool", tool_call_id, content: [{ type: "document", document: { data: "..." } }] }
                let data = if text_parts.is_empty() {
                    String::new()
                } else {
                    text_parts.join("\n")
                };
                Some(serde_json::json!([{
                    "type": "document",
                    "document": { "data": data }
                }]))
            } else if text_parts.is_empty() && !tool_calls.is_empty() {
                // Assistant with only tool calls, no text content
                None
            } else if text_parts.len() == 1 {
                Some(serde_json::json!(text_parts[0]))
            } else if !text_parts.is_empty() {
                Some(serde_json::json!(text_parts.join("\n")))
            } else {
                None
            };

            result.push(CohereV2Message {
                role: role.to_string(),
                content,
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id,
            });
        }
        result
    }

    /// Convert tools from Anthropic format to Cohere v2 format.
    /// Cohere v2 uses the same OpenAI-compatible format: { type: "function", function: { name, description, parameters } }
    fn convert_tools(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
        crate::tools::normalize_tools_for_openai(tools)
    }

    /// Build the Cohere v2 request from CompletionRequest.
    fn build_request(&self, request: &CompletionRequest) -> CohereV2Request {
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request
            .tools
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|t| Self::convert_tools(t));

        CohereV2Request {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            preamble: request.system_prompt.clone(),
            tools,
            tool_choice: request.tool_choice.clone(),
            stream: false,
        }
    }

    /// Extract content and tool calls from Cohere v2 response.
    fn extract_response(&self, cohere_resp: CohereV2Response, model: String) -> CompletionResponse {
        let mut content = String::new();

        if let Some(msg) = &cohere_resp.message {
            // Extract text content
            for block in &msg.content {
                match block {
                    CohereV2ContentBlock::Text { text } => {
                        if !content.is_empty() {
                            content.push('\n');
                        }
                        content.push_str(text);
                    }
                    CohereV2ContentBlock::Document { document } => {
                        if !content.is_empty() {
                            content.push('\n');
                        }
                        content.push_str(&document.data);
                    }
                }
            }

            // Prepend tool_plan to content if present
            if let Some(plan) = &msg.tool_plan {
                if !plan.is_empty() && !content.is_empty() {
                    content = format!("{}\n\n{}", plan, content);
                } else if !plan.is_empty() {
                    content.clone_from(plan);
                }
            }

            // Extract tool calls and embed in content
            if let Some(calls) = &msg.tool_calls {
                let tc_json: Vec<serde_json::Value> = calls
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
                let tc_str = serde_json::to_string(&tc_json).unwrap_or_default();
                if content.is_empty() {
                    content = tc_str;
                } else {
                    content = format!("{content}\n[TOOL_CALLS:{tc_str}]");
                }
            }
        }

        // Build usage
        let usage = cohere_resp.meta.and_then(|m| m.usage).and_then(|u| {
            u.tokens.map(|t| {
                let input = t.input_tokens.unwrap_or(0);
                let output = t.output_tokens.unwrap_or(0);
                crate::provider::Usage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: input.saturating_add(output),
                    cache_read_input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    reasoning_tokens: None,
                }
            })
        });

        // Normalize stop reason to Anthropic convention for agent loop compatibility
        let stop_reason = match cohere_resp.finish_reason.as_deref() {
            Some("COMPLETE") => Some("end_turn".to_string()),
            Some("TOOL_CALL") => Some("tool_use".to_string()),
            Some("MAX_TOKENS") => Some("max_tokens".to_string()),
            other => other.map(|s| s.to_string()),
        };

        CompletionResponse {
            content,
            model,
            usage,
            stop_reason,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }
    }

    async fn complete_internal(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = self.endpoint();
        let api_key = self.api_key()?;
        let model = request.model.clone();
        let body = self.build_request(request);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("failed to send request to Cohere: {}", e))
            })?;
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Authentication failed. Check your COHERE_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(error_text.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Cohere service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("Cohere API error {}: {}", status, error_text)),
            });
        }

        let cohere_response: CohereV2Response = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse Cohere response: {}", e))
        })?;

        Ok(self.extract_response(cohere_response, model))
    }

    fn handle_stream_error(
        status: u16,
        error_text: String,
        headers: &reqwest::header::HeaderMap,
    ) -> ProviderError {
        match status {
            401 => ProviderError::Auth(format!(
                "Authentication failed. Check your COHERE_API_KEY env var. {}",
                error_text
            )),
            404 => ProviderError::InvalidModel(error_text.clone()),
            429 => ProviderError::RateLimited {
                retry_delay: extract_retry_after_ms(headers).map(Duration::from_millis),
            },
            502..=504 => ProviderError::Network(format!(
                "Cohere service temporarily unavailable ({}). Please retry in a few seconds.",
                error_text
            )),
            _ => ProviderError::Api(format!("Cohere API error {}: {}", status, error_text)),
        }
    }
}

#[async_trait]
impl LLMProvider for CohereProvider {
    fn name(&self) -> &'static str {
        "cohere"
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }

    async fn is_available(&self) -> bool {
        if self.api_key().is_err() {
            return false;
        }

        let api_key = match self.api_key() {
            Ok(key) => key,
            Err(_) => return false,
        };

        let response = self
            .client
            .get(format!(
                "{}/models",
                self.endpoint().replace("/v2/chat", "")
            ))
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;

        response.is_ok()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "command-a-03-2025".to_string(),
            "command-r-plus-08-2024".to_string(),
            "command-r-08-2024".to_string(),
            "command-r7b-12-2024".to_string(),
            "command".to_string(),
            "command-light".to_string(),
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
        let api_key = self.api_key()?;
        let endpoint = self.endpoint();
        let model = request.model.clone();

        let messages = Self::convert_messages(request.messages.clone());
        let tools = request
            .tools
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|t| Self::convert_tools(t));

        let request_body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true,
            "preamble": request.system_prompt,
            "tools": tools,
            "tool_choice": request.tool_choice,
        });

        let response = self
            .client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("failed to connect to Cohere API: {}", e))
            })?;

        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(Self::handle_stream_error(
                status.as_u16(),
                error_text,
                &headers,
            ));
        }

        let bytes_stream = response.bytes_stream();
        let line_buffer = crate::sse::SseByteBuffer::new();

        let sse_stream = bytes_stream.map(move |chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("failed to read chunk: {}", e)))?;
            let mut chunks = Vec::new();

            let lines = line_buffer.feed_chunk(&chunk);
            for line in &lines {
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        break;
                    }
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        let event_type = data.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        match event_type {
                            // v2: content-delta — streaming text tokens
                            "content-delta" => {
                                if let Some(delta_text) = data
                                    .get("delta")
                                    .and_then(|d| d.get("message"))
                                    .and_then(|m| m.get("content"))
                                    .and_then(|c| c.get("text"))
                                    .and_then(|t| t.as_str())
                                {
                                    if !delta_text.is_empty() {
                                        chunks.push(delta_text.to_string());
                                    }
                                }
                            }
                            // v2: tool-plan-delta — streaming tool planning text
                            "tool-plan-delta" => {
                                if let Some(plan_text) = data
                                    .get("delta")
                                    .and_then(|d| d.get("message"))
                                    .and_then(|m| m.get("tool_plan"))
                                    .and_then(|t| t.as_str())
                                {
                                    if !plan_text.is_empty() {
                                        chunks.push(plan_text.to_string());
                                    }
                                }
                            }
                            // v2: tool-call-delta — streaming tool call arguments
                            "tool-call-delta" => {
                                if let Some(func_delta) = data
                                    .get("delta")
                                    .and_then(|d| d.get("message"))
                                    .and_then(|m| m.get("tool_calls"))
                                    .and_then(|tc| tc.get(0))
                                    .and_then(|tc| tc.get("function"))
                                    .and_then(|f| f.get("arguments"))
                                    .and_then(|a| a.as_str())
                                {
                                    if !func_delta.is_empty() {
                                        chunks.push(func_delta.to_string());
                                    }
                                }
                            }
                            _ => {
                                // Fallback: v1-style { "text": "..." }
                                if let Some(text_val) = data.get("text").and_then(|t| t.as_str()) {
                                    if !text_val.is_empty() {
                                        chunks.push(text_val.to_string());
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
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(provider.name(), "cohere");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = CohereProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = CohereProvider::metadata();
        assert_eq!(metadata.display_name, "Cohere");
        assert_eq!(metadata.provider_id, "cohere");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = CohereProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = CohereProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"COHERE_API_KEY".to_string())
        );
    }

    #[test]
    fn test_metadata_recommended_models() {
        let metadata = CohereProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(model_ids.iter().any(|id| id.contains("command")));
        assert!(model_ids.iter().any(|id| id.contains("command-a")));
    }

    #[test]
    fn test_all_recommended_models_support_tools() {
        let metadata = CohereProvider::metadata();
        for model in &metadata.recommended_models {
            assert!(
                model.supports_tools,
                "Model {} should support tools",
                model.model_id
            );
        }
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(provider.endpoint(), COHERE_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://custom-cohere.example.com/v2/chat".to_string());
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(
            provider.endpoint(),
            "https://custom-cohere.example.com/v2/chat"
        );
    }

    #[test]
    fn test_cohere_v2_request_serialization() {
        let request = CohereV2Request {
            model: "command-r".to_string(),
            messages: vec![CohereV2Message {
                role: "user".to_string(),
                content: Some(serde_json::json!("What is Rust?")),
                tool_calls: None,
                tool_call_id: None,
            }],
            max_tokens: Some(512),
            temperature: Some(0.3),
            preamble: Some("You are a coding assistant".to_string()),
            tools: None,
            tool_choice: None,
            stream: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"command-r\""));
        assert!(json.contains("\"preamble\":\"You are a coding assistant\""));
        // Optional fields should be absent when None
        assert!(!json.contains("\"tools\""));
        assert!(!json.contains("\"stream\""));
    }

    #[test]
    fn test_cohere_v2_response_deserialization_text() {
        let json = r#"{
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Rust is a systems programming language."}]
            },
            "finish_reason": "COMPLETE",
            "meta": {
                "usage": {
                    "tokens": {"input_tokens": 10, "output_tokens": 20}
                },
                "api_version": {"version": "2.0"}
            }
        }"#;
        let response: CohereV2Response = serde_json::from_str(json).unwrap();
        let msg = response.message.unwrap();
        assert_eq!(msg.content.len(), 1);
        assert!(msg.tool_calls.is_none());
        assert_eq!(response.finish_reason, Some("COMPLETE".to_string()));
    }

    #[test]
    fn test_cohere_v2_response_deserialization_with_tool_calls() {
        let json = r#"{
            "message": {
                "role": "assistant",
                "content": [],
                "tool_plan": "I need to search for information.",
                "tool_calls": [
                    {
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"query\": \"rust programming\"}"
                        }
                    },
                    {
                        "id": "call_def456",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\": \"/tmp/test.rs\"}"
                        }
                    }
                ]
            },
            "finish_reason": "TOOL_CALL"
        }"#;
        let response: CohereV2Response = serde_json::from_str(json).unwrap();
        let msg = response.message.unwrap();
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "search");
        assert_eq!(calls[1].function.name, "read_file");
        assert_eq!(
            msg.tool_plan,
            Some("I need to search for information.".to_string())
        );
    }

    #[test]
    fn test_extract_response_with_text() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();

        let cohere_resp = CohereV2Response {
            message: Some(CohereV2AssistantMessage {
                role: Some("assistant".to_string()),
                content: vec![CohereV2ContentBlock::Text {
                    text: "Hello!".to_string(),
                }],
                tool_plan: None,
                tool_calls: None,
            }),
            finish_reason: Some("COMPLETE".to_string()),
            meta: None,
        };

        let result = provider.extract_response(cohere_resp, "command-r".to_string());
        assert_eq!(result.content, "Hello!");
        assert_eq!(result.model, "command-r");
    }

    #[test]
    fn test_extract_response_with_tool_calls() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();

        let cohere_resp = CohereV2Response {
            message: Some(CohereV2AssistantMessage {
                role: Some("assistant".to_string()),
                content: vec![],
                tool_plan: Some("I need to search.".to_string()),
                tool_calls: Some(vec![CohereV2ToolCall {
                    id: "call_123".to_string(),
                    r#type: "function".to_string(),
                    function: CohereV2ToolCallFunction {
                        name: "search".to_string(),
                        arguments: r#"{"query":"rust"}"#.to_string(),
                    },
                }]),
            }),
            finish_reason: Some("TOOL_CALL".to_string()),
            meta: None,
        };

        let result = provider.extract_response(cohere_resp, "command-r".to_string());
        assert!(result.content.starts_with("I need to search."));
        assert!(result.content.contains("search"));
        assert!(result.content.contains("call_123"));
    }

    #[test]
    fn test_convert_messages_simple() {
        use crate::provider::{ChatMessage, MessageRole};

        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: rustycode_protocol::message::MessageContent::Simple("Hello".to_string()),
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: rustycode_protocol::message::MessageContent::Simple(
                    "Hi there!".to_string(),
                ),
            },
        ];

        let converted = CohereProvider::convert_messages(messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }

    #[test]
    fn test_convert_messages_with_tool_result() {
        use crate::provider::{ChatMessage, MessageRole};
        use rustycode_protocol::message::{ContentBlock, MessageContent};

        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Blocks(vec![ContentBlock::Text {
                    text: "What is 2+2?".to_string(),
                    cache_control: None,
                }]),
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                    id: "call_1".to_string(),
                    name: "calculator".to_string(),
                    input: serde_json::json!({"expr": "2+2"}),
                }]),
            },
            ChatMessage {
                role: MessageRole::Tool("call_1".to_string()),
                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1".to_string(),
                    content: "4".to_string(),
                    is_error: false,
                }]),
            },
        ];

        let converted = CohereProvider::convert_messages(messages);
        assert_eq!(converted.len(), 3);
        // Assistant message should have tool_calls
        assert!(converted[1].tool_calls.is_some());
        let calls = converted[1].tool_calls.clone().unwrap();
        assert_eq!(calls[0].function.name, "calculator");
        // Tool result message should have tool_call_id
        assert_eq!(converted[2].tool_call_id, Some("call_1".to_string()));
        assert_eq!(converted[2].role, "tool");
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![serde_json::json!({
            "name": "calculator",
            "description": "Evaluate math expressions",
            "input_schema": {
                "type": "object",
                "properties": {
                    "expr": {"type": "string"}
                },
                "required": ["expr"]
            }
        })];

        let converted = CohereProvider::convert_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["function"]["name"], "calculator");
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("command")));
        assert!(models.iter().any(|m| m.contains("command-a")));
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert!(provider.config().is_some());
    }

    #[test]
    fn test_get_api_key_from_config() {
        let config = make_config(Some("my-cohere-key"));
        let provider = CohereProvider::new(config).unwrap();
        let key = provider.api_key().unwrap();
        assert_eq!(key, "my-cohere-key");
    }
}
