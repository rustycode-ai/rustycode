// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Tool Dispatch Example
//!
//! Demonstrates runtime tool dispatch via the registry:
//! - Dynamic tool lookup by name
//! - JSON parameter passing
//! - Error handling

use rustycode_protocol::ToolCall;
use rustycode_tools::{ToolContext, default_registry};
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔧 Tool Dispatch Example\n");
    println!("========================\n");

    runtime_dispatch_example().await?;
    performance_measurement().await?;

    println!("\n✅ All examples completed!");
    Ok(())
}

async fn runtime_dispatch_example() -> anyhow::Result<()> {
    println!("1️⃣  Runtime Dispatch");
    println!("   Using ToolRegistry for dynamic tool lookup\n");

    let registry = default_registry();
    let ctx = ToolContext::new(".");

    let call = ToolCall {
        call_id: "test-1".to_string(),
        name: "Read".to_string(),
        arguments: serde_json::json!({
            "path": "Cargo.toml",
        }),
    };

    println!("   📋 Tool Call:");
    println!("      Name: {}", call.name);
    println!("      Arguments: {}", call.arguments);

    let result = registry.execute(&call, &ctx);

    println!("\n   📊 Result:");
    println!("      Success: {}", result.success);
    if result.success {
        println!("      Output length: {} bytes", result.output.len());
    } else {
        println!("      Error: {:?}", result.error);
    }

    println!("\n   💡 Runtime dispatch characteristics:");
    println!("      • Tools looked up by name at runtime");
    println!("      • Arguments validated at execution time");
    println!("      • Can register new tools dynamically");
    println!("   ✅ Runtime dispatch completed\n");

    Ok(())
}

async fn performance_measurement() -> anyhow::Result<()> {
    println!("2️⃣  Performance Measurement");
    println!("   Benchmarking dispatch overhead\n");

    let registry = default_registry();
    let ctx = ToolContext::new(".");

    let call = ToolCall {
        call_id: "bench".to_string(),
        name: "Read".to_string(),
        arguments: serde_json::json!({"path": "Cargo.toml"}),
    };

    const ITERATIONS: usize = 1000;

    println!("   📊 Benchmarking {} iterations...", ITERATIONS);
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = registry.execute(&call, &ctx);
    }
    let duration = start.elapsed();

    println!("\n   📈 Results:");
    println!("      ┌─────────────────────────────────────────────");
    println!("      │ Runtime dispatch:     {:>8.2?}", duration);
    println!(
        "      │ Per call:             {:>8.2}µs",
        duration.as_micros() as f64 / ITERATIONS as f64
    );
    println!("      └─────────────────────────────────────────────");
    println!("   ✅ Performance measurement completed\n");

    Ok(())
}
