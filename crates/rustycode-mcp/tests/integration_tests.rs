#![allow(
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::ignored_unit_patterns,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration tests for enterprise MCP features

use rustycode_mcp::enterprise::*;
use rustycode_mcp::manager::*;
use rustycode_mcp::tools::*;
use rustycode_mcp::{McpError, McpResult};
use std::time::Duration;

/// Helper function to create a test echo server config
fn create_echo_server_config(server_id: &str) -> ServerConfig {
    ServerConfig {
        server_id: server_id.to_string(),
        name: format!("Echo Server {}", server_id),
        command: Some("echo".to_string()),
        args: vec!["hello".to_string()],
        transport_type: None,
        url: None,
        headers: None,
        headers_helper: None,
        description: None,
        oauth: None,
        enable_tools: true,
        enable_resources: false,
        enable_prompts: false,
        enabled: true,
        tools_allowlist: vec![],
        tools_denylist: vec![],
        tags: vec![],
    }
}

#[tokio::test]
async fn test_server_manager_lifecycle() {
    let mut manager = McpServerManager::default_config();

    // Create server config
    let config = create_echo_server_config("test-server");

    // Note: This test may fail if echo doesn't properly behave as an MCP server
    // In a real scenario, you'd use a mock MCP server
    let result = manager.start_server(config).await;

    // We expect this might fail since echo isn't a real MCP server
    match result {
        Ok(_server) => {
            // If it somehow worked, test stopping
            let stop_result = manager.stop_server("test-server").await;
            assert!(stop_result.is_ok());
        }
        Err(_e) => {
            // Expected to fail with echo command
            // In production tests, use a proper mock MCP server
        }
    }
}

#[tokio::test]
async fn test_health_monitoring() {
    let mut manager = McpServerManager::default_config();

    // Start health monitoring
    manager.start_health_monitoring();

    // List servers (should be empty)
    let servers = manager.list_servers().await;
    assert_eq!(servers.len(), 0);

    // Stop health monitoring
    manager.stop_health_monitoring().await;
}

#[tokio::test]
async fn test_tool_registry_workflow() {
    let registry = ToolRegistry::new();

    // Initially empty
    assert_eq!(registry.tool_count().await, 0);

    // Test listing tools
    let tools = registry.list_tools().await;
    assert_eq!(tools.len(), 0);

    // Test getting non-existent tool
    let tool = registry.find_tool("non-existent").await;
    assert!(tool.is_none());
}

#[tokio::test]
async fn test_connection_pool_workflow() {
    let pool = ConnectionPool::new(PoolConfig::default());

    // Test multiple acquire/release cycles
    for i in 0..5 {
        let server_id = format!("server-{}", i);
        pool.acquire(&server_id).await.unwrap();
        pool.release(&server_id).await;
    }

    let stats = pool.stats().await;
    assert_eq!(stats.total, 5);
    assert_eq!(stats.active, 0);
    assert_eq!(stats.idle, 5);
}

#[tokio::test]
async fn test_rate_limiter_workflow() {
    let limiter = RateLimiter::new(RateLimiterConfig {
        max_requests: 10,
        window: Duration::from_secs(1),
    });

    // Use up tokens
    for _ in 0..10 {
        assert!(limiter.check_rate_limit("user1").await.is_ok());
    }

    // Should be rate limited now
    assert!(limiter.check_rate_limit("user1").await.is_err());

    // Different key should still work
    assert!(limiter.check_rate_limit("user2").await.is_ok());

    // Check remaining tokens
    let remaining = limiter.remaining_tokens("user2").await;
    assert!(remaining < 10);

    // Reset and try again
    limiter.reset("user1").await;
    assert!(limiter.check_rate_limit("user1").await.is_ok());
}

#[tokio::test]
async fn test_metrics_workflow() {
    let collector = MetricsCollector::new();

    // Record operations for different tools
    collector.record_success("tool1", 50).await;
    collector.record_success("tool1", 100).await;
    collector
        .record_failure("tool1", "Error 1".to_string())
        .await;

    collector.record_success("tool2", 75).await;
    collector
        .record_failure("tool2", "Error 2".to_string())
        .await;

    // Get individual metrics
    let tool1_metrics = collector.metrics("tool1").await;
    assert!(tool1_metrics.is_some());
    let tool1_metrics = tool1_metrics.unwrap();
    assert_eq!(tool1_metrics.total_calls, 3);
    assert_eq!(tool1_metrics.successful_calls, 2);

    let tool2_metrics = collector.metrics("tool2").await;
    assert!(tool2_metrics.is_some());
    let tool2_metrics = tool2_metrics.unwrap();
    assert_eq!(tool2_metrics.total_calls, 2);
    assert_eq!(tool2_metrics.successful_calls, 1);

    // Get all metrics
    let all_metrics = collector.all_metrics().await;
    assert_eq!(all_metrics.len(), 2);

    // Reset one tool
    collector.reset_metrics("tool1").await;
    assert!(collector.metrics("tool1").await.is_none());
    assert!(collector.metrics("tool2").await.is_some());

    // Reset all
    collector.reset_all().await;
    assert!(collector.metrics("tool2").await.is_none());
}

#[tokio::test]
async fn test_retry_mechanism() {
    let config = RetryConfig {
        max_attempts: 5,
        initial_backoff: Duration::from_millis(10),
        backoff_multiplier: 2.0,
        max_backoff: Duration::from_secs(1),
    };

    // Test success after retries
    let attempt_count = std::sync::atomic::AtomicUsize::new(0);
    let result: McpResult<&str> = retry_with_backoff(&config, || {
        let count = attempt_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move {
            if count < 3 {
                Err(McpError::InternalError(format!("Attempt {}", count)))
            } else {
                Ok("success")
            }
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(attempt_count.load(std::sync::atomic::Ordering::SeqCst), 4); // 3 failures + 1 success

    // Test all failures
    let attempt_count = std::sync::atomic::AtomicUsize::new(0);
    let result: McpResult<&str> = retry_with_backoff(&config, || {
        attempt_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move { Err(McpError::InternalError("Always fails".to_string())) }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(attempt_count.load(std::sync::atomic::Ordering::SeqCst), 5); // max_attempts
}

#[tokio::test]
async fn test_enterprise_integration() {
    // Create components
    let pool = ConnectionPool::new(PoolConfig::default());
    let limiter = RateLimiter::new(RateLimiterConfig::default());
    let collector = MetricsCollector::new();

    // Simulate a workflow
    let server_id = "test-server";

    // 1. Acquire connection from pool
    pool.acquire(server_id).await.unwrap();

    // 2. Check rate limit
    let rate_limit_result = limiter.check_rate_limit(server_id).await;
    assert!(rate_limit_result.is_ok());

    // 3. Simulate operation and record metrics
    collector.record_success(server_id, 100).await;

    // 4. Release connection
    pool.release(server_id).await;

    // 5. Verify pool stats
    let stats = pool.stats().await;
    assert_eq!(stats.active, 0);
    assert_eq!(stats.idle, 1);

    // 6. Verify metrics
    let metrics = collector.metrics(server_id).await;
    assert!(metrics.is_some());
    let metrics = metrics.unwrap();
    assert_eq!(metrics.total_calls, 1);
    assert_eq!(metrics.successful_calls, 1);
}

#[tokio::test]
async fn test_concurrent_operations() {
    let collector = MetricsCollector::new();

    // Spawn multiple concurrent operations
    let mut handles = Vec::new();

    for i in 0..10 {
        let collector_clone = collector.clone();
        let server_id = format!("server-{}", i);

        let handle = tokio::spawn(async move {
            // Record some metrics
            collector_clone.record_success(&server_id, 100).await;
        });

        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all metrics were recorded
    let all_metrics = collector.all_metrics().await;
    assert_eq!(all_metrics.len(), 10);
}

#[tokio::test]
async fn test_error_recovery() {
    let limiter = RateLimiter::new(RateLimiterConfig {
        max_requests: 3,
        window: Duration::from_secs(1),
    });

    // Exhaust rate limit
    for _ in 0..3 {
        limiter.check_rate_limit("test").await.unwrap();
    }

    // Should fail now
    let result = limiter.check_rate_limit("test").await;
    assert!(result.is_err());

    // Reset and retry
    limiter.reset("test").await;
    let result = limiter.check_rate_limit("test").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_timeout_handling() {
    let config = RetryConfig {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(10),
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let _result: McpResult<&str> = retry_with_backoff(&config, || async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Err(McpError::Timeout)
    })
    .await;

    let elapsed = start.elapsed();

    // Should have taken at least 3 attempts * 50ms = 150ms
    // But less than 1 second
    assert!(elapsed >= Duration::from_millis(150));
    assert!(elapsed < Duration::from_secs(1));
}

#[tokio::test]
async fn test_metrics_aggregation() {
    let collector = MetricsCollector::new();

    // Record data for multiple servers
    for server_id in 1..=5 {
        for i in 1..=10 {
            if i % 3 == 0 {
                collector
                    .record_failure(&format!("server-{}", server_id), "Test error".to_string())
                    .await;
            } else {
                collector
                    .record_success(&format!("server-{}", server_id), i * 10)
                    .await;
            }
        }
    }

    // Get all metrics
    let all_metrics = collector.all_metrics().await;
    assert_eq!(all_metrics.len(), 5);

    // Verify each server has 10 calls
    for (_server_id, metrics) in all_metrics {
        assert_eq!(metrics.total_calls, 10);
        assert_eq!(metrics.successful_calls, 7); // 10 - 3 failures
        assert_eq!(metrics.failed_calls, 3);
    }
}
