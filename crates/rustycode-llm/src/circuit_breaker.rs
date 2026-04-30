//! Circuit Breaker Implementation
//!
//! Provides a circuit breaker state machine to handle cascading failures across
//! endpoints or providers. Transitions between Closed (normal), Open (failing),
//! and HalfOpen (recovery) states based on success/failure tracking.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

/// Represents the state of the circuit breaker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CircuitState {
    /// Normal operation - requests flow through
    Closed,
    /// Failing - requests are rejected immediately
    Open,
    /// Recovery mode - limited requests are allowed to test recovery
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "Closed"),
            Self::Open => write!(f, "Open"),
            Self::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,
    /// Number of consecutive successes to transition from HalfOpen to Closed
    pub success_threshold: u32,
    /// Duration to wait before transitioning from Open to HalfOpen
    pub cooldown_duration: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 3,
            cooldown_duration: Duration::from_secs(30),
        }
    }
}

/// Internal state tracking for circuit breaker
#[derive(Debug, Clone)]
struct CircuitBreakerInner {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<SystemTime>,
    last_state_change: SystemTime,
}

impl CircuitBreakerInner {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            last_state_change: SystemTime::now(),
        }
    }
}

/// Circuit breaker for managing endpoint health
///
/// Tracks failures and successes to automatically degrade gracefully
/// when endpoints are experiencing issues.
///
/// # Examples
///
/// ```ignore
/// use rustycode_llm::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
/// use std::time::Duration;
///
/// let config = CircuitBreakerConfig {
///     failure_threshold: 3,
///     success_threshold: 2,
///     cooldown_duration: Duration::from_secs(30),
/// };
///
/// let breaker = CircuitBreaker::new("openai-gpt4", config);
/// if breaker.is_available() {
///     // Attempt request
///     if let Err(_) = attempt_request() {
///         breaker.record_failure();
///     } else {
///         breaker.record_success();
///     }
/// }
/// ```
pub struct CircuitBreaker {
    endpoint: String,
    config: CircuitBreakerConfig,
    inner: Arc<Mutex<CircuitBreakerInner>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker for an endpoint
    pub fn new(endpoint: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        let endpoint = endpoint.into();
        debug!("Creating circuit breaker for endpoint: {}", endpoint);
        Self {
            endpoint,
            config,
            inner: Arc::new(Mutex::new(CircuitBreakerInner::new())),
        }
    }

    /// Create a new circuit breaker with default configuration
    pub fn new_default(endpoint: impl Into<String>) -> Self {
        Self::new(endpoint, CircuitBreakerConfig::default())
    }

    /// Check if the circuit is available for requests
    pub fn is_available(&self) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match inner.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if cooldown period has passed
                if let Ok(elapsed) = inner.last_state_change.elapsed() {
                    elapsed >= self.config.cooldown_duration
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true, // Allow limited requests
        }
    }

    /// Get the current state of the circuit
    pub fn state(&self) -> CircuitState {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).state
    }

    /// Get detailed status information
    pub fn status(&self) -> CircuitBreakerStatus {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let time_since_state_change = inner.last_state_change.elapsed().unwrap_or(Duration::ZERO);
        let is_available = match inner.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Ok(elapsed) = inner.last_state_change.elapsed() {
                    elapsed >= self.config.cooldown_duration
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        };

        CircuitBreakerStatus {
            endpoint: self.endpoint.clone(),
            state: inner.state,
            failure_count: inner.failure_count,
            success_count: inner.success_count,
            last_failure_time: inner.last_failure_time,
            time_since_state_change,
            is_available,
        }
    }

    /// Record a successful request
    pub fn record_success(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.failure_count = 0; // Reset failure count on success

        match inner.state {
            CircuitState::Closed => {
                // Already healthy, no action needed
            }
            CircuitState::HalfOpen => {
                inner.success_count += 1;
                debug!(
                    "Circuit {} success in HalfOpen: {}/{}",
                    self.endpoint, inner.success_count, self.config.success_threshold
                );

                if inner.success_count >= self.config.success_threshold {
                    info!("Circuit {} transitioning to Closed", self.endpoint);
                    inner.state = CircuitState::Closed;
                    inner.success_count = 0;
                }
            }
            CircuitState::Open => {
                // Waiting in cooldown, don't record success yet
            }
        }
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.failure_count += 1;
        inner.success_count = 0; // Reset success count on failure
        inner.last_failure_time = Some(SystemTime::now());

        match inner.state {
            CircuitState::Closed => {
                debug!(
                    "Circuit {} failure count: {}/{}",
                    self.endpoint, inner.failure_count, self.config.failure_threshold
                );

                if inner.failure_count >= self.config.failure_threshold {
                    warn!(
                        "Circuit {} transitioning to Open after {} failures",
                        self.endpoint, inner.failure_count
                    );
                    inner.state = CircuitState::Open;
                    inner.last_state_change = SystemTime::now();
                }
            }
            CircuitState::Open => {
                // Already open, wait for cooldown
                debug!(
                    "Circuit {} failed while Open, waiting for cooldown",
                    self.endpoint
                );

                // Check if cooldown has passed, transition to HalfOpen
                if let Ok(elapsed) = inner.last_state_change.elapsed() {
                    if elapsed >= self.config.cooldown_duration {
                        info!(
                            "Circuit {} cooldown passed, transitioning to HalfOpen",
                            self.endpoint
                        );
                        inner.state = CircuitState::HalfOpen;
                        inner.failure_count = 0;
                        inner.last_state_change = SystemTime::now();
                    }
                }
            }
            CircuitState::HalfOpen => {
                warn!(
                    "Circuit {} failed in HalfOpen, reopening circuit",
                    self.endpoint
                );
                inner.state = CircuitState::Open;
                inner.failure_count = 0;
                inner.last_state_change = SystemTime::now();
            }
        }
    }

    /// Reset the circuit breaker to Closed state
    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.state = CircuitState::Closed;
        inner.failure_count = 0;
        inner.success_count = 0;
        inner.last_failure_time = None;
        inner.last_state_change = SystemTime::now();
        info!("Circuit {} reset to Closed", self.endpoint);
    }

    /// Force the circuit to Open state
    pub fn open(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.state = CircuitState::Open;
        inner.last_state_change = SystemTime::now();
        warn!("Circuit {} forced to Open", self.endpoint);
    }

    /// Force the circuit to HalfOpen state (primarily for testing)
    pub fn half_open(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.state = CircuitState::HalfOpen;
        inner.failure_count = 0;
        inner.success_count = 0;
        inner.last_state_change = SystemTime::now();
        debug!("Circuit {} forced to HalfOpen", self.endpoint);
    }

    /// Get the endpoint name
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

/// Detailed status information for a circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerStatus {
    pub endpoint: String,
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_time: Option<SystemTime>,
    pub time_since_state_change: Duration,
    pub is_available: bool,
}

impl std::fmt::Display for CircuitBreakerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Circuit[{}]: state={}, available={}, failures={}, successes={}, time_since_change={}ms",
            self.endpoint,
            self.state,
            self.is_available,
            self.failure_count,
            self.success_count,
            self.time_since_state_change.as_millis()
        )
    }
}

/// Registry of circuit breakers for multiple endpoints
pub struct CircuitBreakerRegistry {
    breakers: Arc<Mutex<std::collections::HashMap<String, CircuitBreaker>>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreakerRegistry {
    /// Create a new registry with default configuration
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: Arc::new(Mutex::new(std::collections::HashMap::new())),
            config,
        }
    }

    /// Get or create a circuit breaker for an endpoint
    pub fn get_or_create(&self, endpoint: impl Into<String>) -> CircuitBreaker {
        let endpoint_str = endpoint.into();
        let mut breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());

        breakers
            .entry(endpoint_str.clone())
            .or_insert_with(|| CircuitBreaker::new(endpoint_str.clone(), self.config.clone()))
            .clone()
    }

    /// Get an existing circuit breaker
    pub fn get(&self, endpoint: &str) -> Option<CircuitBreaker> {
        let breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        breakers.get(endpoint).cloned()
    }

    /// Get all circuit breaker statuses
    pub fn all_statuses(&self) -> Vec<CircuitBreakerStatus> {
        let breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        breakers.values().map(|breaker| breaker.status()).collect()
    }

    /// Get count of open circuits
    pub fn open_count(&self) -> usize {
        let breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        breakers
            .values()
            .filter(|b| b.state() == CircuitState::Open)
            .count()
    }

    /// Reset all circuits
    pub fn reset_all(&self) {
        let breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        for breaker in breakers.values() {
            breaker.reset();
        }
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            config: self.config.clone(),
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_initial_state() {
        let breaker = CircuitBreaker::new_default("test-endpoint");
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.is_available());
    }

    #[test]
    fn test_circuit_breaker_records_success() {
        let breaker = CircuitBreaker::new_default("test-endpoint");
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            cooldown_duration: Duration::from_millis(100),
        };
        let breaker = CircuitBreaker::new("test-endpoint", config);

        // Record failures
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed);

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed);

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.is_available());
    }

    #[test]
    fn test_circuit_breaker_resets_failure_count_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            cooldown_duration: Duration::from_millis(100),
        };
        let breaker = CircuitBreaker::new("test-endpoint", config);

        breaker.record_failure();
        breaker.record_failure();
        breaker.record_success(); // Reset failure count

        assert_eq!(breaker.state(), CircuitState::Closed);
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed); // Would have opened without reset
    }

    #[test]
    fn test_circuit_breaker_half_open_transitions() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            cooldown_duration: Duration::from_millis(50),
        };
        let cooldown = config.cooldown_duration;
        let breaker = CircuitBreaker::new("test-endpoint", config);

        // Open circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // After cooldown, becomes available
        std::thread::sleep(Duration::from_millis(100));
        assert!(breaker.is_available());

        // Simulate recovery path
        let mut inner = breaker.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.state == CircuitState::Open {
            if let Ok(elapsed) = inner.last_state_change.elapsed() {
                if elapsed >= cooldown {
                    inner.state = CircuitState::HalfOpen;
                }
            }
        }
        drop(inner);

        // Success in HalfOpen should close circuit
        breaker.record_success();
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_status() {
        let breaker = CircuitBreaker::new_default("test-endpoint");
        let status = breaker.status();

        assert_eq!(status.endpoint, "test-endpoint");
        assert_eq!(status.state, CircuitState::Closed);
        assert!(status.is_available);
        assert_eq!(status.failure_count, 0);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let breaker = CircuitBreaker::new_default("test-endpoint");
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        breaker.reset();
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.is_available());
    }

    #[test]
    fn test_circuit_breaker_force_open() {
        let breaker = CircuitBreaker::new_default("test-endpoint");
        assert_eq!(breaker.state(), CircuitState::Closed);

        breaker.open();
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.is_available());
    }

    #[test]
    fn test_circuit_breaker_registry() {
        let config = CircuitBreakerConfig::default();
        let registry = CircuitBreakerRegistry::new(config);

        let breaker1 = registry.get_or_create("endpoint-1");
        let breaker2 = registry.get_or_create("endpoint-2");

        assert_eq!(breaker1.endpoint(), "endpoint-1");
        assert_eq!(breaker2.endpoint(), "endpoint-2");

        breaker1.record_failure();
        assert_eq!(
            registry.get("endpoint-1").unwrap().state(),
            CircuitState::Closed
        );
    }

    #[test]
    fn test_circuit_breaker_registry_get_or_create() {
        let registry = CircuitBreakerRegistry::new(CircuitBreakerConfig::default());
        let breaker1 = registry.get_or_create("test");
        let breaker2 = registry.get_or_create("test");

        breaker1.record_failure();
        assert_eq!(breaker2.state(), CircuitState::Closed); // Same breaker
    }

    #[test]
    fn test_circuit_breaker_registry_open_count() {
        let registry = CircuitBreakerRegistry::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            cooldown_duration: Duration::from_millis(100),
        });

        let b1 = registry.get_or_create("endpoint-1");
        let b2 = registry.get_or_create("endpoint-2");

        b1.record_failure();
        assert_eq!(registry.open_count(), 1);

        b2.record_failure();
        assert_eq!(registry.open_count(), 2);
    }

    #[test]
    fn test_circuit_breaker_registry_reset_all() {
        let registry = CircuitBreakerRegistry::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            cooldown_duration: Duration::from_millis(100),
        });

        let b1 = registry.get_or_create("endpoint-1");
        let b2 = registry.get_or_create("endpoint-2");

        b1.record_failure();
        b2.record_failure();
        assert_eq!(registry.open_count(), 2);

        registry.reset_all();
        assert_eq!(registry.open_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_failure_in_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            cooldown_duration: Duration::from_millis(50),
        };
        let cooldown = config.cooldown_duration;
        let breaker = CircuitBreaker::new("test-endpoint", config);

        // Open the circuit
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for cooldown and transition to HalfOpen
        std::thread::sleep(Duration::from_millis(100));
        let mut inner = breaker.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.state == CircuitState::Open {
            if let Ok(elapsed) = inner.last_state_change.elapsed() {
                if elapsed >= cooldown {
                    inner.state = CircuitState::HalfOpen;
                }
            }
        }
        drop(inner);

        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Failure in HalfOpen should reopen
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
    }
}
