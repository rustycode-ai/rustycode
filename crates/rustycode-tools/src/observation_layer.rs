//! Tracing Observation Layer
//!
//! A `tracing_subscriber::Layer` implementation that captures span and event
//! data for observability platforms (Langfuse, Jaeger, etc.). Provides
//! structured span tracking with UUID-based IDs, batch event submission,
//! and a pluggable `BatchManager` trait for different backends.
//!
//! Inspired by goose's `tracing/observation_layer.rs`.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::observation_layer::{ObservationLayer, ConsoleBatchManager};
//! use tracing_subscriber::registry().with(layer);
//!
//! let manager = ConsoleBatchManager::new();
//! let layer = ObservationLayer::new(Arc::new(Mutex::new(manager)));
//! tracing_subscriber::registry().with(layer).init();
//! ```

use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::{span, Event, Id, Level, Metadata, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;
use uuid::Uuid;

/// Data captured when a new span is created.
#[derive(Debug, Clone)]
pub struct SpanData {
    /// Unique observation ID (UUID v4)
    pub observation_id: String,
    /// Span name
    pub name: String,
    /// ISO 8601 start time
    pub start_time: String,
    /// Log level string
    pub level: String,
    /// Additional metadata fields
    pub metadata: serde_json::Map<String, Value>,
    /// Parent span ID (tracing u64)
    pub parent_span_id: Option<u64>,
}

/// Map tracing level to observability level strings.
pub const fn map_level(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARNING",
        Level::INFO => "DEFAULT",
        Level::DEBUG => "DEBUG",
        Level::TRACE => "DEBUG",
    }
}

/// Flatten nested metadata, extracting `.text` from objects.
pub fn flatten_metadata(
    metadata: serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let mut flattened = serde_json::Map::new();
    for (key, value) in metadata {
        match value {
            Value::String(s) => {
                flattened.insert(key, json!(s));
            }
            Value::Object(mut obj) => {
                if let Some(text) = obj.remove("text") {
                    flattened.insert(key, text);
                } else {
                    flattened.insert(key, json!(obj));
                }
            }
            _ => {
                flattened.insert(key, value);
            }
        }
    }
    flattened
}

/// Trait for batch event submission backends.
///
/// Implement this to send observation events to your preferred
/// observability platform (Langfuse, Jaeger, Honeycomb, etc.).
pub trait BatchManager: Send + Sync + 'static {
    /// Add an event to the batch.
    fn add_event(&mut self, event_type: &str, body: Value);
    /// Flush the batch to the backend.
    fn send(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    /// Check if the batch is empty.
    fn is_empty(&self) -> bool;
}

/// Tracks active spans and the current trace ID.
#[derive(Debug)]
pub struct SpanTracker {
    active_spans: HashMap<u64, String>,
    current_trace_id: Option<String>,
}

impl Default for SpanTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SpanTracker {
    /// Create a new empty span tracker.
    pub fn new() -> Self {
        Self {
            active_spans: HashMap::new(),
            current_trace_id: None,
        }
    }

    /// Add a span to the tracker.
    pub fn add_span(&mut self, span_id: u64, observation_id: String) {
        self.active_spans.insert(span_id, observation_id);
    }

    /// Get a span's observation ID.
    pub fn get_span(&self, span_id: u64) -> Option<&String> {
        self.active_spans.get(&span_id)
    }

    /// Remove a span from the tracker, returning its observation ID.
    pub fn remove_span(&mut self, span_id: u64) -> Option<String> {
        self.active_spans.remove(&span_id)
    }
}

/// A `tracing_subscriber::Layer` that captures spans/events for observability.
#[derive(Clone)]
pub struct ObservationLayer {
    /// Batch manager for submitting events
    pub batch_manager: Arc<Mutex<dyn BatchManager>>,
    /// Span tracker for correlating spans with observation IDs
    pub span_tracker: Arc<Mutex<SpanTracker>>,
}

impl ObservationLayer {
    /// Create a new observation layer with the given batch manager.
    pub fn new(batch_manager: Arc<Mutex<dyn BatchManager>>) -> Self {
        Self {
            batch_manager,
            span_tracker: Arc::new(Mutex::new(SpanTracker::new())),
        }
    }

    /// Handle a new span being created.
    pub async fn handle_span(&self, span_id: u64, span_data: SpanData) {
        let observation_id = span_data.observation_id.clone();

        {
            let mut spans = self.span_tracker.lock().await;
            spans.add_span(span_id, observation_id.clone());
        }

        let parent_id = if let Some(parent_span_id) = span_data.parent_span_id {
            let spans = self.span_tracker.lock().await;
            spans.get_span(parent_span_id).cloned()
        } else {
            None
        };

        let trace_id = self.ensure_trace_id().await;

        let mut batch = self.batch_manager.lock().await;
        batch.add_event(
            "observation-create",
            json!({
                "id": observation_id,
                "traceId": trace_id,
                "type": "SPAN",
                "name": span_data.name,
                "startTime": span_data.start_time,
                "parentObservationId": parent_id,
                "metadata": span_data.metadata,
                "level": span_data.level
            }),
        );
    }

    /// Handle a span being closed.
    pub async fn handle_span_close(&self, span_id: u64) {
        let observation_id = {
            let mut spans = self.span_tracker.lock().await;
            spans.remove_span(span_id)
        };

        if let Some(observation_id) = observation_id {
            let trace_id = self.ensure_trace_id().await;
            let mut batch = self.batch_manager.lock().await;
            batch.add_event(
                "observation-update",
                json!({
                    "id": observation_id,
                    "type": "SPAN",
                    "traceId": trace_id,
                    "endTime": Utc::now().to_rfc3339()
                }),
            );
        }
    }

    /// Ensure a trace ID exists, creating one if needed.
    pub async fn ensure_trace_id(&self) -> String {
        let mut spans = self.span_tracker.lock().await;
        if let Some(id) = spans.current_trace_id.clone() {
            return id;
        }

        let trace_id = Uuid::new_v4().to_string();
        spans.current_trace_id = Some(trace_id.clone());

        let mut batch = self.batch_manager.lock().await;
        batch.add_event(
            "trace-create",
            json!({
                "id": trace_id,
                "name": Utc::now().timestamp().to_string(),
                "timestamp": Utc::now().to_rfc3339(),
                "input": {},
                "metadata": {},
                "tags": [],
                "public": false
            }),
        );

        trace_id
    }

    /// Update the current trace with additional fields.
    pub async fn update_trace(&self, updates: serde_json::Map<String, Value>) {
        let trace_id = self.ensure_trace_id().await;
        let mut body = json!({ "id": trace_id });
        for (k, v) in updates {
            body[k] = v;
        }
        let mut batch = self.batch_manager.lock().await;
        batch.add_event("trace-create", body);
    }

    /// Handle a record (field values) being attached to a span.
    pub async fn handle_record(&self, span_id: u64, metadata: serde_json::Map<String, Value>) {
        let trace_fields: Vec<&str> = vec!["trace_input", "trace_output"];
        let has_trace_fields = trace_fields.iter().any(|f| metadata.contains_key(*f));

        if has_trace_fields {
            let mut trace_updates = serde_json::Map::new();
            if let Some(val) = metadata.get("trace_input") {
                trace_updates.insert("input".to_string(), val.clone());
            }
            if let Some(val) = metadata.get("trace_output") {
                trace_updates.insert("output".to_string(), val.clone());
            }
            if !trace_updates.is_empty() {
                self.update_trace(trace_updates).await;
            }
        }

        let span_metadata: serde_json::Map<String, Value> = metadata
            .into_iter()
            .filter(|(k, _)| !trace_fields.contains(&k.as_str()))
            .collect();

        if span_metadata.is_empty() {
            return;
        }

        let observation_id = {
            let spans = self.span_tracker.lock().await;
            spans.get_span(span_id).cloned()
        };

        if let Some(observation_id) = observation_id {
            let trace_id = self.ensure_trace_id().await;

            let mut update = json!({
                "id": observation_id,
                "traceId": trace_id,
                "type": "SPAN"
            });

            if let Some(val) = span_metadata.get("input") {
                update["input"] = val.clone();
            }

            if let Some(val) = span_metadata.get("output") {
                update["output"] = val.clone();
            }

            if let Some(val) = span_metadata.get("model_config") {
                update["metadata"] = json!({ "model_config": val });
            }

            let remaining_metadata: serde_json::Map<String, Value> = span_metadata
                .iter()
                .filter(|(k, _)| !["input", "output", "model_config"].contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            if !remaining_metadata.is_empty() {
                let flattened = flatten_metadata(remaining_metadata);
                if update.get("metadata").is_some() {
                    if let Some(obj) = update["metadata"].as_object_mut() {
                        for (k, v) in flattened {
                            obj.insert(k, v);
                        }
                    }
                } else {
                    update["metadata"] = json!(flattened);
                }
            }

            let mut batch = self.batch_manager.lock().await;
            batch.add_event("span-update", update);
        }
    }
}

impl<S> Layer<S> for ObservationLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        // Only capture spans from our crate family
        metadata.target().starts_with("rustycode")
    }

    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span_id = id.into_u64();

        let parent_span_id = ctx
            .span_scope(id)
            .and_then(|mut scope| scope.nth(1))
            .map(|parent| parent.id().into_u64());

        let mut visitor = JsonVisitor::new();
        attrs.record(&mut visitor);

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: attrs.metadata().name().to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: map_level(attrs.metadata().level()).to_owned(),
            metadata: visitor.recorded_fields,
            parent_span_id,
        };

        let layer = self.clone();
        tokio::spawn(async move { layer.handle_span(span_id, span_data).await });
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        let span_id = id.into_u64();
        let layer = self.clone();
        tokio::spawn(async move { layer.handle_span_close(span_id).await });
    }

    fn on_record(&self, span: &Id, values: &span::Record<'_>, _ctx: Context<'_, S>) {
        let span_id = span.into_u64();
        let mut visitor = JsonVisitor::new();
        values.record(&mut visitor);
        let metadata = visitor.recorded_fields;

        if !metadata.is_empty() {
            let layer = self.clone();
            tokio::spawn(async move { layer.handle_record(span_id, metadata).await });
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = JsonVisitor::new();
        event.record(&mut visitor);
        let metadata = visitor.recorded_fields;

        if let Some(span_id) = ctx.lookup_current().map(|span| span.id().into_u64()) {
            let layer = self.clone();
            tokio::spawn(async move { layer.handle_record(span_id, metadata).await });
        }
    }
}

/// A `tracing::field::Visit` implementation that records fields as JSON.
#[derive(Debug)]
struct JsonVisitor {
    recorded_fields: serde_json::Map<String, Value>,
}

impl JsonVisitor {
    fn new() -> Self {
        Self {
            recorded_fields: serde_json::Map::new(),
        }
    }

    fn insert_value(&mut self, field: &Field, value: Value) {
        self.recorded_fields.insert(field.name().to_string(), value);
    }
}

macro_rules! record_field {
    ($fn_name:ident, $type:ty) => {
        fn $fn_name(&mut self, field: &Field, value: $type) {
            self.insert_value(field, Value::from(value));
        }
    };
}

impl Visit for JsonVisitor {
    record_field!(record_i64, i64);
    record_field!(record_u64, u64);
    record_field!(record_bool, bool);
    record_field!(record_str, &str);

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert_value(field, Value::String(format!("{value:?}")));
    }
}

/// A simple batch manager that logs events to stderr.
/// Useful for development and debugging.
pub struct ConsoleBatchManager {
    events: Vec<(String, Value)>,
}

impl ConsoleBatchManager {
    /// Create a new console batch manager.
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl Default for ConsoleBatchManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchManager for ConsoleBatchManager {
    fn add_event(&mut self, event_type: &str, body: Value) {
        tracing::debug!(event_type, body = %body, "[observation]");
        self.events.push((event_type.to_string(), body));
    }

    fn send(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.events.clear();
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// A batch manager that collects events into a Vec for testing.
pub struct InMemoryBatchManager {
    events: Vec<(String, Value)>,
}

impl InMemoryBatchManager {
    /// Create a new in-memory batch manager.
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Get all collected events.
    pub fn events(&self) -> &[(String, Value)] {
        &self.events
    }

    /// Get events of a specific type.
    pub fn events_of_type(&self, event_type: &str) -> Vec<&Value> {
        self.events
            .iter()
            .filter(|(t, _)| t == event_type)
            .map(|(_, v)| v)
            .collect()
    }
}

impl Default for InMemoryBatchManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchManager for InMemoryBatchManager {
    fn add_event(&mut self, event_type: &str, body: Value) {
        self.events.push((event_type.to_string(), body));
    }

    fn send(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_tracker_new() {
        let tracker = SpanTracker::new();
        assert!(tracker.active_spans.is_empty());
        assert!(tracker.current_trace_id.is_none());
    }

    #[test]
    fn test_span_tracker_add_get_remove() {
        let mut tracker = SpanTracker::new();
        tracker.add_span(1, "obs-1".to_string());
        tracker.add_span(2, "obs-2".to_string());

        assert_eq!(tracker.get_span(1), Some(&"obs-1".to_string()));
        assert_eq!(tracker.get_span(2), Some(&"obs-2".to_string()));
        assert_eq!(tracker.get_span(99), None);

        let removed = tracker.remove_span(1);
        assert_eq!(removed, Some("obs-1".to_string()));
        assert_eq!(tracker.get_span(1), None);
    }

    #[test]
    fn test_map_level() {
        assert_eq!(map_level(&Level::ERROR), "ERROR");
        assert_eq!(map_level(&Level::WARN), "WARNING");
        assert_eq!(map_level(&Level::INFO), "DEFAULT");
        assert_eq!(map_level(&Level::DEBUG), "DEBUG");
        assert_eq!(map_level(&Level::TRACE), "DEBUG");
    }

    #[test]
    fn test_flatten_metadata_strings() {
        let mut meta = serde_json::Map::new();
        meta.insert("key".to_string(), json!("value"));

        let flat = flatten_metadata(meta);
        assert_eq!(flat["key"], "value");
    }

    #[test]
    fn test_flatten_metadata_object_with_text() {
        let mut meta = serde_json::Map::new();
        meta.insert("complex".to_string(), json!({"text": "inner"}));

        let flat = flatten_metadata(meta);
        assert_eq!(flat["complex"], "inner");
    }

    #[test]
    fn test_flatten_metadata_object_without_text() {
        let mut meta = serde_json::Map::new();
        meta.insert("obj".to_string(), json!({"a": 1, "b": 2}));

        let flat = flatten_metadata(meta);
        assert_eq!(flat["obj"]["a"], 1);
    }

    #[test]
    fn test_flatten_metadata_number() {
        let mut meta = serde_json::Map::new();
        meta.insert("count".to_string(), json!(42));

        let flat = flatten_metadata(meta);
        assert_eq!(flat["count"], 42);
    }

    #[tokio::test]
    async fn test_observation_layer_handle_span() {
        let manager = Arc::new(Mutex::new(InMemoryBatchManager::new()));
        let layer = ObservationLayer::new(manager.clone());

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: "test_span".to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: "DEFAULT".to_string(),
            metadata: serde_json::Map::new(),
            parent_span_id: None,
        };

        layer.handle_span(1, span_data.clone()).await;

        let events = manager.lock().await;
        assert_eq!(events.events().len(), 2); // trace-create + observation-create

        let obs_events = events.events_of_type("observation-create");
        assert_eq!(obs_events.len(), 1);
        assert_eq!(obs_events[0]["id"], span_data.observation_id);
        assert_eq!(obs_events[0]["name"], "test_span");
        assert_eq!(obs_events[0]["type"], "SPAN");
    }

    #[tokio::test]
    async fn test_observation_layer_span_close() {
        let manager = Arc::new(Mutex::new(InMemoryBatchManager::new()));
        let layer = ObservationLayer::new(manager.clone());

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: "test_span".to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: "DEFAULT".to_string(),
            metadata: serde_json::Map::new(),
            parent_span_id: None,
        };

        layer.handle_span(1, span_data.clone()).await;
        layer.handle_span_close(1).await;

        let events = manager.lock().await;
        assert_eq!(events.events().len(), 3); // trace-create + observation-create + observation-update

        let updates = events.events_of_type("observation-update");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0]["id"], span_data.observation_id);
        assert!(updates[0]["endTime"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_observation_layer_handle_record() {
        let manager = Arc::new(Mutex::new(InMemoryBatchManager::new()));
        let layer = ObservationLayer::new(manager.clone());

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: "test_span".to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: "DEFAULT".to_string(),
            metadata: serde_json::Map::new(),
            parent_span_id: None,
        };

        layer.handle_span(1, span_data).await;

        let mut metadata = serde_json::Map::new();
        metadata.insert("input".to_string(), json!("test input"));
        metadata.insert("output".to_string(), json!("test output"));
        metadata.insert("custom_field".to_string(), json!("custom value"));

        layer.handle_record(1, metadata).await;

        let events = manager.lock().await;
        assert_eq!(events.events().len(), 3); // trace-create + observation-create + span-update

        let updates = events.events_of_type("span-update");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0]["input"], "test input");
        assert_eq!(updates[0]["output"], "test output");
        assert_eq!(updates[0]["metadata"]["custom_field"], "custom value");
    }

    #[tokio::test]
    async fn test_trace_input_output_updates() {
        let manager = Arc::new(Mutex::new(InMemoryBatchManager::new()));
        let layer = ObservationLayer::new(manager.clone());

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: "test_span".to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: "DEFAULT".to_string(),
            metadata: serde_json::Map::new(),
            parent_span_id: None,
        };

        layer.handle_span(1, span_data).await;

        let mut metadata = serde_json::Map::new();
        metadata.insert("trace_input".to_string(), json!("hello from user"));
        metadata.insert("trace_output".to_string(), json!("response from assistant"));

        layer.handle_record(1, metadata).await;

        let events = manager.lock().await;
        assert!(events.events().len() >= 3);

        let trace_updates: Vec<_> = events
            .events()
            .iter()
            .filter(|(t, b)| t == "trace-create" && b.get("input").is_some_and(|v| v.is_string()))
            .collect();
        assert_eq!(trace_updates.len(), 1);
        assert_eq!(trace_updates[0].1["input"], "hello from user");
        assert_eq!(trace_updates[0].1["output"], "response from assistant");
    }

    #[tokio::test]
    async fn test_trace_fields_not_sent_as_span_metadata() {
        let manager = Arc::new(Mutex::new(InMemoryBatchManager::new()));
        let layer = ObservationLayer::new(manager.clone());

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: "test_span".to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: "DEFAULT".to_string(),
            metadata: serde_json::Map::new(),
            parent_span_id: None,
        };

        layer.handle_span(1, span_data).await;

        let mut metadata = serde_json::Map::new();
        metadata.insert("trace_input".to_string(), json!("user msg"));

        layer.handle_record(1, metadata).await;

        let events = manager.lock().await;
        let span_updates: Vec<_> = events.events_of_type("span-update");
        assert!(
            span_updates.is_empty(),
            "trace-only fields should not generate span-update events"
        );
    }

    #[tokio::test]
    async fn test_mixed_trace_and_span_fields() {
        let manager = Arc::new(Mutex::new(InMemoryBatchManager::new()));
        let layer = ObservationLayer::new(manager.clone());

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: "test_span".to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: "DEFAULT".to_string(),
            metadata: serde_json::Map::new(),
            parent_span_id: None,
        };

        layer.handle_span(1, span_data).await;

        let mut metadata = serde_json::Map::new();
        metadata.insert("trace_input".to_string(), json!("user msg"));
        metadata.insert("input".to_string(), json!("tool input"));
        metadata.insert("output".to_string(), json!("tool output"));

        layer.handle_record(1, metadata).await;

        let events = manager.lock().await;

        let trace_updates: Vec<_> = events
            .events()
            .iter()
            .filter(|(t, b)| t == "trace-create" && b.get("input").is_some_and(|v| v.is_string()))
            .collect();
        assert_eq!(trace_updates.len(), 1);
        assert_eq!(trace_updates[0].1["input"], "user msg");

        let span_updates = events.events_of_type("span-update");
        assert_eq!(span_updates.len(), 1);
        assert_eq!(span_updates[0]["input"], "tool input");
        assert_eq!(span_updates[0]["output"], "tool output");
    }

    #[test]
    fn test_console_batch_manager() {
        let mut manager = ConsoleBatchManager::new();
        assert!(manager.is_empty());

        manager.add_event("test", json!({"key": "value"}));
        assert!(!manager.is_empty());

        manager.send().unwrap();
        assert!(manager.is_empty());
    }

    #[test]
    fn test_in_memory_batch_manager() {
        let mut manager = InMemoryBatchManager::new();
        assert!(manager.is_empty());

        manager.add_event("type-a", json!({"a": 1}));
        manager.add_event("type-b", json!({"b": 2}));
        manager.add_event("type-a", json!({"a": 3}));

        assert_eq!(manager.events().len(), 3);
        assert_eq!(manager.events_of_type("type-a").len(), 2);

        manager.send().unwrap();
    }

    #[test]
    fn test_json_visitor() {
        let mut visitor = JsonVisitor::new();

        // Simulate recording fields by directly inserting values
        visitor
            .recorded_fields
            .insert("name".to_string(), json!("test"));
        visitor
            .recorded_fields
            .insert("count".to_string(), json!(42));
        visitor
            .recorded_fields
            .insert("active".to_string(), json!(true));

        assert_eq!(visitor.recorded_fields["name"], "test");
        assert_eq!(visitor.recorded_fields["count"], 42);
        assert_eq!(visitor.recorded_fields["active"], true);
    }

    #[test]
    fn test_span_data_debug() {
        let data = SpanData {
            observation_id: "obs-123".to_string(),
            name: "test_span".to_string(),
            start_time: "2026-01-01T00:00:00Z".to_string(),
            level: "DEBUG".to_string(),
            metadata: serde_json::Map::new(),
            parent_span_id: None,
        };

        let debug_str = format!("{:?}", data);
        assert!(debug_str.contains("obs-123"));
        assert!(debug_str.contains("test_span"));
    }
}
