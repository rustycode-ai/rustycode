// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Hybrid Event Publishing Example
//!
//! This example demonstrates the three types of subscriptions:
//! - Broadcast: Traditional channel-based delivery
//! - Callback: Direct function invocation (zero-cost)
//! - Hybrid: Both callback and channel

use rustycode_bus::{EventBus, SessionStartedEvent};
use rustycode_protocol::SessionId;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let bus = EventBus::new();

    println!("\n=== Hybrid Event Publishing Demo ===\n");

    // 1. Broadcast Subscription (traditional)
    println!("1. Creating broadcast subscription...");
    let (_broadcast_id, mut broadcast_rx) = bus.subscribe("session.*").await?;
    tokio::spawn(async move {
        while let Ok(event) = broadcast_rx.recv().await {
            println!("   📡 Broadcast received: {}", event.event_type());
        }
    });

    // 2. Callback Subscription (zero-cost)
    println!("2. Creating callback subscription...");
    let callback_count = Arc::new(AtomicUsize::new(0));
    let callback_count_clone = callback_count.clone();

    let _callback_id = bus
        .subscribe_callback("session.*", move |event| {
            let count = callback_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            println!("   ⚡ Callback invoked #{}: {}", count, event.event_type());
            Ok(())
        })
        .await?;

    // 3. Hybrid Subscription (both)
    println!("3. Creating hybrid subscription...");
    let hybrid_count = Arc::new(AtomicUsize::new(0));
    let hybrid_count_clone = hybrid_count.clone();

    let (_hybrid_id, mut hybrid_rx) = bus
        .subscribe_hybrid("session.*", move |event| {
            let count = hybrid_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            println!("   🔄 Hybrid callback #{}: {}", count, event.event_type());
            Ok(())
        })
        .await?;

    tokio::spawn(async move {
        while let Ok(event) = hybrid_rx.recv().await {
            println!("   🔄 Hybrid channel received: {}", event.event_type());
        }
    });

    println!("\n--- Publishing Events ---\n");

    // Publish some events
    for i in 1..=3 {
        let event =
            SessionStartedEvent::new(SessionId::new(), format!("Task {i}"), format!("Detail {i}"));

        println!("📤 Publishing event #{i}");
        bus.publish(event).await?;
        println!();
    }

    // Give time for all async processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Summary ---");
    println!(
        "Total callback invocations: {}",
        callback_count.load(Ordering::SeqCst)
    );
    println!(
        "Total hybrid callback invocations: {}",
        hybrid_count.load(Ordering::SeqCst)
    );

    // Show metrics
    let metrics = bus.metrics();
    println!("\n--- Metrics ---");
    println!("Events published: {}", metrics.events_published);
    println!("Events delivered: {}", metrics.events_delivered);
    println!("Active subscribers: {}", metrics.subscriber_count);

    println!("\n✅ Demo complete!\n");

    Ok(())
}
