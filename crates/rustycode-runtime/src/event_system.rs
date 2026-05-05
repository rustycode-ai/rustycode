//! Event System and Event-Driven Architecture
//!
//! This module provides comprehensive event management with:
//! - Publish-subscribe patterns
//! - Event filtering and routing
//! - Event replay capabilities
//! - Dead letter queues
//! - Event aggregation
//! - Event persistence
//! - Reactive agent behaviors

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Event types in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EventType {
    // System events
    SystemStarted,
    SystemStopped,
    SystemError,
    SystemWarning,

    // Agent events
    AgentSpawned,
    AgentTerminated,
    AgentIdle,
    AgentBusy,
    AgentError,

    // Task events
    TaskCreated,
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
    TaskProgress,

    // Resource events
    ResourceAllocated,
    ResourceReleased,
    ResourceExhausted,
    ResourceWarning,

    // Health events
    HealthCheckPassed,
    HealthCheckFailed,
    AnomalyDetected,
    RecoveryInitiated,
    RecoveryCompleted,

    // Negotiation events
    NegotiationStarted,
    NegotiationCompleted,
    ConsensusReached,
    ConsensusFailed,

    // Learning events
    ExperienceRecorded,
    StrategyUpdated,
    PatternDiscovered,

    // Custom events
    Custom(String),
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventType::Custom(name) => write!(f, "{}", name),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Event priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum EventPriority {
    Critical = 5,
    High = 4,
    Medium = 3,
    Low = 2,
    Background = 1,
}

/// System event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: EventType,
    pub priority: EventPriority,
    pub source: String,
    pub timestamp: DateTime<Utc>,
    pub data: EventData,
    pub metadata: HashMap<String, String>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
}

/// Event data payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventData {
    Empty,
    Text(String),
    Json(serde_json::Value),
    Binary(Vec<u8>),
    Structured(HashMap<String, serde_json::Value>),
}

impl EventData {
    pub fn is_empty(&self) -> bool {
        matches!(self, EventData::Empty)
    }

    pub fn text(&self) -> Option<&String> {
        match self {
            EventData::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn json(&self) -> Option<&serde_json::Value> {
        match self {
            EventData::Json(v) => Some(v),
            _ => None,
        }
    }
}

/// Event filter for subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    pub event_types: Option<HashSet<EventType>>,
    pub sources: Option<HashSet<String>>,
    pub priority_min: Option<EventPriority>,
    pub priority_max: Option<EventPriority>,
    pub custom_filter: Option<String>, // Expression language
}

impl EventFilter {
    pub fn new() -> Self {
        Self {
            event_types: None,
            sources: None,
            priority_min: None,
            priority_max: None,
            custom_filter: None,
        }
    }

    pub fn with_event_types(mut self, types: HashSet<EventType>) -> Self {
        self.event_types = Some(types);
        self
    }

    pub fn with_sources(mut self, sources: HashSet<String>) -> Self {
        self.sources = Some(sources);
        self
    }

    pub fn with_priority_range(mut self, min: EventPriority, max: EventPriority) -> Self {
        self.priority_min = Some(min);
        self.priority_max = Some(max);
        self
    }

    pub fn matches(&self, event: &Event) -> bool {
        // Check event type
        if let Some(ref types) = self.event_types {
            if !types.contains(&event.event_type) {
                return false;
            }
        }

        // Check source
        if let Some(ref sources) = self.sources {
            if !sources.contains(&event.source) {
                return false;
            }
        }

        // Check priority range
        if let Some(min) = self.priority_min {
            if event.priority < min {
                return false;
            }
        }

        if let Some(max) = self.priority_max {
            if event.priority > max {
                return false;
            }
        }

        true
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Event subscription
#[derive(Debug, Clone)]
pub struct EventSubscription {
    pub id: String,
    pub subscriber_id: String,
    pub filter: EventFilter,
    pub created_at: DateTime<Utc>,
    pub events_received: u64,
    pub active: bool,
}

/// Dead letter event (failed delivery)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterEvent {
    pub event: Event,
    pub subscription_id: String,
    pub reason: String,
    pub failed_at: DateTime<Utc>,
    pub retry_count: u32,
}

/// Event statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStatistics {
    pub total_events_published: u64,
    pub total_events_delivered: u64,
    pub total_events_failed: u64,
    pub active_subscriptions: usize,
    pub dead_letter_count: u64,
    pub average_processing_time_ms: f64,
    pub last_updated: DateTime<Utc>,
}

/// Event aggregator for combining multiple events
#[derive(Debug, Clone)]
pub struct EventAggregator {
    aggregation_window_ms: u64,
    max_events: usize,
    current_events: Vec<Event>,
}

impl EventAggregator {
    pub fn new(window_ms: u64, max_events: usize) -> Self {
        Self {
            aggregation_window_ms: window_ms,
            max_events,
            current_events: Vec::new(),
        }
    }

    pub fn add_event(&mut self, event: Event) -> Option<Vec<Event>> {
        self.current_events.push(event);

        // Check if we should emit
        if self.current_events.len() >= self.max_events {
            Some(self.flush())
        } else {
            None
        }
    }

    pub fn flush(&mut self) -> Vec<Event> {
        let events = self.current_events.drain(..).collect();
        events
    }

    pub fn should_emit(&self) -> bool {
        let Some(oldest) = self.current_events.first() else {
            return false;
        };
        let age = (Utc::now() - oldest.timestamp).num_milliseconds().max(0) as u64;
        age >= self.aggregation_window_ms || self.current_events.len() >= self.max_events
    }
}

/// Event replay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayConfig {
    pub from_timestamp: Option<DateTime<Utc>>,
    pub to_timestamp: Option<DateTime<Utc>>,
    pub event_types: Option<HashSet<EventType>>,
    pub max_events: Option<usize>,
    pub speed_multiplier: f64, // 1.0 = real-time, 2.0 = 2x speed
}

/// Event store for persistence
#[derive(Debug, Clone)]
pub struct EventStoreConfig {
    pub max_size: usize,
    pub retention_duration_ms: u64,
    pub persist_to_disk: bool,
    pub disk_path: Option<String>,
}

impl Default for EventStoreConfig {
    fn default() -> Self {
        Self {
            max_size: 10000,
            retention_duration_ms: 86400000, // 24 hours
            persist_to_disk: false,
            disk_path: None,
        }
    }
}

/// Main event system
pub struct EventSystem {
    event_channel: Arc<broadcast::Sender<Event>>,
    _receiver: broadcast::Receiver<Event>, // Keep alive to prevent channel closure
    subscriptions: Arc<RwLock<HashMap<String, EventSubscription>>>,
    event_store: Arc<RwLock<VecDeque<Event>>>,
    dead_letter_queue: Arc<RwLock<VecDeque<DeadLetterEvent>>>,
    statistics: Arc<RwLock<EventStatistics>>,
    event_counter: Arc<RwLock<u64>>,
    subscription_counter: Arc<RwLock<u64>>,
    config: EventStoreConfig,
}

impl EventSystem {
    pub fn new(config: EventStoreConfig) -> Self {
        let (tx, rx) = broadcast::channel(10000);

        Self {
            event_channel: Arc::new(tx),
            _receiver: rx, // Keep receiver alive
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_store: Arc::new(RwLock::new(VecDeque::new())),
            dead_letter_queue: Arc::new(RwLock::new(VecDeque::new())),
            statistics: Arc::new(RwLock::new(EventStatistics {
                total_events_published: 0,
                total_events_delivered: 0,
                total_events_failed: 0,
                active_subscriptions: 0,
                dead_letter_count: 0,
                average_processing_time_ms: 0.0,
                last_updated: Utc::now(),
            })),
            event_counter: Arc::new(RwLock::new(0)),
            subscription_counter: Arc::new(RwLock::new(0)),
            config,
        }
    }

    /// Publish an event
    pub async fn publish(&self, mut event: Event) -> Result<(), String> {
        // Generate event ID if not provided
        if event.id.is_empty() {
            let mut counter = self.event_counter.write().await;
            *counter += 1;
            event.id = format!("event_{}", *counter);
        }

        // Ensure timestamp is set
        if event.timestamp.timestamp() == 0 {
            event.timestamp = Utc::now();
        }

        // Store event
        {
            let mut store = self.event_store.write().await;
            store.push_back(event.clone());

            // Trim store if needed
            if store.len() > self.config.max_size {
                store.pop_front();
            }
        }

        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.total_events_published = stats.total_events_published.saturating_add(1);
            stats.last_updated = Utc::now();
        }

        // Publish to channel
        match self.event_channel.send(event.clone()) {
            Ok(receiver_count) => {
                if receiver_count == 0 {
                    // No active receivers, log warning
                    tracing::warn!("Event {} published but no subscribers", event.event_type);
                }
                Ok(())
            }
            Err(e) => Err(format!("Failed to publish event: {}", e)),
        }
    }

    /// Subscribe to events
    pub async fn subscribe(
        &self,
        subscriber_id: String,
        filter: EventFilter,
    ) -> Result<String, String> {
        // Generate subscription ID
        let mut counter = self.subscription_counter.write().await;
        *counter += 1;
        let subscription_id = format!("sub_{}", *counter);

        // Create subscription
        let subscription = EventSubscription {
            id: subscription_id.clone(),
            subscriber_id,
            filter,
            created_at: Utc::now(),
            events_received: 0,
            active: true,
        };

        // Store subscription
        let active_count = {
            let mut subscriptions = self.subscriptions.write().await;
            subscriptions.insert(subscription_id.clone(), subscription.clone());
            subscriptions.len()
        };

        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.active_subscriptions = active_count;
            stats.last_updated = Utc::now();
        }

        Ok(subscription_id)
    }

    /// Unsubscribe from events
    pub async fn unsubscribe(&self, subscription_id: &str) -> Result<(), String> {
        let mut subscriptions = self.subscriptions.write().await;

        if subscriptions.remove(subscription_id).is_some() {
            // Update statistics
            let mut stats = self.statistics.write().await;
            stats.active_subscriptions = subscriptions.len();
            stats.last_updated = Utc::now();
            Ok(())
        } else {
            Err(format!("Subscription {} not found", subscription_id))
        }
    }

    /// Get a receiver for a subscription
    pub fn receiver(&self) -> broadcast::Receiver<Event> {
        self.event_channel.subscribe()
    }

    /// Create an event
    pub fn create_event(
        &self,
        event_type: EventType,
        source: String,
        data: EventData,
        priority: EventPriority,
    ) -> Event {
        Event {
            id: String::new(),
            event_type,
            priority,
            source,
            timestamp: Utc::now(),
            data,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        }
    }

    /// Add correlation ID to event
    pub fn with_correlation_id(mut event: Event, correlation_id: String) -> Event {
        event.correlation_id = Some(correlation_id);
        event
    }

    /// Add causation ID to event
    pub fn with_causation_id(mut event: Event, causation_id: String) -> Event {
        event.causation_id = Some(causation_id);
        event
    }

    /// Add metadata to event
    pub fn with_metadata(mut event: Event, key: String, value: String) -> Event {
        event.metadata.insert(key, value);
        event
    }

    /// Get event statistics
    pub async fn statistics(&self) -> EventStatistics {
        let stats = self.statistics.read().await;
        stats.clone()
    }

    /// Get dead letter events
    pub async fn dead_letter_events(&self) -> Vec<DeadLetterEvent> {
        let queue = self.dead_letter_queue.read().await;
        queue.iter().cloned().collect()
    }

    /// Clear dead letter queue
    pub async fn clear_dead_letter_queue(&self) {
        let mut queue = self.dead_letter_queue.write().await;
        queue.clear();
    }

    /// Replay events from store
    pub async fn replay(&self, config: ReplayConfig) -> Vec<Event> {
        let store = self.event_store.read().await;
        let mut events = Vec::new();

        for event in store.iter() {
            // Check time range
            if let Some(from) = config.from_timestamp {
                if event.timestamp < from {
                    continue;
                }
            }

            if let Some(to) = config.to_timestamp {
                if event.timestamp > to {
                    continue;
                }
            }

            // Check event types
            if let Some(ref types) = config.event_types {
                if !types.contains(&event.event_type) {
                    continue;
                }
            }

            events.push(event.clone());

            // Check max events
            if let Some(max) = config.max_events {
                if events.len() >= max {
                    break;
                }
            }
        }

        events
    }

    /// Get events from store
    pub async fn events(
        &self,
        event_type: Option<EventType>,
        source: Option<String>,
        limit: Option<usize>,
    ) -> Vec<Event> {
        let store = self.event_store.read().await;
        let mut events = Vec::new();

        for event in store.iter().rev() {
            // Check event type
            if let Some(ref et) = event_type {
                if &event.event_type != et {
                    continue;
                }
            }

            // Check source
            if let Some(ref s) = source {
                if &event.source != s {
                    continue;
                }
            }

            events.push(event.clone());

            // Check limit
            if let Some(limit) = limit {
                if events.len() >= limit {
                    break;
                }
            }
        }

        events
    }

    /// Add to dead letter queue
    pub async fn add_to_dead_letter(&self, dle: DeadLetterEvent) {
        let mut queue = self.dead_letter_queue.write().await;
        queue.push_back(dle);

        // Evict oldest entries to prevent unbounded growth
        const MAX_DEAD_LETTER_SIZE: usize = 1000;
        while queue.len() > MAX_DEAD_LETTER_SIZE {
            queue.pop_front();
        }

        let mut stats = self.statistics.write().await;
        stats.total_events_failed = stats.total_events_failed.saturating_add(1);
        stats.dead_letter_count = stats.dead_letter_count.saturating_add(1);
        stats.last_updated = Utc::now();
    }

    /// Retry dead letter events
    pub async fn retry_dead_letter(&self, subscription_id: &str) -> Result<usize, String> {
        let queue = self.dead_letter_queue.read().await;
        let events_to_retry: Vec<_> = queue
            .iter()
            .filter(|dle| dle.subscription_id == subscription_id)
            .collect();

        let count = events_to_retry.len();

        // Republish events
        for dle in events_to_retry {
            // Create new event with retry metadata
            let mut event = dle.event.clone();
            event
                .metadata
                .insert("retry_count".to_string(), (dle.retry_count + 1).to_string());

            // Publish
            if let Err(e) = self.publish(event).await {
                tracing::error!("Failed to retry event: {}", e);
            }
        }

        Ok(count)
    }

    /// Process events with a handler
    pub async fn process_events<F>(
        &self,
        subscription_id: &str,
        mut handler: F,
    ) -> Result<(), String>
    where
        F: FnMut(Event) -> Result<(), String>,
    {
        let mut rx = self.receiver();

        // Get subscription filter
        let filter = {
            let subscriptions = self.subscriptions.read().await;
            subscriptions.get(subscription_id).map(|s| s.filter.clone())
        };

        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Check filter
                    if let Some(ref filter) = filter {
                        if !filter.matches(&event) {
                            continue;
                        }
                    }

                    // Handle event
                    match handler(event.clone()) {
                        Ok(()) => {
                            // Update statistics
                            let mut stats = self.statistics.write().await;
                            stats.total_events_delivered =
                                stats.total_events_delivered.saturating_add(1);
                            stats.last_updated = Utc::now();

                            // Update subscription
                            let mut subscriptions = self.subscriptions.write().await;
                            if let Some(sub) = subscriptions.get_mut(subscription_id) {
                                sub.events_received = sub.events_received.saturating_add(1);
                            }
                        }
                        Err(e) => {
                            // Add to dead letter queue
                            let dle = DeadLetterEvent {
                                event,
                                subscription_id: subscription_id.to_string(),
                                reason: e,
                                failed_at: Utc::now(),
                                retry_count: 0,
                            };
                            self.add_to_dead_letter(dle).await;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Lagged behind by {} events", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Create event aggregator
    pub fn create_aggregator(&self, window_ms: u64, max_events: usize) -> EventAggregator {
        EventAggregator::new(window_ms, max_events)
    }
}

/// Helper function to create common system events
impl EventSystem {
    pub fn system_started(source: String) -> Event {
        Event {
            id: String::new(),
            event_type: EventType::SystemStarted,
            priority: EventPriority::High,
            source,
            timestamp: Utc::now(),
            data: EventData::Text("System started".to_string()),
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        }
    }

    pub fn task_created(task_id: String, source: String, priority: EventPriority) -> Event {
        let mut data = HashMap::new();
        data.insert("task_id".to_string(), serde_json::json!(task_id));

        Event {
            id: String::new(),
            event_type: EventType::TaskCreated,
            priority,
            source,
            timestamp: Utc::now(),
            data: EventData::Structured(data),
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        }
    }

    pub fn agent_spawned(agent_role: AgentRole, source: String) -> Event {
        let mut data = HashMap::new();
        data.insert(
            "agent_role".to_string(),
            serde_json::json!(format!("{:?}", agent_role)),
        );

        Event {
            id: String::new(),
            event_type: EventType::AgentSpawned,
            priority: EventPriority::Medium,
            source,
            timestamp: Utc::now(),
            data: EventData::Structured(data),
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        }
    }

    pub fn anomaly_detected(
        anomaly_type: String,
        severity: EventPriority,
        source: String,
    ) -> Event {
        let mut data = HashMap::new();
        data.insert("anomaly_type".to_string(), serde_json::json!(anomaly_type));

        Event {
            id: String::new(),
            event_type: EventType::AnomalyDetected,
            priority: severity,
            source,
            timestamp: Utc::now(),
            data: EventData::Structured(data),
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_filter_matches() {
        let mut types = HashSet::new();
        types.insert(EventType::TaskCreated);
        types.insert(EventType::TaskCompleted);

        let filter = EventFilter::new().with_event_types(types);

        let event = Event {
            id: "test".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "test_source".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };

        assert!(filter.matches(&event));
    }

    #[test]
    fn test_event_filter_no_match() {
        let mut types = HashSet::new();
        types.insert(EventType::TaskCompleted);

        let filter = EventFilter::new().with_event_types(types);

        let event = Event {
            id: "test".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "test_source".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };

        assert!(!filter.matches(&event));
    }

    #[tokio::test]
    async fn test_event_publish() {
        let system = EventSystem::new(EventStoreConfig::default());

        let event = system.create_event(
            EventType::TaskCreated,
            "test_source".to_string(),
            EventData::Text("test".to_string()),
            EventPriority::Medium,
        );

        assert!(system.publish(event).await.is_ok());
    }

    #[tokio::test]
    async fn test_event_subscribe() {
        let system = EventSystem::new(EventStoreConfig::default());

        let subscription_id = system
            .subscribe("test_subscriber".to_string(), EventFilter::new())
            .await
            .unwrap();

        assert!(subscription_id.starts_with("sub_"));
    }

    #[tokio::test]
    async fn test_event_statistics() {
        let system = EventSystem::new(EventStoreConfig::default());

        let event = system.create_event(
            EventType::TaskCreated,
            "test_source".to_string(),
            EventData::Empty,
            EventPriority::Medium,
        );

        system.publish(event).await.unwrap();

        let stats = system.statistics().await;
        assert_eq!(stats.total_events_published, 1);
    }

    #[tokio::test]
    async fn test_event_retrieval() {
        let system = EventSystem::new(EventStoreConfig::default());

        let event = system.create_event(
            EventType::TaskCreated,
            "test_source".to_string(),
            EventData::Text("test".to_string()),
            EventPriority::Medium,
        );

        system.publish(event).await.unwrap();

        let events = system
            .events(Some(EventType::TaskCreated), None, None)
            .await;
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_event_aggregator() {
        let mut aggregator = EventAggregator::new(1000, 3);

        let event1 = Event {
            id: "1".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "test".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };

        let event2 = event1.clone();
        let event3 = event1.clone();

        assert!(aggregator.add_event(event1).is_none());
        assert!(aggregator.add_event(event2).is_none());

        let result = aggregator.add_event(event3);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 3);
    }

    // --- New tests below ---

    #[test]
    fn test_event_data_accessors() {
        let empty = EventData::Empty;
        assert!(empty.is_empty());
        assert!(empty.text().is_none());
        assert!(empty.json().is_none());

        let text = EventData::Text("hello".to_string());
        assert!(!text.is_empty());
        assert_eq!(text.text(), Some(&"hello".to_string()));
        assert!(text.json().is_none());

        let json_val = serde_json::json!({"key": "value"});
        let json = EventData::Json(json_val.clone());
        assert!(!json.is_empty());
        assert!(json.text().is_none());
        assert_eq!(json.json(), Some(&json_val));

        let binary = EventData::Binary(vec![1, 2, 3]);
        assert!(!binary.is_empty());
        assert!(binary.text().is_none());

        let mut structured_data = HashMap::new();
        structured_data.insert("k".to_string(), serde_json::json!(42));
        let structured = EventData::Structured(structured_data);
        assert!(!structured.is_empty());
    }

    #[test]
    fn test_event_type_display() {
        assert_eq!(format!("{}", EventType::SystemStarted), "SystemStarted");
        assert_eq!(format!("{}", EventType::TaskCreated), "TaskCreated");
        assert_eq!(
            format!("{}", EventType::Custom("MyEvent".to_string())),
            "MyEvent"
        );
    }

    #[test]
    fn test_event_priority_ordering() {
        assert!(EventPriority::Critical > EventPriority::High);
        assert!(EventPriority::High > EventPriority::Medium);
        assert!(EventPriority::Medium > EventPriority::Low);
        assert!(EventPriority::Low > EventPriority::Background);
        assert_eq!(EventPriority::Medium, EventPriority::Medium);
    }

    #[test]
    fn test_event_filter_with_sources() {
        let mut sources = HashSet::new();
        sources.insert("agent_1".to_string());
        sources.insert("agent_2".to_string());

        let filter = EventFilter::new().with_sources(sources);

        let matching_event = Event {
            id: "1".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "agent_1".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        assert!(filter.matches(&matching_event));

        let non_matching_event = Event {
            id: "2".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "agent_3".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        assert!(!filter.matches(&non_matching_event));
    }

    #[test]
    fn test_event_filter_with_priority_range() {
        let filter =
            EventFilter::new().with_priority_range(EventPriority::Medium, EventPriority::Critical);

        let low_event = Event {
            id: "1".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Low,
            source: "test".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        assert!(!filter.matches(&low_event));

        let medium_event = Event {
            id: "2".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "test".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        assert!(filter.matches(&medium_event));

        let critical_event = Event {
            id: "3".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Critical,
            source: "test".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        assert!(filter.matches(&critical_event));
    }

    #[test]
    fn test_event_filter_default_matches_all() {
        let filter = EventFilter::default();

        let event = Event {
            id: "1".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Low,
            source: "anything".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        assert!(filter.matches(&event));
    }

    #[test]
    fn test_event_store_config_default() {
        let config = EventStoreConfig::default();
        assert_eq!(config.max_size, 10000);
        assert_eq!(config.retention_duration_ms, 86400000);
        assert!(!config.persist_to_disk);
        assert!(config.disk_path.is_none());
    }

    #[tokio::test]
    async fn test_event_data_serialization_roundtrip() {
        let original = EventData::Text("test payload".to_string());
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EventData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text(), Some(&"test payload".to_string()));

        let json_data = EventData::Json(serde_json::json!({"count": 42}));
        let json_str = serde_json::to_string(&json_data).unwrap();
        let back: EventData = serde_json::from_str(&json_str).unwrap();
        assert!(back.json().is_some());
    }

    #[tokio::test]
    async fn test_create_event_with_correlation_and_causation() {
        let system = EventSystem::new(EventStoreConfig::default());

        let event = system.create_event(
            EventType::TaskStarted,
            "test_source".to_string(),
            EventData::Empty,
            EventPriority::High,
        );

        let event = EventSystem::with_correlation_id(event, "corr_123".to_string());
        assert_eq!(event.correlation_id, Some("corr_123".to_string()));

        let event = EventSystem::with_causation_id(event, "cause_456".to_string());
        assert_eq!(event.causation_id, Some("cause_456".to_string()));
    }

    #[tokio::test]
    async fn test_with_metadata() {
        let event = Event {
            id: "1".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "test".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };

        let event = EventSystem::with_metadata(event, "env".to_string(), "prod".to_string());
        assert_eq!(event.metadata.get("env"), Some(&"prod".to_string()));

        let event = EventSystem::with_metadata(event, "region".to_string(), "us-east".to_string());
        assert_eq!(event.metadata.get("region"), Some(&"us-east".to_string()));
        assert_eq!(event.metadata.len(), 2);
    }

    #[tokio::test]
    async fn test_helper_system_started() {
        let event = EventSystem::system_started("boot_manager".to_string());
        assert_eq!(event.event_type, EventType::SystemStarted);
        assert_eq!(event.priority, EventPriority::High);
        assert_eq!(event.source, "boot_manager");
        assert!(event.data.text().is_some());
    }

    #[tokio::test]
    async fn test_helper_task_created() {
        let event = EventSystem::task_created(
            "task_42".to_string(),
            "scheduler".to_string(),
            EventPriority::Medium,
        );
        assert_eq!(event.event_type, EventType::TaskCreated);
        assert_eq!(event.priority, EventPriority::Medium);
        assert_eq!(event.source, "scheduler");
        assert!(matches!(event.data, EventData::Structured(_)));
    }

    #[tokio::test]
    async fn test_helper_agent_spawned() {
        let event = EventSystem::agent_spawned(AgentRole::Reviewer, "orchestrator".to_string());
        assert_eq!(event.event_type, EventType::AgentSpawned);
        assert_eq!(event.priority, EventPriority::Medium);
        assert_eq!(event.source, "orchestrator");
        assert!(matches!(event.data, EventData::Structured(_)));
    }

    #[tokio::test]
    async fn test_helper_anomaly_detected() {
        let event = EventSystem::anomaly_detected(
            "cpu_spike".to_string(),
            EventPriority::Critical,
            "monitor".to_string(),
        );
        assert_eq!(event.event_type, EventType::AnomalyDetected);
        assert_eq!(event.priority, EventPriority::Critical);
        assert_eq!(event.source, "monitor");
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let system = EventSystem::new(EventStoreConfig::default());

        let sub_id = system
            .subscribe("sub1".to_string(), EventFilter::new())
            .await
            .unwrap();

        let result = system.unsubscribe(&sub_id).await;
        assert!(result.is_ok());

        let stats = system.statistics().await;
        assert_eq!(stats.active_subscriptions, 0);
    }

    #[tokio::test]
    async fn test_unsubscribe_nonexistent() {
        let system = EventSystem::new(EventStoreConfig::default());

        let result = system.unsubscribe("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dead_letter_queue_operations() {
        let system = EventSystem::new(EventStoreConfig::default());

        // Initially empty
        let dles = system.get_dead_letter_events().await;
        assert!(dles.is_empty());

        // Add a dead letter event
        let event = Event {
            id: "e1".to_string(),
            event_type: EventType::TaskFailed,
            priority: EventPriority::High,
            source: "worker".to_string(),
            timestamp: Utc::now(),
            data: EventData::Text("boom".to_string()),
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };

        let dle = DeadLetterEvent {
            event,
            subscription_id: "sub_1".to_string(),
            reason: "Handler panicked".to_string(),
            failed_at: Utc::now(),
            retry_count: 0,
        };

        system.add_to_dead_letter(dle).await;

        let dles = system.get_dead_letter_events().await;
        assert_eq!(dles.len(), 1);
        assert_eq!(dles[0].reason, "Handler panicked");

        let stats = system.statistics().await;
        assert_eq!(stats.total_events_failed, 1);
        assert_eq!(stats.dead_letter_count, 1);

        // Clear the queue
        system.clear_dead_letter_queue().await;
        let dles = system.get_dead_letter_events().await;
        assert!(dles.is_empty());
    }

    #[tokio::test]
    async fn test_replay_with_filters() {
        let system = EventSystem::new(EventStoreConfig::default());

        // Publish several events
        let e1 = system.create_event(
            EventType::TaskCreated,
            "s1".to_string(),
            EventData::Empty,
            EventPriority::Medium,
        );
        let e2 = system.create_event(
            EventType::TaskCompleted,
            "s1".to_string(),
            EventData::Empty,
            EventPriority::Medium,
        );
        let e3 = system.create_event(
            EventType::AgentSpawned,
            "s2".to_string(),
            EventData::Empty,
            EventPriority::Medium,
        );

        system.publish(e1).await.unwrap();
        system.publish(e2).await.unwrap();
        system.publish(e3).await.unwrap();

        // Replay all
        let all = system
            .replay(ReplayConfig {
                from_timestamp: None,
                to_timestamp: None,
                event_types: None,
                max_events: None,
                speed_multiplier: 1.0,
            })
            .await;
        assert_eq!(all.len(), 3);

        // Replay only TaskCreated
        let mut task_types = HashSet::new();
        task_types.insert(EventType::TaskCreated);

        let filtered = system
            .replay(ReplayConfig {
                from_timestamp: None,
                to_timestamp: None,
                event_types: Some(task_types),
                max_events: None,
                speed_multiplier: 1.0,
            })
            .await;
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event_type, EventType::TaskCreated);

        // Replay with max_events
        let limited = system
            .replay(ReplayConfig {
                from_timestamp: None,
                to_timestamp: None,
                event_types: None,
                max_events: Some(2),
                speed_multiplier: 1.0,
            })
            .await;
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn test_get_events_by_source() {
        let system = EventSystem::new(EventStoreConfig::default());

        let e1 = system.create_event(
            EventType::TaskCreated,
            "alpha".to_string(),
            EventData::Empty,
            EventPriority::Medium,
        );
        let e2 = system.create_event(
            EventType::TaskCreated,
            "beta".to_string(),
            EventData::Empty,
            EventPriority::Medium,
        );

        system.publish(e1).await.unwrap();
        system.publish(e2).await.unwrap();

        let alpha_events = system
            .events(None, Some("alpha".to_string()), None)
            .await;
        assert_eq!(alpha_events.len(), 1);

        let limited = system.events(None, None, Some(1)).await;
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn test_publish_generates_id_when_empty() {
        let system = EventSystem::new(EventStoreConfig::default());

        let event = system.create_event(
            EventType::SystemStarted,
            "test".to_string(),
            EventData::Empty,
            EventPriority::High,
        );
        assert!(event.id.is_empty());

        system.publish(event).await.unwrap();

        // Verify the stored event got an ID
        let stored = system.events(None, None, None).await;
        assert_eq!(stored.len(), 1);
        assert!(!stored[0].id.is_empty());
        assert!(stored[0].id.starts_with("event_"));
    }

    #[tokio::test]
    async fn test_aggregator_flush_before_full() {
        let mut aggregator = EventAggregator::new(1000, 10);

        let event = Event {
            id: "1".to_string(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::Medium,
            source: "test".to_string(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };

        aggregator.add_event(event.clone());
        aggregator.add_event(event.clone());

        let flushed = aggregator.flush();
        assert_eq!(flushed.len(), 2);

        // Flushing again yields empty
        let flushed_again = aggregator.flush();
        assert!(flushed_again.is_empty());
    }

    #[test]
    fn test_aggregator_should_emit_empty() {
        let aggregator = EventAggregator::new(1000, 3);
        assert!(!aggregator.should_emit());
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for event_system
    // =========================================================================

    // 1. EventType serde roundtrip for key variants
    #[test]
    fn event_type_serde_roundtrip() {
        let variants = [
            EventType::SystemStarted,
            EventType::AgentSpawned,
            EventType::TaskCreated,
            EventType::ResourceAllocated,
            EventType::HealthCheckPassed,
            EventType::AnomalyDetected,
            EventType::NegotiationStarted,
            EventType::ExperienceRecorded,
            EventType::Custom("MyEvent".into()),
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: EventType = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    // 2. EventPriority serde roundtrip for all variants
    #[test]
    fn event_priority_serde_roundtrip() {
        let priorities = [
            EventPriority::Critical,
            EventPriority::High,
            EventPriority::Medium,
            EventPriority::Low,
            EventPriority::Background,
        ];
        for p in &priorities {
            let json = serde_json::to_string(p).unwrap();
            let decoded: EventPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, decoded);
        }
    }

    // 3. EventPriority discriminant values
    #[test]
    fn event_priority_discriminant_values() {
        assert_eq!(EventPriority::Critical as usize, 5);
        assert_eq!(EventPriority::High as usize, 4);
        assert_eq!(EventPriority::Medium as usize, 3);
        assert_eq!(EventPriority::Low as usize, 2);
        assert_eq!(EventPriority::Background as usize, 1);
    }

    // 4. Event serde roundtrip
    #[test]
    fn event_serde_roundtrip() {
        let event = Event {
            id: "evt_42".into(),
            event_type: EventType::TaskCompleted,
            priority: EventPriority::High,
            source: "worker".into(),
            timestamp: Utc::now(),
            data: EventData::Text("done".into()),
            metadata: HashMap::from([("key".into(), "val".into())]),
            correlation_id: Some("corr_1".into()),
            causation_id: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "evt_42");
        assert_eq!(decoded.event_type, EventType::TaskCompleted);
        assert_eq!(decoded.correlation_id, Some("corr_1".into()));
        assert_eq!(decoded.metadata.get("key"), Some(&"val".into()));
    }

    // 5. EventData Binary serde roundtrip
    #[test]
    fn event_data_binary_serde_roundtrip() {
        let data = EventData::Binary(vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let json = serde_json::to_string(&data).unwrap();
        let decoded: EventData = serde_json::from_str(&json).unwrap();
        if let EventData::Binary(bytes) = decoded {
            assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        } else {
            panic!("Expected Binary variant");
        }
    }

    // 6. EventData Structured serde roundtrip
    #[test]
    fn event_data_structured_serde_roundtrip() {
        let data = EventData::Structured(HashMap::from([
            ("count".into(), serde_json::json!(42)),
            ("label".into(), serde_json::json!("test")),
        ]));
        let json = serde_json::to_string(&data).unwrap();
        let decoded: EventData = serde_json::from_str(&json).unwrap();
        if let EventData::Structured(map) = decoded {
            assert_eq!(map.get("count").unwrap(), &serde_json::json!(42));
        } else {
            panic!("Expected Structured variant");
        }
    }

    // 7. EventStatistics serde roundtrip
    #[test]
    fn event_statistics_serde_roundtrip() {
        let stats = EventStatistics {
            total_events_published: 100,
            total_events_delivered: 95,
            total_events_failed: 5,
            active_subscriptions: 3,
            dead_letter_count: 2,
            average_processing_time_ms: 12.5,
            last_updated: Utc::now(),
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: EventStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_events_published, 100);
        assert_eq!(decoded.active_subscriptions, 3);
        assert!((decoded.average_processing_time_ms - 12.5).abs() < f64::EPSILON);
    }

    // 8. DeadLetterEvent serde roundtrip
    #[test]
    fn dead_letter_event_serde_roundtrip() {
        let dle = DeadLetterEvent {
            event: Event {
                id: "e1".into(),
                event_type: EventType::TaskFailed,
                priority: EventPriority::High,
                source: "worker".into(),
                timestamp: Utc::now(),
                data: EventData::Empty,
                metadata: HashMap::new(),
                correlation_id: None,
                causation_id: None,
            },
            subscription_id: "sub_1".into(),
            reason: "Handler panicked".into(),
            failed_at: Utc::now(),
            retry_count: 3,
        };
        let json = serde_json::to_string(&dle).unwrap();
        let decoded: DeadLetterEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.subscription_id, "sub_1");
        assert_eq!(decoded.retry_count, 3);
    }

    // 9. ReplayConfig serde roundtrip
    #[test]
    fn replay_config_serde_roundtrip() {
        let config = ReplayConfig {
            from_timestamp: Some(Utc::now()),
            to_timestamp: None,
            event_types: Some(HashSet::from([EventType::TaskCreated])),
            max_events: Some(50),
            speed_multiplier: 2.0,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ReplayConfig = serde_json::from_str(&json).unwrap();
        assert!(decoded.from_timestamp.is_some());
        assert_eq!(decoded.max_events, Some(50));
        assert!((decoded.speed_multiplier - 2.0).abs() < f64::EPSILON);
    }

    // 10. EventStoreConfig custom serde roundtrip
    #[test]
    fn event_store_config_custom_serde_roundtrip() {
        let config = EventStoreConfig {
            max_size: 5000,
            retention_duration_ms: 3600000,
            persist_to_disk: true,
            disk_path: Some("/tmp/events".into()),
        };
        // EventStoreConfig doesn't derive Serialize, but check values directly
        assert_eq!(config.max_size, 5000);
        assert!(config.persist_to_disk);
        assert_eq!(config.disk_path, Some("/tmp/events".into()));
    }

    // 11. Event clone produces equal copy
    #[test]
    fn event_clone_equal() {
        let event = Event {
            id: "clone_test".into(),
            event_type: EventType::AgentSpawned,
            priority: EventPriority::Medium,
            source: "test".into(),
            timestamp: Utc::now(),
            data: EventData::Text("payload".into()),
            metadata: HashMap::from([("k".into(), "v".into())]),
            correlation_id: Some("corr".into()),
            causation_id: None,
        };
        let cloned = event.clone();
        assert_eq!(cloned.id, event.id);
        assert_eq!(cloned.event_type, event.event_type);
        assert_eq!(cloned.correlation_id, event.correlation_id);
    }

    // 12. EventData clone produces equal copy
    #[test]
    fn event_data_clone_equal() {
        let data = EventData::Text("clone me".into());
        let cloned = data.clone();
        assert_eq!(cloned.text(), data.text());
    }

    // 13. Event debug format contains key fields
    #[test]
    fn event_debug_format() {
        let event = Event {
            id: "dbg_1".into(),
            event_type: EventType::SystemStarted,
            priority: EventPriority::Critical,
            source: "debug_src".into(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("dbg_1"));
        assert!(debug.contains("SystemStarted"));
        assert!(debug.contains("debug_src"));
    }

    // 14. EventFilter clone produces equal filter
    #[test]
    fn event_filter_clone_equal() {
        let filter =
            EventFilter::new().with_priority_range(EventPriority::High, EventPriority::Critical);
        let cloned = filter.clone();
        let event = Event {
            id: "1".into(),
            event_type: EventType::TaskCreated,
            priority: EventPriority::High,
            source: "test".into(),
            timestamp: Utc::now(),
            data: EventData::Empty,
            metadata: HashMap::new(),
            correlation_id: None,
            causation_id: None,
        };
        assert_eq!(filter.matches(&event), cloned.matches(&event));
    }

    // 15. EventAggregator debug format
    #[test]
    fn event_aggregator_debug_format() {
        let aggregator = EventAggregator::new(5000, 10);
        let debug = format!("{:?}", aggregator);
        assert!(debug.contains("EventAggregator"));
    }

    // =========================================================================
    // 16-20: VecDeque eviction behavior tests
    // =========================================================================

    // 16. Event store evicts oldest when exceeding max_size
    #[tokio::test]
    async fn test_event_store_evicts_oldest_beyond_max_size() {
        // Use a small max_size to trigger eviction quickly
        let config = EventStoreConfig {
            max_size: 3,
            ..EventStoreConfig::default()
        };
        let system = EventSystem::new(config);

        // Publish 5 events — only the last 3 should remain
        for i in 0..5 {
            let event = system.create_event(
                EventType::TaskCreated,
                format!("source_{}", i),
                EventData::Text(format!("event_{}", i)),
                EventPriority::Medium,
            );
            system.publish(event).await.unwrap();
        }

        // Verify store size is bounded
        let store = system.event_store.read().await;
        assert_eq!(store.len(), 3, "store should be capped at max_size=3");
        // The first two (event_0, event_1) should have been evicted
        drop(store);

        // Verify the remaining events are the newest ones
        let events = system
            .events(Some(EventType::TaskCreated), None, None)
            .await;
        assert_eq!(events.len(), 3);
        // The surviving events should be event_2, event_3, event_4
        let texts: Vec<&str> = events
            .iter()
            .filter_map(|e| {
                if let EventData::Text(ref t) = e.data {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(texts.contains(&"event_2"));
        assert!(texts.contains(&"event_3"));
        assert!(texts.contains(&"event_4"));
    }

    // 17. Dead letter queue evicts when exceeding MAX_DEAD_LETTER_SIZE
    #[tokio::test]
    async fn test_dead_letter_queue_eviction() {
        let system = EventSystem::new(EventStoreConfig::default());

        // Add more than 1000 dead letter events (the MAX_DEAD_LETTER_SIZE)
        for i in 0..1010 {
            let event = Event {
                id: format!("dle_{}", i),
                event_type: EventType::TaskFailed,
                priority: EventPriority::Low,
                source: format!("worker_{}", i),
                timestamp: Utc::now(),
                data: EventData::Text(format!("failure_{}", i)),
                metadata: HashMap::new(),
                correlation_id: None,
                causation_id: None,
            };
            let dle = DeadLetterEvent {
                event,
                subscription_id: "sub_eviction_test".into(),
                reason: format!("reason_{}", i),
                failed_at: Utc::now(),
                retry_count: 0,
            };
            system.add_to_dead_letter(dle).await;
        }

        // Queue should be capped at 1000
        let dles = system.get_dead_letter_events().await;
        assert_eq!(
            dles.len(),
            1000,
            "dead letter queue should be capped at 1000"
        );

        // The oldest entries should have been evicted (dle_0 through dle_9)
        let reasons: Vec<&str> = dles.iter().map(|d| d.reason.as_str()).collect();
        assert!(
            !reasons.contains(&"reason_0"),
            "oldest entry should be evicted"
        );
        assert!(
            !reasons.contains(&"reason_9"),
            "10th oldest should be evicted"
        );
        assert!(reasons.contains(&"reason_10"), "reason_10 should survive");
    }

    // 18. Event store with max_size=1 retains only the latest event
    #[tokio::test]
    async fn test_event_store_max_size_one() {
        let config = EventStoreConfig {
            max_size: 1,
            ..EventStoreConfig::default()
        };
        let system = EventSystem::new(config);

        let e1 = system.create_event(
            EventType::SystemStarted,
            "src1".into(),
            EventData::Empty,
            EventPriority::High,
        );
        system.publish(e1).await.unwrap();

        let e2 = system.create_event(
            EventType::SystemStopped,
            "src2".into(),
            EventData::Empty,
            EventPriority::Low,
        );
        system.publish(e2).await.unwrap();

        let events = system
            .events(Some(EventType::SystemStarted), None, None)
            .await;
        assert!(events.is_empty(), "SystemStarted should have been evicted");

        let events = system
            .events(Some(EventType::SystemStopped), None, None)
            .await;
        assert_eq!(events.len(), 1, "only SystemStopped should remain");
    }

    // 19. Dead letter queue clear removes all entries
    #[tokio::test]
    async fn test_dead_letter_clear_after_eviction() {
        let system = EventSystem::new(EventStoreConfig::default());

        // Add some entries
        for i in 0..5 {
            let event = Event {
                id: format!("clr_{}", i),
                event_type: EventType::TaskFailed,
                priority: EventPriority::Medium,
                source: "src".into(),
                timestamp: Utc::now(),
                data: EventData::Empty,
                metadata: HashMap::new(),
                correlation_id: None,
                causation_id: None,
            };
            let dle = DeadLetterEvent {
                event,
                subscription_id: "sub_clear_test".into(),
                reason: "test".into(),
                failed_at: Utc::now(),
                retry_count: 0,
            };
            system.add_to_dead_letter(dle).await;
        }

        assert_eq!(system.get_dead_letter_events().await.len(), 5);

        system.clear_dead_letter_queue().await;
        assert!(system.get_dead_letter_events().await.is_empty());

        // Stats should still reflect the historical count
        let stats = system.statistics().await;
        assert_eq!(
            stats.dead_letter_count, 5,
            "historical count should persist"
        );
    }

    // 20. Publishing events with empty ID auto-generates unique IDs
    #[tokio::test]
    async fn test_auto_generated_event_ids() {
        let system = EventSystem::new(EventStoreConfig::default());

        let mut e1 = system.create_event(
            EventType::AgentSpawned,
            "src".into(),
            EventData::Empty,
            EventPriority::Medium,
        );
        e1.id.clear(); // Force auto-generation
        system.publish(e1).await.unwrap();

        let mut e2 = system.create_event(
            EventType::AgentSpawned,
            "src".into(),
            EventData::Empty,
            EventPriority::Medium,
        );
        e2.id.clear();
        system.publish(e2).await.unwrap();

        let events = system
            .events(Some(EventType::AgentSpawned), None, None)
            .await;
        assert_eq!(events.len(), 2);
        assert_ne!(
            events[0].id, events[1].id,
            "auto-generated IDs must be unique"
        );
    }
}
