// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Property-based tests for configuration system
//!
//! Uses proptest to verify invariants and properties

use proptest::prelude::*;
use rustycode_config::{Config, ProvidersConfig, FeaturesConfig, AdvancedConfig};
use std::collections::HashMap;

// Generate a valid model name
fn arb_model_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("claude-sonnet-4-6".to_string()),
        Just("gpt-4o".to_string()),
        Just("gemini-pro".to_string()),
        Just("ollama/llama2".to_string()),
        "[a-z0-9-]{5,50}",
    ]
}

// Generate valid temperature
fn arb_temperature() -> impl Strategy<Value = Option<f32>> {
    prop_oneof![
        Just(None),
        (0.0f32..2.0f32).prop_map(|t| Some(t)),
    ]
}

// Generate valid max_tokens
fn arb_max_tokens() -> impl Strategy<Value = Option<usize>> {
    prop_oneof![
        Just(None),
        (128usize..8192usize).prop_map(|t| Some(t)),
    ]
}

proptest! {
    #[test]
    fn config_roundtrip Serialization(
        model in arb_model_name(),
        temperature in arb_temperature(),
        max_tokens in arb_max_tokens(),
    ) {
        let config = Config {
            model: model.clone(),
            temperature,
            max_tokens,
            ..Default::default()
        };

        // Serialize
        let json = serde_json::to_string(&config).unwrap();

        // Deserialize
        let deserialized: Config = serde_json::from_str(&json).unwrap();

        // Verify roundtrip
        prop_assert_eq!(deserialized.model, model);
        prop_assert_eq!(deserialized.temperature, temperature);
        prop_assert_eq!(deserialized.max_tokens, max_tokens);
    }

    #[test]
    fn config_default_is_valid() {
        let config = Config::default();

        // Default config should have required fields
        prop_assert!(!config.model.is_empty());
        prop_assert!(config.providers.anthropic.is_none());
        prop_assert!(config.providers.openai.is_none());
    }

    #[test]
    fn providers_config_handles_custom_providers(
        custom_key in "[a-z]{3,10}",
        custom_value in "[a-zA-Z0-9]{10,50}"
    ) {
        let mut providers = ProvidersConfig::default();
        let mut custom = HashMap::new();
        custom.insert(custom_key.clone(), serde_json::json!(custom_value));

        providers.custom = custom;

        // Serialize and deserialize
        let json = serde_json::to_string(&providers).unwrap();
        let deserialized: ProvidersConfig = serde_json::from_str(&json).unwrap();

        prop_assert!(deserialized.custom.contains_key(&custom_key));
    }

    #[test]
    fn features_config_defaults_to_disabled() {
        let features = FeaturesConfig::default();

        prop_assert!(!features.git_integration);
        prop_assert!(!features.file_watcher);
        prop_assert!(features.mcp_servers.is_empty());
        prop_assert!(features.agents.is_empty());
    }

    #[test]
    fn advanced_config_handles_experimental(
        exp_key in "[a-z_]{3,15}",
        exp_value in any::<serde_json::Value>()
    ) {
        let mut advanced = AdvancedConfig::default();
        let mut experimental = HashMap::new();
        experimental.insert(exp_key.clone(), exp_value.clone());

        advanced.experimental = experimental;

        // Serialize and deserialize
        let json = serde_json::to_string(&advanced).unwrap();
        let deserialized: AdvancedConfig = serde_json::from_str(&json).unwrap();

        prop_assert!(deserialized.experimental.contains_key(&exp_key));
        prop_assert_eq!(deserialized.experimental[&exp_key], exp_value);
    }

    #[test]
    fn config_merge_preserves_required_fields(
        model1 in arb_model_name(),
        model2 in arb_model_name(),
    ) {
        let mut config1 = Config {
            model: model1.clone(),
            ..Default::default()
        };

        let config2 = Config {
            model: model2.clone(),
            ..Default::default()
        };

        // Simulate merge (override model)
        config1.model = config2.model.clone();

        // Should preserve config2's model
        prop_assert_eq!(config1.model, model2);
    }

    #[test]
    fn temperature_in_valid_range(temp in arb_temperature()) {
        if let Some(t) = temp {
            prop_assert!(t >= 0.0);
            prop_assert!(t <= 2.0);
        }
    }

    #[test]
    fn max_tokens_in_valid_range(tokens in arb_max_tokens()) {
        if let Some(t) = tokens {
            prop_assert!(t >= 128);
            prop_assert!(t <= 8192);
        }
    }

    #[test]
    fn config_json_is_valid(
        model in arb_model_name(),
        temperature in arb_temperature(),
        max_tokens in arb_max_tokens(),
    ) {
        let config = Config {
            model,
            temperature,
            max_tokens,
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        prop_assert!(parsed.is_object());

        // Should have model field
        prop_assert!(parsed.get("model").is_some());
    }

    #[test]
    fn features_config_merge_replaces_arrays(
        servers1 in prop::collection::vec("[a-z]{3,10}", 0..5),
        servers2 in prop::collection::vec("[a-z]{3,10}", 0..5),
        agents1 in prop::collection::vec("[a-z]{3,10}", 0..5),
        agents2 in prop::collection::vec("[a-z]{3,10}", 0..5),
    ) {
        let mut features1 = FeaturesConfig {
            mcp_servers: servers1.clone(),
            agents: agents1.clone(),
            ..Default::default()
        };

        let features2 = FeaturesConfig {
            mcp_servers: servers2.clone(),
            agents: agents2.clone(),
            ..Default::default()
        };

        // Merge (override arrays)
        features1.mcp_servers = features2.mcp_servers.clone();
        features1.agents = features2.agents.clone();

        // Arrays should be replaced, not merged
        prop_assert_eq!(features1.mcp_servers, servers2);
        prop_assert_eq!(features1.agents, agents2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_immutability() {
        let config1 = Config {
            model: "test-model".to_string(),
            ..Default::default()
        };

        let config2 = config1.clone();

        // Modify config2
        let mut config2_modified = config2;
        config2_modified.model = "modified-model".to_string();

        // Original should be unchanged
        assert_eq!(config1.model, "test-model");
        assert_eq!(config2.model, "test-model");
        assert_eq!(config2_modified.model, "modified-model");
    }
}
