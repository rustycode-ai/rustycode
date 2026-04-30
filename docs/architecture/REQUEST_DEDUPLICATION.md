# Request Deduplication Implementation

## Overview

Request deduplication prevents duplicate LLM calls when identical requests are accidentally re-sent or retried. This feature significantly reduces API costs and improves response latency by returning cached responses for identical requests within a configurable time window.

## Architecture

### Request Hash Computation

The deduplication system uses SHA-256 hashing to create unique fingerprints for each request. The hash is computed from:

1. **System Prompt** (if present)
2. **User Messages** (concatenated)
3. **Model Name**

This ensures that identical requests across the entire conversation produce the same hash, while any variation (message content, system prompt, or model choice) produces a different hash.

```rust
let hash = RequestDeduplicator::compute_hash(
    "Tell me about Rust",
    Some("You are an expert developer"),
    "claude-3-opus"
);
```

### Cache Storage

The cache uses:
- **HashMap** for O(1) lookup
- **Arc<RwLock<>>** for thread-safe concurrent access
- **Bounded size** with LRU-style eviction (removes oldest entry when full)
- **TTL (Time-To-Live)** with configurable deduplication window

### Cache Entries

Each cached entry stores:
- **Response text** - The LLM response
- **Tokens used** - Token count from the API response
- **Finish reason** - Completion reason (e.g., "stop")
- **Cached at** - Unix timestamp of when response was cached

## Configuration

### Default Configuration

```rust
DeduplicationConfig {
    enabled: true,
    dedup_window_secs: 300,      // 5 minutes
    max_cache_entries: 100,
}
```

### Custom Configuration

```rust
let config = DeduplicationConfig {
    enabled: true,
    dedup_window_secs: 600,      // 10 minutes
    max_cache_entries: 500,
};

let client = LlmClient::with_dedup_config(llm_config, config);
```

### Runtime Changes

```rust
let mut dedup = RequestDeduplicator::new(config);

// Change config at runtime
dedup.set_config(new_config);
```

## Integration with LLM Client

The `LlmClient` automatically uses request deduplication:

```rust
// Create with default dedup config
let client = LlmClient::new(config);

// Or with custom config
let client = LlmClient::with_dedup_config(config, dedup_config);

// Execute task - dedup happens automatically
let result = client.execute_task(&model, messages, system_prompt).await?;
```

### Execution Flow

1. **Request Arrives**: Hash is computed from messages, system prompt, and model
2. **Cache Check**: Deduplicator checks if valid (non-expired) cached response exists
3. **Cache Hit**: Return cached response immediately (0ms API call)
4. **Cache Miss**: Send request to LLM API
5. **Response Caching**: Cache response with current timestamp
6. **Return**: Return response to caller

## Cache Management

### Automatic Eviction

When cache reaches max capacity, the oldest entry (by timestamp) is evicted to make room for new entries.

```rust
let stats = dedup.cache_stats().await;
println!("Cached entries: {}/{}", stats.total_entries, stats.max_entries);
```

### Manual Cache Cleanup

Remove expired entries:

```rust
let removed = client.cleanup_expired_cache().await?;
println!("Removed {} expired entries", removed);
```

Clear entire cache:

```rust
client.clear_dedup_cache().await?;
```

### Expiration Window

Responses expire after `dedup_window_secs`. Requests beyond this window are treated as new requests and sent to the API.

```rust
// Config with 10-minute window
DeduplicationConfig {
    enabled: true,
    dedup_window_secs: 600,
    max_cache_entries: 100,
}
```

## Use Cases

### Duplicate Request Prevention

When a user accidentally clicks execute twice:

```
Request 1: "Plan the implementation"
  → Hash computed, not in cache
  → API call made, tokens: 500
  → Response cached

Request 2: "Plan the implementation" (user clicked execute again)
  → Hash computed (same as Request 1)
  → Cache HIT! Return cached response (0 tokens)
  → Response returned instantly
```

### Retry Scenarios

When network errors cause automatic retries:

```
Request 1: "Write the function"
  → API call initiated
  → Network timeout

Request 1 (Retry): "Write the function"
  → Hash matches Request 1
  → Previous attempt might be in cache
  → Return cached response if available
```

### Multi-Round Conversations

In conversations with multiple requests:

```
Request 1: "Plan feature X" → Cache hit 1, API call 1 (planning)
Request 2: "Implement feature X" → Cache miss (different content)
Request 3: "Plan feature X" → Cache hit (same as Request 1)
Request 4: "Implement feature X" → Cache hit (same as Request 2)
```

## Monitoring

### Cache Statistics

```rust
let stats = client.get_dedup_stats().await;
println!("Cache enabled: {}", stats.enabled);
println!("Cached entries: {}/{}", stats.total_entries, stats.max_entries);
println!("TTL window: {} seconds", stats.dedup_window_secs);
```

### Logging

The deduplicator logs all significant events:

- **Cache hits**: `"Cache hit: returning cached response"`
- **Cache misses**: `"Cache miss: no entry found"`
- **Evictions**: `"Evicted oldest cache entry due to capacity limit"`
- **Expirations**: `"Cache entry expired: treating as cache miss"`
- **Cleanup**: `"Cleaned up expired cache entries"`

## Testing

### Unit Tests (16 tests)

The implementation includes comprehensive unit tests:

1. **Hash Computation**: Same inputs produce same hash
2. **Hash Uniqueness**: Different inputs produce different hashes
3. **Expiration Logic**: Responses expire after TTL window
4. **Cache Operations**: Hit/miss/clear/cleanup operations
5. **Capacity Management**: Eviction when at max capacity
6. **Concurrent Access**: Thread-safe under concurrent load
7. **Configuration**: Respects enabled/disabled state and custom settings

### Integration Tests (5 tests)

Integration tests verify:

1. **Cache Hit Scenario**: Full flow from cache to response
2. **Different Messages**: Multiple different requests cached independently
3. **Config Changes**: Runtime configuration updates work correctly
4. **Concurrent Operations**: 20 concurrent cache operations complete successfully
5. **Realistic Workflow**: Multi-request conversation with planning and coding

## Performance Characteristics

### Latency

- **Cache Hit**: ~1ms (HashMap lookup + RwLock acquisition)
- **Cache Miss**: Same as API call + caching overhead (~1ms)

### Memory

- **Per Entry**: ~100-500 bytes (response text size dependent)
- **Max Memory**: `max_cache_entries * avg_response_size`
  - Default: 100 entries × 2KB average = ~200KB max

### CPU

- **Hash Computation**: O(n) where n = message length (SHA-256)
- **Cache Lookup**: O(1) average case (HashMap)
- **Eviction**: O(n) where n = cache size (linear scan for oldest)

## Future Enhancements

Potential improvements for future phases:

1. **Persistent Cache**: Store cache to disk for cross-session deduplication
2. **Partial Request Matching**: Detect similar (not just identical) requests
3. **Cache Statistics**: Track hit/miss rates, token savings, cost reductions
4. **Smart Eviction**: Use LRU or LFU instead of simple FIFO
5. **Compression**: Compress cached responses to reduce memory usage
6. **Distributed Cache**: Share cache across multiple LlmClient instances

## Troubleshooting

### Cache Not Working

1. Check if deduplication is enabled:
   ```rust
   let stats = client.get_dedup_stats().await;
   assert!(stats.enabled, "Dedup should be enabled");
   ```

2. Verify request is identical (same messages, system prompt, model)

3. Check expiration window - entry may have expired

### High Memory Usage

1. Reduce `max_cache_entries` in config
2. Reduce `dedup_window_secs` to expire entries sooner
3. Call `cleanup_expired_cache()` periodically

### Cache Thrashing

If cache is constantly evicting entries:
- Increase `max_cache_entries`
- Analyze if requests are truly identical or just similar

## References

- **Module**: `/crates/rustycode-orchestration/src/request_dedup.rs`
- **Tests**: `/crates/rustycode-orchestration/tests/test_request_dedup.rs`
- **Integration**: `/crates/rustycode-orchestration/src/llm.rs`
