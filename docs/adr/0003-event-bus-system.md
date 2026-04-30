# ADR 0003: Event Bus System

**Status**: Accepted
**Date**: 2025-03-12
**Related**: ADR-0001 (Core Principles), ADR-0002 (Context Budgeting)

## Context

RustyCode currently uses direct function calls between crates for communication. Session events exist in the `rustycode-protocol` crate, but there's no centralized event routing mechanism. This creates several problems:

1. **Tight Coupling**: Crates depend directly on each other's interfaces
2. **No Extension Point**: Adding cross-cutting concerns requires modifying multiple files
3. **Limited Observability**: No central point to log/metrics all system events
4. **Scalability Issues**: Direct calls don't scale well as the system grows

Example of current coupling:
```rust
// rustycode-core must know about storage
pub async fn run(&self, cwd: &Path, task: &str) -> Result<RunReport> {
    let session = Session { /* ... */ };
    self.storage.insert_session(&session)?;  // Direct call
    self.storage.insert_event(&event)?;      // Direct call
}
```

## Decision

Implement a type-safe, asynchronous event bus system as a new `rustycode-bus` crate.

### Key Design Choices

1. **Trait-Based Type Safety**: Events implement a trait, not an enum
2. **Wildcard Subscriptions**: Support patterns like `session.*` and `*.error`
3. **Tokio Primitives**: Use `broadcast::channel` for async message passing
4. **Hook System**: Pre/post-publish hooks for cross-cutting concerns
5. **Gradual Migration**: Three-phase approach to avoid breaking changes

### Architecture

```
Publisher → EventBus → [Hooks] → Wildcard Matcher → Subscribers
```

## Event Trait Design

We chose a trait-based approach over enum-based events:

**Pros**:
- External crates can define their own event types
- No central enum to modify when adding events
- Better separation of concerns

**Cons**:
- Requires downcasting to access concrete types
- Slightly more complex API

Alternative considered (Enum-based):
```rust
pub enum Event {
    SessionStarted(SessionStartedEvent),
    ContextAssembled(ContextAssembledEvent),
    // All events must be defined here
}
```

We rejected this because it creates a bottleneck and prevents external crates from defining events.

## Wildcard Subscription Strategy

We use regex conversion for wildcard matching:

```rust
"session.*"  → "^session\..*$"
"*.started"  → "^.*\.started$"
"*"          → "^.*$"
```

**Alternative**: Token-based matching (`["session", "*"]`)
- Rejected because regex is more flexible and Rust's `regex` crate is fast

## Concurrency Model

Using `tokio::sync::broadcast`:

**Pros**:
- Built-in backpressure handling
- Efficient multi-producer, multi-consumer
- No channels per subscriber (single sender)

**Cons**:
- `recv()` can miss events if lagging (acceptable for our use case)

**Alternative**: `mpsc::channel` per subscriber
- Rejected because it requires managing N channels for N subscribers

## Hook System

Three-phase hooks: PrePublish, PostPublish, OnError

**Alternative**: Middleware chain (like in web frameworks)
- Rejected because it adds unnecessary complexity for async events

## Migration Strategy

Three-phase approach:

**Phase 1 (Shadow Mode)**: Dual-write to storage and bus
- Non-breaking
- Allows testing event bus without affecting existing behavior

**Phase 2 (Internal)**: Crates publish to bus, storage subscribes
- Event bus becomes primary communication mechanism
- Storage moves to subscriber

**Phase 3 (Complete)**: Remove direct storage calls
- All communication flows through event bus

**Alternative**: Big-bang rewrite
- Rejected because it's too risky and harder to debug

## Consequences

### Positive

1. **Decoupling**: Crates communicate via events, not direct calls
2. **Extensibility**: Easy to add new event types and subscribers
3. **Observability**: Central point for logging and metrics
4. **Testability**: Can test event flows in isolation
5. **Future-Proof**: Foundation for distributed systems

### Negative

1. **Complexity**: Additional indirection layer
2. **Debugging**: Harder to trace execution flow
3. **Performance**: Event cloning overhead (mitigate with Arc)
4. **Learning Curve**: Developers must understand pub/sub patterns

### Risks

1. **Memory Leaks**: Subscribers not properly cleaned up
   - Mitigation: `SubscriptionHandle` with Drop trait
2. **Channel Overflow**: Slow subscribers miss events
   - Mitigation: Configurable channel capacity
3. **Hook Failures**: Hooks can block event delivery
   - Mitigation: Async hooks with timeout

## Implementation

See [Event Bus Architecture Design](../design/event-bus.md) for detailed implementation.

### Timeline

- **Week 1**: Create `rustycode-bus` crate with core types
- **Week 2**: Implement event bus and tests
- **Week 3**: Phase 1 migration (shadow mode)
- **Week 4**: Phase 2 migration (internal)
- **Week 5**: Phase 3 migration (complete)

## Alternatives Considered

### 1. Direct Function Calls (Status Quo)
- **Pros**: Simple, explicit
- **Cons**: Tight coupling, no extensibility
- **Verdict**: Doesn't meet requirements

### 2. Actor Model (Rust async channels only)
- **Pros**: No shared state
- **Cons**: Complex, requires runtime for each actor
- **Verdict**: Overkill for our use case

### 3. External Message Queue (Redis, RabbitMQ)
- **Pros**: Distributed, persistent
- **Cons**: External dependency, deployment complexity
- **Verdict**: Future enhancement, not initial implementation

## References

- [Tokio Broadcast Channels](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html)
- [Event-Driven Architecture](https://en.wikipedia.org/wiki/Event-driven_architecture)
- [Event Sourcing Pattern](https://martinfowler.com/eaaDev/EventSourcing.html)
