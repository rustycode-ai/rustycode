// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Cross-crate event flow example
//!
//! This example demonstrates how events flow between crates in the RustyCode workspace:
//!
//! 1. **rustycode-tools** publishes events when tools are executed
//! 2. **rustycode-storage** subscribes to and persists events
//! 3. **rustycode-runtime** subscribes to events for logging and monitoring
//! 4. **rustycode-bus** provides the event bus infrastructure
//!
//! Run with:
//! ```bash
//! cargo run --example cross_crate_event_example
//! ```

use rustycode_bus::{EventBus, HookPhase, SessionStartedEvent, ToolExecutedEvent};
use rustycode_protocol::{SessionId, ToolCall};
use rustycode_runtime::AsyncRuntime;
use rustycode_storage::{EventSubscriber, Storage};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::time::{Duration, timeout};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("rustycode=debug,info")
        .init();

    println!("🚀 Cross-Crate Event Flow Example");
    println!("=================================\n");

    // Create a temporary directory for the example
    let temp_dir = TempDir::new()?;
    let cwd = temp_dir.path();

    // Setup environment
    setup_environment(cwd)?;

    // Initialize the event bus
    println!("1️⃣  Initializing Event Bus...");
    let bus = Arc::new(EventBus::new());

    // Initialize runtime with event bus integration
    println!("2️⃣  Initializing Runtime with Event Bus...");
    let runtime = AsyncRuntime::load(cwd).await?;

    // Initialize storage
    println!("3️⃣  Initializing Storage with Event Subscription...");
    let db_path = cwd.join("events.db");
    let _storage_subscriber = EventSubscriber::new(Storage::open(&db_path)?, bus.clone());
    _storage_subscriber.start().await?;

    println!("✅ Environment initialized\n");

    // Demonstrate wildcard subscription
    println!("4️⃣  Demonstrating Wildcard Subscriptions...");
    demonstrate_wildcard_subscriptions(&bus).await?;

    // Demonstrate tool execution events
    println!("\n5️⃣  Demonstrating Tool Execution Events...");
    demonstrate_tool_execution_events(&runtime, cwd).await?;

    // Demonstrate event persistence
    println!("\n6️⃣  Demonstrating Event Persistence...");
    demonstrate_event_persistence(&bus, &db_path).await?;

    // Demonstrate event hooks
    println!("\n7️⃣  Demonstrating Event Hooks...");
    demonstrate_event_hooks(&bus).await?;

    // Demonstrate metrics
    println!("\n8️⃣  Event Bus Metrics...");
    display_metrics(&bus);

    println!("\n✨ Cross-crate event flow demonstration complete!");

    Ok(())
}

/// Setup the test environment with required directories and config
fn setup_environment(cwd: &Path) -> anyhow::Result<()> {
    // Create necessary directories
    std::fs::create_dir_all(cwd.join("data"))?;
    std::fs::create_dir_all(cwd.join("skills"))?;
    std::fs::create_dir_all(cwd.join("memory"))?;

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
    std::fs::write(cwd.join(".rustycode.toml"), config_content)?;

    // Create a test file to read
    std::fs::write(cwd.join("test.txt"), "Hello from RustyCode event bus!")?;

    Ok(())
}

/// Demonstrate wildcard subscriptions receiving different event types
async fn demonstrate_wildcard_subscriptions(bus: &Arc<EventBus>) -> anyhow::Result<()> {
    let event_count = Arc::new(AtomicUsize::new(0));
    let event_count_clone = event_count.clone();

    // Subscribe to all events
    let (_id, mut rx) = bus.subscribe("*").await?;

    // Spawn task to count events
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            event_count_clone.fetch_add(1, Ordering::SeqCst);
            println!("   📨 Received event: {}", event.event_type());
        }
    });

    // Publish different types of events
    let session_id = SessionId::new();

    println!("   📤 Publishing session.started event...");
    bus.publish(SessionStartedEvent::new(
        session_id.clone(),
        "Example task".to_string(),
        "Example session".to_string(),
    ))
    .await?;

    println!("   📤 Publishing tool.executed event...");
    bus.publish(ToolExecutedEvent::new(
        session_id,
        "example_tool".to_string(),
        serde_json::json!({ "arg": "value" }),
        true,
        "Success".to_string(),
        None,
    ))
    .await?;

    // Give time for events to be delivered
    tokio::time::sleep(Duration::from_millis(100)).await;

    let count = event_count.load(Ordering::SeqCst);
    println!("   ✅ Wildcard subscription received {} events\n", count);

    Ok(())
}

/// Demonstrate tool execution events being published
async fn demonstrate_tool_execution_events(
    runtime: &AsyncRuntime,
    cwd: &Path,
) -> anyhow::Result<()> {
    let session_id = SessionId::new();

    // Subscribe to tool execution events
    let (_id, mut rx) = runtime.event_bus().subscribe("tool.executed").await?;

    // Spawn task to receive event
    let event_receiver = tokio::spawn(async move {
        match timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Ok(event)) => {
                let data = event.serialize();
                println!("   📨 Tool execution event received:");
                println!("      - Tool: {}", data["tool_name"]);
                println!("      - Success: {}", data["success"]);
                println!("      - Session: {}", data["session_id"]);
            }
            Ok(Err(e)) => println!("   ❌ Error receiving event: {:?}", e),
            Err(_) => println!("   ⏱️  Timeout waiting for event"),
        }
    });

    // Execute a tool
    println!("   🔨 Executing read_file tool...");
    let call = ToolCall {
        call_id: uuid::Uuid::new_v4().to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({ "path": "test.txt" }),
    };

    runtime.execute_tool(&session_id, call, cwd).await?;

    // Wait for event to be received
    event_receiver.await?;
    println!("   ✅ Tool execution event published and received\n");

    Ok(())
}

/// Demonstrate event persistence in storage
async fn demonstrate_event_persistence(bus: &Arc<EventBus>, db_path: &Path) -> anyhow::Result<()> {
    // Publish some test events
    let session_id = SessionId::new();

    println!("   📤 Publishing test events...");
    for i in 1..=3 {
        bus.publish(ToolExecutedEvent::new(
            session_id.clone(),
            format!("test_tool_{}", i),
            serde_json::json!({ "index": i }),
            true,
            format!("Output {}", i),
            None,
        ))
        .await?;
    }

    // Give time for events to be persisted
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Retrieve events from storage
    println!("   📂 Retrieving events from storage...");
    let storage = Storage::open(db_path)?;
    let events = storage.get_events(10)?;

    println!("   ✅ Retrieved {} events from storage", events.len());
    for (i, event) in events.iter().take(3).enumerate() {
        println!(
            "      {}. {} at {}",
            i + 1,
            event.event_type,
            event.created_at
        );
    }
    println!();

    Ok(())
}

/// Demonstrate event hooks for logging and monitoring
async fn demonstrate_event_hooks(bus: &Arc<EventBus>) -> anyhow::Result<()> {
    let hook_count = Arc::new(AtomicUsize::new(0));
    let hook_count_clone = hook_count.clone();

    println!("   🎣 Registering pre-publish hook...");
    bus.register_hook(HookPhase::PrePublish, move |event| {
        hook_count_clone.fetch_add(1, Ordering::SeqCst);
        println!("   🔍 Pre-publish hook: {}", event.event_type());
        Ok(())
    })
    .await;

    println!("   📤 Publishing event with hook...");
    bus.publish(SessionStartedEvent::new(
        SessionId::new(),
        "Hook test task".to_string(),
        "Testing event hooks".to_string(),
    ))
    .await?;

    // Give hook time to execute
    tokio::time::sleep(Duration::from_millis(50)).await;

    let count = hook_count.load(Ordering::SeqCst);
    println!("   ✅ Hook executed {} time(s)\n", count);

    Ok(())
}

/// Display event bus metrics
fn display_metrics(bus: &Arc<EventBus>) {
    let metrics = bus.metrics();
    println!("   📊 Event Bus Metrics:");
    println!("      - Events Published: {}", metrics.events_published);
    println!("      - Events Delivered: {}", metrics.events_delivered);
    println!("      - Events Failed: {}", metrics.events_failed);
    println!("      - Active Subscribers: {}", metrics.subscriber_count);
    println!();
}
