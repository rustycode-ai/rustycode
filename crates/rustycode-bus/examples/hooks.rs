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

//! Example demonstrating the priority-based hook system

use rustycode_bus::{FunctionHook, HookContext, HookPhase, HookPriority, HookRegistry};
use rustycode_protocol::SessionId;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create a hook registry
    let registry = HookRegistry::new();

    // Create execution order tracker
    let execution_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    // Register hooks with different priorities (out of order to test sorting)
    {
        let order = execution_order.clone();
        let hook = FunctionHook::new(
            "low_priority",
            HookPriority::Low,
            HookPhase::PrePublish,
            move |ctx| {
                order
                    .lock()
                    .unwrap()
                    .push(format!("Low: {}", ctx.event.event_type()));
                Ok(())
            },
        );
        registry.register(std::sync::Arc::new(hook)).await;
    }

    {
        let order = execution_order.clone();
        let hook = FunctionHook::new(
            "high_priority",
            HookPriority::High,
            HookPhase::PrePublish,
            move |ctx| {
                order
                    .lock()
                    .unwrap()
                    .push(format!("High: {}", ctx.event.event_type()));
                Ok(())
            },
        );
        registry.register(std::sync::Arc::new(hook)).await;
    }

    {
        let order = execution_order.clone();
        let hook = FunctionHook::new(
            "medium_priority",
            HookPriority::Medium,
            HookPhase::PrePublish,
            move |ctx| {
                order
                    .lock()
                    .unwrap()
                    .push(format!("Medium: {}", ctx.event.event_type()));
                Ok(())
            },
        );
        registry.register(std::sync::Arc::new(hook)).await;
    }

    // Create a test event
    let event = rustycode_bus::SessionStartedEvent::new(
        SessionId::new(),
        "Test task".to_string(),
        "Test context".to_string(),
    );

    // Execute pre-publish hooks
    let context = HookContext::new(Box::new(event), HookPhase::PrePublish);
    registry.execute_pre_publish(&context).await?;

    // Print execution order
    println!("\nHook Execution Order:");
    println!("======================");
    for (i, entry) in execution_order.lock().unwrap().iter().enumerate() {
        println!("{}. {}", i + 1, entry);
    }

    // Verify priority ordering
    {
        let order = execution_order.lock().unwrap();
        assert_eq!(
            order[0], "High: session.started",
            "High priority should execute first"
        );
        assert_eq!(
            order[1], "Medium: session.started",
            "Medium priority should execute second"
        );
        assert_eq!(
            order[2], "Low: session.started",
            "Low priority should execute last"
        );
    }

    println!("\n✓ Priority ordering verified!");

    // Demonstrate error handling
    println!("\n=== Error Handling Demo ===");
    let error_hook = FunctionHook::new(
        "error_handler",
        HookPriority::High,
        HookPhase::OnError,
        |ctx| {
            if let Some(error) = &ctx.error {
                println!("Error hook caught: {error}");
            }
            Ok(())
        },
    );
    registry.register(std::sync::Arc::new(error_hook)).await;

    let error_event = rustycode_bus::SessionStartedEvent::new(
        SessionId::new(),
        "Error task".to_string(),
        "Error context".to_string(),
    );
    let error_context = HookContext::new_error(
        Box::new(error_event),
        rustycode_bus::EventBusError::HookError("Test error".to_string()),
    );
    registry.execute_on_error(&error_context).await?;

    println!("\n✓ All demonstrations completed successfully!");

    Ok(())
}
