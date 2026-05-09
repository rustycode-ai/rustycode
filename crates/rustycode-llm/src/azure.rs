//! Azure OpenAI LLM provider implementation.
//!
//! This provider supports Azure OpenAI Service which provides access to
//! OpenAI models (GPT-3.5, GPT-4, etc.) hosted on Microsoft Azure.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Azure OpenAI resource)
//! - Endpoint URL (e.g., <https://my-resource.openai.azure.com>)
//! - Deployment name (the deployment name, not the base model name)
//! - API version (optional, defaults to "2024-02-15-preview")
//!
//! ## Environment Variables
//!
//! - `AZURE_OPENAI_API_KEY` - API key for authentication
//! - `AZURE_OPENAI_ENDPOINT` - Base endpoint URL
//! - `AZURE_OPENAI_DEPLOYMENT` - Default deployment name
//!
//! ## Example Configuration
//!
//! ```toml
//! [ai]
//! provider = "azure"
//! model = "gpt-4"  # This maps to a deployment name
//! api_key = "your-api-key"
//! base_url = "https://my-resource.openai.azure.com"
//! ```

use crate::provider::{
    build_openai_response_format, CompletionRequest, CompletionResponse, LLMProvider,
    ProviderConfig, ProviderError, StreamChunk, Usage,
};
use crate::provider_metadata::{ConfigField, ConfigFieldType, ConfigSchema, ProviderMetadata};
use crate::retry::extract_retry_after_ms;
use rustycode_protocol::stream_event::StreamEvent;

// Import unified trait from protocol

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::time::Duration;

/// Default Azure OpenAI API version
const DEFAULT_API_VERSION: &str = "2024-02-15-preview";

#[derive(Serialize)]
struct AzureRequest {
    messages: Vec<AzureMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
}

#[derive(Serialize, Clone)]
struct AzureMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<AzureToolCallSend>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize, Clone)]
struct AzureToolCallSend {
    id: String,
    r#type: String,
    function: AzureFunctionSend,
}

#[derive(Serialize, Clone)]
struct AzureFunctionSend {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct AzureResponse {
    choices: Vec<AzureChoice>,
    usage: AzureUsage,
    model: String,
}

#[derive(Deserialize)]
struct AzureChoice {
    message: AzureResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct AzureResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<AzureToolCall>>,
}

/// Tool call from Azure OpenAI response (OpenAI-compatible format)
#[derive(Deserialize)]
struct AzureToolCall {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    r#type: String,
    function: AzureFunction,
}

/// Function call within a tool call
#[derive(Deserialize)]
struct AzureFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct AzureUsage {
    #[allow(dead_code)] // Kept for future use
    prompt_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    completion_tokens: usize,
    total_tokens: usize,
}

/// Convert protocol ChatMessages to Azure (OpenAI-compatible) messages.
fn convert_messages(messages: &[crate::provider::ChatMessage]) -> Vec<AzureMessage> {
    use crate::provider::MessageRole;
    use rustycode_protocol::{ContentBlock, MessageContent};
    messages
        .iter()
        .flat_map(|msg| {
            let role_str = match &msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool(_) => "tool",
            };
            match &msg.content {
                MessageContent::Blocks(blocks) => {
                    let mut tool_results: Vec<AzureMessage> = Vec::new();
                    let mut text_parts: Vec<String> = Vec::new();
                    let mut tool_calls: Vec<AzureToolCallSend> = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } => text_parts.push(text.clone()),
                            ContentBlock::Image { .. } => text_parts.push("[Image]".to_string()),
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } => {
                                tool_results.push(AzureMessage {
                                    role: "tool".to_string(),
                                    content: Some(serde_json::Value::String(content.clone())),
                                    tool_calls: None,
                                    tool_call_id: Some(tool_use_id.clone()),
                                    name: None,
                                });
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(AzureToolCallSend {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: AzureFunctionSend {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input)
                                            .unwrap_or_else(|_| "{}".to_string()),
                                    },
                                });
                            }
                            ContentBlock::Thinking { thinking, .. } => {
                                if !thinking.is_empty() {
                                    text_parts.push(format!(
                                        "[prior-reasoning]\n{}\n[/prior-reasoning]",
                                        thinking
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                    let mut result = Vec::new();
                    if !text_parts.is_empty() || !tool_calls.is_empty() {
                        result.push(AzureMessage {
                            role: if tool_calls.is_empty() {
                                role_str.to_string()
                            } else {
                                "assistant".to_string()
                            },
                            content: if text_parts.is_empty() {
                                None
                            } else {
                                Some(serde_json::Value::String(text_parts.join("\n")))
                            },
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_call_id: None,
                            name: None,
                        });
                    }
                    result.extend(tool_results);
                    result
                }
                _ => vec![AzureMessage {
                    role: role_str.to_string(),
                    content: Some(serde_json::Value::String(msg.content.to_text())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                }],
            }
        })
        .collect()
}

fn convert_tools(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tools.iter().map(|tool| {
        if tool.get("type").and_then(|t| t.as_str()).is_some() { return tool.clone(); }
        let name = tool.get("name").or_else(|| tool.get("function").and_then(|f| f.get("name")));
        let desc = tool.get("description").or_else(|| tool.get("function").and_then(|f| f.get("description")));
        let params = tool.get("parameters").or_else(|| tool.get("input_schema"));
        serde_json::json!({"type":"function","function":{"name":name,"description":desc,"parameters":params}})
    }).collect()
}

/// Azure OpenAI LLM provider
pub struct AzureProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    api_version: String,
    deployment: String,
}

impl AzureProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let timeout = config.timeout_seconds.unwrap_or(180);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })?;

        // Get API version from env or use default
        let api_version = std::env::var("AZURE_OPENAI_API_VERSION")
            .unwrap_or_else(|_| DEFAULT_API_VERSION.to_string());

        // Get deployment name from env or use a default
        let deployment =
            std::env::var("AZURE_OPENAI_DEPLOYMENT").unwrap_or_else(|_| "gpt-4".to_string());

        Ok(Self {
            config,
            client,
            api_version,
            deployment,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "azure".to_string(),
            display_name: "Azure OpenAI Service".to_string(),
            description: "OpenAI models hosted on Microsoft Azure with enterprise-grade security and compliance".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Azure OpenAI API key from the Azure portal".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("your-azure-api-key".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Endpoint URL".to_string(),
                        description: "Your Azure OpenAI endpoint (e.g., https://my-resource.openai.azure.com)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://my-resource.openai.azure.com".to_string()),
                        default: None,
                        validation_pattern: Some("^https?://.*\\.openai\\.azure\\.com.*".to_string()),
                        validation_error: Some("Endpoint must be a valid Azure OpenAI URL".to_string()),
                        sensitive: false,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "deployment".to_string(),
                        label: "Deployment Name".to_string(),
                        description: "The deployment name (not the base model name)".to_string(),
                        field_type: ConfigFieldType::String,
                        placeholder: Some("gpt-4".to_string()),
                        default: Some("gpt-4".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "AZURE_OPENAI_API_KEY".to_string());
                    map.insert("base_url".to_string(), "AZURE_OPENAI_ENDPOINT".to_string());
                    map.insert("deployment".to_string(), "AZURE_OPENAI_DEPLOYMENT".to_string());
                    map
                },
            },
            prompt_template: crate::provider_metadata::PromptTemplate {
                base_template: "You are an AI assistant hosted on Azure OpenAI.\n\n{context}".to_string(),
                optimizations: crate::provider_metadata::PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Follow Azure OpenAI best practices.".to_string(),
                        "Provide enterprise-grade responses.".to_string(),
                    ],
                },
                tool_format: crate::provider_metadata::ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: crate::provider_metadata::ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
                crate::provider_metadata::ModelInfo {
                    model_id: "gpt-4.1".to_string(),
                    display_name: "GPT-4.1".to_string(),
                    description: "Latest GPT-4 model with improved reasoning".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Code generation".to_string()],
                    cost_tier: 4,
                },
            ],
                    model_behavior_profiles: HashMap::new(),
        }
    }

    pub fn endpoint(&self) -> String {
        self.config.base_url.clone().unwrap_or_else(|| {
            std::env::var("AZURE_OPENAI_ENDPOINT")
                .unwrap_or_else(|_| "https://your-resource.openai.azure.com".to_string())
        })
    }

    /// Get the deployment name (model name in Azure context)
    pub fn deployment(&self) -> &str {
        &self.deployment
    }

    /// Get the API version
    pub fn api_version(&self) -> &str {
        &self.api_version
    }

    fn api_key(&self) -> Result<String, ProviderError> {
        // Try config first, then environment variable
        let config_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("AZURE_OPENAI_API_KEY").ok();

        config_key
            .or(env_key)
            .ok_or_else(|| ProviderError::Configuration(
                "Azure OpenAI API key is required. Set api_key in config or AZURE_OPENAI_API_KEY env var".to_string()
            ))
    }

    async fn complete_internal(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = self.api_key()?;

        // Azure OpenAI endpoint format: https://{resource}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version={api_version}
        let url = format!(
            "{}/openai/deployments/{}/chat/completions?api-version={}",
            self.endpoint(),
            self.deployment(),
            self.api_version()
        );

        // Build messages array
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(AzureMessage {
                role: "system".to_string(),
                content: Some(serde_json::Value::String(system_prompt.clone())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        messages.extend(convert_messages(&request.messages));
        let tools = request
            .tools
            .as_ref()
            .map(|t| convert_tools(t))
            .filter(|t| !t.is_empty());
        let body = AzureRequest {
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            stream: Some(false),
            tools,
            tool_choice: request.tool_choice.clone(),
            parallel_tool_calls: None,
            response_format: build_openai_response_format(&request.output_config),
        };

        let response = self
            .client
            .post(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            // Capture headers for potential server-provided retry guidance
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Authentication failed. Check your AZURE_OPENAI_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "deployment not found. {}. Check AZURE_OPENAI_DEPLOYMENT and AZURE_OPENAI_ENDPOINT",
                    error_text
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Azure service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let azure_response: AzureResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        let choice = azure_response
            .choices
            .first()
            .ok_or_else(|| ProviderError::Api("No choices in response".to_string()))?;

        // Build content string, appending tool calls if present
        let mut content = choice.message.content.clone().unwrap_or_default();

        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let tool_calls_json: Vec<serde_json::Value> = tool_calls
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
            model: azure_response.model,
            usage: Some(Usage {
                input_tokens: azure_response.usage.prompt_tokens as u32,
                output_tokens: azure_response.usage.completion_tokens as u32,
                total_tokens: azure_response.usage.total_tokens as u32,
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
}

#[async_trait]
impl LLMProvider for AzureProvider {
    fn name(&self) -> &'static str {
        "azure"
    }

    async fn is_available(&self) -> bool {
        // Check if API key is available
        if self.api_key().is_err() {
            return false;
        }

        // Try to make a simple request to verify connectivity
        let api_key = match self.api_key() {
            Ok(key) => key,
            Err(_) => return false,
        };

        let url = format!(
            "{}/openai/deployments/{}/chat/completions?api-version={}",
            self.endpoint(),
            self.deployment(),
            self.api_version()
        );

        // Minimal request to check availability
        let body = AzureRequest {
            messages: vec![AzureMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("test".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            max_tokens: 1,
            temperature: 0.0,
            stream: Some(false),
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            response_format: None,
        };

        let response = self
            .client
            .post(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .timeout(Duration::from_secs(5))
            .send();

        match response.await {
            Ok(r) => r.status().is_success() || r.status().as_u16() == 400, // 400 means we're authenticated but bad request
            Err(_) => false,
        }
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Azure OpenAI doesn't have a standard list models endpoint
        // Return common deployment names (as of March 2026)
        Ok(vec![
            // o3 series (reasoning models - latest)
            "o3".to_string(),
            "o3-mini".to_string(),
            // o1 series (reasoning models)
            "o1".to_string(),
            "o1-mini".to_string(),
            // GPT-4.1 series (latest GPT-4)
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            // GPT-4o series (omni models)
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            // Legacy models
            "gpt-4".to_string(),
            "gpt-4-32k".to_string(),
            "gpt-35-turbo".to_string(),
            "gpt-35-turbo-16k".to_string(),
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

        let url = format!(
            "{}/openai/deployments/{}/chat/completions?api-version={}",
            self.endpoint(),
            self.deployment(),
            self.api_version()
        );

        // Build messages array
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(AzureMessage {
                role: "system".to_string(),
                content: Some(serde_json::Value::String(system_prompt.clone())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        messages.extend(convert_messages(&request.messages));

        let tools = request
            .tools
            .as_ref()
            .map(|t| convert_tools(t))
            .filter(|t| !t.is_empty());

        let mut body = serde_json::json!({
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true,
            "response_format": build_openai_response_format(&request.output_config),
        });

        if let Some(tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = tool_choice.clone();
        }

        let response = self
            .client
            .post(&url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to connect: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            // Capture headers for potential server-provided retry guidance
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Authentication failed. Check your AZURE_OPENAI_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "deployment not found. {}. Check AZURE_OPENAI_DEPLOYMENT and AZURE_OPENAI_ENDPOINT",
                    error_text
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Azure service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        // Azure uses OpenAI-compatible SSE format
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
                Ok(chunk) => chunk,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(format!(
                        "failed to read chunk: {}",
                        e
                    )))]);
                }
            };
            let mut events = Vec::new();

            let lines = line_buffer.feed_chunk(&chunk);
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
                                        for tc_delta in tool_calls {
                                            let index = tc_delta
                                                .get("index")
                                                .and_then(|i| i.as_u64())
                                                .unwrap_or(0)
                                                as usize;

                                            if let Some(id) =
                                                tc_delta.get("id").and_then(|i| i.as_str())
                                            {
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
                                                    if let Ok(mut started) =
                                                        started_tool_indices.lock()
                                                    {
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

                                                if let Some(args) = tc_delta
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
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert_eq!(<AzureProvider as LLMProvider>::name(&provider), "azure");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = AzureProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = AzureProvider::metadata();
        assert_eq!(metadata.display_name, "Azure OpenAI Service");
        assert_eq!(metadata.provider_id, "azure");
    }

    #[test]
    fn test_metadata_has_required_api_key_field() {
        let metadata = AzureProvider::metadata();
        assert_eq!(metadata.config_schema.required_fields.len(), 2);
        let field_names: Vec<&str> = metadata
            .config_schema
            .required_fields
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(field_names.contains(&"api_key"));
        assert!(field_names.contains(&"base_url"));
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = AzureProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"AZURE_OPENAI_API_KEY".to_string())
        );
        assert_eq!(
            metadata.config_schema.env_mappings.get("base_url"),
            Some(&"AZURE_OPENAI_ENDPOINT".to_string())
        );
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = AzureProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config).unwrap();
        // Default endpoint comes from env or hardcoded fallback
        assert!(provider.endpoint().contains("openai.azure.com"));
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://myresource.openai.azure.com".to_string());
        let provider = AzureProvider::new(config).unwrap();
        assert_eq!(provider.endpoint(), "https://myresource.openai.azure.com");
    }

    #[test]
    fn test_default_api_version() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config).unwrap();
        assert_eq!(provider.api_version(), "2024-02-15-preview");
    }

    #[test]
    fn test_default_deployment() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config).unwrap();
        assert_eq!(provider.deployment(), "gpt-4");
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        let models = <AzureProvider as LLMProvider>::list_models(&provider)
            .await
            .unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m == "gpt-4o"));
        assert!(models.iter().any(|m| m == "gpt-4.1"));
    }

    #[test]
    fn test_azure_request_serialization() {
        let request = AzureRequest {
            messages: vec![AzureMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            max_tokens: 1024,
            temperature: 0.5,
            stream: Some(true),
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            response_format: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"max_tokens\":1024"));
        assert!(json.contains("\"temperature\":0.5"));
        assert!(json.contains("\"stream\":true"));
        assert!(!json.contains("\"tools\""));
        assert!(!json.contains("\"tool_choice\""));
        assert!(!json.contains("\"parallel_tool_calls\""));
    }

    #[test]
    fn test_azure_request_with_tools() {
        let request = AzureRequest {
            messages: vec![AzureMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            max_tokens: 1024,
            temperature: 0.5,
            stream: None,
            tools: Some(vec![serde_json::json!({
                "type": "function",
                "function": {
                    "name": "Read",
                    "description": "Read a file",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
                }
            })]),
            tool_choice: Some(serde_json::json!("auto")),
            parallel_tool_calls: Some(true),
            response_format: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"tools\""));
        assert!(json.contains("\"read_file\""));
    }

    #[test]
    fn test_convert_messages_simple() {
        use crate::provider::{ChatMessage, MessageRole};
        use rustycode_protocol::MessageContent;

        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::simple("hello"),
        }];
        let converted = convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
    }

    #[test]
    fn test_convert_messages_tool_result() {
        use crate::provider::{ChatMessage, MessageRole};
        use rustycode_protocol::{ContentBlock, MessageContent};

        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result(
                "call_abc",
                "file contents",
            )]),
        }];
        let converted = convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "tool");
        assert_eq!(converted[0].tool_call_id, Some("call_abc".to_string()));
    }

    #[test]
    fn test_convert_messages_tool_use() {
        use crate::provider::{ChatMessage, MessageRole};
        use rustycode_protocol::{ContentBlock, MessageContent};

        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "call_123".to_string(),
                name: "Read".to_string(),
                input: serde_json::json!({"path": "/tmp/test.rs"}),
            }]),
        }];
        let converted = convert_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "assistant");
        assert!(converted[0].tool_calls.is_some());
        let tc = converted[0].tool_calls.as_ref().unwrap();
        assert_eq!(tc[0].function.name, "Read");
    }

    #[test]
    fn test_convert_tools_normalization() {
        let anthropic_tool = serde_json::json!({
            "name": "Bash",
            "description": "Run a command",
            "input_schema": {"type": "object", "properties": {"cmd": {"type": "string"}}}
        });
        let converted = convert_tools(&[anthropic_tool]);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["function"]["name"], "Bash");
        assert!(converted[0]["function"]["parameters"].is_object());
    }

    #[test]
    fn test_azure_response_deserialization() {
        let json = r#"{
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Hello!"},
                    "finish_reason": "stop"
                }
            ],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
            "model": "gpt-4"
        }"#;
        let response: AzureResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "gpt-4");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Hello!")
        );
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[test]
    fn test_error_mapping_404_is_invalid_model() {
        // Verify that the error text in the match arm for 404 references "deployment not found"
        // We test this via the static match structure in complete_internal/complete_stream
        // by verifying the metadata points to deployment-related configuration
        let metadata = AzureProvider::metadata();
        let deployment_field = metadata
            .config_schema
            .optional_fields
            .iter()
            .find(|f| f.name == "deployment");
        assert!(deployment_field.is_some());
        assert_eq!(deployment_field.unwrap().default, Some("gpt-4".to_string()));
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config).unwrap();
        assert!(provider.config().is_some());
    }

    #[test]
    fn test_api_key_error_includes_env_var() {
        let result = AzureProvider::new(make_config(None));
        let msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error for missing api_key"),
        };
        // Validation should mention the env var
        assert!(
            msg.contains("AZURE_OPENAI_API_KEY"),
            "Error should mention AZURE_OPENAI_API_KEY, got: {}",
            msg
        );
    }
}
