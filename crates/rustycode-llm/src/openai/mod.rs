//! OpenAI LLM provider implementation.
//!
//! Supports both Chat Completions API and Responses API, with automatic
//! fallback from Responses to Chat Completions when the endpoint doesn't
//! support it.

pub(crate) mod responses;
pub(crate) mod streaming;
pub(crate) mod types;

#[cfg(test)]
mod tests;

use crate::provider::{
    ApiMode, ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, MessageRole,
    ProviderConfig, ProviderError, StreamChunk, ThinkingBlock, Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use rustycode_tools_api::{Tool, ToolProfile, ToolRegistry, ToolSelector};

use crate::{build_request, get_api_key, shared_client};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

// Re-export internal types for use within this module's methods and tests.
use types::{
    OpenAiContentPart, OpenAiFunction, OpenAiImageUrl, OpenAiMessage, OpenAiRequest,
    OpenAiResponse, OpenAiToolCall,
};

/// OpenAI LLM provider (also supports OpenAI-compatible APIs)
pub struct OpenAiProvider {
    pub(crate) config: ProviderConfig,
    pub(crate) client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    pub(crate) default_model: String,
    pub(crate) tool_registry: Arc<ToolRegistry>,
    pub(crate) tool_selector: ToolSelector,
    /// Last Responses API response ID for server-side conversation state.
    pub(crate) last_response_id: Arc<std::sync::Mutex<Option<String>>>,
    /// Cached Responses API capability: None=unknown, Some(true)=supported, Some(false)=not supported.
    pub(crate) responses_api_supported: Arc<std::sync::Mutex<Option<bool>>>,
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
                reasoning_content: None,
            });
        }
        messages.extend(Self::convert_messages(&request.messages));

        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(tools) => {
                let normalized = crate::tools::normalize_tools_for_openai(&tools);
                // Chat Completions: no strict mode, no sanitization
                // (OpenAI accepts schemas as-is; sanitization is only for
                // providers that don't support strict mode like Zhipu/GLM)
                normalized
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
            return Err(crate::openai_compatible::map_http_error(
                status,
                text,
                &headers,
                "OpenAI",
                "OPENAI_API_KEY",
            ));
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
            let tool_registry = Arc::new(ToolRegistry::new());
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
        let tool_registry = Arc::new(ToolRegistry::new());
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

    /// Check if a model is a reasoning model (o-series, GPT-5.x, or GLM-5.x).
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
                                ContentBlock::Thinking { thinking, .. } => {
                                    if !thinking.is_empty() {
                                        reasoning_content = Some(thinking.clone());
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

    /// Stream using the Chat Completions API.
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
                reasoning_content: None,
            });
        }
        messages.extend(Self::convert_messages(&request.messages));

        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(tools) => {
                let mut normalized = crate::tools::normalize_tools_for_openai(&tools);
                // Responses API: enable strict mode so OpenAI normalizes schemas
                // (adds additionalProperties: false, marks all fields required)
                for tool in &mut normalized {
                    if let Some(func) = tool.get_mut("function") {
                        func.as_object_mut()
                            .map(|obj| obj.insert("strict".to_string(), serde_json::json!(true)));
                    }
                }
                normalized
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
            return Err(crate::openai_compatible::map_http_error(
                status,
                error_text,
                &headers,
                "OpenAI",
                "OPENAI_API_KEY",
            ));
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

            let events = streaming::parse_sse_lines_stream_events(&complete_lines);

            futures::stream::iter(events)
        });

        Ok(Box::pin(sse_stream))
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
