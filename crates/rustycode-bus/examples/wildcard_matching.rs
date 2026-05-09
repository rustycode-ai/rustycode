// Copyright 2025 The RustyCode Authors. All rights reserved.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::significant_drop_tightening,
    clippy::unreadable_literal,
    clippy::too_many_lines
)]
// Use of this source code is governed by an MIT-style license.

//! Wildcard subscription patterns example
//!
//! Run with: `cargo run --example wildcard_matching --package rustycode-bus`

use rustycode_bus::{ContextAssembledEvent, EventBus, SessionStartedEvent, ToolExecutedEvent};
use rustycode_protocol::{ContextPlan, SessionId};
use serde_json::json;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let bus = EventBus::new();

    // Subscribe to all session events
    let (_id, mut rx_session) = bus.subscribe("session.*").await?;
    tokio::spawn(async move {
        while let Ok(event) = rx_session.recv().await {
            println!("📁 [session.*] {}", event.event_type());
        }
    });

    // Subscribe to all context events
    let (_id, mut rx_context) = bus.subscribe("context.*").await?;
    tokio::spawn(async move {
        while let Ok(event) = rx_context.recv().await {
            println!("📋 [context.*] {}", event.event_type());
        }
    });

    // Subscribe to all tool events
    let (_id, mut rx_tool) = bus.subscribe("tool.*").await?;
    tokio::spawn(async move {
        while let Ok(event) = rx_tool.recv().await {
            println!("🔧 [tool.*] {}", event.event_type());
        }
    });

    // Subscribe to all events (catch-all)
    let (_id, mut rx_all) = bus.subscribe("*").await?;
    tokio::spawn(async move {
        let mut count = 0;
        while let Ok(event) = rx_all.recv().await {
            println!("⭐ [*] {} (total: {})", event.event_type(), count + 1);
            count += 1;
            if count >= 6 {
                break; // We expect 6 events total
            }
        }
    });

    // Subscribe to errors (pattern matching with *)
    let (_id, mut rx_errors) = bus.subscribe("*.error").await?;
    tokio::spawn(async move {
        while let Ok(event) = rx_errors.recv().await {
            println!("❌ [*.error] {}", event.event_type());
        }
    });

    println!("🎯 Subscriptions created. Publishing events...\n");

    // Publish session event
    bus.publish(SessionStartedEvent::new(
        SessionId::new(),
        "Test task".to_string(),
        "Test detail".to_string(),
    ))
    .await?;

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Publish context event
    bus.publish(ContextAssembledEvent::new(
        SessionId::new(),
        ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![],
        },
        "Context ready".to_string(),
    ))
    .await?;

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Publish tool event
    bus.publish(ToolExecutedEvent::new(
        SessionId::new(),
        "Read".to_string(),
        json!({ "path": "/test/path" }),
        true,
        "File read successfully".to_string(),
        None,
    ))
    .await?;

    // Give time for all events to be processed
    tokio::time::sleep(Duration::from_millis(200)).await;

    println!("\n✨ Wildcard pattern matching completed!");

    Ok(())
}
