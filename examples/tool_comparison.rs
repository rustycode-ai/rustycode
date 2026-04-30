// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Tool Dispatch Comparison Example
//!
//! This example demonstrates the difference between runtime and compile-time tool dispatch:
//! - Runtime dispatch: Dynamic, flexible, type-safe at execution time
//! - Compile-time dispatch: Static, fast, type-safe at compile time
//!
//! The key tradeoff is flexibility vs. performance and type safety.

use rustycode_protocol::ToolCall;
use rustycode_tools::{ReadFileInput, ToolContext, ToolDispatcher, default_registry};
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔧 Tool Dispatch Comparison Example\n");
    println!("=================================\n");

    // Example 1: Runtime dispatch
    runtime_dispatch_example().await?;

    // Example 2: Compile-time dispatch
    compile_time_dispatch_example().await?;

    // Example 3: Type safety demonstration
    type_safety_example().await?;

    // Example 4: Performance comparison
    performance_comparison().await?;

    println!("\n✅ All comparison examples completed!");
    Ok(())
}

/// Example 1: Runtime dispatch (dynamic)
///
/// Runtime dispatch uses trait objects and dynamic lookup.
/// Pros: Flexible, can register tools at runtime
/// Cons: Higher overhead, errors only caught at runtime
async fn runtime_dispatch_example() -> anyhow::Result<()> {
    println!("1️⃣  Runtime Dispatch (Dynamic)");
    println!("   Using ToolRegistry for dynamic tool lookup\n");

    let registry = default_registry();
    let ctx = ToolContext::new(".");

    // Create a tool call
    let call = ToolCall {
        call_id: "test-1".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({
            "path": "Cargo.toml",
        }),
    };

    println!("   📋 Tool Call:");
    println!("      Name: {}", call.name);
    println!("      Arguments: {}", call.arguments);

    // Execute the tool
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
    println!("      • Errors occur during execution");
    println!("   ✅ Runtime dispatch completed\n");

    Ok(())
}

/// Example 2: Compile-time dispatch (static)
///
/// Compile-time dispatch uses generics and monomorphization.
/// Pros: Zero-cost abstraction, compile-time type checking
/// Cons: Less flexible, tool types must be known at compile time
async fn compile_time_dispatch_example() -> anyhow::Result<()> {
    println!("2️⃣  Compile-Time Dispatch (Static)");
    println!("   Using ToolDispatcher for zero-cost abstraction\n");

    // Input is statically typed
    let input = ReadFileInput {
        path: PathBuf::from("Cargo.toml"),
        start_line: None,
        end_line: None,
    };

    println!("   📋 Input (type-checked at compile time):");
    println!("      Path: {:?}", input.path);
    println!("      Start line: {:?}", input.start_line);
    println!("      End line: {:?}", input.end_line);

    // Dispatch happens at compile time - no dynamic lookup!
    let result = ToolDispatcher::<rustycode_tools::CompileTimeReadFile>::dispatch(input);

    println!("\n   📊 Result:");
    match result {
        Ok(output) => {
            println!("      Success: true");
            println!("      Content length: {} bytes", output.content.len());
        }
        Err(e) => {
            println!("      Success: false");
            println!("      Error: {}", e);
        }
    }

    println!("\n   💡 Compile-time dispatch characteristics:");
    println!("      • Tool type known at compile time");
    println!("      • Arguments validated during compilation");
    println!("      • Zero runtime overhead (inlined)");
    println!("      • Errors caught before code runs");
    println!("   ✅ Compile-time dispatch completed\n");

    Ok(())
}

/// Example 3: Type safety demonstration
///
/// Shows how compile-time dispatch catches type errors early.
async fn type_safety_example() -> anyhow::Result<()> {
    println!("3️⃣  Type Safety Comparison");
    println!("   When are errors caught?\n");

    println!("   a) Compile-time dispatch:");
    println!("      ❌ This would NOT compile (wrong type):");
    println!("      ```rust");
    println!("      let wrong_input = WriteFileInput {{"); // Wrong input type!
    println!("          path: PathBuf::from(\"test\"),");
    println!("          content: \"hello\".to_string(),");
    println!("      }};");
    println!("      ToolDispatcher::<CompileTimeReadFile>::dispatch(wrong_input);");
    println!("      ```");
    println!("      💡 Compiler error: expected ReadFileInput, found WriteFileInput");

    println!("\n   b) Runtime dispatch:");
    println!("      ⚠️  This DOES compile (wrong arguments):");
    println!("      ```rust");
    println!("      let call = ToolCall {{");
    println!("          name: \"read_file\".to_string(),");
    println!("          arguments: serde_json::json!({{"); // Wrong arguments!
    println!("              \"wrong_field\": \"oops\",");
    println!("          }}),");
    println!("      }};");
    println!("      registry.execute(&call, &ctx);");
    println!("      ```");
    println!("      💡 Error only occurs when you try to execute it");

    println!("\n   💡 Key insight:");
    println!("      • Compile-time: Fail fast, during development");
    println!("      • Runtime: Fail later, during execution");
    println!("   ✅ Type safety comparison completed\n");

    Ok(())
}

/// Example 4: Performance comparison
///
/// Measures the performance difference between runtime and compile-time dispatch.
async fn performance_comparison() -> anyhow::Result<()> {
    println!("4️⃣  Performance Comparison");
    println!("   Benchmarking dispatch overhead\n");

    let registry = default_registry();
    let ctx = ToolContext::new(".");

    // Prepare test inputs
    let call = ToolCall {
        call_id: "bench".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path": "Cargo.toml"}),
    };

    let input = ReadFileInput {
        path: PathBuf::from("Cargo.toml"),
        start_line: None,
        end_line: None,
    };

    const ITERATIONS: usize = 1000;

    // Benchmark runtime dispatch
    println!("   📊 Benchmarking {} iterations...", ITERATIONS);
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = registry.execute(&call, &ctx);
    }
    let runtime_duration = start.elapsed();

    // Benchmark compile-time dispatch
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = ToolDispatcher::<rustycode_tools::CompileTimeReadFile>::dispatch(input.clone());
    }
    let compile_time_duration = start.elapsed();

    println!("\n   📈 Results:");
    println!("      ┌─────────────────────────────────────────────");
    println!("      │ Runtime dispatch:     {:>8.2?}", runtime_duration);
    println!(
        "      │ Compile-time dispatch: {:>8.2?}",
        compile_time_duration
    );
    let speedup =
        runtime_duration.as_nanos() as f64 / compile_time_duration.as_nanos().max(1) as f64;
    println!("      │ Speedup:              {:.2}x", speedup);
    println!("      └─────────────────────────────────────────────");

    println!("\n   💡 Why compile-time is faster:");
    println!("      • No hash map lookup for tool registry");
    println!("      • No dynamic trait object dispatch");
    println!("      • Compiler can inline the call");
    println!("      • No JSON parsing/validation overhead");

    println!("\n   🎯 When to use each:");
    println!("      • Use compile-time when:");
    println!("        - Tool types are known in advance");
    println!("        - Performance is critical");
    println!("        - You want maximum type safety");
    println!("\n      • Use runtime when:");
    println!("        - Tools registered dynamically");
    println!("        - Building plugin systems");
    println!("        - Need maximum flexibility");
    println!("   ✅ Performance comparison completed\n");

    Ok(())
}
