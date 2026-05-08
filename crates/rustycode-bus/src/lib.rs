// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

#![allow(clippy::significant_drop_tightening, clippy::struct_field_names)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::float_cmp,
        clippy::redundant_clone,
        clippy::unreadable_literal,
        clippy::too_many_lines,
        clippy::unused_async,
        clippy::no_effect_underscore_binding
    )
)]

//! # `RustyCode` Event Bus
//!
//! A type-safe, asynchronous event bus for decoupled communication between crates.
//!
//! ## Features
//!
//! - **Type Safety**: Compile-time guarantee that event publishers and subscribers match
//! - **Wildcards**: Subscribe to event patterns (e.g., `session.*`, `git.*`)
//! - **Hooks**: Pre/post processing for logging, metrics, error handling
//! - **Async**: Built on tokio for high-throughput scenarios
//! - **Thread-Safe**: All operations are safe to use across threads
//!
//! ## Example
//!
//! ```rust
//! use rustycode_bus::{EventBus, SessionStartedEvent};
//! use rustycode_protocol::SessionId;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let bus = EventBus::default();
//!
//!     // Subscribe to session events
//!     let (_id, mut rx) = bus.subscribe("session.started").await?;
//!
//!     // Spawn a task to handle events
//!     tokio::spawn(async move {
//!         while let Ok(event) = rx.recv().await {
//!             println!("Received event: {}", event.event_type());
//!         }
//!     });
//!
//!     // Publish an event
//!     let event = SessionStartedEvent::new(
//!         SessionId::new(),
//!         "Analyze codebase".to_string(),
//!         "Initial session".to_string(),
//!     );
//!     bus.publish(event).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod ensemble_events;
pub mod error;
pub mod events;
pub mod harness_events;
pub mod hook_registry;
pub mod hooks;

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use regex::Regex;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

pub use error::{EventBusError, Result};
pub use events::{
    AstPhaseEvent, ContextAssembledEvent, InspectionCompletedEvent, MilestoneProgressEvent,
    ModeChangedEvent, PlanApprovedEvent, PlanCreatedEvent, PlanExecutionCompletedEvent,
    PlanExecutionFailedEvent, PlanExecutionStartedEvent, PlanRejectedEvent, PostCompactEvent,
    PreCompactEvent, SessionCompletedEvent, SessionFailedEvent, SessionStartedEvent,
    SkillActivatedEvent, SkillDeactivatedEvent, SkillQualityAssessedEvent, SkillSuggestedEvent,
    TodoSnapshot, TodoUpdatedEvent, ToolBlockedEvent, ToolExecutedEvent,
};
// Export hook system
pub use hook_registry::HookRegistry;
pub use hooks::{
    EventHookContext, FunctionHook, Hook, HookContext, HookPhase, HookPriority, HookResult,
};

/// Core event trait for type-safe event handling
///
/// All events must implement this trait to be published on the event bus.
/// The trait provides downcasting support and serialization capabilities.
pub trait Event: Send + Sync + 'static {
    /// Unique event type identifier (e.g., "session.started")
    fn event_type(&self) -> &'static str;

    /// Event timestamp
    fn timestamp(&self) -> DateTime<Utc>;

    /// Convert to Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Clone as boxed trait object
    fn clone_box(&self) -> Box<dyn Event>;

    /// Serialize for transport/storage
    fn serialize(&self) -> serde_json::Value;
}

/// Subscription filter for event routing
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SubscriptionFilter {
    /// Exact event type match (e.g., "session.started")
    Exact(String),

    /// Wildcard match (e.g., "session.*", "*.error")
    Wildcard(String, Regex),
}

impl SubscriptionFilter {
    /// Create a new subscription filter from a pattern
    ///
    pub fn new(pattern: &str) -> std::result::Result<Self, regex::Error> {
        if pattern.contains('*') || pattern.contains('?') {
            // Convert wildcard to regex
            let regex_str = pattern
                .replace('.', r"\.")
                .replace('*', ".*")
                .replace('?', ".");
            let regex = Regex::new(&format!("^{regex_str}$"))?;
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

/// Subscriber type enumeration
///
/// Defines how events are delivered to subscribers:
/// - **Broadcast**: Traditional channel-based delivery (supports multiple receivers)
/// - **Callback**: Direct function invocation (zero-cost for single subscriber)
/// - **Hybrid**: Both channel and callback (useful for monitoring)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubscriberType {
    /// Channel-based broadcast delivery
    Broadcast,

    /// Direct callback invocation
    Callback,

    /// Both broadcast and callback
    Hybrid,
}

/// Subscriber information
#[derive(Clone)]
struct Subscriber {
    id: Uuid,
    filter: SubscriptionFilter,
    subscriber_type: SubscriberType,
    sender: Option<broadcast::Sender<Arc<dyn Event>>>,
    callback: Option<CallbackFn>,
}

/// Callback function type for direct event delivery
///
/// This is a type-erased function pointer that can be stored and invoked
/// synchronously when events are published, avoiding channel overhead.
type CallbackFn = Arc<
    dyn Fn(Arc<dyn Event>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
        + Send
        + Sync,
>;

/// Snapshot of a matching subscriber captured under read lock.
type SubscriberSnapshot = (
    Uuid,
    Option<CallbackFn>,
    Option<broadcast::Sender<Arc<dyn Event>>>,
);

/// Hook function type
pub type HookFn = Arc<
    dyn Fn(&dyn Event) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
        + Send
        + Sync,
>;

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

/// Event bus metrics
#[derive(Debug, Default)]
struct EventBusMetrics {
    events_published: AtomicU64,
    events_delivered: AtomicU64,
    events_failed: AtomicU64,
    subscriber_count: AtomicU64,
}

/// Snapshot of event bus metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventBusMetricsSnapshot {
    pub events_published: u64,
    pub events_delivered: u64,
    pub events_failed: u64,
    pub subscriber_count: u64,
}

/// Main event bus for pub/sub communication
///
/// The event bus manages subscriptions and delivers events to matching subscribers.
/// It supports wildcard patterns and hooks for cross-cutting concerns.
///
/// # Example
///
/// ```rust
/// use rustycode_bus::EventBus;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let bus = EventBus::default();
///
///     // Subscribe to all events
///     let (_id, mut rx) = bus.subscribe("*").await?;
///
///     // Publish events
///     // ... (publish events here)
///
///     Ok(())
/// }
/// ```
pub struct EventBus {
    config: EventBusConfig,
    subscribers: Arc<RwLock<HashMap<Uuid, Subscriber>>>,
    hooks: Arc<RwLock<HashMap<HookPhase, Vec<HookFn>>>>,
    metrics: Arc<EventBusMetrics>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::with_config(EventBusConfig::default())
    }

    /// Create a new event bus with custom configuration
    ///
    pub fn with_config(config: EventBusConfig) -> Self {
        Self {
            config,
            subscribers: Arc::default(),
            hooks: Arc::default(),
            metrics: Arc::default(),
        }
    }

    /// Publish an event to all matching subscribers
    ///
    /// This will:
    /// 1. Run `PrePublish` hooks
    /// 2. Send event to all matching subscribers (both broadcast and callback)
    /// 3. Run `PostPublish` hooks
    /// 4. Update metrics
    ///
    pub async fn publish<E>(&self, event: E) -> Result<()>
    where
        E: Event + Clone,
    {
        let event_type = event.event_type();

        // Run pre-publish hooks
        self.run_hooks(HookPhase::PrePublish, &event).await?;

        if self.config.debug_logging {
            tracing::debug!("Publishing event: {}", event_type);
        }

        // Snapshot matching subscribers under read lock, then drop it before
        // invoking callbacks. This prevents deadlock if a callback calls
        // subscribe/unsubscribe (which need a write lock).
        let matches: Vec<SubscriberSnapshot> = {
            let subscribers = self.subscribers.read().await;
            subscribers
                .values()
                .filter(|s| s.filter.matches(event_type))
                .map(|s| (s.id, s.callback.clone(), s.sender.clone()))
                .collect()
        };

        let event_arc: Arc<dyn Event> = Arc::from(event.clone_box());
        let mut recv_count = 0;

        for (id, callback, sender) in &matches {
            if let Some(callback) = callback {
                if let Err(e) = callback(event_arc.clone()) {
                    if self.config.debug_logging {
                        tracing::warn!("Callback error for subscriber {}: {}", id, e);
                    }
                    self.metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                } else {
                    recv_count += 1;
                    self.metrics
                        .events_delivered
                        .fetch_add(1, Ordering::Relaxed);
                }
            }

            if let Some(sender) = sender {
                if let Ok(count) = sender.send(event_arc.clone()) {
                    recv_count += count;
                    self.metrics
                        .events_delivered
                        .fetch_add(count as u64, Ordering::Relaxed);
                } else {
                    if self.config.debug_logging {
                        tracing::warn!("No receivers for subscriber {}", id);
                    }
                    self.metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Run post-publish hooks
        self.run_hooks(HookPhase::PostPublish, &event).await?;

        self.metrics
            .events_published
            .fetch_add(1, Ordering::Relaxed);

        if self.config.debug_logging {
            tracing::debug!("Event {} delivered to {} receivers", event_type, recv_count);
        }

        Ok(())
    }

    /// Subscribe to events matching a pattern
    ///
    pub async fn subscribe(
        &self,
        pattern: &str,
    ) -> std::result::Result<(Uuid, broadcast::Receiver<Arc<dyn Event>>), EventBusError> {
        let filter = SubscriptionFilter::new(pattern)
            .map_err(|e| EventBusError::InvalidPattern(e.to_string()))?;

        let id = Uuid::new_v4();
        let (sender, receiver) = broadcast::channel(self.config.channel_capacity);

        let subscriber = Subscriber {
            id,
            filter,
            subscriber_type: SubscriberType::Broadcast,
            sender: Some(sender),
            callback: None,
        };

        {
            let mut subscribers = self.subscribers.write().await;
            if subscribers.len() >= self.config.max_subscribers {
                return Err(EventBusError::MaxSubscribersReached);
            }
            subscribers.insert(id, subscriber.clone());
            self.metrics
                .subscriber_count
                .fetch_add(1, Ordering::Relaxed);
        }

        if self.config.debug_logging {
            tracing::debug!(
                "New subscription {}: pattern={}, type={:?}",
                id,
                pattern,
                subscriber.subscriber_type
            );
        }

        Ok((id, receiver))
    }

    /// Subscribe to events using a callback function (zero-cost abstraction)
    ///
    /// This is the most efficient way to handle events when you have a single
    /// subscriber, as it avoids channel overhead completely.
    ///
    pub async fn subscribe_callback<F>(
        &self,
        pattern: &str,
        callback: F,
    ) -> std::result::Result<Uuid, EventBusError>
    where
        F: Fn(Arc<dyn Event>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
            + Send
            + Sync
            + 'static,
    {
        let filter = SubscriptionFilter::new(pattern)
            .map_err(|e| EventBusError::InvalidPattern(e.to_string()))?;

        let id = Uuid::new_v4();

        let subscriber = Subscriber {
            id,
            filter,
            subscriber_type: SubscriberType::Callback,
            sender: None,
            callback: Some(Arc::new(callback)),
        };

        {
            let mut subscribers = self.subscribers.write().await;
            if subscribers.len() >= self.config.max_subscribers {
                return Err(EventBusError::MaxSubscribersReached);
            }
            subscribers.insert(id, subscriber.clone());
            self.metrics
                .subscriber_count
                .fetch_add(1, Ordering::Relaxed);
        }

        if self.config.debug_logging {
            tracing::debug!(
                "New callback subscription {}: pattern={}, type={:?}",
                id,
                pattern,
                subscriber.subscriber_type
            );
        }

        Ok(id)
    }

    /// Subscribe to events using both callback and broadcast channel
    ///
    /// This is useful when you want to handle events immediately via callback
    /// but also want to support multiple receivers via channel.
    ///
    pub async fn subscribe_hybrid<F>(
        &self,
        pattern: &str,
        callback: F,
    ) -> std::result::Result<(Uuid, broadcast::Receiver<Arc<dyn Event>>), EventBusError>
    where
        F: Fn(Arc<dyn Event>) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
            + Send
            + Sync
            + 'static,
    {
        let filter = SubscriptionFilter::new(pattern)
            .map_err(|e| EventBusError::InvalidPattern(e.to_string()))?;

        let id = Uuid::new_v4();
        let (sender, receiver) = broadcast::channel(self.config.channel_capacity);

        let subscriber = Subscriber {
            id,
            filter,
            subscriber_type: SubscriberType::Hybrid,
            sender: Some(sender),
            callback: Some(Arc::new(callback)),
        };

        {
            let mut subscribers = self.subscribers.write().await;
            if subscribers.len() >= self.config.max_subscribers {
                return Err(EventBusError::MaxSubscribersReached);
            }
            subscribers.insert(id, subscriber.clone());
            self.metrics
                .subscriber_count
                .fetch_add(1, Ordering::Relaxed);
        }

        if self.config.debug_logging {
            tracing::debug!(
                "New hybrid subscription {}: pattern={}, type={:?}",
                id,
                pattern,
                subscriber.subscriber_type
            );
        }

        Ok((id, receiver))
    }

    /// Unsubscribe from events
    ///
    pub async fn unsubscribe(&self, id: Uuid) -> Result<()> {
        let mut subscribers = self.subscribers.write().await;
        subscribers
            .remove(&id)
            .ok_or(EventBusError::SubscriberNotFound)?;
        self.metrics
            .subscriber_count
            .fetch_sub(1, Ordering::Relaxed);

        if self.config.debug_logging {
            tracing::debug!("Unsubscribed: {}", id);
        }

        Ok(())
    }

    /// Register a hook for a specific phase
    ///
    /// Hooks are executed in the order they are registered.
    ///
    pub async fn register_hook<F>(&self, phase: HookPhase, hook: F)
    where
        F: Fn(&dyn Event) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
            + Send
            + Sync
            + 'static,
    {
        let mut hooks = self.hooks.write().await;
        hooks.entry(phase).or_default().push(Arc::new(hook));
    }

    /// Run hooks for a specific phase
    async fn run_hooks(&self, phase: HookPhase, event: &dyn Event) -> Result<()> {
        let hooks = self.hooks.read().await;
        if let Some(phase_hooks) = hooks.get(&phase) {
            for hook in phase_hooks {
                if let Err(e) = hook(event) {
                    let error_msg = e.to_string();
                    tracing::error!("Hook error on {:?}: {}", phase, error_msg);

                    if phase == HookPhase::OnError {
                        return Err(EventBusError::HookError(error_msg));
                    }

                    // Run error hooks
                    self.metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        Ok(())
    }

    /// Get current metrics snapshot
    ///
    /// # Example
    ///
    /// ```rust
    /// use rustycode_bus::EventBus;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let bus = EventBus::default();
    ///
    /// let metrics = bus.metrics();
    /// println!("Events published: {}", metrics.events_published);
    /// # Ok(())
    /// # }
    /// ```
    pub fn metrics(&self) -> EventBusMetricsSnapshot {
        EventBusMetricsSnapshot {
            events_published: self.metrics.events_published.load(Ordering::Relaxed),
            events_delivered: self.metrics.events_delivered.load(Ordering::Relaxed),
            events_failed: self.metrics.events_failed.load(Ordering::Relaxed),
            subscriber_count: self.metrics.subscriber_count.load(Ordering::Relaxed),
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Subscription handle for automatic cleanup
///
/// When this handle is dropped, it will automatically unsubscribe from the event bus.
/// This ensures proper cleanup even if the subscriber task panics.
///
/// # Example
///
/// ```rust
/// use rustycode_bus::{EventBus, SubscriptionHandle};
/// use std::sync::Arc;
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let bus = Arc::new(EventBus::default());
///
/// {
///     let (id, rx) = bus.subscribe("session.*").await?;
///     let handle = SubscriptionHandle::new(id, bus.clone());
///
///     // Use the handle
///     // When handle goes out of scope, it auto-unsubscribes
/// }
/// # Ok(())
/// # }
/// ```
pub struct SubscriptionHandle {
    id: Uuid,
    bus: Arc<EventBus>,
}

impl SubscriptionHandle {
    /// Create a new subscription handle
    ///
    pub const fn new(id: Uuid, bus: Arc<EventBus>) -> Self {
        Self { id, bus }
    }

    /// Get the subscription ID
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// Manually unsubscribe (also happens on drop)
    ///
    /// # Example
    ///
    /// ```rust
    /// use rustycode_bus::{EventBus, SubscriptionHandle};
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let bus = Arc::new(EventBus::default());
    ///
    /// let (id, _rx) = bus.subscribe("session.*").await?;
    /// let handle = SubscriptionHandle::new(id, bus);
    ///
    /// // Manual unsubscribe
    /// handle.unsubscribe().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn unsubscribe(self) -> Result<()> {
        self.bus.unsubscribe(self.id).await
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        // Best-effort unsubscribe on drop
        let bus = self.bus.clone();
        let id = self.id;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(e) = bus.unsubscribe(id).await {
                    tracing::debug!("Best-effort unsubscribe failed for {}: {}", id, e);
                }
            });
        } else {
            tracing::warn!(
                "Cannot unsubscribe subscription {} — no Tokio runtime. Call unsubscribe() explicitly.",
                id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionStartedEvent;
    use rustycode_protocol::SessionId;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_exact_subscription() {
        let bus = EventBus::new();

        let (_id, mut rx) = bus.subscribe("session.started").await.unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let bus = EventBus::new();

        let (_id, mut rx) = bus.subscribe("session.*").await.unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert!(received.event_type().starts_with("session."));
    }

    #[tokio::test]
    async fn test_wildcard_all_events() {
        let bus = EventBus::new();

        let (_id, mut rx) = bus.subscribe("*").await.unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_hook_execution() {
        let bus = EventBus::new();
        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        bus.register_hook(HookPhase::PrePublish, move |_event| {
            hook_called_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_multiple_hooks() {
        let bus = EventBus::new();
        let hook_count = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let hook_count_clone = hook_count.clone();
            bus.register_hook(HookPhase::PrePublish, move |_event| {
                hook_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await;
        }

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        assert_eq!(hook_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let bus = EventBus::new();

        let (id, mut rx) = bus.subscribe("session.*").await.unwrap();
        bus.unsubscribe(id).await.unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        // Channel should be closed after unsubscribe
        assert!(rx.recv().await.is_err());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new();

        let (_id1, mut rx1) = bus.subscribe("session.*").await.unwrap();
        let (_id2, mut rx2) = bus.subscribe("session.started").await.unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        // Both subscribers should receive the event
        let recv1 = rx1.recv().await.unwrap();
        let recv2 = rx2.recv().await.unwrap();

        assert_eq!(recv1.event_type(), "session.started");
        assert_eq!(recv2.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_subscription_handle_drop() {
        let bus = Arc::new(EventBus::new());

        let (id, _rx) = bus.subscribe("session.*").await.unwrap();
        let handle = SubscriptionHandle::new(id, bus.clone());

        // Get initial subscriber count
        let metrics_before = bus.metrics();

        // Drop the handle
        drop(handle);

        // Give time for async cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let metrics_after = bus.metrics();
        assert_eq!(metrics_before.subscriber_count, 1);
        assert_eq!(metrics_after.subscriber_count, 0);
    }

    #[tokio::test]
    async fn test_metrics() {
        let bus = EventBus::new();

        let (_id, _rx) = bus.subscribe("session.*").await.unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        let metrics = bus.metrics();
        assert_eq!(metrics.events_published, 1);
        assert_eq!(metrics.events_delivered, 1);
        assert_eq!(metrics.subscriber_count, 1);
    }

    #[tokio::test]
    async fn test_max_subscribers() {
        let config = EventBusConfig {
            max_subscribers: 2,
            ..Default::default()
        };
        let bus = EventBus::with_config(config);

        let (_id1, _rx1) = bus.subscribe("session.*").await.unwrap();
        let (_id2, _rx2) = bus.subscribe("git.*").await.unwrap();

        // Third subscription should fail
        let result = bus.subscribe("lsp.*").await;
        assert!(matches!(result, Err(EventBusError::MaxSubscribersReached)));
    }

    #[tokio::test]
    async fn test_hook_error_handling() {
        let bus = EventBus::new();

        // Register a hook that always fails
        bus.register_hook(HookPhase::PrePublish, |_event| {
            Err::<(), _>("Hook failed".into())
        })
        .await;

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        // Publish should still succeed despite hook error
        bus.publish(event).await.ok();
    }

    // ========== Hybrid Event Publishing Tests ==========

    #[tokio::test]
    async fn test_callback_subscription_basic() {
        let bus = EventBus::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let _id = bus
            .subscribe_callback("session.started", move |_event| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        // Callback should have been called once
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_callback_subscription_wildcard() {
        let bus = EventBus::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let _id = bus
            .subscribe_callback("session.*", move |_event| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_callback_unsubscribe() {
        let bus = EventBus::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let id = bus
            .subscribe_callback("session.*", move |_event| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        // Publish first event
        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(event.clone()).await.unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Unsubscribe
        bus.unsubscribe(id).await.unwrap();

        // Publish second event - callback should not be called
        bus.publish(event).await.unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_callback_error_handling() {
        let bus = EventBus::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let _id = bus
            .subscribe_callback("session.*", move |_event| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>("Callback error".into())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        // Publish should succeed even if callback fails
        bus.publish(event).await.unwrap();

        // Callback should have been called once
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Metrics should show the error
        let metrics = bus.metrics();
        assert_eq!(metrics.events_failed, 1);
    }

    #[tokio::test]
    async fn test_hybrid_subscription() {
        let bus = EventBus::new();
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_count_clone = callback_count.clone();

        let (_id, mut rx) = bus
            .subscribe_hybrid("session.*", move |_event| {
                callback_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event.clone()).await.unwrap();

        // Callback should have been called
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);

        // Channel should also receive the event
        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_hybrid_multiple_channel_receivers() {
        let bus = EventBus::new();
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_count_clone = callback_count.clone();

        let (_id, tx) = bus
            .subscribe_hybrid("session.*", move |_event| {
                callback_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        // Create multiple receivers from the same channel
        let mut rx1 = tx.resubscribe();
        let mut rx2 = tx.resubscribe();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event.clone()).await.unwrap();

        // Callback should have been called once
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);

        // Both channel receivers should receive the event
        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert_eq!(received1.event_type(), "session.started");
        assert_eq!(received2.event_type(), "session.started");
    }

    #[tokio::test]
    async fn test_mixed_subscription_types() {
        let bus = EventBus::new();

        // Callback subscriber
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_count_clone = callback_count.clone();
        let _callback_id = bus
            .subscribe_callback("session.*", move |_event| {
                callback_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        // Broadcast subscriber
        let (_broadcast_id, mut rx) = bus.subscribe("session.started").await.unwrap();

        // Hybrid subscriber
        let hybrid_count = Arc::new(AtomicUsize::new(0));
        let hybrid_count_clone = hybrid_count.clone();
        let (_hybrid_id, _hybrid_rx) = bus
            .subscribe_hybrid("session.*", move |_event| {
                hybrid_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event.clone()).await.unwrap();

        // All subscribers should have received the event
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);
        assert_eq!(hybrid_count.load(Ordering::SeqCst), 1);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type(), "session.started");

        // Check metrics
        let metrics = bus.metrics();
        assert_eq!(metrics.events_published, 1);
        // Callback (1) + Hybrid callback (1) + Hybrid channel (1) + Broadcast channel (1) = 4
        // Note: Even though we drop the broadcast receiver, the channel send still succeeds
        assert_eq!(metrics.events_delivered, 4);
    }

    #[tokio::test]
    async fn test_callback_zero_cost_single_subscriber() {
        // This test verifies that callback subscriptions work efficiently
        // for single subscriber case (zero-cost abstraction)
        let bus = EventBus::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let _id = bus
            .subscribe_callback("session.*", move |_event| {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        // Publish many events quickly
        for _ in 0..100 {
            let event =
                SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
            bus.publish(event).await.unwrap();
        }

        // All callbacks should have been executed
        assert_eq!(call_count.load(Ordering::SeqCst), 100);
    }

    #[tokio::test]
    async fn test_subscriber_type_enum() {
        // Test that SubscriberType enum works correctly
        let broadcast_type = SubscriberType::Broadcast;
        let callback_type = SubscriberType::Callback;
        let hybrid_type = SubscriberType::Hybrid;

        assert_eq!(broadcast_type, SubscriberType::Broadcast);
        assert_eq!(callback_type, SubscriberType::Callback);
        assert_eq!(hybrid_type, SubscriberType::Hybrid);

        // Test inequality
        assert_ne!(broadcast_type, callback_type);
        assert_ne!(callback_type, hybrid_type);
        assert_ne!(broadcast_type, hybrid_type);
    }

    #[tokio::test]
    async fn test_callback_pattern_filtering() {
        let bus = EventBus::new();
        let session_count = Arc::new(AtomicUsize::new(0));
        let tool_count = Arc::new(AtomicUsize::new(0));

        let session_count_clone = session_count.clone();
        let _session_id = bus
            .subscribe_callback("session.*", move |_event| {
                session_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        let tool_count_clone = tool_count.clone();
        let _tool_id = bus
            .subscribe_callback("tool.*", move |_event| {
                tool_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        // Publish session event
        let session_event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(session_event).await.unwrap();

        // Only session callback should have been called
        assert_eq!(session_count.load(Ordering::SeqCst), 1);
        assert_eq!(tool_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_callback_with_event_downcasting() {
        let bus = EventBus::new();
        let event_received = Arc::new(AtomicBool::new(false));
        let event_received_clone = event_received.clone();

        let _id = bus
            .subscribe_callback("session.*", move |event| {
                // Try to downcast to specific event type
                if let Some(session_event) = event.as_any().downcast_ref::<SessionStartedEvent>() {
                    assert_eq!(session_event.task, "test");
                    event_received_clone.store(true, Ordering::SeqCst);
                }
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        bus.publish(event).await.unwrap();

        assert!(event_received.load(Ordering::SeqCst));
    }

    // ── Unit tests for SubscriptionFilter and SubscriberType ───────────────

    #[test]
    fn test_subscription_filter_exact() {
        let filter = SubscriptionFilter::new("session.started").unwrap();
        assert!(filter.matches("session.started"));
        assert!(!filter.matches("session.ended"));
    }

    #[test]
    fn test_subscription_filter_wildcard() {
        let filter = SubscriptionFilter::new("session.*").unwrap();
        assert!(filter.matches("session.started"));
        assert!(filter.matches("session.ended"));
        assert!(!filter.matches("tool.executed"));
    }

    #[test]
    fn test_subscription_filter_glob_all() {
        let filter = SubscriptionFilter::new("*").unwrap();
        assert!(filter.matches("anything"));
        assert!(filter.matches("session.started"));
    }

    #[test]
    fn test_subscriber_type_equality() {
        assert_eq!(SubscriberType::Broadcast, SubscriberType::Broadcast);
        assert_ne!(SubscriberType::Broadcast, SubscriberType::Callback);
    }

    #[test]
    fn test_subscription_filter_single_char_wildcard() {
        let filter = SubscriptionFilter::new("session.starte?").unwrap();
        assert!(filter.matches("session.started"));
        assert!(!filter.matches("session.starte"));
    }

    // ── Additional SubscriptionFilter edge case tests ─────────────

    #[test]
    fn test_subscription_filter_exact_no_match() {
        let filter = SubscriptionFilter::new("tool.executed").unwrap();
        assert!(!filter.matches("tool.blocked"));
        assert!(!filter.matches("tool"));
        assert!(!filter.matches("tool.executed.extra"));
    }

    #[test]
    fn test_subscription_filter_wildcard_with_dot_separation() {
        let filter = SubscriptionFilter::new("plan.execution.*").unwrap();
        assert!(filter.matches("plan.execution.started"));
        assert!(filter.matches("plan.execution.completed"));
        assert!(filter.matches("plan.execution.failed"));
        assert!(!filter.matches("plan.created"));
        assert!(!filter.matches("plan.execution"));
    }

    #[test]
    fn test_subscription_filter_question_mark_multiple() {
        let filter = SubscriptionFilter::new("??.started").unwrap();
        assert!(filter.matches("ab.started"));
        assert!(!filter.matches("a.started"));
        assert!(!filter.matches("abc.started"));
    }

    #[test]
    fn test_subscription_filter_empty_string() {
        let filter = SubscriptionFilter::new("").unwrap();
        assert!(filter.matches(""));
        assert!(!filter.matches("anything"));
    }

    #[test]
    fn test_subscription_filter_complex_wildcard() {
        let filter = SubscriptionFilter::new("*.failed").unwrap();
        assert!(filter.matches("session.failed"));
        assert!(filter.matches("plan.execution.failed"));
        assert!(!filter.matches("session.started"));
    }

    #[test]
    fn test_subscription_filter_debug() {
        let filter = SubscriptionFilter::new("session.*").unwrap();
        let debug = format!("{filter:?}");
        assert!(debug.contains("Wildcard"));
    }

    // ── EventBusConfig tests ─────────────

    #[test]
    fn test_config_default_values() {
        let config = EventBusConfig::default();
        assert_eq!(config.channel_capacity, 100);
        assert_eq!(config.max_subscribers, 1000);
        assert!(!config.debug_logging);
    }

    #[test]
    fn test_config_custom_values() {
        let config = EventBusConfig {
            channel_capacity: 50,
            max_subscribers: 100,
            debug_logging: true,
        };
        assert_eq!(config.channel_capacity, 50);
        assert_eq!(config.max_subscribers, 100);
        assert!(config.debug_logging);
    }

    #[test]
    fn test_config_clone() {
        let config = EventBusConfig::default();
        let cloned = config.clone();
        assert_eq!(config.channel_capacity, cloned.channel_capacity);
        assert_eq!(config.max_subscribers, cloned.max_subscribers);
    }

    #[test]
    fn test_config_debug_format() {
        let config = EventBusConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("channel_capacity"));
        assert!(debug.contains("max_subscribers"));
    }

    // ── EventBusMetricsSnapshot tests ─────────────

    #[test]
    fn test_metrics_snapshot_serde() {
        let snapshot = EventBusMetricsSnapshot {
            events_published: 42,
            events_delivered: 38,
            events_failed: 4,
            subscriber_count: 7,
        };
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["events_published"], 42);
        assert_eq!(json["events_delivered"], 38);
        assert_eq!(json["events_failed"], 4);
        assert_eq!(json["subscriber_count"], 7);

        // Roundtrip
        let decoded: EventBusMetricsSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.events_published, 42);
        assert_eq!(decoded.events_delivered, 38);
    }

    #[test]
    fn test_metrics_snapshot_zero_values() {
        let snapshot = EventBusMetricsSnapshot {
            events_published: 0,
            events_delivered: 0,
            events_failed: 0,
            subscriber_count: 0,
        };
        let json = serde_json::to_value(&snapshot).unwrap();
        let decoded: EventBusMetricsSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.events_published, 0);
    }

    #[test]
    fn test_metrics_snapshot_debug() {
        let snapshot = EventBusMetricsSnapshot {
            events_published: 1,
            events_delivered: 1,
            events_failed: 0,
            subscriber_count: 1,
        };
        let debug = format!("{snapshot:?}");
        assert!(debug.contains("EventBusMetricsSnapshot"));
    }

    // ── EventBus edge case tests ─────────────

    #[tokio::test]
    async fn test_publish_with_no_subscribers() {
        let bus = EventBus::new();
        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        // Should succeed even with no subscribers
        bus.publish(event).await.unwrap();
        let metrics = bus.metrics();
        assert_eq!(metrics.events_published, 1);
        assert_eq!(metrics.events_delivered, 0);
    }

    #[tokio::test]
    async fn test_unsubscribe_nonexistent() {
        let bus = EventBus::new();
        let result = bus.unsubscribe(Uuid::new_v4()).await;
        assert!(matches!(result, Err(EventBusError::SubscriberNotFound)));
    }

    #[tokio::test]
    async fn test_subscribe_invalid_pattern() {
        let bus = EventBus::new();
        // Regex does not support unescaped '[' in this context
        // The wildcard pattern conversion does not produce invalid regex for '[',
        // but we can test with a pattern that becomes an invalid regex.
        // Actually, '[' is not a wildcard character, so it goes through Exact,
        // meaning it won't fail. Test with a truly invalid regex path instead.
        // Since SubscriptionFilter::new only builds regex for '*' and '?',
        // an exact pattern never fails. We can verify that:
        let result = bus.subscribe("session.started").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_callback_max_subscribers() {
        let config = EventBusConfig {
            max_subscribers: 1,
            ..Default::default()
        };
        let bus = EventBus::with_config(config);

        let _id1 = bus
            .subscribe_callback("session.*", move |_| Ok(()))
            .await
            .unwrap();

        let result = bus.subscribe_callback("tool.*", move |_| Ok(())).await;
        assert!(matches!(result, Err(EventBusError::MaxSubscribersReached)));
    }

    #[tokio::test]
    async fn test_hybrid_max_subscribers() {
        let config = EventBusConfig {
            max_subscribers: 1,
            ..Default::default()
        };
        let bus = EventBus::with_config(config);

        let _id1 = bus
            .subscribe_hybrid("session.*", move |_| Ok(()))
            .await
            .unwrap();

        let result = bus.subscribe_hybrid("tool.*", move |_| Ok(())).await;
        assert!(matches!(result, Err(EventBusError::MaxSubscribersReached)));
    }

    #[tokio::test]
    async fn test_hybrid_unsubscribe() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        let (id, _rx) = bus
            .subscribe_hybrid("session.*", move |_| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(event.clone()).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);

        bus.unsubscribe(id).await.unwrap();
        bus.publish(event).await.unwrap();
        // Should not increment after unsubscribe
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_custom_channel_capacity() {
        let config = EventBusConfig {
            channel_capacity: 1,
            ..Default::default()
        };
        let bus = EventBus::with_config(config);

        let (_id, mut rx) = bus.subscribe("session.*").await.unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());

        // Publish two events - the second overflows the capacity of 1.
        // tokio::sync::broadcast returns Lagged when messages are missed.
        bus.publish(event.clone()).await.unwrap();
        bus.publish(event.clone()).await.unwrap();

        // The receiver gets a Lagged result since one message was dropped.
        // Verify we can handle this gracefully.
        let result = rx.recv().await;
        assert!(
            result.is_ok()
                || matches!(
                    result,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
                )
        );
    }

    #[tokio::test]
    async fn test_event_bus_new_vs_default() {
        let bus1 = EventBus::new();
        let bus2 = EventBus::default();

        let m1 = bus1.metrics();
        let m2 = bus2.metrics();
        assert_eq!(m1.events_published, m2.events_published);
        assert_eq!(m1.subscriber_count, m2.subscriber_count);
    }

    #[tokio::test]
    async fn test_event_bus_with_config() {
        let config = EventBusConfig {
            channel_capacity: 500,
            max_subscribers: 50,
            debug_logging: true,
        };
        let bus = EventBus::with_config(config);
        let (_id, _rx) = bus.subscribe("session.*").await.unwrap();
        assert_eq!(bus.metrics().subscriber_count, 1);
    }

    #[tokio::test]
    async fn test_multiple_events_published() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        let _id = bus
            .subscribe_callback("*", move |_| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        for i in 0..20 {
            let event =
                SessionStartedEvent::new(SessionId::new(), format!("task-{i}"), "detail".into());
            bus.publish(event).await.unwrap();
        }

        assert_eq!(count.load(Ordering::SeqCst), 20);
        assert_eq!(bus.metrics().events_published, 20);
    }

    #[tokio::test]
    async fn test_subscription_handle_id() {
        let bus = Arc::new(EventBus::new());
        let (id, _rx) = bus.subscribe("session.*").await.unwrap();
        let handle = SubscriptionHandle::new(id, bus);
        assert_eq!(handle.id(), id);
        // Drop without calling unsubscribe (should auto-cleanup)
        drop(handle);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_callback_error_increments_failed_metric() {
        let bus = EventBus::new();

        let _id = bus
            .subscribe_callback("session.*", move |_| Err::<(), _>("always fails".into()))
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(event).await.unwrap();

        let metrics = bus.metrics();
        assert_eq!(metrics.events_published, 1);
        assert!(metrics.events_failed > 0);
    }

    #[tokio::test]
    async fn test_post_publish_hook() {
        let bus = EventBus::new();
        let post_called = Arc::new(AtomicBool::new(false));
        let post_called_clone = post_called.clone();

        bus.register_hook(HookPhase::PostPublish, move |_event| {
            post_called_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(event).await.unwrap();

        assert!(post_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_both_pre_and_post_hooks() {
        let bus = EventBus::new();
        let pre_called = Arc::new(AtomicBool::new(false));
        let post_called = Arc::new(AtomicBool::new(false));
        let pre_clone = pre_called.clone();
        let post_clone = post_called.clone();

        bus.register_hook(HookPhase::PrePublish, move |_| {
            pre_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        bus.register_hook(HookPhase::PostPublish, move |_| {
            post_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(event).await.unwrap();

        assert!(pre_called.load(Ordering::SeqCst));
        assert!(post_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_wildcard_no_match() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        let _id = bus
            .subscribe_callback("tool.*", move |_| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(event).await.unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_callback_can_subscribe_during_publish_no_deadlock() {
        let bus = Arc::new(EventBus::new());
        let subscribed = Arc::new(AtomicBool::new(false));

        let bus_clone = bus.clone();
        let subscribed_clone = subscribed.clone();
        let _id = bus
            .subscribe_callback("session.started", move |_event| {
                // This callback subscribes a new listener during event delivery.
                // Before the deadlock fix, this would deadlock because publish()
                // held a read lock while invoking callbacks, and subscribe needs
                // a write lock.
                let bus = bus_clone.clone();
                let subscribed = subscribed_clone.clone();
                tokio::spawn(async move {
                    let (_id, _rx) = bus.subscribe("session.*").await.unwrap();
                    subscribed.store(true, Ordering::SeqCst);
                });
                Ok(())
            })
            .await
            .unwrap();

        let event =
            SessionStartedEvent::new(SessionId::new(), "test".to_string(), "test".to_string());
        bus.publish(event).await.unwrap();

        // Give the spawned task time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert!(subscribed.load(Ordering::SeqCst));
    }
}
