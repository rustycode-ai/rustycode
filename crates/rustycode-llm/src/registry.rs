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
        task_models.insert(TaskType::CodeGeneration, "claude-3-5-sonnet".to_string());
        task_models.insert(TaskType::CodeAnalysis, "claude-3-5-sonnet".to_string());
        task_models.insert(TaskType::Planning, "claude-3-opus".to_string());
        task_models.insert(TaskType::Testing, "claude-3-haiku".to_string());
        task_models.insert(TaskType::General, "claude-3-5-sonnet".to_string());
        task_models.insert(TaskType::Research, "claude-3-opus".to_string());
        task_models.insert(TaskType::Specialized, "claude-3-5-sonnet".to_string());

        Self {
            task_models,
            fallback_chain: vec![
                "claude-3-5-sonnet".to_string(),
                "claude-3-opus".to_string(),
                "gpt-4".to_string(),
            ],
            default_model: "claude-3-5-sonnet".to_string(),
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

    /// Register all built-in providers
    fn register_all_providers(&mut self) {
        // Anthropic
        self.register_provider(ProviderMetadata {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            description: "Claude models".to_string(),
            api_url: "https://api.anthropic.com".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            models: vec![
                ModelInfo {
                    id: "claude-3-opus".to_string(),
                    name: "Claude 3 Opus".to_string(),
                    description: "Most capable model".to_string(),
                    context_window: 200000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 15.0,
                    cost_per_1m_output: 75.0,
                    release_date: "2024-03-04".to_string(),
                    tier: ModelTier::Premium,
                },
                ModelInfo {
                    id: "claude-3-5-sonnet".to_string(),
                    name: "Claude 3.5 Sonnet".to_string(),
                    description: "Balanced and capable".to_string(),
                    context_window: 200000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 3.0,
                    cost_per_1m_output: 15.0,
                    release_date: "2024-06-20".to_string(),
                    tier: ModelTier::Balanced,
                },
                ModelInfo {
                    id: "claude-3-haiku".to_string(),
                    name: "Claude 3 Haiku".to_string(),
                    description: "Fast and cheap".to_string(),
                    context_window: 200000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 0.80,
                    cost_per_1m_output: 4.0,
                    release_date: "2024-03-04".to_string(),
                    tier: ModelTier::Budget,
                },
            ],
            default_model: "claude-3-5-sonnet".to_string(),
            supports_streaming: true,
            supports_tools: true,
            rate_limit_rpm: Some(50),
        });

        // OpenAI
        self.register_provider(ProviderMetadata {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            description: "GPT models".to_string(),
            api_url: "https://api.openai.com".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            models: vec![
                ModelInfo {
                    id: "gpt-4".to_string(),
                    name: "GPT-4".to_string(),
                    description: "Most capable GPT".to_string(),
                    context_window: 128000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 10.0,
                    cost_per_1m_output: 30.0,
                    release_date: "2023-06-27".to_string(),
                    tier: ModelTier::Premium,
                },
                ModelInfo {
                    id: "gpt-4-turbo".to_string(),
                    name: "GPT-4 Turbo".to_string(),
                    description: "Improved GPT-4".to_string(),
                    context_window: 128000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 10.0,
                    cost_per_1m_output: 30.0,
                    release_date: "2023-11-06".to_string(),
                    tier: ModelTier::Premium,
                },
                ModelInfo {
                    id: "gpt-4o".to_string(),
                    name: "GPT-4o".to_string(),
                    description: "Optimized GPT-4".to_string(),
                    context_window: 128000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 5.0,
                    cost_per_1m_output: 15.0,
                    release_date: "2024-05-13".to_string(),
                    tier: ModelTier::Balanced,
                },
                ModelInfo {
                    id: "gpt-4o-mini".to_string(),
                    name: "GPT-4o Mini".to_string(),
                    description: "Lightweight GPT-4o".to_string(),
                    context_window: 128000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 0.15,
                    cost_per_1m_output: 0.60,
                    release_date: "2024-07-18".to_string(),
                    tier: ModelTier::Budget,
                },
            ],
            default_model: "gpt-4o".to_string(),
            supports_streaming: true,
            supports_tools: true,
            rate_limit_rpm: Some(90),
        });

        // Google Gemini
        self.register_provider(ProviderMetadata {
            id: "google".to_string(),
            name: "Google Gemini".to_string(),
            description: "Gemini models".to_string(),
            api_url: "https://generativelanguage.googleapis.com".to_string(),
            api_key_env: "GOOGLE_API_KEY".to_string(),
            models: vec![
                ModelInfo {
                    id: "gemini-2.0-flash".to_string(),
                    name: "Gemini 2.0 Flash".to_string(),
                    description: "Fast Gemini 2.0".to_string(),
                    context_window: 1000000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 0.075,
                    cost_per_1m_output: 0.3,
                    release_date: "2024-12-19".to_string(),
                    tier: ModelTier::Balanced,
                },
                ModelInfo {
                    id: "gemini-1.5-pro".to_string(),
                    name: "Gemini 1.5 Pro".to_string(),
                    description: "Advanced Gemini".to_string(),
                    context_window: 1000000,
                    supports_vision: true,
                    supports_tools: true,
                    cost_per_1m_input: 1.25,
                    cost_per_1m_output: 5.0,
                    release_date: "2024-05-14".to_string(),
                    tier: ModelTier::Premium,
                },
            ],
            default_model: "gemini-2.0-flash".to_string(),
            supports_streaming: true,
            supports_tools: true,
            rate_limit_rpm: Some(60),
        });

        // Ollama (local)
        self.register_provider(ProviderMetadata {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            description: "Local models via Ollama".to_string(),
            api_url: "http://localhost:11434".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            models: vec![
                ModelInfo {
                    id: "llama2".to_string(),
                    name: "Llama 2".to_string(),
                    description: "Meta's Llama 2".to_string(),
                    context_window: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    cost_per_1m_input: 0.0,
                    cost_per_1m_output: 0.0,
                    release_date: "2023-07-18".to_string(),
                    tier: ModelTier::Budget,
                },
                ModelInfo {
                    id: "mistral".to_string(),
                    name: "Mistral".to_string(),
                    description: "Mistral AI".to_string(),
                    context_window: 8192,
                    supports_vision: false,
                    supports_tools: false,
                    cost_per_1m_input: 0.0,
                    cost_per_1m_output: 0.0,
                    release_date: "2023-09-27".to_string(),
                    tier: ModelTier::Balanced,
                },
            ],
            default_model: "mistral".to_string(),
            supports_streaming: true,
            supports_tools: false,
            rate_limit_rpm: None,
        });
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
        let default_id = "claude-3-5-sonnet";
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
        let provider = registry.provider_for_model("claude-3-5-sonnet");
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
        assert_eq!(config.default_model, "claude-3-5-sonnet");
        assert!(config.task_models.contains_key(&TaskType::CodeGeneration));
    }

    #[test]
    fn test_select_model_for_task() {
        let registry = ProviderMetadataRegistry::new();
        let config = TaskModelConfig::default();

        let model = registry.select_model_for_task(TaskType::CodeGeneration, &config);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "claude-3-5-sonnet");
    }
}
