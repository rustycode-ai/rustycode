# Rust-Native Async Patterns for RustyCode

**Status**: Design Document
**Author**: RustyCode Ensemble
**Created**: 2025-03-12
**Related**: [Event Bus Architecture](./event-bus.md), [ADR-0003](../adr/0003-event-bus-system.md)

## Executive Summary

This document provides comprehensive guidance on using Rust-native async patterns in RustyCode's architecture. It covers Tokio primitives, channel selection, task spawning strategies, backpressure handling, and graceful shutdown patterns tailored to RustyCode's multi-crate architecture.

## Table of Contents

1. [Tokio Foundations](#tokio-foundations)
2. [Channel Selection Guidelines](#channel-selection-guidelines)
3. [Task Spawning Strategies](#task-spawning-strategies)
4. [Backpressure Handling](#backpressure-handling)
5. [Resource Management](#resource-management)
6. [Graceful Shutdown](#graceful-shutdown)
7. [Crate-Specific Patterns](#crate-specific-patterns)
8. [Tower Integration](#tower-integration)
9. [Testing Async Code](#testing-async-code)
10. [Performance Considerations](#performance-considerations)

---

## Tokio Foundations

### When to Use Async vs Sync

**Use async (`tokio`) when:**
- I/O-bound operations (network, file I/O with async-optimized libraries)
- Concurrent operations that spend time waiting
- Building networking services or clients
- Need to handle many concurrent operations efficiently

**Use sync (blocking) when:**
- CPU-bound computations (use `rayon` instead)
- Simple scripts with no concurrency needs
- Operations that complete quickly
- Working with libraries that only provide blocking APIs

**Example: Current RustyCode State**
```rust
// Current: Blocking operations (appropriate for simple CLI)
pub fn inspect(cwd: &Path) -> Result<GitStatus> {
    let output = Command::new("git")
        .args(&["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()?;
    // ...
}

// Future: Async version for event-driven monitoring
pub async fn inspect_async(cwd: &Path) -> Result<GitStatus> {
    let output = tokio::process::Command::new("git")
        .args(&["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .await?;
    // ...
}
```

### Tokio Runtime Selection

Tokio provides two main runtime configurations:

```rust
// Multi-threaded runtime (default for servers)
#[tokio::main]
async fn main() {
    // Uses work-stealing scheduler
    // Optimal for CPU-intensive async work
}

// Current-thread runtime (for testing or simple CLIs)
#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Single-threaded, no work-stealing
    // Lower overhead, simpler debugging
}
```

**Recommendation for RustyCode:**
- **CLI crate**: Use `current_thread` runtime (single-threaded is sufficient)
- **Future server/TUI**: Use multi-threaded runtime

---

## Channel Selection Guidelines

Tokio provides four channel types, each optimized for specific use cases:

### Decision Tree

```
Need to send values between tasks?
├─ Single value, one time?
│  └─ oneshot
├─ Many values, one consumer?
│  └─ mpsc
├─ Many values, all consumers need every value?
│  └─ broadcast
└─ Many values, consumers only need latest value?
   └─ watch
```

### 1. `oneshot` Channel

**Use case:** Send a single value from producer to consumer

```rust
use tokio::sync::oneshot;

// Pattern: Request/Response
pub async fn fetch_with_timeout(url: &str) -> Result<String> {
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let response = reqwest::get(url).await?.text().await?;
        tx.send(response).unwrap();
        Ok::<(), reqwest::Error>(())
    });

    tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(|_| anyhow::anyhow!("timeout"))?
        .map_err(|_| anyhow::anyhow!("sender dropped"))
}
```

**When to use in RustyCode:**
- LSP server initialization responses
- One-shot Git command results
- Single-value returns from spawned tasks

### 2. `mpsc` Channel (Multi-Producer, Single-Consumer)

**Use case:** Send many values from many tasks to one consumer

```rust
use tokio::sync::mpsc;

// Pattern: Work queue
pub struct WorkQueue<T> {
    tx: mpsc::Sender<T>,
}

impl<T: Send + 'static> WorkQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, mut rx) = mpsc::channel(capacity);

        tokio::spawn(async move {
            while let Some(work) = rx.recv().await {
                // Process work item
                if let Err(e) = process_item(work).await {
                    tracing::error!("Work processing failed: {}", e);
                }
            }
        });

        Self { tx }
    }

    pub async fn submit(&self, work: T) -> Result<()> {
        self.tx.send(work).await
            .map_err(|_| anyhow::anyhow!("channel closed"))
    }
}
```

**Channel capacity guidelines:**
- `capacity = 0`: Unbounded (dangerous, can cause OOM)
- `capacity = 1-10`: Low latency, strong backpressure
- `capacity = 100-1000`: Balanced (default choice)
- `capacity > 1000`: High throughput, weak backpressure

**When to use in RustyCode:**
- Event bus subscriptions (as in event bus design)
- LSP message processing
- Git operation queue
- File search result streaming

### 3. `broadcast` Channel

**Use case:** Send many values to many consumers, each receives all values

```rust
use tokio::sync::broadcast;

// Pattern: Event bus fan-out
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: Event) {
        // Send errors are OK - means no receivers
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
```

**Key characteristics:**
- Each receiver gets **every** message
- Lagging receivers miss old messages
- Channel retains only `capacity` most recent messages
- Send never blocks (returns error if no receivers)

**When to use in RustyCode:**
- **Event bus implementation** (primary use case)
- Configuration updates to multiple subsystems
- Log/telemetry fan-out
- Multi-subscriber notifications

**Example for RustyCode event bus:**
```rust
// In rustycode-bus crate
pub struct EventBus {
    broadcast: broadcast::Sender<Box<dyn Event>>,
}

impl EventBus {
    pub async fn publish<T>(&self, event: T) -> Result<()>
    where
        T: Event + Clone,
    {
        let boxed = Box::new(event);
        // Ignore lagging receivers
        let _ = self.broadcast.send(boxed);
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Box<dyn Event>> {
        self.broadcast.subscribe()
    }
}
```

### 4. `watch` Channel

**Use case:** Broadcast state changes, consumers only need latest value

```rust
use tokio::sync::watch;

// Pattern: Configuration/State broadcasting
pub struct ConfigManager {
    tx: watch::Sender<Config>,
}

impl ConfigManager {
    pub fn new(initial: Config) -> Self {
        let (tx, _) = watch::channel(initial);
        Self { tx }
    }

    pub fn update(&self, new_config: Config) {
        // Send fails if no receivers, which is OK
        let _ = self.tx.send(new_config);
    }

    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.tx.subscribe()
    }
}
```

**Key characteristics:**
- Only the **latest** value is retained
- New subscribers immediately receive current value
- Recipients can miss intermediate values
- Useful for state broadcasting

**When to use in RustyCode:**
- Configuration reload notifications
- Git repository state changes (branch, dirty status)
- Session state updates
- Shutdown signals

### Channel Comparison Table

| Channel | Producers | Consumers | Guarantees | Backpressure | Use Case |
|---------|-----------|-----------|------------|--------------|----------|
| `oneshot` | 1 | 1 | Single value | Sender waits | One-shot responses |
| `mpsc` | ∞ | 1 | All values | Sender waits | Work queues |
| `broadcast` | ∞ | ∞ | All values* | None | Event bus |
| `watch` | ∞ | ∞ | Latest only | None | State updates |

*Slow receivers may miss values

---

## Task Spawning Strategies

### 1. Fire-and-Forget Tasks

```rust
// Use when: Task is independent, no result needed
tokio::spawn(async move {
    if let Err(e) = background_cleanup().await {
        tracing::error!("Cleanup failed: {}", e);
    }
});
```

**When to use in RustyCode:**
- Telemetry reporting
- Log aggregation
- Cache warming
- Metrics collection

### 2. Structured Concurrency with `JoinSet`

**Tokio 1.20+**: Use `JoinSet` for managing task groups

```rust
use tokio::task::JoinSet;

pub async fn parallel_search(paths: Vec<PathBuf>) -> Vec<SearchResult> {
    let mut set = JoinSet::new();

    for path in paths {
        set.spawn(async move {
            search_in_path(path).await
        });
    }

    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        if let Ok(search_result) = result {
            results.push(search_result);
        }
    }

    results
}
```

**When to use in RustyCode:**
- Parallel file searches
- Multiple LSP server queries
- Concurrent skill discovery
- Batch operations

### 3. Task Per Request Pattern

```rust
// Pattern: Dedicated task per resource
pub struct GitStatusMonitor {
    cmd_tx: mpsc::Sender<GitCommand>,
}

enum GitCommand {
    GetStatus(oneshot::Sender<GitStatus>),
    Watch(PathBuf, oneshot::Sender<broadcast::Receiver<GitStatus>>),
}

impl GitStatusMonitor {
    pub fn new() -> Self {
        let (cmd_tx, mut cmd_rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    GitCommand::GetStatus(resp) => {
                        let status = git_inspect().await.unwrap();
                        let _ = resp.send(status);
                    }
                    GitCommand::Watch(path, resp) => {
                        let (tx, _) = broadcast::channel(16);
                        let _ = resp.send(tx.subscribe());
                        // Spawn watcher task
                    }
                }
            }
        });

        Self { cmd_tx }
    }
}
```

**When to use in RustyCode:**
- Long-lived subsystems (Git, LSP, storage)
- Resource management (connection pools, file handles)
- Stateful services

### 4. Task Prioritization

```rust
// Pattern: Priority-based task spawning
pub struct TaskScheduler {
    high_priority: mpsc::Sender<Task>,
    normal_priority: mpsc::Sender<Task>,
    low_priority: mpsc::Sender<Task>,
}

impl TaskScheduler {
    pub async fn schedule(&self, task: Task, priority: Priority) {
        let channel = match priority {
            Priority::High => &self.high_priority,
            Priority::Normal => &self.normal_priority,
            Priority::Low => &self.low_priority,
        };
        channel.send(task).await.ok();
    }
}
```

---

## Backpressure Handling

Backpressure is crucial for preventing resource exhaustion when producers are faster than consumers.

### 1. Bounded Channels

```rust
// Good: Bounded channel applies backpressure
let (tx, mut rx) = mpsc::channel(100); // Max 100 messages

// When channel is full, send() waits:
tx.send(msg).await.unwrap(); // Blocks if full
```

### 2. Semaphore for Concurrency Limiting

```rust
use tokio::sync::Semaphore;

pub struct ConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
}

impl ConcurrencyLimiter {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    pub async fn acquire<F, T>(&self, f: F) -> T
    where
        F: Future<Output = T>,
    {
        let _permit = self.semaphore.acquire().await.unwrap();
        f.await
    }
}

// Usage
let limiter = ConcurrencyLimiter::new(10);

// Spawn 100 tasks, but only 10 run concurrently
for i in 0..100 {
    tokio::spawn(async move {
        limiter.acquire(async move {
            expensive_operation(i).await
        }).await
    });
}
```

**When to use in RustyCode:**
- Limit concurrent file operations
- Throttle LSP server requests
- Control Git command concurrency
- Manage database connection pools

### 3. Timeout and Cancellation

```rust
use tokio::time::{timeout, Duration};

pub async fn with_timeout<F, T>(
    duration: Duration,
    future: F,
) -> Result<T> {
    timeout(duration, future)
        .await
        .map_err(|_| anyhow::anyhow!("operation timed out"))
}

// Usage
let result = with_timeout(
    Duration::from_secs(5),
    git_inspect(path)
).await?;
```

### 4. Graceful Degradation

```rust
// Pattern: Shed load when overwhelmed
pub struct LoadSheddingService {
    inner: Service,
    max_pending: usize,
    current: Arc<AtomicUsize>,
}

impl LoadSheddingService {
    pub async fn call(&self, req: Request) -> Result<Response> {
        let current = self.current.fetch_add(1, Ordering::Relaxed);

        if current > self.max_pending {
            self.current.fetch_sub(1, Ordering::Relaxed);
            return Err(anyhow::anyhow!("service overloaded"));
        }

        let result = self.inner.call(req).await;
        self.current.fetch_sub(1, Ordering::Relaxed);
        result
    }
}
```

---

## Resource Management

### 1. Connection Pooling

```rust
use tokio::sync::Semaphore;

pub struct ConnectionPool<T> {
    connections: Arc<Mutex<Vec<T>>>,
    semaphore: Arc<Semaphore>,
}

impl<T> ConnectionPool<T>
where
    T: Clone,
{
    pub fn new(connections: Vec<T>) -> Self {
        let count = connections.len();
        Self {
            connections: Arc::new(Mutex::new(connections)),
            semaphore: Arc::new(Semaphore::new(count)),
        }
    }

    pub async fn acquire<F, R>(&self, f: F) -> R
    where
        F: FnOnce(T) -> R,
    {
        let _permit = self.semaphore.acquire().await.unwrap();
        let mut connections = self.connections.lock().await;
        let conn = connections.pop().unwrap();
        drop(connections);

        let result = f(conn.clone());

        let mut connections = self.connections.lock().await;
        connections.push(conn);
        result
    }
}
```

### 2. Async Read/Write

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn async_file_copy(
    src: &Path,
    dst: &Path,
) -> std::io::Result<()> {
    let mut src_file = tokio::fs::File::open(src).await?;
    let mut dst_file = tokio::fs::File::create(dst).await?;

    let mut buffer = vec![0u8; 8192]; // 8KB buffer

    loop {
        let n = src_file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        dst_file.write_all(&buffer[..n]).await?;
    }

    dst_file.flush().await?;
    Ok(())
}
```

**Note:** Tokio's file I/O uses thread pool under the hood (OS limitation).

---

## Graceful Shutdown

### 1. CancellationToken Pattern

```rust
use tokio_util::sync::CancellationToken;

pub struct Runtime {
    shutdown_token: CancellationToken,
    _tasks: Vec<JoinHandle<()>>,
}

impl Runtime {
    pub fn new() -> Self {
        let shutdown_token = CancellationToken::new();

        // Spawn tasks that respect cancellation
        let task1 = tokio::spawn({
            let token = shutdown_token.clone();
            async move {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Task 1 shutting down");
                    }
                    result = long_running_task() => {
                        tracing::info!("Task 1 completed");
                    }
                }
            }
        });

        Self {
            shutdown_token,
            _tasks: vec![task1],
        }
    }

    pub async fn shutdown(self) {
        self.shutdown_token.cancel();
        // Wait for tasks to finish
        for task in self._tasks {
            let _ = task.await;
        }
    }
}
```

### 2. Watch Channel for Shutdown

```rust
use tokio::sync::watch;

pub struct Shutdown {
    tx: watch::Sender<bool>,
    rx: watch::Receiver<bool>,
}

impl Shutdown {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self { tx, rx }
    }

    pub async fn wait(&mut self) {
        while !*self.rx.borrow() {
            if self.rx.changed().await.is_err() {
                break;
            }
        }
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(true);
    }
}

// Usage in tasks
pub async fn worker_task(mut shutdown: Shutdown) {
    loop {
        tokio::select! {
            _ = shutdown.wait() => {
                tracing::info!("Shutting down worker");
                break;
            }
            result = work() => {
                // Process result
            }
        }
    }
}
```

### 3. Multi-Phase Shutdown

```rust
pub enum ShutdownPhase {
    Graceful(Duration), // Wait this long
    Forced,            // Immediate shutdown
}

pub struct GracefulShutdown {
    tasks: Vec<JoinHandle<()>>,
}

impl GracefulShutdown {
    pub async fn shutdown(mut self, phase: ShutdownPhase) {
        match phase {
            ShutdownPhase::Graceful(timeout) => {
                // Phase 1: Signal tasks to stop
                // (Assume tasks listen to shutdown signal)

                // Phase 2: Wait for graceful completion or timeout
                let deadline = tokio::time::Instant::now() + timeout;

                for task in &mut self.tasks {
                    let _ = tokio::time::timeout_until(deadline, task).await;
                }
            }
            ShutdownPhase::Forced => {
                // Abort all tasks immediately
                for task in self.tasks {
                    task.abort();
                }
            }
        }
    }
}
```

---

## Crate-Specific Patterns

### rustycode-core

**Pattern:** Central orchestration with task supervision

```rust
pub struct Runtime {
    event_bus: Arc<EventBus>,
    git_monitor: Arc<GitStatusMonitor>,
    lsp_manager: Arc<LspManager>,
    _shutdown: CancellationToken,
}

impl Runtime {
    pub async fn new() -> Result<Self> {
        let event_bus = Arc::new(EventBus::new());
        let shutdown_token = CancellationToken::new();

        // Spawn subsystem monitors
        let git_monitor = Arc::new(GitStatusMonitor::new(
            event_bus.clone(),
        ));

        let lsp_manager = Arc::new(LspManager::new(
            event_bus.clone(),
        ));

        Ok(Self {
            event_bus,
            git_monitor,
            lsp_manager,
            _shutdown: shutdown_token,
        })
    }

    pub async fn run_task(&self, task: &str) -> Result<RunReport> {
        // Publish session start event
        self.event_bus.publish(SessionStartedEvent {
            // ...
        }).await?;

        // Task execution happens here
        // Events flow through bus

        Ok(RunReport {
            // ...
        })
    }
}
```

### rustycode-git

**Pattern:** Async Git operations with change notification

```rust
pub struct GitStatusMonitor {
    bus: Arc<EventBus>,
    watchers: Arc<RwLock<HashMap<PathBuf, CancellationToken>>>,
}

impl GitStatusMonitor {
    pub async fn watch(&self, path: PathBuf) -> Result<()> {
        let token = CancellationToken::new();
        let bus = self.bus.clone();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let mut last_status = inspect(&path).await.unwrap();

            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {
                        if let Ok(current) = inspect(&path).await {
                            if current != last_status {
                                bus.publish(GitStatusChangedEvent {
                                    // ...
                                }).await.ok();
                                last_status = current;
                            }
                        }
                    }
                }
            }
        });

        self.watchers.write().await.insert(path, token);
        Ok(())
    }
}
```

### rustycode-lsp

**Pattern:** Connection pooling with async I/O

```rust
pub struct LspClientPool {
    clients: Arc<RwLock<HashMap<String, LspClient>>>,
    semaphore: Arc<Semaphore>,
}

impl LspClientPool {
    pub async fn call<F, R>(
        &self,
        server_name: &str,
        f: F,
    ) -> Result<R>
    where
        F: FnOnce(&LspClient) -> Pin<Box<dyn Future<Output = Result<R>> + Send>>,
    {
        let _permit = self.semaphore.acquire().await?;

        let clients = self.clients.read().await;
        let client = clients.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("LSP client not found"))?;

        f(client).await
    }
}
```

### rustycode-bus

**Pattern:** Event-driven pub/sub with backpressure

```rust
pub struct EventBus {
    broadcast: broadcast::Sender<Box<dyn Event>>,
    metrics: Arc<AtomicU64>,
}

impl EventBus {
    pub async fn publish<T>(&self, event: T) -> Result<()>
    where
        T: Event + Clone,
    {
        let boxed = Box::new(event);

        // Publish returns error if no receivers (OK)
        let _ = self.broadcast.send(boxed);

        self.metrics.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Box<dyn Event>> {
        self.broadcast.subscribe()
    }
}
```

### rustycode-storage

**Pattern:** Async SQLite with connection pooling

```rust
pub struct Storage {
    pool: Arc<Pool<SqliteConnection>>,
}

impl Storage {
    pub async fn insert_event_async(&self, event: &SessionEvent) -> Result<()> {
        let mut conn = self.pool.acquire().await?;

        sqlx::query(
            "INSERT INTO session_events (session_id, at, kind, detail) VALUES (?, ?, ?, ?)"
        )
        .bind(&event.session_id.0)
        .bind(event.at)
        .bind(&event.kind)
        .bind(&event.detail)
        .execute(&mut conn)
        .await?;

        Ok(())
    }
}
```

---

## Tower Integration

Tower provides a `Service` trait for building composable middleware.

### The Service Trait

```rust
use tower::Service;

pub trait Service<Request> {
    type Response;
    type Error;
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;

    fn call(&mut self, req: Request) -> Self::Future;
}
```

### Example: Timeout Middleware for LSP Calls

```rust
use tower::ServiceBuilder;
use tower::timeout::TimeoutLayer;

pub struct LspService {
    // ...
}

impl Service<LspRequest> for LspService {
    type Response = LspResponse;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: LspRequest) -> Self::Future {
        Box::pin(async move {
            // Handle LSP request
            Ok(LspResponse::default())
        })
    }
}

// Wrap with timeout middleware
let service = ServiceBuilder::new()
    .layer(TimeoutLayer::new(Duration::from_secs(5)))
    .service(LspService::new());
```

### When to Use Tower

**Good use cases for RustyCode:**
- LSP client middleware (retry, timeout, rate limiting)
- HTTP client middleware (if making API calls)
- Request/response transformation
- Composable behavior layers

**Not needed for:**
- Simple event publishing
- Direct function calls
- Non-service-oriented code

---

## Testing Async Code

### 1. Use `tokio::test`

```rust
#[tokio::test]
async fn test_git_inspect() {
    let temp_dir = tempfile::tempdir().unwrap();
    let status = inspect(temp_dir.path()).await.unwrap();

    assert!(status.root.is_some());
}
```

### 2. Test Timeouts

```rust
#[tokio::test]
async fn test_timeout() {
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        slow_operation()
    ).await;

    assert!(result.is_err());
}
```

### 3. Test Channels

```rust
#[tokio::test]
async fn test_event_bus() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    bus.publish(TestEvent).await.unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.event_type(), "test");
}
```

### 4. Mock Services with Tower

```rust
use tower::service_fn;

let mock_service = service_fn(|request: Request| async {
    Ok(Response::new("mock response"))
});

// Test against mock
let result = mock_service.oneshot(Request::new()).await.unwrap();
```

---

## Performance Considerations

### 1. Async vs Sync Overhead

**Async overhead per task:** ~0.5-2KB stack space
**Rule of thumb:** Use sync for operations < 1ms

```rust
// Bad: Async for tiny operations
async fn add(a: i32, b: i32) -> i32 {
    a + b
}

// Good: Sync for tiny operations
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### 2. Channel Capacity Tuning

```rust
// Too small: Excessive blocking
let (tx, rx) = mpsc::channel(1);

// Too large: Memory pressure
let (tx, rx) = mpsc::channel(1_000_000);

// Just right: Based on producer/consumer speed ratio
let (tx, rx) = mpsc::channel(100);
```

### 3. Buffer Size for I/O

```rust
// 8KB is typically optimal
const BUFFER_SIZE: usize = 8192;

let mut buffer = vec![0u8; BUFFER_SIZE];
```

### 4. Avoid `std::thread::sleep` in Async Code

```rust
// Bad: Blocks thread
std::thread::sleep(Duration::from_secs(1));

// Good: Yields to runtime
tokio::time::sleep(Duration::from_secs(1)).await;
```

### 5. Use `Arc` for Shared State

```rust
// Good: Arc for read-heavy shared data
let config = Arc::new(Config::load());
tokio::spawn({
    let config = config.clone();
    async move {
        // Read config
    }
});
```

---

## Migration Path for RustyCode

### Phase 1: Add Tokio (Non-Breaking)

1. Add `tokio` to workspace dependencies
2. Make existing functions async where beneficial
3. Add `#[tokio::main]` to CLI entry point

```toml
[workspace.dependencies]
tokio = { version = "1", features = ["sync", "rt", "process", "io-util", "macros"] }
```

### Phase 2: Introduce Async Patterns

1. Implement event bus with async channels
2. Add async wrappers for long-running operations
3. Introduce graceful shutdown

### Phase 3: Full Async Adoption

1. Migrate I/O operations to async
2. Add Tower middleware where appropriate
3. Implement async resource management

---

## Anti-Patterns to Avoid

### 1. Don't Block Async Code

```rust
// Bad: Blocks thread in async context
async fn bad() {
    std::thread::sleep(Duration::from_secs(1)); // DON'T
}

// Good: Async sleep
async fn good() {
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

### 2. Don't Use Unbounded Channels Unnecessarily

```rust
// Dangerous: Can cause OOM
let (tx, rx) = mpsc::channel::<Message>(unbounded());

// Better: Bounded with backpressure
let (tx, rx) = mpsc::channel(100);
```

### 3. Don't Forget Error Handling in Spawns

```rust
// Bad: Panics are ignored
tokio::spawn(async {
    panic!("lost panic");
});

// Good: Handle errors
tokio::spawn(async move {
    if let Err(e) = operation().await {
        tracing::error!("Task failed: {}", e);
    }
});
```

### 4. Don't Mix Async and Blocking Naively

```rust
// Dangerous in current-thread runtime
async fn mixed() {
    let result = std::fs::read("file.txt")?; // Blocks!
}

// Better: Use spawn_blocking
async fn better() {
    let result = tokio::task::spawn_blocking(|| {
        std::fs::read("file.txt")
    }).await??;
}
```

---

## References

### Tokio Documentation
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [tokio::sync primitives](https://docs.rs/tokio/latest/tokio/sync/index.html)

### Tower Documentation
- [Tower Overview](https://docs.rs/tower/latest/tower/)
- [Tower Service Guide](https://github.com/tower-rs/tower)

### Related RustyCode Documents
- [Event Bus Architecture](./event-bus.md)
- [ADR-0003: Event Bus System](../adr/0003-event-bus-system.md)

### External Resources
- [Async Rust Book](https://rust-lang.github.io/async-book/)
- [Tokio GitHub Examples](https://github.com/tokio-rs/tokio/tree/master/examples)
