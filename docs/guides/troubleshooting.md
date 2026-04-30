# RustyCode Troubleshooting Guide

This document provides solutions to common issues, debugging tips, and performance tuning guidance for RustyCode.

## Table of Contents

- [Build Issues](#build-issues)
- [Testing Problems](#testing-problems)
- [Runtime Issues](#runtime-issues)
- [Performance Issues](#performance-issues)
- [Event Bus Issues](#event-bus-issues)
- [Tool System Issues](#tool-system-issues)
- [Development Environment](#development-environment)
- [Getting Help](#getting-help)

---

## Build Issues

### Rust Version Too Old

**Problem**: Compilation fails with edition errors or unsupported features.

```
error: edition 2021 is not supported
```

**Solution**: Update Rust to latest stable:

```bash
rustup update stable
rustup default stable
```

**Verify**: Check your version:

```bash
rustc --version  # Should be 1.85 or later
```

---

### Dependency Conflicts

**Problem**: Cargo fails with dependency resolution errors.

```
error: failed to select a version for `dependency`
```

**Solutions**:

1. **Update Cargo index**:
```bash
cargo update
```

2. **Clean build**:
```bash
cargo clean
cargo build
```

3. **Check for local changes**:
```bash
git status
git checkout Cargo.lock
```

4. **Update Rust**:
```bash
rustup update
```

---

### Linker Errors

**Problem**: Build fails with linker errors.

```
error: linking with `cc` failed
```

**Solutions**:

1. **Install Xcode Command Line Tools** (macOS):
```bash
xcode-select --install
```

2. **Install build-essential** (Linux):
```bash
sudo apt-get install build-essential
```

3. **Use bundled libraries** (already configured):
```toml
# In Cargo.toml
[dependencies]
rusqlite = { version = "0.37", features = ["bundled"] }
```

---

### Out of Memory

**Problem**: Build runs out of memory.

**Solution**: Limit parallel jobs:

```bash
# Build with 2 jobs
cargo build -j 2

# Set environment variable
export CARGO_BUILD_JOBS=2
```

---

## Testing Problems

### Test Failures Due to Timing

**Problem**: Tests fail intermittently with timing issues.

```
thread 'test_async_timeout' panicked at 'assertion failed: timeout occurred'
```

**Solutions**:

1. **Run tests sequentially**:
```bash
cargo test -- --test-threads=1
```

2. **Increase timeouts**:
```rust
// In test code
let timeout = Duration::from_secs(10); // Increase from 5
```

3. **Add delays**:
```rust
tokio::time::sleep(Duration::from_millis(100)).await;
```

---

### Filesystem Test Failures

**Problem**: Tests fail due to filesystem issues.

**Solution**: Use temp directories:

```rust
use tempfile::TempDir;

#[test]
fn test_file_operation() {
    let dir = TempDir::new().unwrap();
    let test_file = dir.path().join("test.txt");

    // Use test_file for testing
    // ...
}
```

---

### Race Conditions

**Problem**: Tests pass individually but fail together.

**Solutions**:

1. **Run with race detector**:
```bash
cargo test --release -- --test-threads=1
```

2. **Add synchronization**:
```rust
use tokio::sync::Mutex;

let shared = Arc::new(Mutex::new(data));
```

3. **Use barriers**:
```rust
use tokio::sync::Barrier;

let barrier = Arc::new(Barrier::new(2));
// Wait for all tasks
barrier.wait().await;
```

---

## Runtime Issues

### Event Bus Not Receiving Events

**Problem**: Subscribers don't receive events.

**Debug Steps**:

1. **Check subscription pattern**:
```rust
let (_id, mut rx) = bus.subscribe("session.*").await?;

// Verify pattern
println!("Subscribed to: session.*");
```

2. **Check event type**:
```rust
println!("Published event type: {}", event.event_type());
```

3. **Enable debug logging**:
```bash
RUST_LOG=debug cargo run
```

4. **Check channel capacity**:
```rust
let config = EventBusConfig {
    channel_capacity: 1000, // Increase if needed
    ..Default::default()
};
```

**Solution**: Ensure event types match subscription patterns.

---

### Async Runtime Blocking

**Problem**: Async operations block unexpectedly.

**Debug Steps**:

1. **Check for blocking calls**:
```rust
// BAD: Blocks in async context
let content = std::fs::read_to_string(path)?;

// GOOD: Use async equivalent
let content = tokio::fs::read_to_string(path).await?;
```

2. **Use spawn_blocking for CPU work**:
```rust
tokio::task::spawn_blocking(|| {
    // CPU-intensive work
    heavy_computation()
}).await?
```

3. **Check for mutex locks**:
```rust
// Use tokio::sync::Mutex in async code
let mutex = Arc::new(tokio::sync::Mutex::new(data));
```

---

### Memory Leaks

**Problem**: Memory usage grows over time.

**Debug Steps**:

1. **Check for Arc cycles**:
```rust
// Avoid circular references
struct Node {
    next: Option<Arc<Node>>, // OK
    // prev: Option<Arc<Node>>, // Can cause cycles
}
```

2. **Use weak references**:
```rust
use std::sync::{Arc, Weak};

struct Node {
    next: Option<Arc<Node>>,
    prev: Option<Weak<Node>>, // Breaks cycles
}
```

3. **Profile with valgrind** (Linux):
```bash
cargo build --release
valgrind --leak-check=full target/release/rustycode
```

4. **Use memory profiler**:
```bash
cargo install heaptrack
heaptrack target/release/rustycode
```

---

## Performance Issues

### Slow Tool Execution

**Problem**: Tools execute slowly.

**Solutions**:

1. **Use compile-time tools** (5-10x faster):
```rust
// FAST: Compile-time
let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(input)?;

// SLOW: Runtime
let result = tool.execute(params, &ctx)?;
```

2. **Profile with flamegraph**:
```bash
cargo install flamegraph
cargo flamegraph --bin rustycode
```

3. **Use release builds**:
```bash
cargo run --release
```

4. **Cache results**:
```rust
use lru::LruCache;

let mut cache = LruCache::new(100);
if let Some(result) = cache.get(&key) {
    return Ok(result.clone());
}
```

---

### High Memory Usage

**Problem**: Application uses too much memory.

**Solutions**:

1. **Limit channel buffers**:
```rust
let (tx, rx) = channel(100); // Limit buffer size
```

2. **Use streaming**:
```rust
// BAD: Load entire file
let content = fs::read_to_string(path)?;

// GOOD: Stream lines
let file = File::open(path)?;
let reader = BufReader::new(file);
for line in reader.lines() {
    // Process line by line
}
```

3. **Reuse buffers**:
```rust
let mut buffer = Vec::with_capacity(1024);
file.read_to_end(&mut buffer)?;
```

---

### Event Bus Bottleneck

**Problem**: Event bus becomes slow under load.

**Solutions**:

1. **Increase channel capacity**:
```rust
let config = EventBusConfig {
    channel_capacity: 1000, // Increase from 100
    ..Default::default()
};
```

2. **Use selective subscriptions**:
```rust
// BAD: Subscribe to all events
bus.subscribe("*").await?;

// GOOD: Subscribe to specific events
bus.subscribe("session.started").await?;
```

3. **Spawn tasks for handlers**:
```rust
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        // Handle event
    }
});
```

---

## Event Bus Issues

### Subscriber Not Receiving Events

**Problem**: Subscription exists but no events received.

**Debug Steps**:

1. **Check subscription pattern**:
```rust
// Make sure pattern matches event type
bus.subscribe("session.started").await?;  // Exact match
bus.subscribe("session.*").await?;        // Wildcard
```

2. **Verify event is published**:
```rust
bus.publish(event).await?;
println!("Event published: {}", event.event_type());
```

3. **Check receiver is active**:
```rust
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        println!("Received: {}", event.event_type());
    }
});
```

4. **Enable debug logging**:
```rust
let config = EventBusConfig {
    debug_logging: true,
    ..Default::default()
};
```

---

### Channel Overflow

**Problem**: Events dropped due to full channel.

**Solutions**:

1. **Increase channel capacity**:
```rust
let config = EventBusConfig {
    channel_capacity: 1000,
    ..Default::default()
};
```

2. **Process events faster**:
```rust
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        // Process quickly
        handle_event(event).await;
    }
});
```

3. **Use multiple subscribers**:
```rust
// Distribute load across subscribers
for _ in 0..4 {
    let (_id, rx) = bus.subscribe("session.*").await?;
    spawn_handler(rx);
}
```

---

## Tool System Issues

### Type Errors in Compile-Time Tools

**Problem**: Compiler rejects tool usage.

**Example**:
```rust
error[E0277]: the trait bound `WriteFileInput:Into<ReadFileInput>` is not satisfied
```

**Solution**: Use correct input types:

```rust
// CORRECT
let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
    path: PathBuf::from("test.txt"),
    start_line: None,
    end_line: None,
})?;

// WRONG: Type mismatch
let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(
    WriteFileInput { ... }  // Wrong type!
)?;
```

---

### Tool Not Found

**Problem**: Runtime tool execution fails.

```
Error: unknown tool 'my_tool'
```

**Solutions**:

1. **Check tool is registered**:
```rust
let mut registry = ToolRegistry::new();
registry.register(MyTool);  // Make sure this is called
```

2. **Verify tool name**:
```rust
impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"  // Must match call name
    }
}
```

3. **Use default registry**:
```rust
let registry = default_registry();  // Includes built-in tools
```

---

### Permission Denied

**Problem**: Tool execution fails with permission error.

```
Error: permission denied: tool 'write_file' requires Write permission
```

**Solution**: Increase max permission:

```rust
let ctx = ToolContext::new(cwd)
    .with_max_permission(ToolPermission::Write);
```

---

## Development Environment

### IDE Issues

**rust-analyzer not working**:

**Solution**: Ensure rustup is installed and in PATH:

```bash
# Check Rust installation
which rustc
rustc --version

# Reinstall rust-analyzer in VS Code
# Ctrl+Shift+P -> "rust-analyzer: Server restart"
```

---

### VS Code Extensions

**Required extensions**:
- `rust-analyzer` - Language server
- `CodeLLDB` - Debugger
- `Even Better TOML` - TOML syntax

**Install from command palette**:
```
Ctrl+Shift+P -> "Extensions: Install Extensions"
```

---

### Git Issues

**Line ending issues**:

```bash
# Configure Git for line endings
git config core.autocrlf input  # macOS/Linux
git config core.autocrlf true   # Windows
```

**Large file issues**:

```bash
# Check for large files
find . -type f -size +10M

# Use Git LFS if needed
git lfs track "*.db"
git lfs track "*.sqlite"
```

---

## Debugging Tips

### Enable Debug Logging

```bash
# Set log level
export RUST_LOG=debug
export RUST_LOG=rustycode_bus=trace

# Run with logging
cargo run
```

### Use LLDB Debugger

```bash
# Build with debug symbols
cargo build

# Start debugger
rust-lldb target/debug/rustycode

# Set breakpoint
(lldb) breakpoint set --name main

# Run
(lldb) run
```

### Memory Profiling

**Linux**:
```bash
cargo install heaptrack
heaptrack target/release/rustycode
heaptrack_print heaptrack.$pid.gz
```

**macOS**:
```bash
cargo install instruments
cargo instruments --release
```

### CPU Profiling

```bash
cargo install flamegraph
cargo flamegraph --bin rustycode
```

### Leak Detection

**Valgrind** (Linux):
```bash
cargo build --release
valgrind --leak-check=full --show-leak-kinds=all target/release/rustycode
```

---

## Performance Tuning

### Compiler Optimizations

**Release profile**:
```toml
# In Cargo.toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

**Build with optimizations**:
```bash
cargo build --release
```

### Runtime Optimizations

**Use tokio multi-threaded**:
```rust
#[tokio::main]
async fn main() {
    // Already multi-threaded by default
}
```

**Configure thread pool**:
```rust
let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4)
    .enable_all()
    .build()
    .unwrap();
```

### Memory Optimizations

**Reuse allocations**:
```rust
let mut buffer = Vec::with_capacity(1024);
for item in items {
    buffer.clear();
    // Reuse buffer
}
```

**Use Cow for lazy cloning**:
```rust
use std::borrow::Cow;

fn process(s: Cow<str>) {
    // Only clones if modified
}
```

---

## Getting Help

### Check Resources

1. **Documentation**:
   - [Developer Guide](developer-guide.md)
   - [API Reference](api-reference.md)
   - [Architecture](architecture.md)

2. **GitHub**:
   - [Issues](https://github.com/original/rustycode/issues)
   - [Discussions](https://github.com/original/rustycode/discussions)

### Ask Questions

**Before asking**:
1. Search existing issues and docs
2. Try to reproduce with minimal example
3. Gather error messages and logs

**When asking**:
1. Describe the problem clearly
2. Show what you've tried
3. Include error messages
4. Provide minimal reproduction case
5. Specify your environment (OS, Rust version)

### Report Bugs

**Include**:
- Rust version (`rustc --version`)
- OS and version
- Minimal reproduction code
- Error messages and backtraces
- Expected vs actual behavior

### Request Features

**Include**:
- Use case and motivation
- Proposed solution
- Alternative approaches considered
- Potential impact on existing code

---

## Quick Reference

### Common Commands

```bash
# Build
cargo build
cargo build --release

# Test
cargo test
cargo test --release
cargo test -- --test-threads=1

# Debug
RUST_LOG=debug cargo run

# Profile
cargo flamegraph

# Docs
cargo doc --open
```

### Environment Variables

```bash
# Logging
export RUST_LOG=debug
export RUST_LOG=error

# Build
export CARGO_BUILD_JOBS=4

# Test
export RUST_TEST_THREADS=1
```

### Useful Crates

```bash
# Profiling
cargo install flamegraph
cargo install heaptrack

# Development
cargo install cargo-watch
cargo install cargo-expand
cargo install cargo-edit
```

---

## Next Steps

- Read [Developer Guide](developer-guide.md)
- Check [API Reference](api-reference.md)
- Review [Architecture](architecture.md)
- Browse [ADRs](adr/)

Good luck troubleshooting! 🍀
