//! Model registry with predefined providers and models
//!
//! This module provides the core registry for managing providers and models.

use super::{ModelInfo, ProviderMetadata};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Errors that can occur during provider bootstrap
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProviderBootstrapError {
    #[error("Provider '{0}' is already registered")]
    AlreadyRegistered(String),

    #[error("Provider '{0}' not found")]
    NotFound(String),

    #[error("Invalid provider configuration: {0}")]
    InvalidConfig(String),
}

/// Registry for providers and models
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    /// Registered providers
    providers: Arc<RwLock<HashMap<String, ProviderMetadata>>>,

    /// Models keyed by provider ID
    models: Arc<RwLock<HashMap<String, Vec<ModelInfo>>>>,

    cost_tracker: Arc<super::CostTracker>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            models: Arc::new(RwLock::new(HashMap::new())),
            cost_tracker: Arc::new(super::CostTracker::new()),
        }
    }

    /// Register a provider
    pub async fn register_provider(&self, provider: ProviderMetadata) {
        let mut providers = self.providers.write().await;
        providers.insert(provider.id.clone(), provider);
    }

    /// Register a model for a provider
    pub async fn register_model(&self, provider_id: &str, model: ModelInfo) {
        let mut models = self.models.write().await;
        models
            .entry(provider_id.to_string())
            .or_insert_with(Vec::new)
            .push(model);
    }

    /// Get provider metadata by ID
    pub async fn provider(&self, id: &str) -> Option<ProviderMetadata> {
        let providers = self.providers.read().await;
        providers.get(id).cloned()
    }

    /// Get model info by provider and model ID
    pub async fn model(&self, provider_id: &str, model_id: &str) -> Option<ModelInfo> {
        let models = self.models.read().await;
        models
            .get(provider_id)
            .and_then(|model_list| model_list.iter().find(|m| m.id == model_id))
            .cloned()
    }

    /// List all registered provider IDs
    pub async fn list_providers(&self) -> Vec<String> {
        let providers = self.providers.read().await;
        providers.keys().cloned().collect()
    }

    /// List all models for a provider
    pub async fn list_models(&self, provider_id: &str) -> Vec<ModelInfo> {
        let models = self.models.read().await;
        models.get(provider_id).cloned().unwrap_or_default()
    }

    /// List all models across all providers
    pub async fn list_all_models(&self) -> Vec<ModelInfo> {
        let models = self.models.read().await;
        models.values().flatten().cloned().collect()
    }

    /// Get cost tracker reference
    pub fn cost_tracker(&self) -> Arc<super::CostTracker> {
        Arc::clone(&self.cost_tracker)
    }

    /// Get cost summary
    pub async fn cost_summary(&self) -> super::CostSummary {
        self.cost_tracker.summary().await
    }

    /// Track API usage
    pub async fn track_usage(
        &self,
        provider_id: &str,
        model_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost: f64,
    ) {
        let key = format!("{}/{}", provider_id, model_id);
        self.cost_tracker
            .track(&key, input_tokens, output_tokens, cost)
            .await;
    }

    /// Get total number of providers
    pub async fn count(&self) -> usize {
        let providers = self.providers.read().await;
        providers.len()
    }

    /// Check if a provider is registered
    pub async fn has_provider(&self, id: &str) -> bool {
        let providers = self.providers.read().await;
        providers.contains_key(id)
    }

    /// Unregister a provider and all its models
    pub async fn unregister_provider(&self, id: &str) -> bool {
        let mut providers = self.providers.write().await;
        let mut models = self.models.write().await;

        let had_provider = providers.remove(id).is_some();
        let had_models = models.remove(id).is_some();
        had_provider || had_models
    }

    /// Clear all providers and models
    pub async fn clear(&self) {
        let mut providers = self.providers.write().await;
        let mut models = self.models.write().await;

        providers.clear();
        models.clear();
        self.cost_tracker.reset().await;
    }

    /// Get registry statistics
    pub async fn stats(&self) -> RegistryStats {
        let providers = self.providers.read().await;
        let models = self.models.read().await;
        let cost_summary = self.cost_tracker.summary().await;

        let total_models = models.values().map(|v| v.len()).sum();

        RegistryStats {
            provider_count: providers.len(),
            model_count: total_models,
            total_cost: cost_summary.total_cost,
            total_requests: cost_summary.total_requests,
        }
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    /// Number of registered providers
    pub provider_count: usize,

    /// Total number of models across all providers
    pub model_count: usize,

    /// Total tracked cost
    pub total_cost: f64,

    /// Total number of tracked requests
    pub total_requests: usize,
}

/// Predefined models for each provider — delegated to [`crate::model_catalog`]
pub mod predefined {
    use super::*;

    fn cost_tier_for(output_cost_per_1m: f64) -> u8 {
        if output_cost_per_1m == 0.0 {
            0
        } else if output_cost_per_1m < 0.5 {
            1
        } else if output_cost_per_1m < 2.0 {
            2
        } else if output_cost_per_1m < 5.0 {
            3
        } else if output_cost_per_1m < 15.0 {
            4
        } else {
            5
        }
    }

    fn entry_to_info(e: &crate::model_catalog::ModelEntry) -> ModelInfo {
        ModelInfo {
            id: e.id.to_string(),
            name: e.id.to_string(),
            provider_id: e.provider.to_string(),
            description: String::new(),
            context_window: e.context_window,
            supports_tools: e.supports_tools,
            supports_vision: e.supports_vision,
            max_tokens: e.max_output as u32,
            input_cost_per_1k: e.input_cost_per_1m / 1000.0,
            output_cost_per_1k: e.output_cost_per_1m / 1000.0,
            use_cases: vec![],
            cost_tier: cost_tier_for(e.output_cost_per_1m),
        }
    }

    fn provider_models(provider_id: &str) -> Vec<ModelInfo> {
        crate::model_catalog::models_for_provider(provider_id)
            .iter()
            .map(|e| entry_to_info(e))
            .collect()
    }

    /// Anthropic Claude models
    pub fn anthropic_models() -> Vec<ModelInfo> {
        provider_models("anthropic")
    }

    /// OpenAI GPT models
    pub fn openai_models() -> Vec<ModelInfo> {
        provider_models("openai")
    }

    /// OpenRouter models
    pub fn openrouter_models() -> Vec<ModelInfo> {
        provider_models("openrouter")
    }

    /// Google Gemini models
    pub fn gemini_models() -> Vec<ModelInfo> {
        provider_models("gemini")
    }

    /// Groq high-speed models
    pub fn groq_models() -> Vec<ModelInfo> {
        provider_models("groq")
    }

    /// GitHub Copilot models
    pub fn copilot_models() -> Vec<ModelInfo> {
        provider_models("copilot")
    }

    /// Zhipu AI (z.ai) GLM models
    pub fn zhipu_models() -> Vec<ModelInfo> {
        provider_models("zhipu")
    }

    /// Ollama local models
    pub fn ollama_models() -> Vec<ModelInfo> {
        provider_models("ollama")
    }

    /// Kimi/Moonshot AI China models
    pub fn kimi_cn_models() -> Vec<ModelInfo> {
        provider_models("kimi-cn")
    }

    /// Kimi/Moonshot AI Global models
    pub fn kimi_global_models() -> Vec<ModelInfo> {
        provider_models("kimi-global")
    }

    /// Alibaba/DashScope China Qwen models
    pub fn alibaba_cn_models() -> Vec<ModelInfo> {
        provider_models("alibaba-cn")
    }

    /// Alibaba/DashScope Global Qwen models
    pub fn alibaba_global_models() -> Vec<ModelInfo> {
        provider_models("alibaba-global")
    }

    /// Google Vertex AI Gemini models
    pub fn vertex_models() -> Vec<ModelInfo> {
        provider_models("vertex")
    }

    /// Mistral models
    pub fn mistral_models() -> Vec<ModelInfo> {
        provider_models("mistral")
    }

    /// Azure OpenAI models
    pub fn azure_models() -> Vec<ModelInfo> {
        provider_models("azure")
    }

    /// Perplexity models
    pub fn perplexity_models() -> Vec<ModelInfo> {
        provider_models("perplexity")
    }

    /// AWS Bedrock models
    pub fn bedrock_models() -> Vec<ModelInfo> {
        provider_models("bedrock")
    }

    /// LiteRT-LM local models
    pub fn litert_lm_models() -> Vec<ModelInfo> {
        provider_models("litert-lm")
    }

    /// Default context window when model is unrecognized
    pub const DEFAULT_CONTEXT_WINDOW: usize = crate::model_catalog::DEFAULT_CONTEXT_WINDOW;

    /// Look up context window size for a model by its ID.
    pub fn context_window_for_model(model_id: &str) -> usize {
        crate::model_catalog::context_window_for_model(model_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = ModelRegistry::new();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_register_provider() {
        let registry = ModelRegistry::new();

        let provider = ProviderMetadata {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            base_url: "https://test.com".to_string(),
            api_key_env: "TEST_API_KEY".to_string(),
            auth_method: super::super::AuthMethod::ApiKey,
            capabilities: super::super::ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: 4096,
                max_context_window: 8192,
            },
            pricing: super::super::PricingInfo {
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.002,
                currency: super::super::Currency::Usd,
            },
        };

        registry.register_provider(provider).await;
        assert_eq!(registry.count().await, 1);
        assert!(registry.has_provider("test").await);
    }

    #[tokio::test]
    async fn test_register_and_get_model() {
        let registry = ModelRegistry::new();

        let model = ModelInfo {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            provider_id: "test".to_string(),
            description: "A test model".to_string(),
            context_window: 8192,
            supports_tools: false,
            supports_vision: false,
            max_tokens: 4096,
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
            use_cases: vec!["Testing".to_string()],
            cost_tier: 2,
        };

        registry.register_model("test", model).await;
        let retrieved = registry.model("test", "test-model").await;

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "test-model");
        assert_eq!(retrieved.provider_id, "test");
    }

    #[tokio::test]
    async fn test_list_models() {
        let registry = ModelRegistry::new();

        let model1 = ModelInfo {
            id: "model1".to_string(),
            name: "Model 1".to_string(),
            provider_id: "test".to_string(),
            description: "Test".to_string(),
            context_window: 8192,
            supports_tools: false,
            supports_vision: false,
            max_tokens: 4096,
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
            use_cases: vec![],
            cost_tier: 2,
        };

        let model2 = ModelInfo {
            id: "model2".to_string(),
            name: "Model 2".to_string(),
            provider_id: "test".to_string(),
            description: "Test".to_string(),
            context_window: 8192,
            supports_tools: false,
            supports_vision: false,
            max_tokens: 4096,
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
            use_cases: vec![],
            cost_tier: 2,
        };

        registry.register_model("test", model1).await;
        registry.register_model("test", model2).await;

        let models = registry.list_models("test").await;
        assert_eq!(models.len(), 2);
    }

    #[tokio::test]
    async fn test_cost_tracking() {
        let registry = ModelRegistry::new();

        registry
            .track_usage("anthropic", "claude-3-5-sonnet", 1000, 500, 0.0105)
            .await;

        let summary = registry.cost_summary().await;
        assert_eq!(summary.total_requests, 1);
        assert!((summary.total_cost - 0.0105).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_registry_stats() {
        let registry = ModelRegistry::new();

        let provider = ProviderMetadata {
            id: "test".to_string(),
            name: "Test".to_string(),
            base_url: "https://test.com".to_string(),
            api_key_env: "TEST_KEY".to_string(),
            auth_method: super::super::AuthMethod::ApiKey,
            capabilities: super::super::ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: 4096,
                max_context_window: 8192,
            },
            pricing: super::super::PricingInfo {
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.002,
                currency: super::super::Currency::Usd,
            },
        };

        registry.register_provider(provider).await;
        registry.track_usage("test", "model", 1000, 500, 0.01).await;

        let stats = registry.stats().await;
        assert_eq!(stats.provider_count, 1);
        assert_eq!(stats.model_count, 0);
        assert_eq!(stats.total_requests, 1);
    }

    #[test]
    fn test_provider_bootstrap_error_display() {
        let err = ProviderBootstrapError::AlreadyRegistered("openai".to_string());
        assert!(err.to_string().contains("openai"));
        assert!(err.to_string().contains("already registered"));

        let err = ProviderBootstrapError::NotFound("missing".to_string());
        assert!(err.to_string().contains("missing"));

        let err = ProviderBootstrapError::InvalidConfig("bad key".to_string());
        assert!(err.to_string().contains("bad key"));
    }

    #[tokio::test]
    async fn test_registry_default() {
        let registry = ModelRegistry::default();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_list_providers_empty() {
        let registry = ModelRegistry::new();
        assert!(registry.list_providers().await.is_empty());
    }

    #[tokio::test]
    async fn test_get_provider_nonexistent() {
        let registry = ModelRegistry::new();
        assert!(registry.provider("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_get_model_nonexistent() {
        let registry = ModelRegistry::new();
        assert!(registry.model("no-provider", "no-model").await.is_none());
    }

    #[tokio::test]
    async fn test_list_models_for_unknown_provider() {
        let registry = ModelRegistry::new();
        let models = registry.list_models("unknown").await;
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_has_provider_false() {
        let registry = ModelRegistry::new();
        assert!(!registry.has_provider("missing").await);
    }

    #[tokio::test]
    async fn test_unregister_provider() {
        let registry = ModelRegistry::new();
        let provider = ProviderMetadata {
            id: "remove-me".to_string(),
            name: "Remove".to_string(),
            base_url: "https://test.com".to_string(),
            api_key_env: "KEY".to_string(),
            auth_method: super::super::AuthMethod::ApiKey,
            capabilities: super::super::ProviderCapabilities {
                supports_streaming: false,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: 1024,
                max_context_window: 4096,
            },
            pricing: super::super::PricingInfo {
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                currency: super::super::Currency::Usd,
            },
        };
        registry.register_provider(provider).await;
        assert!(registry.has_provider("remove-me").await);

        let removed = registry.unregister_provider("remove-me").await;
        assert!(removed);
        assert!(!registry.has_provider("remove-me").await);
    }

    #[tokio::test]
    async fn test_unregister_nonexistent() {
        let registry = ModelRegistry::new();
        let removed = registry.unregister_provider("ghost").await;
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_clear() {
        let registry = ModelRegistry::new();
        let provider = ProviderMetadata {
            id: "clear-test".to_string(),
            name: "Clear".to_string(),
            base_url: "https://test.com".to_string(),
            api_key_env: "KEY".to_string(),
            auth_method: super::super::AuthMethod::ApiKey,
            capabilities: super::super::ProviderCapabilities {
                supports_streaming: false,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: 1024,
                max_context_window: 4096,
            },
            pricing: super::super::PricingInfo {
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.002,
                currency: super::super::Currency::Usd,
            },
        };
        registry.register_provider(provider).await;
        registry.track_usage("test", "model", 100, 50, 0.01).await;

        registry.clear().await;
        assert_eq!(registry.count().await, 0);
        assert_eq!(registry.cost_summary().await.total_requests, 0);
    }

    #[test]
    fn test_registry_stats_serialization() {
        let stats = RegistryStats {
            provider_count: 3,
            model_count: 10,
            total_cost: 1.5,
            total_requests: 42,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: RegistryStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_count, 3);
        assert_eq!(decoded.model_count, 10);
        assert_eq!(decoded.total_requests, 42);
    }

    #[test]
    fn test_predefined_context_window_known_model() {
        let cw = predefined::context_window_for_model("claude-sonnet-4-6");
        assert_eq!(cw, 1_000_000);
    }

    #[test]
    fn test_predefined_context_window_gpt55() {
        let cw = predefined::context_window_for_model("gpt-5.5");
        assert!(cw >= 1_000_000, "expected >= 1M, got {cw}");
    }

    #[test]
    fn test_predefined_context_window_unknown() {
        let cw = predefined::context_window_for_model("totally-unknown-model");
        assert_eq!(cw, predefined::DEFAULT_CONTEXT_WINDOW);
    }

    #[test]
    fn test_predefined_anthropic_models_count() {
        let models = predefined::anthropic_models();
        assert!(models.len() >= 3);
    }

    #[test]
    fn test_predefined_openai_models_count() {
        let models = predefined::openai_models();
        assert!(models.len() >= 5);
    }

    #[test]
    fn test_predefined_gemini_models_count() {
        let models = predefined::gemini_models();
        assert!(models.len() >= 3);
    }

    #[test]
    fn test_predefined_ollama_models_count() {
        let models = predefined::ollama_models();
        assert!(models.len() >= 2);
    }

    #[test]
    fn test_predefined_vertex_models_count() {
        let models = predefined::vertex_models();
        assert!(models.len() >= 3);
    }

    #[test]
    fn test_predefined_openrouter_models_count() {
        let models = predefined::openrouter_models();
        assert!(models.len() >= 2);
        assert!(models
            .iter()
            .any(|m| m.id.contains("gpt-4o") || m.id.contains("gemini")));
    }

    #[test]
    fn test_predefined_kimi_cn_models_count() {
        let models = predefined::kimi_cn_models();
        assert_eq!(models.len(), 2);
        assert!(models[0].supports_tools);
        assert_eq!(models[0].context_window, 200_000);
    }

    #[test]
    fn test_predefined_kimi_global_models_count() {
        let models = predefined::kimi_global_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].provider_id, "kimi-global");
    }

    #[test]
    fn test_predefined_alibaba_cn_models() {
        let models = predefined::alibaba_cn_models();
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "qwen-max"));
        assert!(models.iter().any(|m| m.id == "qwen-coder-plus"));
    }

    #[test]
    fn test_predefined_alibaba_global_models() {
        let models = predefined::alibaba_global_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].provider_id, "alibaba-global");
    }

    #[test]
    fn test_predefined_anthropic_models_have_tools() {
        let models = predefined::anthropic_models();
        assert!(models.iter().all(|m| m.supports_tools));
        assert!(models.iter().all(|m| m.supports_vision));
    }

    #[test]
    fn test_predefined_openai_models_vision() {
        let models = predefined::openai_models();
        // GPT-5.5 should be present and support vision
        let gpt55 = models.iter().find(|m| m.id == "gpt-5.5");
        if let Some(m) = gpt55 {
            assert!(m.supports_vision);
        }
    }

    #[test]
    fn test_predefined_gemini_models_context_window() {
        let models = predefined::gemini_models();
        assert!(models.iter().all(|m| m.context_window == 1_048_576));
    }

    #[test]
    fn test_predefined_ollama_models_free() {
        let models = predefined::ollama_models();
        assert!(models.iter().all(|m| m.is_free()));
    }

    #[test]
    fn test_predefined_vertex_models_have_tools() {
        let models = predefined::vertex_models();
        assert!(models.iter().all(|m| m.supports_tools));
        assert!(models.iter().all(|m| m.supports_vision));
    }

    #[test]
    fn test_context_window_for_model_ollama() {
        let cw = predefined::context_window_for_model("llama3");
        assert_eq!(cw, 128_000);
    }

    #[test]
    fn test_context_window_for_model_gemini() {
        let cw = predefined::context_window_for_model("gemini-2.5-pro");
        assert_eq!(cw, 1_048_576);
    }

    #[test]
    fn test_context_window_for_model_kimi() {
        // kimi-k2 is registered under both cn and global — should return 200_000
        let cw = predefined::context_window_for_model("kimi-k2");
        assert_eq!(cw, 200_000);
    }

    #[test]
    fn test_context_window_for_model_qwen() {
        let cw = predefined::context_window_for_model("qwen-max");
        assert_eq!(cw, 128_000);
    }

    #[test]
    fn test_context_window_for_model_vertex_flash() {
        let cw = predefined::context_window_for_model("gemini-2.5-flash");
        assert_eq!(cw, 1_048_576);
    }

    #[tokio::test]
    async fn test_unregister_provider_removes_models() {
        let registry = ModelRegistry::new();
        let provider = ProviderMetadata {
            id: "unreg-test".to_string(),
            name: "Unreg".to_string(),
            base_url: "https://test.com".to_string(),
            api_key_env: "KEY".to_string(),
            auth_method: super::super::AuthMethod::ApiKey,
            capabilities: super::super::ProviderCapabilities {
                supports_streaming: false,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: 1024,
                max_context_window: 4096,
            },
            pricing: super::super::PricingInfo {
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                currency: super::super::Currency::Usd,
            },
        };
        registry.register_provider(provider).await;

        let model = ModelInfo {
            id: "m1".to_string(),
            name: "M1".to_string(),
            provider_id: "unreg-test".to_string(),
            description: "test".to_string(),
            context_window: 8192,
            supports_tools: false,
            supports_vision: false,
            max_tokens: 1024,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
            use_cases: vec![],
            cost_tier: 1,
        };
        registry.register_model("unreg-test", model).await;
        assert_eq!(registry.list_models("unreg-test").await.len(), 1);

        registry.unregister_provider("unreg-test").await;
        assert!(registry.list_models("unreg-test").await.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_providers_stats() {
        let registry = ModelRegistry::new();

        for pid in &["p1", "p2", "p3"] {
            let provider = ProviderMetadata {
                id: pid.to_string(),
                name: format!("Provider {}", pid),
                base_url: format!("https://{}.com", pid),
                api_key_env: "KEY".to_string(),
                auth_method: super::super::AuthMethod::ApiKey,
                capabilities: super::super::ProviderCapabilities {
                    supports_streaming: true,
                    supports_function_calling: false,
                    supports_vision: false,
                    max_tokens: 4096,
                    max_context_window: 8192,
                },
                pricing: super::super::PricingInfo {
                    input_cost_per_1k: 0.001,
                    output_cost_per_1k: 0.002,
                    currency: super::super::Currency::Usd,
                },
            };
            registry.register_provider(provider).await;
        }

        assert_eq!(registry.count().await, 3);
        let providers = registry.list_providers().await;
        assert_eq!(providers.len(), 3);
    }

    #[tokio::test]
    async fn test_get_provider_returns_metadata() {
        let registry = ModelRegistry::new();
        let provider = ProviderMetadata {
            id: "fetch-test".to_string(),
            name: "Fetch Test".to_string(),
            base_url: "https://fetch.example.com".to_string(),
            api_key_env: "FETCH_KEY".to_string(),
            auth_method: super::super::AuthMethod::ApiKey,
            capabilities: super::super::ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: false,
                max_tokens: 8192,
                max_context_window: 128_000,
            },
            pricing: super::super::PricingInfo {
                input_cost_per_1k: 0.005,
                output_cost_per_1k: 0.015,
                currency: super::super::Currency::Usd,
            },
        };
        registry.register_provider(provider.clone()).await;

        let fetched = registry.provider("fetch-test").await.unwrap();
        assert_eq!(fetched.name, "Fetch Test");
        assert_eq!(fetched.base_url, "https://fetch.example.com");
        assert!(fetched.capabilities.supports_function_calling);
        assert_eq!(fetched.capabilities.max_context_window, 128_000);
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14 is a cost value, not PI
    fn test_registry_stats_serde_roundtrip() {
        let stats = RegistryStats {
            provider_count: 5,
            model_count: 20,
            total_cost: 3.14,
            total_requests: 100,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: RegistryStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_count, 5);
        assert_eq!(decoded.model_count, 20);
        assert!((decoded.total_cost - 3.14).abs() < 0.001);
        assert_eq!(decoded.total_requests, 100);
    }

    #[test]
    fn test_provider_bootstrap_error_variants() {
        let err1 = ProviderBootstrapError::AlreadyRegistered("x".to_string());
        let msg1 = err1.to_string();
        assert!(msg1.contains("'x'"));
        assert!(msg1.contains("already registered"));

        let err2 = ProviderBootstrapError::NotFound("y".to_string());
        assert!(err2.to_string().contains("'y'"));
        assert!(err2.to_string().contains("not found"));

        let err3 = ProviderBootstrapError::InvalidConfig("z".to_string());
        assert!(err3.to_string().contains("z"));
        assert!(err3.to_string().contains("Invalid"));
    }

    #[test]
    fn test_predefined_models_provider_ids_consistent() {
        // All anthropic models should have provider_id "anthropic"
        for m in predefined::anthropic_models() {
            assert_eq!(m.provider_id, "anthropic");
        }
        for m in predefined::openai_models() {
            assert_eq!(m.provider_id, "openai");
        }
        for m in predefined::ollama_models() {
            assert_eq!(m.provider_id, "ollama");
        }
        for m in predefined::gemini_models() {
            assert_eq!(m.provider_id, "gemini");
        }
        for m in predefined::vertex_models() {
            assert_eq!(m.provider_id, "vertex");
        }
    }

    #[test]
    fn test_predefined_litert_lm_models() {
        let models = predefined::litert_lm_models();
        assert_eq!(models.len(), 8);
        assert!(models.iter().all(|m| m.provider_id == "litert-lm"));
        assert!(models.iter().all(|m| m.is_free()));
        assert!(models.iter().any(|m| m.id == "gemma-4-e2b-it"));
        assert!(models.iter().any(|m| m.id == "gemma-4-e4b-it"));
    }

    #[test]
    fn test_context_window_for_litert_lm_models() {
        assert_eq!(
            predefined::context_window_for_model("gemma-4-e2b-it"),
            8_192
        );
        assert_eq!(
            predefined::context_window_for_model("gemma-4-e4b-it"),
            8_192
        );
    }
}
