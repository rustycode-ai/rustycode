//! Unified LLM provider trait and types
//!
//! This module provides the UnifiedLLMProvider trait — a simplified interface
//! that some providers implement as a compatibility layer. The primary LLM
//! provider trait lives in `crate::provider::LLMProvider`.
//!
//! This module was migrated from `rustycode-protocol` to keep protocol
//! focused on pure data types.

pub mod types;

pub use types::{CompletionRequest, CompletionResponse, Cost, ModelInfo, TokenCount, Usage};

use anyhow::Result;
use async_trait::async_trait;

/// Simplified LLM provider interface (compatibility trait).
///
/// Some providers implement this in addition to the main `LLMProvider` trait
/// in `crate::provider`. The main trait supports streaming and richer error
/// types; this trait provides a simpler request/response model.
#[async_trait]
pub trait UnifiedLLMProvider: Send + Sync {
    /// List all available models from this provider.
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Check if this provider is available and properly configured.
    async fn is_available(&self) -> Result<bool>;

    /// Execute a completion request against this provider.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Get the name of this provider (e.g., "anthropic", "openai").
    fn name(&self) -> &'static str;

    /// Estimate the cost of a completion request before executing it.
    fn estimate_cost(&self, request: &CompletionRequest) -> Result<Cost>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Ok;

    struct MockProvider;
    #[async_trait::async_trait]
    impl UnifiedLLMProvider for MockProvider {
        fn name(&self) -> &'static str {
            "mock"
        }

        async fn is_available(&self) -> Result<bool> {
            Ok(true)
        }

        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                text: format!("Echo: {}", request.prompt),
                tokens_used: TokenCount {
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                },
                cost: Cost {
                    input_cost: 0.0,
                    output_cost: 0.0,
                    total_cost: 0.0,
                },
                finish_reason: "stop".to_string(),
            })
        }

        fn estimate_cost(&self, _request: &CompletionRequest) -> Result<Cost> {
            Ok(Cost {
                input_cost: 0.0,
                output_cost: 0.0,
                total_cost: 0.0,
            })
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn mock_provider_implements_trait() {
        let p: &dyn UnifiedLLMProvider = &MockProvider;
        assert!(p.is_available().await.unwrap());
        assert_eq!(p.name(), "mock");
    }

    #[tokio::test]
    async fn mock_provider_returns_echo() {
        let p: &dyn UnifiedLLMProvider = &MockProvider;
        let request = CompletionRequest {
            model: "test".to_string(),
            prompt: "hello".to_string(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            system: None,
        };
        let response = p.complete(request).await.unwrap();
        assert_eq!(response.text, "Echo: hello");
    }
}
