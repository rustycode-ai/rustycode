//! Enterprise features: connection pooling, rate limiting, metrics, etc.

use crate::{McpError, McpResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::debug;

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum pool size
    pub max_size: usize,
    /// Minimum idle connections
    pub min_idle: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Idle timeout
    pub idle_timeout: Duration,
    /// Max connection lifetime
    pub max_lifetime: Option<Duration>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,
            min_idle: 2,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_mins(10),
            max_lifetime: Some(Duration::from_hours(1)),
        }
    }
}

/// Pooled connection
#[derive(Debug)]
struct PooledConnection {
    #[allow(dead_code)] // Kept for future use
    server_id: String,
    #[allow(dead_code)] // Kept for future use
    created_at: Instant,
    last_used: Instant,
    in_use: bool,
}

impl PooledConnection {
    fn new(server_id: String) -> Self {
        let now = Instant::now();
        Self {
            server_id,
            created_at: now,
            last_used: now,
            in_use: false,
        }
    }
}

/// Connection pool for MCP servers
pub struct ConnectionPool {
    config: PoolConfig,
    connections: Arc<RwLock<HashMap<String, PooledConnection>>>,
    semaphore: Arc<Semaphore>,
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_size));
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            semaphore,
        }
    }

    /// Acquire a connection from the pool
    pub async fn acquire(&self, server_id: &str) -> McpResult<()> {
        debug!("Acquiring connection for server '{}'", server_id);

        // Wait for semaphore permit
        let _permit =
            tokio::time::timeout(self.config.connection_timeout, self.semaphore.acquire())
                .await
                .map_err(|_| McpError::Timeout)?
                .map_err(|e| McpError::InternalError(format!("Failed to acquire permit: {e}")))?;

        // Check if connection exists and is valid
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_id) {
            if !conn.in_use {
                conn.in_use = true;
                conn.last_used = Instant::now();
                return Ok(());
            }
        }

        // Create new connection
        let mut conn = PooledConnection::new(server_id.to_string());
        conn.in_use = true;

        connections.insert(server_id.to_string(), conn);
        drop(connections);

        // Permit is released when dropped
        Ok(())
    }

    /// Release a connection back to the pool
    pub async fn release(&self, server_id: &str) {
        debug!("Releasing connection for server '{}'", server_id);

        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_id) {
            conn.in_use = false;
            conn.last_used = Instant::now();
        }
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let connections = self.connections.read().await;
        let total = connections.len();
        let active = connections.values().filter(|c| c.in_use).count();
        let idle = total - active;

        PoolStats {
            total,
            active,
            idle,
        }
    }

    /// Clean up idle connections
    pub async fn cleanup_idle(&self) -> usize {
        let mut connections = self.connections.write().await;
        let now = Instant::now();
        let before = connections.len();

        connections.retain(|_, conn| {
            if conn.in_use {
                true
            } else {
                now.duration_since(conn.last_used) < self.config.idle_timeout
            }
        });

        before - connections.len()
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total: usize,
    pub active: usize,
    pub idle: usize,
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Maximum requests per window
    pub max_requests: usize,
    /// Time window
    pub window: Duration,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_mins(1),
        }
    }
}

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    config: RateLimiterConfig,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
}

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
    capacity: usize,
}

impl TokenBucket {
    fn new(capacity: usize) -> Self {
        Self {
            tokens: capacity as f64,
            last_update: Instant::now(),
            capacity,
        }
    }

    fn try_consume(&mut self, tokens: usize, refill_rate: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.last_update = now;

        // Refill tokens
        self.tokens = refill_rate
            .mul_add(elapsed, self.tokens)
            .min(self.capacity as f64);

        if self.tokens >= tokens as f64 {
            self.tokens -= tokens as f64;
            true
        } else {
            false
        }
    }

    fn time_until_available(&self, tokens: usize, refill_rate: f64) -> Option<Duration> {
        if self.tokens >= tokens as f64 {
            return Some(Duration::ZERO);
        }

        let needed = tokens as f64 - self.tokens;
        if refill_rate <= 0.0 {
            return None;
        }

        let seconds = needed / refill_rate;
        Some(Duration::from_secs_f64(seconds))
    }
}

impl RateLimiter {
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check if a request is allowed
    pub async fn check_rate_limit(&self, key: &str) -> McpResult<()> {
        let mut buckets = self.buckets.lock().await;
        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.max_requests));

        let refill_rate = self.config.max_requests as f64 / self.config.window.as_secs_f64();

        if bucket.try_consume(1, refill_rate) {
            Ok(())
        } else {
            let wait_time = bucket.time_until_available(1, refill_rate);
            Err(McpError::RateLimited(
                wait_time.unwrap_or(Duration::from_mins(1)),
            ))
        }
    }

    /// Get remaining tokens for a key
    pub async fn remaining_tokens(&self, key: &str) -> usize {
        let mut buckets = self.buckets.lock().await;
        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.max_requests));

        let refill_rate = self.config.max_requests as f64 / self.config.window.as_secs_f64();
        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_update).as_secs_f64();

        bucket.tokens = refill_rate
            .mul_add(elapsed, bucket.tokens)
            .min(bucket.capacity as f64);
        bucket.last_update = now;

        bucket.tokens as usize
    }

    /// Reset rate limit for a key
    pub async fn reset(&self, key: &str) {
        let mut buckets = self.buckets.lock().await;
        buckets.remove(key);
    }
}

/// Metrics collector
#[derive(Debug, Clone)]
pub struct Metrics {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub total_latency_ms: u64,
    pub avg_latency_ms: f64,
    pub last_error: Option<String>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            total_calls: 0,
            successful_calls: 0,
            failed_calls: 0,
            total_latency_ms: 0,
            avg_latency_ms: 0.0,
            last_error: None,
        }
    }
}

impl Metrics {
    /// Record a successful call
    pub fn record_success(&mut self, latency_ms: u64) {
        self.total_calls += 1;
        self.successful_calls += 1;
        self.total_latency_ms += latency_ms;
        self.avg_latency_ms = self.total_latency_ms as f64 / self.total_calls as f64;
    }

    /// Record a failed call
    pub fn record_failure(&mut self, error: String) {
        self.total_calls += 1;
        self.failed_calls += 1;
        self.last_error = Some(error);
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_calls == 0 {
            return 1.0;
        }
        self.successful_calls as f64 / self.total_calls as f64
    }
}

/// Metrics collector for MCP operations
#[derive(Clone)]
pub struct MetricsCollector {
    metrics: Arc<RwLock<HashMap<String, Metrics>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a successful call
    pub async fn record_success(&self, key: &str, latency_ms: u64) {
        let mut metrics = self.metrics.write().await;
        let entry = metrics.entry(key.to_string()).or_default();
        entry.record_success(latency_ms);
    }

    /// Record a failed call
    pub async fn record_failure(&self, key: &str, error: String) {
        let mut metrics = self.metrics.write().await;
        let entry = metrics.entry(key.to_string()).or_default();
        entry.record_failure(error);
    }

    /// Get metrics for a key
    pub async fn metrics(&self, key: &str) -> Option<Metrics> {
        let metrics = self.metrics.read().await;
        metrics.get(key).cloned()
    }

    /// Get all metrics
    pub async fn all_metrics(&self) -> HashMap<String, Metrics> {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// Reset metrics for a key
    pub async fn reset_metrics(&self, key: &str) {
        let mut metrics = self.metrics.write().await;
        metrics.remove(key);
    }

    /// Reset all metrics
    pub async fn reset_all(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.clear();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: usize,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Maximum backoff duration
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(30),
        }
    }
}

/// Retry with exponential backoff
pub async fn retry_with_backoff<F, Fut, T>(
    config: &RetryConfig,
    mut operation: F,
) -> Result<T, McpError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, McpError>>,
{
    let mut last_error = None;
    let mut backoff = config.initial_backoff;

    for attempt in 0..config.max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < config.max_attempts - 1 {
                    debug!(
                        "Attempt {} failed, retrying after {:?}",
                        attempt + 1,
                        backoff
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = std::cmp::min(
                        Duration::from_secs_f64(backoff.as_secs_f64() * config.backoff_multiplier),
                        config.max_backoff,
                    );
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| McpError::InternalError("All retry attempts exhausted".to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_pool_stats() {
        let pool = ConnectionPool::new(PoolConfig::default());
        let stats = pool.stats().await;
        assert_eq!(stats.total, 0);
        assert_eq!(stats.active, 0);
        assert_eq!(stats.idle, 0);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(RateLimiterConfig::default());
        let key = "test-key";

        // Should allow first request
        assert!(limiter.check_rate_limit(key).await.is_ok());

        // Should have remaining tokens
        let remaining = limiter.remaining_tokens(key).await;
        assert!(remaining > 0);
    }

    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        let key = "test-key";

        collector.record_success(key, 100).await;
        collector.record_success(key, 200).await;
        collector
            .record_failure(key, "test error".to_string())
            .await;

        let metrics = collector.metrics(key).await;
        assert!(metrics.is_some());
        let metrics = metrics.unwrap();
        assert_eq!(metrics.total_calls, 3);
        assert_eq!(metrics.successful_calls, 2);
        assert_eq!(metrics.failed_calls, 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(10),
            ..Default::default()
        };

        let mut attempt_count = 0;
        let result = retry_with_backoff(&config, || {
            attempt_count += 1;
            async move {
                if attempt_count < 3 {
                    Err(McpError::InternalError("Not yet".to_string()))
                } else {
                    Ok("success")
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempt_count, 3);
    }

    #[test]
    fn test_pool_config_defaults() {
        let config = PoolConfig::default();
        assert_eq!(config.max_size, 10);
        assert_eq!(config.min_idle, 2);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert_eq!(config.idle_timeout, Duration::from_mins(10));
        assert_eq!(config.max_lifetime, Some(Duration::from_hours(1)));
    }

    #[test]
    fn test_rate_limiter_config_defaults() {
        let config = RateLimiterConfig::default();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window, Duration::from_mins(1));
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_backoff, Duration::from_millis(100));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.max_backoff, Duration::from_secs(30));
    }

    #[test]
    fn test_metrics_defaults() {
        let metrics = Metrics::default();
        assert_eq!(metrics.total_calls, 0);
        assert_eq!(metrics.successful_calls, 0);
        assert_eq!(metrics.failed_calls, 0);
        assert_eq!(metrics.total_latency_ms, 0);
        assert_eq!(metrics.avg_latency_ms, 0.0);
        assert!(metrics.last_error.is_none());
    }

    #[test]
    fn test_metrics_success_rate() {
        let mut metrics = Metrics::default();
        // No calls -> 100% success rate
        assert_eq!(metrics.success_rate(), 1.0);

        metrics.record_success(100);
        metrics.record_success(200);
        metrics.record_failure("error".to_string());

        assert_eq!(metrics.total_calls, 3);
        let rate = metrics.success_rate();
        assert!((rate - 0.6666).abs() < 0.01);
    }

    #[test]
    fn test_metrics_record_success_updates_avg_latency() {
        let mut metrics = Metrics::default();
        metrics.record_success(100);
        assert_eq!(metrics.total_latency_ms, 100);
        assert!((metrics.avg_latency_ms - 100.0).abs() < f64::EPSILON);

        metrics.record_success(200);
        assert_eq!(metrics.total_latency_ms, 300);
        assert!((metrics.avg_latency_ms - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metrics_record_failure() {
        let mut metrics = Metrics::default();
        metrics.record_failure("connection refused".to_string());
        assert_eq!(metrics.failed_calls, 1);
        assert_eq!(metrics.total_calls, 1);
        assert_eq!(metrics.last_error, Some("connection refused".to_string()));
    }

    #[tokio::test]
    async fn test_connection_pool_acquire_release() {
        let pool = ConnectionPool::new(PoolConfig::default());

        // Acquire a connection
        pool.acquire("server-1").await.unwrap();
        let stats = pool.stats().await;
        assert_eq!(stats.total, 1);
        assert_eq!(stats.active, 1);
        assert_eq!(stats.idle, 0);

        // Release the connection
        pool.release("server-1").await;
        let stats = pool.stats().await;
        assert_eq!(stats.total, 1);
        assert_eq!(stats.active, 0);
        assert_eq!(stats.idle, 1);
    }

    #[tokio::test]
    async fn test_connection_pool_cleanup_idle() {
        let pool = ConnectionPool::new(PoolConfig {
            idle_timeout: Duration::from_millis(1),
            ..Default::default()
        });

        // Acquire and release to create an idle connection
        pool.acquire("server-1").await.unwrap();
        pool.release("server-1").await;

        // Wait for idle timeout
        tokio::time::sleep(Duration::from_millis(5)).await;

        let cleaned = pool.cleanup_idle().await;
        assert_eq!(cleaned, 1);

        let stats = pool.stats().await;
        assert_eq!(stats.total, 0);
    }

    #[tokio::test]
    async fn test_rate_limiter_exhaustion() {
        let config = RateLimiterConfig {
            max_requests: 2,
            window: Duration::from_mins(1),
        };
        let limiter = RateLimiter::new(config);

        // First two should succeed
        assert!(limiter.check_rate_limit("key").await.is_ok());
        assert!(limiter.check_rate_limit("key").await.is_ok());

        // Third should fail
        let result = limiter.check_rate_limit("key").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), McpError::RateLimited(_)));
    }

    #[tokio::test]
    async fn test_rate_limiter_remaining_tokens() {
        let config = RateLimiterConfig {
            max_requests: 10,
            window: Duration::from_mins(1),
        };
        let limiter = RateLimiter::new(config);

        let remaining = limiter.remaining_tokens("new-key").await;
        assert_eq!(remaining, 10);

        limiter.check_rate_limit("new-key").await.unwrap();
        let remaining = limiter.remaining_tokens("new-key").await;
        assert_eq!(remaining, 9);
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let limiter = RateLimiter::new(RateLimiterConfig::default());
        limiter.check_rate_limit("key").await.unwrap();
        limiter.reset("key").await;
        // After reset, tokens are re-created at full capacity
        let remaining = limiter.remaining_tokens("key").await;
        assert_eq!(remaining, 100);
    }

    #[tokio::test]
    async fn test_metrics_collector_reset() {
        let collector = MetricsCollector::new();
        collector.record_success("key", 50).await;
        collector.record_failure("key", "err".to_string()).await;

        collector.reset_metrics("key").await;
        assert!(collector.metrics("key").await.is_none());

        // Test reset_all
        collector.record_success("key1", 10).await;
        collector.record_success("key2", 20).await;
        collector.reset_all().await;
        assert!(collector.all_metrics().await.is_empty());
    }

    #[tokio::test]
    async fn test_metrics_collector_get_all() {
        let collector = MetricsCollector::new();
        collector.record_success("a", 100).await;
        collector.record_success("b", 200).await;

        let all = collector.all_metrics().await;
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("a"));
        assert!(all.contains_key("b"));
    }

    #[tokio::test]
    async fn test_retry_with_backoff_all_fail() {
        let config = RetryConfig {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(1),
            ..Default::default()
        };

        let result: Result<String, McpError> = retry_with_backoff(&config, || async {
            Err(McpError::InternalError("always fail".to_string()))
        })
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("always fail"));
    }

    #[tokio::test]
    async fn test_retry_with_backoff_first_success() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            ..Default::default()
        };

        let result = retry_with_backoff(&config, || async { Ok("immediate") }).await;
        assert_eq!(result.unwrap(), "immediate");
    }

    #[tokio::test]
    async fn test_pool_stats_clone() {
        let stats = PoolStats {
            total: 5,
            active: 2,
            idle: 3,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.total, 5);
        assert_eq!(cloned.active, 2);
        assert_eq!(cloned.idle, 3);
    }
}
