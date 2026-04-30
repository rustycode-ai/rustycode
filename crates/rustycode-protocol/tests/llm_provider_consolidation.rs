#![allow(clippy::doc_markdown, clippy::float_cmp, clippy::unwrap_used)]
// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Integration tests for LLMProvider trait consolidation
//!
//! Tests verify the unified LLMProvider trait contract and ensure that
//! mock implementations work correctly with the trait interface.

use anyhow::Result;
use async_trait::async_trait;
use rustycode_protocol::llm::{
    CompletionRequest, CompletionResponse, Cost, LLMProvider, ModelInfo, TokenCount,
};

/// Mock provider for testing trait contract
struct MockProvider;

#[async_trait]
impl LLMProvider for MockProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![ModelInfo {
            name: "gpt-4".to_string(),
            provider: "openai".to_string(),
            context_window: 8192,
            supports_streaming: true,
            cost_per_1k_input_tokens: 0.03,
            cost_per_1k_output_tokens: 0.06,
        }])
    }

    async fn is_available(&self) -> Result<bool> {
        Ok(true)
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse {
            text: "test response".to_string(),
            tokens_used: TokenCount {
                input_tokens: 10,
                output_tokens: 20,
                total_tokens: 30,
            },
            cost: Cost {
                input_cost: 0.0003,
                output_cost: 0.0012,
                total_cost: 0.0015,
            },
            finish_reason: "stop".to_string(),
        })
    }

    fn name(&self) -> &'static str {
        "mock"
    }

    fn estimate_cost(&self, _request: &CompletionRequest) -> Result<Cost> {
        Ok(Cost {
            input_cost: 0.0,
            output_cost: 0.0,
            total_cost: 0.0,
        })
    }
}

#[tokio::test]
async fn test_llm_provider_trait_contract() {
    let provider = MockProvider;

    // Test list_models
    let models = provider.list_models().await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].provider, "openai");

    // Test is_available
    let available = provider.is_available().await.unwrap();
    assert!(available);

    // Test name
    assert_eq!(provider.name(), "mock");

    // Test estimate_cost
    let request = CompletionRequest {
        model: "test".to_string(),
        prompt: "test".to_string(),
        max_tokens: None,
        temperature: None,
        top_p: None,
        system: None,
    };
    let cost = provider.estimate_cost(&request).unwrap();
    assert_eq!(cost.total_cost, 0.0);
}

#[tokio::test]
async fn test_llm_provider_complete_returns_valid_response() {
    let provider = MockProvider;

    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        prompt: "Hello, world!".to_string(),
        max_tokens: Some(100),
        temperature: Some(0.7),
        top_p: Some(0.9),
        system: Some("You are helpful.".to_string()),
    };

    let response = provider.complete(request).await.unwrap();

    // Verify response structure
    assert_eq!(response.text, "test response");
    assert_eq!(response.tokens_used.input_tokens, 10);
    assert_eq!(response.tokens_used.output_tokens, 20);
    assert_eq!(response.tokens_used.total_tokens, 30);
    assert_eq!(response.cost.input_cost, 0.0003);
    assert_eq!(response.cost.output_cost, 0.0012);
    assert_eq!(response.cost.total_cost, 0.0015);
    assert_eq!(response.finish_reason, "stop");
}

#[tokio::test]
async fn test_llm_provider_model_info_structure() {
    let provider = MockProvider;

    let models = provider.list_models().await.unwrap();
    let model = &models[0];

    // Verify all fields are accessible and correct
    assert_eq!(model.name, "gpt-4");
    assert_eq!(model.provider, "openai");
    assert_eq!(model.context_window, 8192);
    assert!(model.supports_streaming);
    assert_eq!(model.cost_per_1k_input_tokens, 0.03);
    assert_eq!(model.cost_per_1k_output_tokens, 0.06);
}

#[test]
fn test_llm_provider_sync_methods() {
    let provider = MockProvider;

    // Test name method (synchronous)
    assert_eq!(provider.name(), "mock");

    // Test estimate_cost method (synchronous)
    let request = CompletionRequest {
        model: "test".to_string(),
        prompt: "test".to_string(),
        max_tokens: None,
        temperature: None,
        top_p: None,
        system: None,
    };

    let cost = provider.estimate_cost(&request).unwrap();
    assert_eq!(cost.input_cost, 0.0);
    assert_eq!(cost.output_cost, 0.0);
    assert_eq!(cost.total_cost, 0.0);
}

#[tokio::test]
async fn test_llm_provider_trait_object() {
    let provider: Box<dyn LLMProvider> = Box::new(MockProvider);

    // Test trait object methods
    let available = provider.is_available().await.unwrap();
    assert!(available);

    let name = provider.name();
    assert_eq!(name, "mock");

    let models = provider.list_models().await.unwrap();
    assert_eq!(models.len(), 1);
}

#[tokio::test]
async fn test_completion_request_with_all_fields() {
    let provider = MockProvider;

    let request = CompletionRequest {
        model: "claude-3-opus".to_string(),
        prompt: "What is 2+2?".to_string(),
        max_tokens: Some(50),
        temperature: Some(0.5),
        top_p: Some(0.95),
        system: Some("You are a mathematician.".to_string()),
    };

    let response = provider.complete(request).await.unwrap();
    assert!(!response.text.is_empty());
    assert!(response.tokens_used.total_tokens > 0);
}

#[tokio::test]
async fn test_completion_request_minimal_fields() {
    let provider = MockProvider;

    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        prompt: "Hello".to_string(),
        max_tokens: None,
        temperature: None,
        top_p: None,
        system: None,
    };

    let response = provider.complete(request).await.unwrap();
    assert_eq!(response.text, "test response");
}

#[tokio::test]
async fn test_cost_calculation_consistency() {
    let provider = MockProvider;

    let request = CompletionRequest {
        model: "test".to_string(),
        prompt: "test".to_string(),
        max_tokens: None,
        temperature: None,
        top_p: None,
        system: None,
    };

    // Test that estimate_cost returns consistent values
    let estimated = provider.estimate_cost(&request).unwrap();
    assert!(
        (estimated.total_cost - (estimated.input_cost + estimated.output_cost)).abs()
            < f64::EPSILON
    );

    // Execute completion and verify cost structure
    let response = provider.complete(request).await.unwrap();
    assert!(
        (response.cost.total_cost - (response.cost.input_cost + response.cost.output_cost)).abs()
            < 1e-10
    );
}

#[test]
fn test_cost_structure_validity() {
    let cost = Cost {
        input_cost: 0.001,
        output_cost: 0.002,
        total_cost: 0.003,
    };

    assert!(cost.input_cost >= 0.0);
    assert!(cost.output_cost >= 0.0);
    assert!(cost.total_cost >= 0.0);
    assert_eq!(cost.total_cost, cost.input_cost + cost.output_cost);
}

#[test]
fn test_token_count_structure_validity() {
    let tokens = TokenCount {
        input_tokens: 100,
        output_tokens: 50,
        total_tokens: 150,
    };

    assert_eq!(
        tokens.total_tokens,
        tokens.input_tokens + tokens.output_tokens
    );
}
