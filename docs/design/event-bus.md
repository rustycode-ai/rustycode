# RustyCode Event Bus Architecture

**Status**: Design Document
**Author**: RustyCode Ensemble
**Created**: 2025-03-12
**Related ADRs**: 0001-core-principles.md, 0002-context-budgeting.md

## Executive Summary

This document describes the architecture for a type-safe, asynchronous event bus system for RustyCode. The event bus enables decoupled communication between crates while maintaining compile-time type safety and supporting wildcard subscriptions for cross-cutting concerns.

## Motivation

### Current State
- Session events (`SessionStarted`, `ContextAssembled`, `InspectionCompleted`) exist in `rustycode-protocol`
- Direct function calls between crates create tight coupling
- No centralized event routing or subscription mechanism
- Cross-cutting concerns (logging, metrics) require code duplication

### Goals
1. **Type Safety**: Compile-time guarantee that event publishers and subscribers match
2. **Decoupling**: Crates communicate via events, not direct dependencies
3. **Extensibility**: Easy to add new event types and subscribers
4. **Performance**: Async/await with tokio for high-throughput scenarios
5. **Wildcards**: Subscribe to event patterns (e.g., `session.*`, `git.*`)
6. **Hooks**: Pre/post processing for logging, metrics, error handling

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     RustyCode Event Bus                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────┐    ┌────────────┐    ┌──────────────────┐        │
│  │ Publisher│───>│EventBus    │───>│  Dispatch Logic  │        │
│  │          │    │            │    │                  │        │
│  └──────────┘    └────────────┘    └────────┬─────────┘        │
│                                     │                           │
│                                     ▼                           │
│                          ┌─────────────────────┐               │
│                          │   Wildcard Matcher  │               │
│                          └──────────┬──────────┘               │
│                                     │                           │
│                    ┌────────────────┴────────────────┐         │
│                    ▼                                 ▼         │
│          ┌─────────────────┐               ┌──────────────┐   │
│          │Exact Subscribers│               │Wildcard Subs │   │
│          └────────┬────────┘               └──────┬───────┘   │
│                   │                                │           │
│                   └────────────┬───────────────────┘           │
│                                ▼                               │
│                   ┌─────────────────────┐                     │
│                   │   Hook Processing   │                     │
│                   │  - Pre-publish      │                     │
│                   │  - Post-publish     │                     │
│                   │  - Error handling   │                     │
│                   └──────────┬──────────┘                     │
│                                │                               │
│                                ▼                               │
│                   ┌─────────────────────┐                     │
│                   │  Subscriber Notify  │                     │
│                   │  (async channels)   │                     │
│                   └─────────────────────┘                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Crate Structure

### New Crate: `rustycode-bus`

**Location**: `/crates/rustycode-bus/`

**Responsibilities**:
- Core event bus implementation
- Type-safe event registration and routing
- Wildcard subscription matching
- Hook system for cross-cutting concerns
- Async channel management

**Dependencies**:
- `tokio` (runtime, sync primitives)
- `rustycode-protocol` (event types)
- `serde` (serialization)
- `tracing` (logging)
- `regex` (wildcard matching)
- `thiserror` (error types)

### Updated Crates

#### `rustycode-protocol`
- Add `Event` trait for type-safe event definitions
- Extend `EventKind` enum with new event types
- Add event metadata structures

#### `rustycode-core`
- Integrate `EventBus` into `Runtime`
- Replace direct event storage calls with event bus publishes
- Maintain backward compatibility during migration

## Core Type Definitions

### Event Trait

```rust
use serde::{Serialize, Deserialize};
use std::any::Any;
use std::hash::Hash;

/// Type-safe event trait
pub trait Event: Any + Send + Sync + 'static {
    /// Unique event type identifier (e.g., "session.started")
    fn event_type(&self) -> &'static str;

    /// Event timestamp
    fn timestamp(&self) -> DateTime<Utc>;

    /// Convert to Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Clone as boxed trait object
    fn clone_box(&self) -> Box<dyn Event>;

    /// Serialize for transport/storage
    fn serialize(&self) -> Result<String, serde_json::Error>;
}

/// Clone implementation for trait object
impl Clone for Box<dyn Event> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
```

### Event Definitions

```rust
/// Macro to generate type-safe events
macro_rules! define_event {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(pub $field:ident: $ty:ty),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub struct $name {
            #[serde(skip_serializing_if = "Option::is_none")]
            pub event_id: Option<Uuid>,
            pub timestamp: DateTime<Utc>,
            $(pub $field: $ty),*
        }

        impl Event for $name {
            fn event_type(&self) -> &'static str {
                stringify!($name).to_lowercase().replace('_', ".")
            }

            fn timestamp(&self) -> DateTime<Utc> {
                self.timestamp
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn clone_box(&self) -> Box<dyn Event> {
                Box::new(self.clone())
            }

            fn serialize(&self) -> Result<String, serde_json::Error> {
                serde_json::to_string(self)
            }
        }
    };
}

// Existing events from protocol crate
define_event! {
    pub struct SessionStartedEvent {
        pub session_id: SessionId,
        pub task: String,
        pub detail: String,
    }
}

define_event! {
    pub struct ContextAssembledEvent {
        pub session_id: SessionId,
        pub context_plan: ContextPlan,
        pub detail: String,
    }
}

// New events for other crates
define_event! {
    pub struct GitStatusChangedEvent {
        pub root: Option<PathBuf>,
        pub branch: Option<String>,
        pub dirty: bool,
    }
}

define_event! {
    pub struct LspServerDiscoveredEvent {
        pub server_name: String,
        pub installed: bool,
        pub path: Option<String>,
    }
}

define_event! {
    pub struct MemoryEntryAddedEvent {
        pub entry_path: String,
        pub preview: String,
        pub session_id: Option<SessionId>,
    }
}

define_event! {
    pub struct SkillDiscoveredEvent {
        pub skill_name: String,
        pub skill_path: String,
    }
}

define_event! {
    pub struct ErrorEvent {
        pub error_type: String,
        pub message: String,
        pub source: String,
        pub session_id: Option<SessionId>,
    }
}
```

### Event Bus Core

```rust
use tokio::sync::{broadcast, RwLock, RwLockWriteGuard};
use std::collections::HashMap;
use std::sync::Arc;
use regex::Regex;

/// Subscriber channel type
type SubscriberSender = broadcast::Sender<Box<dyn Event>>;

/// Subscription filter
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionFilter {
    /// Exact event type match (e.g., "session.started")
    Exact(String),

    /// Wildcard match (e.g., "session.*", "*.error")
    Wildcard(String, Regex),
}

impl SubscriptionFilter {
    /// Create a new subscription filter
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        if pattern.contains('*') {
            // Convert wildcard to regex
            let regex_str = pattern
                .replace('.', r"\.")
                .replace('*', ".*")
                .replace('?', ".");
            let regex = Regex::new(&format!("^{}$", regex_str))?;
            Ok(Self::Wildcard(pattern.to_string(), regex))
        } else {
            Ok(Self::Exact(pattern.to_string()))
        }
    }

    /// Check if event type matches this filter
    pub fn matches(&self, event_type: &str) -> bool {
        match self {
            Self::Exact(pattern) => pattern == event_type,
            Self::Wildcard(_, regex) => regex.is_match(event_type),
        }
    }
}

/// Subscriber information
#[derive(Clone)]
struct Subscriber {
    id: Uuid,
    filter: SubscriptionFilter,
    sender: SubscriberSender,
}

/// Hook function type
pub type HookFn = Arc<dyn Fn(&dyn Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync>;

/// Hook phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPhase {
    PrePublish,
    PostPublish,
    OnError,
}

/// Event bus configuration
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Channel buffer size per subscriber
    pub channel_capacity: usize,

    /// Maximum number of subscribers
    pub max_subscribers: usize,

    /// Enable debug logging
    pub debug_logging: bool,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 100,
            max_subscribers: 1000,
            debug_logging: false,
        }
    }
}

/// Main event bus
pub struct EventBus {
    config: EventBusConfig,
    subscribers: Arc<RwLock<HashMap<Uuid, Subscriber>>>,
    hooks: Arc<RwLock<HashMap<HookPhase, Vec<HookFn>>>>,
    metrics: Arc<EventBusMetrics>,
}

#[derive(Debug, Default)]
struct EventBusMetrics {
    events_published: AtomicU64,
    events_delivered: AtomicU64,
    events_failed: AtomicU64,
    subscriber_count: AtomicU64,
}

impl EventBus {
    /// Create a new event bus
    pub fn new(config: EventBusConfig) -> Self {
        Self {
            config,
            subscribers: Arc::default(),
            hooks: Arc::default(),
            metrics: Arc::default(),
        }
    }

    /// Publish an event to all matching subscribers
    pub async fn publish<T>(&self, event: T) -> Result<(), EventBusError>
    where
        T: Event + Clone,
    {
        let event_type = event.event_type();

        // Run pre-publish hooks
        self.run_hooks(HookPhase::PrePublish, &event).await?;

        tracing::debug!("Publishing event: {}", event_type);

        let subscribers = self.subscribers.read().await;
        let mut recv_count = 0;

        for subscriber in subscribers.values() {
            if subscriber.filter.matches(event_type) {
                match subscriber.sender.send(Box::new(event.clone_box())) {
                    Ok(count) => {
                        recv_count += count;
                        self.metrics.events_delivered.fetch_add(count as u64, Ordering::Relaxed);
                    }
                    Err(_) => {
                        // No active receivers for this subscriber
                        tracing::warn!("No receivers for subscriber {}", subscriber.id);
                    }
                }
            }
        }

        // Run post-publish hooks
        self.run_hooks(HookPhase::PostPublish, &event).await?;

        self.metrics.events_published.fetch_add(1, Ordering::Relaxed);

        tracing::debug!("Event {} delivered to {} receivers", event_type, recv_count);
        Ok(())
    }

    /// Subscribe to events matching a pattern
    pub async fn subscribe(
        &self,
        pattern: &str,
    ) -> Result<(Uuid, broadcast::Receiver<Box<dyn Event>>), EventBusError> {
        let filter = SubscriptionFilter::new(pattern)?;
        let id = Uuid::new_v4();
        let (sender, receiver) = broadcast::channel(self.config.channel_capacity);

        let subscriber = Subscriber {
            id,
            filter,
            sender,
        };

        {
            let mut subscribers = self.subscribers.write().await;
            if subscribers.len() >= self.config.max_subscribers {
                return Err(EventBusError::MaxSubscribersReached);
            }
            subscribers.insert(id, subscriber);
            self.metrics.subscriber_count.fetch_add(1, Ordering::Relaxed);
        }

        tracing::debug!("New subscription {}: pattern={}", id, pattern);
        Ok((id, receiver))
    }

    /// Unsubscribe from events
    pub async fn unsubscribe(&self, id: Uuid) -> Result<(), EventBusError> {
        let mut subscribers = self.subscribers.write().await;
        subscribers.remove(&id).ok_or(EventBusError::SubscriberNotFound)?;
        self.metrics.subscriber_count.fetch_sub(1, Ordering::Relaxed);
        tracing::debug!("Unsubscribed: {}", id);
        Ok(())
    }

    /// Register a hook for a specific phase
    pub async fn register_hook<F>(&self, phase: HookPhase, hook: F)
    where
        F: Fn(&dyn Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
    {
        let mut hooks = self.hooks.write().await;
        hooks.entry(phase).or_default().push(Arc::new(hook));
    }

    /// Run hooks for a specific phase
    async fn run_hooks(&self, phase: HookPhase, event: &dyn Event) -> Result<(), EventBusError> {
        let hooks = self.hooks.read().await;
        if let Some(phase_hooks) = hooks.get(&phase) {
            for hook in phase_hooks {
                if let Err(e) = hook(event) {
                    tracing::error!("Hook error on {:?}: {}", phase, e);
                    if phase == HookPhase::OnError {
                        return Err(EventBusError::HookError(e.to_string()));
                    }
                }
            }
        }
        Ok(())
    }

    /// Get current metrics
    pub fn metrics(&self) -> EventBusMetricsSnapshot {
        EventBusMetricsSnapshot {
            events_published: self.metrics.events_published.load(Ordering::Relaxed),
            events_delivered: self.metrics.events_delivered.load(Ordering::Relaxed),
            events_failed: self.metrics.events_failed.load(Ordering::Relaxed),
            subscriber_count: self.metrics.subscriber_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EventBusMetricsSnapshot {
    pub events_published: u64,
    pub events_delivered: u64,
    pub events_failed: u64,
    pub subscriber_count: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    #[error("Invalid subscription pattern: {0}")]
    InvalidPattern(String),

    #[error("Maximum subscribers reached")]
    MaxSubscribersReached,

    #[error("Subscriber not found")]
    SubscriberNotFound,

    #[error("Hook error: {0}")]
    HookError(String),

    #[error("Event serialization error: {0}")]
    SerializationError(String),
}
```

### Helper Types

```rust
/// Subscriber handle for automatic cleanup
pub struct SubscriptionHandle {
    id: Uuid,
    bus: Arc<EventBus>,
}

impl SubscriptionHandle {
    pub fn new(id: Uuid, bus: Arc<EventBus>) -> Self {
        Self { id, bus }
    }

    pub async fn unsubscribe(self) -> Result<(), EventBusError> {
        self.bus.unsubscribe(self.id).await
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        // Best-effort unsubscribe on drop
        let bus = self.bus.clone();
        let id = self.id;
        tokio::spawn(async move {
            let _ = bus.unsubscribe(id).await;
        });
    }
}
```

## API Design

### Publishing Events

```rust
use rustycode_bus::{EventBus, SessionStartedEvent};
use rustycode_protocol::SessionId;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bus = EventBus::default();

    // Simple publish
    let event = SessionStartedEvent {
        event_id: None,
        timestamp: Utc::now(),
        session_id: SessionId::new(),
        task: "Analyze codebase".to_string(),
        detail: "Initial session".to_string(),
    };

    bus.publish(event).await?;

    Ok(())
}
```

### Subscribing to Events

```rust
use rustycode_bus::{EventBus, SubscriptionHandle};

// Exact subscription
let bus = Arc::new(EventBus::default());
let (id, mut rx) = bus.subscribe("session.started").await?;

tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        println!("Received session.started event");
        // Downcast to concrete type
        if let Some(event) = event.as_any().downcast_ref::<SessionStartedEvent>() {
            println!("Session: {}", event.session_id);
        }
    }
});

// Wildcard subscription
let (id, mut rx) = bus.subscribe("session.*").await?;

tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        println!("Received session event: {}", event.event_type());
    }
});

// All events
let (id, mut rx) = bus.subscribe("*").await?;
```

### Using Subscription Handle

```rust
use rustycode_bus::{EventBus, SubscriptionHandle};

let bus = Arc::new(EventBus::default());
let (id, rx) = bus.subscribe("git.*").await?;

// Handle auto-unsubscribes when dropped
let handle = SubscriptionHandle::new(id, bus.clone());

tokio::spawn(async move {
    let mut rx = rx;
    while let Ok(event) = rx.recv().await {
        // Handle event
    }
    // handle is dropped here, automatically unsubscribing
});
```

### Registering Hooks

```rust
use rustycode_bus::{EventBus, HookPhase};

// Logging hook
bus.register_hook(HookPhase::PrePublish, |event| {
    tracing::info!("Event pre-publish: {}", event.event_type());
    Ok(())
}).await;

// Metrics hook
bus.register_hook(HookPhase::PostPublish, |event| {
    metrics::counter("events_published", event.event_type());
    Ok(())
}).await;

// Error handling hook
bus.register_hook(HookPhase::OnError, |event| {
    if let Some(error_event) = event.as_any().downcast_ref::<ErrorEvent>() {
        tracing::error!("Error event: {}", error_event.message);
    }
    Ok(())
}).await;
```

## Integration with Existing Crates

### 1. rustycode-protocol Integration

```rust
// crates/rustycode-protocol/src/lib.rs

// Add Event trait
pub use rustycode_bus::Event;

// Implement Event for existing types
impl Event for SessionEvent {
    fn event_type(&self) -> &'static str {
        match self.kind {
            EventKind::SessionStarted => "session.started",
            EventKind::ContextAssembled => "context.assembled",
            EventKind::InspectionCompleted => "inspection.completed",
        }
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.at
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
```

### 2. rustycode-core Integration

```rust
// crates/rustycode-core/src/lib.rs

use rustycode_bus::{EventBus, SessionStartedEvent, ContextAssembledEvent};

pub struct Runtime {
    config: Config,
    storage: Storage,
    bus: Arc<EventBus>,  // Add event bus
}

impl Runtime {
    pub fn load(cwd: &Path) -> Result<Self> {
        let config = Config::load(cwd)?;
        let storage = Storage::open(&config.data_dir.join("rustycode.db"))?;
        let bus = Arc::new(EventBus::default());  // Create bus

        // Register storage hook
        let storage_clone = storage.clone();
        bus.register_hook(HookPhase::PostPublish, move |event| {
            let serialized = event.serialize()?;
            storage_clone.persist_event(&serialized)?;
            Ok(())
        }).await;

        Ok(Self { config, storage, bus })
    }

    pub fn event_bus(&self) -> Arc<EventBus> {
        self.bus.clone()
    }

    pub async fn run(&self, cwd: &Path, task: &str) -> Result<RunReport> {
        let session = Session {
            id: SessionId::new(),
            task: task.to_string(),
            created_at: Utc::now(),
        };

        // Publish event instead of direct storage call
        self.bus.publish(SessionStartedEvent {
            event_id: None,
            timestamp: session.created_at,
            session_id: session.id.clone(),
            task: task.to_string(),
            detail: format!("task={task}"),
        }).await?;

        // ... rest of implementation

        let context_plan = build_context_plan(task, &git, &lsp_servers, &memory, &skills);

        // Publish context assembled event
        self.bus.publish(ContextAssembledEvent {
            event_id: None,
            timestamp: Utc::now(),
            session_id: session.id.clone(),
            context_plan: context_plan.clone(),
            detail: serde_json::to_string(&context_plan)?,
        }).await?;

        Ok(RunReport {
            session,
            git,
            lsp_servers,
            memory,
            skills,
            context_plan,
        })
    }
}
```

### 3. rustycode-git Integration

```rust
// crates/rustycode-git/src/lib.rs

use rustycode_bus::{EventBus, GitStatusChangedEvent};

pub async fn watch_git_status(cwd: &Path, bus: Arc<EventBus>) -> Result<()> {
    let mut last_status = inspect(cwd)?;

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Ok(current_status) = inspect(cwd).await {
                if current_status != last_status {
                    bus.publish(GitStatusChangedEvent {
                        event_id: None,
                        timestamp: Utc::now(),
                        root: current_status.root.clone(),
                        branch: current_status.branch.clone(),
                        dirty: current_status.dirty.unwrap_or(false),
                    }).await.ok();

                    last_status = current_status;
                }
            }
        }
    });

    Ok(())
}
```

### 4. rustycode-lsp Integration

```rust
// crates/rustycode-lsp/src/lib.rs

use rustycode_bus::{EventBus, LspServerDiscoveredEvent};

pub async fn discover_and_publish(
    candidates: &[String],
    bus: Arc<EventBus>
) -> Vec<LspServerStatus> {
    let servers = discover(candidates);

    for server in &servers {
        bus.publish(LspServerDiscoveredEvent {
            event_id: None,
            timestamp: Utc::now(),
            server_name: server.name.clone(),
            installed: server.installed,
            path: server.path.clone(),
        }).await.ok();
    }

    servers
}
```

### 5. rustycode-memory Integration

```rust
// crates/rustycode-memory/src/lib.rs

use rustycode_bus::{EventBus, MemoryEntryAddedEvent};

pub async fn add_entry_with_event(
    entry: &MemoryEntry,
    bus: Arc<EventBus>
) -> Result<()> {
    // Add to storage
    add_entry(entry)?;

    // Publish event
    bus.publish(MemoryEntryAddedEvent {
        event_id: None,
        timestamp: Utc::now(),
        entry_path: entry.path.clone(),
        preview: entry.preview.clone(),
        session_id: None,
    }).await?;

    Ok(())
}
```

## Migration Path

### Phase 1: Add Event Bus (Non-Breaking)

1. Create `rustycode-bus` crate
2. Add `EventBus` to `Runtime` as optional feature
3. Keep existing direct storage calls
4. Shadow publishes: call both storage and event bus

```rust
// Runtime::run with shadow publishes
pub async fn run(&self, cwd: &Path, task: &str) -> Result<RunReport> {
    let session = Session { /* ... */ };

    // Existing direct call
    self.storage.insert_session(&session)?;

    // New: also publish to bus
    if let Some(bus) = &self.bus {
        bus.publish(SessionStartedEvent { /* ... */ }).await.ok();
    }

    // ... rest of implementation
}
```

### Phase 2: Internal Migration

1. Migrate internal crate communications to use event bus
2. Subscribe to events instead of direct calls
3. Keep storage persistence as a subscriber

```rust
// Storage as subscriber
tokio::spawn(async move {
    let mut rx = bus.subscribe("*").await?.1;
    while let Ok(event) = rx.recv().await {
        storage.persist_event(&event.serialize()?)?;
    }
    Ok::<_, EventBusError>(())
});
```

### Phase 3: External API Migration

1. Remove direct storage calls from core runtime
2. All events flow through bus
3. Storage is just another subscriber

```rust
// Final Runtime::run
pub async fn run(&self, cwd: &Path, task: &str) -> Result<RunReport> {
    let session = Session { /* ... */ };

    // Only publish to bus, storage handles it
    self.bus.publish(SessionStartedEvent { /* ... */ }).await?;

    // ... rest of implementation
}
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_bus::{EventBus, SessionStartedEvent};
    use rustycode_protocol::SessionId;

    #[tokio::test]
    async fn test_exact_subscription() {
        let bus = EventBus::default();

        let (id, mut rx) = bus.subscribe("session.started").await.unwrap();

        let event = SessionStartedEvent {
            event_id: None,
            timestamp: Utc::now(),
            session_id: SessionId::new(),
            task: "test".to_string(),
            detail: "test".to_string(),
        };

        bus.publish(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type(), "sessionstartedevent");
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let bus = EventBus::default();

        let (id, mut rx) = bus.subscribe("session.*").await.unwrap();

        bus.publish(SessionStartedEvent { /* ... */ }).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert!(received.event_type().contains("session"));
    }

    #[tokio::test]
    async fn test_hook_execution() {
        let bus = EventBus::default();
        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        bus.register_hook(HookPhase::PrePublish, move |_event| {
            hook_called_clone.store(true, Ordering::SeqCst);
            Ok(())
        }).await;

        bus.publish(SessionStartedEvent { /* ... */ }).await.unwrap();

        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let bus = EventBus::default();

        let (id, mut rx) = bus.subscribe("session.*").await.unwrap();
        bus.unsubscribe(id).await.unwrap();

        bus.publish(SessionStartedEvent { /* ... */ }).await.unwrap();

        // Channel should be closed after unsubscribe
        assert!(rx.recv().await.is_err());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_event_flow() {
    let bus = Arc::new(EventBus::default());

    // Subscribe to session events
    let bus_clone = bus.clone();
    let handle = tokio::spawn(async move {
        let (_id, mut rx) = bus_clone.subscribe("session.*").await.unwrap();
        let mut count = 0;
        while let Ok(event) = rx.recv().await {
            count += 1;
            if count >= 2 {
                break;
            }
        }
        count
    });

    // Publish events
    bus.publish(SessionStartedEvent { /* ... */ }).await.unwrap();
    bus.publish(ContextAssembledEvent { /* ... */ }).await.unwrap();

    let count = handle.await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_concurrent_subscribers() {
    let bus = Arc::new(EventBus::default());

    let mut handles = vec![];

    for i in 0..10 {
        let bus_clone = bus.clone();
        let handle = tokio::spawn(async move {
            let (_id, mut rx) = bus_clone.subscribe("*").await.unwrap();
            let mut count = 0;
            while let Ok(_) = rx.recv().await {
                count += 1;
                if count >= 5 {
                    break;
                }
            }
            count
        });
        handles.push(handle);
    }

    // Publish events
    for _ in 0..5 {
        bus.publish(SessionStartedEvent { /* ... */ }).await.unwrap();
    }

    // All subscribers should receive all events
    for handle in handles {
        let count = handle.await.unwrap();
        assert_eq!(count, 5);
    }
}
```

### Performance Tests

```rust
#[tokio::test]
async fn bench_high_throughput() {
    let bus = Arc::new(EventBus::default());
    let (_id, _rx) = bus.subscribe("*").await.unwrap();

    let start = Instant::now();
    for i in 0..10_000 {
        bus.publish(SessionStartedEvent { /* ... */ }).await.unwrap();
    }
    let duration = start.elapsed();

    println!("Published 10,000 events in {:?}", duration);
    assert!(duration.as_millis() < 1000, "Too slow");
}
```

## Example Usage Patterns

### Pattern 1: Cross-Cutting Logging

```rust
// Register logging hook
bus.register_hook(HookPhase::PostPublish, |event| {
    tracing::info!(
        event_type = event.event_type(),
        timestamp = %event.timestamp(),
        "Event published"
    );
    Ok(())
}).await;
```

### Pattern 2: Metrics Collection

```rust
use prometheus::{Counter, IntCounter};

lazy_static! {
    static ref EVENT_COUNTER: IntCounter = register_int_counter!(
        "rustycode_events_total",
        "Total events published"
    ).unwrap();
}

bus.register_hook(HookPhase::PostPublish, |event| {
    EVENT_COUNTER.inc();
    Ok(())
}).await;
```

### Pattern 3: Error Aggregation

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

struct ErrorCollector {
    errors: Arc<Mutex<Vec<ErrorEvent>>>,
}

impl ErrorCollector {
    fn new(bus: &EventBus) -> Self {
        let errors = Arc::default();
        let errors_clone = errors.clone();

        tokio::spawn(async move {
            let (_id, mut rx) = bus.subscribe("*.error").await.unwrap();
            while let Ok(event) = rx.recv().await {
                if let Some(error_event) = event.as_any().downcast_ref::<ErrorEvent>() {
                    errors_clone.lock().await.push(error_event.clone());
                }
            }
        });

        Self { errors }
    }

    async fn get_errors(&self) -> Vec<ErrorEvent> {
        self.errors.lock().await.clone()
    }
}
```

### Pattern 4: Event Replay

```rust
impl EventBus {
    pub async fn replay_from_storage(&self, storage: &Storage) -> Result<()> {
        let events = storage.load_all_events()?;

        for serialized in events {
            let event: Box<dyn Event> = deserialize_event(&serialized)?;
            self.publish(event).await?;
        }

        Ok(())
    }
}
```

### Pattern 5: Circuit Breaker

```rust
struct CircuitBreaker {
    failure_count: Arc<AtomicU32>,
    threshold: u32,
}

impl CircuitBreaker {
    fn new(threshold: u32) -> Self {
        Self {
            failure_count: Arc::new(AtomicU32::new(0)),
            threshold,
        }
    }

    fn register(&self, bus: &EventBus) {
        let count = self.failure_count.clone();
        bus.register_hook(HookPhase::OnError, move |event| {
            if let Some(error_event) = event.as_any().downcast_ref::<ErrorEvent>() {
                count.fetch_add(1, Ordering::SeqCst);
                if count.load(Ordering::SeqCst) >= self.threshold {
                    tracing::error!("Circuit breaker opened!");
                }
            }
            Ok(())
        }).await;
    }
}
```

## Future Enhancements

### 1. Event Persistence

```rust
pub trait EventStore: Send + Sync {
    async fn append(&self, event: &dyn Event) -> Result<(), Error>;
    async fn replay(&self, since: DateTime<Utc>) -> Result<Vec<Box<dyn Event>>, Error>;
}
```

### 2. Distributed Event Bus

```rust
pub struct DistributedEventBus {
    local: EventBus,
    network: Option<NetworkTransport>,
}

impl DistributedEventBus {
    pub async fn publish_to_cluster<T>(&self, event: T) -> Result<(), Error>
    where
        T: Event + Clone,
    {
        // Publish locally
        self.local.publish(event.clone()).await?;

        // Publish to cluster
        if let Some(network) = &self.network {
            network.broadcast(event).await?;
        }

        Ok(())
    }
}
```

### 3. Event Sourcing

```rust
pub struct EventSourced<T> {
    state: T,
    events: Vec<Box<dyn Event>>,
}

impl<T> EventSourced<T> {
    pub fn apply(&mut self, event: Box<dyn Event>) -> Result<(), Error> {
        // Apply event to state
        self.events.push(event);
        Ok(())
    }

    pub fn replay(&self) -> impl Iterator<Item = &dyn Event> {
        self.events.iter().map(|e| e.as_ref())
    }
}
```

## Dependencies

### New Dependencies

```toml
# crates/rustycode-bus/Cargo.toml

[dependencies]
tokio = { version = "1", features = ["sync", "rt"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
regex = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "test-util"] }
```

### Updated Dependencies

```toml
# crates/rustycode-protocol/Cargo.toml

[dependencies]
rustycode-bus = { path = "../rustycode-bus" }
```

## Performance Considerations

1. **Channel Capacity**: Default 100 messages per subscriber (configurable)
2. **Cloning overhead**: Events are cloned for each subscriber (use Arc for large payloads)
3. **Wildcard matching**: Regex compilation cached in subscription filter
4. **Hook execution**: Run sequentially (consider async hooks for I/O)
5. **Metrics**: Atomic operations for lock-free reads

## Security Considerations

1. **Event validation**: Validate event data before publishing
2. **Subscriber isolation**: Each subscriber gets independent channel
3. **Resource limits**: Max subscribers, channel capacity caps
4. **Error handling**: Hooks should not panic (use catch_unwind)

## Migration Checklist

- [ ] Create `rustycode-bus` crate
- [ ] Implement `Event` trait and core types
- [ ] Implement `EventBus` with pub/sub
- [ ] Add wildcard subscription support
- [ ] Implement hook system
- [ ] Add comprehensive tests
- [ ] Add `EventBus` to `Runtime` (Phase 1: shadow mode)
- [ ] Migrate `rustycode-git` to publish events
- [ ] Migrate `rustycode-lsp` to publish events
- [ ] Migrate `rustycode-memory` to publish events
- [ ] Migrate `rustycode-skill` to publish events
- [ ] Update storage to subscribe instead of direct calls
- [ ] Remove shadow mode (Phase 3: full migration)
- [ ] Update documentation
- [ ] Add examples

## References

- [Tokio Broadcast Channels](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html)
- [Event-Driven Architecture](https://en.wikipedia.org/wiki/Event-driven_architecture)
- [Rust Async Book](https://rust-lang.github.io/async-book/)
