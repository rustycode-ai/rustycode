# rustycode-bus

Event bus for inter-module communication in RustyCode.

## Purpose

Provides pub/sub event system for loosely-coupled inter-module communication. Allows components to emit events and subscribe to event types without direct dependencies on each other.

## Key Types

- `EventBus` — Central event bus with pub/sub
- `EventChannel` — Channel for specific event type
- `EventHandler` — Trait for handling events
- `EventListener` — Subscription handle
- `Event` — Base trait for all events
- `Priority` — Handler execution priority

## Event Types

- `SessionEvent` — Session lifecycle (created, completed, failed)
- `MessageEvent` — New message in conversation
- `ToolEvent` — Tool execution (started, completed, failed)
- `LLMEvent` — LLM call (started, completed, failed)
- `StorageEvent` — Data persisted
- `ContextEvent` — Context updated

## Public API

```rust
use rustycode_bus::{EventBus, EventHandler};

// Create global bus
let bus = EventBus::new();

// Publish event
bus.publish(MessageEvent {
    session_id: "sess_123".to_string(),
    content: "Hello".to_string(),
    timestamp: SystemTime::now(),
})?;

// Subscribe to events
let mut listener = bus.subscribe::<MessageEvent>()?;

// Handle in background task
tokio::spawn(async move {
    while let Ok(event) = listener.recv().await {
        println!("Got message: {}", event.content);
    }
});
```

## Handler Priority

Handlers execute in priority order:
- `High` — First (critical handlers)
- `Normal` — Middle (default)
- `Low` — Last (logging, metrics)

This ensures critical operations complete before side effects run.

## Dependencies

- `tokio` — Async channels and spawning
- `parking_lot` — Efficient locking
- `serde` — Event serialization
- `anyhow` — Error handling

## Architecture Notes

The bus uses channels for each event type. Subscribers receive copies of events. Publishing is fire-and-forget (non-blocking). Handlers run async/concurrently by default.

Critical handlers can run with `Priority::High` to execute first. Subscribers hold listener handles; dropping cancels subscription.

The bus is typically global (static LazyLock) for easy access from anywhere.

## Testing

Tests verify event delivery, subscription cancellation, priority ordering, and error handling.

## See Also

- All modules (most are event publishers/subscribers)
- `rustycode-observability` — Metrics from events
- `rustycode-tui` — UI updates from events
