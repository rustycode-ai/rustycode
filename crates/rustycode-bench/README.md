# rustycode-bench

Benchmark runner for agent evaluation (Harbor-compatible pipeline).

## Purpose

Provides a comprehensive benchmarking framework for evaluating autonomous agents on software engineering tasks. Implements the Harbor benchmark protocol: environment → agent → verifier → result. Supports containerized task execution, multiple agent types (oracle, code agent, noop), and automated result verification.

## Key Types

- `BenchEnvironment` — Container lifecycle management (start/stop/exec)
- `BenchAgent` — Agent abstraction (trait for different agent implementations)
- `OracleAgent` — Reference agent implementation
- `CodeAgent` — Autonomous code generation agent
- `Trial` — Single benchmark task execution
- `Job` — Manages concurrent trials
- `TaskConfig` — Harbor task.toml configuration parser
- `DatasetRegistry` — Discovers and loads task datasets
- `Verifier` — Parses and validates task outputs
- `BenchMcpBridge` — MCP-compatible bridge for tool operations

## Public API

```rust
use rustycode_bench::{Job, JobConfig, BenchEnvironment};

let config = JobConfig::new();
let job = Job::new(config);
let results = job.run_trials().await?;
```

## Dependencies

- `tokio` — Async runtime
- `docker-api` — Container orchestration (implicit)
- `rustycode-protocol` — Shared types
- `serde` — Configuration serialization
- `anyhow` — Error handling

## Architecture Notes

- Harbor-compatible for benchmark standardization
- Supports both containerized and local execution
- Extensible agent framework for new evaluation methods
- Automatic result collection and aggregation

## Testing

- Unit tests for config parsing and verification logic
- Integration tests with mock containers
- Benchmark-specific test harness for validating result collection

## See Also

- `rustycode-execution` — Plan execution engine
- `rustycode-tools` — Tool execution for agents
- `rustycode-load` — Load testing utilities
