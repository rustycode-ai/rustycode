#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cloned_instead_of_copied,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::if_not_else,
    clippy::ignored_unit_patterns,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_midpoint,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::non_std_lazy_statics,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::significant_drop_tightening,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_literal_bound,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self
)]
#![cfg_attr(test, allow(clippy::float_cmp, clippy::similar_names,))]
//! # rustycode-llm
//!
//! Multi-provider LLM client library supporting 18+ AI providers.
//!
//! ## Features
//!
//! - **Multi-Provider Support**: Anthropic, OpenAI, Google Gemini, Azure OpenAI, AWS Bedrock, Ollama, and more
//! - **Streaming**: Real-time response streaming with SSE support
//! - **Tool Calling**: Structured function/tool calling across providers
//! - **Extended Thinking**: Claude's reasoning/thinking features
//! - **Token Tracking**: Monitor token usage and costs per request
//! - **Provider Metadata**: Dynamic model discovery and capability querying
//! - **Failover**: Graceful degradation and circuit breaker patterns
//! - **Cost Tracking**: Track and optimize API costs
//!
//! ## Migration Guides
//!
//! ### From Old Thinking API to New ThinkingConfig
//!
//! ```ignore
//! // OLD (deprecated)
//! let request = CompletionRequest::new(model, messages)
//!     .with_extended_thinking(true)
//!     .with_thinking_budget(10000);
//!
//! // NEW (recommended)
//! let request = CompletionRequest::new(model, messages)
//!     .with_thinking_config(ThinkingConfig::enabled(10000));
//! ```
//!
//! ### From Old Effort to New OutputConfig
//!
//! ```ignore
//! // OLD (deprecated)
//! let request = CompletionRequest::new(model, messages)
//!     .with_effort(EffortLevel::High);
//!
//! // NEW (recommended)
//! let request = CompletionRequest::new(model, messages)
//!     .with_output_config(OutputConfig::with_effort(EffortLevel::High));
//! ```

//! ### Using Thinking Configuration
//!
//! ```ignore
//! let request = CompletionRequest::new(model, messages)
//!     .with_thinking_config(ThinkingConfig::enabled(10000));
//! ```
//!
//! ### Using Output Configuration
//!
//! ```ignore
//! let request = CompletionRequest::new(model, messages)
//!     .with_output_config(OutputConfig::with_effort(EffortLevel::High));
//! ```

pub mod advisor;
pub mod anthropic;
pub mod anthropic_advanced_tools;
pub mod anthropic_streaming;
pub mod azure;
pub mod bedrock;
pub mod caching;
pub mod circuit_breaker;
pub mod client_pool;
pub mod cohere;
pub mod compaction;
pub mod conversation;
pub mod copilot;
pub mod cost_tracker;
pub mod degradation_status;
pub mod download_manager;
pub mod error_recovery;
pub mod gemini;
pub mod graceful_degradation;
pub mod huggingface;
#[cfg(feature = "litert")]
pub mod litert_lm;
pub mod mistral;
pub mod mock;
pub mod model_info;
pub mod offline_mode;
pub mod ollama;
pub mod openai;
pub mod openai_compatible;
pub mod openrouter;
pub mod perplexity;
pub mod provider;
pub mod provider_error_policy;
pub mod provider_fallback;
pub mod provider_helpers;
pub mod provider_metadata;
pub mod rate_limiter;
pub mod registry;
pub mod replay_provider;
pub mod retry;
pub mod singleton_provider;
pub mod sse;
pub mod timeout_handler;
pub mod together;
pub mod token_tracker;
pub mod tool_annotations;
pub mod tool_executor;
pub mod tool_selection_helper;
pub mod tools;
pub mod usage_estimator;
pub mod utils;
pub mod zhipu;

#[cfg(test)]
mod cross_provider_tests;

use anyhow::Result;
use secrecy::SecretString;

// Use shared config parsing utilities from rustycode-config
use rustycode_config::{api_key_env_name, default_model_for_provider};

pub use advisor::{AdvisorConfig, AdvisorResponse, AdvisorTool};
pub use anthropic::AnthropicProvider;
pub use azure::AzureProvider;
pub use bedrock::BedrockProvider;
pub use client_pool::{global_client, global_pool, ClientPool, ClientPoolConfig, PoolStats};
pub use cohere::CohereProvider;
pub use conversation::ConversationManager;
pub use copilot::CopilotProvider;
pub use degradation_status::{
    DegradationReport, OperationStatus, RecoveryGuidance, StatusIndicator,
};
pub use error_recovery::{
    classify_error, default_strategy, with_recovery, ErrorKind, RecoveryStrategy,
};
pub use gemini::GeminiProvider;
pub use graceful_degradation::{
    DegradationHandler, DegradationMetadata, DegradationMetadataBuilder, ErrorClassifier,
    ErrorKind as DegradationErrorKind, ErrorSeverity, PartialResult,
    RetryConfig as DegradationRetryConfig,
};
pub use huggingface::HuggingFaceProvider;
#[cfg(feature = "litert")]
pub use litert_lm::LiteRtLmProvider;
pub use mistral::MistralProvider;
pub use offline_mode::{
    CodeMetadata, LocalCodeAnalysisResult, LocalCodeAnalyzer, LocalSearchEngine, OfflineMode,
    OfflineModeConfig, SearchResult, SearchStats, StaticToolDescriptions, SyntaxValidationResult,
};
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use openrouter::OpenRouterProvider;
pub use perplexity::PerplexityProvider;
pub use together::TogetherProvider;
pub use zhipu::ZhipuProvider;

// Export provider types
pub use provider::{
    sanitize_error_message, validate_endpoint, ChatMessage, CompletionRequest, CompletionResponse,
    LLMProvider, MessageRole, ProviderConfig, ProviderError, ProviderType, StreamChunk, Usage,
};

// Export unified LLMProvider trait from protocol
pub use rustycode_protocol::llm::{
    CompletionRequest as UnifiedCompletionRequest, CompletionResponse as UnifiedCompletionResponse,
    Cost as UnifiedCost, LLMProvider as UnifiedLLMProvider, ModelInfo as UnifiedModelInfo,
    TokenCount as UnifiedTokenCount,
};

pub use provider_error_policy::{retry_plan_for_error, user_facing_error_for, RetryPlan};
pub use rate_limiter::{RateLimitConfig, RateLimitType, RateLimiter, RateLimiterBuilder};
pub use registry::{
    ModelInfo as ModelSpec, ModelTier, ProviderMetadata as ProviderMeta, ProviderMetadataRegistry,
    ProviderRegistry, ProviderRegistryBuilder, TaskModelConfig, TaskType,
};
pub use retry::{is_retryable_error, retry_with_backoff, RetryConfig};
pub use token_tracker::{
    cost_per_million_tokens, cost_per_million_tokens_io, estimate_cost, ModelUsage, TokenTracker,
    TrackedRequest, UsageSummary,
};

// Tool execution integration
pub use singleton_provider::{initialize_provider, is_initialized, reset, SharedLLMProvider};
pub use tool_executor::{LLMToolExecutor, ParsedToolCall, ToolExecutionResult};

pub use utils::{
    chunk_text, estimate_tokens, extract_reasoning_effort, extract_xml, extract_xml_all,
    extract_xml_all_multiline, extract_xml_multiline, has_tag, is_reasoning_model, llm_call,
    parse_summary, strip_xml_tags, ReasoningEffort, Summary,
};

// Provider metadata system for dynamic configuration and prompt optimization
pub use provider_metadata::{
    get_metadata, ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat, ToolSchema,
};

// Model info and capability metadata exports
pub use model_info::{
    is_reasoning_model_name, KnownModels, ModelCapabilities, DEFAULT_CONTEXT_LIMIT,
};

// Provider helpers for convenient access to registry functions
pub use provider_helpers::{
    find_model_provider, find_provider, get_cheapest_model, get_context_window, get_default_model,
    get_model_cost, get_models_by_tier, get_provider_info_json, get_registry, is_model_available,
    is_provider_available, list_models, list_provider_models, list_providers, select_model,
    select_model_with_config,
};

// Circuit breaker for managing endpoint health and cascading failure prevention
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerRegistry, CircuitBreakerStatus,
    CircuitState,
};

// Timeout handling for LLM operations and tool execution
pub use timeout_handler::{
    ModelTimeoutPreset, TimeoutConfig, TimeoutEvent, TimeoutHandler, TimeoutStats, TimeoutTracker,
};

// Multi-provider router for intelligent provider selection and failover

pub use mock::MockProvider;

/// Create a provider of the specified type with the given model and default config.
pub fn create_provider(
    provider_type: &str,
    model: &str,
) -> Result<std::sync::Arc<dyn LLMProvider>> {
    create_provider_with_config(provider_type, model, ProviderConfig::default())
}

/// Create a provider with a specific config
pub fn create_provider_with_config(
    provider_type: &str,
    model: &str,
    config: ProviderConfig,
) -> Result<std::sync::Arc<dyn LLMProvider>> {
    // Fall back to env var for API key if not set in config
    let config = if config.api_key.is_some() {
        config
    } else {
        ProviderConfig {
            api_key: std::env::var(api_key_env_name(provider_type))
                .ok()
                .map(|s| SecretString::new(s.into())),
            ..config
        }
    };

    let provider: std::sync::Arc<dyn LLMProvider> = match provider_type.to_lowercase().as_str() {
        "openai" | "open_ai" => {
            std::sync::Arc::new(OpenAiProvider::new(config, model.to_string())?)
        }
        "anthropic" => std::sync::Arc::new(AnthropicProvider::new(config, model.to_string())?),
        "gemini" | "google" => std::sync::Arc::new(GeminiProvider::new(config)?),
        "ollama" => std::sync::Arc::new(OllamaProvider::new(config)?),
        "azure" | "azure_openai" | "microsoft" => std::sync::Arc::new(AzureProvider::new(config)?),
        "bedrock" | "aws" => std::sync::Arc::new(BedrockProvider::new(config, model.to_string())?),
        "cohere" => std::sync::Arc::new(CohereProvider::new(config)?),
        "mistral" | "mistral_ai" => {
            std::sync::Arc::new(MistralProvider::new(config, model.to_string())?)
        }
        "together" | "together_ai" => std::sync::Arc::new(TogetherProvider::new(config)?),
        "perplexity" | "pplx" => {
            std::sync::Arc::new(PerplexityProvider::new(config, model.to_string())?)
        }
        "huggingface" | "hf" => {
            std::sync::Arc::new(HuggingFaceProvider::new(config, model.to_string())?)
        }
        "openrouter" => std::sync::Arc::new(OpenRouterProvider::new(config, model.to_string())?),
        "nvidia" => {
            let nvidia_config = if config.base_url.is_none() {
                ProviderConfig {
                    base_url: Some("https://integrate.api.nvidia.com/v1".to_string()),
                    ..config
                }
            } else {
                config
            };
            std::sync::Arc::new(OpenAiProvider::new(nvidia_config, model.to_string())?)
        }
        #[cfg(feature = "litert")]
        "litert-lm" | "litert_lm" | "litert" => {
            std::sync::Arc::new(LiteRtLmProvider::new(config, model.to_string())?)
        }
        _ => anyhow::bail!("Unsupported provider type: {}", provider_type),
    };
    Ok(provider)
}

/// Load just the model name from config
pub fn load_model_from_config() -> Result<String> {
    let (_, model, _) = load_provider_config_from_env()?;
    Ok(model)
}

/// Load provider type from config
pub fn load_provider_type_from_config() -> Result<String> {
    let (provider_type, _, _) = load_provider_config_from_env()?;
    Ok(provider_type)
}

/// Load provider config from config file (~/.rustycode/config.json), with env var overrides.
///
/// Priority: environment variable > config file > default
pub fn load_provider_config_from_env() -> Result<(String, String, ProviderConfig)> {
    let file_config = load_file_config();

    let provider_type = std::env::var("RUSTYCODE_PROVIDER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| file_config.as_ref().map(|c| c.provider.clone()))
        .unwrap_or_else(|| "anthropic".to_string());

    let model = std::env::var("RUSTYCODE_MODEL_OVERRIDE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| file_config.as_ref().map(|c| c.model.clone()))
        .unwrap_or_else(|| default_model_for_provider(&provider_type));

    let api_key = resolve_api_key(&provider_type, file_config.as_ref());
    let base_url = resolve_base_url(&provider_type, file_config.as_ref());
    let timeout = file_config
        .as_ref()
        .and_then(|c| c.timeout_seconds)
        .unwrap_or(120);

    let config = ProviderConfig {
        api_key,
        base_url,
        timeout_seconds: Some(timeout),
        extra_headers: None,
        retry_config: None,
    };

    Ok((provider_type, model, config))
}

struct FileConfig {
    provider: String,
    model: String,
    timeout_seconds: Option<u64>,
    providers: rustycode_config::ProvidersConfig,
}

/// Load config from ~/.rustycode/config.json
fn load_file_config() -> Option<FileConfig> {
    let home = dirs::home_dir()?;
    let config_path = home.join(".rustycode").join("config.json");
    if !config_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&config_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;

    Some(FileConfig {
        provider: value
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("anthropic")
            .to_string(),
        model: value
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-6-20250514")
            .to_string(),
        timeout_seconds: value.get("timeout_seconds").and_then(|v| v.as_u64()),
        providers: serde_json::from_value(
            value
                .get("providers")
                .cloned()
                .unwrap_or(serde_json::json!({})),
        )
        .ok()?,
    })
}

fn resolve_api_key(provider_type: &str, file_config: Option<&FileConfig>) -> Option<SecretString> {
    if let Ok(key) = std::env::var(api_key_env_name(provider_type)) {
        if !key.trim().is_empty() {
            return Some(SecretString::new(key.into()));
        }
    }

    if matches!(provider_type.to_lowercase().as_str(), "copilot" | "github") {
        let store = rustycode_auth::TokenStore::new();
        if let Ok(true) = store.is_token_valid("copilot") {
            if let Ok(token) = store.get_token("copilot") {
                return Some(token.access_token);
            }
        }
    }

    file_config.and_then(|fc| {
        let providers = &fc.providers;

        let provider_cfg = resolve_builtin_provider_config(provider_type, providers);

        if let Some(cfg) = provider_cfg {
            if let Some(ref key) = cfg.api_key {
                if !key.trim().is_empty() {
                    return Some(SecretString::new(key.clone().into()));
                }
            }
        }

        providers
            .custom
            .get(provider_type)
            .and_then(|v| v.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| SecretString::new(s.to_string().into()))
            .or_else(|| {
                if matches!(provider_type.to_lowercase().as_str(), "copilot" | "github") {
                    providers
                        .custom
                        .get("copilot")
                        .or_else(|| providers.custom.get("github"))
                        .and_then(|v| v.get("api_key"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| SecretString::new(s.to_string().into()))
                } else {
                    None
                }
            })
    })
}

fn resolve_base_url(provider_type: &str, file_config: Option<&FileConfig>) -> Option<String> {
    if let Ok(url) = std::env::var(format!("{}_BASE_URL", provider_type.to_uppercase())) {
        if !url.trim().is_empty() {
            return Some(url);
        }
    }

    file_config.and_then(|fc| {
        let providers = &fc.providers;

        let provider_cfg = resolve_builtin_provider_config(provider_type, providers);

        if let Some(cfg) = provider_cfg {
            if let Some(ref url) = cfg.base_url {
                if !url.trim().is_empty() {
                    return Some(url.clone());
                }
            }
        }

        providers
            .custom
            .get(provider_type)
            .and_then(|v| v.get("base_url"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
    })
}

fn resolve_builtin_provider_config<'a>(
    provider_type: &str,
    providers: &'a rustycode_config::ProvidersConfig,
) -> Option<&'a rustycode_config::ProviderConfig> {
    match provider_type.to_lowercase().as_str() {
        "anthropic" => providers.anthropic.as_ref(),
        "openai" | "open_ai" => providers.openai.as_ref(),
        "openrouter" => providers.openrouter.as_ref(),
        "nvidia" => providers.nvidia.as_ref(),
        _ => None,
    }
}

#[cfg(test)]
mod config_file_tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn test_resolve_api_key_from_file_config() {
        let providers = rustycode_config::ProvidersConfig {
            openai: Some(rustycode_config::ProviderConfig {
                api_key: Some("sk-test-fake-key-for-testing-123456".to_string()),
                base_url: Some("https://example.com/v1".to_string()),
                models: None,
                headers: None,
            }),
            ..Default::default()
        };
        let file_config = FileConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            providers,
            timeout_seconds: None,
        };
        let api_key = resolve_api_key("openai", Some(&file_config));
        assert!(
            api_key.is_some(),
            "API key should be loaded from file config"
        );
        let key = api_key.unwrap().expose_secret().to_string();
        assert_eq!(key, "sk-test-fake-key-for-testing-123456");
    }

    #[test]
    fn test_resolve_base_url_from_file_config() {
        let providers = rustycode_config::ProvidersConfig {
            openai: Some(rustycode_config::ProviderConfig {
                api_key: Some("sk-test".to_string()),
                base_url: Some("https://custom.example.com/v1".to_string()),
                models: None,
                headers: None,
            }),
            ..Default::default()
        };
        let file_config = FileConfig {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            providers,
            timeout_seconds: None,
        };
        let base_url = resolve_base_url("openai", Some(&file_config));
        assert_eq!(base_url, Some("https://custom.example.com/v1".to_string()));
    }
}
