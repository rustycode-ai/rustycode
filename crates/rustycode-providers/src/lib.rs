#![allow(
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::manual_string_new,
    clippy::missing_const_for_fn,
    clippy::redundant_closure_for_method_calls,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::use_self
)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::float_cmp,
        clippy::ignored_unit_patterns,
        clippy::match_same_arms,
        clippy::redundant_clone,
        clippy::unwrap_used,
    )
)]
//! # RustyCode Provider Registry & Bootstrap System
//!
//! This crate provides a comprehensive registry for LLM providers with:
//! - Provider metadata (capabilities, pricing, endpoints)
//! - Model registry with accurate pricing data
//! - Cost tracking and token usage
//! - Auto-discovery from environment variables
//!
//! ## Usage
//!
//! ```rust,no_run
//! use rustycode_providers::{ModelRegistry, bootstrap_from_env};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Auto-discover providers from environment
//!     let registry = bootstrap_from_env().await;
//!
//!     // List available providers
//!     for provider_id in registry.list_providers().await {
//!         if let Some(provider) = registry.get_provider(&provider_id).await {
//!             println!("Provider: {} ({})", provider.name, provider.id);
//!         }
//!     }
//!
//!     // Get cost tracking
//!     let costs = registry.get_cost_summary().await;
//!     println!("Total cost: ${:.2}", costs.total_cost);
//! }
//! ```

mod cost_tracker;
mod models;
mod pricing;
mod registry;

pub use cost_tracker::{CostAccumulator, CostSummary, CostTracker};
pub use models::{AuthMethod, ModelInfo, ProviderCapabilities, ProviderMetadata};
pub use pricing::{Currency, PricingInfo};
pub use registry::predefined;
pub use registry::ModelRegistry;
pub use registry::ProviderBootstrapError;

use std::env;

/// Auto-discover and bootstrap providers from environment variables
///
/// This function scans the environment for API keys and automatically
/// registers providers with their default configurations.
///
/// # Environment Variables
///
/// - `ANTHROPIC_API_KEY` → Anthropic provider (Claude models)
/// - `OPENAI_API_KEY` → OpenAI provider (GPT models)
/// - `OPENROUTER_API_KEY` → OpenRouter provider (multi-provider)
/// - `GEMINI_API_KEY` → Google Gemini provider
/// - `KIMI_CN_API_KEY` → Kimi/Moonshot AI China provider
/// - `KIMI_GLOBAL_API_KEY` → Kimi/Moonshot AI Global provider
/// - `ALIBABA_CN_API_KEY` → Alibaba/DashScope China provider (Qwen models)
/// - `ALIBABA_GLOBAL_API_KEY` → Alibaba/DashScope Global provider (Qwen models)
/// - `VERTEX_ACCESS_TOKEN` → Google Vertex AI OAuth token (Gemini models)
/// - `VERTEX_SERVICE_ACCOUNT_KEY` → Google Vertex AI service account JSON key
/// - `VERTEX_REGION` → Vertex AI region (defaults to us-central1)
/// - `OLLAMA_BASE_URL` → Ollama provider (local models, defaults to http://localhost:11434)
///
/// # Example
///
/// ```rust,no_run
/// use rustycode_providers::bootstrap_from_env;
///
/// #[tokio::main]
/// async fn main() {
///     let registry = bootstrap_from_env().await;
///     println!("Found {} providers", registry.count().await);
/// }
/// ```
pub async fn bootstrap_from_env() -> ModelRegistry {
    let registry = ModelRegistry::new();

    // Check for Anthropic
    if env::var("ANTHROPIC_API_KEY").is_ok() {
        tracing::info!("Found ANTHROPIC_API_KEY, registering Anthropic provider");
        registry.register_provider(providers::anthropic()).await;
    }

    // Check for OpenAI
    if env::var("OPENAI_API_KEY").is_ok() {
        tracing::info!("Found OPENAI_API_KEY, registering OpenAI provider");
        registry.register_provider(providers::openai()).await;
    }

    // Check for OpenRouter
    if env::var("OPENROUTER_API_KEY").is_ok() {
        tracing::info!("Found OPENROUTER_API_KEY, registering OpenRouter provider");
        registry.register_provider(providers::openrouter()).await;
    }

    // Check for Gemini
    if env::var("GEMINI_API_KEY").is_ok() {
        tracing::info!("Found GEMINI_API_KEY, registering Gemini provider");
        registry.register_provider(providers::gemini()).await;
    }

    // Check for Kimi/Moonshot AI - China
    if env::var("KIMI_CN_API_KEY").is_ok() {
        tracing::info!("Found KIMI_CN_API_KEY, registering Kimi China provider");
        registry.register_provider(providers::kimi_cn()).await;
    }

    // Check for Kimi/Moonshot AI - Global
    if env::var("KIMI_GLOBAL_API_KEY").is_ok() {
        tracing::info!("Found KIMI_GLOBAL_API_KEY, registering Kimi Global provider");
        registry.register_provider(providers::kimi_global()).await;
    }

    // Check for Alibaba/DashScope - China
    if env::var("ALIBABA_CN_API_KEY").is_ok() {
        tracing::info!("Found ALIBABA_CN_API_KEY, registering Alibaba China provider");
        registry.register_provider(providers::alibaba_cn()).await;
    }

    // Check for Alibaba/DashScope - Global
    if env::var("ALIBABA_GLOBAL_API_KEY").is_ok() {
        tracing::info!("Found ALIBABA_GLOBAL_API_KEY, registering Alibaba Global provider");
        registry
            .register_provider(providers::alibaba_global())
            .await;
    }

    // Check for Google Vertex AI (OAuth or Service Account)
    if env::var("VERTEX_ACCESS_TOKEN").is_ok() || env::var("VERTEX_SERVICE_ACCOUNT_KEY").is_ok() {
        tracing::info!("Found Vertex credentials, registering Google Vertex AI provider");
        let region = env::var("VERTEX_REGION").unwrap_or_else(|_| "us-central1".to_string());
        registry.register_provider(providers::vertex(&region)).await;
    }

    // Ollama is special - always available if running, but check for explicit config
    let ollama_url =
        env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
    tracing::info!("Ollama available at {}", ollama_url);
    registry
        .register_provider(providers::ollama(&ollama_url))
        .await;

    registry
}

/// Built-in provider definitions with accurate metadata
pub mod providers {
    use super::*;

    /// Anthropic provider with Claude models
    pub fn anthropic() -> ProviderMetadata {
        ProviderMetadata {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 200_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.003, // claude-3-5-sonnet
                output_cost_per_1k: 0.015,
                currency: Currency::Usd,
            },
        }
    }

    /// OpenAI provider with GPT models
    pub fn openai() -> ProviderMetadata {
        ProviderMetadata {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 4096,
                max_context_window: 128_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.005, // gpt-4o
                output_cost_per_1k: 0.015,
                currency: Currency::Usd,
            },
        }
    }

    /// OpenRouter provider (multi-provider aggregation)
    pub fn openrouter() -> ProviderMetadata {
        ProviderMetadata {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key_env: "OPENROUTER_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: false,
                max_tokens: 4096,
                max_context_window: 128_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.0, // Varies by model
                output_cost_per_1k: 0.0,
                currency: Currency::Usd,
            },
        }
    }

    /// Google Gemini provider
    pub fn gemini() -> ProviderMetadata {
        ProviderMetadata {
            id: "gemini".to_string(),
            name: "Google Gemini".to_string(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            api_key_env: "GEMINI_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 1_000_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.001, // gemini-pro
                output_cost_per_1k: 0.002,
                currency: Currency::Usd,
            },
        }
    }

    /// Ollama provider (local models)
    pub fn ollama(base_url: &str) -> ProviderMetadata {
        ProviderMetadata {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            base_url: base_url.to_string(),
            api_key_env: "".to_string(), // No API key needed
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: false, // Varies by model
                supports_vision: false,           // Varies by model
                max_tokens: 4096,                 // Varies by model
                max_context_window: 128_000,      // Varies by model
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.0, // Free (local)
                output_cost_per_1k: 0.0,
                currency: Currency::Usd,
            },
        }
    }

    /// Kimi/Moonshot AI China provider (Kimi models)
    pub fn kimi_cn() -> ProviderMetadata {
        ProviderMetadata {
            id: "kimi-cn".to_string(),
            name: "Kimi China (Moonshot AI)".to_string(),
            base_url: "https://api.moonshot.cn/v1".to_string(),
            api_key_env: "KIMI_CN_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 200_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.003, // kimi-latest pricing
                output_cost_per_1k: 0.015,
                currency: Currency::Usd,
            },
        }
    }

    /// Kimi/Moonshot AI Global provider (Kimi models)
    pub fn kimi_global() -> ProviderMetadata {
        ProviderMetadata {
            id: "kimi-global".to_string(),
            name: "Kimi Global (Moonshot AI)".to_string(),
            base_url: "https://api.moonshot.ai/v1".to_string(),
            api_key_env: "KIMI_GLOBAL_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 200_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.003, // kimi-latest pricing
                output_cost_per_1k: 0.015,
                currency: Currency::Usd,
            },
        }
    }

    /// Alibaba/DashScope China provider (Qwen models)
    pub fn alibaba_cn() -> ProviderMetadata {
        ProviderMetadata {
            id: "alibaba-cn".to_string(),
            name: "Alibaba China (DashScope)".to_string(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            api_key_env: "ALIBABA_CN_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 128_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.002, // qwen-max pricing
                output_cost_per_1k: 0.006,
                currency: Currency::Usd,
            },
        }
    }

    /// Alibaba/DashScope Global provider (Qwen models)
    pub fn alibaba_global() -> ProviderMetadata {
        ProviderMetadata {
            id: "alibaba-global".to_string(),
            name: "Alibaba Global (DashScope)".to_string(),
            base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1".to_string(),
            api_key_env: "ALIBABA_GLOBAL_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 128_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.002, // qwen-max pricing
                output_cost_per_1k: 0.006,
                currency: Currency::Usd,
            },
        }
    }

    /// Google Vertex AI provider (Gemini models)
    pub fn vertex(region: &str) -> ProviderMetadata {
        ProviderMetadata {
            id: "vertex".to_string(),
            name: "Google Vertex AI".to_string(),
            base_url: format!("https://{}-aiplatform.googleapis.com/v1", region),
            api_key_env: "VERTEX_ACCESS_TOKEN".to_string(),
            auth_method: AuthMethod::OAuth {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                scopes: vec!["https://www.googleapis.com/auth/cloud-platform".to_string()],
            },
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 1_000_000,
            },
            pricing: PricingInfo {
                input_cost_per_1k: 0.00125, // gemini-1.5-pro pricing
                output_cost_per_1k: 0.005,
                currency: Currency::Usd,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_metadata() {
        let provider = providers::anthropic();
        assert_eq!(provider.id, "anthropic");
        assert_eq!(provider.name, "Anthropic");
        assert_eq!(provider.base_url, "https://api.anthropic.com");
        assert_eq!(provider.api_key_env, "ANTHROPIC_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 200_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.003);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.015);
    }

    #[test]
    fn test_openai_metadata() {
        let provider = providers::openai();
        assert_eq!(provider.id, "openai");
        assert_eq!(provider.name, "OpenAI");
        assert_eq!(provider.base_url, "https://api.openai.com/v1");
        assert_eq!(provider.api_key_env, "OPENAI_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 128_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.005);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.015);
    }

    #[test]
    fn test_openrouter_metadata() {
        let provider = providers::openrouter();
        assert_eq!(provider.id, "openrouter");
        assert_eq!(provider.name, "OpenRouter");
        assert_eq!(provider.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(provider.api_key_env, "OPENROUTER_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(!provider.capabilities.supports_vision);
        // OpenRouter pricing varies by model
        assert_eq!(provider.pricing.input_cost_per_1k, 0.0);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.0);
    }

    #[test]
    fn test_gemini_metadata() {
        let provider = providers::gemini();
        assert_eq!(provider.id, "gemini");
        assert_eq!(provider.name, "Google Gemini");
        assert_eq!(
            provider.base_url,
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(provider.api_key_env, "GEMINI_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 1_000_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.001);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.002);
    }

    #[test]
    fn test_ollama_metadata() {
        let provider = providers::ollama("http://localhost:11434");
        assert_eq!(provider.id, "ollama");
        assert_eq!(provider.name, "Ollama");
        assert_eq!(provider.base_url, "http://localhost:11434");
        assert_eq!(provider.api_key_env, "");
        assert!(provider.capabilities.supports_streaming);
        // Ollama capabilities vary by model
        assert!(!provider.capabilities.supports_function_calling);
        assert!(!provider.capabilities.supports_vision);
        // Ollama is free (local)
        assert_eq!(provider.pricing.input_cost_per_1k, 0.0);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.0);
    }

    #[test]
    fn test_kimi_cn_metadata() {
        let provider = providers::kimi_cn();
        assert_eq!(provider.id, "kimi-cn");
        assert_eq!(provider.name, "Kimi China (Moonshot AI)");
        assert_eq!(provider.base_url, "https://api.moonshot.cn/v1");
        assert_eq!(provider.api_key_env, "KIMI_CN_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 200_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.003);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.015);
    }

    #[test]
    fn test_kimi_global_metadata() {
        let provider = providers::kimi_global();
        assert_eq!(provider.id, "kimi-global");
        assert_eq!(provider.name, "Kimi Global (Moonshot AI)");
        assert_eq!(provider.base_url, "https://api.moonshot.ai/v1");
        assert_eq!(provider.api_key_env, "KIMI_GLOBAL_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 200_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.003);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.015);
    }

    #[test]
    fn test_alibaba_cn_metadata() {
        let provider = providers::alibaba_cn();
        assert_eq!(provider.id, "alibaba-cn");
        assert_eq!(provider.name, "Alibaba China (DashScope)");
        assert_eq!(
            provider.base_url,
            "https://dashscope.aliyuncs.com/compatible-mode/v1"
        );
        assert_eq!(provider.api_key_env, "ALIBABA_CN_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 128_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.002);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.006);
    }

    #[test]
    fn test_alibaba_global_metadata() {
        let provider = providers::alibaba_global();
        assert_eq!(provider.id, "alibaba-global");
        assert_eq!(provider.name, "Alibaba Global (DashScope)");
        assert_eq!(
            provider.base_url,
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"
        );
        assert_eq!(provider.api_key_env, "ALIBABA_GLOBAL_API_KEY");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 128_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.002);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.006);
    }

    #[test]
    fn test_vertex_metadata() {
        let provider = providers::vertex("us-central1");
        assert_eq!(provider.id, "vertex");
        assert_eq!(provider.name, "Google Vertex AI");
        assert!(provider.base_url.contains("us-central1"));
        assert_eq!(provider.api_key_env, "VERTEX_ACCESS_TOKEN");
        assert!(provider.capabilities.supports_streaming);
        assert!(provider.capabilities.supports_function_calling);
        assert!(provider.capabilities.supports_vision);
        assert_eq!(provider.capabilities.max_context_window, 1_000_000);
        assert_eq!(provider.pricing.input_cost_per_1k, 0.00125);
        assert_eq!(provider.pricing.output_cost_per_1k, 0.005);

        // Check OAuth configuration
        match provider.auth_method {
            AuthMethod::OAuth {
                auth_url,
                token_url,
                scopes,
            } => {
                assert!(auth_url.contains("google.com"));
                assert!(token_url.contains("googleapis.com"));
                assert!(!scopes.is_empty());
            }
            _ => panic!("Expected OAuth auth method"),
        }
    }
}
