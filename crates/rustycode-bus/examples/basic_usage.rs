// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Basic event bus usage example
//!
//! Run with: `cargo run --example basic_usage --package rustycode-bus`

use rustycode_bus::{EventBus, HookPhase, SessionStartedEvent};
use rustycode_protocol::SessionId;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Create event bus with debug logging
    let bus = Arc::new(EventBus::new());

    // Register a logging hook
    bus.register_hook(HookPhase::PrePublish, move |event| {
        println!("📤 Pre-publish hook: {}", event.event_type());
        Ok(())
    })
    .await;

    // Register a metrics hook
    bus.register_hook(HookPhase::PostPublish, move |event| {
        println!("📊 Post-publish hook: {}", event.event_type());
        Ok(())
    })
    .await;

    // Subscribe to all session events
    let (id1, mut rx1) = bus.subscribe("session.*").await?;
    println!("✅ Subscribed to session.* with ID: {id1}");

    // Subscribe to specific event
    let (id2, mut rx2) = bus.subscribe("session.started").await?;
    println!("✅ Subscribed to session.started with ID: {id2}");

    // Spawn task to handle session.* events
    let handle1 = tokio::spawn(async move {
        while let Ok(event) = rx1.recv().await {
            println!(
                "🎯 [session.*] Received: {} at {}",
                event.event_type(),
                event.timestamp()
            );
        }
    });

    // Spawn task to handle session.started events
    let handle2 = tokio::spawn(async move {
        while let Ok(event) = rx2.recv().await {
            // Try to downcast to concrete type
            if let Some(session_event) = event.as_any().downcast_ref::<SessionStartedEvent>() {
                println!(
                    "🎉 [session.started] Session {} started with task: {}",
                    session_event.session_id, session_event.task
                );
            }
        }
    });

    // Publish some events
    println!("\n📢 Publishing events...\n");

    for i in 1..=3 {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            format!("Task {i}"),
            format!("Detail for task {i}"),
        );

        bus.publish(event).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Print metrics
    tokio::time::sleep(Duration::from_millis(100)).await;
    let metrics = bus.metrics();
    println!("\n📈 Metrics:");
    println!("  Events published: {}", metrics.events_published);
    println!("  Events delivered: {}", metrics.events_delivered);
    println!("  Subscriber count: {}", metrics.subscriber_count);

    // Unsubscribe from one channel
    bus.unsubscribe(id1).await?;
    println!("\n🔌 Unsubscribed from session.*");

    // Publish another event (only session.started should receive it)
    println!("\n📢 Publishing final event...\n");
    let event = SessionStartedEvent::new(
        SessionId::new(),
        "Final task".to_string(),
        "Final detail".to_string(),
    );
    bus.publish(event).await?;

    // Give time for events to be processed
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Cancel handlers
    handle1.abort();
    handle2.abort();

    println!("\n✨ Example completed successfully!");

    Ok(())
}
