//! Provider Registry - Centralized management of all LLM providers and models
//!
//! Single source of truth for:
//! - Available providers and their metadata
//! - Models supported by each provider
//! - Default models for different use cases
//! - Easy provider/model switching and discovery

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Comprehensive provider information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    /// Provider ID (e.g., "anthropic", "openai")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Provider description
    pub description: String,

    /// API endpoint URL
    pub api_url: String,

    /// Environment variable for API key
    pub api_key_env: String,

    /// Supported models
    pub models: Vec<ModelInfo>,

    /// Default model for this provider
    pub default_model: String,

    /// Whether provider supports streaming
    pub supports_streaming: bool,

    /// Whether provider supports tool calling
    pub supports_tools: bool,

    /// Rate limit info (requests per minute)
    pub rate_limit_rpm: Option<u32>,
}

/// Information about a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID/name
    pub id: String,

    /// Human-readable display name
    pub name: String,

    /// Model description
    pub description: String,

    /// Context window size
    pub context_window: usize,

    /// Whether model supports vision
    pub supports_vision: bool,

    /// Whether model supports tool calling
    pub supports_tools: bool,

    /// Cost per 1M input tokens (in USD)
    pub cost_per_1m_input: f64,

    /// Cost per 1M output tokens (in USD)
    pub cost_per_1m_output: f64,

    /// Release date (YYYY-MM-DD)
    pub release_date: String,

    /// Model tier for routing decisions
    pub tier: ModelTier,
}

/// Model tier for cost-aware routing
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ModelTier {
    /// Fast, cheap models (e.g., Claude Haiku, GPT-4o Mini)
    Budget,
    /// Balanced models (e.g., Claude Sonnet, GPT-4)
    Balanced,
    /// Most capable models (e.g., Claude Opus, o1)
    Premium,
}

impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelTier::Budget => write!(f, "budget"),
            ModelTier::Balanced => write!(f, "balanced"),
            ModelTier::Premium => write!(f, "premium"),
            #[allow(unreachable_patterns)]
            _ => write!(f, "unknown"),
        }
    }
}

/// Task type for model selection
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TaskType {
    /// Code analysis and understanding
    CodeAnalysis,
    /// Code generation
    CodeGeneration,
    /// Planning and architecture
    Planning,
    /// Testing and validation
    Testing,
    /// General conversation
    General,
    /// Research and documentation
    Research,
    /// Specialized domain work
    Specialized,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::CodeAnalysis => write!(f, "code_analysis"),
            TaskType::CodeGeneration => write!(f, "code_generation"),
            TaskType::Planning => write!(f, "planning"),
            TaskType::Testing => write!(f, "testing"),
            TaskType::General => write!(f, "general"),
            TaskType::Research => write!(f, "research"),
            TaskType::Specialized => write!(f, "specialized"),
            #[allow(unreachable_patterns)]
            _ => write!(f, "unknown"),
        }
    }
}

/// Configuration for task-specific model selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskModelConfig {
    /// Default model for each task type
    pub task_models: HashMap<TaskType, String>,
    /// Fallback chain: [preferred, fallback1, fallback2]
    pub fallback_chain: Vec<String>,
    /// Global default model
    pub default_model: String,
}

impl Default for TaskModelConfig {
    fn default() -> Self {
        let mut task_models = HashMap::new();
        // Default task-model mappings (can be overridden)
        task_models.insert(TaskType::CodeGeneration, "claude-sonnet-4-6".to_string());
        task_models.insert(TaskType::CodeAnalysis, "claude-sonnet-4-6".to_string());
        task_models.insert(TaskType::Planning, "claude-opus-4-7".to_string());
        task_models.insert(TaskType::Testing, "claude-haiku-4-5-20251001".to_string());
        task_models.insert(TaskType::General, "claude-sonnet-4-6".to_string());
        task_models.insert(TaskType::Research, "claude-opus-4-7".to_string());
        task_models.insert(TaskType::Specialized, "claude-sonnet-4-6".to_string());

        Self {
            task_models,
            fallback_chain: vec![
                "claude-sonnet-4-6".to_string(),
                "claude-opus-4-7".to_string(),
                "gpt-4.1".to_string(),
            ],
            default_model: "claude-sonnet-4-6".to_string(),
        }
    }
}

/// Provider registry - centralized provider management
pub struct ProviderMetadataRegistry {
    providers: HashMap<String, ProviderMetadata>,
    model_to_provider: HashMap<String, String>,
}

impl ProviderMetadataRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            model_to_provider: HashMap::new(),
        };
        registry.register_all_providers();
        registry
    }

    /// Register all built-in providers from the model catalog
    fn register_all_providers(&mut self) {
        fn tier_for(output_cost_per_1m: f64) -> ModelTier {
            if output_cost_per_1m <= 1.0 {
                ModelTier::Budget
            } else if output_cost_per_1m <= 10.0 {
                ModelTier::Balanced
            } else {
                ModelTier::Premium
            }
        }

        // (registry_id, catalog_provider_id, display_name, api_url, api_key_env, default_model, streaming, tools, rpm)
        let configs: &[(&str, &str, &str, &str, &str, &str, bool, bool, Option<u32>)] = &[
            (
                "anthropic",
                "anthropic",
                "Anthropic",
                "https://api.anthropic.com",
                "ANTHROPIC_API_KEY",
                "claude-sonnet-4-6",
                true,
                true,
                Some(50),
            ),
            (
                "openai",
                "openai",
                "OpenAI",
                "https://api.openai.com",
                "OPENAI_API_KEY",
                "gpt-5.4",
                true,
                true,
                Some(90),
            ),
            (
                "google",
                "gemini",
                "Google Gemini",
                "https://generativelanguage.googleapis.com",
                "GOOGLE_API_KEY",
                "gemini-2.5-flash",
                true,
                true,
                Some(60),
            ),
            (
                "ollama",
                "ollama",
                "Ollama",
                "http://localhost:11434",
                "OLLAMA_API_KEY",
                "qwen2.5-coder",
                true,
                false,
                None,
            ),
        ];

        for &(id, catalog_id, name, api_url, api_key_env, default_model, streaming, tools, rpm) in
            configs
        {
            let models: Vec<ModelInfo> =
                rustycode_providers::model_catalog::models_for_provider(catalog_id)
                    .iter()
                    .map(|e| ModelInfo {
                        id: e.id.to_string(),
                        name: e.id.to_string(),
                        description: format!("{} model", e.id),
                        context_window: e.context_window,
                        supports_vision: e.supports_vision,
                        supports_tools: e.supports_tools,
                        cost_per_1m_input: e.input_cost_per_1m,
                        cost_per_1m_output: e.output_cost_per_1m,
                        release_date: String::new(),
                        tier: tier_for(e.output_cost_per_1m),
                    })
                    .collect();

            if !models.is_empty() {
                self.register_provider(ProviderMetadata {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: String::new(),
                    api_url: api_url.to_string(),
                    api_key_env: api_key_env.to_string(),
                    models,
                    default_model: default_model.to_string(),
                    supports_streaming: streaming,
                    supports_tools: tools,
                    rate_limit_rpm: rpm,
                });
            }
        }
    }

    /// Register a provider
    fn register_provider(&mut self, provider: ProviderMetadata) {
        // Map each model to this provider
        for model in &provider.models {
            self.model_to_provider
                .insert(model.id.clone(), provider.id.clone());
        }
        self.providers.insert(provider.id.clone(), provider);
    }

    /// Get all providers
    pub fn all_providers(&self) -> Vec<&ProviderMetadata> {
        self.providers.values().collect()
    }

    /// Get a specific provider
    pub fn provider(&self, id: &str) -> Option<&ProviderMetadata> {
        self.providers.get(id)
    }

    /// Get provider for a model
    pub fn provider_for_model(&self, model: &str) -> Option<&ProviderMetadata> {
        let provider_id = self.model_to_provider.get(model)?;
        self.providers.get(provider_id)
    }

    /// Get all models
    pub fn all_models(&self) -> Vec<&ModelInfo> {
        self.providers
            .values()
            .flat_map(|p| p.models.iter())
            .collect()
    }

    /// Get models for a provider
    pub fn models_for_provider(&self, provider_id: &str) -> Vec<&ModelInfo> {
        self.providers
            .get(provider_id)
            .map(|p| p.models.iter().collect())
            .unwrap_or_default()
    }

    /// Get models by tier
    pub fn models_by_tier(&self, tier: ModelTier) -> Vec<&ModelInfo> {
        self.all_models()
            .into_iter()
            .filter(|m| m.tier == tier)
            .collect()
    }

    /// Get cheapest model
    pub fn cheapest_model(&self) -> Option<&ModelInfo> {
        self.models_by_tier(ModelTier::Budget).first().copied()
    }

    /// Get default model
    pub fn default_model(&self) -> Option<&ModelInfo> {
        let default_id = "claude-sonnet-4-6";
        self.all_models().into_iter().find(|m| m.id == default_id)
    }

    /// Select model for a task
    pub fn select_model_for_task(
        &self,
        task: TaskType,
        config: &TaskModelConfig,
    ) -> Option<&ModelInfo> {
        // Get configured model for this task
        let model_id = config
            .task_models
            .get(&task)
            .or(Some(&config.default_model))?;

        self.all_models().into_iter().find(|m| &m.id == model_id)
    }
}

impl Default for ProviderMetadataRegistry {
    fn default() -> Self {
        Self::new()
    }
}

use crate::provider::{LLMProvider, ProviderError};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn LLMProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, provider: Arc<dyn LLMProvider>) {
        self.providers
            .write()
            .await
            .insert(provider.name().to_string(), provider);
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn LLMProvider>> {
        self.providers.read().await.get(name).cloned()
    }

    pub async fn contains(&self, name: &str) -> bool {
        self.providers.read().await.contains_key(name)
    }

    pub async fn list_providers(&self) -> Vec<String> {
        self.providers.read().await.keys().cloned().collect()
    }

    pub async fn list_available(&self) -> Vec<String> {
        let providers = self.providers.read().await;
        let mut available = Vec::new();

        for (name, provider) in providers.iter() {
            if provider.is_available().await {
                available.push(name.clone());
            }
        }

        available
    }

    pub async fn unregister(&self, name: &str) -> bool {
        self.providers.write().await.remove(name).is_some()
    }

    pub async fn count(&self) -> usize {
        self.providers.read().await.len()
    }

    pub async fn clear(&self) {
        self.providers.write().await.clear();
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ProviderRegistryBuilder {
    registry: ProviderRegistry,
}

impl ProviderRegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: ProviderRegistry::new(),
        }
    }

    pub async fn with_provider(self, provider: Arc<dyn LLMProvider>) -> Self {
        self.registry.register(provider).await;
        self
    }

    pub fn build(self) -> ProviderRegistry {
        self.registry
    }

    pub async fn try_with_provider(
        self,
        provider: Arc<dyn LLMProvider>,
    ) -> Result<Self, ProviderError> {
        self.registry.register(provider).await;
        Ok(self)
    }
}

impl Default for ProviderRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_registry_creation() {
        let registry = ProviderMetadataRegistry::new();
        assert!(!registry.all_providers().is_empty());
    }

    #[test]
    fn test_get_provider() {
        let registry = ProviderMetadataRegistry::new();
        let anthropic = registry.provider("anthropic");
        assert!(anthropic.is_some());
        assert_eq!(anthropic.unwrap().name, "Anthropic");
    }

    #[test]
    fn test_get_all_models() {
        let registry = ProviderMetadataRegistry::new();
        let models = registry.all_models();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_get_provider_for_model() {
        let registry = ProviderMetadataRegistry::new();
        let provider = registry.provider_for_model("claude-sonnet-4-6");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().id, "anthropic");
    }

    #[test]
    fn test_get_models_by_tier() {
        let registry = ProviderMetadataRegistry::new();
        let budget_models = registry.models_by_tier(ModelTier::Budget);
        assert!(!budget_models.is_empty());

        let premium_models = registry.models_by_tier(ModelTier::Premium);
        assert!(!premium_models.is_empty());
    }

    #[test]
    fn test_task_model_config_default() {
        let config = TaskModelConfig::default();
        assert_eq!(config.default_model, "claude-sonnet-4-6");
        assert!(config.task_models.contains_key(&TaskType::CodeGeneration));
    }

    #[test]
    fn test_select_model_for_task() {
        let registry = ProviderMetadataRegistry::new();
        let config = TaskModelConfig::default();

        let model = registry.select_model_for_task(TaskType::CodeGeneration, &config);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "claude-sonnet-4-6");
    }
}
