// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Example demonstrating event persistence in rustycode-storage
//!
//! This example shows how to:
//! 1. Create a Storage instance
//! 2. Persist events from the event bus
//! 3. Retrieve and query stored events

use rustycode_bus::{ContextAssembledEvent, SessionStartedEvent, ToolExecutedEvent};
use rustycode_protocol::{ContextPlan, ContextSection, ContextSectionKind, SessionId};
use rustycode_storage::Storage;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary storage database
    let db_path = std::env::temp_dir().join("event_persistence_example.db");
    println!("Creating storage at: {}", db_path.display());

    let storage = Storage::open(&db_path)?;

    // Create and persist a session started event
    let session_event = SessionStartedEvent::new(
        SessionId::new(),
        "Analyze codebase structure".to_string(),
        "Initial session for code analysis".to_string(),
    );

    storage.insert_event_bus(&session_event)?;
    println!("✓ Persisted session started event");

    // Create and persist a context assembled event
    let context_plan = ContextPlan {
        total_budget: 200000,
        reserved_budget: 150000,
        sections: vec![
            ContextSection {
                kind: ContextSectionKind::SystemInstructions,
                tokens_reserved: 50000,
                tokens_used: 5000,
                items: vec!["Read project structure".to_string()],
                note: "System prompt section".to_string(),
            },
            ContextSection {
                kind: ContextSectionKind::CodeExcerpts,
                tokens_reserved: 100000,
                tokens_used: 25000,
                items: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
                note: "Core source files".to_string(),
            },
        ],
    };

    let context_event = ContextAssembledEvent::new(
        SessionId::new(),
        context_plan,
        "Context with 2 sections ready".to_string(),
    );

    storage.insert_event_bus(&context_event)?;
    println!("✓ Persisted context assembled event");

    // Create and persist a tool executed event
    let tool_event = ToolExecutedEvent::new(
        SessionId::new(),
        "read_file".to_string(),
        json!({ "path": "/path/to/file.rs" }),
        true,
        "File contents read successfully".to_string(),
        None,
    );

    storage.insert_event_bus(&tool_event)?;
    println!("✓ Persisted tool executed event");

    // Retrieve and display the most recent events
    println!("\n📋 Recent Events:");
    println!("═━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━═");

    let events = storage.get_events(5)?;
    for (i, event) in events.iter().enumerate() {
        println!("\n[{}] Event Type: {}", i + 1, event.event_type);
        println!("    ID: {}", event.id);
        println!("    Created At: {}", event.created_at);
        println!("    Data Length: {} bytes", event.event_data.len());

        // Parse and display a preview of the event data
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&event.event_data) {
            if let Some(task) = data.get("task").and_then(|v| v.as_str()) {
                println!("    Task: {}", task);
            }
            if let Some(tool_name) = data.get("tool_name").and_then(|v| v.as_str()) {
                println!("    Tool: {}", tool_name);
            }
            if let Some(detail) = data.get("detail").and_then(|v| v.as_str()) {
                println!("    Detail: {}", detail);
            }
        }
    }

    println!("\n═━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━═");
    println!("\n✅ Event persistence example completed successfully!");

    // Clean up the example database
    std::fs::remove_file(db_path)?;
    println!("🧹 Cleaned up example database");

    Ok(())
}
