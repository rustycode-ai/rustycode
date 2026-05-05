//! OpenAI LLM provider implementation.
use crate::provider::{
    ApiMode, ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, MessageRole,
    ProviderConfig, ProviderError, StreamChunk, ThinkingBlock, Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use rustycode_tools_api::{Tool, ToolProfile, ToolRegistry, ToolSelector};

// Import unified trait from protocol
use rustycode_protocol::llm::{
    CompletionRequest as UnifiedCompletionRequest, CompletionResponse as UnifiedCompletionResponse,
    Cost as UnifiedCost, LLMProvider as UnifiedLLMProvider, ModelInfo as UnifiedModelInfo,
    TokenCount as UnifiedTokenCount,
};
use rustycode_protocol::stream_event::StreamEvent;

// Import macros exported at crate root
use crate::retry::extract_retry_after_ms;
use crate::{build_request, get_api_key, shared_client};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    /// Controls tool calling behavior: "none", "auto", "required", or specific tool
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    /// Whether to enable parallel tool calling (default true)
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<serde_json::Value>,
    /// Request usage stats in final streaming chunk
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<serde_json::Value>,
    /// Cache routing key — groups related requests for higher prompt cache hit rates.
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_cache_key: Option<String>,
}

/// Content part for structured OpenAI messages (text, image, etc.)
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum OpenAiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAiImageUrl },
}

#[derive(Serialize, Deserialize, Clone)]
struct OpenAiImageUrl {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct OpenAiMessage {
    pub(crate) role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<serde_json::Value>,
    /// For assistant messages with tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<OpenAiToolCall>>,
    /// For tool result messages: references the tool call ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
    /// Name of the tool (for legacy function calling)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
    model: String,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
    finish_reason: Option<String>,
}

/// Full response message that can include tool calls
#[derive(Deserialize)]
struct OpenAiResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

/// Tool call from OpenAI API response
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct OpenAiToolCall {
    pub(crate) id: String,
    pub(crate) r#type: String,
    pub(crate) function: OpenAiFunction,
}

/// Function call within a tool call
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct OpenAiFunction {
    pub(crate) name: String,
    pub(crate) arguments: String,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: Option<OpenAiPromptTokensDetails>,
}

#[derive(Deserialize)]
struct OpenAiPromptTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

/// OpenAI LLM provider (also supports OpenAI-compatible APIs)
pub struct OpenAiProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    default_model: String,
    tool_registry: Arc<ToolRegistry>,
    tool_selector: ToolSelector,
    /// Last Responses API response ID for server-side conversation state.
    last_response_id: Arc<std::sync::Mutex<Option<String>>>,
    /// Cached Responses API capability: None=unknown, Some(true)=supported, Some(false)=not supported.
    responses_api_supported: Arc<std::sync::Mutex<Option<bool>>>,
}

impl OpenAiProvider {
    /// Internal implementation of complete without retry logic
    pub async fn complete_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = get_api_key!(self, "OPENAI_API_KEY")?;

        let url = format!("{}/chat/completions", self.endpoint());

        // Build messages array
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenAiMessage {
                role: "system".to_string(),
                content: Some(serde_json::Value::String(system_prompt.clone())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        messages.extend(Self::convert_messages(&request.messages));

        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(tools) => {
                let normalized = crate::tools::normalize_tools_for_openai(&tools);
                crate::tools::sanitize_tools_for_strict_providers(&normalized)
            }
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
                    .unwrap_or_default()
            }
        };

        let body = self.build_request_body(
            request.model.clone(),
            messages,
            tools,
            request.max_tokens,
            request.temperature,
            request
                .output_config
                .as_ref()
                .and_then(|c| c.effort.as_ref()),
            Some(false),
            request.output_config.as_ref(),
            request.tool_choice.clone(),
            request.parallel_tool_calls,
            request.session_id.as_ref(),
            request.thinking.as_ref(),
        );

        // HTTP trace dump for debugging (before send so we capture body even if send hangs)
        let http_trace_dir = std::env::var("RTK_HTTP_TRACE_DIR")
            .unwrap_or_else(|_| "/tmp/rtk-http-trace".to_string());
        let _ = std::fs::create_dir_all(&http_trace_dir);
        let trace_seq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        if let Ok(req_body) = serde_json::to_string_pretty(&body) {
            let trace_path = format!("{http_trace_dir}/{trace_seq}_req.json");
            let _ = std::fs::write(&trace_path, &req_body);
            tracing::info!(
                "[openai-trace] Request dumped to {trace_path} ({} bytes)",
                req_body.len()
            );
        }

        // Build request with per-request headers
        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

        tracing::info!("[openai] Sending request to {url}...");
        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;
        tracing::info!("[openai] Response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 | 403 => ProviderError::auth(format!(
                    "Authentication failed. Check your OPENAI_API_KEY env var. {}",
                    text
                )),
                404 => ProviderError::InvalidModel(text.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "OpenAI service temporarily unavailable ({}). Please retry in a few seconds.",
                    text
                )),
                _ => ProviderError::api(format!("{}: {}", status, text)),
            });
        }

        let resp_text = response
            .text()
            .await
            .map_err(|e| ProviderError::network(format!("failed to read response body: {}", e)))?;
        let resp_trace_path = format!("{http_trace_dir}/{trace_seq}_resp.json");
        if std::fs::write(&resp_trace_path, &resp_text).is_ok() {
            tracing::info!("[openai-trace] Response dumped to {resp_trace_path}");
        }
        let resp: OpenAiResponse = serde_json::from_str(&resp_text).map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::api("no choices in response"))?;

        // Build content string, appending tool calls if present
        let mut content = choice.message.content.unwrap_or_default();

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

        let usage = resp.usage.map(|u| {
            let cached = u
                .prompt_tokens_details
                .as_ref()
                .map_or(0, |d| d.cached_tokens);
            if cached > 0 {
                let hit_pct = (cached * 100).checked_div(u.prompt_tokens).unwrap_or(0);
                tracing::info!(
                    "Cache: {hit_pct}% hit ({cached}/{} prompt tokens), total={}",
                    u.prompt_tokens,
                    u.total_tokens
                );
            }
            Usage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                cache_read_input_tokens: cached,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }
        });

        Ok(CompletionResponse {
            content,
            model: resp.model,
            usage,
            stop_reason: crate::provider::normalize_stop_reason(choice.finish_reason.as_deref()),
            citations: None,
            thinking_blocks: choice.message.reasoning_content.map(|rc| {
                vec![ThinkingBlock {
                    block_type: "thinking".to_string(),
                    thinking: rc,
                    signature: String::new(),
                    data: String::new(),
                    display: None,
                }]
            }),
            structured_output: None,
        })
    }

    pub fn new(config: ProviderConfig, default_model: String) -> Result<Self, ProviderError> {
        // Skip API key format validation when using a custom base URL —
        // compatible endpoints (z.ai, Azure, proxies) use different key formats.
        let has_custom_base_url = config
            .base_url
            .as_ref()
            .is_some_and(|url| !url.starts_with("https://api.openai.com"));

        if has_custom_base_url {
            Self::new_without_validation(config, default_model)
        } else {
            Self::metadata().validate_config(&config)?;
            let client = if let Some(timeout_secs) = config.timeout_seconds {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(timeout_secs))
                    .connect_timeout(Duration::from_secs(10))
                    .pool_idle_timeout(Duration::from_secs(90))
                    .tcp_keepalive(Duration::from_mins(1))
                    .build()
                    .map_err(|e| {
                        ProviderError::Configuration(format!("Failed to create HTTP client: {}", e))
                    })?
            } else {
                shared_client!()
            };
            let tool_registry = Arc::new(rustycode_tools::default_registry());
            let tool_selector = ToolSelector::new();
            Ok(Self {
                config,
                client,
                default_model,
                tool_registry,
                tool_selector,
                last_response_id: Arc::new(std::sync::Mutex::new(None)),
                responses_api_supported: Arc::new(std::sync::Mutex::new(None)),
            })
        }
    }

    /// Create provider without config validation (for custom endpoints/proxies)
    pub fn new_without_validation(
        config: ProviderConfig,
        default_model: String,
    ) -> Result<Self, ProviderError> {
        // Skip validation - trust the provided config
        let client = if let Some(timeout_secs) = config.timeout_seconds {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .connect_timeout(Duration::from_secs(10))
                .pool_idle_timeout(Duration::from_secs(90))
                .tcp_keepalive(Duration::from_mins(1))
                .build()
                .map_err(|e| {
                    ProviderError::Configuration(format!("Failed to create HTTP client: {}", e))
                })?
        } else {
            shared_client!()
        };

        // Initialize tool registry and selector
        let tool_registry = Arc::new(rustycode_tools::default_registry());
        let tool_selector = ToolSelector::new();

        Ok(Self {
            config,
            client,
            default_model,
            tool_registry,
            tool_selector,
            last_response_id: Arc::new(std::sync::Mutex::new(None)),
            responses_api_supported: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Detect the user's intent from their latest message and select appropriate tools
    fn select_tools_for_prompt(&self, messages: &[ChatMessage]) -> Option<Vec<serde_json::Value>> {
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
    fn format_tools_for_openai(&self, tool_names: &[String]) -> Vec<serde_json::Value> {
        tool_names
            .iter()
            .filter_map(|name| {
                self.tool_registry
                    .get(name)
                    .map(|tool| self.tool_to_openai_format(tool))
            })
            .collect()
    }

    /// Convert a tool to OpenAI's function format
    fn tool_to_openai_format(&self, tool: &dyn Tool) -> serde_json::Value {
        let schema = tool.parameters_schema();
        serde_json::json!({
            "type": "function",
            "function": {
                "name": tool.name(),
                "description": tool.description(),
                "parameters": schema
            }
        })
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
                        "Adapt detail level to reasoning effort — direct for low, thorough for high.".to_string(),
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
        }
    }

    /// Check if an error from the Responses API indicates the endpoint is unavailable,
    /// signalling that Auto mode should fall back to Chat Completions.
    fn is_responses_unsupported_error(err: &ProviderError) -> bool {
        matches!(err, ProviderError::InvalidModel(_))
            || matches!(err, ProviderError::Api(msg) if msg.starts_with("404"))
    }

    pub fn endpoint(&self) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1");
        base.trim_end_matches('/').to_string()
    }

    /// Complete a request using the Responses API (`POST /v1/responses`).
    async fn complete_responses(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = get_api_key!(self, "OPENAI_API_KEY")?;

        let url = format!("{}/responses", self.endpoint());

        let (instructions, input_items) =
            crate::openai_compatible::convert_messages_to_responses_input(&request);

        let tools = request
            .tools
            .as_ref()
            .map(|t| crate::tools::normalize_tools_for_responses(t))
            .unwrap_or_default();
        let tools_opt = if tools.is_empty() {
            None
        } else {
            let parsed: Vec<crate::openai_compatible::ResponsesApiTool> = tools
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        };

        let prev_id = self
            .last_response_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let body = crate::openai_compatible::ResponsesApiRequest {
            model: request.model.clone(),
            input: input_items,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            max_output_tokens: request.max_tokens,
            stream: Some(false),
            previous_response_id: prev_id,
            tool_choice: request.tool_choice,
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning: None,
            include: None,
        };

        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::auth(format!(
                    "Authentication failed. Check your OPENAI_API_KEY env var. {}",
                    text
                )),
                404 => ProviderError::InvalidModel(format!("model not found: {}", text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                500..=599 => {
                    ProviderError::network(format!("OpenAI service error ({}): {}", status, text))
                }
                _ => ProviderError::api(format!("{}: {}", status, text)),
            });
        }

        let resp: crate::openai_compatible::ResponsesApiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::api(format!("failed to parse response: {}", e)))?;

        // Store response ID for server-side conversation state
        if let Ok(mut guard) = self.last_response_id.lock() {
            *guard = Some(resp.id.clone());
        }

        crate::openai_compatible::build_responses_completion_response(&resp)
    }

    /// Stream a request using the Responses API (`POST /v1/responses` with `stream: true`).
    async fn complete_responses_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                ProviderError::auth(
                    "OpenAI API key is required. Set api_key in config or OPENAI_API_KEY env var",
                )
            })?
            .expose_secret();

        let url = format!("{}/responses", self.endpoint());

        let (instructions, input_items) =
            crate::openai_compatible::convert_messages_to_responses_input(&request);

        let tools = request
            .tools
            .as_ref()
            .map(|t| crate::tools::normalize_tools_for_responses(t))
            .unwrap_or_default();
        let tools_opt = if tools.is_empty() {
            None
        } else {
            let parsed: Vec<crate::openai_compatible::ResponsesApiTool> = tools
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        };

        let prev_id = self
            .last_response_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let body = crate::openai_compatible::ResponsesApiRequest {
            model: request.model.clone(),
            input: input_items,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            max_output_tokens: request.max_tokens,
            stream: Some(true),
            previous_response_id: prev_id,
            tool_choice: request.tool_choice,
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning: None,
            include: None,
        };

        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::auth(format!(
                    "Authentication failed. Check your OPENAI_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited { retry_delay: None },
                500..=599 => ProviderError::network(format!(
                    "OpenAI service error ({}): {}",
                    status, error_text
                )),
                _ => ProviderError::api(format!("{}: {}", status, error_text)),
            });
        }

        let bytes_stream = response.bytes_stream();
        let line_buffer = crate::sse::SseByteBuffer::new();
        let state = crate::openai_compatible::ResponsesSseState::default();

        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let state = state.clone();

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(e.to_string()))]);
                }
            };

            let lines = line_buffer.feed_chunk(&chunk);
            let joined = lines.join("\n");
            let events = crate::openai_compatible::parse_responses_sse_lines(&joined, &state);

            futures::stream::iter(events)
        });

        Ok(Box::pin(sse_stream))
    }

    /// Stream a Responses API request over WebSocket (feature-gated).
    ///
    /// Converts the HTTP endpoint to a WebSocket URL and uses the WS transport.
    #[cfg(feature = "ws")]
    async fn complete_responses_ws(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        use secrecy::ExposeSecret;

        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                ProviderError::auth(
                    "OpenAI API key is required. Set api_key in config or OPENAI_API_KEY env var",
                )
            })?
            .expose_secret();

        // Convert HTTP endpoint to WebSocket URL
        let endpoint = self.endpoint();
        let base = endpoint.trim_end_matches('/');
        let ws_url = base
            .replace("https://", "wss://")
            .replace("http://", "ws://")
            + "/responses";

        let (instructions, input_items) =
            crate::openai_compatible::convert_messages_to_responses_input(&request);

        let tools = request
            .tools
            .as_ref()
            .map(|t| crate::tools::normalize_tools_for_responses(t))
            .unwrap_or_default();
        let tools_opt = if tools.is_empty() {
            None
        } else {
            let parsed: Vec<crate::openai_compatible::ResponsesApiTool> = tools
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        };

        let prev_id = self
            .last_response_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        let body = crate::openai_compatible::ResponsesApiRequest {
            model: request.model.clone(),
            input: input_items,
            instructions,
            tools: tools_opt,
            temperature: request.temperature,
            max_output_tokens: request.max_tokens,
            stream: Some(true),
            previous_response_id: prev_id,
            tool_choice: request.tool_choice,
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning: None,
            include: None,
        };

        let body_json =
            serde_json::to_value(&body).map_err(|e| ProviderError::Serialization(e.to_string()))?;

        crate::openai_compatible::responses_ws::stream_responses_ws(&ws_url, api_key, body_json)
            .await
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
        // Latest OpenAI models as of 2026
        Ok(vec![
            // GPT-5 series (latest)
            "gpt-5.2".to_string(),
            "gpt-5.1".to_string(),
            "gpt-5-pro".to_string(),
            // o-series (reasoning models)
            "o4-mini".to_string(),
            "o3".to_string(),
            "o3-mini".to_string(),
            "o1".to_string(),
            "o1-mini".to_string(),
            // GPT-4.1 series
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4.1-nano".to_string(),
            // GPT-4o series (omni models)
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            // Legacy models
            "gpt-4-turbo".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        match request.api_mode {
            Some(ApiMode::Responses) => return self.complete_responses(request).await,
            Some(ApiMode::Auto) => {
                let cached = self.responses_api_supported.lock().ok().and_then(|g| *g);
                if cached != Some(false) {
                    match self.complete_responses(request.clone()).await {
                        Ok(resp) => {
                            if let Ok(mut g) = self.responses_api_supported.lock() {
                                *g = Some(true);
                            }
                            return Ok(resp);
                        }
                        Err(ref e) if Self::is_responses_unsupported_error(e) => {
                            tracing::info!(
                                "Responses API unavailable, falling back to Chat Completions"
                            );
                            if let Ok(mut g) = self.responses_api_supported.lock() {
                                *g = Some(false);
                            }
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
                let cached = self.responses_api_supported.lock().ok().and_then(|g| *g);
                if cached != Some(false) {
                    match self.complete_responses_stream(request.clone()).await {
                        Ok(stream) => {
                            if let Ok(mut g) = self.responses_api_supported.lock() {
                                *g = Some(true);
                            }
                            return Ok(stream);
                        }
                        Err(ref e) if Self::is_responses_unsupported_error(e) => {
                            tracing::info!(
                                "Responses API streaming unavailable, falling back to Chat \
                                 Completions"
                            );
                            if let Ok(mut g) = self.responses_api_supported.lock() {
                                *g = Some(false);
                            }
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

impl OpenAiProvider {
    /// Check if a model is a reasoning model (o-series, GPT-5.x, or GLM-5.x).
    fn is_reasoning_model(model: &str) -> bool {
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
        // GLM-5.x reasoning models (z.ai)
        model.starts_with("glm-5")
    }

    /// Convert protocol ChatMessages to OpenAI messages, handling structured content blocks.
    ///
    /// OpenAI message format:
    /// - System/user/assistant: `{role, content}` where content is a string or array of parts
    /// - Tool results: `{role: "tool", content: string, tool_call_id: string}`
    /// - Assistant with tool calls: `{role: "assistant", content, tool_calls: [...]}`
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
                        // Check if this contains tool results — each needs its own message
                        let mut tool_results: Vec<OpenAiMessage> = Vec::new();
                        let mut other_parts: Vec<OpenAiContentPart> = Vec::new();
                        let mut tool_calls: Vec<OpenAiToolCall> = Vec::new();

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
                                    // OpenAI has no is_error field — prefix error content explicitly
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
                                ContentBlock::Thinking { thinking, .. } => {
                                    // OpenAI doesn't support thinking blocks natively.
                                    if !thinking.is_empty() {
                                        other_parts.push(OpenAiContentPart::Text {
                                            text: format!(
                                                "[prior-reasoning]\n{}\n[/prior-reasoning]",
                                                thinking
                                            ),
                                        });
                                    }
                                }
                                _ => {} // non-exhaustive
                            }
                        }

                        let mut result = Vec::new();

                        // Build the main message (with text/images and/or tool_calls)
                        if !other_parts.is_empty() || !tool_calls.is_empty() {
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
    #[allow(clippy::too_many_arguments)]
    fn build_request_body(
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
            None if model.starts_with("glm-5") => {
                // Default for GLM-5.x: adaptive thinking (model decides)
                Some(serde_json::json!({"type": "enabled"}))
            }
            None => None,
        };

        let stream_options = if stream == Some(true) {
            Some(serde_json::json!({"include_usage": true}))
        } else {
            None
        };

        OpenAiRequest {
            model,
            messages,
            temperature,
            max_tokens,
            max_completion_tokens,
            stream,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_choice,
            parallel_tool_calls,
            reasoning_effort,
            response_format,
            thinking,
            stream_options,
            prompt_cache_key: session_id.cloned(),
        }
    }

    /// Parse SSE-formatted lines into SSEEvent results.
    ///
    /// This is the core parsing logic extracted from `complete_stream_internal`
    /// so it can be tested independently without network calls.
    #[allow(dead_code)]
    fn parse_sse_lines(lines: &str) -> Vec<Result<crate::provider::SSEEvent, ProviderError>> {
        let mut events = Vec::new();
        let mut seen_tool_indices: HashSet<usize> = HashSet::new();
        for line in lines.lines() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            if line.starts_with("data: ") {
                let json_str = line.trim_start_matches("data: ").trim();
                if json_str == "[DONE]" {
                    events.push(Ok(crate::provider::SSEEvent::MessageStop));
                    continue;
                }
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            // Handle content delta (text streaming)
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content") {
                                    if let Some(content_str) = content.as_str() {
                                        if !content_str.is_empty() {
                                            events.push(Ok(
                                                crate::provider::SSEEvent::ContentBlockDelta {
                                                    index: 0,
                                                    delta: crate::provider::ContentDelta::Text {
                                                        text: content_str.to_string(),
                                                    },
                                                },
                                            ));
                                        }
                                    }
                                }

                                // Handle reasoning content (GLM-5 via Z.AI official API)
                                if let Some(reasoning) = delta.get("reasoning_content") {
                                    if let Some(reasoning_str) = reasoning.as_str() {
                                        if !reasoning_str.is_empty() {
                                            events.push(Ok(
                                                crate::provider::SSEEvent::ThinkingDelta {
                                                    thinking: reasoning_str.to_string(),
                                                },
                                            ));
                                        }
                                    }
                                }

                                // Handle reasoning content (GLM-5 via vLLM uses "reasoning" key)
                                if let Some(reasoning) = delta.get("reasoning") {
                                    if let Some(reasoning_str) = reasoning.as_str() {
                                        if !reasoning_str.is_empty() {
                                            events.push(Ok(
                                                crate::provider::SSEEvent::ThinkingDelta {
                                                    thinking: reasoning_str.to_string(),
                                                },
                                            ));
                                        }
                                    }
                                }

                                // Handle tool call deltas
                                if let Some(tool_calls) =
                                    delta.get("tool_calls").and_then(|tc| tc.as_array())
                                {
                                    for tc_delta in tool_calls {
                                        let index = tc_delta
                                            .get("index")
                                            .and_then(|i| i.as_u64())
                                            .unwrap_or(0)
                                            as usize;

                                        // Check for tool call start (has id and function.name)
                                        if let Some(id) =
                                            tc_delta.get("id").and_then(|i| i.as_str())
                                        {
                                            let name = tc_delta
                                                .get("function")
                                                .and_then(|f| f.get("name"))
                                                .and_then(|n| n.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            seen_tool_indices.insert(index);
                                            events.push(Ok(
                                                crate::provider::SSEEvent::ContentBlockStart {
                                                    index,
                                                    content_block:
                                                        crate::provider::ContentBlockType::ToolUse {
                                                            id: id.to_string(),
                                                            name,
                                                            input: None,
                                                        },
                                                },
                                            ));
                                        }

                                        // Check for partial function arguments
                                        if let Some(partial) = tc_delta
                                            .get("function")
                                            .and_then(|f| f.get("arguments"))
                                            .and_then(|a| a.as_str())
                                        {
                                            if !partial.is_empty() {
                                                events.push(Ok(
                                                    crate::provider::SSEEvent::ContentBlockDelta {
                                                        index,
                                                        delta:
                                                            crate::provider::ContentDelta::PartialJson {
                                                                partial_json: partial.to_string(),
                                                            },
                                                    },
                                                ));
                                            }
                                        }
                                    }
                                }
                            }

                            // Handle finish_reason
                            if let Some(finish_reason) =
                                choice.get("finish_reason").and_then(|f| f.as_str())
                            {
                                if finish_reason == "tool_calls" || finish_reason == "stop" {
                                    for &idx in &seen_tool_indices {
                                        events.push(Ok(
                                            crate::provider::SSEEvent::ContentBlockStop {
                                                index: idx,
                                            },
                                        ));
                                    }
                                    if seen_tool_indices.is_empty() {
                                        events.push(Ok(
                                            crate::provider::SSEEvent::ContentBlockStop {
                                                index: 0,
                                            },
                                        ));
                                    }
                                }

                                let usage = data.get("usage").and_then(|u| {
                                    let input_tokens = u.get("prompt_tokens")?.as_u64()? as u32;
                                    let output_tokens =
                                        u.get("completion_tokens")?.as_u64()? as u32;
                                    let cached_tokens: u32 = u
                                        .get("prompt_tokens_details")
                                        .and_then(|d| d.get("cached_tokens"))
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0) as u32;
                                    let reasoning_tokens: u32 = u
                                        .get("completion_tokens_details")
                                        .and_then(|d| d.get("reasoning_tokens"))
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0) as u32;
                                    if cached_tokens > 0 {
                                        let hit_pct = (cached_tokens * 100).checked_div(input_tokens).unwrap_or(0);
                                        tracing::info!(
                                            "Cache: {hit_pct}% hit ({cached_tokens}/{input_tokens} prompt tokens)"
                                        );
                                    }
                                    tracing::info!(
                                        input_tokens,
                                        output_tokens,
                                        reasoning_tokens,
                                        total = input_tokens.saturating_add(output_tokens).saturating_add(reasoning_tokens),
                                        "Usage breakdown (streaming)"
                                    );
                                    Some(Usage {
                                        input_tokens,
                                        output_tokens,
                                        total_tokens: input_tokens.saturating_add(output_tokens).saturating_add(reasoning_tokens),
                                        cache_read_input_tokens: cached_tokens,
                                        cache_creation_input_tokens: 0,
                                        reasoning_tokens: if reasoning_tokens > 0 { Some(reasoning_tokens) } else { None },
                                    })
                                });

                                events.push(Ok(crate::provider::SSEEvent::MessageDelta {
                                    stop_reason: crate::provider::normalize_stop_reason(Some(
                                        finish_reason,
                                    )),
                                    usage,
                                }));
                            }
                        }
                    } else if let Some(error) = data.get("error") {
                        let code = error
                            .get("code")
                            .and_then(|c| c.as_str())
                            .unwrap_or("unknown");
                        let message = error
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Unknown streaming error");
                        events.push(Err(ProviderError::api(format!("{}: {}", code, message))));
                    }
                }
            } else if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(error) = data.get("error") {
                    let code = error
                        .get("code")
                        .and_then(|c| c.as_str())
                        .unwrap_or("unknown");
                    let message = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown streaming error");
                    events.push(Err(ProviderError::api(format!("{}: {}", code, message))));
                }
            }
        }

        if events.is_empty() {
            events.push(Ok(crate::provider::SSEEvent::text(String::new())));
        }

        events
    }

    /// Parse SSE-formatted lines into StreamEvent results for the runtime stream path.
    fn parse_sse_lines_stream_events(lines: &str) -> Vec<Result<StreamEvent, ProviderError>> {
        let mut events = Vec::new();
        let mut tool_ids_by_index: HashMap<usize, String> = HashMap::new();

        for line in lines.lines() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            if line.starts_with("data: ") {
                let json_str = line.trim_start_matches("data: ").trim();
                if json_str == "[DONE]" {
                    events.push(Ok(StreamEvent::Done));
                    continue;
                }
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                    Self::dispatch_data_payload(&data, &mut events, &mut tool_ids_by_index);
                }
            } else if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(err) = Self::extract_stream_error(&data) {
                    events.push(err);
                }
            }
        }

        if events.is_empty() {
            events.push(Ok(StreamEvent::Done));
        }
        events
    }

    /// Route a parsed JSON data payload: choices → delta/finish, or top-level error.
    fn dispatch_data_payload(
        data: &serde_json::Value,
        events: &mut Vec<Result<StreamEvent, ProviderError>>,
        tool_ids_by_index: &mut HashMap<usize, String>,
    ) {
        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
            if let Some(choice) = choices.first() {
                if let Some(delta) = choice.get("delta") {
                    Self::extract_delta_events(delta, events, tool_ids_by_index);
                }
                if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    Self::extract_finish_events(data, finish_reason, events);
                }
            }
        } else if let Some(err) = Self::extract_stream_error(data) {
            events.push(err);
        }
    }

    /// Extract TextDelta, ThinkingDelta, ToolCallStarted, and ToolInputDelta from a delta object.
    fn extract_delta_events(
        delta: &serde_json::Value,
        events: &mut Vec<Result<StreamEvent, ProviderError>>,
        tool_ids_by_index: &mut HashMap<usize, String>,
    ) {
        // Text content
        if let Some(content_str) = delta
            .get("content")
            .and_then(|c| c.as_str())
            .filter(|s| !s.is_empty())
        {
            events.push(Ok(StreamEvent::TextDelta {
                content: content_str.to_string(),
            }));
        }

        // Reasoning content (two key variants used by different providers)
        for key in ["reasoning_content", "reasoning"] {
            if let Some(reasoning_str) = delta
                .get(key)
                .and_then(|r| r.as_str())
                .filter(|s| !s.is_empty())
            {
                events.push(Ok(StreamEvent::ThinkingDelta {
                    content: reasoning_str.to_string(),
                }));
            }
        }

        // Tool call deltas
        if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
            for tc_delta in tool_calls {
                Self::extract_tool_call_delta(tc_delta, events, tool_ids_by_index);
            }
        }
    }

    /// Handle a single tool call delta entry within the tool_calls array.
    fn extract_tool_call_delta(
        tc_delta: &serde_json::Value,
        events: &mut Vec<Result<StreamEvent, ProviderError>>,
        tool_ids_by_index: &mut HashMap<usize, String>,
    ) {
        let index = tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

        let resolved_id = tc_delta
            .get("id")
            .and_then(|i| i.as_str())
            .map(ToString::to_string)
            .or_else(|| tool_ids_by_index.get(&index).cloned());

        // Tool call start (id present)
        if let Some(id) = tc_delta.get("id").and_then(|i| i.as_str()) {
            tool_ids_by_index.insert(index, id.to_string());
            let name = tc_delta
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            events.push(Ok(StreamEvent::ToolCallStarted {
                id: id.to_string(),
                name,
            }));
        }

        // Partial function arguments
        if let Some(partial) = tc_delta
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(|a| a.as_str())
            .filter(|s| !s.is_empty())
        {
            if let Some(id) = resolved_id {
                events.push(Ok(StreamEvent::ToolInputDelta {
                    id,
                    chunk: partial.to_string(),
                }));
            }
        }
    }

    /// Extract TokenUsage and TurnCompleted events when a finish_reason is present.
    fn extract_finish_events(
        data: &serde_json::Value,
        finish_reason: &str,
        events: &mut Vec<Result<StreamEvent, ProviderError>>,
    ) {
        if let Some(usage) = data.get("usage").and_then(Self::parse_usage) {
            events.push(Ok(StreamEvent::TokenUsage {
                input_tokens: u64::from(usage.input_tokens),
                output_tokens: u64::from(usage.output_tokens),
            }));
        }

        let stop_reason = crate::provider::normalize_stop_reason(Some(finish_reason))
            .unwrap_or_else(|| finish_reason.to_string());
        events.push(Ok(StreamEvent::TurnCompleted { stop_reason }));
    }

    /// Parse a usage JSON object into a [`Usage`] struct, logging cache hit percentage.
    fn parse_usage(u: &serde_json::Value) -> Option<Usage> {
        let input_tokens = u.get("prompt_tokens")?.as_u64()? as u32;
        let output_tokens = u.get("completion_tokens")?.as_u64()? as u32;
        let cached_tokens: u32 = u
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        // Parse reasoning tokens from completion_tokens_details
        let reasoning_tokens: u32 = u
            .get("completion_tokens_details")
            .and_then(|d| d.get("reasoning_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        if cached_tokens > 0 {
            let hit_pct = (cached_tokens * 100).checked_div(input_tokens).unwrap_or(0);
            tracing::info!("Cache: {hit_pct}% hit ({cached_tokens}/{input_tokens} prompt tokens)");
        }
        tracing::info!(
            input_tokens,
            output_tokens,
            reasoning_tokens,
            total = input_tokens
                .saturating_add(output_tokens)
                .saturating_add(reasoning_tokens),
            "Usage breakdown"
        );
        Some(Usage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens
                .saturating_add(output_tokens)
                .saturating_add(reasoning_tokens),
            cache_read_input_tokens: cached_tokens,
            cache_creation_input_tokens: 0,
            reasoning_tokens: if reasoning_tokens > 0 {
                Some(reasoning_tokens)
            } else {
                None
            },
        })
    }

    /// Extract an error from a JSON value that has a top-level "error" object.
    fn extract_stream_error(
        data: &serde_json::Value,
    ) -> Option<Result<StreamEvent, ProviderError>> {
        let error = data.get("error")?;
        let code = error
            .get("code")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown streaming error");
        Some(Err(ProviderError::api(format!("{}: {}", code, message))))
    }

    async fn complete_stream_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                ProviderError::auth(
                    "OpenAI API key is required. Set api_key in config or OPENAI_API_KEY env var",
                )
            })?
            .expose_secret();

        let url = format!("{}/chat/completions", self.endpoint());

        // Build messages array
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenAiMessage {
                role: "system".to_string(),
                content: Some(serde_json::Value::String(system_prompt.clone())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        messages.extend(Self::convert_messages(&request.messages));

        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(tools) => {
                let normalized = crate::tools::normalize_tools_for_openai(&tools);
                crate::tools::sanitize_tools_for_strict_providers(&normalized)
            }
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
                    .unwrap_or_default()
            }
        };

        let body = self.build_request_body(
            request.model.clone(),
            messages,
            tools,
            request.max_tokens,
            request.temperature,
            request
                .output_config
                .as_ref()
                .and_then(|c| c.effort.as_ref()),
            Some(true),
            request.output_config.as_ref(),
            request.tool_choice.clone(),
            request.parallel_tool_calls,
            request.session_id.as_ref(),
            request.thinking.as_ref(),
        );

        // Build request with per-request headers
        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

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
                401 | 403 => ProviderError::auth(format!(
                    "Authentication failed. Check your OPENAI_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(error_text.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "OpenAI service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::api(format!("{}: {}", status, error_text)),
            });
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        // Parse SSE events from byte stream using the shared SseByteBuffer + parse_sse_lines helper
        let line_buffer = crate::sse::SseByteBuffer::new();
        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(e.to_string()))])
                }
            };

            let lines = line_buffer.feed_chunk(&chunk);
            let complete_lines = lines.join("\n");

            let events = Self::parse_sse_lines_stream_events(&complete_lines);

            futures::stream::iter(events)
        });

        Ok(Box::pin(sse_stream))
    }
}

/// Unified LLMProvider trait implementation for OpenAI
///
/// This implementation wraps the provider methods to conform to the unified
/// trait from rustycode-protocol, providing a bridge between the two API versions.
#[async_trait]
impl UnifiedLLMProvider for OpenAiProvider {
    async fn list_models(&self) -> anyhow::Result<Vec<UnifiedModelInfo>> {
        // Use the existing provider list_models and convert results
        let model_names = <Self as LLMProvider>::list_models(self)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list models: {}", e))?;

        Ok(model_names
            .into_iter()
            .map(|name| {
                // Map known OpenAI models to their specifications
                let (context_window, cost_input, cost_output) = match name.as_str() {
                    "gpt-4" | "gpt-4-turbo" => (8192, 0.03, 0.06),
                    "gpt-4-32k" => (32000, 0.06, 0.12),
                    "gpt-4-turbo-preview" | "gpt-4-turbo-2024-04-09" => (128000, 0.01, 0.03),
                    "gpt-4o" | "gpt-4o-2024-11-20" => (128000, 0.005, 0.015),
                    "gpt-4o-mini" => (128000, 0.00015, 0.0006),
                    "gpt-3.5-turbo" => (4096, 0.0005, 0.0015),
                    _ => (4096, 0.001, 0.002), // Default fallback
                };

                UnifiedModelInfo {
                    name,
                    provider: "openai".to_string(),
                    context_window,
                    supports_streaming: true,
                    cost_per_1k_input_tokens: cost_input,
                    cost_per_1k_output_tokens: cost_output,
                }
            })
            .collect())
    }

    async fn is_available(&self) -> anyhow::Result<bool> {
        // Convert provider bool to Result<bool>
        Ok(<Self as LLMProvider>::is_available(self).await)
    }

    async fn complete(
        &self,
        request: UnifiedCompletionRequest,
    ) -> anyhow::Result<UnifiedCompletionResponse> {
        // Convert unified request to provider request
        use rustycode_protocol::MessageContent;

        let v2_request = CompletionRequest {
            model: request.model.clone(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
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
            api_mode: None,
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

    fn estimate_cost(&self, request: &UnifiedCompletionRequest) -> anyhow::Result<UnifiedCost> {
        // Estimate tokens from prompt (rough calculation: ~1.3 chars per token)
        let input_tokens = request.prompt.len() as f64 / 4.0;
        let output_tokens = request.max_tokens.unwrap_or(1000) as f64 / 2.0;

        // Use model-specific pricing
        let (cost_in, cost_out) = match request.model.as_str() {
            "gpt-4" | "gpt-4-turbo" => (0.03, 0.06),
            "gpt-4-32k" => (0.06, 0.12),
            "gpt-4-turbo-preview" | "gpt-4-turbo-2024-04-09" => (0.01, 0.03),
            "gpt-4o" | "gpt-4o-2024-11-20" => (0.005, 0.015),
            "gpt-4o-mini" => (0.00015, 0.0006),
            "gpt-3.5-turbo" => (0.0005, 0.0015),
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
    fn test_creates_provider() {
        let config = make_config(Some(
            &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test123".to_string()),
        ));
        let provider = OpenAiProvider::new(config, "gpt-4o".to_string()).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert_eq!(<OpenAiProvider as LLMProvider>::name(&provider), "openai");
        assert_eq!(provider.endpoint(), "https://api.openai.com/v1");
    }

    #[test]
    fn test_default_endpoint() {
        let p = OpenAiProvider::new(
            make_config(Some(
                &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
            )),
            "gpt-4o".to_string(),
        )
        .unwrap();
        assert_eq!(p.endpoint(), "https://api.openai.com/v1");
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some(
            &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
        ));
        config.base_url = Some("https://proxy.example.com/v1".to_string());
        let p = OpenAiProvider::new(config, "gpt-4o".to_string()).unwrap();
        assert_eq!(p.endpoint(), "https://proxy.example.com/v1");
    }

    #[test]
    fn test_provider_name() {
        let p = OpenAiProvider::new(
            make_config(Some(
                &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
            )),
            "gpt-4o".to_string(),
        )
        .unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert_eq!(<OpenAiProvider as LLMProvider>::name(&p), "openai");
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = OpenAiProvider::metadata();
        assert_eq!(metadata.display_name, "OpenAI");
        assert_eq!(metadata.provider_id, "openai");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = OpenAiProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = OpenAiProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"OPENAI_API_KEY".to_string())
        );
    }

    #[test]
    fn test_metadata_recommended_models() {
        let metadata = OpenAiProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(model_ids.iter().any(|id| id.contains("gpt-4o")));
    }

    #[test]
    fn test_openai_request_serialization() {
        let request = OpenAiRequest {
            model: "gpt-4o".to_string(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("Hello".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            temperature: Some(0.5),
            max_tokens: Some(2048),
            max_completion_tokens: None,
            stream: Some(true),
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning_effort: None,
            response_format: None,
            thinking: None,
            stream_options: None,
            prompt_cache_key: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"gpt-4o\""));
        assert!(json.contains("\"temperature\":0.5"));
        assert!(json.contains("\"max_tokens\":2048"));
        assert!(json.contains("\"stream\":true"));
        // tools, max_completion_tokens, reasoning_effort, thinking should be absent when None
        assert!(!json.contains("\"tools\""));
        assert!(!json.contains("\"max_completion_tokens\""));
        assert!(!json.contains("\"reasoning_effort\""));
        assert!(!json.contains("\"thinking\""));
        assert!(!json.contains("\"tool_choice\""));
        assert!(!json.contains("\"parallel_tool_calls\""));
    }

    #[test]
    fn test_openai_request_serialization_with_tools() {
        let request = OpenAiRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            max_completion_tokens: None,
            stream: None,
            tools: Some(vec![serde_json::json!({
                "type": "function",
                "function": {"name": "get_weather", "description": "Get weather"}
            })]),
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning_effort: None,
            response_format: None,
            thinking: None,
            stream_options: None,
            prompt_cache_key: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"tools\""));
        assert!(json.contains("get_weather"));
        // temperature now has skip_serializing_if so it should be absent when None
        assert!(!json.contains("\"temperature\""));
        // max_tokens and stream have skip_serializing_if so they are absent
        assert!(!json.contains("\"max_tokens\""));
        assert!(!json.contains("\"stream\""));
    }

    #[test]
    fn test_openai_request_reasoning_model() {
        let request = OpenAiRequest {
            model: "o4-mini".to_string(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("Solve this".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            temperature: None,
            max_tokens: None,
            max_completion_tokens: Some(4096),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning_effort: Some("medium".to_string()),
            response_format: None,
            thinking: None,
            stream_options: None,
            prompt_cache_key: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"o4-mini\""));
        assert!(json.contains("\"max_completion_tokens\":4096"));
        assert!(json.contains("\"reasoning_effort\":\"medium\""));
        // max_tokens should be absent (None) for reasoning models
        assert!(!json.contains("\"max_tokens\":"));
        // temperature absent when None
        assert!(!json.contains("\"temperature\""));
    }

    #[test]
    fn test_stream_options_included_when_streaming() {
        let request = OpenAiRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            max_completion_tokens: None,
            stream: Some(true),
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning_effort: None,
            response_format: None,
            thinking: None,
            stream_options: Some(serde_json::json!({"include_usage": true})),
            prompt_cache_key: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"stream_options\""));
        assert!(json.contains("\"include_usage\":true"));

        let request_no_stream = OpenAiRequest {
            stream: Some(false),
            stream_options: None,
            prompt_cache_key: None,
            ..request
        };
        let json_no_stream = serde_json::to_string(&request_no_stream).unwrap();
        assert!(!json_no_stream.contains("\"stream_options\""));
    }

    #[test]
    fn test_is_reasoning_model() {
        assert!(OpenAiProvider::is_reasoning_model("o1"));
        assert!(OpenAiProvider::is_reasoning_model("o1-mini"));
        assert!(OpenAiProvider::is_reasoning_model("o3"));
        assert!(OpenAiProvider::is_reasoning_model("o3-mini"));
        assert!(OpenAiProvider::is_reasoning_model("o4-mini"));
        assert!(OpenAiProvider::is_reasoning_model("glm-5.1"));
        assert!(OpenAiProvider::is_reasoning_model("glm-5"));
        // GPT-5.x models support reasoning
        assert!(OpenAiProvider::is_reasoning_model("gpt-5"));
        assert!(OpenAiProvider::is_reasoning_model("gpt-5.2"));
        assert!(OpenAiProvider::is_reasoning_model("gpt-5.1-mini"));
        // Non-reasoning models
        assert!(!OpenAiProvider::is_reasoning_model("gpt-4o"));
        assert!(!OpenAiProvider::is_reasoning_model("optimum"));
        assert!(!OpenAiProvider::is_reasoning_model("glm-4"));
    }

    #[test]
    fn test_build_request_body_standard_model() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "gpt-4o".to_string(),
            vec![],
            vec![],
            Some(2048),
            Some(0.7),
            None,
            Some(false),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(body.max_tokens, Some(2048));
        assert_eq!(body.max_completion_tokens, None);
        assert_eq!(body.reasoning_effort, None);
    }

    #[test]
    fn test_build_request_body_reasoning_model() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "o4-mini".to_string(),
            vec![],
            vec![],
            Some(4096),
            None,
            Some(&crate::provider::EffortLevel::High),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(body.max_tokens, None);
        assert_eq!(body.max_completion_tokens, Some(4096));
        assert_eq!(body.reasoning_effort, Some("high".to_string()));
    }

    #[test]
    fn test_build_request_body_glm5_thinking() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "glm-5.1".to_string(),
            vec![],
            vec![],
            Some(8192),
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(body.max_tokens, None);
        assert_eq!(body.max_completion_tokens, Some(8192));
        let thinking = body
            .thinking
            .as_ref()
            .expect("GLM-5.x should have thinking enabled");
        assert_eq!(thinking["type"], "enabled");
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"thinking\":{\"type\":\"enabled\"}"));
    }

    #[test]
    fn test_build_request_body_standard_model_no_thinking() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "gpt-4o".to_string(),
            vec![],
            vec![],
            Some(2048),
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
        );
        assert!(body.thinking.is_none());
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("\"thinking\""));
    }

    #[test]
    fn test_openai_response_deserialization() {
        let json = r#"{
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Hello! How can I help?"},
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18,
                "prompt_tokens_details": {"cached_tokens": 5}
            },
            "model": "gpt-4o"
        }"#;
        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "gpt-4o");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Hello! How can I help?")
        );
        assert!(response.usage.is_some());
        let usage = response.usage.as_ref().unwrap();
        assert_eq!(usage.total_tokens, 18);
        assert_eq!(
            usage.prompt_tokens_details.as_ref().unwrap().cached_tokens,
            5
        );
    }

    #[test]
    fn test_openai_response_deserialization_no_cached_tokens() {
        let json = r#"{
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Hi"},
                    "finish_reason": "stop"
                }
            ],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
            "model": "gpt-4o"
        }"#;
        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        let usage = response.usage.as_ref().unwrap();
        // prompt_tokens_details is optional and should be None when absent
        assert!(usage.prompt_tokens_details.is_none());
    }

    #[test]
    fn test_openai_response_usage_optional() {
        let json = r#"{
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Hi"},
                    "finish_reason": null
                }
            ],
            "usage": null,
            "model": "gpt-4o"
        }"#;
        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage.is_none());
        assert!(response.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_openai_response_with_tool_calls() {
        let json = r#"{
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_abc123",
                                "type": "function",
                                "function": {
                                    "name": "get_weather",
                                    "arguments": "{\"location\": \"San Francisco\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ],
            "usage": {"prompt_tokens": 50, "completion_tokens": 20, "total_tokens": 70},
            "model": "gpt-4o"
        }"#;
        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.choices.len(), 1);
        let msg = &response.choices[0].message;
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_some());
        let tool_calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(
            tool_calls[0].function.arguments,
            "{\"location\": \"San Francisco\"}"
        );
        assert_eq!(
            response.choices[0].finish_reason.as_deref(),
            Some("tool_calls")
        );
    }

    #[test]
    fn test_openai_response_with_text_and_tool_calls() {
        let json = r#"{
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "I'll check the weather for you.",
                        "tool_calls": [
                            {
                                "id": "call_xyz",
                                "type": "function",
                                "function": {
                                    "name": "read_file",
                                    "arguments": "{\"path\": \"src/main.rs\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {"prompt_tokens": 30, "completion_tokens": 15, "total_tokens": 45},
            "model": "gpt-4o"
        }"#;
        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        let msg = &response.choices[0].message;
        assert_eq!(
            msg.content.as_deref(),
            Some("I'll check the weather for you.")
        );
        assert!(msg.tool_calls.is_some());
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string());
        let p = OpenAiProvider::new(make_config(Some(&api_key)), "gpt-4o".to_string()).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        let models = <OpenAiProvider as LLMProvider>::list_models(&p)
            .await
            .unwrap();
        assert!(models.iter().any(|m| m == "gpt-5.2"));
        assert!(models.iter().any(|m| m == "o4-mini"));
        assert!(models.iter().any(|m| m == "gpt-4o"));
        assert!(models.iter().any(|m| m == "o3"));
    }

    #[test]
    fn test_new_without_validation() {
        let config = make_config(Some(
            &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
        ));
        let provider =
            OpenAiProvider::new_without_validation(config, "gpt-4o".to_string()).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert_eq!(<OpenAiProvider as LLMProvider>::name(&provider), "openai");
    }

    #[test]
    fn test_endpoint_trims_trailing_slash() {
        let mut config = make_config(Some(
            &std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-test".to_string()),
        ));
        config.base_url = Some("https://proxy.example.com/v1/".to_string());
        let p = OpenAiProvider::new(config, "gpt-4o".to_string()).unwrap();
        assert_eq!(p.endpoint(), "https://proxy.example.com/v1");
    }

    #[tokio::test]
    async fn test_is_available_with_key() {
        let p = OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert!(<OpenAiProvider as LLMProvider>::is_available(&p).await);
    }

    #[tokio::test]
    async fn test_is_available_without_key() {
        let p = OpenAiProvider::new_without_validation(make_config(None), "gpt-4o".to_string())
            .unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert!(!<OpenAiProvider as LLMProvider>::is_available(&p).await);
    }

    #[test]
    fn test_convert_messages_simple_text() {
        let messages = vec![ChatMessage::user("Hello")];
        let result = OpenAiProvider::convert_messages(&messages);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content.as_ref().unwrap().as_str(), Some("Hello"));
        assert!(result[0].tool_calls.is_none());
        assert!(result[0].tool_call_id.is_none());
    }

    #[test]
    fn test_convert_messages_tool_result() {
        use rustycode_protocol::{ContentBlock, MessageContent};
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result(
                "call_abc123",
                "File contents here",
            )]),
        }];
        let result = OpenAiProvider::convert_messages(&messages);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "tool");
        assert_eq!(result[0].tool_call_id.as_deref(), Some("call_abc123"));
        assert_eq!(
            result[0].content.as_ref().unwrap().as_str(),
            Some("File contents here")
        );
    }

    #[test]
    fn test_convert_messages_assistant_with_tool_use() {
        use rustycode_protocol::{ContentBlock, MessageContent};
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("I'll read that file."),
                ContentBlock::tool_use(
                    "call_xyz",
                    "read_file",
                    serde_json::json!({"path": "test.rs"}),
                ),
            ]),
        }];
        let result = OpenAiProvider::convert_messages(&messages);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        assert!(result[0].tool_calls.is_some());
        let tool_calls = result[0].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_xyz");
        assert_eq!(tool_calls[0].function.name, "read_file");
    }

    #[test]
    fn test_convert_messages_mixed_tool_result_and_text() {
        use rustycode_protocol::{ContentBlock, MessageContent};
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("Here's the result:"),
                ContentBlock::tool_result("call_1", "output data"),
                ContentBlock::tool_result("call_2", "more data"),
            ]),
        }];
        let result = OpenAiProvider::convert_messages(&messages);
        // Should produce: 1 message with text + 2 separate tool result messages
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "tool");
        assert_eq!(result[1].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(result[2].role, "tool");
        assert_eq!(result[2].tool_call_id.as_deref(), Some("call_2"));
    }

    #[test]
    fn test_openai_message_tool_result_serialization() {
        let msg = OpenAiMessage {
            role: "tool".to_string(),
            content: Some(serde_json::Value::String("result data".to_string())),
            tool_calls: None,
            tool_call_id: Some("call_abc".to_string()),
            name: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"tool\""));
        assert!(json.contains("\"tool_call_id\":\"call_abc\""));
        assert!(json.contains("\"content\":\"result data\""));
        assert!(!json.contains("\"tool_calls\""));
        assert!(!json.contains("\"name\""));
    }

    #[test]
    fn test_openai_request_with_tool_choice() {
        let request = OpenAiRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            max_completion_tokens: None,
            stream: None,
            tools: Some(vec![serde_json::json!({
                "type": "function",
                "function": {"name": "test", "description": "test"}
            })]),
            tool_choice: Some(serde_json::json!("auto")),
            parallel_tool_calls: Some(false),
            reasoning_effort: None,
            response_format: None,
            thinking: None,
            stream_options: None,
            prompt_cache_key: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"tool_choice\":\"auto\""));
        assert!(json.contains("\"parallel_tool_calls\":false"));
    }

    // ── SSE Streaming Edge Case Tests ────────────────────────────────────────

    /// Empty content in delta (`{"delta":{"content":""}}`) should not emit a text event.
    #[test]
    fn test_sse_empty_content_delta_not_emitted() {
        let lines = r#"data: {"choices":[{"delta":{"content":""},"finish_reason":null}]}"#;
        let events = OpenAiProvider::parse_sse_lines(lines);
        let text_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Ok(crate::provider::SSEEvent::ContentBlockDelta {
                        delta: crate::provider::ContentDelta::Text { .. },
                        ..
                    })
                )
            })
            .collect();
        assert!(
            text_events.is_empty(),
            "empty content delta should not produce a text event"
        );
    }

    /// Multiple tool calls in a single SSE response should produce separate ContentBlockStart
    /// events, each with the correct index.
    #[test]
    fn test_sse_multiple_tool_calls_get_separate_starts() {
        let lines = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_aaa\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}},{\"index\":1,\"id\":\"call_bbb\",\"type\":\"function\",\"function\":{\"name\":\"write_file\",\"arguments\":\"\"}}]}}]}\n";
        let events = OpenAiProvider::parse_sse_lines(lines);

        let starts: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Ok(crate::provider::SSEEvent::ContentBlockStart {
                    index,
                    content_block,
                }) => Some((*index, content_block.clone())),
                _ => None,
            })
            .collect();

        assert_eq!(
            starts.len(),
            2,
            "should have 2 ContentBlockStart events for 2 tool calls"
        );
        assert_eq!(starts[0].0, 0);
        assert_eq!(starts[1].0, 1);

        match &starts[0].1 {
            crate::provider::ContentBlockType::ToolUse { name, .. } => {
                assert_eq!(name, "read_file");
            }
            other => panic!("expected ToolUse, got {:?}", other),
        }
        match &starts[1].1 {
            crate::provider::ContentBlockType::ToolUse { name, .. } => {
                assert_eq!(name, "write_file");
            }
            other => panic!("expected ToolUse, got {:?}", other),
        }
    }

    /// Rapid alternating text and tool deltas — state tracking must remain correct.
    #[test]
    fn test_sse_alternating_text_and_tool_deltas() {
        let lines = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]},\"finish_reason\":null}]}
data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"test.rs\\\"}\"}}]},\"finish_reason\":null}]}";
        let events = OpenAiProvider::parse_sse_lines(lines);

        let text_deltas: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                Ok(crate::provider::SSEEvent::ContentBlockDelta {
                    delta: crate::provider::ContentDelta::Text { text },
                    ..
                }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text_deltas, vec!["Hello", " world"]);

        let json_deltas: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                Ok(crate::provider::SSEEvent::ContentBlockDelta {
                    delta: crate::provider::ContentDelta::PartialJson { partial_json },
                    ..
                }) => Some(partial_json.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(json_deltas.len(), 2);
        assert_eq!(json_deltas[0], "{\"path\":");
        assert_eq!(json_deltas[1], "\"test.rs\"}");
    }

    /// Both "tool_calls" and "stop" finish reasons should produce ContentBlockStop + MessageDelta.
    #[test]
    fn test_sse_finish_reason_tool_calls_and_stop() {
        let lines_tool = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        let events_tool = OpenAiProvider::parse_sse_lines(lines_tool);
        let has_stop = events_tool
            .iter()
            .any(|e| matches!(e, Ok(crate::provider::SSEEvent::ContentBlockStop { .. })));
        let has_msg_delta = events_tool.iter().any(|e| matches!(
            e,
            Ok(crate::provider::SSEEvent::MessageDelta { stop_reason: Some(ref s), .. }) if s == "tool_use"
        ));
        assert!(
            has_stop,
            "tool_calls finish should produce ContentBlockStop"
        );
        assert!(
            has_msg_delta,
            "tool_calls finish should produce MessageDelta"
        );

        let lines_stop = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let events_stop = OpenAiProvider::parse_sse_lines(lines_stop);
        let has_stop2 = events_stop
            .iter()
            .any(|e| matches!(e, Ok(crate::provider::SSEEvent::ContentBlockStop { .. })));
        let has_msg_delta2 = events_stop.iter().any(|e| matches!(
            e,
            Ok(crate::provider::SSEEvent::MessageDelta { stop_reason: Some(ref s), .. }) if s == "end_turn"
        ));
        assert!(has_stop2, "stop finish should produce ContentBlockStop");
        assert!(has_msg_delta2, "stop finish should produce MessageDelta");
    }

    /// SSE chunks from OpenAI typically don't include a model field — parser should handle it.
    #[test]
    fn test_sse_missing_model_field_still_parses() {
        let lines = r#"data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#;
        let events = OpenAiProvider::parse_sse_lines(lines);
        let text_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Ok(crate::provider::SSEEvent::ContentBlockDelta {
                    delta: crate::provider::ContentDelta::Text { text },
                    ..
                }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text_events, vec!["Hi".to_string()]);
    }

    /// Response with null content should not emit a text delta event.
    #[test]
    fn test_sse_null_content_produces_no_text_event() {
        let lines = r#"data: {"choices":[{"delta":{"content":null},"finish_reason":null}]}"#;
        let events = OpenAiProvider::parse_sse_lines(lines);
        let text_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Ok(crate::provider::SSEEvent::ContentBlockDelta {
                        delta: crate::provider::ContentDelta::Text { .. },
                        ..
                    })
                )
            })
            .collect();
        assert!(
            text_events.is_empty(),
            "null content should not produce a text delta event"
        );
    }

    // ── Protocol-level message roundtrip tests ────────────────────────────────

    use rustycode_protocol::{ContentBlock, ImageSource, MessageContent};

    #[test]
    fn test_roundtrip_simple_text_message() {
        let msgs = vec![ChatMessage::user("Hello, world!")];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(
            result[0].content.as_ref().unwrap().as_str(),
            Some("Hello, world!")
        );
        assert!(result[0].tool_calls.is_none());
        assert!(result[0].tool_call_id.is_none());
    }

    #[test]
    fn test_roundtrip_text_block() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::text("Block text")]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        // Single text part should serialize as plain string
        assert_eq!(
            result[0].content.as_ref().unwrap().as_str(),
            Some("Block text")
        );
    }

    #[test]
    fn test_roundtrip_tool_use_block() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::tool_use(
                "call_123",
                "read_file",
                serde_json::json!({"path": "a.rs"}),
            )]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        // Tool uses force role to "assistant"
        assert_eq!(result[0].role, "assistant");
        let tcs = result[0]
            .tool_calls
            .as_ref()
            .expect("should have tool_calls");
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "call_123");
        assert_eq!(tcs[0].function.name, "read_file");
        assert_eq!(tcs[0].r#type, "function");
        // Arguments should be a JSON string
        let args: serde_json::Value = serde_json::from_str(&tcs[0].function.arguments).unwrap();
        assert_eq!(args["path"], "a.rs");
    }

    #[test]
    fn test_roundtrip_tool_result_block() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result(
                "call_abc",
                "file contents here",
            )]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        // Tool results become separate role="tool" messages
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "tool");
        assert_eq!(result[0].tool_call_id.as_deref(), Some("call_abc"));
        assert_eq!(
            result[0].content.as_ref().unwrap().as_str(),
            Some("file contents here")
        );
    }

    #[test]
    fn test_roundtrip_tool_error_block_prefixed() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_error(
                "call_err",
                "command not found: foo",
            )]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "tool");
        assert_eq!(result[0].tool_call_id.as_deref(), Some("call_err"));
        // OpenAI has no is_error field — content is prefixed with "Error: "
        assert_eq!(
            result[0].content.as_ref().unwrap().as_str(),
            Some("Error: command not found: foo")
        );
    }

    #[test]
    fn test_roundtrip_image_block() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::image(ImageSource::base64(
                "image/png",
                "iVBOR",
            ))]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        // Content should be an array with an image_url part
        let content = result[0].content.as_ref().unwrap();
        assert!(content.is_object() || content.is_array());
    }

    #[test]
    fn test_roundtrip_thinking_block_converted_to_text() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::thinking(
                "internal reasoning",
                "sig123",
            )]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        let content = result[0].content.as_ref().unwrap().as_str().unwrap();
        assert!(content.contains("[prior-reasoning]"));
        assert!(content.contains("internal reasoning"));
    }

    #[test]
    fn test_roundtrip_empty_thinking_block_skipped() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::thinking("", "sig123")]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert!(result.is_empty());
    }

    #[test]
    fn test_roundtrip_mixed_text_and_tool_use() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("Let me read that file."),
                ContentBlock::tool_use("call_x", "read_file", serde_json::json!({"path": "x.rs"})),
            ]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        // Should have text content
        assert!(result[0].content.is_some());
        // Should have tool_calls
        let tcs = result[0]
            .tool_calls
            .as_ref()
            .expect("should have tool_calls");
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "call_x");
    }

    #[test]
    fn test_roundtrip_mixed_text_and_tool_result() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![
                ContentBlock::text("Here's the result:"),
                ContentBlock::tool_result("call_1", "output data"),
                ContentBlock::tool_result("call_2", "more output"),
            ]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        // Text stays in user message; each tool_result becomes a separate "tool" message
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "tool");
        assert_eq!(result[1].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(result[2].role, "tool");
        assert_eq!(result[2].tool_call_id.as_deref(), Some("call_2"));
    }

    #[test]
    fn test_roundtrip_empty_message_content() {
        let msgs = vec![ChatMessage::user("")];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content.as_ref().unwrap().as_str(), Some(""));
    }

    #[test]
    fn test_roundtrip_whitespace_only_content() {
        let msgs = vec![ChatMessage::user("   \n\t  ")];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content.as_ref().unwrap().as_str(),
            Some("   \n\t  ")
        );
    }

    #[test]
    fn test_roundtrip_very_long_text_content() {
        let long_text = "A".repeat(12_000);
        let msgs = vec![ChatMessage::user(long_text.clone())];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content.as_ref().unwrap().as_str(),
            Some(long_text.as_str())
        );
    }

    #[test]
    fn test_roundtrip_empty_blocks_array() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![]),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        // Empty blocks array: no other_parts and no tool_calls, so no message emitted
        assert!(result.is_empty());
    }

    #[test]
    fn test_roundtrip_system_role_mapping() {
        let msgs = vec![ChatMessage {
            role: MessageRole::System,
            content: MessageContent::simple("System prompt"),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "system");
    }

    #[test]
    fn test_roundtrip_tool_role_mapping() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Tool("call_id".to_string()),
            content: MessageContent::simple("Tool output"),
        }];
        let result = OpenAiProvider::convert_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "tool");
    }

    // ============================================================
    // New tests: SSE parsing, tool call streaming, and request
    // serialization for standard vs reasoning models
    // ============================================================

    // --- SSE Event Parsing ---

    #[test]
    fn test_sse_parse_single_content_chunk() {
        let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta { index, delta }) => {
                assert_eq!(*index, 0);
                match delta {
                    crate::provider::ContentDelta::Text { text } => {
                        assert_eq!(text, "Hello");
                    }
                    _ => panic!("expected Text delta"),
                }
            }
            _ => panic!("expected ContentBlockDelta, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_sse_parse_multiple_content_chunks() {
        let input = "\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"!\"},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 3);
        let texts: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                Ok(crate::provider::SSEEvent::ContentBlockDelta {
                    delta: crate::provider::ContentDelta::Text { text },
                    ..
                }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["Hello", " world", "!"]);
    }

    #[test]
    fn test_sse_parse_empty_content_skipped() {
        let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::Text { text }) => {
                assert!(text.is_empty());
            }
            _ => panic!(
                "expected fallback Text event for empty input, got {:?}",
                events[0]
            ),
        }
    }

    #[test]
    fn test_sse_parse_done_event() {
        let input = "data: [DONE]\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::MessageStop) => {}
            _ => panic!("expected MessageStop, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_sse_parse_finish_reason_stop() {
        let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 2);
        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockStop { index }) => {
                assert_eq!(*index, 0);
            }
            _ => panic!("expected ContentBlockStop, got {:?}", events[0]),
        }
        match &events[1] {
            Ok(crate::provider::SSEEvent::MessageDelta { stop_reason, .. }) => {
                assert_eq!(stop_reason.as_deref(), Some("end_turn"));
            }
            _ => panic!("expected MessageDelta, got {:?}", events[1]),
        }
    }

    #[test]
    fn test_sse_parse_finish_reason_tool_calls() {
        let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 2);
        match &events[1] {
            Ok(crate::provider::SSEEvent::MessageDelta { stop_reason, .. }) => {
                assert_eq!(stop_reason.as_deref(), Some("tool_use"));
            }
            _ => panic!("expected MessageDelta with tool_calls"),
        }
    }

    #[test]
    fn test_sse_parse_finish_reason_length_not_stop() {
        let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::MessageDelta { stop_reason, .. }) => {
                assert_eq!(stop_reason.as_deref(), Some("max_tokens"));
            }
            _ => panic!("expected MessageDelta with length, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_sse_parse_ignores_non_data_lines() {
        let input = "\
: comment line\n\
event: ping\n\
id: 42\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta { delta, .. }) => match delta {
                crate::provider::ContentDelta::Text { text } => assert_eq!(text, "hi"),
                _ => panic!("expected Text delta"),
            },
            _ => panic!("expected ContentBlockDelta"),
        }
    }

    #[test]
    fn test_sse_parse_malformed_json_ignored() {
        let input = "data: {not valid json}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::Text { text }) => assert!(text.is_empty()),
            _ => panic!("expected fallback Text event"),
        }
    }

    #[test]
    fn test_sse_parse_empty_input() {
        let events = OpenAiProvider::parse_sse_lines("");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::Text { text }) => assert!(text.is_empty()),
            _ => panic!("expected fallback Text event for empty input"),
        }
    }

    #[test]
    fn test_sse_parse_content_with_special_characters() {
        let input = format!(
            "{}\n",
            r#"data: {"id":"chatcmpl-1","choices":[{"index":0,"delta":{"content":"He said \"hello\" and left\nGoodbye 👋"},"finish_reason":null}]}"#
        );
        let events = OpenAiProvider::parse_sse_lines(&input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta { delta, .. }) => match delta {
                crate::provider::ContentDelta::Text { text } => {
                    assert!(text.contains("\"hello\""));
                    assert!(text.contains('\n'));
                    assert!(text.contains('\u{1F44B}'));
                }
                _ => panic!("expected Text delta"),
            },
            _ => panic!("expected ContentBlockDelta"),
        }
    }

    // --- Tool Call Streaming ---

    #[test]
    fn test_sse_parse_tool_call_start() {
        let input = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc123\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockStart {
                index,
                content_block:
                    crate::provider::ContentBlockType::ToolUse {
                        id,
                        name,
                        input: tool_input,
                    },
            }) => {
                assert_eq!(*index, 0);
                assert_eq!(id, "call_abc123");
                assert_eq!(name, "get_weather");
                assert!(tool_input.is_none());
            }
            _ => panic!(
                "expected ContentBlockStart with ToolUse, got {:?}",
                events[0]
            ),
        }
    }

    #[test]
    fn test_sse_parse_tool_call_argument_deltas() {
        let input = "\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"lo\"}}]},\"finish_reason\":null}]}\n\
data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"cation\\\": \\\"SF\\\"}\"}}]},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 2);

        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta { index, delta }) => {
                assert_eq!(*index, 0);
                match delta {
                    crate::provider::ContentDelta::PartialJson { partial_json } => {
                        assert_eq!(partial_json, "{\"lo");
                    }
                    _ => panic!("expected PartialJson delta"),
                }
            }
            _ => panic!("expected ContentBlockDelta with PartialJson"),
        }

        match &events[1] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta { index, delta }) => {
                assert_eq!(*index, 0);
                match delta {
                    crate::provider::ContentDelta::PartialJson { partial_json } => {
                        assert_eq!(partial_json, "cation\": \"SF\"}");
                    }
                    _ => panic!("expected PartialJson delta"),
                }
            }
            _ => panic!("expected ContentBlockDelta with PartialJson"),
        }
    }

    #[test]
    fn test_sse_parse_tool_call_full_flow() {
        let input = "\
data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_xyz\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\" \\\"main.rs\\\"}\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\
data: [DONE]\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        // ContentBlockStart + 2 PartialJson + ContentBlockStop + MessageDelta + MessageStop = 6
        assert_eq!(events.len(), 6);

        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockStart {
                content_block: crate::provider::ContentBlockType::ToolUse { id, name, .. },
                ..
            }) => {
                assert_eq!(id, "call_xyz");
                assert_eq!(name, "read_file");
            }
            _ => panic!("expected ContentBlockStart at index 0, got {:?}", events[0]),
        }

        match &events[1] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta {
                delta: crate::provider::ContentDelta::PartialJson { partial_json },
                ..
            }) => {
                assert_eq!(partial_json, "{\"path\":");
            }
            _ => panic!("expected PartialJson at index 1"),
        }

        match &events[2] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta {
                delta: crate::provider::ContentDelta::PartialJson { partial_json },
                ..
            }) => {
                assert_eq!(partial_json, " \"main.rs\"}");
            }
            _ => panic!("expected PartialJson at index 2"),
        }

        match &events[3] {
            Ok(crate::provider::SSEEvent::ContentBlockStop { index }) => {
                assert_eq!(*index, 0);
            }
            _ => panic!("expected ContentBlockStop at index 3"),
        }

        match &events[4] {
            Ok(crate::provider::SSEEvent::MessageDelta { stop_reason, .. }) => {
                assert_eq!(stop_reason.as_deref(), Some("tool_use"));
            }
            _ => panic!("expected MessageDelta at index 4"),
        }

        match &events[5] {
            Ok(crate::provider::SSEEvent::MessageStop) => {}
            _ => panic!("expected MessageStop at index 5"),
        }
    }

    #[test]
    fn test_sse_parse_parallel_tool_calls() {
        let input = "\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"type\":\"function\",\"function\":{\"name\":\"fn_a\",\"arguments\":\"\"}},{\"index\":1,\"id\":\"call_b\",\"type\":\"function\",\"function\":{\"name\":\"fn_b\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"x\\\":1}\"}},{\"index\":1,\"function\":{\"arguments\":\"{\\\"y\\\":2}\"}}]},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        // 2 ContentBlockStart + 2 PartialJson = 4 events
        assert_eq!(events.len(), 4);

        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockStart {
                index,
                content_block,
            }) => {
                assert_eq!(*index, 0);
                match content_block {
                    crate::provider::ContentBlockType::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call_a");
                        assert_eq!(name, "fn_a");
                    }
                    _ => panic!("expected ToolUse"),
                }
            }
            _ => panic!("expected ContentBlockStart for tool 0"),
        }

        match &events[1] {
            Ok(crate::provider::SSEEvent::ContentBlockStart {
                index,
                content_block,
            }) => {
                assert_eq!(*index, 1);
                match content_block {
                    crate::provider::ContentBlockType::ToolUse { id, name, .. } => {
                        assert_eq!(id, "call_b");
                        assert_eq!(name, "fn_b");
                    }
                    _ => panic!("expected ToolUse"),
                }
            }
            _ => panic!("expected ContentBlockStart for tool 1"),
        }

        match &events[2] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta { index, delta }) => {
                assert_eq!(*index, 0);
                match delta {
                    crate::provider::ContentDelta::PartialJson { partial_json } => {
                        assert_eq!(partial_json, "{\"x\":1}");
                    }
                    _ => panic!("expected PartialJson"),
                }
            }
            _ => panic!("expected PartialJson delta for tool 0"),
        }
        match &events[3] {
            Ok(crate::provider::SSEEvent::ContentBlockDelta { index, delta }) => {
                assert_eq!(*index, 1);
                match delta {
                    crate::provider::ContentDelta::PartialJson { partial_json } => {
                        assert_eq!(partial_json, "{\"y\":2}");
                    }
                    _ => panic!("expected PartialJson"),
                }
            }
            _ => panic!("expected PartialJson delta for tool 1"),
        }
    }

    #[test]
    fn test_sse_parse_tool_call_empty_arguments_skipped() {
        let input = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"test\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::ContentBlockStart { .. }) => {}
            _ => panic!("expected only ContentBlockStart (no PartialJson for empty args)"),
        }
    }

    #[test]
    fn test_sse_parse_tool_call_no_id_no_name_no_args() {
        let input = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0}]},\"finish_reason\":null}]}\n";
        let events = OpenAiProvider::parse_sse_lines(input);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(crate::provider::SSEEvent::Text { text }) => assert!(text.is_empty()),
            _ => panic!("expected fallback Text event"),
        }
    }

    // --- Request Serialization: Standard vs Reasoning Models ---

    #[test]
    fn test_build_request_body_standard_model_uses_max_tokens() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "gpt-4o".to_string(),
            vec![OpenAiMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("test".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            vec![],
            Some(1024),
            Some(0.3),
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(body.max_tokens, Some(1024));
        assert_eq!(body.max_completion_tokens, None);
        assert_eq!(body.reasoning_effort, None);
        assert_eq!(body.stream, Some(true));
        assert_eq!(body.model, "gpt-4o");

        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"max_tokens\":1024"));
        assert!(!json.contains("max_completion_tokens"));
        assert!(!json.contains("reasoning_effort"));
    }

    #[test]
    fn test_build_request_body_reasoning_model_uses_max_completion_tokens() {
        let provider = OpenAiProvider::new(make_config(Some("sk-test")), "o3".to_string()).unwrap();
        let body = provider.build_request_body(
            "o3".to_string(),
            vec![OpenAiMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("solve it".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            vec![],
            Some(8192),
            None,
            Some(&crate::provider::EffortLevel::Max),
            Some(false),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(body.max_tokens, None);
        assert_eq!(body.max_completion_tokens, Some(8192));
        assert_eq!(body.reasoning_effort, Some("xhigh".to_string()));

        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"max_completion_tokens\":8192"));
        assert!(json.contains("\"reasoning_effort\":\"xhigh\""));
        assert!(!json.contains("\"max_tokens\":"));
    }

    #[test]
    fn test_build_request_body_effort_levels() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "o4-mini".to_string()).unwrap();

        let cases = vec![
            (crate::provider::EffortLevel::Low, "low"),
            (crate::provider::EffortLevel::Medium, "medium"),
            (crate::provider::EffortLevel::High, "high"),
            (crate::provider::EffortLevel::Xhigh, "xhigh"),
            (crate::provider::EffortLevel::Max, "xhigh"),
        ];

        for (effort, expected_str) in cases {
            let body = provider.build_request_body(
                "o4-mini".to_string(),
                vec![],
                vec![],
                Some(4096),
                None,
                Some(&effort),
                None,
                None,
                None,
                None,
                None,
                None,
            );
            assert_eq!(
                body.reasoning_effort,
                Some(expected_str.to_string()),
                "EffortLevel::{:?} should map to {}",
                effort,
                expected_str
            );
        }
    }

    #[test]
    fn test_build_request_body_standard_model_no_effort() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "gpt-4o".to_string(),
            vec![],
            vec![],
            Some(2048),
            Some(0.5),
            Some(&crate::provider::EffortLevel::High),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(body.max_tokens, Some(2048));
        assert_eq!(body.max_completion_tokens, None);
        assert_eq!(body.reasoning_effort, None);
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let tools = vec![
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get current weather",
                    "parameters": {"type": "object", "properties": {"location": {"type": "string"}}}
                }
            }),
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": "read_file",
                    "description": "Read a file",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
                }
            }),
        ];
        let body = provider.build_request_body(
            "gpt-4o".to_string(),
            vec![],
            tools,
            Some(4096),
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
        );
        assert!(body.tools.is_some());
        let tools_val = body.tools.as_ref().unwrap();
        assert_eq!(tools_val.len(), 2);
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("get_weather"));
        assert!(json.contains("read_file"));
    }

    #[test]
    fn test_build_request_body_empty_tools_omits_field() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "gpt-4o".to_string(),
            vec![],
            vec![],
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(body.tools.is_none());
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("\"tools\""));
    }

    #[test]
    fn test_build_request_body_no_max_tokens_no_temperature() {
        let provider =
            OpenAiProvider::new(make_config(Some("sk-test")), "gpt-4o".to_string()).unwrap();
        let body = provider.build_request_body(
            "gpt-4o".to_string(),
            vec![OpenAiMessage {
                role: "user".to_string(),
                content: Some(serde_json::Value::String("hi".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            vec![],
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(body.max_tokens, None);
        assert_eq!(body.temperature, None);
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("\"max_tokens\""));
        assert!(!json.contains("\"temperature\""));
    }

    // --- SSE Parse: realistic full streaming conversation ---

    #[test]
    fn test_sse_parse_realistic_text_stream() {
        let input = "\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Rust\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" is\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" fast.\"},\"finish_reason\":null}]}\n\
\n\
data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\
\n\
data: [DONE]\n";
        let events = OpenAiProvider::parse_sse_lines(input);

        let content_deltas: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                Ok(crate::provider::SSEEvent::ContentBlockDelta {
                    delta: crate::provider::ContentDelta::Text { text },
                    ..
                }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(content_deltas, vec!["Rust", " is", " fast."]);

        let has_block_stop = events
            .iter()
            .any(|e| matches!(e, Ok(crate::provider::SSEEvent::ContentBlockStop { .. })));
        let has_msg_delta = events.iter().any(|e| {
            matches!(
                e,
                Ok(crate::provider::SSEEvent::MessageDelta {
                    stop_reason: Some(s),
                    ..
                }) if s == "end_turn"
            )
        });
        let has_msg_stop = events
            .iter()
            .any(|e| matches!(e, Ok(crate::provider::SSEEvent::MessageStop)));
        assert!(has_block_stop, "should have ContentBlockStop");
        assert!(has_msg_delta, "should have MessageDelta with stop");
        assert!(has_msg_stop, "should have MessageStop");
    }
}
