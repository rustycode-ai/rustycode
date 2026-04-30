# rustycode-execution

Plan execution and orchestration engine.

## Purpose

Provides the core execution framework for RustyCode's autonomous mode. Transforms abstract plans into concrete step execution with monitoring, error handling, and progress tracking. Enables both sequential and parallel task execution with support for cancellation and recovery.

## Key Types

- `Executor` — Main execution engine, orchestrates plan and step execution
- `ExecutionConfig` — Configuration for execution behavior (timeouts, retries, etc.)
- `ExecutionResult` — Result of execution with status and metrics
- `PlanExecutor` — Specializes in executing complete plans
- `StepExecutor` — Handles individual step execution and transitions
- `ExecutionContext` — Context passed during execution (session, environment, etc.)
- `ExecutionMonitor` — Tracks execution progress and metrics

## Public API

```rust
use rustycode_execution::{Executor, ExecutionConfig, ExecutionResult};

let config = ExecutionConfig::default();
let executor = Executor::new(config);
let result = executor.execute_plan(plan).await?;
```

## Dependencies

- `anyhow` — Error handling with context
- `tokio` — Async runtime
- `rustycode-protocol` — Shared type definitions
- `rustycode-session` — Session management
- `rustycode-observability` — Metrics and tracing

## Architecture Notes

- Sits at the boundary between orchestration and execution layers
- Transforms high-level plans into concrete operations
- Provides hooks for monitoring, logging, and recovery
- Designed for extensibility (custom step types, error handlers)

## Testing

- Unit tests for each executor component
- Integration tests for multi-step plans
- Mock step implementations for testing plan logic without real tools

## See Also

- `rustycode-orchestration` — Algorithmic execution strategies and reasoning
- `rustycode-session` — Session lifecycle management
