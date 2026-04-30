//! Error classification and recovery strategies for LLM providers.
use serde::{Deserialize, Serialize};

/// How to recover from a provider error
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RecoveryStrategy {
    /// Exponential backoff retry
    Retry {
        max_attempts: u32,
        base_delay_ms: u64,
    },
    /// Try alternate model names in order
    FallbackModel { models: Vec<String> },
    /// Return a static default instead of erroring
    UseDefault { default_response: String },
    /// Fail immediately, no recovery
    Fail,
}

/// Classified error kind
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    RateLimit,
    AuthError,
    NetworkError,
    ModelUnavailable,
    ContextTooLong,
    InvalidRequest,
    Unknown,
}

/// Classify an anyhow error into a recoverable kind
pub fn classify_error(err: &anyhow::Error) -> ErrorKind {
    let msg = err.to_string().to_lowercase();
    if msg.contains("429")
        || msg.contains("529")
        || msg.contains("rate limit")
        || msg.contains("too many requests")
        || msg.contains("overloaded")
    {
        ErrorKind::RateLimit
    } else if msg.contains("401")
        || msg.contains("403")
        || msg.contains("unauthorized")
        || msg.contains("api key")
    {
        ErrorKind::AuthError
    } else if msg.contains("context")
        && (msg.contains("limit") || msg.contains("too long") || msg.contains("maximum"))
    {
        ErrorKind::ContextTooLong
    } else if msg.contains("connection")
        || msg.contains("timeout")
        || msg.contains("network")
        || msg.contains("timed out")
    {
        ErrorKind::NetworkError
    } else if msg.contains("model")
        && (msg.contains("not found")
            || msg.contains("unavailable")
            || msg.contains("does not exist"))
    {
        ErrorKind::ModelUnavailable
    } else if msg.contains("400") || msg.contains("invalid") || msg.contains("bad request") {
        ErrorKind::InvalidRequest
    } else {
        ErrorKind::Unknown
    }
}

/// Get the default recovery strategy for a given error kind
pub fn default_strategy(kind: &ErrorKind) -> RecoveryStrategy {
    match kind {
        ErrorKind::RateLimit => RecoveryStrategy::Retry {
            max_attempts: 3,
            base_delay_ms: 5_000,
        },
        ErrorKind::NetworkError => RecoveryStrategy::Retry {
            max_attempts: 3,
            base_delay_ms: 1_000,
        },
        ErrorKind::ModelUnavailable => RecoveryStrategy::FallbackModel {
            models: vec![
                "llama2".to_string(),
                "mistral".to_string(),
                "codellama".to_string(),
            ],
        },
        ErrorKind::ContextTooLong => RecoveryStrategy::UseDefault {
            default_response:
                "The input is too long for this model. Please reduce the context size.".to_string(),
        },
        ErrorKind::AuthError | ErrorKind::InvalidRequest => RecoveryStrategy::Fail,
        ErrorKind::Unknown => RecoveryStrategy::Retry {
            max_attempts: 2,
            base_delay_ms: 500,
        },
    }
}

/// Execute an async operation, applying the given recovery strategy.
pub async fn with_recovery<F, Fut, T>(
    mut operation: F,
    strategy: &RecoveryStrategy,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    match strategy {
        RecoveryStrategy::Retry {
            max_attempts,
            base_delay_ms,
        } => {
            let mut delay_ms = *base_delay_ms;
            for attempt in 0..*max_attempts {
                match operation().await {
                    Ok(v) => return Ok(v),
                    Err(e) if attempt + 1 < *max_attempts => {
                        tracing::warn!(
                            "Attempt {}/{} failed: {}. Retrying in {}ms",
                            attempt + 1,
                            max_attempts,
                            e,
                            delay_ms
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        delay_ms = delay_ms.saturating_mul(2);
                    }
                    Err(e) => return Err(e),
                }
            }
            unreachable!()
        }
        RecoveryStrategy::UseDefault { default_response } => operation().await.map_err(|e| {
            tracing::warn!("Using default response due to error: {}", e);
            anyhow::anyhow!("Default: {}", default_response)
        }),
        RecoveryStrategy::FallbackModel { models } => operation()
            .await
            .map_err(|e| anyhow::anyhow!("Failed (tried fallback models {:?}): {}", models, e)),
        RecoveryStrategy::Fail => operation().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_rate_limit() {
        let err = anyhow::anyhow!("HTTP 429: too many requests");
        assert_eq!(classify_error(&err), ErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_overloaded_as_rate_limit() {
        let err = anyhow::anyhow!("Anthropic API error 529: overloaded");
        assert_eq!(classify_error(&err), ErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_auth() {
        let err = anyhow::anyhow!("HTTP 401: unauthorized");
        assert_eq!(classify_error(&err), ErrorKind::AuthError);
    }

    #[test]
    fn test_classify_network() {
        let err = anyhow::anyhow!("connection refused to localhost:11434");
        assert_eq!(classify_error(&err), ErrorKind::NetworkError);
    }

    #[test]
    fn test_classify_context() {
        let err = anyhow::anyhow!("context limit exceeded: 4096 tokens maximum");
        assert_eq!(classify_error(&err), ErrorKind::ContextTooLong);
    }

    #[test]
    fn test_default_strategy_rate_limit() {
        let s = default_strategy(&ErrorKind::RateLimit);
        assert!(matches!(
            s,
            RecoveryStrategy::Retry {
                max_attempts: 3,
                ..
            }
        ));
    }

    #[test]
    fn test_default_strategy_auth_fails() {
        let s = default_strategy(&ErrorKind::AuthError);
        assert_eq!(s, RecoveryStrategy::Fail);
    }

    #[tokio::test]
    async fn test_with_recovery_succeeds_first_try() {
        let strategy = RecoveryStrategy::Retry {
            max_attempts: 3,
            base_delay_ms: 1,
        };
        let result = with_recovery(|| async { Ok::<i32, anyhow::Error>(42) }, &strategy).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_recovery_fail_strategy() {
        let strategy = RecoveryStrategy::Fail;
        let result = with_recovery(
            || async { Err::<i32, _>(anyhow::anyhow!("hard fail")) },
            &strategy,
        )
        .await;
        assert!(result.is_err());
    }
}
