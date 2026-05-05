//! HTTP client management with connection pooling.
//!
//! This module provides:
//! - Shared HTTP client with connection pooling
//! - Configurable connection limits and timeouts
//! - Proper TLS configuration
//! - Connection reuse across providers

use anyhow::Result;
use reqwest::Client;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::RwLock;

/// HTTP client pool configuration
#[derive(Debug, Clone)]
pub struct ClientPoolConfig {
    /// Maximum number of idle connections per host
    pub max_idle_per_host: usize,
    /// Maximum number of idle connections in the pool
    pub pool_max_idle: usize,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Read timeout
    pub read_timeout: Duration,
    /// Whether to use HTTP/2
    pub http2: bool,
    /// Minimum number of connections to keep alive
    pub pool_min_idle: usize,
}

impl Default for ClientPoolConfig {
    fn default() -> Self {
        Self {
            max_idle_per_host: 10,
            pool_max_idle: 100,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_mins(2),
            http2: true,
            pool_min_idle: 5,
        }
    }
}

impl ClientPoolConfig {
    /// Create a new config with optimized settings for high throughput
    pub fn high_throughput() -> Self {
        Self {
            max_idle_per_host: 50,
            pool_max_idle: 200,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_mins(5),
            http2: true,
            pool_min_idle: 10,
        }
    }

    /// Create a new config with optimized settings for low latency
    pub fn low_latency() -> Self {
        Self {
            max_idle_per_host: 20,
            pool_max_idle: 100,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(30),
            http2: true,
            pool_min_idle: 5,
        }
    }
}

/// Shared HTTP client pool
pub struct ClientPool {
    client: Arc<Client>,
    config: ClientPoolConfig,
    stats: Arc<RwLock<PoolStats>>,
}

#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    pub total_requests: u64,
}

impl ClientPool {
    pub fn new() -> Result<Self> {
        Self::with_config(ClientPoolConfig::default())
    }

    /// Create a new client pool with custom configuration
    pub fn with_config(config: ClientPoolConfig) -> Result<Self> {
        let mut builder = Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.read_timeout)
            .pool_max_idle_per_host(config.max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_mins(1))
            .tcp_nodelay(true);

        // Configure HTTP/2 based on config
        // Note: do NOT use http2_prior_knowledge() — it skips ALPN negotiation
        // and assumes the server speaks raw HTTP/2. Most API servers (including
        // OpenAI-compatible proxies) require proper TLS ALPN negotiation.
        if !config.http2 {
            builder = builder.http1_only();
        }

        let client = builder.build()?;

        Ok(Self {
            client: Arc::new(client),
            config,
            stats: Arc::new(RwLock::new(PoolStats::default())),
        })
    }

    /// Get a shared HTTP client
    pub fn client(&self) -> Arc<Client> {
        self.client.clone()
    }

    /// Increment the request counter (called internally by execute methods)
    pub async fn record_request(&self) {
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;
    }

    /// Get current pool statistics
    pub async fn stats(&self) -> PoolStats {
        self.stats.read().await.clone()
    }

    /// Get the pool configuration
    pub fn config(&self) -> &ClientPoolConfig {
        &self.config
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = PoolStats::default();
    }
}

/// Global client pool instance
static GLOBAL_POOL: OnceLock<Arc<ClientPool>> = OnceLock::new();

/// Get the global HTTP client pool
pub fn global_pool() -> &'static Arc<ClientPool> {
    GLOBAL_POOL.get_or_init(|| {
        Arc::new(ClientPool::new().unwrap_or_else(|e| {
            tracing::error!("Failed to create global client pool, using fallback: {}", e);
            // Fallback: use minimal config that is less likely to fail
            ClientPool::with_config(ClientPoolConfig {
                http2: false,
                ..ClientPoolConfig::default()
            })
            .unwrap_or_else(|fallback_err| {
                tracing::error!(
                    "Fallback client pool creation also failed: {}. Using bare reqwest client.",
                    fallback_err
                );
                ClientPool {
                    client: Arc::new(Client::new()),
                    config: ClientPoolConfig::default(),
                    stats: Arc::new(RwLock::new(PoolStats::default())),
                }
            })
        }))
    })
}

/// Get a client from the global pool
pub fn global_client() -> Arc<Client> {
    global_pool().client()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_pool_creation() {
        let pool = ClientPool::new();
        assert!(pool.is_ok());
    }

    #[test]
    fn test_client_pool_with_config() {
        let config = ClientPoolConfig {
            max_idle_per_host: 5,
            pool_max_idle: 10,
            ..Default::default()
        };
        let pool = ClientPool::with_config(config);
        assert!(pool.is_ok());
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let pool = ClientPool::new().unwrap();
        let _client = pool.client();
        pool.record_request().await;

        let stats = pool.stats().await;
        assert_eq!(stats.total_requests, 1);
    }

    #[tokio::test]
    async fn test_pool_stats_reset() {
        let pool = ClientPool::new().unwrap();
        let _client = pool.client();

        pool.reset_stats().await;
        let stats = pool.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[test]
    fn test_high_throughput_config() {
        let config = ClientPoolConfig::high_throughput();
        assert_eq!(config.max_idle_per_host, 50);
        assert_eq!(config.connect_timeout.as_secs(), 5);
    }

    #[test]
    fn test_low_latency_config() {
        let config = ClientPoolConfig::low_latency();
        assert_eq!(config.max_idle_per_host, 20);
        assert_eq!(config.connect_timeout.as_secs(), 2);
    }

    #[test]
    fn test_global_pool_returns_valid_pool() {
        let pool = global_pool();

        // Verify the pool has a usable client (Arc<Client> is always present
        // if the pool was created successfully)
        let _client = pool.client();

        // Verify the pool config is populated
        let config = pool.config();
        assert_eq!(
            config.max_idle_per_host, 10,
            "default config should have max_idle_per_host = 10"
        );
        assert_eq!(
            config.pool_max_idle, 100,
            "default config should have pool_max_idle = 100"
        );
    }

    #[tokio::test]
    async fn test_global_pool_records_requests() {
        let pool = global_pool();

        // Reset to a known baseline, record a request, and verify the counter
        pool.reset_stats().await;
        pool.record_request().await;
        pool.record_request().await;

        let stats = pool.stats().await;
        assert_eq!(
            stats.total_requests, 2,
            "global pool should track recorded requests"
        );

        // Clean up so other tests are not affected
        pool.reset_stats().await;
    }

    #[test]
    fn test_global_client_returns_shared_client() {
        let client_a = global_client();
        let client_b = global_client();

        // Both should point to the same underlying reqwest::Client (Arc comparison)
        assert!(
            Arc::ptr_eq(&client_a, &client_b),
            "global_client should always return the same shared client"
        );
    }
}
