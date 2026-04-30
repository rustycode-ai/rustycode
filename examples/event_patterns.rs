// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Event Bus Pattern Examples
//!
//! This example demonstrates the key patterns and features of the RustyCode event bus:
//! - Exact subscription matching
//! - Wildcard subscription patterns
//! - Hook-based cross-cutting concerns
//! - Backpressure handling
//! - Metrics and monitoring

use rustycode_bus::{EventBus, EventBusConfig, HookPhase, SessionStartedEvent, ToolExecutedEvent};
use rustycode_protocol::SessionId;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚌 Event Bus Pattern Examples\n");
    println!("============================\n");

    // Example 1: Exact subscription matching
    exact_subscription_example().await?;

    // Example 2: Wildcard subscription patterns
    wildcard_subscription_example().await?;

    // Example 3: Filter chain with hooks
    hooks_example().await?;

    // Example 4: Backpressure handling
    backpressure_example().await?;

    // Example 5: Metrics and monitoring
    metrics_example().await?;

    println!("\n✅ All examples completed successfully!");
    Ok(())
}

/// Example 1: Exact subscription matching
///
/// Shows how to subscribe to specific event types and receive only those events.
async fn exact_subscription_example() -> anyhow::Result<()> {
    println!("1️⃣  Exact Subscription Example");
    println!("   Subscribing to specific event type 'session.started'\n");

    let bus = EventBus::new();

    // Subscribe to a specific event type
    let (_id, mut rx) = bus.subscribe("session.started").await?;

    // Spawn a task to handle received events
    let handle = tokio::spawn(async move {
        match rx.recv().await {
            Ok(event) => {
                println!("   📥 Received event: {}", event.event_type());
                if let Some(session_evt) = event.as_any().downcast_ref::<SessionStartedEvent>() {
                    println!("   📝 Session ID: {}", session_evt.session_id);
                    println!("   📋 Task: {}", session_evt.task);
                }
            }
            Err(e) => println!("   ❌ Error receiving event: {}", e),
        }
    });

    // Publish an event
    let event = SessionStartedEvent::new(
        SessionId::new(),
        "Analyze codebase structure".to_string(),
        "Initial analysis session".to_string(),
    );
    bus.publish(event).await?;

    // Wait for the event to be processed
    sleep(Duration::from_millis(100)).await;
    handle.abort();

    println!("   ✅ Exact subscription completed\n");
    Ok(())
}

/// Example 2: Wildcard subscription patterns
///
/// Shows how to use wildcards to subscribe to multiple event types at once.
async fn wildcard_subscription_example() -> anyhow::Result<()> {
    println!("2️⃣  Wildcard Subscription Example");
    println!("   Demonstrating different wildcard patterns\n");

    let bus = EventBus::new();

    // Subscribe to all session-related events
    let (_id1, mut rx1) = bus.subscribe("session.*").await?;
    println!("   📡 Subscribed to: session.* (all session events)");

    // Subscribe to all error events
    let (_id2, _rx2) = bus.subscribe("*.error").await?;
    println!("   📡 Subscribed to: *.error (all error events)");

    // Subscribe to ALL events
    let (_id3, mut rx3) = bus.subscribe("*").await?;
    println!("   📡 Subscribed to: * (all events)\n");

    // Spawn handlers
    let handle1 = tokio::spawn(async move {
        if let Ok(event) = rx1.recv().await {
            println!("   📥 [session.*] Received: {}", event.event_type());
        }
    });

    let handle3 = tokio::spawn(async move {
        if let Ok(event) = rx3.recv().await {
            println!("   📥 [*] Received: {}", event.event_type());
        }
    });

    // Publish different events
    let event1 = SessionStartedEvent::new(
        SessionId::new(),
        "Test task".to_string(),
        "Testing wildcards".to_string(),
    );
    bus.publish(event1).await?;
    println!("   📤 Published: session.started");

    let event2 = ToolExecutedEvent::new(
        SessionId::new(),
        "read_file".to_string(),
        serde_json::json!({"path": "/test"}),
        true,
        "Success".to_string(),
        None,
    );
    bus.publish(event2).await?;
    println!("   📤 Published: tool.executed\n");

    // Wait for events to be processed
    sleep(Duration::from_millis(100)).await;
    handle1.abort();
    handle3.abort();

    println!("   ✅ Wildcard subscription completed\n");
    Ok(())
}

/// Example 3: Filter chain with hooks
///
/// Shows how to use hooks for cross-cutting concerns like logging, metrics, etc.
async fn hooks_example() -> anyhow::Result<()> {
    println!("3️⃣  Hooks Example (Cross-Cutting Concerns)");
    println!("   Adding logging and metrics hooks\n");

    let bus = EventBus::new();

    // Counter to track hook executions
    let event_count = Arc::new(AtomicUsize::new(0));
    let event_count_clone = event_count.clone();

    // Register a pre-publish hook for logging
    bus.register_hook(HookPhase::PrePublish, move |event| {
        println!("   🔔 [PrePublish] Event: {}", event.event_type());
        event_count_clone.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .await;

    // Register a post-publish hook for metrics
    bus.register_hook(HookPhase::PostPublish, move |event| {
        println!(
            "   📊 [PostPublish] Event: {} published successfully",
            event.event_type()
        );
        Ok(())
    })
    .await;

    // Publish an event (hooks will be triggered)
    let event = SessionStartedEvent::new(
        SessionId::new(),
        "Test hooks".to_string(),
        "Testing hook execution".to_string(),
    );
    bus.publish(event).await?;

    println!(
        "\n   📈 Total events processed: {}",
        event_count.load(Ordering::SeqCst)
    );
    println!("   ✅ Hooks example completed\n");
    Ok(())
}

/// Example 4: Backpressure handling
///
/// Shows how the event bus handles slow consumers through bounded channels.
async fn backpressure_example() -> anyhow::Result<()> {
    println!("4️⃣  Backpressure Handling Example");
    println!("   Demonstrating bounded channel behavior\n");

    // Create a bus with small channel capacity
    let config = EventBusConfig {
        channel_capacity: 5, // Small buffer to demonstrate backpressure
        ..Default::default()
    };
    let bus = EventBus::with_config(config.clone());

    let (_id, mut rx) = bus.subscribe("session.*").await?;

    // Don't consume from the channel - this will fill up the buffer
    println!("   📦 Channel capacity: {}", config.channel_capacity);
    println!("   📤 Publishing 10 events without consuming...\n");

    // Publish more events than the channel can hold
    for i in 1..=10 {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            format!("Task {}", i),
            format!("Task {} details", i),
        );
        match bus.publish(event).await {
            Ok(_) => println!("   ✅ Event {} published", i),
            Err(e) => println!("   ❌ Event {} failed: {}", i, e),
        }
    }

    // Now consume some events
    println!("\n   📥 Consuming events...");
    for i in 1..=3 {
        match rx.recv().await {
            Ok(event) => println!("   📥 Received event {}: {}", i, event.event_type()),
            Err(e) => println!("   ❌ Error receiving: {}", e),
        }
    }

    println!("\n   💡 Key insight: The event bus uses bounded channels to prevent");
    println!("   unbounded memory growth. When channels are full, slow consumers");
    println!("   don't block fast publishers - they just miss events.");
    println!("   ✅ Backpressure example completed\n");

    Ok(())
}

/// Example 5: Metrics and monitoring
///
/// Shows how to collect and analyze event bus metrics.
async fn metrics_example() -> anyhow::Result<()> {
    println!("5️⃣  Metrics and Monitoring Example");
    println!("   Collecting event bus statistics\n");

    let bus = EventBus::new();

    // Create multiple subscribers
    let (_id1, _rx1) = bus.subscribe("session.*").await?;
    let (_id2, _rx2) = bus.subscribe("*").await?;

    // Publish some events
    println!("   📤 Publishing 5 test events...");
    for i in 1..=5 {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            format!("Task {}", i),
            format!("Details {}", i),
        );
        bus.publish(event).await?;
    }

    // Get metrics snapshot
    let metrics = bus.metrics();

    println!("\n   📊 Event Bus Metrics:");
    println!("   ┌─────────────────────────────────");
    println!("   │ Events Published: {}", metrics.events_published);
    println!("   │ Events Delivered: {}", metrics.events_delivered);
    println!("   │ Events Failed:    {}", metrics.events_failed);
    println!("   │ Subscribers:      {}", metrics.subscriber_count);
    println!("   └─────────────────────────────────");

    println!("\n   💡 Metrics help you:");
    println!("   • Track event flow through the system");
    println!("   • Identify bottlenecks and slow consumers");
    println!("   • Monitor system health in production");
    println!("   ✅ Metrics example completed\n");

    Ok(())
}
