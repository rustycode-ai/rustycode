//! OpenAI LLM provider implementation.
//!
//! Supports both Chat Completions API and Responses API, with automatic
//! fallback from Responses to Chat Completions when the endpoint doesn't
//! support it.
//!
//! Uses the Route+Protocol architecture: `chat_route` delegates to
//! `OpenAIChatProtocol` and `responses_route` delegates to
//! `OpenAIResponsesProtocol`.

pub(crate) mod responses;
pub(crate) mod streaming;
pub(crate) mod types;

#[cfg(test)]
mod tests;

use crate::auth::AuthMethod;
use crate::model_cache::ModelCache;
use crate::provider::{
    ApiMode, ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, MessageRole,
    ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::route::Route;
use crate::schema::tool_schema::ToolSchema;
use rustycode_tools_api::{ToolMetadataProvider, ToolProfile, ToolRegistry, ToolSelector};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

// Re-export internal types for use within this module's methods and tests.
use types::{
    OpenAiContentPart, OpenAiFunction, OpenAiImageUrl, OpenAiMessage, OpenAiRequest, OpenAiToolCall,
};

/// OpenAI LLM provider (also supports OpenAI-compatible APIs)
pub struct OpenAiProvider {
    pub(crate) config: ProviderConfig,
    pub(crate) chat_route: Route,
    pub(crate) responses_route: Route,
    #[allow(dead_code)] // Kept for future use
    pub(crate) default_model: String,
    pub(crate) tool_registry: Arc<ToolRegistry>,
    pub(crate) tool_selector: ToolSelector,
    /// Last Responses API response ID for server-side conversation state.
    /// Shared with `OpenAIResponsesProtocol` via Arc for automatic injection.
    /// Also read directly by the WebSocket streaming path (feature-gated).
    #[allow(dead_code)] // Read by WS path (feature-gated) and shared with protocol
    pub(crate) last_response_id: Arc<std::sync::Mutex<Option<String>>>,
    /// Cached Responses API capability: None=unknown, Some(true)=supported, Some(false)=not supported.
    pub(crate) responses_api_supported: Arc<std::sync::Mutex<Option<bool>>>,
    model_cache: ModelCache,
}

impl OpenAiProvider {
    /// Internal implementation of complete without retry logic
    pub async fn complete_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(_) => None, // Already provided in request.tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
            }
        };

        // Execute via Route
        let response = self
            .chat_route
            .execute(&request, tools.as_deref())
            .await
            .map_err(|e| ProviderError::Network(format!("route execution failed: {}", e)))?;

        Ok(response)
    }

    pub fn new(config: ProviderConfig, default_model: String) -> Result<Self, ProviderError> {
        // Skip API key format validation when using a custom base URL --
        // compatible endpoints (z.ai, Azure, proxies) use different key formats.
        let has_custom_base_url = config
            .base_url
            .as_ref()
            .is_some_and(|url| !url.starts_with("https://api.openai.com"));

        if has_custom_base_url {
            Self::new_without_validation(config, default_model)
        } else {
            Self::metadata().validate_config(&config)?;
            Self::build(config, default_model)
        }
    }

    /// Create provider without config validation (for custom endpoints/proxies)
    pub fn new_without_validation(
        config: ProviderConfig,
        default_model: String,
    ) -> Result<Self, ProviderError> {
        // Skip validation - trust the provided config
        Self::build(config, default_model)
    }

    /// Shared constructor that builds routes and initializes all fields.
    fn build(config: ProviderConfig, default_model: String) -> Result<Self, ProviderError> {
        let tool_registry = Arc::new(ToolRegistry::new());
        let tool_selector = ToolSelector::new();

        let timeout_secs = config.timeout_seconds.unwrap_or(120);

        let base_endpoint = {
            let base = config
                .base_url
                .as_deref()
                .unwrap_or("https://api.openai.com/v1");
            base.trim_end_matches('/').to_string()
        };

        let api_key = config
            .api_key
            .clone()
            .unwrap_or_else(|| secrecy::SecretString::new("".into()));

        let extra_headers: Vec<(String, String)> = config
            .extra_headers
            .as_ref()
            .map(|h| h.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let auth = Box::new(crate::auth::BearerAuth::new(api_key));

        // Shared state for Responses API response ID tracking
        let last_response_id = Arc::new(std::sync::Mutex::new(None));
        let responses_api_supported = Arc::new(std::sync::Mutex::new(None));

        // Chat Completions route: POST {base}/chat/completions
        let chat_route = Route::builder()
            .endpoint(format!("{}/chat/completions", base_endpoint))
            .protocol(Box::new(crate::wire::openai_chat::OpenAIChatProtocol))
            .transport(Box::new(
                crate::transport::HttpTransport::new(timeout_secs)
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ))
            .auth(auth.clone_box())
            .extra_headers(extra_headers.clone())
            .name("openai-chat")
            .build();

        // Responses API route: POST {base}/responses
        // The protocol shares the last_response_id Arc so it automatically
        // injects previous_response_id and stores new response IDs.
        let responses_route = Route::builder()
            .endpoint(format!("{}/responses", base_endpoint))
            .protocol(Box::new(
                crate::wire::openai_responses::OpenAIResponsesProtocol {
                    last_response_id: last_response_id.clone(),
                },
            ))
            .transport(Box::new(
                crate::transport::HttpTransport::new(timeout_secs)
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ))
            .auth(auth)
            .extra_headers(extra_headers)
            .name("openai-responses")
            .build();

        Ok(Self {
            config,
            chat_route,
            responses_route,
            default_model,
            tool_registry,
            tool_selector,
            last_response_id,
            responses_api_supported,
            model_cache: ModelCache::new(),
        })
    }

    /// Detect the user's intent from their latest message and select appropriate tools
    fn select_tools_for_prompt(&self, messages: &[ChatMessage]) -> Option<Vec<ToolSchema>> {
        // Find the last user message to detect intent
        let user_prompt = messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role, MessageRole::User))
            .map(|msg| msg.content.as_text());

        if let Some(prompt) = user_prompt {
            // Detect profile from prompt
            let profile = ToolProfile::from_prompt(&prompt);

            // Update selector with detected profile
            let selector = self.tool_selector.clone().with_profile(profile);

            // Get ranked tools for this profile using tag-based discovery
            let tools = selector.select_tools(&self.tool_registry);

            // Format tools for OpenAI API
            Some(self.format_tools_for_openai(&tools))
        } else {
            // No user message found, return None (no tools)
            None
        }
    }

    /// Format tool definitions for OpenAI function calling API
    fn format_tools_for_openai(&self, tool_names: &[String]) -> Vec<ToolSchema> {
        tool_names
            .iter()
            .filter_map(|name| {
                self.tool_registry
                    .tool_info(name)
                    .map(|info| ToolSchema::from(&info))
            })
            .collect()
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            description: "GPT models with strong language understanding and generation".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your OpenAI API key from platform.openai.com".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("sk-...".to_string()),
                        default: None,
                        validation_pattern: Some("^sk-.*".to_string()),
                        validation_error: Some("API key must start with 'sk-' (e.g. sk-..., sk-proj-...)".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (for Azure or compatible services)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.openai.com/v1".to_string()),
                        default: Some("https://api.openai.com/v1".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "OPENAI_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are RustyCode, a coding assistant.\n\n{context}\n\n## GPT Guidance\n- Use outcome-first framing: state the goal, then the steps\n- Maximize parallel tool calls when outputs are independent\n- Adapt output depth to reasoning effort level".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Use explicit behavioral contracts for complex tasks.".to_string(),
                        "Adapt detail level to reasoning effort -- direct for low, thorough for high.".to_string(),
                    ],
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
                    model_id: "gpt-5.2".to_string(),
                    display_name: "GPT-5.2".to_string(),
                    description: "Latest flagship model with strongest reasoning and coding".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Coding".to_string(), "Function calling".to_string()],
                    cost_tier: 5,
                },
                ModelInfo {
                    model_id: "gpt-5.1".to_string(),
                    display_name: "GPT-5.1".to_string(),
                    description: "High-capability model balancing quality and cost".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["General tasks".to_string(), "Coding".to_string()],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "gpt-4.1".to_string(),
                    display_name: "GPT-4.1".to_string(),
                    description: "Improved GPT-4 with better instruction following".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["General tasks".to_string(), "Function calling".to_string()],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "gpt-4o".to_string(),
                    display_name: "GPT-4o".to_string(),
                    description: "Omni model with vision and tool capabilities".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["General tasks".to_string(), "Vision".to_string(), "Function calling".to_string()],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "o4-mini".to_string(),
                    display_name: "o4 Mini".to_string(),
                    description: "Fast reasoning model for complex problem-solving".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Math".to_string(), "Coding".to_string()],
                    cost_tier: 3,
                },
            ],
                    model_behavior_profiles: HashMap::new(),
        }
    }

    pub fn endpoint(&self) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1");
        base.trim_end_matches('/').to_string()
    }

    /// Check if a model uses OpenAI reasoning-model parameters.
    ///
    /// These models send `max_completion_tokens` (not `max_tokens`),
    /// support `reasoning_effort`, and suppress `temperature`.
    ///
    /// GLM models (z.ai) are NOT included — z.ai uses `max_tokens`,
    /// supports `temperature`, and doesn't support `reasoning_effort`.
    /// GLM thinking is handled separately via the `thinking` param.
    pub(crate) fn is_reasoning_model(model: &str) -> bool {
        // o-series: o1, o3, o4-mini, etc.
        if model.starts_with('o')
            && model[1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            return true;
        }
        // GPT-5.x models support reasoning
        if model.starts_with("gpt-5") {
            return true;
        }
        false
    }

    /// Convert protocol ChatMessages to OpenAI messages, handling structured content blocks.
    ///
    /// OpenAI message format:
    /// - System/user/assistant: `{role, content}` where content is a string or array of parts
    /// - Tool results: `{role: "tool", content: string, tool_call_id: string}`
    /// - Assistant with tool calls: `{role: "assistant", content, tool_calls: [...]}`
    #[allow(dead_code)] // Used by tests and wire protocol
    pub(crate) fn convert_messages(messages: &[ChatMessage]) -> Vec<OpenAiMessage> {
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
                        // Check if this contains tool results -- each needs its own message
                        let mut tool_results: Vec<OpenAiMessage> = Vec::new();
                        let mut other_parts: Vec<OpenAiContentPart> = Vec::new();
                        let mut tool_calls: Vec<OpenAiToolCall> = Vec::new();
                        let mut reasoning_content: Option<String> = None;

                        for block in blocks {
                            match block {
                                ContentBlock::Text { text, .. } => {
                                    other_parts
                                        .push(OpenAiContentPart::Text { text: text.clone() });
                                }
                                ContentBlock::Image { source, .. } => {
                                    let url = match source.source_type.as_str() {
                                        "url" => source.data.clone(),
                                        _ => {
                                            if let Some((mime, data)) =
                                                crate::provider::resolve_image_to_base64(source)
                                            {
                                                format!("data:{mime};base64,{data}")
                                            } else {
                                                tracing::warn!(
                                                    "Skipping image with unresolvable source"
                                                );
                                                continue;
                                            }
                                        }
                                    };
                                    other_parts.push(OpenAiContentPart::ImageUrl {
                                        image_url: OpenAiImageUrl { url, detail: None },
                                    });
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => {
                                    // Tool results must be separate messages with role="tool"
                                    // OpenAI has no is_error field -- prefix error content explicitly
                                    let display_content = if *is_error {
                                        format!("Error: {content}")
                                    } else {
                                        content.clone()
                                    };
                                    tool_results.push(OpenAiMessage {
                                        role: "tool".to_string(),
                                        content: Some(serde_json::Value::String(display_content)),
                                        tool_calls: None,
                                        tool_call_id: Some(tool_use_id.clone()),
                                        name: None,
                                        reasoning_content: None,
                                    });
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    // Tool calls in assistant messages
                                    tool_calls.push(OpenAiToolCall {
                                        id: id.clone(),
                                        r#type: "function".to_string(),
                                        function: OpenAiFunction {
                                            name: name.clone(),
                                            arguments: serde_json::to_string(input)
                                                .unwrap_or_else(|_| "{}".to_string()),
                                        },
                                    });
                                }
                                ContentBlock::Thinking { thinking, .. } if !thinking.is_empty() => {
                                    reasoning_content = Some(thinking.clone());
                                }
                                _ => {} // non-exhaustive
                            }
                        }

                        let mut result = Vec::new();

                        // Build the main message (with text/images and/or tool_calls)
                        if !other_parts.is_empty()
                            || !tool_calls.is_empty()
                            || reasoning_content.is_some()
                        {
                            let content_val = if other_parts.is_empty() {
                                None
                            } else if other_parts.len() == 1 {
                                // Single text part: send as plain string (more compatible)
                                match &other_parts[0] {
                                    OpenAiContentPart::Text { text } => {
                                        Some(serde_json::Value::String(text.clone()))
                                    }
                                    _ => serde_json::to_value(&other_parts).ok(),
                                }
                            } else {
                                serde_json::to_value(&other_parts).ok()
                            };

                            result.push(OpenAiMessage {
                                role: if tool_calls.is_empty() {
                                    role_str.to_string()
                                } else {
                                    // Tool calls must be in an assistant message
                                    "assistant".to_string()
                                },
                                content: content_val,
                                tool_calls: if tool_calls.is_empty() {
                                    None
                                } else {
                                    Some(tool_calls)
                                },
                                tool_call_id: None,
                                name: None,
                                reasoning_content,
                            });
                        }

                        // Append tool result messages
                        result.extend(tool_results);
                        result
                    }
                    _ => {
                        // Simple text content
                        vec![OpenAiMessage {
                            role: role_str.to_string(),
                            content: Some(serde_json::Value::String(msg.content.to_text())),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                            reasoning_content: None,
                        }]
                    }
                }
            })
            .collect()
    }

    /// Build the request body with proper parameter selection based on model type.
    ///
    /// - Reasoning models (o-series, GPT-5.x, GLM-5.x): use `max_completion_tokens` instead of
    ///   deprecated `max_tokens`, and include `reasoning_effort` if provided.
    /// - Standard models: use `max_tokens`.
    #[allow(dead_code)] // Used by tests and wire protocol
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_request_body(
        &self,
        model: String,
        messages: Vec<OpenAiMessage>,
        tools: Vec<serde_json::Value>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        effort: Option<&crate::provider::EffortLevel>,
        stream: Option<bool>,
        output_config: Option<&crate::provider::OutputConfig>,
        tool_choice: Option<serde_json::Value>,
        parallel_tool_calls: Option<bool>,
        session_id: Option<&String>,
        thinking_config: Option<&crate::provider::ThinkingConfig>,
    ) -> OpenAiRequest {
        let (max_tokens, max_completion_tokens) = if Self::is_reasoning_model(&model) {
            // Reasoning models require max_completion_tokens (max_tokens is not supported)
            (None, max_tokens)
        } else {
            // Standard models use max_tokens (max_completion_tokens also works but keep compat)
            (max_tokens, None)
        };

        // reasoning_effort valid for o-series, GPT-5.x, and GLM-5.x models
        let reasoning_effort = if Self::is_reasoning_model(&model) {
            effort.map(|e| match e {
                crate::provider::EffortLevel::Low => "low".to_string(),
                crate::provider::EffortLevel::Medium => "medium".to_string(),
                crate::provider::EffortLevel::High => "high".to_string(),
                crate::provider::EffortLevel::Xhigh => "xhigh".to_string(),
                crate::provider::EffortLevel::Max => "xhigh".to_string(),
            })
        } else {
            None
        };

        // temperature not supported by reasoning models
        let temperature = if Self::is_reasoning_model(&model) {
            None
        } else {
            temperature
        };

        let response_format = output_config.and_then(|cfg| {
            cfg.format.as_ref().map(|fmt| match fmt.format_type {
                crate::provider::OutputFormatType::JsonSchema => {
                    serde_json::json!({
                        "type": "json_schema",
                        "json_schema": fmt.json_schema.as_ref().unwrap_or(&serde_json::json!({}))
                    })
                }
            })
        });

        let thinking = match thinking_config {
            Some(cfg) => match cfg.thinking_type {
                crate::provider::ThinkingType::Disabled => None,
                crate::provider::ThinkingType::Enabled => {
                    let mut obj = serde_json::json!({"type": "enabled"});
                    if let Some(budget) = cfg.budget_tokens {
                        obj["budget_tokens"] = serde_json::json!(budget);
                    }
                    Some(obj)
                }
                crate::provider::ThinkingType::Adaptive => {
                    Some(serde_json::json!({"type": "enabled"}))
                }
            },
            None if model.starts_with("glm-5")
                || model.starts_with("glm-4.5")
                || model.starts_with("glm-4.6")
                || model.starts_with("glm-4.7") =>
            {
                // Default for GLM reasoning models: adaptive thinking (model decides)
                Some(serde_json::json!({"type": "enabled"}))
            }
            None => None,
        };

        let stream_options = if stream == Some(true) {
            Some(serde_json::json!({"include_usage": true}))
        } else {
            None
        };

        let has_tools = !tools.is_empty();

        OpenAiRequest {
            model,
            messages,
            temperature,
            max_tokens,
            max_completion_tokens,
            stream,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_stream: if stream == Some(true) && has_tools {
                Some(true)
            } else {
                None
            },
            tool_choice,
            parallel_tool_calls,
            reasoning_effort,
            response_format,
            thinking,
            stream_options,
            prompt_cache_key: session_id.cloned(),
        }
    }

    /// Stream using the Chat Completions API via Route.
    async fn complete_stream_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(_) => None, // Already provided in request.tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
            }
        };

        // Execute via Route
        let stream = self
            .chat_route
            .execute_stream(&request, tools.as_deref())
            .await
            .map_err(|e| ProviderError::Network(format!("route stream failed: {}", e)))?;

        // Map anyhow::Error to ProviderError
        let mapped_stream =
            stream.map(|res| res.map_err(|e| ProviderError::Network(e.to_string())));

        Ok(Box::pin(mapped_stream))
    }
}

#[async_trait]
impl LLMProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        const FALLBACK: &[&str] = &[
            "gpt-5.2",
            "gpt-5.1",
            "gpt-5-pro",
            "o4-mini",
            "o3",
            "o3-mini",
            "o1",
            "o1-mini",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4-turbo",
        ];
        let endpoint = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/');
        let url = format!("{endpoint}/models");
        let cache = &self.model_cache;
        let config = &self.config;
        cache
            .fetch_or_fallback(FALLBACK, || async {
                let client = reqwest::Client::new();
                let mut req = client.get(&url);
                if let Some(key) = &config.api_key {
                    req = req.bearer_auth(key.expose_secret());
                }
                let resp = req
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|e| ProviderError::Network(e.to_string()))?;
                let body: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| ProviderError::Api(e.to_string()))?;
                let models = body
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                m.get("id").and_then(|id| id.as_str()).map(String::from)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Ok(models)
            })
            .await
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        match request.api_mode {
            Some(ApiMode::Responses) => return self.complete_responses(request).await,
            Some(ApiMode::Auto) => {
                let cached = {
                    let guard = self.responses_api_supported.lock().unwrap_or_else(|e| {
                        tracing::warn!("responses_api_supported mutex poisoned, recovering: {}", e);
                        e.into_inner()
                    });
                    *guard
                };
                if cached != Some(false) {
                    match self.complete_responses(request.clone()).await {
                        Ok(resp) => {
                            let mut guard =
                                self.responses_api_supported.lock().unwrap_or_else(|e| {
                                    tracing::warn!(
                                        "responses_api_supported mutex poisoned, recovering: {}",
                                        e
                                    );
                                    e.into_inner()
                                });
                            *guard = Some(true);
                            return Ok(resp);
                        }
                        Err(ref e) if Self::is_responses_unsupported_error(e) => {
                            tracing::info!(
                                "Responses API unavailable, falling back to Chat Completions"
                            );
                            let mut guard =
                                self.responses_api_supported.lock().unwrap_or_else(|e| {
                                    tracing::warn!(
                                        "responses_api_supported mutex poisoned, recovering: {}",
                                        e
                                    );
                                    e.into_inner()
                                });
                            *guard = Some(false);
                            // Fall through to Chat Completions below
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
            _ => {}
        }

        let retry_config = self.config.retry_config.clone().unwrap_or_default();

        crate::retry::retry_with_backoff(retry_config, || {
            let request = request.clone();
            async move {
                self.complete_internal(request)
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

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        #[cfg(feature = "ws")]
        if request.api_mode == Some(ApiMode::ResponsesWs) {
            return self.complete_responses_ws(request).await;
        }

        match request.api_mode {
            Some(ApiMode::Responses) => return self.complete_responses_stream(request).await,
            Some(ApiMode::Auto) => {
                let cached = {
                    let guard = self.responses_api_supported.lock().unwrap_or_else(|e| {
                        tracing::warn!("responses_api_supported mutex poisoned, recovering: {}", e);
                        e.into_inner()
                    });
                    *guard
                };
                if cached != Some(false) {
                    match self.complete_responses_stream(request.clone()).await {
                        Ok(stream) => {
                            let mut guard =
                                self.responses_api_supported.lock().unwrap_or_else(|e| {
                                    tracing::warn!(
                                        "responses_api_supported mutex poisoned, recovering: {}",
                                        e
                                    );
                                    e.into_inner()
                                });
                            *guard = Some(true);
                            return Ok(stream);
                        }
                        Err(ref e) if Self::is_responses_unsupported_error(e) => {
                            tracing::info!(
                                "Responses API streaming unavailable, falling back to Chat \
                                 Completions"
                            );
                            let mut guard =
                                self.responses_api_supported.lock().unwrap_or_else(|e| {
                                    tracing::warn!(
                                        "responses_api_supported mutex poisoned, recovering: {}",
                                        e
                                    );
                                    e.into_inner()
                                });
                            *guard = Some(false);
                            // Fall through to Chat Completions below
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
            _ => {}
        }

        let retry_config = self.config.retry_config.clone().unwrap_or_default();

        crate::retry::retry_with_backoff(retry_config, || {
            let request = request.clone();
            async move {
                self.complete_stream_internal(request)
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
}
