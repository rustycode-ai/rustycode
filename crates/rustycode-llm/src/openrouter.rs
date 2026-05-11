//! OpenRouter LLM provider implementation.
//!
//! OpenRouter provides unified access to multiple LLM providers through a single API.

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use crate::auth::AuthMethod;
use crate::provider::{
    ApiMode, CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError,
    StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::route::Route;
use crate::route_selection::RouteSelection;
use crate::transport::HttpTransport;
use crate::wire::openai_chat::OpenAIChatProtocol;
use crate::wire::openai_responses::OpenAIResponsesProtocol;

/// OpenRouter LLM provider
pub struct OpenRouterProvider {
    config: ProviderConfig,
    chat_route: Route,
    responses_route: Route,
    #[allow(dead_code)]
    route_selection: RouteSelection,
    #[allow(dead_code)]
    selection_counter: AtomicUsize,
    /// Cached result of Responses API availability probe.
    responses_api_supported: Arc<std::sync::Mutex<Option<bool>>>,
}

impl OpenRouterProvider {
    pub fn new(config: ProviderConfig, _default_model: String) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;
        Self::new_internal(config)
    }

    pub fn new_without_validation(
        config: ProviderConfig,
        _default_model: String,
    ) -> Result<Self, ProviderError> {
        Self::new_internal(config)
    }

    fn new_internal(config: ProviderConfig) -> Result<Self, ProviderError> {
        let endpoint = config
            .base_url
            .as_deref()
            .unwrap_or("https://openrouter.ai/api/v1")
            .trim_end_matches('/')
            .to_string();

        // OpenRouter requires these headers
        let extra_headers = vec![
            (
                "HTTP-Referer".to_string(),
                "https://rustycode.ai".to_string(),
            ),
            ("X-Title".to_string(), "RustyCode".to_string()),
        ];

        // Resolve Auth (using AuthResolver logic but simplified here for the migration)
        // In a real implementation, we might want a factory that takes ProviderConfig.
        let auth: Box<dyn AuthMethod> = Box::new(crate::auth::bearer::BearerAuth::new(
            config
                .api_key
                .clone()
                .ok_or_else(|| ProviderError::auth("Missing API key"))?,
        ));

        let chat_route = Route::new(
            format!("{}/chat/completions", endpoint),
            Box::new(OpenAIChatProtocol),
            Box::new(
                HttpTransport::new(120).map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            auth.clone_box(),
        )
        .with_name("openrouter-chat")
        .with_extra_headers(extra_headers.clone());

        let responses_route = Route::new(
            format!("{}/responses", endpoint),
            Box::new(OpenAIResponsesProtocol {
                last_response_id: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }),
            Box::new(
                HttpTransport::new(120).map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            auth,
        )
        .with_name("openrouter-responses")
        .with_extra_headers(extra_headers);

        Ok(Self {
            config,
            chat_route,
            responses_route,
            route_selection: RouteSelection::First,
            selection_counter: AtomicUsize::new(0),
            responses_api_supported: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "openrouter".to_string(),
            display_name: "OpenRouter".to_string(),
            description: "Unified API for multiple LLM providers including free models".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    description: "Your OpenRouter API key from openrouter.ai/keys".to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("sk-or-...".to_string()),
                    default: None,
                    validation_pattern: Some("^sk-or-.*".to_string()),
                    validation_error: Some("API key must start with 'sk-or-'".to_string()),
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "Custom API endpoint (defaults to OpenRouter)".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some("https://openrouter.ai/api/v1".to_string()),
                    default: Some("https://openrouter.ai/api/v1".to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("api_key".to_string(), "OPENROUTER_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant...".to_string(),
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
                max_tools_per_call: Some(128),
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![ModelInfo {
                model_id: "google/gemma-2-9b:free".to_string(),
                display_name: "Gemma 2 9B (Free)".to_string(),
                description: "Free model with good quality and speed".to_string(),
                context_window: 8192,
                supports_tools: false,
                use_cases: vec!["General tasks".to_string()],
                cost_tier: 0,
            }],
            model_behavior_profiles: std::collections::HashMap::new(),
        }
    }

    fn is_responses_unsupported_error(err: &ProviderError) -> bool {
        matches!(err, ProviderError::InvalidModel(_))
            || matches!(err, ProviderError::Api(msg) if msg.contains("404"))
    }
}

#[async_trait]
impl LLMProvider for OpenRouterProvider {
    fn name(&self) -> &'static str {
        "openrouter"
    }

    async fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "google/gemma-2-9b:free".into(),
            "openai/gpt-4o".into(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        match request.api_mode {
            Some(ApiMode::Responses) => self
                .responses_route
                .execute(&request, None)
                .await
                .map_err(|e| ProviderError::api(e.to_string())),
            Some(ApiMode::Auto) => {
                let cached = self.responses_api_supported.lock().ok().and_then(|g| *g);
                if cached != Some(false) {
                    match self.responses_route.execute(&request, None).await {
                        Ok(resp) => {
                            if let Ok(mut g) = self.responses_api_supported.lock() {
                                *g = Some(true);
                            }
                            return Ok(resp);
                        }
                        Err(e) => {
                            let prov_err = ProviderError::api(e.to_string());
                            if Self::is_responses_unsupported_error(&prov_err) {
                                tracing::info!("Responses API unavailable on OpenRouter, falling back to Chat Completions");
                                if let Ok(mut g) = self.responses_api_supported.lock() {
                                    *g = Some(false);
                                }
                                // Fall through to Chat Completions below
                            } else {
                                return Err(prov_err);
                            }
                        }
                    }
                }
                self.chat_route
                    .execute(&request, None)
                    .await
                    .map_err(|e| ProviderError::api(e.to_string()))
            }
            _ => self
                .chat_route
                .execute(&request, None)
                .await
                .map_err(|e| ProviderError::api(e.to_string())),
        }
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let route = match request.api_mode {
            Some(ApiMode::Responses) => &self.responses_route,
            _ => &self.chat_route,
        };

        let stream = route
            .execute_stream(&request, None)
            .await
            .map_err(|e| ProviderError::api(e.to_string()))?;

        // Map Result<StreamEvent> to StreamChunk
        let chunk_stream = stream.map(|res| res.map_err(|e| ProviderError::api(e.to_string())));

        Ok(Box::pin(chunk_stream))
    }
}
