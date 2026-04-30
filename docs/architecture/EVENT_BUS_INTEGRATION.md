# EventBus Integration Across Crates

This document describes how the EventBus is integrated across all crates in the RustyCode workspace, creating a cohesive event-driven architecture.

## Overview

The EventBus provides decoupled communication between crates through a publish-subscribe pattern. Events flow through the system asynchronously, allowing crates to react to state changes without tight coupling.

## Architecture

```
┌─────────────────┐
│ rustycode-tools │ ── publishes ──▶ ToolExecutedEvent
└─────────────────┘
                          │
                          ▼
┌─────────────────┐     ┌─────────────────┐
│ rustycode-bus   │◀────│   Event Bus     │
│   (Core)        │────▶│  (Central Hub)  │
└─────────────────┘     └─────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ rustycode-core  │ │ rustycode-      │ │ rustycode-      │
│   (Runtime)     │ │   storage       │ │   -cli          │
│   (Subscribes)  │ │   (Persists)    │ │   (Displays)    │
└─────────────────┘ └─────────────────┘ └─────────────────┘
```

## Crate Integration

### rustycode-bus (Core Infrastructure)

**Purpose**: Provides the event bus implementation

**Key Features**:
- Type-safe event publishing and subscription
- Wildcard pattern matching (`session.*`, `tool.*`, `*`)
- Event hooks for cross-cutting concerns
- Metrics tracking
- Thread-safe async operations

**Exports**:
```rust
use rustycode_bus::{
    EventBus, Event, SubscriptionHandle, HookPhase,
    SessionStartedEvent, ToolExecutedEvent, PlanCreatedEvent,
    ContextAssembledEvent, InspectionCompletedEvent,
};
```

### rustycode-tools (Event Publisher)

**Purpose**: Publishes events when tools are executed

**Integration**:
```rust
use rustycode_tools::ToolExecutor;
use rustycode_bus::EventBus;
use std::sync::Arc;

let bus = Arc::new(EventBus::new());
let executor = ToolExecutor::with_event_bus(cwd, bus);

// Tool execution now automatically publishes events
let result = executor.execute_with_session(&call, Some(session_id));
```

**Events Published**:
- `ToolExecutedEvent` - Every tool execution
  - Includes tool name, arguments, success status, output, errors
  - Correlated with session ID

### rustycode-storage (Event Subscriber)

**Purpose**: Persists events to the database

**Integration**:
```rust
use rustycode_storage::Storage;
use rustycode_bus::EventBus;

let bus = Arc::new(EventBus::new());
let (storage, _handle) = Storage::open_with_subscription(&db_path, &bus).await?;

// All events are now automatically persisted
```

**Subscription Pattern**:
- Subscribes to `*` (all events)
- Runs background task to receive and persist events
- Uses blocking tasks for database writes

**Storage Schema**:
```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    at TEXT NOT NULL,
    kind TEXT NOT NULL,
    detail TEXT NOT NULL
);
```

### rustycode-runtime (Event Coordinator)

**Purpose**: Coordinates events and provides high-level runtime services

**Integration**:
```rust
use rustycode_runtime::AsyncRuntime;

let runtime = AsyncRuntime::load(cwd).await?;

// Runtime automatically:
// 1. Publishes session lifecycle events
// 2. Registers logging hooks
// 3. Subscribes to relevant events
// 4. Coordinates event flow across crates
```

**Hooks Registered**:
- `PrePublish`: Logs all events before distribution
- `PostPublish`: Tracks event delivery

**Events Published**:
- `SessionStartedEvent` - When a session is created
- `ContextAssembledEvent` - When context is built
- `PlanCreatedEvent` - When a plan is generated
- `PlanApprovedEvent` - When a plan is approved
- `PlanRejectedEvent` - When a plan is rejected

## Event Types

### Session Events

**`session.started`**:
```rust
SessionStartedEvent::new(
    session_id,
    "Analyze codebase".to_string(),
    "Initial session".to_string(),
)
```

### Tool Events

**`tool.executed`**:
```rust
ToolExecutedEvent::new(
    session_id,
    "read_file".to_string(),
    json!({ "path": "/path/to/file" }),
    true,  // success
    "File contents".to_string(),
    None,  // error
)
```

### Plan Events

**`plan.created`**:
```rust
PlanCreatedEvent::new(
    session_id,
    plan,
    "Plan generated".to_string(),
)
```

**`plan.approved`**:
```rust
PlanApprovedEvent::new(
    session_id,
    "Plan approved by user".to_string(),
)
```

**`plan.rejected`**:
```rust
PlanRejectedEvent::new(
    session_id,
    "Plan rejected by user".to_string(),
)
```

### Context Events

**`context.assembled`**:
```rust
ContextAssembledEvent::new(
    session_id,
    context_plan,
    "Context ready".to_string(),
)
```

### Inspection Events

**`inspection.completed`**:
```rust
InspectionCompletedEvent::new(
    working_dir,
    git_status,
    lsp_server_count,
    memory_entry_count,
    skill_count,
    "Inspection complete".to_string(),
)
```

## Subscription Patterns

### Exact Match

Subscribe to a specific event type:
```rust
let (_id, mut rx) = bus.subscribe("tool.executed").await?;
```

### Wildcard Match

Subscribe to all events in a category:
```rust
// All session events
let (_id, mut rx) = bus.subscribe("session.*").await?;

// All tool events
let (_id, mut rx) = bus.subscribe("tool.*").await?;

// All plan events
let (_id, mut rx) = bus.subscribe("plan.*").await?;
```

### Global Wildcard

Subscribe to all events:
```rust
let (_id, mut rx) = bus.subscribe("*").await?;
```

## Event Handlers

### Basic Handler

```rust
let (_id, mut rx) = bus.subscribe("tool.executed").await?;

tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        let data = event.serialize();
        println!("Tool: {}", data["tool_name"]);
        println!("Success: {}", data["success"]);
    }
});
```

### Conditional Handler

```rust
let (_id, mut rx) = bus.subscribe("tool.executed").await?;

tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        let data = event.serialize();

        // Only process failed tool executions
        if !data["success"].as_bool().unwrap() {
            eprintln!("Tool failed: {}", data["tool_name"]);
            // Send alert, log to error tracking, etc.
        }
    }
});
```

### Aggregating Handler

```rust
let event_count = Arc::new(AtomicUsize::new(0));
let count_clone = event_count.clone();

let (_id, mut rx) = bus.subscribe("*").await?;

tokio::spawn(async move {
    while let Ok(_event) = rx.recv().await {
        count_clone.fetch_add(1, Ordering::SeqCst);
    }
});

// Later...
println!("Total events: {}", event_count.load(Ordering::SeqCst));
```

## Event Hooks

### Logging Hook

```rust
bus.register_hook(HookPhase::PrePublish, |event| {
    tracing::info!("Event: {}", event.event_type());
    Ok(())
}).await;
```

### Metrics Hook

```rust
bus.register_hook(HookPhase::PostPublish, |event| {
    metrics.increment(event.event_type());
    Ok(())
}).await;
```

### Validation Hook

```rust
bus.register_hook(HookPhase::PrePublish, |event| {
    if event.event_type() == "tool.executed" {
        let data = event.serialize();
        if data["success"].as_bool().unwrap() {
            tracing::warn!("Successful tool execution detected");
        }
    }
    Ok(())
}).await;
```

## Testing

### Unit Tests

Each crate includes unit tests for event integration:

```rust
#[tokio::test]
async fn test_tool_publishes_event() {
    let bus = Arc::new(EventBus::new());
    let executor = ToolExecutor::with_event_bus(tmp_dir, bus.clone());

    let (_id, mut rx) = bus.subscribe("tool.executed").await.unwrap();

    executor.execute(&call);

    let event = rx.recv().await.unwrap();
    assert_eq!(event.event_type(), "tool.executed");
}
```

### Integration Tests

Cross-crate integration tests validate the complete event flow:

```bash
# Run all integration tests
cargo test --test cross_crate_event_flow

# Run specific test
cargo test --test cross_crate_event_flow test_tool_execution_publishes_events
```

### Examples

Run the comprehensive example:

```bash
cargo run --example cross_crate_event_example
```

## Performance Considerations

### Async Event Publishing

Events are published asynchronously to avoid blocking tool execution:

```rust
tokio::spawn(async move {
    if let Err(e) = bus.publish(event).await {
        tracing::warn!("Failed to publish event: {:?}", e);
    }
});
```

### Channel Capacity

Event channels have configurable capacity (default: 100):

```rust
use rustycode_bus::EventBusConfig;

let config = EventBusConfig {
    channel_capacity: 1000,
    max_subscribers: 100,
    debug_logging: false,
};

let bus = EventBus::with_config(config);
```

### Metrics Tracking

Monitor event flow using built-in metrics:

```rust
let metrics = bus.metrics();
println!("Events published: {}", metrics.events_published);
println!("Events delivered: {}", metrics.events_delivered);
println!("Events failed: {}", metrics.events_failed);
```

## Best Practices

### 1. Always Handle Subscription Errors

```rust
let (_id, mut rx) = bus.subscribe("tool.*")
    .await
    .context("Failed to subscribe to tool events")?;
```

### 2. Use Wildcards Judiciously

Wildcards are powerful but can receive many events:

```rust
// Good: Specific subscription
let (_id, mut rx) = bus.subscribe("tool.executed").await?;

// Use with caution: Broad subscription
let (_id, mut rx) = bus.subscribe("*").await?;
```

### 3. Provide Context in Events

Events should include sufficient context:

```rust
ToolExecutedEvent::new(
    session_id,           // Correlation
    "read_file".to_string(), // Action
    json!({ "path": "/file" }), // Context
    true,                 // Result
    "content".to_string(), // Output
    None,                 // Errors
)
```

### 4. Handle Subscription Cleanup

Use `SubscriptionHandle` for automatic cleanup:

```rust
let (id, rx) = bus.subscribe("tool.*").await?;
let handle = SubscriptionHandle::new(id, bus);

// Automatically unsubscribes when dropped
```

### 5. Use Hooks for Cross-Cutting Concerns

Hooks are ideal for logging, metrics, validation:

```rust
bus.register_hook(HookPhase::PrePublish, |event| {
    // Log, validate, enrich
    Ok(())
}).await;
```

## Troubleshooting

### Events Not Received

1. Check subscription pattern matches event type
2. Verify receiver task is running
3. Check channel capacity (may need increase)
4. Enable debug logging

### Events Not Persisted

1. Verify storage subscription is active
2. Check database write permissions
3. Review storage logs for errors
4. Confirm event data is serializable

### Performance Issues

1. Reduce number of wildcard subscriptions
2. Increase channel capacity if needed
3. Use async handlers for heavy processing
4. Monitor metrics for bottlenecks

## Future Enhancements

Potential improvements to the event system:

1. **Event Replay**: Replay events from storage for debugging
2. **Event Filtering**: Client-side filtering to reduce events
3. **Dead Letter Queue**: Capture failed events for analysis
4. **Event Batching**: Batch events for efficiency
5. **Distributed Event Bus**: Multi-process event distribution
6. **Event Schemas**: Strongly typed event validation
7. **Event Versioning**: Handle event schema evolution

## References

- [Event Bus API Documentation](../crates/rustycode-bus/README.md)
- [Integration Tests](../tests/integration/cross_crate_event_flow.rs)
- [Example Code](../examples/cross_crate_event_example.rs)
- [Event Types](../crates/rustycode-bus/src/events.rs)
