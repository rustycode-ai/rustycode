//! Provider Fallback Chain — Multi-provider resilience
//!
//! Provides automatic fallback to the next provider when a request fails.
//! Supports configurable retry policies and provider ordering.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::provider::{CompletionRequest, CompletionResponse, LLMProvider};

/// Retry policy for provider fallback
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RetryPolicy {
    /// No retry, fail immediately
    None,
    /// Retry immediately once
    Immediate,
    /// Exponential backoff with configurable params
    ExponentialBackoff {
        max_retries: u32,
        base_delay_ms: u64,
        max_delay_ms: u64,
    },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::ExponentialBackoff {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
        }
    }
}

impl RetryPolicy {
    /// Get the delay before the nth retry attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> Option<Duration> {
        match self {
            Self::None => None,
            Self::Immediate => {
                if attempt == 0 {
                    Some(Duration::ZERO)
                } else {
                    None
                }
            }
            Self::ExponentialBackoff {
                max_retries,
                base_delay_ms,
                max_delay_ms,
            } => {
                if attempt >= *max_retries {
                    return None;
                }
                let delay_ms = base_delay_ms.saturating_mul(2u64.saturating_pow(attempt));
                Some(Duration::from_millis(delay_ms.min(*max_delay_ms)))
            }
        }
    }
}

/// Result of a fallback chain execution
#[derive(Clone, Debug)]
pub struct FallbackResult {
    pub result: CompletionResponse,
    pub provider_used: String,
    pub attempts: Vec<FallbackAttempt>,
}

/// Record of a single fallback attempt
#[derive(Clone, Debug)]
pub struct FallbackAttempt {
    pub provider_name: String,
    pub succeeded: bool,
    pub error: Option<String>,
    pub attempt_number: u32,
}

/// Provider fallback chain — tries providers in order on failure
pub struct ProviderFallbackChain {
    providers: Vec<Box<dyn LLMProvider>>,
    fallback_enabled: bool,
    retry_policy: RetryPolicy,
}

impl ProviderFallbackChain {
    pub fn new(providers: Vec<Box<dyn LLMProvider>>, retry_policy: RetryPolicy) -> Self {
        Self {
            providers,
            fallback_enabled: true,
            retry_policy,
        }
    }

    /// Create with fallback disabled (uses only primary provider)
    pub fn no_fallback(primary: Box<dyn LLMProvider>) -> Self {
        Self {
            providers: vec![primary],
            fallback_enabled: false,
            retry_policy: RetryPolicy::None,
        }
    }

    /// Enable or disable fallback
    pub fn set_fallback_enabled(&mut self, enabled: bool) {
        self.fallback_enabled = enabled;
    }

    /// Check if fallback is enabled
    pub fn is_fallback_enabled(&self) -> bool {
        self.fallback_enabled
    }

    /// Get number of providers in the chain
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Execute a request with automatic fallback
    pub async fn execute(&self, request: CompletionRequest) -> anyhow::Result<FallbackResult> {
        let mut attempts = Vec::new();

        for (i, provider) in self.providers.iter().enumerate() {
            let provider_name = provider.name().to_string();
            let is_last = i == self.providers.len() - 1;

            // Apply retry policy for this provider
            let mut retry_attempt: u32 = 0;
            loop {
                match provider.complete(request.clone()).await {
                    Ok(response) => {
                        attempts.push(FallbackAttempt {
                            provider_name: provider_name.clone(),
                            succeeded: true,
                            error: None,
                            attempt_number: retry_attempt,
                        });
                        return Ok(FallbackResult {
                            result: response,
                            provider_used: provider_name,
                            attempts,
                        });
                    }
                    Err(e) => {
                        let error_msg = e.to_string();

                        // Check if we should retry this provider
                        if let Some(delay) = self.retry_policy.delay_for_attempt(retry_attempt) {
                            attempts.push(FallbackAttempt {
                                provider_name: provider_name.clone(),
                                succeeded: false,
                                error: Some(error_msg.clone()),
                                attempt_number: retry_attempt,
                            });
                            retry_attempt += 1;
                            if !delay.is_zero() {
                                tokio::time::sleep(delay).await;
                            }
                            continue;
                        }

                        // No more retries for this provider
                        attempts.push(FallbackAttempt {
                            provider_name: provider_name.clone(),
                            succeeded: false,
                            error: Some(error_msg.clone()),
                            attempt_number: retry_attempt,
                        });

                        // Try next provider if fallback is enabled
                        if !is_last && self.fallback_enabled {
                            tracing::warn!(
                                "Provider '{}' failed, trying next: {}",
                                provider_name,
                                error_msg
                            );
                            break; // Move to next provider
                        } else {
                            return Err(anyhow::anyhow!(
                                "All providers exhausted. Last error from '{}': {}",
                                provider_name,
                                error_msg
                            ));
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!("No providers available in fallback chain"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{LLMProvider, ProviderError, StreamChunk};
    use async_trait::async_trait;
    use futures::Stream;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    /// Mock provider for testing
    struct MockProvider {
        name: String,
        should_fail: Arc<Mutex<bool>>,
    }

    impl MockProvider {
        fn new(name: &str, should_fail: bool) -> Self {
            Self {
                name: name.to_string(),
                should_fail: Arc::new(Mutex::new(should_fail)),
            }
        }
    }

    #[async_trait]
    impl LLMProvider for MockProvider {
        fn name(&self) -> &'static str {
            // Leak to get 'static lifetime (test-only)
            Box::leak(self.name.clone().into_boxed_str())
        }

        async fn is_available(&self) -> bool {
            true
        }

        async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
            Ok(vec!["test-model".to_string()])
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            let should_fail = *self
                .should_fail
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if should_fail {
                Err(ProviderError::RateLimited {
                    retry_delay: Some(Duration::from_secs(1)),
                })
            } else {
                Ok(CompletionResponse {
                    content: format!("response from {}", self.name),
                    model: self.name.clone(),
                    usage: None,
                    stop_reason: Some("stop".to_string()),
                    citations: None,
                    thinking_blocks: None,
                    structured_output: None,
                })
            }
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
            Err(ProviderError::Unknown("not implemented".to_string()))
        }
    }

    fn simple_request() -> CompletionRequest {
        CompletionRequest::new("test-model", vec![])
    }

    #[tokio::test]
    async fn single_provider_succeeds() {
        let provider = MockProvider::new("primary", false);
        let chain = ProviderFallbackChain::new(vec![Box::new(provider)], RetryPolicy::None);

        let result = chain.execute(simple_request()).await.unwrap();
        assert_eq!(result.provider_used, "primary");
        assert!(result.result.content.contains("primary"));
        assert_eq!(result.attempts.len(), 1);
        assert!(result.attempts[0].succeeded);
    }

    #[tokio::test]
    async fn fallback_to_second_provider() {
        let failing = MockProvider::new("failing", true);
        let working = MockProvider::new("working", false);
        let chain = ProviderFallbackChain::new(
            vec![Box::new(failing), Box::new(working)],
            RetryPolicy::None,
        );

        let result = chain.execute(simple_request()).await.unwrap();
        assert_eq!(result.provider_used, "working");
        assert_eq!(result.attempts.len(), 2);
        assert!(!result.attempts[0].succeeded);
        assert!(result.attempts[1].succeeded);
    }

    #[tokio::test]
    async fn all_providers_fail() {
        let p1 = MockProvider::new("p1", true);
        let p2 = MockProvider::new("p2", true);
        let chain = ProviderFallbackChain::new(vec![Box::new(p1), Box::new(p2)], RetryPolicy::None);

        let result = chain.execute(simple_request()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("All providers exhausted"));
    }

    #[tokio::test]
    async fn no_fallback_uses_primary_only() {
        let failing = MockProvider::new("primary", true);
        let chain = ProviderFallbackChain::no_fallback(Box::new(failing));

        let result = chain.execute(simple_request()).await;
        assert!(result.is_err());
        assert!(!chain.is_fallback_enabled());
    }

    #[test]
    fn retry_policy_none_no_delay() {
        let policy = RetryPolicy::None;
        assert!(policy.delay_for_attempt(0).is_none());
    }

    #[test]
    fn retry_policy_immediate_one_retry() {
        let policy = RetryPolicy::Immediate;
        assert_eq!(policy.delay_for_attempt(0), Some(Duration::ZERO));
        assert!(policy.delay_for_attempt(1).is_none());
    }

    #[test]
    fn retry_policy_exponential_backoff() {
        let policy = RetryPolicy::ExponentialBackoff {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 10_000,
        };
        assert_eq!(
            policy.delay_for_attempt(0),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            policy.delay_for_attempt(1),
            Some(Duration::from_millis(200))
        );
        assert_eq!(
            policy.delay_for_attempt(2),
            Some(Duration::from_millis(400))
        );
        assert!(policy.delay_for_attempt(3).is_none());
    }

    #[test]
    fn retry_policy_respects_max_delay() {
        let policy = RetryPolicy::ExponentialBackoff {
            max_retries: 10,
            base_delay_ms: 1000,
            max_delay_ms: 5000,
        };
        let delay = policy.delay_for_attempt(5).unwrap();
        assert!(delay <= Duration::from_secs(5));
    }

    #[test]
    fn retry_policy_default() {
        let policy = RetryPolicy::default();
        match policy {
            RetryPolicy::ExponentialBackoff { max_retries, .. } => {
                assert_eq!(max_retries, 3);
            }
            _ => panic!("Expected ExponentialBackoff"),
        }
    }

    #[test]
    fn set_fallback_enabled() {
        let provider = MockProvider::new("p", false);
        let mut chain = ProviderFallbackChain::new(vec![Box::new(provider)], RetryPolicy::None);
        assert!(chain.is_fallback_enabled());
        chain.set_fallback_enabled(false);
        assert!(!chain.is_fallback_enabled());
    }

    #[test]
    fn provider_count() {
        let p1 = MockProvider::new("p1", false);
        let p2 = MockProvider::new("p2", false);
        let chain = ProviderFallbackChain::new(vec![Box::new(p1), Box::new(p2)], RetryPolicy::None);
        assert_eq!(chain.provider_count(), 2);
    }

    #[test]
    fn fallback_attempt_tracking() {
        let attempt = FallbackAttempt {
            provider_name: "test".to_string(),
            succeeded: true,
            error: None,
            attempt_number: 0,
        };
        assert!(attempt.succeeded);
        assert!(attempt.error.is_none());
    }
}
