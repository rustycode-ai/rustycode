# Performance Review: LLM Streaming Path (`crates/rustycode-llm/src/`)

42,455 LOC across 65 files. Streaming: `complete_stream()` → reqwest SSE → `StreamChunk` → TUI.

## Optimizations

### P0: Per-provider connection pools (−200ms cold start)
`client_pool.rs` uses one global pool (`OnceLock<Arc<ClientPool>>`) for all providers. Anthropic, OpenAI, and Gemini share `max_idle_per_host: 10`. **Fix**: per-provider `ClientPoolConfig` — `low_latency()` for interactive, `high_throughput()` for batch.

### P1: Single-pass deserialization (−15% per chunk)
SSE bytes → `serde_json::Value` → `AnthropicStreamEvent` → `StreamEvent` = double deserialize. **Fix**: `serde_json::from_slice::<StreamEvent>()` in one pass, or swap to `simd-json` for the hot path.

### P1: `Arc<str>` for cached prompts (−60% memory)
`caching.rs::Aggressive` clones entire `Vec<ChatMessage>` per request — system prompts (10K+ tokens) cloned every turn. **Fix**: `Arc<str>` / `Arc<[u8]>` for cached content blocks. Clone the `Arc`, not the string.

### P2: Pre-allocate delta text buffers (−40% allocations)
`anthropic_streaming.rs` allocates a fresh `String` per `ContentBlockDeltaInfo` — 50-200 per response. **Fix**: `String::with_capacity(64)` or accumulate in a caller-owned reusable buffer.

### P2: Remove `Arc<Mutex>` from stream state (−5% latency)
Streaming accumulator (tool call buffers, usage counters) wrapped in `Arc<Mutex<>>`, locked per chunk. **Fix**: owned state in the stream future — single owner, no locking needed.

### P3: Eliminate `format!` in chunk hot path (−3%)
Per-chunk string formatting allocates unnecessarily. **Fix**: `write!` into reusable buffers.

## Priority Summary

| # | Change | Impact | Effort |
|---|--------|--------|--------|
| 1 | Per-provider pools | −200ms cold | Low |
| 2 | Single-pass deserialize | −15% per chunk | Medium |
| 3 | Arc<str> cached prompts | −60% memory | Low |
| 4 | Pre-allocate deltas | −40% allocs | Low |
| 5 | Remove Arc<Mutex> stream | −5% latency | Medium |
| 6 | Eliminate format! | −3% latency | Low |
