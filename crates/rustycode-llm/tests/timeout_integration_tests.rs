#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration tests for timeout handling with graceful degradation
//!
//! These tests verify that timeout handling integrates properly with
//! graceful degradation and circuit breaker mechanisms.

use rustycode_llm::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerRegistry, CircuitState, ProviderError,
    TimeoutConfig, TimeoutHandler,
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_timeout_with_circuit_breaker_integration() {
    let circuit_config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        cooldown_duration: Duration::from_millis(100),
    };
    let breaker = CircuitBreaker::new("test-api", circuit_config);

    let timeout_config = TimeoutConfig {
        default_timeout: Duration::from_millis(10),
        ..Default::default()
    };
    let handler = TimeoutHandler::new(timeout_config);

    // Simulate timeout failures
    for _ in 0..2 {
        let result = handler
            .with_timeout(
                async {
                    sleep(Duration::from_millis(50)).await;
                    Ok::<_, ProviderError>(())
                },
                "test-api",
                None,
            )
            .await;

        assert!(result.is_err());
        breaker.record_failure();
    }

    // Circuit should now be open
    assert_eq!(breaker.state(), CircuitState::Open);
    assert!(!breaker.is_available());
}

#[test]
fn test_circuit_breaker_registry_multiple_endpoints() {
    let registry = CircuitBreakerRegistry::new(CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 1,
        cooldown_duration: Duration::from_millis(50),
    });

    // Simulate failures on multiple endpoints
    let breaker1 = registry.get_or_create("api-1");
    let breaker2 = registry.get_or_create("api-2");
    let breaker3 = registry.get_or_create("api-3");

    // Open first two circuits
    breaker1.record_failure();
    breaker1.record_failure();
    breaker2.record_failure();
    breaker2.record_failure();

    // Third remains closed
    breaker3.record_success();

    assert_eq!(registry.open_count(), 2);

    // Should have all three endpoints tracked
    let statuses = registry.all_statuses();
    assert_eq!(statuses.len(), 3);

    // Open count should reflect correct state
    assert!(
        statuses
            .iter()
            .filter(|s| s.state == CircuitState::Open)
            .count()
            >= 2
    );
}

#[tokio::test]
async fn test_timeout_tracking_accumulation() {
    let config = TimeoutConfig {
        default_timeout: Duration::from_millis(10),
        ..Default::default()
    };
    let handler = TimeoutHandler::new(config);

    // Generate multiple timeouts
    for i in 0..5 {
        let _ = handler
            .with_timeout(
                async {
                    sleep(Duration::from_millis(50)).await;
                    Ok::<_, ProviderError>(())
                },
                &format!("endpoint-{}", i % 2), // Cycle between 2 endpoints
                None,
            )
            .await;
    }

    // Verify tracking
    let tracker = handler.tracker();
    let events = tracker.events();
    assert_eq!(events.len(), 5);

    let stats = tracker.stats();
    assert_eq!(stats.total_timeouts, 5);
    assert_eq!(stats.endpoints_with_timeouts, 2);
}

#[tokio::test]
async fn test_model_specific_timeouts() {
    let handler = TimeoutHandler::default_config();

    // Haiku should have tighter timeout
    let haiku_timeout = handler.config().model_timeout("claude-haiku");
    let opus_timeout = handler.config().model_timeout("claude-opus");

    assert!(haiku_timeout < opus_timeout);
    assert_eq!(haiku_timeout, Duration::from_secs(20));
    assert_eq!(opus_timeout, Duration::from_mins(2));
}

#[test]
fn test_circuit_breaker_recovery_with_success() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        cooldown_duration: Duration::from_millis(50),
    };
    let breaker = CircuitBreaker::new("test-api", config);

    // Trigger open state
    breaker.record_failure();
    breaker.record_failure();
    assert_eq!(breaker.state(), CircuitState::Open);

    // Wait for cooldown
    std::thread::sleep(Duration::from_millis(100));

    // Manually transition to HalfOpen for testing
    breaker.half_open();

    // Record successes to close
    breaker.record_success();
    breaker.record_success();

    assert_eq!(breaker.state(), CircuitState::Closed);
}

#[tokio::test]
async fn test_tool_timeout_handling() {
    let config = TimeoutConfig {
        tool_timeout: Duration::from_millis(20),
        ..Default::default()
    };
    let handler = TimeoutHandler::new(config);

    let result = handler
        .with_tool_timeout(
            async {
                sleep(Duration::from_millis(50)).await;
                Ok::<_, ProviderError>(())
            },
            "slow-tool",
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ProviderError::Timeout(_)));

    // Verify tracking
    let events = handler.tracker().events();
    assert!(events.iter().any(|e| e.endpoint == "tool:slow-tool"));
}

#[test]
fn test_circuit_breaker_status_reporting() {
    let breaker = CircuitBreaker::new("test-api", CircuitBreakerConfig::default());

    // Initial state
    let status = breaker.status();
    assert_eq!(status.state, CircuitState::Closed);
    assert!(status.is_available);
    assert_eq!(status.failure_count, 0);

    // After failure
    breaker.record_failure();
    let status = breaker.status();
    assert_eq!(status.failure_count, 1);
    assert!(status.is_available);

    // After reset
    breaker.reset();
    let status = breaker.status();
    assert_eq!(status.failure_count, 0);
    assert_eq!(status.state, CircuitState::Closed);
}

#[tokio::test]
async fn test_concurrent_timeouts_multiple_endpoints() {
    let handler = TimeoutHandler::default_config();
    let mut config = handler.config().clone();
    config.default_timeout = Duration::from_millis(50);
    let handler = TimeoutHandler::new(config);

    // Spawn multiple concurrent timeout operations
    let futures = (0..5).map(|i| {
        let handler = handler.clone();
        async move {
            handler
                .with_timeout(
                    async {
                        sleep(Duration::from_millis(100)).await;
                        Ok::<_, ProviderError>(i)
                    },
                    &format!("endpoint-{}", i),
                    None,
                )
                .await
        }
    });

    // All should timeout
    let results = futures::future::join_all(futures).await;
    for result in &results {
        assert!(result.is_err());
    }

    // All should be tracked
    let tracker = handler.tracker();
    let events = tracker.events();
    assert_eq!(events.len(), 5);
}

#[test]
fn test_circuit_breaker_half_open_recovery() {
    let config = CircuitBreakerConfig {
        failure_threshold: 1,
        success_threshold: 2,
        cooldown_duration: Duration::from_millis(50),
    };
    let breaker = CircuitBreaker::new("test-api", config);

    // Open circuit
    breaker.record_failure();
    assert_eq!(breaker.state(), CircuitState::Open);

    // Manually move to HalfOpen after cooldown
    std::thread::sleep(Duration::from_millis(100));
    breaker.half_open();

    // First success in HalfOpen
    breaker.record_success();
    assert_eq!(breaker.state(), CircuitState::HalfOpen); // Not closed yet

    // Second success closes circuit
    breaker.record_success();
    assert_eq!(breaker.state(), CircuitState::Closed);
}

#[test]
fn test_timeout_config_builder_pattern() {
    let config = TimeoutConfig::custom(
        Duration::from_secs(15),
        Duration::from_secs(10),
        Duration::from_secs(30),
        Duration::from_mins(1),
        Duration::from_secs(20),
    );

    assert_eq!(config.default_timeout, Duration::from_secs(15));
    assert_eq!(config.haiku_timeout, Duration::from_secs(10));
    assert_eq!(config.sonnet_timeout, Duration::from_secs(30));
    assert_eq!(config.opus_timeout, Duration::from_mins(1));
    assert_eq!(config.tool_timeout, Duration::from_secs(20));
}
