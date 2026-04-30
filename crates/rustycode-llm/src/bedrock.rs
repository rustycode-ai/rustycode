//! AWS Bedrock LLM provider implementation.
//!
//! This provider supports AWS Bedrock which offers access to foundation models
//! from Anthropic, AI21, Meta, Mistral, and more through a single API.
//!
//! ## Configuration
//!
//! The provider can be configured with:
//! - Direct AWS credentials (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
//! - API key via the `api_key` field (for simpler setups)
//! - Custom endpoint for AWS Bedrock proxies
//!
//! ## Supported Models
//!
//! - Anthropic Claude (anthropic.claude-3-sonnet, claude-3-haiku, claude-3-opus)
//! - Meta Llama (meta.llama3-8b-instant, llama3-70b-instruct)
//! - Mistral AI (mistral.large-2407, mistral.small-2402)
//! - AI21 Jurassic (ai21.jamba-1-5-large, jamba-instruct)

use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{ConfigField, ConfigFieldType, ConfigSchema, ProviderMetadata};
use crate::retry::{extract_retry_after_ms, retry_with_backoff, RetryConfig};

// Import unified trait from protocol
use rustycode_protocol::llm::{
    CompletionRequest as UnifiedCompletionRequest, CompletionResponse as UnifiedCompletionResponse,
    Cost as UnifiedCost, LLMProvider as UnifiedLLMProvider, ModelInfo as UnifiedModelInfo,
    TokenCount as UnifiedTokenCount,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default AWS Bedrock endpoint format
#[allow(dead_code)] // Kept for future use
const BEDROCK_ENDPOINT_FORMAT: &str = "https://bedrock-runtime.{}.amazonaws.com";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockRequest {
    messages: Vec<BedrockConverseMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Vec<BedrockSystemContent>>,
    inference_config: BedrockInferenceConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<BedrockToolConfig>,
}

#[derive(Serialize)]
struct BedrockSystemContent {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockInferenceConfig {
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockToolConfig {
    tools: Vec<BedrockTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockTool {
    tool_spec: BedrockToolSpec,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockToolSpec {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: BedrockInputSchema,
}

#[derive(Serialize)]
struct BedrockInputSchema {
    json: serde_json::Value,
}

#[derive(Serialize, Clone)]
struct BedrockConverseMessage {
    role: String,
    content: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct BedrockResponse {
    output: BedrockOutput,
    usage: BedrockUsage,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct BedrockOutput {
    message: BedrockResponseMessage,
}

#[derive(Deserialize)]
struct BedrockResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Vec<BedrockResponseContent>,
}

#[derive(Deserialize)]
struct BedrockResponseContent {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    content_type: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    tool_use: Option<BedrockResponseToolUse>,
}

#[derive(Deserialize)]
struct BedrockResponseToolUse {
    tool_use_id: String,
    name: String,
    input: serde_json::Value,
}

#[derive(Deserialize)]
struct BedrockUsage {
    #[allow(dead_code)] // Kept for future use
    input_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    output_tokens: usize,
    total_tokens: usize,
}

/// Convert protocol ChatMessages to Bedrock Converse API format.
fn convert_messages(messages: &[crate::provider::ChatMessage]) -> Vec<BedrockConverseMessage> {
    use crate::provider::MessageRole;
    use rustycode_protocol::{ContentBlock, MessageContent};

    messages
        .iter()
        .flat_map(|msg| {
            let role = match &msg.role {
                MessageRole::User | MessageRole::Tool(_) => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "user",
            };

            match &msg.content {
                MessageContent::Blocks(blocks) => {
                    let mut parts: Vec<serde_json::Value> = Vec::new();
                    let mut tool_results: Vec<BedrockConverseMessage> = Vec::new();

                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } => {
                                parts.push(serde_json::json!({ "text": text }));
                            }
                            ContentBlock::Image { .. } => {
                                parts.push(serde_json::json!({ "text": "[Image]" }));
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                parts.push(serde_json::json!({
                                    "toolUse": { "toolUseId": id, "name": name, "input": input }
                                }));
                            }
                            ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                                let status = if *is_error { "error" } else { "success" };
                                tool_results.push(BedrockConverseMessage {
                                    role: "user".to_string(),
                                    content: vec![serde_json::json!({
                                        "toolResult": {
                                            "toolUseId": tool_use_id,
                                            "content": [{ "text": content }],
                                            "status": status
                                        }
                                    })],
                                });
                            }
                            ContentBlock::Thinking { thinking, .. } => {
                                if !thinking.is_empty() {
                                    parts.push(serde_json::json!({
                                        "text": format!("[prior-reasoning]\n{}\n[/prior-reasoning]", thinking)
                                    }));
                                }
                            }
                            _ => {}
                        }
                    }

                    let mut result = Vec::new();
                    if !parts.is_empty() {
                        result.push(BedrockConverseMessage { role: role.to_string(), content: parts });
                    }
                    result.extend(tool_results);
                    result
                }
                _ => {
                    vec![BedrockConverseMessage {
                        role: role.to_string(),
                        content: vec![serde_json::json!({ "text": msg.content.to_text() })],
                    }]
                }
            }
        })
        .collect()
}

/// Convert tool definitions to Bedrock toolSpec format.
fn convert_tools(tools: &[serde_json::Value]) -> Vec<BedrockTool> {
    tools
        .iter()
        .map(|tool| {
            let name = tool
                .get("name")
                .or_else(|| tool.get("function").and_then(|f| f.get("name")))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let description = tool
                .get("description")
                .or_else(|| tool.get("function").and_then(|f| f.get("description")))
                .and_then(|v| v.as_str());
            let parameters = tool
                .get("parameters")
                .or_else(|| tool.get("input_schema"))
                .cloned()
                .unwrap_or(serde_json::json!({"type": "object", "properties": {}}));

            BedrockTool {
                tool_spec: BedrockToolSpec {
                    name: name.to_string(),
                    description: description.map(|d| d.to_string()),
                    input_schema: BedrockInputSchema { json: parameters },
                },
            }
        })
        .collect()
}

/// Convert tool_choice to Bedrock toolChoice format.
fn convert_tool_choice(tc: &serde_json::Value) -> serde_json::Value {
    match tc.as_str() {
        Some("auto") | Some("AUTO") => serde_json::json!({"auto": {}}),
        Some("required") | Some("any") => serde_json::json!({"any": {}}),
        Some(name) => serde_json::json!({"tool": {"name": name}}),
        None => serde_json::json!({"auto": {}}),
    }
}

/// AWS Bedrock LLM provider
pub struct BedrockProvider {
    config: ProviderConfig,
    region: String,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    model: String,
}

impl BedrockProvider {
    pub fn new(config: ProviderConfig, model: String) -> Result<Self> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        // Get AWS region from config or environment
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| {
                // Extract region from model name if possible (e.g., "us-east-1")
                if model.contains('.') {
                    model
                        .split('.')
                        .next_back()
                        .unwrap_or("us-east-1")
                        .to_string()
                } else {
                    "us-east-1".to_string()
                }
            });

        // Check for AWS credentials or API key
        let _has_aws_creds = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok();

        // Use API key from config if provided
        let api_key = config.api_key.as_ref().map(|k| k.expose_secret());

        // Create HTTP client with headers
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        // Add API key header if provided (for custom endpoints/proxies)
        if let Some(key) = api_key {
            headers.insert(
                reqwest::header::HeaderName::from_static("x-api-key"),
                reqwest::header::HeaderValue::from_str(key).map_err(|e| {
                    ProviderError::Configuration(format!("invalid API key format: {}", e))
                })?,
            );
        }

        let timeout = config.timeout_seconds.unwrap_or(180);
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })?;

        Ok(Self {
            config,
            region,
            client,
            model,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "bedrock".to_string(),
            display_name: "AWS Bedrock".to_string(),
            description: "Foundation models from Anthropic, Meta, Mistral, and more through AWS"
                .to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![],
                optional_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "AWS access key ID or custom endpoint API key".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Custom Endpoint".to_string(),
                        description: "Custom Bedrock endpoint (for proxies or custom deployments)"
                            .to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some(
                            "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
                        ),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "AWS_ACCESS_KEY_ID".to_string());
                    map
                },
            },
            prompt_template: crate::provider_metadata::PromptTemplate {
                base_template: "You are an AI assistant hosted on AWS Bedrock.\n\n{context}"
                    .to_string(),
                optimizations: crate::provider_metadata::PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Follow AWS best practices.".to_string(),
                        "Provide secure, enterprise-grade responses.".to_string(),
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
            recommended_models: vec![crate::provider_metadata::ModelInfo {
                model_id: "anthropic.claude-3-5-sonnet-20240620-v1:0".to_string(),
                display_name: "Claude 3.5 Sonnet".to_string(),
                description: "Balanced performance and speed".to_string(),
                context_window: 200_000,
                supports_tools: true,
                use_cases: vec!["General assistance".to_string(), "Coding".to_string()],
                cost_tier: 3,
            }],
        }
    }

    pub fn endpoint(&self) -> String {
        if let Some(endpoint) = &self.config.base_url {
            endpoint.clone()
        } else {
            format!("https://bedrock-runtime.{}.amazonaws.com", self.region)
        }
    }

    /// Get the AWS region for this provider
    pub fn region(&self) -> &str {
        &self.region
    }

    async fn complete_internal(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let url = format!("{}/model/{}/converse", self.endpoint(), request.model);

        let system = request
            .system_prompt
            .as_ref()
            .map(|s| vec![BedrockSystemContent { text: s.clone() }]);
        let messages = convert_messages(&request.messages);
        let tool_config = request
            .tools
            .as_ref()
            .map(|tools| {
                let bt = convert_tools(tools);
                let tc = request.tool_choice.as_ref().map(convert_tool_choice);
                BedrockToolConfig {
                    tools: bt,
                    tool_choice: tc,
                }
            })
            .filter(|tc| !tc.tools.is_empty());

        let request_body = BedrockRequest {
            messages,
            system,
            inference_config: BedrockInferenceConfig {
                max_tokens: request.max_tokens.unwrap_or(4096),
                temperature: request.temperature.unwrap_or(0.7),
            },
            tool_config,
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .context("request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Bedrock API error: {} - {}",
                status.as_u16(),
                error_text
            ));
        }

        let br: BedrockResponse = response.json().await.context("failed to parse response")?;

        let mut parts: Vec<String> = Vec::new();
        let mut tc_json: Vec<serde_json::Value> = Vec::new();
        for c in &br.output.message.content {
            if let Some(text) = &c.text {
                if !text.is_empty() {
                    parts.push(text.clone());
                }
            }
            if let Some(tu) = &c.tool_use {
                tc_json.push(serde_json::json!({
                    "id": tu.tool_use_id, "type": "function",
                    "function": { "name": tu.name, "arguments": serde_json::to_string(&tu.input).unwrap_or_else(|_| "{}".to_string()) }
                }));
            }
        }
        let mut content = parts.join("\n");
        if !tc_json.is_empty() {
            let fmt = serde_json::to_string_pretty(&tc_json).unwrap_or_else(|_| "[]".to_string());
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&format!("```tool\n{}\n```", fmt));
        }

        Ok(CompletionResponse {
            content,
            model: request.model.clone(),
            usage: Some(crate::provider::Usage {
                input_tokens: br.usage.input_tokens as u32,
                output_tokens: br.usage.output_tokens as u32,
                total_tokens: br.usage.total_tokens as u32,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: crate::provider::normalize_stop_reason(br.stop_reason.as_deref()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    #[allow(dead_code)] // Kept for future use
    async fn complete_v2(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/model/{}/converse", self.endpoint(), request.model);

        let system = request
            .system_prompt
            .as_ref()
            .map(|s| vec![BedrockSystemContent { text: s.clone() }]);
        let messages = convert_messages(&request.messages);
        let tool_config = request
            .tools
            .as_ref()
            .map(|tools| {
                let bt = convert_tools(tools);
                let tc = request.tool_choice.as_ref().map(convert_tool_choice);
                BedrockToolConfig {
                    tools: bt,
                    tool_choice: tc,
                }
            })
            .filter(|tc| !tc.tools.is_empty());

        let request_body = BedrockRequest {
            messages,
            system,
            inference_config: BedrockInferenceConfig {
                max_tokens: request.max_tokens.unwrap_or(4096),
                temperature: request.temperature.unwrap_or(0.7),
            },
            tool_config,
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("request failed: {}", e)))?;

        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Bedrock authentication failed. Check AWS credentials (aws configure). {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found: {}. Check model ID and region in AWS Bedrock console",
                    request.model
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Bedrock service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("Bedrock API error: {} - {}", status, error_text)),
            });
        }

        let bedrock_response: BedrockResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        let mut parts: Vec<String> = Vec::new();
        for c in &bedrock_response.output.message.content {
            if let Some(text) = &c.text {
                if !text.is_empty() {
                    parts.push(text.clone());
                }
            }
        }

        Ok(CompletionResponse {
            content: parts.join("\n"),
            model: request.model.clone(),
            usage: Some(crate::provider::Usage {
                input_tokens: bedrock_response.usage.input_tokens as u32,
                output_tokens: bedrock_response.usage.output_tokens as u32,
                total_tokens: bedrock_response.usage.total_tokens as u32,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }
}

#[async_trait]
impl LLMProvider for BedrockProvider {
    fn name(&self) -> &'static str {
        "bedrock"
    }

    async fn is_available(&self) -> bool {
        // We'll do a simple check - if we have AWS creds or API key, consider it available
        let has_credentials = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok();

        let has_api_key = self
            .config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty());

        has_credentials || has_api_key
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return a list of commonly available Bedrock models (as of March 2026)
        // In a real implementation, this would call the Bedrock ListFoundationModels API
        Ok(vec![
            // Claude 4.x series (latest)
            "anthropic.claude-opus-v4:0".to_string(),
            "anthropic.claude-sonnet-v4:0".to_string(),
            "anthropic.claude-haiku-v4:0".to_string(),
            // Claude 3.7 (latest Claude 3)
            "anthropic.claude-3-7-sonnet-20250219-v1:0".to_string(),
            // Claude 3.5 (stable)
            "anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
            "anthropic.claude-3-5-haiku-20241022-v1:0".to_string(),
            // Claude 3 Opus
            "anthropic.claude-3-opus-20240229-v1:0".to_string(),
            // Llama 4.x
            "meta.llama4-8b-instruct-v1:0".to_string(),
            "meta.llama4-405b-instruct-v1:0".to_string(),
            // Llama 3.x
            "meta.llama3-3-70b-instruct-v1:0".to_string(),
            "meta.llama3-1-405b-instruct-v1:0".to_string(),
            "meta.llama3-8b-instruct-v1:0".to_string(),
            "meta.llama3-70b-instruct-v1:0".to_string(),
            // Mistral
            "mistral.mistral-large-2407-v1:0".to_string(),
            "mistral.mistral-small-2402-v1:0".to_string(),
            // AI21
            "ai21.jamba-1-5-large-v1:0".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let retry_config = RetryConfig::new()
            .with_max_attempts(5) // AWS needs more retries
            .with_base_delay(Duration::from_millis(800))
            .with_max_delay(Duration::from_secs(45))
            .with_jitter_factor(0.15);

        // We need to convert between anyhow::Error and ProviderError
        let result = retry_with_backoff(retry_config, || async {
            self.complete_internal(&request).await
        })
        .await;

        match result {
            Ok(response) => Ok(response),
            Err(e) => {
                let error_msg = e.to_string();
                // Parse status code from "Bedrock API error: XXX - ..." format
                let status_code = error_msg
                    .strip_prefix("Bedrock API error: ")
                    .and_then(|rest| rest.split(" - ").next())
                    .and_then(|code| code.parse::<u16>().ok());

                match status_code {
                    Some(401) | Some(403) => Err(ProviderError::Auth(error_msg)),
                    Some(404) => Err(ProviderError::InvalidModel(request.model)),
                    Some(429) => Err(ProviderError::RateLimited { retry_delay: None }),
                    _ => Err(ProviderError::Api(error_msg)),
                }
            }
        }
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = format!(
            "{}/model/{}/converse-stream",
            self.endpoint(),
            request.model
        );

        let system = request
            .system_prompt
            .as_ref()
            .map(|s| vec![BedrockSystemContent { text: s.clone() }]);
        let messages = convert_messages(&request.messages);
        let tool_config = request
            .tools
            .as_ref()
            .map(|tools| {
                let bt = convert_tools(tools);
                let tc = request.tool_choice.as_ref().map(convert_tool_choice);
                BedrockToolConfig {
                    tools: bt,
                    tool_choice: tc,
                }
            })
            .filter(|tc| !tc.tools.is_empty());

        let request_body = BedrockRequest {
            messages,
            system,
            inference_config: BedrockInferenceConfig {
                max_tokens: request.max_tokens.unwrap_or(4096),
                temperature: request.temperature.unwrap_or(0.7),
            },
            tool_config,
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("request failed: {}", e)))?;
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Bedrock authentication failed. Check AWS credentials (aws configure). {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found: {}. Check model ID and region in AWS Bedrock console",
                    request.model
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Bedrock service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("Bedrock API error: {} - {}", status, error_text)),
            });
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        // Parse Bedrock's streaming response format
        let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("failed to read chunk: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);

            let mut current_text = String::new();

            for line in text.lines() {
                if line.is_empty() { continue; }

                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" { continue; }

                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        // Converse-stream: contentBlockDelta
                        if let Some(delta) = data.get("contentBlockDelta") {
                            if let Some(d) = delta.get("delta") {
                                if let Some(t) = d.get("text").and_then(|v| v.as_str()) {
                                    current_text.push_str(t);
                                }
                                if let Some(ti) = d.get("toolUse").and_then(|v| v.get("input")).and_then(|v| v.as_str()) {
                                    current_text.push_str(&format!("```tool_delta\n{{\"function\":{{\"arguments\":\"{}\"}}}}\n```", ti));
                                }
                            }
                        }
                        // contentBlockStart for tool use
                        if let Some(start) = data.get("contentBlockStart") {
                            if let Some(s) = start.get("start") {
                                if let Some(tu) = s.get("toolUse") {
                                    current_text.push_str(&format!("```tool_start\n{}\n```", tu));
                                }
                            }
                        }
                        // Legacy format
                        if let Some(output) = data.get("output") {
                            if let Some(message) = output.get("message") {
                                if let Some(content) = message.get("content") {
                                    if let Some(arr) = content.as_array() {
                                        for item in arr {
                                            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                                                current_text.push_str(t);
                                            }
                                            if let Some(tu) = item.get("toolUse") {
                                                current_text.push_str(&format!("```tool\n{}\n```",
                                                    serde_json::to_string(tu).unwrap_or_default()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !current_text.is_empty() {
                Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta { content: current_text })
            } else {
                Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta { content: String::new() })
            }
        });

        Ok(Box::pin(sse_stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
}

/// Unified LLMProvider trait implementation for AWS Bedrock
///
/// This implementation wraps the provider methods to conform to the unified
/// trait from rustycode-protocol, providing a bridge between the two API versions.
#[async_trait]
impl UnifiedLLMProvider for BedrockProvider {
    async fn list_models(&self) -> Result<Vec<UnifiedModelInfo>> {
        // Use the existing provider list_models and convert results
        let model_names = <Self as LLMProvider>::list_models(self)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list models: {}", e))?;

        Ok(model_names
            .into_iter()
            .map(|name| {
                // Map known Bedrock models to their specifications
                let (context_window, cost_input, cost_output) = match name.as_str() {
                    "anthropic.claude-3-sonnet" => (200000, 0.003, 0.015),
                    "anthropic.claude-3-opus" => (200000, 0.015, 0.075),
                    "meta.llama3-8b-instant" => (8000, 0.0005, 0.001),
                    "meta.llama3-70b-instruct" => (8000, 0.00265, 0.0035),
                    "mistral.large-2407" => (32000, 0.008, 0.024),
                    "mistral.small-2402" => (32000, 0.00014, 0.00042),
                    _ => (8000, 0.001, 0.002), // Default fallback
                };

                UnifiedModelInfo {
                    name,
                    provider: "bedrock".to_string(),
                    context_window,
                    supports_streaming: true,
                    cost_per_1k_input_tokens: cost_input,
                    cost_per_1k_output_tokens: cost_output,
                }
            })
            .collect())
    }

    async fn is_available(&self) -> Result<bool> {
        // Convert provider bool to Result<bool>
        Ok(<Self as LLMProvider>::is_available(self).await)
    }

    async fn complete(
        &self,
        request: UnifiedCompletionRequest,
    ) -> Result<UnifiedCompletionResponse> {
        // Convert unified request to provider request
        use rustycode_protocol::MessageContent;

        let v2_request = CompletionRequest {
            model: request.model.clone(),
            messages: vec![crate::provider::ChatMessage {
                role: crate::provider::MessageRole::User,
                content: MessageContent::Simple(request.prompt.clone()),
            }],
            max_tokens: request.max_tokens.map(|t| t as u32),
            temperature: request.temperature,
            stream: false,
            system_prompt: request.system,
            tools: None,
            thinking: None,
            output_config: None,
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
            session_id: None,
        };

        // Call provider complete
        let v2_response = <Self as LLMProvider>::complete(self, v2_request)
            .await
            .map_err(|e| anyhow::anyhow!("Completion failed: {}", e))?;

        // Extract usage info
        let (input_tokens, output_tokens) = if let Some(usage) = v2_response.usage {
            (usage.input_tokens as usize, usage.output_tokens as usize)
        } else {
            (0, 0)
        };

        // Convert provider response to unified response
        Ok(UnifiedCompletionResponse {
            text: v2_response.content,
            tokens_used: UnifiedTokenCount {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens.saturating_add(output_tokens),
            },
            cost: UnifiedCost {
                input_cost: 0.0,
                output_cost: 0.0,
                total_cost: 0.0,
            },
            finish_reason: v2_response.stop_reason.unwrap_or_default(),
        })
    }

    fn name(&self) -> &'static str {
        <Self as LLMProvider>::name(self)
    }

    fn estimate_cost(&self, request: &UnifiedCompletionRequest) -> Result<UnifiedCost> {
        let input_tokens = request.prompt.len() as f64 / 4.0;
        let output_tokens = request.max_tokens.unwrap_or(1000) as f64 / 2.0;

        let (cost_in, cost_out) = match request.model.as_str() {
            "anthropic.claude-3-sonnet" => (0.003, 0.015),
            "anthropic.claude-3-opus" => (0.015, 0.075),
            "meta.llama3-8b-instant" => (0.0005, 0.001),
            "meta.llama3-70b-instruct" => (0.00265, 0.0035),
            "mistral.large-2407" => (0.008, 0.024),
            "mistral.small-2402" => (0.00014, 0.00042),
            _ => (0.001, 0.002),
        };

        let input_cost = (input_tokens * cost_in) / 1000.0;
        let output_cost = (output_tokens * cost_out) / 1000.0;

        Ok(UnifiedCost {
            input_cost,
            output_cost,
            total_cost: input_cost + output_cost,
        })
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
        let provider =
            BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert_eq!(<BedrockProvider as LLMProvider>::name(&provider), "bedrock");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_creates_without_api_key() {
        let config = make_config(None);
        let provider = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_metadata_tool_calling_enabled() {
        let metadata = BedrockProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_convert_messages_simple() {
        use crate::provider::{ChatMessage, MessageRole};
        use rustycode_protocol::MessageContent;
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::simple("hello"),
        }];
        let converted = convert_messages(&msgs);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content[0]["text"], "hello");
    }

    #[test]
    fn test_convert_messages_tool_result() {
        use crate::provider::{ChatMessage, MessageRole};
        use rustycode_protocol::{ContentBlock, MessageContent};
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result("t1", "result text")]),
        }];
        let c = convert_messages(&msgs);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].role, "user");
        let tr = &c[0].content[0]["toolResult"];
        assert_eq!(tr["toolUseId"], "t1");
        assert_eq!(tr["status"], "success");
    }

    #[test]
    fn test_convert_messages_tool_use() {
        use crate::provider::{ChatMessage, MessageRole};
        use rustycode_protocol::{ContentBlock, MessageContent};
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "t2".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({"command": "ls"}),
            }]),
        }];
        let c = convert_messages(&msgs);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].role, "assistant");
        let tu = &c[0].content[0]["toolUse"];
        assert_eq!(tu["name"], "bash");
        assert_eq!(tu["toolUseId"], "t2");
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![serde_json::json!({
            "name": "read_file", "description": "Read file",
            "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
        })];
        let c = convert_tools(&tools);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].tool_spec.name, "read_file");
    }

    #[test]
    fn test_convert_tool_choice() {
        assert_eq!(
            convert_tool_choice(&serde_json::json!("auto")),
            serde_json::json!({"auto": {}})
        );
        assert_eq!(
            convert_tool_choice(&serde_json::json!("required")),
            serde_json::json!({"any": {}})
        );
        assert_eq!(
            convert_tool_choice(&serde_json::json!("bash")),
            serde_json::json!({"tool": {"name": "bash"}})
        );
    }

    #[test]
    fn test_bedrock_request_serialization() {
        let req = BedrockRequest {
            messages: vec![BedrockConverseMessage {
                role: "user".to_string(),
                content: vec![serde_json::json!({ "text": "hello" })],
            }],
            system: Some(vec![BedrockSystemContent {
                text: "You are helpful.".to_string(),
            }]),
            inference_config: BedrockInferenceConfig {
                max_tokens: 1024,
                temperature: 0.5,
            },
            tool_config: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("inferenceConfig"));
        assert!(json.contains("maxTokens"));
        assert!(!json.contains("toolConfig"));
    }

    #[test]
    fn test_bedrock_request_with_tools() {
        let req = BedrockRequest {
            messages: vec![BedrockConverseMessage {
                role: "user".to_string(),
                content: vec![serde_json::json!({ "text": "hello" })],
            }],
            system: None,
            inference_config: BedrockInferenceConfig {
                max_tokens: 4096,
                temperature: 0.7,
            },
            tool_config: Some(BedrockToolConfig {
                tools: vec![BedrockTool {
                    tool_spec: BedrockToolSpec {
                        name: "bash".to_string(),
                        description: Some("Run command".to_string()),
                        input_schema: BedrockInputSchema {
                            json: serde_json::json!({"type": "object"}),
                        },
                    },
                }],
                tool_choice: Some(serde_json::json!({"auto": {}})),
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("toolConfig"));
        assert!(json.contains("toolSpec"));
        assert!(json.contains("bash"));
    }
}
