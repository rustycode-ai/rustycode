//! Timeout Handling for LLM Operations
//!
//! Provides comprehensive timeout management for LLM API calls and tool execution,
//! with model-specific timeouts and graceful degradation integration.

use crate::provider::ProviderError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{debug, warn};

/// Model-specific timeout defaults
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelTimeoutPreset {
    /// Claude Haiku - fast, lightweight (~20 seconds)
    Haiku,
    /// Claude Sonnet - balanced (~60 seconds)
    Sonnet,
    /// Claude Opus - slow, reasoning-heavy (~120 seconds)
    Opus,
    /// Default for unknown models (~30 seconds)
    Default,
}

impl ModelTimeoutPreset {
    /// Get timeout duration for this model
    pub fn duration(&self) -> Duration {
        match self {
            Self::Haiku => Duration::from_secs(20),
            Self::Sonnet => Duration::from_mins(1),
            Self::Opus => Duration::from_mins(2),
            Self::Default => Duration::from_secs(30),
            #[allow(unreachable_patterns)]
            _ => Duration::from_secs(30),
        }
    }

    /// Determine preset from model name
    pub fn from_model_name(model: &str) -> Self {
        let model_lower = model.to_lowercase();
        if model_lower.contains("haiku") {
            Self::Haiku
        } else if model_lower.contains("opus") {
            Self::Opus
        } else if model_lower.contains("sonnet") {
            Self::Sonnet
        } else {
            Self::Default
        }
    }
}

/// Configuration for timeout handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Global default timeout (used if no model-specific timeout applies)
    pub default_timeout: Duration,
    /// Timeout for Haiku models
    pub haiku_timeout: Duration,
    /// Timeout for Sonnet models
    pub sonnet_timeout: Duration,
    /// Timeout for Opus models
    pub opus_timeout: Duration,
    /// Timeout for tool execution
    pub tool_timeout: Duration,
    /// Enable timeout tracking
    pub enable_tracking: bool,
    /// Maximum number of concurrent timeouts to track
    pub max_tracked_timeouts: usize,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            haiku_timeout: Duration::from_secs(20),
            sonnet_timeout: Duration::from_mins(1),
            opus_timeout: Duration::from_mins(2),
            tool_timeout: Duration::from_secs(30),
            enable_tracking: true,
            max_tracked_timeouts: 1000,
        }
    }
}

impl TimeoutConfig {
    /// Get timeout for a specific model
    pub fn model_timeout(&self, model: &str) -> Duration {
        match ModelTimeoutPreset::from_model_name(model) {
            ModelTimeoutPreset::Haiku => self.haiku_timeout,
            ModelTimeoutPreset::Sonnet => self.sonnet_timeout,
            ModelTimeoutPreset::Opus => self.opus_timeout,
            ModelTimeoutPreset::Default => self.default_timeout,
            #[allow(unreachable_patterns)]
            _ => self.default_timeout,
        }
    }

    /// Create a custom configuration
    pub fn custom(
        default: Duration,
        haiku: Duration,
        sonnet: Duration,
        opus: Duration,
        tool: Duration,
    ) -> Self {
        Self {
            default_timeout: default,
            haiku_timeout: haiku,
            sonnet_timeout: sonnet,
            opus_timeout: opus,
            tool_timeout: tool,
            enable_tracking: true,
            max_tracked_timeouts: 1000,
        }
    }
}

/// Event recorded when a timeout occurs
#[derive(Debug, Clone)]
pub struct TimeoutEvent {
    /// Name of the endpoint or operation
    pub endpoint: String,
    /// Model being used (if applicable)
    pub model: Option<String>,
    /// Configured timeout duration
    pub timeout: Duration,
    /// How long the operation actually took
    pub elapsed: Duration,
    /// Timestamp of the event
    pub timestamp: std::time::SystemTime,
}

impl TimeoutEvent {
    /// Calculate how much time was exceeded
    pub fn overage(&self) -> Duration {
        if self.elapsed > self.timeout {
            self.elapsed - self.timeout
        } else {
            Duration::ZERO
        }
    }
}

/// Tracks timeout events for observability
pub struct TimeoutTracker {
    events: Arc<std::sync::Mutex<Vec<TimeoutEvent>>>,
    max_events: usize,
}

impl TimeoutTracker {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Arc::new(std::sync::Mutex::new(Vec::with_capacity(max_events))),
            max_events,
        }
    }

    /// Record a timeout event
    pub fn record(&self, event: TimeoutEvent) {
        if let Ok(mut events) = self.events.lock() {
            if events.len() >= self.max_events {
                events.remove(0); // FIFO eviction
            }
            events.push(event);
        }
    }

    /// Get all recorded events
    pub fn events(&self) -> Vec<TimeoutEvent> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Get recent timeout events
    pub fn recent(&self, count: usize) -> Vec<TimeoutEvent> {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        events.iter().rev().take(count).cloned().collect()
    }

    /// Get timeout count for an endpoint
    pub fn count_by_endpoint(&self, endpoint: &str) -> usize {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        events.iter().filter(|e| e.endpoint == endpoint).count()
    }

    /// Clear all tracked events
    pub fn clear(&self) {
        if let Ok(mut events) = self.events.lock() {
            events.clear();
        }
    }

    /// Get statistics about timeouts
    pub fn stats(&self) -> TimeoutStats {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner());

        let total = events.len();
        if total == 0 {
            return TimeoutStats::default();
        }

        let avg_overage = events
            .iter()
            .map(|e| e.overage().as_millis() as f64)
            .sum::<f64>()
            / total as f64;

        let mut endpoint_counts = std::collections::HashMap::new();
        for event in events.iter() {
            *endpoint_counts.entry(event.endpoint.clone()).or_insert(0) += 1;
        }

        TimeoutStats {
            total_timeouts: total,
            avg_overage_ms: avg_overage,
            endpoints_with_timeouts: endpoint_counts.len(),
            most_affected_endpoint: endpoint_counts
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(endpoint, _)| endpoint),
        }
    }
}

impl Clone for TimeoutTracker {
    fn clone(&self) -> Self {
        Self {
            events: Arc::clone(&self.events),
            max_events: self.max_events,
        }
    }
}

/// Statistics about timeout events
#[derive(Debug, Clone, Default)]
pub struct TimeoutStats {
    pub total_timeouts: usize,
    pub avg_overage_ms: f64,
    pub endpoints_with_timeouts: usize,
    pub most_affected_endpoint: Option<String>,
}

/// Timeout handler for LLM operations
pub struct TimeoutHandler {
    config: TimeoutConfig,
    tracker: TimeoutTracker,
}

impl TimeoutHandler {
    pub fn new(config: TimeoutConfig) -> Self {
        let tracker = TimeoutTracker::new(config.max_tracked_timeouts);
        Self { config, tracker }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(TimeoutConfig::default())
    }

    /// Wrap an async operation with timeout
    pub async fn with_timeout<F, T>(
        &self,
        operation: F,
        endpoint: &str,
        model: Option<&str>,
    ) -> Result<T, ProviderError>
    where
        F: std::future::Future<Output = Result<T, ProviderError>>,
    {
        let timeout_duration = model
            .map(|m| self.config.model_timeout(m))
            .unwrap_or(self.config.default_timeout);

        let start = Instant::now();
        debug!(
            "Starting operation {} with timeout: {:?}",
            endpoint, timeout_duration
        );

        match timeout(timeout_duration, operation).await {
            Ok(result) => {
                let elapsed = start.elapsed();
                debug!("Operation {} completed in {:?}", endpoint, elapsed);
                result
            }
            Err(_) => {
                let elapsed = start.elapsed();
                warn!(
                    "Operation {} timed out after {:?} (configured: {:?})",
                    endpoint, elapsed, timeout_duration
                );

                // Record timeout event
                if self.config.enable_tracking {
                    self.tracker.record(TimeoutEvent {
                        endpoint: endpoint.to_string(),
                        model: model.map(|s| s.to_string()),
                        timeout: timeout_duration,
                        elapsed,
                        timestamp: std::time::SystemTime::now(),
                    });
                }

                Err(ProviderError::Timeout(format!(
                    "Operation {} timed out after {:?}",
                    endpoint, elapsed
                )))
            }
        }
    }

    /// Wrap a tool execution with timeout
    pub async fn with_tool_timeout<F, T>(
        &self,
        operation: F,
        tool_name: &str,
    ) -> Result<T, ProviderError>
    where
        F: std::future::Future<Output = Result<T, ProviderError>>,
    {
        let start = Instant::now();
        debug!(
            "Starting tool execution {} with timeout: {:?}",
            tool_name, self.config.tool_timeout
        );

        match timeout(self.config.tool_timeout, operation).await {
            Ok(result) => {
                let elapsed = start.elapsed();
                debug!("Tool {} completed in {:?}", tool_name, elapsed);
                result
            }
            Err(_) => {
                let elapsed = start.elapsed();
                warn!("Tool {} timed out after {:?}", tool_name, elapsed);

                if self.config.enable_tracking {
                    self.tracker.record(TimeoutEvent {
                        endpoint: format!("tool:{}", tool_name),
                        model: None,
                        timeout: self.config.tool_timeout,
                        elapsed,
                        timestamp: std::time::SystemTime::now(),
                    });
                }

                Err(ProviderError::Timeout(format!(
                    "Tool {} timed out after {:?}",
                    tool_name, elapsed
                )))
            }
        }
    }

    /// Get the timeout tracker
    pub fn tracker(&self) -> &TimeoutTracker {
        &self.tracker
    }

    /// Get the current configuration
    pub fn config(&self) -> &TimeoutConfig {
        &self.config
    }

    /// Update timeout configuration
    pub fn update_config(&mut self, config: TimeoutConfig) {
        self.config = config;
    }
}

impl Clone for TimeoutHandler {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            tracker: self.tracker.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_timeout_preset_haiku() {
        assert_eq!(
            ModelTimeoutPreset::from_model_name("claude-3-5-haiku"),
            ModelTimeoutPreset::Haiku
        );
        assert_eq!(
            ModelTimeoutPreset::Haiku.duration(),
            Duration::from_secs(20)
        );
    }

    #[test]
    fn test_model_timeout_preset_sonnet() {
        assert_eq!(
            ModelTimeoutPreset::from_model_name("claude-3-5-sonnet"),
            ModelTimeoutPreset::Sonnet
        );
        assert_eq!(
            ModelTimeoutPreset::Sonnet.duration(),
            Duration::from_mins(1)
        );
    }

    #[test]
    fn test_model_timeout_preset_opus() {
        assert_eq!(
            ModelTimeoutPreset::from_model_name("claude-opus"),
            ModelTimeoutPreset::Opus
        );
        assert_eq!(ModelTimeoutPreset::Opus.duration(), Duration::from_mins(2));
    }

    #[test]
    fn test_model_timeout_preset_default() {
        assert_eq!(
            ModelTimeoutPreset::from_model_name("gpt-4"),
            ModelTimeoutPreset::Default
        );
        assert_eq!(
            ModelTimeoutPreset::Default.duration(),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn test_timeout_config_default() {
        let config = TimeoutConfig::default();
        assert_eq!(config.default_timeout, Duration::from_secs(30));
        assert_eq!(config.haiku_timeout, Duration::from_secs(20));
        assert_eq!(config.sonnet_timeout, Duration::from_mins(1));
        assert_eq!(config.opus_timeout, Duration::from_mins(2));
        assert_eq!(config.tool_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_timeout_config_get_model_timeout() {
        let config = TimeoutConfig::default();
        assert_eq!(config.model_timeout("haiku"), Duration::from_secs(20));
        assert_eq!(config.model_timeout("sonnet"), Duration::from_mins(1));
        assert_eq!(config.model_timeout("opus"), Duration::from_mins(2));
        assert_eq!(config.model_timeout("unknown"), Duration::from_secs(30));
    }

    #[test]
    fn test_timeout_config_custom() {
        let config = TimeoutConfig::custom(
            Duration::from_secs(10),
            Duration::from_secs(15),
            Duration::from_secs(30),
            Duration::from_mins(1),
            Duration::from_secs(20),
        );
        assert_eq!(config.default_timeout, Duration::from_secs(10));
        assert_eq!(config.haiku_timeout, Duration::from_secs(15));
        assert_eq!(config.sonnet_timeout, Duration::from_secs(30));
        assert_eq!(config.opus_timeout, Duration::from_mins(1));
        assert_eq!(config.tool_timeout, Duration::from_secs(20));
    }

    #[test]
    fn test_timeout_event_overage() {
        let event = TimeoutEvent {
            endpoint: "test".to_string(),
            model: Some("haiku".to_string()),
            timeout: Duration::from_secs(10),
            elapsed: Duration::from_secs(15),
            timestamp: std::time::SystemTime::now(),
        };
        assert_eq!(event.overage(), Duration::from_secs(5));
    }

    #[test]
    fn test_timeout_event_overage_within_limit() {
        let event = TimeoutEvent {
            endpoint: "test".to_string(),
            model: None,
            timeout: Duration::from_secs(10),
            elapsed: Duration::from_secs(8),
            timestamp: std::time::SystemTime::now(),
        };
        assert_eq!(event.overage(), Duration::ZERO);
    }

    #[test]
    fn test_timeout_tracker_record() {
        let tracker = TimeoutTracker::new(10);
        let event = TimeoutEvent {
            endpoint: "test".to_string(),
            model: None,
            timeout: Duration::from_secs(10),
            elapsed: Duration::from_secs(15),
            timestamp: std::time::SystemTime::now(),
        };

        tracker.record(event);
        assert_eq!(tracker.events().len(), 1);
    }

    #[test]
    fn test_timeout_tracker_max_capacity() {
        let tracker = TimeoutTracker::new(2);

        for i in 0..5 {
            tracker.record(TimeoutEvent {
                endpoint: format!("test-{}", i),
                model: None,
                timeout: Duration::from_secs(10),
                elapsed: Duration::from_secs(15),
                timestamp: std::time::SystemTime::now(),
            });
        }

        let events = tracker.events();
        assert_eq!(events.len(), 2);
        // Should have most recent events
        assert_eq!(events[0].endpoint, "test-3");
        assert_eq!(events[1].endpoint, "test-4");
    }

    #[test]
    fn test_timeout_tracker_get_recent() {
        let tracker = TimeoutTracker::new(10);

        for i in 0..5 {
            tracker.record(TimeoutEvent {
                endpoint: format!("test-{}", i),
                model: None,
                timeout: Duration::from_secs(10),
                elapsed: Duration::from_secs(15),
                timestamp: std::time::SystemTime::now(),
            });
        }

        let recent = tracker.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].endpoint, "test-4");
        assert_eq!(recent[1].endpoint, "test-3");
    }

    #[test]
    fn test_timeout_tracker_count_by_endpoint() {
        let tracker = TimeoutTracker::new(10);

        for _ in 0..3 {
            tracker.record(TimeoutEvent {
                endpoint: "endpoint-1".to_string(),
                model: None,
                timeout: Duration::from_secs(10),
                elapsed: Duration::from_secs(15),
                timestamp: std::time::SystemTime::now(),
            });
        }

        for _ in 0..2 {
            tracker.record(TimeoutEvent {
                endpoint: "endpoint-2".to_string(),
                model: None,
                timeout: Duration::from_secs(10),
                elapsed: Duration::from_secs(15),
                timestamp: std::time::SystemTime::now(),
            });
        }

        assert_eq!(tracker.count_by_endpoint("endpoint-1"), 3);
        assert_eq!(tracker.count_by_endpoint("endpoint-2"), 2);
    }

    #[test]
    fn test_timeout_tracker_clear() {
        let tracker = TimeoutTracker::new(10);
        tracker.record(TimeoutEvent {
            endpoint: "test".to_string(),
            model: None,
            timeout: Duration::from_secs(10),
            elapsed: Duration::from_secs(15),
            timestamp: std::time::SystemTime::now(),
        });

        assert!(!tracker.events().is_empty());
        tracker.clear();
        assert!(tracker.events().is_empty());
    }

    #[test]
    fn test_timeout_tracker_stats() {
        let tracker = TimeoutTracker::new(10);

        tracker.record(TimeoutEvent {
            endpoint: "endpoint-1".to_string(),
            model: None,
            timeout: Duration::from_secs(10),
            elapsed: Duration::from_secs(15), // 5 second overage
            timestamp: std::time::SystemTime::now(),
        });

        tracker.record(TimeoutEvent {
            endpoint: "endpoint-1".to_string(),
            model: None,
            timeout: Duration::from_secs(10),
            elapsed: Duration::from_secs(20), // 10 second overage
            timestamp: std::time::SystemTime::now(),
        });

        tracker.record(TimeoutEvent {
            endpoint: "endpoint-2".to_string(),
            model: None,
            timeout: Duration::from_secs(10),
            elapsed: Duration::from_secs(18), // 8 second overage
            timestamp: std::time::SystemTime::now(),
        });

        let stats = tracker.stats();
        assert_eq!(stats.total_timeouts, 3);
        assert_eq!(stats.endpoints_with_timeouts, 2);
        assert_eq!(stats.most_affected_endpoint, Some("endpoint-1".to_string()));
        assert!((stats.avg_overage_ms - 7666.67).abs() < 1.0); // (5 + 10 + 8) / 3 seconds
    }

    #[test]
    fn test_timeout_handler_creation() {
        let handler = TimeoutHandler::default_config();
        let config = handler.config();
        assert_eq!(config.default_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_timeout_handler_clone() {
        let handler = TimeoutHandler::default_config();
        let handler2 = handler.clone();
        assert_eq!(handler2.config().default_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_with_timeout_success() {
        let handler = TimeoutHandler::default_config();

        let result = handler
            .with_timeout(async { Ok::<_, ProviderError>(42) }, "test-endpoint", None)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_timeout_exceed() {
        let config = TimeoutConfig {
            default_timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let handler = TimeoutHandler::new(config);

        let result = handler
            .with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok::<_, ProviderError>(42)
                },
                "test-endpoint",
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProviderError::Timeout(_)));
    }

    #[tokio::test]
    async fn test_with_timeout_model_specific() {
        let handler = TimeoutHandler::default_config();

        let result = handler
            .with_timeout(
                async { Ok::<_, ProviderError>(42) },
                "test-endpoint",
                Some("claude-haiku"),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_with_tool_timeout_success() {
        let handler = TimeoutHandler::default_config();

        let result = handler
            .with_tool_timeout(async { Ok::<_, ProviderError>(42) }, "test-tool")
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_with_tool_timeout_exceed() {
        let config = TimeoutConfig {
            tool_timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let handler = TimeoutHandler::new(config);

        let result = handler
            .with_tool_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok::<_, ProviderError>(42)
                },
                "test-tool",
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_timeout_tracking() {
        let handler = TimeoutHandler::default_config();
        let mut config = handler.config().clone();
        config.default_timeout = Duration::from_millis(10);
        let handler = TimeoutHandler::new(config);

        let _ = handler
            .with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok::<_, ProviderError>(42)
                },
                "test-endpoint",
                None,
            )
            .await;

        let events = handler.tracker().events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].endpoint, "test-endpoint");
    }
}
