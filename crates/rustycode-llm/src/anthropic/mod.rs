//! Anthropic Claude LLM provider implementation.

pub(crate) mod helpers;
pub(crate) mod streaming;
#[cfg(test)]
mod tests;
pub(crate) mod types;

use crate::advisor::{AdvisorConfig, AdvisorTool};
use crate::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, MessageRole, ProviderConfig,
    ProviderError, StreamChunk, Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::tool_annotations::anthropic_annotations_for_tool_info;
use async_trait::async_trait;
use futures::Stream;
use rustycode_tools_api::{Tool, ToolMetadataProvider, ToolProfile, ToolRegistry, ToolSelector};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use helpers::{
    apply_cache_to_last_messages, map_anthropic_error, map_anthropic_structured_error,
    normalize_thinking_for_model, parse_tool_result_content,
};
use types::{
    AnthropicMessage, AnthropicRequest, AnthropicResponse, ContentBlock, ImageSource,
    SystemContentBlock, SystemPrompt, ToolResultContent,
};
// Re-export types used by integration tests and examples
pub use types::{
    AnthropicRequestContent, CacheControl, CitationMetadata, SearchResultBlock, SearchResultContent,
};

/// Anthropic Claude LLM provider
pub struct AnthropicProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    model: String,
    tool_registry: Arc<ToolRegistry>,
    tool_selector: ToolSelector,
    /// Optional advisor tool configuration for executor+advisor pattern
    advisor_config: Option<AdvisorConfig>,
}

impl AnthropicProvider {
    /// Internal implementation of complete without retry logic
    pub async fn complete_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let model = &request.model;
        let url = self.endpoint();
        tracing::info!("Anthropic API request to: {} (model: {})", url, model);

        // Convert ChatMessage to AnthropicMessage
        let parsed_messages = self.parse_conversation_messages(&request.messages);

        // Filter out empty or invalid messages that could cause error 1214
        let mut messages: Vec<AnthropicMessage> = parsed_messages
            .into_iter()
            .filter(|msg| {
                // Filter out messages with empty content
                match &msg.content {
                    AnthropicRequestContent::Text(text) => !text.trim().is_empty(),
                    AnthropicRequestContent::Blocks(blocks) => !blocks.is_empty(),
                }
            })
            .collect();

        // Ensure we have at least one valid message
        if messages.is_empty() {
            return Err(ProviderError::Api(
                "No valid messages to send to Anthropic API after filtering".to_string(),
            ));
        }

        // Use intelligent tool selection if tools not explicitly provided
        let mut tools = match request.tools {
            Some(tools) => tools, // Use explicitly provided tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
                    .unwrap_or_default()
            }
        };

        // Inject advisor tool if configured via builder or RUSTYCODE_ADVISOR_MODEL env var
        let advisor_tool = self
            .advisor_config
            .as_ref()
            .map(|c| c.advisor.to_anthropic_tool())
            .or_else(|| {
                std::env::var("RUSTYCODE_ADVISOR_MODEL").ok().map(|model| {
                    let advisor = AdvisorTool::new(model);
                    tracing::info!(
                        "Advisor tool auto-enabled via RUSTYCODE_ADVISOR_MODEL={}",
                        advisor.advisor_model
                    );
                    advisor.to_anthropic_tool()
                })
            });

        if let Some(tool) = &advisor_tool {
            tools.push(tool.clone());
        }

        let has_advisor = advisor_tool.is_some();

        let wants_structured_output = request
            .output_config
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_some_and(|f| {
                matches!(f.format_type, crate::provider::OutputFormatType::JsonSchema)
            });

        // Enable prompt caching for all Anthropic-compatible endpoints.
        // Most proxies (OpenRouter, z.ai, etc.) support cache_control correctly.
        let use_cache_control = true;

        // Apply cache_control to last 2 conversation messages.
        // Combined with system prompt (1) and tools (1), this uses all 4 allowed breakpoints.
        if use_cache_control {
            apply_cache_to_last_messages(&mut messages, 2);
        }

        let system = request.system_prompt.map(|text| {
            if use_cache_control {
                SystemPrompt::Blocks(vec![SystemContentBlock {
                    block_type: "text",
                    text,
                    cache_control: Some(CacheControl {
                        cache_type: "ephemeral",
                    }),
                }])
            } else {
                SystemPrompt::Text(text)
            }
        });

        // Replace prompt-based deferred stubs with native Anthropic defer_loading.
        // Stub tools are identified by the "[DEFERRED:" marker in their description.
        let has_deferred = !tools.is_empty()
            && tools.iter().any(|t| {
                t.get("description")
                    .and_then(|d| d.as_str())
                    .is_some_and(|d| d.contains("[DEFERRED:"))
            });

        if has_deferred {
            for tool in &mut tools {
                let is_deferred = tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .is_some_and(|d| d.contains("[DEFERRED:"));
                if is_deferred {
                    if let Some(name) = tool
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                    {
                        *tool = serde_json::json!({
                            "name": name,
                            "defer_loading": true
                        });
                    }
                }
            }
        }

        let tools = if !tools.is_empty() {
            if use_cache_control {
                if let Some(last_tool) = tools.last_mut().and_then(|t| t.as_object_mut()) {
                    // Don't add cache_control to deferred-only tools (no description key)
                    if last_tool.contains_key("description")
                        || last_tool.contains_key("input_schema")
                    {
                        last_tool.insert(
                            "cache_control".to_string(),
                            serde_json::json!({"type": "ephemeral"}),
                        );
                    }
                }
            }
            Some(tools)
        } else {
            None
        };

        let anthropic_request = AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system,
            stream: Some(false),
            tools,
            thinking: normalize_thinking_for_model(
                request.thinking,
                request.output_config.as_ref().and_then(|c| c.effort),
                &request.model,
            ),
            output_config: request.output_config,
            container: request.container.clone(),
            tool_choice: request.tool_choice.as_ref().map(|tc| match tc.as_str() {
                Some("required") => serde_json::json!({"type": "any"}),
                Some("auto") => serde_json::json!({"type": "auto"}),
                Some("none") => serde_json::json!({"type": "none"}),
                _ => tc.clone(),
            }),
            parallel_tool_calls: request.parallel_tool_calls,
        };

        tracing::info!("Sending request with model: {}", request.model);

        // Dump raw request/response to files for debugging
        let http_trace_dir = std::env::var("RTK_HTTP_TRACE_DIR")
            .unwrap_or_else(|_| "/tmp/rtk-http-trace".to_string());
        let _ = std::fs::create_dir_all(&http_trace_dir);
        let trace_seq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        if let Ok(body) = serde_json::to_string_pretty(&anthropic_request) {
            let path = format!("{http_trace_dir}/{trace_seq}_req.json");
            let _ = std::fs::write(&path, &body);
            tracing::info!("[http-trace] Request written to {path}");
        }

        // Build the request, adding beta headers for advisor and skills
        let mut request_builder = self.client.post(&url).json(&anthropic_request);

        if has_advisor {
            request_builder = request_builder.header("anthropic-beta", AdvisorTool::beta_header());
        }

        if request.container.is_some() {
            for header in crate::tools::anthropic_skills_beta_headers() {
                request_builder = request_builder.header("anthropic-beta", header);
            }
        }

        if has_deferred {
            request_builder = request_builder.header("anthropic-beta", "prompt-caching-2024-07-31");
        }

        let response = request_builder.send().await.map_err(|e| {
            ProviderError::Network(format!("failed to send request (model: {}): {}", model, e))
        })?;

        tracing::info!("Response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            tracing::error!(
                "API error {} (model: {}): {}",
                status,
                self.model,
                error_text
            );

            if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(error_obj) = error_json.get("error").and_then(|e| e.as_object()) {
                    let error_type = error_obj
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown_error");

                    let message = error_obj
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or(&error_text);

                    let param = error_obj.get("param").and_then(|p| p.as_str());

                    return Err(map_anthropic_structured_error(
                        status, error_type, message, param, &headers,
                    ));
                }
            }

            return Err(map_anthropic_error(status, &error_text, &headers));
        }

        let response_text = response.text().await.map_err(|e| {
            ProviderError::Network(format!(
                "failed to read response body (model: {}): {}",
                model, e
            ))
        })?;

        // Dump raw response
        {
            let path = format!("{http_trace_dir}/{trace_seq}_resp.json");
            let _ = std::fs::write(&path, &response_text);
            tracing::info!("[http-trace] Response written to {path}");
        }

        let anthropic_response: AnthropicResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                ProviderError::Serialization(format!(
                    "failed to parse response (model: {}): {}",
                    model, e
                ))
            })?;

        // Extract text, tool_use, and refusal content blocks
        let mut content_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut all_citations: Vec<crate::provider::Citation> = Vec::new();
        let mut thinking_blocks: Vec<crate::provider::ThinkingBlock> = Vec::new();
        let mut refused = false;

        for block in &anthropic_response.content {
            if block.content_type == "json" {
                // Structured output responses use type "json" instead of "text"
                content_parts.push(block.text.clone());
            } else if block.content_type == "text" && !block.text.is_empty() {
                content_parts.push(block.text.clone());

                // Extract citations from text blocks (web search results)
                if let Some(ref citations) = block.citations {
                    for c in citations {
                        all_citations.push(crate::provider::Citation {
                            source: c.url.clone(),
                            title: c.title.clone(),
                            cited_text: c.cited_text.clone(),
                            index: c.search_result_index.unwrap_or(0),
                        });
                    }
                }
            } else if block.content_type == "tool_use" {
                // Extract tool_use blocks and serialize them
                let tool_call = serde_json::json!({
                    "id": block.id,
                    "name": block.name,
                    "arguments": block.input
                });
                tool_calls.push(tool_call);
            } else if block.content_type == "tool_reference" {
                // Native deferred tool resolution: the model wants to use a
                // deferred tool. Resolve by looking up the full schema from the
                // registry so downstream code can treat this as a normal tool_use.
                let tool_name = &block.name;
                let resolved = self.tool_registry.tool_info(tool_name).map(|info| {
                    serde_json::json!({
                        "id": block.id,
                        "name": tool_name,
                        "arguments": info.parameters_schema
                    })
                });
                if let Some(tool_call) = resolved {
                    tracing::debug!("Resolved tool_reference '{}' to tool_use", tool_name);
                    tool_calls.push(tool_call);
                } else {
                    tracing::warn!(
                        "tool_reference '{}' could not be resolved from registry",
                        tool_name
                    );
                }
            } else if block.content_type == "thinking" {
                tracing::debug!("Received thinking block ({} chars)", block.thinking.len());
                thinking_blocks.push(crate::provider::ThinkingBlock {
                    block_type: "thinking".to_string(),
                    thinking: block.thinking.clone(),
                    signature: block.signature.clone(),
                    data: String::new(),
                    display: None,
                });
            } else if block.content_type == "redacted_thinking" {
                tracing::debug!(
                    "Received redacted_thinking block ({} bytes)",
                    block.data.len()
                );
                thinking_blocks.push(crate::provider::ThinkingBlock {
                    block_type: "redacted_thinking".to_string(),
                    thinking: String::new(),
                    signature: String::new(),
                    data: block.data.clone(),
                    display: None,
                });
            } else if block.content_type == "refusal" {
                // Handle refusal content blocks (Claude 4 models)
                refused = true;
                if !block.text.is_empty() {
                    tracing::warn!("Model refused: {}", block.text);
                    content_parts.push(format!("[REFUSAL] {}", block.text));
                } else {
                    tracing::warn!("Model refused (no reason provided)");
                    content_parts.push("[REFUSAL]".to_string());
                }
            }
        }

        // Append tool calls as JSON to the content for auto_tool_parser to find
        if !tool_calls.is_empty() {
            let tool_calls_json =
                serde_json::to_string_pretty(&tool_calls).unwrap_or_else(|_| "[]".to_string());
            content_parts.push(format!("```tool\n{}\n```", tool_calls_json));
        }

        // Build usage with cache information if available
        let input_tokens: u32 = anthropic_response
            .usage
            .input_tokens
            .try_into()
            .unwrap_or_else(|_| {
                tracing::warn!("input_tokens overflow, clamping to u32::MAX");
                u32::MAX
            });
        let output_tokens: u32 = anthropic_response
            .usage
            .output_tokens
            .try_into()
            .unwrap_or_else(|_| {
                tracing::warn!("output_tokens overflow, clamping to u32::MAX");
                u32::MAX
            });
        // Zero out cache fields when we didn't request caching —
        // proxies may return fake values that produce nonsensical stats.
        let (cache_read, cache_creation) = if use_cache_control {
            let r: u32 = anthropic_response
                .usage
                .cache_read_input_tokens
                .try_into()
                .unwrap_or_else(|_| {
                    tracing::warn!("cache_read_input_tokens overflow, clamping to u32::MAX");
                    u32::MAX
                });
            let c: u32 = anthropic_response
                .usage
                .cache_creation_input_tokens
                .try_into()
                .unwrap_or_else(|_| {
                    tracing::warn!("cache_creation_input_tokens overflow, clamping to u32::MAX");
                    u32::MAX
                });
            (r, c)
        } else {
            (0, 0)
        };

        let usage = if cache_read > 0 || cache_creation > 0 {
            tracing::info!(
                "Cache: {cache_read} read, {cache_creation} written, input={input_tokens}, output={output_tokens}"
            );
            Usage::with_cache(input_tokens, output_tokens, cache_read, cache_creation)
        } else {
            Usage::new(input_tokens, output_tokens)
        };

        // Extract structured output when output_config.format was JsonSchema
        let structured_output = if wants_structured_output {
            let content_str = content_parts.join("\n");
            match serde_json::from_str::<serde_json::Value>(&content_str) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Structured output JSON parse failed: {e}");
                    None
                }
            }
        } else {
            None
        };

        Ok(CompletionResponse {
            content: content_parts.join("\n"),
            model: anthropic_response.model,
            usage: Some(usage),
            stop_reason: anthropic_response.stop_reason.or_else(|| {
                // Infer stop_reason from content if not explicitly provided
                if refused {
                    Some("refusal".to_string())
                } else if !tool_calls.is_empty() {
                    Some("tool_use".to_string())
                } else {
                    Some("end_turn".to_string())
                }
            }),
            citations: if all_citations.is_empty() {
                None
            } else {
                Some(all_citations)
            },
            thinking_blocks: if thinking_blocks.is_empty() {
                None
            } else {
                Some(thinking_blocks)
            },
            structured_output,
        })
    }

    pub fn new(config: ProviderConfig, model: String) -> Result<Self, ProviderError> {
        // Strict constructor: require API key to be present and non-empty
        if config
            .api_key
            .as_ref()
            .is_none_or(|k| k.expose_secret().trim().is_empty())
        {
            return Err(ProviderError::Configuration(
                "Anthropic API key is required. Set api_key in config or ANTHROPIC_API_KEY env var"
                    .to_string(),
            ));
        }

        // Delegate to the non-strict constructor for common setup
        Self::new_without_validation(config, model)
    }

    /// Create provider without config validation (for custom endpoints/proxies)
    pub fn new_without_validation(
        config: ProviderConfig,
        model: String,
    ) -> Result<Self, ProviderError> {
        // Non-strict constructor: allow missing API key (used for validation-free creation)
        let timeout = config.timeout_seconds.unwrap_or(300);

        // Build headers conditionally with API key if present and non-empty
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key_secret) = config.api_key.as_ref() {
            let key = key_secret.expose_secret();
            if !key.trim().is_empty() {
                headers.insert(
                    reqwest::header::HeaderName::from_static("x-api-key"),
                    key.parse().map_err(|e| {
                        ProviderError::Configuration(format!("invalid API key format: {}", e))
                    })?,
                );
            }
        }

        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("anthropic-version"),
            reqwest::header::HeaderValue::from_static("2023-06-01"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout))
            .connect_timeout(Duration::from_secs(30))
            .tcp_keepalive({
                #[allow(clippy::duration_suboptimal_units)]
                Duration::from_secs(60)
            })
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| ProviderError::Network(format!("failed to build HTTP client: {}", e)))?;

        // Initialize tool registry with built-in tools and selector
        let tool_registry = Arc::new(ToolRegistry::new());
        let tool_selector = ToolSelector::new();

        Ok(Self {
            config,
            client,
            model,
            tool_registry,
            tool_selector,
            advisor_config: None,
        })
    }

    /// Enable the advisor pattern for this provider.
    ///
    /// When enabled, requests will include the advisor tool, allowing the
    /// executor model to consult a more capable advisor model for guidance.
    /// The advisor tool is sent as a special `type: "advisor_20260301"` tool
    /// with the required `anthropic-beta` header.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use rustycode_llm::{AnthropicProvider, AdvisorConfig, ProviderConfig};
    ///
    /// let config = ProviderConfig { ... };
    /// let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".into())?
    ///     .with_advisor(AdvisorConfig::new());
    /// ```
    pub fn with_advisor(mut self, config: AdvisorConfig) -> Self {
        self.advisor_config = Some(config);
        self
    }

    fn endpoint(&self) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com");

        let endpoint = format!("{}/v1/messages", base.trim_end_matches('/'));

        tracing::debug!(
            "Anthropic endpoint constructed: base_url={}, full_endpoint={}",
            base,
            endpoint
        );

        endpoint
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

            // Get ranked tools for this profile using tag-based discovery
            let selector = self.tool_selector.clone().with_profile(profile);
            let tool_names = selector.select_tools(&self.tool_registry);

            // Format tools for Anthropic API
            Some(self.format_tools_for_anthropic(&tool_names))
        } else {
            // No user message found, return all tools (or none if preferred)
            None
        }
    }

    /// Format tool definitions for Anthropic API
    fn format_tools_for_anthropic(&self, tool_names: &[String]) -> Vec<serde_json::Value> {
        tool_names
            .iter()
            .filter_map(|name| {
                self.tool_registry
                    .get(name)
                    .map(|tool| self.tool_to_anthropic_format(tool))
            })
            .collect()
    }

    /// Convert a tool to Anthropic's tool format
    fn tool_to_anthropic_format(&self, tool: &dyn Tool) -> serde_json::Value {
        let schema = tool.parameters_schema();
        let mut tool_json = serde_json::json!({
            "name": tool.name(),
            "description": tool.description(),
            "input_schema": schema
        });
        if let Some(annotations) = anthropic_annotations_for_tool_info(
            tool.name(),
            matches!(tool.permission(), rustycode_tools_api::ToolPermission::Read),
        ) {
            tool_json["annotations"] = annotations;
        }
        tool_json
    }

    /// Parse conversation string into individual messages
    /// Input format: "role: content\n\nrole: content\n\n..."
    /// Output: Vec of AnthropicMessage with proper roles
    pub(crate) fn parse_conversation_messages(
        &self,
        messages: &[ChatMessage],
    ) -> Vec<AnthropicMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match &msg.role {
                    MessageRole::System => "user",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool(_) => "user",
                };

                // Handle MessageContent::Blocks with protocol-level ContentBlock variants
                // (used by headless mode and other code paths that construct messages directly)
                if let rustycode_protocol::MessageContent::Blocks(blocks) = &msg.content {
                    let anthropic_blocks: Vec<ContentBlock> = blocks
                        .iter()
                        .map(|b| match b {
                            rustycode_protocol::ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => ContentBlock::ToolResult {
                                content_type: "tool_result",
                                tool_use_id: tool_use_id.clone(),
                                content: ToolResultContent::Text(content.clone()),
                                is_error: if *is_error { Some(true) } else { None },
                                cache_control: None,
                            },
                            rustycode_protocol::ContentBlock::ToolUse { id, name, input } => {
                                ContentBlock::ToolUse {
                                    content_type: "tool_use",
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                    cache_control: None,
                                }
                            }
                            rustycode_protocol::ContentBlock::Text { text, .. } => {
                                ContentBlock::Text {
                                    content_type: "text",
                                    text: text.clone(),
                                    cache_control: None,
                                }
                            }
                            rustycode_protocol::ContentBlock::Image { source, .. } => {
                                let (source_type, media_type, data) =
                                    if source.source_type == "file" {
                                        match crate::provider::resolve_image_to_base64(source) {
                                            Some((mime, b64)) => ("base64".to_string(), mime, b64),
                                            None => {
                                                tracing::warn!(
                                                    "Skipping image with unreadable file source"
                                                );
                                                (
                                                    source.source_type.clone(),
                                                    source.media_type.clone(),
                                                    source.data.clone(),
                                                )
                                            }
                                        }
                                    } else {
                                        (
                                            source.source_type.clone(),
                                            source.media_type.clone(),
                                            source.data.clone(),
                                        )
                                    };
                                ContentBlock::Image {
                                    content_type: "image",
                                    source: ImageSource {
                                        source_type,
                                        media_type,
                                        data,
                                    },
                                }
                            }
                            rustycode_protocol::ContentBlock::Thinking {
                                thinking,
                                signature,
                            } => ContentBlock::Thinking {
                                content_type: "thinking",
                                thinking: thinking.clone(),
                                signature: signature.clone(),
                            },
                            rustycode_protocol::ContentBlock::RedactedThinking { data } => {
                                ContentBlock::RedactedThinking {
                                    content_type: "redacted_thinking",
                                    data: data.clone(),
                                }
                            }
                            _ => ContentBlock::Text {
                                content_type: "text",
                                text: "[unsupported block]".to_string(),
                                cache_control: None,
                            },
                        })
                        .collect();

                    if !anthropic_blocks.is_empty() {
                        // Determine the correct role: tool results must be in a user message
                        let effective_role = if anthropic_blocks
                            .iter()
                            .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
                        {
                            "user"
                        } else {
                            role
                        };
                        return AnthropicMessage {
                            role: effective_role,
                            content: AnthropicRequestContent::Blocks(anthropic_blocks),
                        };
                    }
                }

                // Check if this is a tool result message (JSON format)
                if let Ok(tool_result_json) =
                    serde_json::from_str::<serde_json::Value>(&msg.content.as_text())
                {
                    if tool_result_json["type"] == "tool_result" {
                        // This is a properly formatted tool result
                        let is_error = tool_result_json
                            .get("is_error")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        return AnthropicMessage {
                            role: "user",
                            content: AnthropicRequestContent::Blocks(vec![
                                ContentBlock::ToolResult {
                                    content_type: "tool_result",
                                    tool_use_id: tool_result_json["tool_use_id"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    content: parse_tool_result_content(
                                        &tool_result_json["content"],
                                    ),
                                    is_error: if is_error { Some(true) } else { None },
                                    cache_control: None,
                                },
                            ]),
                        };
                    }

                    // Check if this is a search_result block
                    if tool_result_json["type"] == "search_result" {
                        let content_blocks = if let Some(content_array) =
                            tool_result_json.get("content").and_then(|c| c.as_array())
                        {
                            content_array
                                .iter()
                                .filter_map(|block| {
                                    block.get("text").and_then(|t| t.as_str()).map(|text| {
                                        SearchResultContent {
                                            content_type: "text",
                                            text: text.to_string(),
                                        }
                                    })
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };

                        let citations = tool_result_json.get("citations").and_then(|c| {
                            c.get("enabled")
                                .and_then(|e| e.as_bool())
                                .map(|enabled| CitationMetadata { enabled })
                        });

                        return AnthropicMessage {
                            role: "user",
                            content: AnthropicRequestContent::Blocks(vec![
                                ContentBlock::SearchResult {
                                    content_type: "search_result",
                                    source: tool_result_json["source"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    title: tool_result_json["title"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    content: content_blocks,
                                    citations,
                                    cache_control: None,
                                },
                            ]),
                        };
                    }

                    // Check if this is an array of content blocks (mixed content)
                    if let Ok(content_array) =
                        serde_json::from_str::<Vec<serde_json::Value>>(&msg.content.as_text())
                    {
                        let mut blocks = Vec::new();
                        for item in content_array {
                            if let Some(content_type) = item.get("type").and_then(|t| t.as_str()) {
                                match content_type {
                                    "search_result" => {
                                        let content_blocks = if let Some(content_array) =
                                            item.get("content").and_then(|c| c.as_array())
                                        {
                                            content_array
                                                .iter()
                                                .filter_map(|block| {
                                                    block.get("text").and_then(|t| t.as_str()).map(
                                                        |text| SearchResultContent {
                                                            content_type: "text",
                                                            text: text.to_string(),
                                                        },
                                                    )
                                                })
                                                .collect()
                                        } else {
                                            Vec::new()
                                        };

                                        let citations = item.get("citations").and_then(|c| {
                                            c.get("enabled")
                                                .and_then(|e| e.as_bool())
                                                .map(|enabled| CitationMetadata { enabled })
                                        });

                                        blocks.push(ContentBlock::SearchResult {
                                            content_type: "search_result",
                                            source: item
                                                .get("source")
                                                .and_then(|s| s.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            title: item
                                                .get("title")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            content: content_blocks,
                                            citations,
                                            cache_control: None,
                                        });
                                    }
                                    "text" => {
                                        if let Some(text) =
                                            item.get("text").and_then(|t| t.as_str())
                                        {
                                            blocks.push(ContentBlock::Text {
                                                content_type: "text",
                                                text: text.to_string(),
                                                cache_control: None,
                                            });
                                        }
                                    }
                                    _ => {
                                        // For other content types, just add as text
                                        if let Some(text) =
                                            item.get("text").and_then(|t| t.as_str())
                                        {
                                            blocks.push(ContentBlock::Text {
                                                content_type: "text",
                                                text: text.to_string(),
                                                cache_control: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        if !blocks.is_empty() {
                            return AnthropicMessage {
                                role,
                                content: AnthropicRequestContent::Blocks(blocks),
                            };
                        }
                    }
                }

                // Regular text message
                AnthropicMessage {
                    role,
                    content: AnthropicRequestContent::Text(msg.content.to_text()),
                }
            })
            .collect()
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "anthropic".to_string(),
            display_name: "Anthropic".to_string(),
            description: "Claude models with strong reasoning and analysis capabilities".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Anthropic API key from console.anthropic.com".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("sk-ant-...".to_string()),
                        default: None,
                        validation_pattern: Some("^sk-ant-.*".to_string()),
                        validation_error: Some("API key must start with 'sk-ant-'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (for proxy or compatible services)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.anthropic.com".to_string()),
                        default: Some("https://api.anthropic.com".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "ANTHROPIC_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are RustyCode, a coding assistant.\n\n{context}\n\n## Claude Guidance\n- Use XML tags for structured output (<analysis>, <plan>, <implementation>)\n- Clear, direct instructions work better than verbose ones\n- Handle complex multi-step instructions in a single prompt when possible".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: true,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Use <thinking> blocks for internal reasoning on complex problems.".to_string(),
                        "Keep instructions concise — Claude attends more to shorter prompts.".to_string(),
                    ],
                },
                tool_format: ToolFormat::AnthropicXML,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: false,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "claude-sonnet-4-6".to_string(),
                    display_name: "Claude Sonnet 4.6".to_string(),
                    description: "Best coding model with extended thinking".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec!["Code generation".to_string(), "Development".to_string(), "Analysis".to_string()],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "claude-opus-4-6".to_string(),
                    display_name: "Claude Opus 4.6".to_string(),
                    description: "Deepest reasoning with extended thinking".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Architecture".to_string(), "Research".to_string()],
                    cost_tier: 5,
                },
                ModelInfo {
                    model_id: "claude-haiku-4-5-20251001".to_string(),
                    display_name: "Claude Haiku 4.5".to_string(),
                    description: "Fast and cost-efficient for lightweight tasks".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec!["Quick responses".to_string(), "Classification".to_string(), "Agent workers".to_string()],
                    cost_tier: 1,
                },
            ],
        }
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn is_available(&self) -> bool {
        // Check if API key is present
        if self
            .config
            .api_key
            .as_ref()
            .map_or(true, |k| k.expose_secret().trim().is_empty())
        {
            return false;
        }

        // Try a simple health check or just verify the client is working
        true
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return known Anthropic models (as of April 2026)
        Ok(vec![
            // Claude 4.6 (latest)
            "claude-opus-4-6".to_string(),
            "claude-sonnet-4-6".to_string(),
            // Claude 4.7
            "claude-opus-4-7-20260401".to_string(),
            // Claude 4.5 (with extended thinking)
            "claude-opus-4-6".to_string(),
            "claude-sonnet-4-6".to_string(),
            // Claude 4.0
            "claude-opus-4-20250214".to_string(),
            "claude-sonnet-4-20250214".to_string(),
            // Claude 3.7
            "claude-3-7-sonnet-20250219".to_string(),
            // Claude 3.5
            "claude-sonnet-4-6".to_string(),
            "claude-haiku-4-5-20251001".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
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
