//! Tool orchestration and management with performance optimizations

use crate::enterprise::{retry_with_backoff, MetricsCollector, RetryConfig};
use crate::manager::McpServer;
use crate::types::*;
use crate::{McpError, McpResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, error, info, warn};

/// Tool call request
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Server to call the tool on
    pub server_id: String,
    /// Tool name
    pub tool_name: String,
    /// Tool arguments
    pub arguments: Value,
    /// Optional request timeout
    pub timeout: Option<Duration>,
}

impl ToolCall {
    pub const fn new(server_id: String, tool_name: String, arguments: Value) -> Self {
        Self {
            server_id,
            tool_name,
            arguments,
            timeout: None,
        }
    }

    /// Set timeout for this call
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Tool execution result with metadata
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// Tool result
    pub result: McpToolResult,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Server that executed the tool
    pub server_id: String,
    /// Tool name
    pub tool_name: String,
    /// Whether result was cached
    pub cached: bool,
}

/// Tool result cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    result: McpToolResult,
    timestamp: Instant,
    hits: usize,
}

/// Tool result cache with TTL
pub struct ToolCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: Duration,
    max_size: usize,
}

impl ToolCache {
    pub fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
            max_size,
        }
    }

    /// Generate cache key
    fn cache_key(server_id: &str, tool_name: &str, args: &Value) -> String {
        format!("{server_id}:{tool_name}:{args}")
    }

    /// Get cached result
    pub async fn get(
        &self,
        server_id: &str,
        tool_name: &str,
        args: &Value,
    ) -> Option<McpToolResult> {
        let key = Self::cache_key(server_id, tool_name, args);
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(&key) {
            if entry.timestamp.elapsed() < self.ttl {
                entry.hits += 1;
                debug!("Cache hit for tool '{}'", tool_name);
                return Some(entry.result.clone());
            }
            // Expired, remove it
            cache.remove(&key);
        }
        None
    }

    /// Put result in cache
    pub async fn put(&self, server_id: &str, tool_name: &str, args: &Value, result: McpToolResult) {
        let key = Self::cache_key(server_id, tool_name, args);
        let mut cache = self.cache.write().await;

        // Evict oldest if at capacity
        if cache.len() >= self.max_size {
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.timestamp)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&oldest_key);
            }
        }

        cache.insert(
            key,
            CacheEntry {
                result,
                timestamp: Instant::now(),
                hits: 0,
            },
        );
    }

    /// Clear expired entries
    pub async fn cleanup(&self) -> usize {
        let mut cache = self.cache.write().await;
        let before = cache.len();
        cache.retain(|_, entry| entry.timestamp.elapsed() < self.ttl);
        before - cache.len()
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total_entries = cache.len();
        let total_hits: usize = cache.values().map(|e| e.hits).sum();

        CacheStats {
            total_entries,
            total_hits,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub total_hits: usize,
}

/// Tool registry for managing available tools
pub struct ToolRegistry {
    /// All discovered tools by name
    tools: Arc<RwLock<HashMap<String, McpTool>>>,
    /// Mapping of `server_id` to tool names
    server_tools: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Mapping of tool name to `server_id`
    tool_servers: Arc<RwLock<HashMap<String, String>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            server_tools: Arc::new(RwLock::new(HashMap::new())),
            tool_servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Discover tools from a server
    pub async fn discover_tools(&mut self, server: &McpServer) -> McpResult<Vec<McpTool>> {
        let server_id = server.id().to_string();
        info!("Discovering tools from server '{}'", server_id);

        let client_lock = server.client();
        let client = client_lock.read().await;
        let tools = client
            .list_tools(&server_id)
            .await
            .map_err(|e| McpError::ProtocolError(format!("Failed to discover tools: {e}")))?;
        drop(client);

        // Update registry
        let mut tool_names = Vec::new();
        for tool in &tools {
            let tool_name = tool.name.clone();

            // Check for conflicts
            let mut tool_servers = self.tool_servers.write().await;
            if let Some(existing_server) = tool_servers.get(&tool_name) {
                warn!(
                    "Tool '{}' already registered on server '{}', skipping",
                    tool_name, existing_server
                );
                continue;
            }

            tool_servers.insert(tool_name.clone(), server_id.clone());
            drop(tool_servers);

            tool_names.push(tool_name.clone());

            let mut tools_map = self.tools.write().await;
            tools_map.insert(tool_name.clone(), tool.clone());
        }

        // Update server tools mapping
        let mut server_tools = self.server_tools.write().await;
        server_tools.insert(server_id.clone(), tool_names.clone());

        info!(
            "Discovered {} tools from server '{}'",
            tool_names.len(),
            server_id
        );

        Ok(tools)
    }

    /// Call a tool by name
    pub async fn call_tool(
        &self,
        tool_name: &str,
        _arguments: Value,
    ) -> McpResult<ToolExecutionResult> {
        debug!("Calling tool '{}'", tool_name);

        // Find server for this tool
        let tool_servers = self.tool_servers.read().await;
        let _server_id = tool_servers
            .get(tool_name)
            .ok_or_else(|| McpError::ToolNotFound(tool_name.to_string()))?;
        drop(tool_servers);

        // This would need access to the server manager
        // For now, return an error indicating the need for server access
        Err(McpError::InternalError(
            "Tool calling requires server manager access".to_string(),
        ))
    }

    /// Call tools in parallel
    pub async fn call_tool_parallel(
        &self,
        calls: Vec<ToolCall>,
    ) -> Vec<Result<ToolExecutionResult, McpError>> {
        info!("Calling {} tools in parallel", calls.len());

        let futures: Vec<_> = calls
            .into_iter()
            .map(|_call| async {
                // This would need access to the server manager
                // For now, return an error
                Err(McpError::InternalError(
                    "Tool calling requires server manager access".to_string(),
                ))
            })
            .collect();

        futures::future::join_all(futures).await
    }

    /// List all tools
    pub async fn list_tools(&self) -> Vec<McpTool> {
        let tools = self.tools.read().await;
        tools.values().cloned().collect()
    }

    /// Get tool by name
    pub async fn find_tool(&self, name: &str) -> Option<McpTool> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }

    /// List tools for a specific server
    pub async fn list_server_tools(&self, server_id: &str) -> Vec<McpTool> {
        let server_tools = self.server_tools.read().await;
        let tool_names = match server_tools.get(server_id) {
            Some(names) => names.clone(),
            None => return Vec::new(),
        };
        drop(server_tools);

        let tools = self.tools.read().await;
        tool_names
            .iter()
            .filter_map(|name| tools.get(name).cloned())
            .collect()
    }

    /// Remove all tools for a server
    pub async fn remove_server_tools(&mut self, server_id: &str) {
        info!("Removing tools for server '{}'", server_id);

        let tool_names = {
            let mut server_tools = self.server_tools.write().await;
            server_tools.remove(server_id).unwrap_or_default()
        };

        let mut tools = self.tools.write().await;
        let mut tool_servers = self.tool_servers.write().await;

        for tool_name in tool_names {
            tools.remove(&tool_name);
            tool_servers.remove(&tool_name);
        }
    }

    /// Get tool count
    pub async fn tool_count(&self) -> usize {
        let tools = self.tools.read().await;
        tools.len()
    }

    /// Get server count
    pub async fn server_count(&self) -> usize {
        let server_tools = self.server_tools.read().await;
        server_tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Optimized tool execution engine
pub struct ToolExecutionEngine {
    registry: ToolRegistry,
    cache: ToolCache,
    metrics: MetricsCollector,
    retry_config: RetryConfig,
    semaphore: Arc<Semaphore>,
}

impl ToolExecutionEngine {
    pub fn new(cache_ttl: Duration, cache_size: usize, max_concurrent: usize) -> Self {
        Self {
            registry: ToolRegistry::new(),
            cache: ToolCache::new(cache_ttl, cache_size),
            metrics: MetricsCollector::new(),
            retry_config: RetryConfig::default(),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Set retry configuration
    pub const fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Execute a tool with caching, retries, and metrics
    pub async fn execute_tool(
        &self,
        server: &McpServer,
        tool_name: &str,
        arguments: Value,
    ) -> McpResult<ToolExecutionResult> {
        let server_id = server.id().to_string();
        let start = Instant::now();

        // Check cache first
        if let Some(cached_result) = self.cache.get(&server_id, tool_name, &arguments).await {
            let execution_time_ms = start.elapsed().as_millis() as u64;
            self.metrics
                .record_success(&format!("{server_id}:{tool_name}"), execution_time_ms)
                .await;

            return Ok(ToolExecutionResult {
                result: cached_result,
                execution_time_ms,
                server_id,
                tool_name: tool_name.to_string(),
                cached: true,
            });
        }

        // Acquire semaphore permit for concurrency control
        let _permit =
            self.semaphore.acquire().await.map_err(|e| {
                McpError::InternalError(format!("Failed to acquire semaphore: {e}"))
            })?;

        // Clone arguments for cache and retry
        let arguments_clone = arguments.clone();

        // Execute with retry logic
        let result = retry_with_backoff(&self.retry_config, || {
            let server = server.clone();
            let tool_name = tool_name.to_string();
            let arguments = arguments.clone();
            async move { Self::call_tool_internal_static(&server, &tool_name, arguments).await }
        })
        .await?;

        // Cache the result
        self.cache
            .put(&server_id, tool_name, &arguments_clone, result.clone())
            .await;

        let execution_time_ms = start.elapsed().as_millis() as u64;
        self.metrics
            .record_success(&format!("{server_id}:{tool_name}"), execution_time_ms)
            .await;

        Ok(ToolExecutionResult {
            result,
            execution_time_ms,
            server_id,
            tool_name: tool_name.to_string(),
            cached: false,
        })
    }

    /// Internal tool call implementation (static to work with retry closure)
    async fn call_tool_internal_static(
        server: &McpServer,
        tool_name: &str,
        arguments: Value,
    ) -> McpResult<McpToolResult> {
        let client_lock = server.client();
        let client = client_lock.read().await;
        client
            .call_tool(server.id(), tool_name, arguments)
            .await
            .map_err(|e| McpError::CallFailed(format!("Tool call failed: {e}")))
    }

    /// Execute multiple tools in parallel with automatic batching
    pub async fn execute_tools_parallel(
        &self,
        calls: Vec<(McpServer, String, Value)>,
    ) -> Vec<ToolExecutionResult> {
        info!("Executing {} tools in parallel", calls.len());

        // Use FuturesUnordered for better performance with large batches
        use futures::stream::{FuturesUnordered, StreamExt};

        let futures = FuturesUnordered::new();

        for (server, tool_name, arguments) in calls {
            let engine = self;
            futures.push(async move {
                match engine.execute_tool(&server, &tool_name, arguments).await {
                    Ok(result) => result,
                    Err(e) => {
                        error!("Tool execution failed: {}", e);
                        ToolExecutionResult {
                            result: McpToolResult {
                                content: vec![McpContent::Text {
                                    text: format!("Error: {e}"),
                                }],
                                is_error: Some(true),
                                structured_content: None,
                                meta: None,
                            },
                            execution_time_ms: 0,
                            server_id: server.id().to_string(),
                            tool_name,
                            cached: false,
                        }
                    }
                }
            });
        }

        futures.collect().await
    }

    /// Discover tools from a server
    pub async fn discover_tools(&mut self, server: &McpServer) -> McpResult<Vec<McpTool>> {
        self.registry.discover_tools(server).await
    }

    /// List all tools
    pub async fn list_tools(&self) -> Vec<McpTool> {
        self.registry.list_tools().await
    }

    /// Get tool by name
    pub async fn find_tool(&self, name: &str) -> Option<McpTool> {
        self.registry.find_tool(name).await
    }

    /// Get metrics
    pub async fn metrics(&self, key: &str) -> Option<crate::enterprise::Metrics> {
        self.metrics.metrics(key).await
    }

    /// Get all metrics
    pub async fn all_metrics(&self) -> HashMap<String, crate::enterprise::Metrics> {
        self.metrics.all_metrics().await
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        self.cache.stats().await
    }

    /// Cleanup expired cache entries
    pub async fn cleanup_cache(&self) -> usize {
        self.cache.cleanup().await
    }
}

impl Default for ToolExecutionEngine {
    fn default() -> Self {
        Self::new(Duration::from_mins(5), 1000, 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_registry_creation() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.tool_count().await, 0);
        assert_eq!(registry.server_count().await, 0);
    }

    #[tokio::test]
    async fn test_tool_call_creation() {
        let call = ToolCall::new(
            "test-server".to_string(),
            "test_tool".to_string(),
            serde_json::json!({"param": "value"}),
        );

        assert_eq!(call.server_id, "test-server");
        assert_eq!(call.tool_name, "test_tool");
        assert!(call.timeout.is_none());
    }

    #[tokio::test]
    async fn test_tool_execution_result() {
        let result = ToolExecutionResult {
            result: McpToolResult {
                content: vec![McpContent::Text {
                    text: "output".to_string(),
                }],
                is_error: None,
                structured_content: None,
                meta: None,
            },
            execution_time_ms: 100,
            server_id: "test-server".to_string(),
            tool_name: "test_tool".to_string(),
            cached: false,
        };

        assert_eq!(result.execution_time_ms, 100);
        assert_eq!(result.server_id, "test-server");
        assert_eq!(result.tool_name, "test_tool");
        assert!(!result.cached);
    }

    #[tokio::test]
    async fn test_tool_cache() {
        let cache = ToolCache::new(Duration::from_mins(1), 10);

        // Test cache miss
        let result = cache
            .get("server1", "tool1", &serde_json::json!({"key": "value"}))
            .await;
        assert!(result.is_none());

        // Test cache put
        let tool_result = McpToolResult {
            content: vec![McpContent::Text {
                text: "test result".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };
        cache
            .put(
                "server1",
                "tool1",
                &serde_json::json!({"key": "value"}),
                tool_result.clone(),
            )
            .await;

        // Test cache hit
        let cached = cache
            .get("server1", "tool1", &serde_json::json!({"key": "value"}))
            .await;
        assert!(cached.is_some());
        let cached_result = cached.unwrap();
        assert_eq!(cached_result.content.len(), 1);
        match &cached_result.content[0] {
            McpContent::Text { text } => assert_eq!(text, "test result"),
            _ => panic!("Expected text content"),
        }

        // Test cache stats
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.total_hits, 1);
    }

    #[tokio::test]
    async fn test_tool_execution_engine() {
        let engine = ToolExecutionEngine::new(Duration::from_mins(1), 100, 5);

        // Test default configuration
        let metrics = engine.all_metrics().await;
        assert_eq!(metrics.len(), 0);

        // Test cache stats
        let cache_stats = engine.cache_stats().await;
        assert_eq!(cache_stats.total_entries, 0);
        assert_eq!(cache_stats.total_hits, 0);

        // Test cleanup
        let cleaned = engine.cleanup_cache().await;
        assert_eq!(cleaned, 0);
    }

    #[tokio::test]
    async fn test_tool_call_with_timeout() {
        let call = ToolCall::new("srv".to_string(), "tool".to_string(), serde_json::json!({}))
            .with_timeout(Duration::from_secs(10));
        assert_eq!(call.timeout, Some(Duration::from_secs(10)));
    }

    #[tokio::test]
    async fn test_tool_cache_ttl_expiration() {
        let cache = ToolCache::new(Duration::from_millis(10), 10);

        let tool_result = McpToolResult {
            content: vec![McpContent::Text {
                text: "cached".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };
        cache
            .put("srv", "tool", &serde_json::json!({}), tool_result)
            .await;

        // Immediate access should hit
        let cached = cache.get("srv", "tool", &serde_json::json!({})).await;
        assert!(cached.is_some());

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Should miss now
        let cached = cache.get("srv", "tool", &serde_json::json!({})).await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_tool_cache_eviction() {
        let cache = ToolCache::new(Duration::from_mins(1), 2);

        let result1 = McpToolResult {
            content: vec![McpContent::Text {
                text: "r1".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };
        let result2 = McpToolResult {
            content: vec![McpContent::Text {
                text: "r2".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };
        let result3 = McpToolResult {
            content: vec![McpContent::Text {
                text: "r3".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };

        cache
            .put("srv", "tool1", &serde_json::json!(1), result1)
            .await;
        cache
            .put("srv", "tool2", &serde_json::json!(2), result2)
            .await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 2);

        // Adding a third should evict the oldest
        cache
            .put("srv", "tool3", &serde_json::json!(3), result3)
            .await;
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 2);

        // tool1 should have been evicted (oldest timestamp)
        let miss = cache.get("srv", "tool1", &serde_json::json!(1)).await;
        assert!(miss.is_none());
    }

    #[tokio::test]
    async fn test_tool_cache_cleanup() {
        let cache = ToolCache::new(Duration::from_millis(5), 100);

        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "x".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };
        cache
            .put("srv", "tool", &serde_json::json!({}), result)
            .await;
        assert_eq!(cache.stats().await.total_entries, 1);

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(10)).await;

        let cleaned = cache.cleanup().await;
        assert_eq!(cleaned, 1);
        assert_eq!(cache.stats().await.total_entries, 0);
    }

    #[tokio::test]
    async fn test_tool_registry_default() {
        let registry = ToolRegistry::default();
        assert_eq!(registry.tool_count().await, 0);
        assert_eq!(registry.server_count().await, 0);
    }

    #[tokio::test]
    async fn test_tool_registry_list_server_tools_empty() {
        let registry = ToolRegistry::new();
        let tools = registry.list_server_tools("unknown").await;
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn test_tool_registry_get_tool_not_found() {
        let registry = ToolRegistry::new();
        assert!(registry.find_tool("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_tool_registry_remove_server_tools_empty() {
        let mut registry = ToolRegistry::new();
        registry.remove_server_tools("unknown").await;
        assert_eq!(registry.tool_count().await, 0);
    }

    #[tokio::test]
    async fn test_tool_registry_call_tool_not_found() {
        let registry = ToolRegistry::new();
        let result = registry
            .call_tool("nonexistent", serde_json::json!({}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_tool_execution_engine_default() {
        let engine = ToolExecutionEngine::default();
        assert_eq!(engine.cache_stats().await.total_entries, 0);
    }

    #[tokio::test]
    async fn test_tool_execution_engine_with_retry_config() {
        let config = RetryConfig {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(200),
            backoff_multiplier: 3.0,
            max_backoff: Duration::from_mins(1),
        };
        let engine =
            ToolExecutionEngine::new(Duration::from_mins(1), 100, 5).with_retry_config(config);
        // Engine created successfully with custom retry config
        assert_eq!(engine.all_metrics().await.len(), 0);
    }

    #[test]
    fn test_tool_execution_result_debug() {
        let result = ToolExecutionResult {
            result: McpToolResult {
                content: vec![McpContent::Text {
                    text: "output".to_string(),
                }],
                is_error: Some(false),
                structured_content: None,
                meta: None,
            },
            execution_time_ms: 42,
            server_id: "srv".to_string(),
            tool_name: "tool".to_string(),
            cached: true,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("cached: true"));
    }

    #[test]
    fn test_cache_stats_debug() {
        let stats = CacheStats {
            total_entries: 5,
            total_hits: 10,
        };
        let debug = format!("{:?}", stats);
        assert!(debug.contains("total_entries: 5"));
        assert!(debug.contains("total_hits: 10"));
    }

    #[tokio::test]
    async fn test_tool_cache_key_different_args() {
        let cache = ToolCache::new(Duration::from_mins(1), 10);
        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "r".to_string(),
            }],
            is_error: None,
            structured_content: None,
            meta: None,
        };

        cache
            .put("srv", "tool", &serde_json::json!({"a": 1}), result)
            .await;
        // Different args should miss
        let miss = cache.get("srv", "tool", &serde_json::json!({"a": 2})).await;
        assert!(miss.is_none());
        // Same args should hit
        let hit = cache.get("srv", "tool", &serde_json::json!({"a": 1})).await;
        assert!(hit.is_some());
    }
}
