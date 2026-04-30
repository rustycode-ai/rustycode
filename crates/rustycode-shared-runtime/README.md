# rustycode-shared-runtime

Shared multi-threaded Tokio runtime for RustyCode.

## Purpose

Provides a single, application-wide Tokio runtime to avoid creating many short-lived runtimes that cause allocator/TLS growth and performance degradation. Exposed as a lazily-initialized static with convenient async/sync blocking APIs.

## Key Types

- `SHARED_RUNTIME` — Lazily-initialized global Tokio runtime (LazyLock<Runtime>)
- Helper functions: `spawn_on_shared()`, `block_on_shared()`, `block_on_shared_send()`

## Public API

```rust
use rustycode_shared_runtime::{spawn_on_shared, block_on_shared, SHARED_RUNTIME};

// Spawn a future onto the shared runtime
let handle = spawn_on_shared(async { 42 });
let result = SHARED_RUNTIME.block_on(handle)?;

// Block on a future using the shared runtime
let result = block_on_shared(async { "hello" }).await;

// Block on Send future when already in a runtime
let result = block_on_shared_send(async { 99 });
```

## Features

- **Named worker threads** — Threads named `shared-runtime-worker-{id}` for debugging
- **CPU-aware scaling** — Worker count = CPU core count via `num_cpus`
- **Smart blocking** — Uses `block_in_place` on multi-threaded runtimes when available
- **Fallback handling** — Gracefully handles single-threaded or restricted runtimes
- **Thread safety** — Send futures across thread boundaries via `block_on_shared_send()`

## Dependencies

- `tokio` — Async runtime
- `futures` — Executor and utilities
- `num_cpus` — Core count detection

## Architecture Notes

- Global static, initialized once per process
- Eliminates per-task runtime allocation overhead
- Essential for CLI applications that spawn many short-lived async tasks
- Used throughout RustyCode for background work, tool execution, and orchestration

## Testing

- Tests for shared runtime initialization and metrics
- Concurrent spawning tests with verification
- Panic handling verification
- Nested async computation tests

## See Also

- `rustycode-execution` — Uses shared runtime for plan execution
- `rustycode-tools` — Uses shared runtime for tool execution
