# rustycode-runtime

Async runtime utilities and orchestration helpers for RustyCode.

## Purpose

Provides async runtime configuration, task coordination, and concurrency utilities for RustyCode's async execution model. Handles runtime setup, task spawning, and graceful shutdown.

## Key Types

- `RuntimeConfig` — Configuration for tokio runtime
- `TaskPool` — Task execution pool with limits
- `TaskHandle` — Handle to spawned task
- `CancellationToken` — Cooperative cancellation
- `Barrier` — Synchronization primitive
- `Channel` — Type-safe message passing

## Public API

```rust
use rustycode_runtime::{RuntimeConfig, TaskPool};

// Configure and build runtime
let runtime = RuntimeConfig::default()
    .with_worker_threads(num_cpus::get())
    .with_max_io_threads(128)
    .build()?;

// Spawn tasks
runtime.spawn(async {
    println!("Task running");
}).await?;

// Create bounded task pool
let pool = TaskPool::new(10);

// Spawn multiple tasks with limits
for i in 0..100 {
    pool.spawn(async move {
        println!("Task {}", i);
    }).await?;
}

// Graceful shutdown
runtime.shutdown(Duration::from_secs(30)).await?;
```

## Features

- **Multi-threaded Runtime** — CPU-aware thread count
- **I/O Optimized** — Separate I/O thread pool
- **Bounded Execution** — Prevent unbounded task growth
- **Cancellation** — Cooperative cancellation tokens
- **Metrics** — Track active tasks, queue length
- **Graceful Shutdown** — Wait for all tasks with timeout

## Task Pool

Limits concurrent task execution to prevent resource exhaustion:
- Queue incoming tasks
- Execute up to limit concurrently
- FIFO scheduling
- Fair distribution across workers

## Cancellation

```rust
use rustycode_runtime::CancellationToken;

let token = CancellationToken::new();
let token_clone = token.clone();

tokio::spawn(async move {
    tokio::select! {
        _ = token_clone.cancelled() => {
            println!("Task was cancelled");
        }
        _ = long_operation() => {
            println!("Task completed");
        }
    }
});

// Cancel all listeners
token.cancel();
```

## Dependencies

- `tokio` — Async runtime
- `num_cpus` — CPU detection
- `parking_lot` — Efficient locking
- `anyhow` — Error handling

## Architecture Notes

Runtime is configured once at startup. Task spawning is cheap (green threads). Bounded pools prevent unbounded growth.

Cancellation is cooperative — tasks must explicitly check for cancellation. Shutdown waits for all tasks with timeout before force-killing.

Metrics are optional but recommended for observability.

## Testing

Tests verify runtime creation, task spawning, task limits, cancellation, and graceful shutdown.

## See Also

- `rustycode-shared-runtime` — Global shared runtime
- `rustycode-core` — Session execution
- `rustycode-execution` — Plan execution
