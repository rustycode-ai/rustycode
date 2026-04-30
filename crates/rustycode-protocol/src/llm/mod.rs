//! Unified LLM provider trait and types
//!
//! This module provides the unified LLMProvider trait that serves as the
//! single source of truth for all LLM provider implementations and consumers.
//!
//! The trait abstracts away provider-specific details while maintaining
//! consistency across different LLM backends (Anthropic, OpenAI, Azure, etc.).

pub mod types;

pub use types::{CompletionRequest, CompletionResponse, Cost, ModelInfo, TokenCount};

use anyhow::Result;
use async_trait::async_trait;

/// Unified interface for LLM providers
///
/// All LLM provider implementations (Anthropic, OpenAI, Azure, Bedrock, etc.)
/// must implement this trait to be compatible with the RustyCode system.
///
/// # Examples
///
/// ```ignore
/// use rustycode_protocol::LLMProvider;
///
/// let provider = get_provider("anthropic").await?;
///
/// // List available models
/// let models = provider.list_models().await?;
/// println!("Available models: {:?}", models);
///
/// // Check availability
/// if provider.is_available().await? {
///     // Execute a completion
///     let request = CompletionRequest {
///         model: "claude-3-opus".to_string(),
///         prompt: "Hello, world!".to_string(),
///         max_tokens: Some(100),
///         temperature: Some(0.7),
///         top_p: None,
///         system: Some("You are helpful.".to_string()),
///     };
///
///     let response = provider.complete(request).await?;
///     println!("Response: {}", response.text);
///     println!("Cost: ${:.6}", response.cost.total_cost);
/// }
/// ```
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// List all available models from this provider
    ///
    /// Returns a vector of ModelInfo structs describing each available model,
    /// including context window size, streaming support, and pricing information.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider cannot be queried (e.g., API unavailable).
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Check if this provider is available and properly configured
    ///
    /// This method should verify that:
    /// - API credentials are available
    /// - The provider service is reachable
    /// - Authentication is valid
    ///
    /// # Errors
    ///
    /// Returns an error if the provider is not available or not properly configured.
    async fn is_available(&self) -> Result<bool>;

    /// Execute a completion request against this provider
    ///
    /// Sends the request to the provider's API and returns the response with
    /// token counts and cost information.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The request is invalid
    /// - The API call fails
    /// - The model is not available
    /// - The provider is not authenticated
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Get the name of this provider
    ///
    /// Returns a static string identifying the provider (e.g., "anthropic", "openai").
    fn name(&self) -> &'static str;

    /// Estimate the cost of a completion request before executing it
    ///
    /// Useful for cost-aware decision making before making expensive API calls.
    /// This should be based on model pricing information and the estimated
    /// token counts without actually executing the request.
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not known or pricing is unavailable.
    fn estimate_cost(&self, request: &CompletionRequest) -> Result<Cost>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Ok;

    struct MockProvider;
    #[async_trait::async_trait]
    impl LLMProvider for MockProvider {
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
        let p: &dyn LLMProvider = &MockProvider;
        assert!(p.is_available().await.unwrap());
        assert_eq!(p.name(), "mock");
    }

    #[tokio::test]
    async fn mock_provider_returns_echo() {
        let p: &dyn LLMProvider = &MockProvider;
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
