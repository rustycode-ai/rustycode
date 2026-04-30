# rustycode-observability

Metrics, tracing, and logging infrastructure for RustyCode.

## Purpose

Provides centralized observability for the RustyCode system. Tracks execution metrics (latency, token counts, costs), structured logging with context, and distributed tracing support. Enables performance profiling and cost analysis across operations.

## Key Types

- `SessionMetrics` — Aggregate metrics for a session (tokens, costs, duration)
- `Counter` — Monotonically increasing counter
- `Gauge` — Point-in-time measurement (temperature, queue size)
- `Histogram` — Distribution of values (latencies, request sizes)
- `HistogramStats` — Summary statistics (min, max, mean, p50, p95, p99)
- `MetricsStore` — Central storage for all metrics
- `ExecutionContext` — Distributed trace context (trace ID, span ID)
- `LogContext` — Contextual information for log messages

## Public API

```rust
use rustycode_observability::{SessionMetrics, MetricsStore, LogContext, set_log_context};

// Initialize logging
rustycode_observability::init_logging(LogLevel::Debug)?;

// Create metrics store
let store = MetricsStore::new();

// Record metrics
store.record_counter("request.count", 1.0);
store.record_gauge("queue.size", 42.0);
store.record_histogram("request.latency_ms", 125.5);

// Set log context for structured logging
let context = LogContext::new("session-123".to_string());
set_log_context(context);

// Get session metrics
if let Some(metrics) = store.session_metrics("session-123") {
    println!("Total tokens: {}", metrics.total_tokens);
    println!("Total cost: ${:.4}", metrics.total_cost);
}
```

## Metrics Categories

- **Request metrics**: Count, latency, error rate
- **Token metrics**: Input tokens, output tokens, total tokens
- **Cost metrics**: Cost per request, cumulative cost, cost per model
- **LLM metrics**: Model selection, provider usage, cache hit rate
- **Tool metrics**: Tool invocations, execution time, success rate
- **Resource metrics**: Memory usage, thread count, goroutine count

## Logging Levels

- `Trace` — Very detailed diagnostic information
- `Debug` — Debugging information
- `Info` — Informational messages
- `Warn` — Warning messages (unexpected but recoverable)
- `Error` — Error messages (operation failed)

## Dependencies

- `tracing` — Distributed tracing framework
- `tracing-subscriber` — Tracing composable layers
- `serde` — Serialization
- `tokio` — Async runtime (for context management)
- `parking_lot` — Efficient locking

## Architecture Notes

Metrics are stored in a thread-safe central store. Log context is thread-local or task-local to maintain execution context across async boundaries. Histogram stats are computed on-demand from samples (memory-efficient).

Cost tracking integrates with provider metadata (rustycode-providers) to calculate real-time costs. Token counts come from LLM provider responses and are aggregated at session level.

## Testing

Tests verify metric recording, statistics computation, log context isolation, and serialization.

## See Also

- `rustycode-providers` — Provider metadata (used for cost calculations)
- `rustycode-core` — Session lifecycle (observability data is session-scoped)
- `rustycode-llm` — LLM metrics and token counts
- `rustycode-tools` — Tool invocation metrics
