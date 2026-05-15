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

/// Predefined models for each provider
pub mod predefined {
    use super::*;

    /// Anthropic Claude models
    pub fn anthropic_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-sonnet-4-6".to_string(),
                name: "Claude Sonnet 4.6".to_string(),
                provider_id: "anthropic".to_string(),
                description: "Best coding model with 1M context and adaptive thinking".to_string(),
                context_window: 1_000_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 64000,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                use_cases: vec![
                    "Code generation".to_string(),
                    "Complex reasoning".to_string(),
                    "Analysis".to_string(),
                ],
                cost_tier: 4,
            },
            ModelInfo {
                id: "claude-opus-4-7".to_string(),
                name: "Claude Opus 4.7".to_string(),
                provider_id: "anthropic".to_string(),
                description: "Current flagship with 1M context and adaptive thinking".to_string(),
                context_window: 1_000_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 128000,
                input_cost_per_1k: 0.005,
                output_cost_per_1k: 0.025,
                use_cases: vec![
                    "Most complex tasks".to_string(),
                    "Deep analysis".to_string(),
                ],
                cost_tier: 5,
            },
            ModelInfo {
                id: "claude-haiku-4-5-20251001".to_string(),
                name: "Claude Haiku 4.5".to_string(),
                provider_id: "anthropic".to_string(),
                description: "Fast and cheap with best speed/cost ratio".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 64000,
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.005,
                use_cases: vec!["Simple tasks".to_string(), "Fast response".to_string()],
                cost_tier: 2,
            },
        ]
    }

    /// OpenAI GPT models
    // Model catalog — used by model selection UI and cost estimation
    pub fn openai_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-5.5".to_string(),
                name: "GPT-5.5".to_string(),
                provider_id: "openai".to_string(),
                description: "Most capable OpenAI model with 1M context".to_string(),
                context_window: 1_000_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 32768,
                input_cost_per_1k: 0.005,
                output_cost_per_1k: 0.03,
                use_cases: vec!["Complex tasks".to_string(), "Code generation".to_string()],
                cost_tier: 5,
            },
            ModelInfo {
                id: "gpt-5.4".to_string(),
                name: "GPT-5.4".to_string(),
                provider_id: "openai".to_string(),
                description: "High-performance model with 1M context".to_string(),
                context_window: 1_000_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 32768,
                input_cost_per_1k: 0.0025,
                output_cost_per_1k: 0.015,
                use_cases: vec!["Code generation".to_string(), "Complex tasks".to_string()],
                cost_tier: 4,
            },
            ModelInfo {
                id: "gpt-5.4-mini".to_string(),
                name: "GPT-5.4 mini".to_string(),
                provider_id: "openai".to_string(),
                description: "Cost-effective with 400K context".to_string(),
                context_window: 400_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 32768,
                input_cost_per_1k: 0.00075,
                output_cost_per_1k: 0.0045,
                use_cases: vec!["General tasks".to_string(), "Chat".to_string()],
                cost_tier: 2,
            },
            ModelInfo {
                id: "gpt-4.1".to_string(),
                name: "GPT-4.1".to_string(),
                provider_id: "openai".to_string(),
                description: "Instruction-following with 1M context (closing 2026-06-01)"
                    .to_string(),
                context_window: 1_047_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 32768,
                input_cost_per_1k: 0.002,
                output_cost_per_1k: 0.008,
                use_cases: vec!["Code generation".to_string(), "Complex tasks".to_string()],
                cost_tier: 4,
            },
            ModelInfo {
                id: "o4-mini".to_string(),
                name: "o4-mini".to_string(),
                provider_id: "openai".to_string(),
                description: "Reasoning model with vision and agentic tool use".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 100000,
                input_cost_per_1k: 0.0011,
                output_cost_per_1k: 0.0044,
                use_cases: vec!["Reasoning".to_string(), "Agentic tasks".to_string()],
                cost_tier: 3,
            },
        ]
    }

    /// OpenRouter models
    // Model catalog — used by model selection UI and cost estimation
    pub fn openrouter_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "anthropic/claude-sonnet-4-6".to_string(),
                name: "Claude Sonnet 4.6 (via OpenRouter)".to_string(),
                provider_id: "openrouter".to_string(),
                description: "Best coding model via OpenRouter".to_string(),
                context_window: 1_000_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 64000,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                use_cases: vec!["Complex tasks".to_string(), "Coding".to_string()],
                cost_tier: 4,
            },
            ModelInfo {
                id: "google/gemini-2.5-flash:free".to_string(),
                name: "Gemini 2.5 Flash (Free)".to_string(),
                provider_id: "openrouter".to_string(),
                description: "Free tier Gemini 2.5 Flash".to_string(),
                context_window: 1_048_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 65536,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Testing".to_string(), "General tasks".to_string()],
                cost_tier: 0,
            },
        ]
    }

    /// Google Gemini models
    // Model catalog — used by model selection UI and cost estimation
    pub fn gemini_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gemini-2.5-pro".to_string(),
                name: "Gemini 2.5 Pro".to_string(),
                provider_id: "gemini".to_string(),
                description: "Advanced reasoning with 1M context".to_string(),
                context_window: 1_048_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 65536,
                input_cost_per_1k: 0.00125,
                output_cost_per_1k: 0.01,
                use_cases: vec!["Complex reasoning".to_string(), "Long context".to_string()],
                cost_tier: 4,
            },
            ModelInfo {
                id: "gemini-2.5-flash".to_string(),
                name: "Gemini 2.5 Flash".to_string(),
                provider_id: "gemini".to_string(),
                description: "Best for high-volume, low-latency agentic tasks".to_string(),
                context_window: 1_048_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 65536,
                input_cost_per_1k: 0.0003,
                output_cost_per_1k: 0.0025,
                use_cases: vec!["General tasks".to_string(), "Coding".to_string()],
                cost_tier: 2,
            },
            ModelInfo {
                id: "gemini-2.5-flash-lite".to_string(),
                name: "Gemini 2.5 Flash-Lite".to_string(),
                provider_id: "gemini".to_string(),
                description: "Fastest and cheapest Gemini 2.5 model".to_string(),
                context_window: 1_048_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 65536,
                input_cost_per_1k: 0.0001,
                output_cost_per_1k: 0.0004,
                use_cases: vec!["Quick tasks".to_string(), "High-volume".to_string()],
                cost_tier: 1,
            },
        ]
    }

    /// Groq high-speed models
    // Model catalog — used by model selection UI and cost estimation
    pub fn groq_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "llama-3.1-70b-versatile".to_string(),
                name: "Llama 3.1 70B (Groq)".to_string(),
                provider_id: "groq".to_string(),
                description: "High-speed Llama 3.1 70B via Groq".to_string(),
                context_window: 128_000,
                supports_tools: true,
                supports_vision: false,
                max_tokens: 8192,
                input_cost_per_1k: 0.00059,
                output_cost_per_1k: 0.00079,
                use_cases: vec!["Fast response".to_string(), "Coding".to_string()],
                cost_tier: 2,
            },
            ModelInfo {
                id: "llama3-70b-8192".to_string(),
                name: "Llama 3 70B (Groq)".to_string(),
                provider_id: "groq".to_string(),
                description: "High-speed Llama 3 70B via Groq".to_string(),
                context_window: 8192,
                supports_tools: true,
                supports_vision: false,
                max_tokens: 8192,
                input_cost_per_1k: 0.00059,
                output_cost_per_1k: 0.00079,
                use_cases: vec!["Fast response".to_string()],
                cost_tier: 2,
            },
        ]
    }

    /// GitHub Copilot models
    // Model catalog — used by model selection UI and cost estimation
    pub fn copilot_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-5.4-copilot".to_string(),
                name: "GPT-5.4 (Copilot)".to_string(),
                provider_id: "copilot".to_string(),
                description: "GPT-5.4 with 1M context via GitHub Copilot".to_string(),
                context_window: 1_000_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 32768,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Coding".to_string(), "Architecture".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "gpt-5.5-copilot".to_string(),
                name: "GPT-5.5 (Copilot)".to_string(),
                provider_id: "copilot".to_string(),
                description: "Most capable OpenAI model via GitHub Copilot".to_string(),
                context_window: 1_000_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 32768,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Complex tasks".to_string(), "Reasoning".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "claude-sonnet-4-6-copilot".to_string(),
                name: "Claude Sonnet 4.6 (Copilot)".to_string(),
                provider_id: "copilot".to_string(),
                description: "Anthropic Claude Sonnet 4.6 via GitHub Copilot".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 16384,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Coding".to_string(), "Analysis".to_string()],
                cost_tier: 1,
            },
        ]
    }

    /// Zhipu AI (z.ai) GLM models
    // Model catalog — used by model selection UI and cost estimation
    pub fn zhipu_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "glm-5.1".to_string(),
                name: "GLM-5.1".to_string(),
                provider_id: "zhipu".to_string(),
                description: "Flagship — 8h autonomous work, matches Claude Opus 4.6".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 131_072,
                input_cost_per_1k: 0.005,
                output_cost_per_1k: 0.020,
                use_cases: vec![
                    "Long-horizon agents".to_string(),
                    "Complex reasoning".to_string(),
                    "Code generation".to_string(),
                ],
                cost_tier: 5,
            },
            ModelInfo {
                id: "glm-5".to_string(),
                name: "GLM-5".to_string(),
                provider_id: "zhipu".to_string(),
                description: "Strong coding, reliable multi-step reasoning".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 16_384,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                use_cases: vec![
                    "Complex reasoning".to_string(),
                    "Code generation".to_string(),
                    "Agent tasks".to_string(),
                ],
                cost_tier: 4,
            },
            ModelInfo {
                id: "glm-5-turbo".to_string(),
                name: "GLM-5 Turbo".to_string(),
                provider_id: "zhipu".to_string(),
                description: "Fast GLM-5 optimized for dynamic long-chain tasks".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 16_384,
                input_cost_per_1k: 0.002,
                output_cost_per_1k: 0.008,
                use_cases: vec![
                    "Quick reasoning".to_string(),
                    "Code generation".to_string(),
                    "Chat".to_string(),
                ],
                cost_tier: 3,
            },
            ModelInfo {
                id: "glm-4.7-flash".to_string(),
                name: "GLM-4.7 Flash".to_string(),
                provider_id: "zhipu".to_string(),
                description: "30B lightweight model, outperforms similar-scale open-source"
                    .to_string(),
                context_window: 128_000,
                supports_tools: true,
                supports_vision: false,
                max_tokens: 8192,
                input_cost_per_1k: 0.0001,
                output_cost_per_1k: 0.0001,
                use_cases: vec!["Fast response".to_string(), "Budget workloads".to_string()],
                cost_tier: 1,
            },
        ]
    }

    /// Default context window when model is unrecognized
    pub const DEFAULT_CONTEXT_WINDOW: usize = 100_000;

    /// Look up context window size for a model by its ID.
    ///
    /// Searches all predefined provider model lists synchronously.
    /// Returns [`DEFAULT_CONTEXT_WINDOW`] if the model is not found.
    pub fn context_window_for_model(model_id: &str) -> usize {
        let all_models: Vec<ModelInfo> = vec![
            anthropic_models(),
            openai_models(),
            openrouter_models(),
            gemini_models(),
            groq_models(),
            copilot_models(),
            zhipu_models(),
            ollama_models(),
            kimi_cn_models(),
            kimi_global_models(),
            alibaba_cn_models(),
            alibaba_global_models(),
            vertex_models(),
            litert_lm_models(),
        ]
        .into_iter()
        .flatten()
        .collect();

        // 1. Exact match
        if let Some(cw) = all_models
            .iter()
            .find(|m| m.id == model_id)
            .map(|m| m.context_window)
        {
            return cw;
        }

        // 2. Strip provider prefix (e.g., "zai-coding-plan/glm-5" → "glm-5")
        if let Some(short_id) = model_id.rsplit('/').next() {
            if short_id != model_id {
                if let Some(cw) = all_models
                    .iter()
                    .find(|m| m.id == short_id)
                    .map(|m| m.context_window)
                {
                    return cw;
                }
            }
        }

        // 3. Prefix match (e.g., "claude-3-5-sonnet-20241022" matches "claude-3-5-sonnet")
        if let Some(cw) = all_models
            .iter()
            .find(|m| model_id.starts_with(&m.id))
            .map(|m| m.context_window)
        {
            return cw;
        }

        DEFAULT_CONTEXT_WINDOW
    }

    /// Ollama local models
    // Model catalog — used by model selection UI and cost estimation
    pub fn ollama_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "llama3".to_string(),
                name: "Llama 3 (Ollama)".to_string(),
                provider_id: "ollama".to_string(),
                description: "Local Llama 3 model".to_string(),
                context_window: 128_000,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Local inference".to_string(), "Privacy".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "mistral".to_string(),
                name: "Mistral (Ollama)".to_string(),
                provider_id: "ollama".to_string(),
                description: "Local Mistral model".to_string(),
                context_window: 32_000,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Local inference".to_string(), "Fast response".to_string()],
                cost_tier: 1,
            },
        ]
    }

    /// Kimi/Moonshot AI China models
    // Model catalog — used by model selection UI and cost estimation
    pub fn kimi_cn_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "kimi-k2".to_string(),
                name: "Kimi K2".to_string(),
                provider_id: "kimi-cn".to_string(),
                description: "Most capable model for coding and complex tasks (China endpoint)"
                    .to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 8192,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                use_cases: vec![
                    "Code generation".to_string(),
                    "Complex reasoning".to_string(),
                    "Analysis".to_string(),
                ],
                cost_tier: 4,
            },
            ModelInfo {
                id: "kimi-latest".to_string(),
                name: "Kimi Latest".to_string(),
                provider_id: "kimi-cn".to_string(),
                description: "Latest stable Kimi model (China endpoint)".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 8192,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                use_cases: vec![
                    "General tasks".to_string(),
                    "Chat".to_string(),
                    "Code assistance".to_string(),
                ],
                cost_tier: 4,
            },
        ]
    }

    /// Kimi/Moonshot AI Global models
    // Model catalog — used by model selection UI and cost estimation
    pub fn kimi_global_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "kimi-k2".to_string(),
                name: "Kimi K2".to_string(),
                provider_id: "kimi-global".to_string(),
                description: "Most capable model for coding and complex tasks (Global endpoint)"
                    .to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 8192,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                use_cases: vec![
                    "Code generation".to_string(),
                    "Complex reasoning".to_string(),
                    "Analysis".to_string(),
                ],
                cost_tier: 4,
            },
            ModelInfo {
                id: "kimi-latest".to_string(),
                name: "Kimi Latest".to_string(),
                provider_id: "kimi-global".to_string(),
                description: "Latest stable Kimi model (Global endpoint)".to_string(),
                context_window: 200_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 8192,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                use_cases: vec![
                    "General tasks".to_string(),
                    "Chat".to_string(),
                    "Code assistance".to_string(),
                ],
                cost_tier: 4,
            },
        ]
    }

    /// Alibaba/DashScope China Qwen models
    // Model catalog — used by model selection UI and cost estimation
    pub fn alibaba_cn_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "qwen-max".to_string(),
                name: "Qwen Max".to_string(),
                provider_id: "alibaba-cn".to_string(),
                description: "Most capable Qwen model for complex tasks (China endpoint)"
                    .to_string(),
                context_window: 128_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 8192,
                input_cost_per_1k: 0.002,
                output_cost_per_1k: 0.006,
                use_cases: vec![
                    "Complex reasoning".to_string(),
                    "Analysis".to_string(),
                    "Creative writing".to_string(),
                ],
                cost_tier: 3,
            },
            ModelInfo {
                id: "qwen-coder-plus".to_string(),
                name: "Qwen Coder Plus".to_string(),
                provider_id: "alibaba-cn".to_string(),
                description: "Optimized for coding tasks (China endpoint)".to_string(),
                context_window: 128_000,
                supports_tools: true,
                supports_vision: false,
                max_tokens: 8192,
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.003,
                use_cases: vec![
                    "Code generation".to_string(),
                    "Code review".to_string(),
                    "Technical writing".to_string(),
                ],
                cost_tier: 3,
            },
        ]
    }

    /// Alibaba/DashScope Global Qwen models
    // Model catalog — used by model selection UI and cost estimation
    pub fn alibaba_global_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "qwen-max".to_string(),
                name: "Qwen Max".to_string(),
                provider_id: "alibaba-global".to_string(),
                description: "Most capable Qwen model for complex tasks (Global endpoint)"
                    .to_string(),
                context_window: 128_000,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 8192,
                input_cost_per_1k: 0.002,
                output_cost_per_1k: 0.006,
                use_cases: vec![
                    "Complex reasoning".to_string(),
                    "Analysis".to_string(),
                    "Creative writing".to_string(),
                ],
                cost_tier: 3,
            },
            ModelInfo {
                id: "qwen-coder-plus".to_string(),
                name: "Qwen Coder Plus".to_string(),
                provider_id: "alibaba-global".to_string(),
                description: "Optimized for coding tasks (Global endpoint)".to_string(),
                context_window: 128_000,
                supports_tools: true,
                supports_vision: false,
                max_tokens: 8192,
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.003,
                use_cases: vec![
                    "Code generation".to_string(),
                    "Code review".to_string(),
                    "Technical writing".to_string(),
                ],
                cost_tier: 3,
            },
        ]
    }

    /// Google Vertex AI Gemini models
    // Model catalog — used by model selection UI and cost estimation
    pub fn vertex_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gemini-2.5-pro".to_string(),
                name: "Gemini 2.5 Pro".to_string(),
                provider_id: "vertex".to_string(),
                description: "Google's most capable multimodal model via Vertex AI".to_string(),
                context_window: 1_048_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 65536,
                input_cost_per_1k: 0.00125,
                output_cost_per_1k: 0.01,
                use_cases: vec![
                    "Complex reasoning".to_string(),
                    "Multimodal tasks".to_string(),
                    "Long context".to_string(),
                    "Analysis".to_string(),
                ],
                cost_tier: 3,
            },
            ModelInfo {
                id: "gemini-2.5-flash".to_string(),
                name: "Gemini 2.5 Flash".to_string(),
                provider_id: "vertex".to_string(),
                description: "Fast and cost-effective multimodal model".to_string(),
                context_window: 1_048_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 65536,
                input_cost_per_1k: 0.0003,
                output_cost_per_1k: 0.0025,
                use_cases: vec![
                    "Fast response".to_string(),
                    "Cost-effective tasks".to_string(),
                    "Chat".to_string(),
                ],
                cost_tier: 2,
            },
            ModelInfo {
                id: "gemini-2.5-flash-lite".to_string(),
                name: "Gemini 2.5 Flash-Lite".to_string(),
                provider_id: "vertex".to_string(),
                description: "Lightest-cost Gemini model for high-volume tasks".to_string(),
                context_window: 1_048_576,
                supports_tools: true,
                supports_vision: true,
                max_tokens: 65536,
                input_cost_per_1k: 0.0001,
                output_cost_per_1k: 0.0004,
                use_cases: vec![
                    "High-volume tasks".to_string(),
                    "Classification".to_string(),
                    "Summarization".to_string(),
                ],
                cost_tier: 1,
            },
        ]
    }

    /// LiteRT-LM local models
    pub fn litert_lm_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gemma-4-e2b-it".to_string(),
                name: "Gemma 4 E2B (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description: "Gemma 4 2B parameter instruction-tuned model for local inference"
                    .to_string(),
                context_window: 8_192,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Local inference".to_string(), "Chat".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "gemma-4-e4b-it".to_string(),
                name: "Gemma 4 E4B (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description:
                    "Gemma 4 4B parameter instruction-tuned model, best quality for local inference"
                        .to_string(),
                context_window: 8_192,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec![
                    "Local inference".to_string(),
                    "Best local quality".to_string(),
                ],
                cost_tier: 1,
            },
            ModelInfo {
                id: "gemma3-1b".to_string(),
                name: "Gemma 3 1B (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description: "Lightweight Gemma 3 model for local inference".to_string(),
                context_window: 8_192,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Local inference".to_string(), "Privacy".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "gemma-3n-e2b".to_string(),
                name: "Gemma 3N E2B (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description: "Gemma 3N 2B parameter model for local inference".to_string(),
                context_window: 8_192,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Local inference".to_string(), "Fast response".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "gemma-3n-e4b".to_string(),
                name: "Gemma 3N E4B (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description: "Gemma 3N 4B parameter model for local inference".to_string(),
                context_window: 8_192,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec![
                    "Local inference".to_string(),
                    "Balanced quality".to_string(),
                ],
                cost_tier: 1,
            },
            ModelInfo {
                id: "phi-4-mini".to_string(),
                name: "Phi-4 Mini (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description: "Microsoft Phi-4 Mini for local inference".to_string(),
                context_window: 8_192,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Local inference".to_string(), "Coding".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "qwen2.5-1.5b".to_string(),
                name: "Qwen 2.5 1.5B (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description: "Alibaba Qwen 2.5 1.5B for local inference".to_string(),
                context_window: 8_192,
                supports_tools: false,
                supports_vision: false,
                max_tokens: 4096,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Local inference".to_string(), "Multilingual".to_string()],
                cost_tier: 1,
            },
            ModelInfo {
                id: "functiongemma-270m".to_string(),
                name: "FunctionGemma 270M (LiteRT-LM)".to_string(),
                provider_id: "litert-lm".to_string(),
                description: "Ultra-light function calling model for local inference".to_string(),
                context_window: 4_096,
                supports_tools: true,
                supports_vision: false,
                max_tokens: 2048,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
                use_cases: vec!["Function calling".to_string(), "Tool use".to_string()],
                cost_tier: 1,
            },
        ]
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
        assert_eq!(cw, 1_000_000);
    }

    #[test]
    fn test_predefined_context_window_unknown() {
        let cw = predefined::context_window_for_model("totally-unknown-model");
        assert_eq!(cw, predefined::DEFAULT_CONTEXT_WINDOW);
    }

    #[test]
    fn test_predefined_anthropic_models_count() {
        let models = predefined::anthropic_models();
        assert_eq!(models.len(), 3);
    }

    #[test]
    fn test_predefined_openai_models_count() {
        let models = predefined::openai_models();
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_predefined_gemini_models_count() {
        let models = predefined::gemini_models();
        assert_eq!(models.len(), 3);
    }

    #[test]
    fn test_predefined_ollama_models_count() {
        let models = predefined::ollama_models();
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_predefined_vertex_models_count() {
        let models = predefined::vertex_models();
        assert_eq!(models.len(), 3);
    }

    #[test]
    fn test_predefined_openrouter_models_count() {
        let models = predefined::openrouter_models();
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "anthropic/claude-sonnet-4-6"));
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
        // All current OpenAI models support vision
        assert!(models.iter().all(|m| m.supports_vision));
        // GPT-5.5 should be present and support vision
        let gpt55 = models.iter().find(|m| m.id == "gpt-5.5").unwrap();
        assert!(gpt55.supports_vision);
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
