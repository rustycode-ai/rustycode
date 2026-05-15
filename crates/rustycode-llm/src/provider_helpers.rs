//! Helper functions for easy provider and model selection
//!
//! Simplified API for common provider/model operations.

use crate::registry::{ModelTier, ProviderMetadataRegistry, TaskModelConfig, TaskType};

/// Singleton registry instance
static REGISTRY: std::sync::OnceLock<ProviderMetadataRegistry> = std::sync::OnceLock::new();

/// Get the global provider registry
pub fn registry() -> &'static ProviderMetadataRegistry {
    REGISTRY.get_or_init(ProviderMetadataRegistry::new)
}

/// Find a provider by ID
///
/// # Examples
/// ```
/// # use rustycode_llm::provider_helpers::find_provider;
/// let anthropic = find_provider("anthropic");
/// assert!(anthropic.is_some());
/// ```
pub fn find_provider(id: &str) -> Option<String> {
    registry().provider(id).map(|p| p.id.clone())
}

/// Find a model and its provider
///
/// # Examples
/// ```
/// # use rustycode_llm::provider_helpers::find_model_provider;
/// let (model, provider) = find_model_provider("claude-sonnet-4-6").unwrap();
/// assert_eq!(model, "claude-sonnet-4-6");
/// assert_eq!(provider, "anthropic");
/// ```
pub fn find_model_provider(model: &str) -> Option<(String, String)> {
    let registry = registry();
    let provider = registry.provider_for_model(model)?;
    Some((model.to_string(), provider.id.clone()))
}

/// Get all available provider IDs
pub fn list_providers() -> Vec<String> {
    registry()
        .all_providers()
        .into_iter()
        .map(|p| p.id.clone())
        .collect()
}

/// Get all available model IDs
pub fn list_models() -> Vec<String> {
    registry()
        .all_models()
        .into_iter()
        .map(|m| m.id.clone())
        .collect()
}

/// Get models for a specific provider
///
/// # Examples
/// ```
/// # use rustycode_llm::provider_helpers::list_provider_models;
/// let models = list_provider_models("anthropic");
/// assert!(models.contains(&"claude-sonnet-4-6".to_string()));
/// ```
pub fn list_provider_models(provider_id: &str) -> Vec<String> {
    registry()
        .models_for_provider(provider_id)
        .into_iter()
        .map(|m| m.id.clone())
        .collect()
}

/// Get cheapest available model
///
/// # Examples
/// ```
/// # use rustycode_llm::provider_helpers::cheapest_model;
/// let model = cheapest_model().unwrap();
/// // Returns cheapest model (usually haiku/mini variant)
/// ```
pub fn cheapest_model() -> Option<String> {
    registry().cheapest_model().map(|m| m.id.clone())
}

/// Get default model
pub fn default_model() -> Option<String> {
    registry().default_model().map(|m| m.id.clone())
}

/// Get all models of a specific tier
///
/// # Examples
/// ```
/// # use rustycode_llm::{provider_helpers::models_by_tier, registry::ModelTier};
/// let budget = models_by_tier(ModelTier::Budget);
/// assert!(!budget.is_empty());
/// ```
pub fn models_by_tier(tier: ModelTier) -> Vec<String> {
    registry()
        .models_by_tier(tier)
        .into_iter()
        .map(|m| m.id.clone())
        .collect()
}

/// Select model for a specific task using default configuration
///
/// # Examples
/// ```
/// # use rustycode_llm::{provider_helpers::select_model, TaskType};
/// let model = select_model(TaskType::Planning).unwrap();
/// // Returns claude-3-opus by default (most capable for planning)
/// ```
pub fn select_model(task: TaskType) -> Option<String> {
    let config = TaskModelConfig::default();
    registry()
        .select_model_for_task(task, &config)
        .map(|m| m.id.clone())
}

/// Select model for a task with custom configuration
///
/// # Examples
/// ```
/// # use rustycode_llm::{provider_helpers::select_model_with_config, TaskType, TaskModelConfig};
/// let mut config = TaskModelConfig::default();
/// // Customize config...
/// let model = select_model_with_config(TaskType::CodeGeneration, &config).unwrap();
/// ```
pub fn select_model_with_config(task: TaskType, config: &TaskModelConfig) -> Option<String> {
    registry()
        .select_model_for_task(task, config)
        .map(|m| m.id.clone())
}

/// Check if a model is available
///
/// # Examples
/// ```
/// # use rustycode_llm::provider_helpers::is_model_available;
/// assert!(is_model_available("claude-sonnet-4-6"));
/// assert!(!is_model_available("nonexistent-model"));
/// ```
pub fn is_model_available(model: &str) -> bool {
    registry().provider_for_model(model).is_some()
}

/// Check if a provider is available
pub fn is_provider_available(provider_id: &str) -> bool {
    registry().provider(provider_id).is_some()
}

/// Get cost estimate for using a model
///
/// Returns (cost_per_1m_input, cost_per_1m_output) in USD
///
/// # Examples
/// ```
/// # use rustycode_llm::provider_helpers::model_cost;
/// let (input_cost, output_cost) = model_cost("claude-sonnet-4-6").unwrap();
/// println!("Input: ${}/1M tokens", input_cost);
/// println!("Output: ${}/1M tokens", output_cost);
/// ```
pub fn model_cost(model: &str) -> Option<(f64, f64)> {
    let registry = registry();
    let model_info = registry.all_models().into_iter().find(|m| m.id == model)?;
    Some((model_info.cost_per_1m_input, model_info.cost_per_1m_output))
}

/// Get context window for a model
///
/// # Examples
/// ```
/// # use rustycode_llm::provider_helpers::context_window;
/// let ctx = context_window("claude-sonnet-4-6").unwrap();
/// assert_eq!(ctx, 1_000_000);
/// ```
pub fn context_window(model: &str) -> Option<usize> {
    let registry = registry();
    registry
        .all_models()
        .into_iter()
        .find(|m| m.id == model)
        .map(|m| m.context_window)
}

/// Get provider info as JSON
pub fn provider_info_json(id: &str) -> Option<serde_json::Value> {
    let registry = registry();
    registry.provider(id).map(|p| {
        serde_json::json!({
            "id": p.id,
            "name": p.name,
            "description": p.description,
            "api_url": p.api_url,
            "api_key_env": p.api_key_env,
            "supports_streaming": p.supports_streaming,
            "supports_tools": p.supports_tools,
            "rate_limit_rpm": p.rate_limit_rpm,
            "models": p.models.iter().map(|m| &m.id).collect::<Vec<_>>(),
        })
    })
}

/// Get all providers as JSON
pub fn all_providers_json() -> serde_json::Value {
    let registry = registry();
    serde_json::json!(registry
        .all_providers()
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "name": p.name,
                "model_count": p.models.len(),
            })
        })
        .collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_provider() {
        assert!(find_provider("anthropic").is_some());
        assert!(find_provider("invalid").is_none());
    }

    #[test]
    fn test_find_model_provider() {
        let (model, provider) = find_model_provider("claude-sonnet-4-6").unwrap();
        assert_eq!(model, "claude-sonnet-4-6");
        assert_eq!(provider, "anthropic");
    }

    #[test]
    fn test_list_providers() {
        let providers = list_providers();
        assert!(providers.contains(&"anthropic".to_string()));
        assert!(providers.contains(&"openai".to_string()));
    }

    #[test]
    fn test_list_models() {
        let models = list_models();
        assert!(models.contains(&"claude-sonnet-4-6".to_string()));
        assert!(models.contains(&"gpt-4o".to_string()));
    }

    #[test]
    fn test_list_provider_models() {
        let models = list_provider_models("anthropic");
        assert!(models.contains(&"claude-sonnet-4-6".to_string()));
    }

    #[test]
    fn test_is_model_available() {
        assert!(is_model_available("claude-sonnet-4-6"));
        assert!(!is_model_available("fake-model"));
    }

    #[test]
    fn test_get_cheapest_model() {
        let model = cheapest_model();
        assert!(model.is_some());
    }

    #[test]
    fn test_select_model() {
        let model = select_model(TaskType::CodeGeneration);
        assert!(model.is_some());
    }

    #[test]
    fn test_get_model_cost() {
        let (input, output) = model_cost("claude-sonnet-4-6").unwrap();
        assert!(input > 0.0);
        assert!(output > 0.0);
    }

    #[test]
    fn test_get_context_window() {
        let ctx = context_window("claude-sonnet-4-6").unwrap();
        assert!(ctx >= 200000);
    }
}
