//! Anthropic Claude LLM provider implementation.

pub(crate) mod helpers;
pub(crate) mod streaming;
#[cfg(test)]
mod tests;
pub(crate) mod types;
pub(crate) use types::ContentBlock;

use crate::advisor::{AdvisorConfig, AdvisorTool};
use crate::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, MessageRole, ProviderConfig,
    ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use async_trait::async_trait;
use futures::Stream;
use rustycode_tools_api::{ToolMetadataProvider, ToolProfile, ToolRegistry, ToolSelector};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

// Re-export types used by integration tests and examples
pub use types::{
    AnthropicRequestContent, CacheControl, CitationMetadata, SearchResultBlock, SearchResultContent,
};

/// Anthropic Claude LLM provider
pub struct AnthropicProvider {
    route: crate::route::Route,
    #[allow(dead_code)] // Kept for metadata
    config: ProviderConfig,
    #[allow(dead_code)] // Kept for metadata
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
        mut request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Use intelligent tool selection if tools not explicitly provided
        let tools = match request.tools {
            Some(_) => None, // Already provided in request.tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
            }
        };

        // Inject advisor tool if configured
        let advisor_tool = self
            .advisor_config
            .as_ref()
            .map(|c| c.advisor.to_anthropic_tool())
            .or_else(|| {
                std::env::var("RUSTYCODE_ADVISOR_MODEL")
                    .ok()
                    .map(|model| AdvisorTool::new(model).to_anthropic_tool())
            });

        if let Some(tool) = advisor_tool {
            if let Some(ref mut t_list) = request.tools {
                t_list.push(tool);
            } else {
                request.tools = Some(vec![tool]);
            }
        }

        // Execute via Route
        let response = self
            .route
            .execute(&request, tools.as_deref())
            .await
            .map_err(|e| ProviderError::Network(format!("route execution failed: {}", e)))?;

        Ok(response)
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
        let tool_registry = Arc::new(ToolRegistry::new());
        let tool_selector = ToolSelector::new();

        let endpoint = {
            let base = config
                .base_url
                .as_deref()
                .unwrap_or("https://api.anthropic.com");
            format!("{}/v1/messages", base.trim_end_matches('/'))
        };

        let route = crate::route::Route::builder()
            .endpoint(endpoint)
            .protocol(Box::new(crate::wire::anthropic::AnthropicProtocol {
                registry: Some(tool_registry.clone()),
                state: Arc::new(std::sync::Mutex::new(Default::default())),
            }))
            .transport(Box::new(
                crate::transport::http::HttpTransport::new(600)
                    .map_err(|e| ProviderError::Network(e.to_string()))?,
            ))
            .auth(Box::new(crate::auth::ApiKeyHeaderAuth::new(
                "x-api-key",
                config
                    .api_key
                    .clone()
                    .unwrap_or_else(|| secrecy::SecretString::new("".into())),
            )))
            .build();

        Ok(Self {
            route,
            config,
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

    #[cfg(test)]
    pub(crate) fn endpoint(&self) -> String {
        self.route.endpoint.clone()
    }

    /// Detect the user's intent from their latest message and select appropriate tools
    fn select_tools_for_prompt(
        &self,
        messages: &[ChatMessage],
    ) -> Option<Vec<crate::schema::tool_schema::ToolSchema>> {
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
    fn format_tools_for_anthropic(
        &self,
        tool_names: &[String],
    ) -> Vec<crate::schema::tool_schema::ToolSchema> {
        tool_names
            .iter()
            .filter_map(|name| {
                self.tool_registry
                    .tool_info(name)
                    .map(|info| crate::schema::tool_schema::ToolSchema::from(&info))
            })
            .collect()
    }
    #[cfg(test)]
    /// Parse conversation string into individual messages
    /// Input format: "role: content\n\nrole: content\n\n..."
    /// Output: Vec of AnthropicMessage with proper roles
    #[cfg(test)]
    pub(crate) fn parse_conversation_messages(
        &self,
        messages: &[crate::provider::ChatMessage],
    ) -> Vec<types::AnthropicMessage> {
        use crate::anthropic::types::{AnthropicRequestContent, ContentBlock, ImageSource};
        use crate::provider::MessageRole;

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
                                content: helpers::parse_tool_result_content(
                                    &serde_json::from_str::<serde_json::Value>(content)
                                        .unwrap_or(serde_json::json!(content)),
                                ),
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
                                            None => (
                                                source.source_type.clone(),
                                                source.media_type.clone(),
                                                source.data.clone(),
                                            ),
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
                        let effective_role = if anthropic_blocks
                            .iter()
                            .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
                        {
                            "user"
                        } else {
                            role
                        };
                        return types::AnthropicMessage {
                            role: effective_role,
                            content: AnthropicRequestContent::Blocks(anthropic_blocks),
                        };
                    }
                }

                // Regular text message
                types::AnthropicMessage {
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
                    model_behavior_profiles: HashMap::new(),
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
            .is_none_or(|k| k.expose_secret().trim().is_empty())
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
