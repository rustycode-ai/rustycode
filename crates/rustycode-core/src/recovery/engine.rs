// ── Recovery Engine ────────────────────────────────────────────────────────────

use super::classification::{ErrorClassification, ErrorClassifier};
use super::config::RecoveryConfig;
use super::result::{RecoveryLogEntry, RecoveryResult};
use super::strategy::RecoveryStrategy;
use crate::sleep::hybrid_sleep;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Error recovery engine with automatic retry, fallback, and skip capabilities.
pub struct RecoveryEngine {
    /// Recovery configuration.
    config: RecoveryConfig,

    /// Error classifier.
    classifier: ErrorClassifier,

    /// Registered fallback handlers.
    fallbacks: HashMap<String, Arc<dyn Fn() -> anyhow::Result<serde_json::Value> + Send + Sync>>,
}

impl RecoveryEngine {
    /// Create a new recovery engine with default configuration.
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            config,
            classifier: ErrorClassifier::new(),
            fallbacks: HashMap::new(),
        }
    }

    /// Create a recovery engine with default config.
    pub fn with_defaults() -> Self {
        Self::new(RecoveryConfig::default())
    }

    /// Register a fallback handler for a specific operation.
    ///
    /// The handler should return a JSON-serializable result.
    pub fn register_fallback<F>(&mut self, operation: String, handler: F) -> anyhow::Result<()>
    where
        F: Fn() -> anyhow::Result<serde_json::Value> + Send + Sync + 'static,
    {
        self.fallbacks.insert(operation, Arc::new(handler));
        Ok(())
    }

    /// Attempt to recover from an error using the configured strategy.
    ///
    /// This is the main entry point for error recovery. It will:
    /// 1. Classify the error
    /// 2. Apply the appropriate recovery strategy
    /// 3. Log all recovery attempts
    /// 4. Return the final result
    pub async fn recover<T, E, Fut>(
        &self,
        error: anyhow::Error,
        operation: &str,
        f: &impl Fn() -> Fut,
    ) -> anyhow::Result<RecoveryResult<T>>
    where
        T: Send + serde::de::DeserializeOwned + 'static,
        E: std::error::Error + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        let start_time = std::time::Instant::now();
        let mut log: Vec<RecoveryLogEntry> = Vec::new();

        // Classify the error
        let classification = self.classifier.classify(&error);
        let strategy = classification.suggested_strategy;

        info!(
            "Recovery: {} - Category: {}, Strategy: {}",
            operation, classification.category, strategy
        );

        // Apply recovery strategy
        let (result, attempts) = match strategy {
            RecoveryStrategy::Retry => {
                let (result, attempts, retry_log) = self
                    .retry_with_backoff(operation, f, &classification)
                    .await?;
                log.extend(retry_log);
                (result, attempts)
            }
            RecoveryStrategy::Skip => {
                let entry = RecoveryLogEntry::new(
                    1,
                    RecoveryStrategy::Skip,
                    Some(error.to_string()),
                    format!("Skipping operation: {}", operation),
                    Duration::from_millis(0),
                    false,
                );
                log.push(entry);
                (Err(error), 1)
            }
            RecoveryStrategy::Abort => {
                let entry = RecoveryLogEntry::new(
                    1,
                    RecoveryStrategy::Abort,
                    Some(error.to_string()),
                    format!("Aborting due to critical error: {}", operation),
                    Duration::from_millis(0),
                    false,
                );
                log.push(entry);
                (Err(error), 1)
            }
            RecoveryStrategy::Fallback => {
                let (result, attempts, fallback_log) =
                    self.try_fallback(operation, &error, &classification).await;
                log.extend(fallback_log);
                (result, attempts)
            }
        };

        let duration = start_time.elapsed();

        Ok(RecoveryResult::new(
            result, strategy, attempts, duration, log,
        ))
    }

    /// Retry an operation with exponential backoff.
    async fn retry_with_backoff<T, E, Fut>(
        &self,
        operation: &str,
        f: &impl Fn() -> Fut,
        _classification: &ErrorClassification,
    ) -> anyhow::Result<(anyhow::Result<T>, u32, Vec<RecoveryLogEntry>)>
    where
        T: Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        let mut log = Vec::new();
        let mut last_error = None;

        for attempt in 0..self.config.retry.max_attempts {
            let attempt_start = std::time::Instant::now();

            // Attempt the operation
            match f().await {
                Ok(result) => {
                    let duration = attempt_start.elapsed();
                    let entry = RecoveryLogEntry::new(
                        attempt + 1,
                        RecoveryStrategy::Retry,
                        None,
                        format!("Retry attempt {} succeeded", attempt + 1),
                        duration,
                        true,
                    );
                    log.push(entry);

                    if self.config.log_recovery {
                        info!(
                            "Retry success: {} - Attempt {} - Duration: {:?}",
                            operation,
                            attempt + 1,
                            duration
                        );
                    }

                    return Ok((Ok(result), attempt + 1, log));
                }
                Err(e) => {
                    let duration = attempt_start.elapsed();
                    last_error = Some(anyhow::anyhow!(e));

                    let entry = RecoveryLogEntry::new(
                        attempt + 1,
                        RecoveryStrategy::Retry,
                        last_error.as_ref().map(|e| e.to_string()),
                        format!("Retry attempt {} failed", attempt + 1),
                        duration,
                        false,
                    );
                    log.push(entry);

                    if self.config.log_recovery {
                        if let Some(ref err) = last_error {
                            warn!(
                                "Retry failed: {} - Attempt {} - Error: {}",
                                operation,
                                attempt + 1,
                                err
                            );
                        }
                    }

                    // Don't sleep after the last attempt
                    if attempt < self.config.retry.max_attempts - 1 {
                        let backoff = self.config.retry.backoff_duration(attempt);
                        debug!(
                            "Retry: {} - Sleeping for {:?} before next attempt",
                            operation, backoff
                        );
                        hybrid_sleep(backoff).await;
                    }
                }
            }
        }

        // All retries exhausted
        let error = last_error.unwrap_or_else(|| anyhow::anyhow!("All retries exhausted"));
        Ok((Err(error), self.config.retry.max_attempts, log))
    }

    /// Try to use a fallback handler.
    async fn try_fallback<T>(
        &self,
        operation: &str,
        error: &anyhow::Error,
        _classification: &ErrorClassification,
    ) -> (anyhow::Result<T>, u32, Vec<RecoveryLogEntry>)
    where
        T: Send + serde::de::DeserializeOwned + 'static,
    {
        let mut log: Vec<RecoveryLogEntry> = Vec::new();
        let start = std::time::Instant::now();

        // Look for a registered fallback handler
        if let Some(fallback) = self.fallbacks.get(operation) {
            match fallback() {
                Ok(value) => {
                    // Try to deserialize the fallback value
                    match serde_json::from_value::<T>(value) {
                        Ok(result) => {
                            let duration = start.elapsed();
                            let entry = RecoveryLogEntry::new(
                                1,
                                RecoveryStrategy::Fallback,
                                Some(error.to_string()),
                                format!("Fallback handler succeeded for: {}", operation),
                                duration,
                                true,
                            );
                            log.push(entry);

                            if self.config.log_recovery {
                                info!("Fallback success: {} - Duration: {:?}", operation, duration);
                            }

                            (Ok(result), 1, log)
                        }
                        Err(e) => {
                            let duration = start.elapsed();
                            let entry = RecoveryLogEntry::new(
                                1,
                                RecoveryStrategy::Fallback,
                                Some(format!("Fallback deserialization failed: {}", e)),
                                format!("Fallback deserialization failed for: {}", operation),
                                duration,
                                false,
                            );
                            log.push(entry);

                            warn!(
                                "Fallback deserialization failed: {} - Error: {}",
                                operation, e
                            );

                            (
                                Err(anyhow::anyhow!("Fallback deserialization failed: {}", e)),
                                1,
                                log,
                            )
                        }
                    }
                }
                Err(e) => {
                    let duration = start.elapsed();
                    let entry = RecoveryLogEntry::new(
                        1,
                        RecoveryStrategy::Fallback,
                        Some(format!("Fallback handler failed: {}", e)),
                        format!("Fallback handler failed for: {}", operation),
                        duration,
                        false,
                    );
                    log.push(entry);

                    warn!("Fallback handler failed: {} - Error: {}", operation, e);

                    (
                        Err(anyhow::anyhow!("Fallback handler failed: {}", e)),
                        1,
                        log,
                    )
                }
            }
        } else {
            let duration = start.elapsed();
            let entry = RecoveryLogEntry::new(
                1,
                RecoveryStrategy::Fallback,
                Some("No fallback handler registered".to_string()),
                format!("No fallback handler for: {}", operation),
                duration,
                false,
            );
            log.push(entry);

            warn!("No fallback handler registered for: {}", operation);

            (
                Err(anyhow::anyhow!(
                    "No fallback handler registered for: {}",
                    operation
                )),
                1,
                log,
            )
        }
    }

    /// Get the error classifier.
    pub fn classifier(&self) -> &ErrorClassifier {
        &self.classifier
    }

    /// Get a mutable reference to the error classifier.
    pub fn classifier_mut(&mut self) -> &mut ErrorClassifier {
        &mut self.classifier
    }
}

impl Default for RecoveryEngine {
    fn default() -> Self {
        Self::new(RecoveryConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_recovery_engine_retry_success() {
        let engine = RecoveryEngine::with_defaults();
        let attempt_count = Arc::new(AtomicU32::new(0));

        let operation = || {
            let count = Arc::clone(&attempt_count);
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err::<String, _>(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Temporary failure",
                    ))
                } else {
                    Ok("success".to_string())
                }
            }
        };

        let error = anyhow::anyhow!("Initial failure");
        let result = engine.recover(error, "test_operation", &operation).await;

        assert!(result.is_ok());
        let recovery_result = result.unwrap();
        assert!(recovery_result.is_success());
        assert_eq!(recovery_result.strategy_used(), RecoveryStrategy::Retry);
        assert!(recovery_result.attempts() >= 2);
    }

    #[tokio::test]
    async fn test_recovery_engine_fallback() {
        let mut engine = RecoveryEngine::with_defaults();

        // Register a fallback handler
        engine
            .register_fallback("test_fallback".to_string(), || {
                Ok(serde_json::json!("fallback_result"))
            })
            .unwrap();

        // This will immediately fail and use fallback
        let operation = || async {
            Err::<String, _>(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Always fails",
            ))
        };

        // We need to modify the classifier to suggest fallback
        engine.classifier_mut().add_pattern(
            "Always fails".to_string(),
            ErrorClassification::new(
                super::super::strategy::ErrorCategory::ResourceNotFound,
                false,
                RecoveryStrategy::Fallback,
                "Test fallback".to_string(),
            ),
        );

        let error = anyhow::anyhow!("Always fails");
        let result = engine.recover(error, "test_fallback", &operation).await;

        assert!(result.is_ok());
        let recovery_result = result.unwrap();
        assert!(recovery_result.is_success());
        assert_eq!(recovery_result.strategy_used(), RecoveryStrategy::Fallback);
        assert_eq!(recovery_result.attempts(), 1);
    }

    #[tokio::test]
    async fn test_recovery_engine_abort() {
        let mut engine = RecoveryEngine::with_defaults();

        // Modify classifier to suggest abort
        engine.classifier_mut().add_pattern(
            "fatal".to_string(),
            ErrorClassification::critical("Fatal error".to_string()),
        );

        let operation = || async {
            Err::<String, _>(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fatal error",
            ))
        };

        let error = anyhow::anyhow!("fatal error");
        let result = engine.recover(error, "test_abort", &operation).await;

        assert!(result.is_ok());
        let recovery_result = result.unwrap();
        assert!(!recovery_result.is_success());
        assert_eq!(recovery_result.strategy_used(), RecoveryStrategy::Abort);
        assert_eq!(recovery_result.attempts(), 1);
    }

    #[tokio::test]
    async fn test_recovery_engine_retry_exhausted() {
        // Configure engine with small backoff and few attempts to keep test fast
        let mut config = RecoveryConfig::default();
        config.retry.max_attempts = 2;
        config.retry.initial_backoff = Duration::from_millis(1);
        config.retry.jitter = false;
        let engine = RecoveryEngine::new(config);

        let operation = || async { Err::<String, _>(std::io::Error::other("always fails")) };

        let error = anyhow::anyhow!("always fails");
        let result = engine.recover(error, "op_exhaust", &operation).await;

        assert!(result.is_ok());
        let recovery_result = result.unwrap();
        assert!(!recovery_result.is_success());
        assert_eq!(recovery_result.strategy_used(), RecoveryStrategy::Retry);
        assert_eq!(recovery_result.attempts(), 2);
    }
}
