#![allow(dead_code)]

//! Provider management helpers for the TUI
//!
//! Extracted from the TUI god object. Handles API key validation,
//! effort level cycling, and model switching logic.

use crate::ui::model_selector::ModelInfo;

/// Check for API key configuration issues and return a warning message.
///
/// Returns an empty string if no warning is needed.
pub fn compute_api_key_warning() -> String {
    if let Ok((provider_type, _, v2_config)) = rustycode_llm::load_provider_config_from_env() {
        let needs_api_key = !matches!(
            provider_type.to_lowercase().as_str(),
            "ollama" | "local" | "lmstudio" | ""
        );
        if needs_api_key && v2_config.api_key.is_none() {
            return format!(
                "⚠ No API key — set {} to get started",
                rustycode_config::api_key_env_name(&provider_type)
            );
        }
    }
    String::new()
}

/// Cycle through effort levels: low → medium → high → xhigh → max → low
///
/// Returns the new effort level string.
/// Skips "xhigh" if the model doesn't support it (currently only opus-4-7).
pub fn cycle_effort_level(current_effort: &str, current_model: &str) -> String {
    let all_levels = ["low", "medium", "high", "xhigh", "max"];
    let supports_xhigh =
        current_model.contains("opus-4-7") || current_model.contains("opus-4.7");

    let start_idx = all_levels
        .iter()
        .position(|&l| l == current_effort)
        .unwrap_or(1);

    let mut next_idx = (start_idx + 1) % all_levels.len();
    let mut attempts = 0;
    while !supports_xhigh && all_levels[next_idx] == "xhigh" && attempts < all_levels.len() {
        next_idx = (next_idx + 1) % all_levels.len();
        attempts += 1;
    }

    all_levels[next_idx].to_string()
}

/// Result of applying a model switch
pub struct ModelSwitchResult {
    /// Model identifier to set as current
    pub model_id: String,
    /// Provider name for the new model
    pub provider: String,
    /// Human-readable model name for display
    pub model_name: String,
    /// Status message to show the user
    pub status_message: String,
}

/// Compute the result of switching to a new model.
///
/// This is a pure function — it returns what SHOULD happen.
/// The caller (TUI) applies the changes to its own fields.
pub fn compute_model_switch(model: &ModelInfo) -> ModelSwitchResult {
    ModelSwitchResult {
        model_id: model.id.clone(),
        provider: model.provider.clone(),
        model_name: model.name.clone(),
        status_message: format!(
            "✓ Model switched to `{}` ({}). New requests will use this model.",
            model.name, model.provider
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_effort_medium_to_high() {
        assert_eq!(cycle_effort_level("medium", "claude-sonnet-4"), "high");
    }

    #[test]
    fn cycle_effort_max_wraps_to_low() {
        assert_eq!(cycle_effort_level("max", "claude-sonnet-4"), "low");
    }

    #[test]
    fn cycle_effort_skips_xhigh_for_non_opus() {
        assert_eq!(cycle_effort_level("high", "claude-sonnet-4"), "max");
    }

    #[test]
    fn cycle_effort_allows_xhigh_for_opus() {
        assert_eq!(cycle_effort_level("high", "claude-opus-4-7"), "xhigh");
    }

    #[test]
    fn cycle_effort_unknown_defaults_to_medium() {
        // "unknown" not in list, so position defaults to 1 (medium), next is high
        assert_eq!(cycle_effort_level("unknown", "claude-sonnet-4"), "high");
    }

    #[test]
    fn compute_model_switch_populates_result() {
        let model = ModelInfo::new(
            "claude-opus-4-7",
            "Claude Opus 4.7",
            "anthropic",
            "Most capable model",
        );
        let result = compute_model_switch(&model);
        assert_eq!(result.model_id, "claude-opus-4-7");
        assert_eq!(result.provider, "anthropic");
        assert_eq!(result.model_name, "Claude Opus 4.7");
        assert!(result.status_message.contains("Claude Opus 4.7"));
        assert!(result.status_message.contains("anthropic"));
    }
}
