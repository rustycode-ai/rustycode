// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Cross-crate event flow integration tests
//!
//! This test suite validates that events flow correctly between crates:
//! - rustycode-tools publishes events when tools are executed
//! - rustycode-storage subscribes and persists events
//! - rustycode-runtime subscribes to events for logging
//! - Wildcard subscriptions work correctly
//! - Events are delivered with proper correlation

use rustycode_bus::{SessionStartedEvent, ToolExecutedEvent};
use rustycode_protocol::{SessionId, ToolCall};
use rustycode_runtime::AsyncRuntime;
use rustycode_storage::{EventSubscriber, Storage};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::time::{Duration, timeout};

/// Test helper to create a temporary runtime environment
struct TestEnvironment {
    _temp_dir: TempDir,
    runtime: AsyncRuntime,
    storage: Storage,
}

impl TestEnvironment {
    async fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cwd = temp_dir.path();

        // Create necessary directories
        std::fs::create_dir_all(cwd.join("data")).expect("Failed to create data dir");
        std::fs::create_dir_all(cwd.join("skills")).expect("Failed to create skills dir");
        std::fs::create_dir_all(cwd.join("memory")).expect("Failed to create memory dir");

        // Create config file
        let config_content = format!(
            r#"
data_dir = "{}"
skills_dir = "{}"
memory_dir = "{}"
lsp_servers = []
"#,
            cwd.join("data").display(),
            cwd.join("skills").display(),
            cwd.join("memory").display()
        );
        std::fs::write(cwd.join(".rustycode.toml"), config_content)
            .expect("Failed to write config");

        // Initialize runtime
        let runtime = AsyncRuntime::load(cwd)
            .await
            .expect("Failed to load runtime");

        // Initialize storage
        let storage =
            Storage::open(&cwd.join("data").join("rustycode.db")).expect("Failed to open storage");

        Self {
            _temp_dir: temp_dir,
            runtime,
            storage,
        }
    }

    fn path(&self) -> &Path {
        self._temp_dir.path()
    }
}

#[tokio::test]
async fn test_tool_execution_publishes_events() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Subscribe to tool execution events
    let (_id, mut rx) = bus
        .subscribe("tool.executed")
        .await
        .expect("Failed to subscribe to tool events");

    // Execute a tool
    let session_id = SessionId::new();
    let call = ToolCall {
        call_id: uuid::Uuid::new_v4().to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({ "path": ".rustycode.toml" }),
    };

    let _result = env
        .runtime
        .execute_tool(&session_id, call, env.path())
        .await
        .expect("Tool execution failed");

    // Verify event was received
    let event = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("No event received");

    assert_eq!(event.event_type(), "tool.executed");

    // Verify event data
    let event_data = event.serialize();
    assert_eq!(event_data["tool_name"], "read_file");
    assert_eq!(event_data["session_id"], session_id.to_string());
    assert!(event_data["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_wildcard_subscription_receives_all_events() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Subscribe to all events using wildcard
    let (_id, mut rx) = bus
        .subscribe("*")
        .await
        .expect("Failed to subscribe to all events");

    // Publish multiple different events
    let session_id = SessionId::new();

    bus.publish(SessionStartedEvent::new(
        session_id.clone(),
        "Test task".to_string(),
        "Test detail".to_string(),
    ))
    .await
    .expect("Failed to publish session event");

    bus.publish(ToolExecutedEvent::new(
        session_id.clone(),
        "test_tool".to_string(),
        serde_json::json!({}),
        true,
        "success".to_string(),
        None,
    ))
    .await
    .expect("Failed to publish tool event");

    // Verify both events were received
    let event1 = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event1")
        .expect("No event received");
    assert_eq!(event1.event_type(), "session.started");

    let event2 = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event2")
        .expect("No event received");
    assert_eq!(event2.event_type(), "tool.executed");
}

#[tokio::test]
async fn test_session_wildcard_subscription() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Subscribe to all session events using wildcard
    let (_id, mut rx) = bus
        .subscribe("session.*")
        .await
        .expect("Failed to subscribe to session events");

    let session_id = SessionId::new();

    // Publish session started event
    bus.publish(SessionStartedEvent::new(
        session_id.clone(),
        "Test task".to_string(),
        "Test detail".to_string(),
    ))
    .await
    .expect("Failed to publish session event");

    // Publish tool event (should not be received)
    bus.publish(ToolExecutedEvent::new(
        session_id.clone(),
        "test_tool".to_string(),
        serde_json::json!({}),
        true,
        "success".to_string(),
        None,
    ))
    .await
    .expect("Failed to publish tool event");

    // Verify only session event was received
    let event = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("No event received");
    assert_eq!(event.event_type(), "session.started");

    // Verify no more events (tool event should not be received)
    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_multiple_subscribers_receive_same_event() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Create multiple subscribers
    let (_id1, mut rx1) = bus
        .subscribe("tool.executed")
        .await
        .expect("Failed to create subscriber 1");

    let (_id2, mut rx2) = bus
        .subscribe("tool.executed")
        .await
        .expect("Failed to create subscriber 2");

    let (_id3, mut rx3) = bus
        .subscribe("*")
        .await
        .expect("Failed to create subscriber 3");

    // Publish a single event
    let session_id = SessionId::new();
    bus.publish(ToolExecutedEvent::new(
        session_id,
        "test_tool".to_string(),
        serde_json::json!({}),
        true,
        "success".to_string(),
        None,
    ))
    .await
    .expect("Failed to publish event");

    // All subscribers should receive the event
    let event1 = timeout(Duration::from_secs(1), rx1.recv())
        .await
        .expect("Timeout waiting for subscriber 1")
        .expect("Subscriber 1 did not receive event");
    assert_eq!(event1.event_type(), "tool.executed");

    let event2 = timeout(Duration::from_secs(1), rx2.recv())
        .await
        .expect("Timeout waiting for subscriber 2")
        .expect("Subscriber 2 did not receive event");
    assert_eq!(event2.event_type(), "tool.executed");

    let event3 = timeout(Duration::from_secs(1), rx3.recv())
        .await
        .expect("Timeout waiting for subscriber 3")
        .expect("Subscriber 3 did not receive event");
    assert_eq!(event3.event_type(), "tool.executed");
}

#[tokio::test]
async fn test_storage_persists_events_from_bus() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Subscribe storage to the event bus via EventSubscriber
    let storage =
        Storage::open(&env.path().join("data").join("events.db")).expect("Failed to open storage");
    let subscriber = EventSubscriber::new(storage, bus.clone());
    subscriber
        .start()
        .await
        .expect("Failed to start event subscriber");

    // Publish some events
    let session_id = SessionId::new();

    bus.publish(SessionStartedEvent::new(
        session_id.clone(),
        "Test task".to_string(),
        "Test detail".to_string(),
    ))
    .await
    .expect("Failed to publish session event");

    bus.publish(ToolExecutedEvent::new(
        session_id.clone(),
        "test_tool".to_string(),
        serde_json::json!({}),
        true,
        "success".to_string(),
        None,
    ))
    .await
    .expect("Failed to publish tool event");

    // Give time for events to be persisted
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Open a second storage connection to read events (SQLite supports multiple readers)
    let db_path = env.path().join("data").join("events.db");
    let read_storage = Storage::open(&db_path).expect("Failed to open read storage");

    // Retrieve events from storage
    let events = read_storage
        .get_events(10)
        .expect("Failed to get events from storage");

    // Verify events were persisted
    assert!(
        events.len() >= 2,
        "Expected at least 2 events, got {}",
        events.len()
    );

    // Verify event types
    let event_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(event_types.contains(&"session.started"));
    assert!(event_types.contains(&"tool.executed"));
}

#[tokio::test]
async fn test_event_hooks_fire_correctly() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    let hook_count = Arc::new(AtomicUsize::new(0));
    let hook_count_clone = hook_count.clone();

    // Register a custom hook
    bus.register_hook(rustycode_bus::HookPhase::PrePublish, move |_event| {
        hook_count_clone.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .await;

    // Publish an event
    let session_id = SessionId::new();
    bus.publish(SessionStartedEvent::new(
        session_id,
        "Test task".to_string(),
        "Test detail".to_string(),
    ))
    .await
    .expect("Failed to publish event");

    // Give hook time to execute
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Verify hook was called
    assert_eq!(hook_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_event_metrics_tracking() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Get initial metrics
    let initial_metrics = bus.metrics();
    let initial_published = initial_metrics.events_published;

    // Publish some events
    let session_id = SessionId::new();

    for i in 0..5 {
        bus.publish(ToolExecutedEvent::new(
            session_id.clone(),
            format!("tool_{}", i),
            serde_json::json!({}),
            true,
            "success".to_string(),
            None,
        ))
        .await
        .expect("Failed to publish event");
    }

    // Get updated metrics
    let updated_metrics = bus.metrics();

    // Verify metrics increased
    assert_eq!(
        updated_metrics.events_published,
        initial_published + 5,
        "Event count should increase by 5"
    );
}

#[tokio::test]
async fn test_cross_crate_event_flow_integration() {
    // This is the comprehensive integration test that validates
    // the complete event flow across all crates

    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Subscribe to track all events
    let (_id, mut rx) = bus
        .subscribe("*")
        .await
        .expect("Failed to subscribe to all events");

    let event_count = Arc::new(AtomicUsize::new(0));
    let event_count_clone = event_count.clone();

    // Spawn task to count events
    tokio::spawn(async move {
        while let Ok(_event) = rx.recv().await {
            event_count_clone.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Execute a tool (should publish event via rustycode-tools)
    let session_id = SessionId::new();
    let call = ToolCall {
        call_id: uuid::Uuid::new_v4().to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({ "path": ".rustycode.toml" }),
    };

    let _result = env
        .runtime
        .execute_tool(&session_id, call, env.path())
        .await
        .expect("Tool execution failed");

    // Give time for events to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify at least one event was published
    let count = event_count.load(Ordering::SeqCst);
    assert!(
        count >= 1,
        "Expected at least 1 event to be published, got {}",
        count
    );

    // Verify events were persisted to storage
    let events = env
        .storage
        .get_events(10)
        .expect("Failed to get events from storage");
    assert!(
        !events.is_empty(),
        "Expected events to be persisted to storage"
    );

    // Verify metrics were updated
    let metrics = bus.metrics();
    assert!(
        metrics.events_published > 0,
        "Expected metrics to show published events"
    );
}

#[tokio::test]
async fn test_event_ordering_preserved() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    let (_id, mut rx) = bus
        .subscribe("*")
        .await
        .expect("Failed to subscribe to all events");

    let session_id = SessionId::new();

    // Publish events in a specific order
    let tool_names = vec!["tool_a", "tool_b", "tool_c"];

    for tool_name in &tool_names {
        bus.publish(ToolExecutedEvent::new(
            session_id.clone(),
            tool_name.to_string(),
            serde_json::json!({}),
            true,
            "success".to_string(),
            None,
        ))
        .await
        .expect("Failed to publish event");
    }

    // Verify events are received in the same order
    for expected_tool in &tool_names {
        let event = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("Timeout waiting for event")
            .expect("No event received");

        let event_data = event.serialize();
        let actual_tool = event_data["tool_name"]
            .as_str()
            .expect("Tool name not found in event");

        assert_eq!(actual_tool, *expected_tool);
    }
}

#[tokio::test]
async fn test_error_handling_in_event_delivery() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Register a hook that always fails
    bus.register_hook(rustycode_bus::HookPhase::PrePublish, |_event| {
        Err::<(), _>("Hook error".into())
    })
    .await;

    // Publishing should still succeed despite hook failure
    let session_id = SessionId::new();
    let result = bus
        .publish(SessionStartedEvent::new(
            session_id,
            "Test task".to_string(),
            "Test detail".to_string(),
        ))
        .await;

    assert!(result.is_ok(), "Publish should succeed even if hook fails");
}

#[tokio::test]
async fn test_unsubscription_prevents_event_delivery() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Subscribe and then immediately unsubscribe
    let (id, _rx) = bus
        .subscribe("tool.executed")
        .await
        .expect("Failed to subscribe");

    bus.unsubscribe(id).await.expect("Failed to unsubscribe");

    // Publish an event
    let session_id = SessionId::new();
    bus.publish(ToolExecutedEvent::new(
        session_id,
        "test_tool".to_string(),
        serde_json::json!({}),
        true,
        "success".to_string(),
        None,
    ))
    .await
    .expect("Failed to publish event");

    // Give time for event delivery
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify subscriber count decreased
    let metrics = bus.metrics();
    assert_eq!(metrics.subscriber_count, 0, "Should have no subscribers");
}
