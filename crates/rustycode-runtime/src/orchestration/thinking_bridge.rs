//! Thinking mode bridge — converts TaskThinkingProfile to provider ThinkingConfig.
//!
//! This module handles the mapping from task-level thinking preferences to
//! LLM provider thinking configurations, gating by model capability and
//! enforcing mutual exclusion between Graph-of-Thoughts and native thinking.

use rustycode_llm::provider::{ThinkingConfig, ThinkingDisplay, ThinkingType};
use rustycode_protocol::task_routing::{TaskThinkingMode, TaskThinkingProfile};

/// Maps a TaskThinkingProfile to an optional ThinkingConfig based on model capabilities.
///
/// # Arguments
///
/// * `profile` - The task thinking profile specifying depth and style
/// * `model_name` - The model identifier (e.g., "claude-sonnet-4-6")
///
/// # Returns
///
/// `Some(ThinkingConfig)` if the profile requests thinking and the model supports it,
/// `None` if the profile is Standard or the model doesn't support thinking.
///
/// # Mapping Rules
///
/// - `Standard` → `None` (no extended thinking)
/// - `Deep` → `Adaptive` if model supports thinking, else `None`
/// - `Extended` → `Enabled` with 20K token budget if model supports thinking, else `None`
///
/// # Model Support
///
/// Thinking is supported by:
/// - Claude Opus 4.5+, Sonnet 4.5+ (claude-opus-4-*, claude-sonnet-4-*)
/// - GPT-5.x family (gpt-5-pro, gpt-5.1, gpt-5.2)
pub fn task_thinking_profile_to_config(
    profile: &TaskThinkingProfile,
    model_name: &str,
) -> Option<ThinkingConfig> {
    // First check if the model supports thinking
    let supports_thinking = model_supports_thinking(model_name);

    match profile.depth {
        TaskThinkingMode::Standard => None,
        TaskThinkingMode::Deep => {
            if supports_thinking {
                Some(ThinkingConfig {
                    thinking_type: ThinkingType::Adaptive,
                    display: Some(ThinkingDisplay::Summarized),
                    budget_tokens: None,
                })
            } else {
                None
            }
        }
        TaskThinkingMode::Extended => {
            if supports_thinking {
                Some(ThinkingConfig {
                    thinking_type: ThinkingType::Enabled,
                    display: Some(ThinkingDisplay::Summarized),
                    budget_tokens: Some(30_000),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Checks if a model supports extended thinking capability.
///
/// # Arguments
///
/// * `model_name` - The model identifier to check
///
/// # Returns
///
/// `true` if the model supports extended thinking, `false` otherwise.
fn model_supports_thinking(model_name: &str) -> bool {
    // Anthropic models: Claude Opus 4.5+, Sonnet 4.5+
    if model_name.starts_with("claude-") {
        // Claude models 4.5 and later support thinking
        model_name.contains("opus-4-") || model_name.contains("sonnet-4-")
    } else if model_name.starts_with("gpt-5") {
        // GPT-5.x models support thinking
        true
    } else {
        // For unknown models, conservatively assume no thinking support
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_depth_never_enables_thinking() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Standard,
            ..Default::default()
        };

        assert_eq!(
            task_thinking_profile_to_config(&profile, "claude-opus-4-6"),
            None
        );
        assert_eq!(
            task_thinking_profile_to_config(&profile, "gpt-5-pro"),
            None
        );
    }

    #[test]
    fn deep_depth_adaptive_when_supported() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Deep,
            ..Default::default()
        };

        let config = task_thinking_profile_to_config(&profile, "claude-opus-4-6");
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.thinking_type, ThinkingType::Adaptive);
        assert_eq!(config.budget_tokens, None);
    }

    #[test]
    fn deep_depth_none_when_unsupported() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Deep,
            ..Default::default()
        };

        assert_eq!(
            task_thinking_profile_to_config(&profile, "claude-haiku-4-5"),
            None
        );
        assert_eq!(
            task_thinking_profile_to_config(&profile, "gpt-4o"),
            None
        );
    }

    #[test]
    fn extended_depth_enabled_with_budget() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Extended,
            ..Default::default()
        };

        let config = task_thinking_profile_to_config(&profile, "claude-sonnet-4-6");
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.thinking_type, ThinkingType::Enabled);
        assert_eq!(config.budget_tokens, Some(30_000));
    }

    #[test]
    fn extended_depth_none_when_unsupported() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Extended,
            ..Default::default()
        };

        assert_eq!(
            task_thinking_profile_to_config(&profile, "llama3.1:8b"),
            None
        );
    }

    #[test]
    fn model_support_detection() {
        // Opus and Sonnet 4.x support thinking
        assert!(model_supports_thinking("claude-opus-4-6"));
        assert!(model_supports_thinking("claude-sonnet-4-6"));
        assert!(model_supports_thinking("claude-opus-4-7"));

        // Haiku and older Claude don't support thinking
        assert!(!model_supports_thinking("claude-haiku-4-5"));
        assert!(!model_supports_thinking("claude-3-opus"));

        // GPT-5.x supports thinking
        assert!(model_supports_thinking("gpt-5-pro"));
        assert!(model_supports_thinking("gpt-5.1"));
        assert!(model_supports_thinking("gpt-5.2"));

        // Other OpenAI models don't (in our mapping)
        assert!(!model_supports_thinking("gpt-4o"));
        assert!(!model_supports_thinking("gpt-4o-mini"));

        // Unknown models conservatively don't support
        assert!(!model_supports_thinking("unknown-model-xyz"));
    }

    #[test]
    fn thinking_display_is_summarized() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Deep,
            ..Default::default()
        };

        let config = task_thinking_profile_to_config(&profile, "claude-opus-4-6");
        assert!(config.is_some());
        assert_eq!(config.unwrap().display, Some(ThinkingDisplay::Summarized));
    }
}
