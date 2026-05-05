// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Provider integration tests
//!
//! Tests cover:
//! - Provider bootstrap and auto-discovery
//! - Model registry and listing
//! - Cost tracking and accumulation
//! - Multi-provider usage
//! - Provider capabilities

use rustycode_providers::{
    bootstrap_from_env, CostTracker, ModelInfo, ModelRegistry, ProviderMetadata,
};

mod common;
use common::TestEnv;

#[tokio::test]
async fn test_provider_bootstrap() {
    let _test_env = TestEnv::new();

    // Bootstrap providers from environment (graceful: no keys required)
    let registry = bootstrap_from_env().await;

    // Registry should always be usable (even if empty)
    let provider_count = registry.count().await;
    // No assertions on count — env may or may not have keys set
    let _ = provider_count;
}

#[tokio::test]
async fn test_model_registry() {
    let registry = ModelRegistry::new();

    let test_provider = ProviderMetadata {
        id: "test-provider".to_string(),
        name: "Test Provider".to_string(),
        base_url: "https://api.test.com".to_string(),
        api_key_env: "TEST_API_KEY".to_string(),
        auth_method: rustycode_providers::AuthMethod::ApiKey,
        capabilities: rustycode_providers::ProviderCapabilities {
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: false,
            max_tokens: 4096,
            max_context_window: 128_000,
        },
        pricing: rustycode_providers::PricingInfo {
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
            currency: rustycode_providers::Currency::Usd,
        },
    };

    registry.register_provider(test_provider).await;

    assert!(registry.has_provider("test-provider").await);

    let provider = registry.provider("test-provider").await;
    assert!(provider.is_some());
    let provider = provider.unwrap();
    assert_eq!(provider.id, "test-provider");
    assert_eq!(provider.name, "Test Provider");

    let providers = registry.list_providers().await;
    assert!(providers.contains(&"test-provider".to_string()));
}

#[tokio::test]
async fn test_model_listing() {
    let registry = ModelRegistry::new();

    let anthropic = rustycode_providers::providers::anthropic();
    registry.register_provider(anthropic).await;

    // Register Anthropic models
    for model in rustycode_providers::predefined::anthropic_models() {
        registry.register_model("anthropic", model).await;
    }

    let models = registry.list_models("anthropic").await;
    assert!(!models.is_empty(), "Should have at least one model");

    for model in &models {
        assert!(!model.id.is_empty());
        assert!(!model.name.is_empty());
        assert!(model.input_cost_per_1k >= 0.0);
        assert!(model.output_cost_per_1k >= 0.0);
    }
}

#[tokio::test]
async fn test_cost_tracking() {
    let cost_tracker = CostTracker::new();

    cost_tracker
        .track("anthropic/claude-3-5-sonnet", 1000, 500, 0.0105)
        .await;

    cost_tracker
        .track("anthropic/claude-3-5-sonnet", 2000, 1000, 0.021)
        .await;

    let summary = cost_tracker.summary().await;

    assert_eq!(summary.total_input_tokens, 3000);
    assert_eq!(summary.total_output_tokens, 1500);
    assert!(summary.total_cost > 0.0);

    // Verify by-provider breakdown
    assert!(summary.by_provider.contains_key("anthropic"));
    let anthropic = summary.by_provider.get("anthropic").unwrap();
    assert_eq!(anthropic.input_tokens, 3000);
    assert_eq!(anthropic.output_tokens, 1500);
}

#[tokio::test]
async fn test_multi_provider_usage() {
    let registry = ModelRegistry::new();

    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;
    registry
        .register_provider(rustycode_providers::providers::openai())
        .await;
    registry
        .register_provider(rustycode_providers::providers::ollama(
            "http://localhost:11434",
        ))
        .await;

    let cost_tracker = CostTracker::new();

    cost_tracker
        .track("anthropic/claude-3-5-sonnet", 1000, 500, 0.0105)
        .await;

    cost_tracker.track("openai/gpt-4o", 2000, 1000, 0.025).await;

    cost_tracker.track("ollama/llama2", 500, 250, 0.0).await;

    let summary = cost_tracker.summary().await;
    assert_eq!(summary.by_provider.len(), 3);

    assert!(summary.by_provider.contains_key("anthropic"));
    assert!(summary.by_provider.contains_key("openai"));

    let ollama = summary.by_provider.get("ollama").unwrap();
    assert_eq!(ollama.total_cost, 0.0);
}

#[tokio::test]
async fn test_provider_capabilities() {
    let registry = ModelRegistry::new();

    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;
    registry
        .register_provider(rustycode_providers::providers::openai())
        .await;

    let anthropic = registry.provider("anthropic").await.unwrap();
    assert!(anthropic.capabilities.supports_streaming);
    assert!(anthropic.capabilities.supports_function_calling);
    assert!(anthropic.capabilities.supports_vision);
    assert_eq!(anthropic.capabilities.max_context_window, 200_000);

    let openai = registry.provider("openai").await.unwrap();
    assert!(openai.capabilities.supports_streaming);
    assert!(openai.capabilities.supports_function_calling);
    assert!(openai.capabilities.supports_vision);
    assert_eq!(openai.capabilities.max_context_window, 128_000);
}

#[tokio::test]
async fn test_model_info_structure() {
    let model = ModelInfo {
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        provider_id: "test-provider".to_string(),
        description: "A test model".to_string(),
        context_window: 128_000,
        supports_tools: true,
        supports_vision: false,
        max_tokens: 4096,
        input_cost_per_1k: 0.003,
        output_cost_per_1k: 0.015,
        use_cases: vec!["Testing".to_string()],
        cost_tier: 2,
    };

    assert_eq!(model.id, "test-model");
    assert_eq!(model.name, "Test Model");
    assert_eq!(model.provider_id, "test-provider");
    assert_eq!(model.input_cost_per_1k, 0.003);
    assert_eq!(model.output_cost_per_1k, 0.015);
    assert!(model.supports_tools);
    assert!(!model.supports_vision);
}

#[tokio::test]
async fn test_cost_accumulation_accuracy() {
    let cost_tracker = CostTracker::new();

    let input_tokens = 1000u64;
    let output_tokens = 500u64;

    // Anthropic: $0.003/1k input, $0.015/1k output
    let input_cost = (input_tokens as f64 / 1000.0) * 0.003;
    let output_cost = (output_tokens as f64 / 1000.0) * 0.015;
    let total = input_cost + output_cost;

    cost_tracker
        .track(
            "anthropic/claude-3-5-sonnet",
            input_tokens,
            output_tokens,
            total,
        )
        .await;

    let summary = cost_tracker.summary().await;

    assert!(
        (summary.total_cost - total).abs() < 0.0001,
        "Cost mismatch: expected {}, got {}",
        total,
        summary.total_cost
    );
}

#[tokio::test]
async fn test_provider_pricing_currency() {
    let registry = ModelRegistry::new();

    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;
    registry
        .register_provider(rustycode_providers::providers::openai())
        .await;

    let anthropic = registry.provider("anthropic").await.unwrap();
    assert_eq!(
        anthropic.pricing.currency,
        rustycode_providers::Currency::Usd
    );

    let openai = registry.provider("openai").await.unwrap();
    assert_eq!(openai.pricing.currency, rustycode_providers::Currency::Usd);
}

#[tokio::test]
async fn test_cost_reset() {
    let cost_tracker = CostTracker::new();

    cost_tracker
        .track("anthropic/claude-3-5-sonnet", 1000, 500, 0.0105)
        .await;

    let summary1 = cost_tracker.summary().await;
    assert!(summary1.total_cost > 0.0);

    cost_tracker.reset().await;

    let summary2 = cost_tracker.summary().await;
    assert_eq!(summary2.total_input_tokens, 0);
    assert_eq!(summary2.total_output_tokens, 0);
    assert_eq!(summary2.total_cost, 0.0);
}

#[tokio::test]
async fn test_model_registry_persistence() {
    let registry = ModelRegistry::new();

    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;

    assert!(registry.has_provider("anthropic").await);
    assert_eq!(registry.count().await, 1);

    let providers = registry.list_providers().await;
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0], "anthropic");
}

#[tokio::test]
async fn test_unknown_provider_handling() {
    let registry = ModelRegistry::new();

    let unknown = registry.provider("unknown-provider").await;
    assert!(unknown.is_none());

    let models = registry.list_models("unknown-provider").await;
    assert!(models.is_empty());
}
