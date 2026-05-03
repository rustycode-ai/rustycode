# rustycode-load

Load testing and stress testing utilities for RustyCode.

## Purpose

Provides infrastructure for load testing RustyCode components. Enables measuring performance under stress, finding bottlenecks, and verifying scalability.

## Key Types

- `LoadTester` — Main load testing coordinator
- `LoadConfig` — Configuration (concurrency, duration, request rate)
- `LoadResult` — Results with metrics (latency, throughput, errors)
- `RequestGenerator` — Generates load requests
- `MetricsCollector` — Collects performance metrics

## Public API

```rust
use rustycode_load::{LoadTester, LoadConfig, RequestPattern};

// Configure load test
let config = LoadConfig::default()
    .with_concurrent_requests(100)
    .with_duration(Duration::from_secs(60))
    .with_ramp_up(Duration::from_secs(10));

// Create load tester
let mut tester = LoadTester::new(config)?;

// Add request patterns
tester.add_pattern(RequestPattern::LlmCall {
    model: "claude-opus-4-7".to_string(),
    tokens: 1000,
})?;

tester.add_pattern(RequestPattern::ToolExecution {
    tool: "bash".to_string(),
    complexity: "medium".to_string(),
})?;

// Run load test
let results = tester.run().await?;

// Analyze results
println!("Throughput: {:.2} req/s", results.throughput);
println!("P99 Latency: {:.2}ms", results.latency_p99);
println!("Error Rate: {:.2}%", results.error_rate);
```

## Request Patterns

- **LLMCall** — Simulate LLM API calls
- **ToolExecution** — Simulate tool execution
- **SessionFlow** — Full session with messages and tools
- **Mixed** — Random mix of patterns

## Metrics Collected

- Throughput (requests/second)
- Latency (p50, p95, p99, max)
- Error rate and breakdown
- Resource usage (memory, CPU)
- Queue depth (if applicable)

## Test Scenarios

- **Steady Load** — Constant request rate
- **Ramp Up** — Gradually increasing load
- **Spike** — Sudden load spike
- **Sustained** — Run until error threshold

## Features

- **Concurrent Requests** — Multiple parallel requests
- **Real LLM Calls** (optional) — Actually call LLM for realistic load
- **Mock Mode** — Use fake responses for faster tests
- **Metrics Collection** — Detailed performance metrics
- **Visualization** — Plot latency and throughput over time
- **Reporting** — Generate HTML/markdown reports

## Dependencies

- `tokio` — Concurrent request execution
- `hyper` (optional) — HTTP client for API calls
- `rustycode-core` — Session simulation
- `rustycode-llm` — LLM provider
- `rustycode-tools` — Tool execution
- Plotting libraries (optional) — Visualization

## Architecture Notes

Load testing runs requests concurrently. Metrics collected per-request and aggregated. Ramp-up phase prevents thundering herd.

Real or mock mode can be selected. Mock mode useful for quick tests; real mode for accurate performance under actual conditions.

## Testing

Tests verify load generation, metric collection, and result accuracy.

## See Also

- `rustycode-observability` — Metrics and profiling
- `rustycode-bench` — Benchmark runner (different purpose)
- `rustycode-core` — Session management
