// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Async Runtime Pattern Examples
//!
//! This example demonstrates common async patterns used in RustyCode:
//! - Concurrent operations with JoinSet
//! - Timeout handling
//! - Cancellation tokens
//! - Graceful shutdown patterns
//! - Select! for racing multiple futures

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("⚡ Async Runtime Pattern Examples\n");
    println!("===============================\n");

    // Example 1: Concurrent operations with JoinSet
    concurrent_operations_example().await?;

    // Example 2: Timeout handling
    timeout_example().await?;

    // Example 3: Cancellation token
    cancellation_example().await?;

    // Example 4: Graceful shutdown
    graceful_shutdown_example().await?;

    // Example 5: Select! for racing futures
    select_example().await?;

    println!("\n✅ All async examples completed successfully!");
    Ok(())
}

/// Example 1: Concurrent operations with JoinSet
///
/// Shows how to run multiple tasks concurrently and collect their results.
async fn concurrent_operations_example() -> anyhow::Result<()> {
    println!("1️⃣  Concurrent Operations with JoinSet");
    println!("   Running multiple tasks in parallel\n");

    let mut set = JoinSet::new();

    // Spawn multiple tasks concurrently
    println!("   🚀 Spawning 5 concurrent tasks...");
    for i in 0..5 {
        set.spawn(async move {
            // Simulate some work
            sleep(Duration::from_millis(100)).await;
            println!("   ✅ Task {} completed", i);
            i * 2 // Return a result
        });
    }

    // Collect results as they complete
    println!("\n   📥 Collecting results as tasks complete:");
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        match result {
            Ok(value) => {
                println!("   📦 Task finished with result: {}", value);
                results.push(value);
            }
            Err(e) => println!("   ❌ Task failed: {}", e),
        }
    }

    println!("\n   💡 Key insight: JoinSet allows you to:");
    println!("   • Run many tasks concurrently");
    println!("   • Collect results in completion order");
    println!("   • Handle errors per-task without aborting all tasks");
    println!("   ✅ Concurrent operations completed\n");

    Ok(())
}

/// Example 2: Timeout handling
///
/// Shows how to add timeouts to long-running operations.
async fn timeout_example() -> anyhow::Result<()> {
    println!("2️⃣  Timeout Handling Example");
    println!("   Adding time limits to operations\n");

    // Example 2a: Operation completes before timeout
    println!("   a) Fast operation (completes before timeout):");
    let start = std::time::Instant::now();
    let result = timeout(Duration::from_millis(100), sleep(Duration::from_millis(50))).await;

    match result {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("   ✅ Completed in {:.2}ms", elapsed.as_millis());
        }
        Err(_) => println!("   ⏱️  Timed out"),
    }

    // Example 2b: Operation times out
    println!("\n   b) Slow operation (exceeds timeout):");
    let start = std::time::Instant::now();
    let result = timeout(Duration::from_millis(50), sleep(Duration::from_millis(200))).await;

    match result {
        Ok(_) => println!("   ✅ Completed"),
        Err(_) => {
            let elapsed = start.elapsed();
            println!("   ⏱️  Timed out after {:.2}ms", elapsed.as_millis());
        }
    }

    println!("\n   💡 Key insight: Timeouts prevent operations from hanging forever");
    println!("   ✅ Timeout handling completed\n");

    Ok(())
}

/// Example 3: Cancellation token
///
/// Shows how to gracefully cancel running tasks.
async fn cancellation_example() -> anyhow::Result<()> {
    println!("3️⃣  Cancellation Token Example");
    println!("   Coordinating task cancellation\n");

    let token = CancellationToken::new();
    let token_clone = token.clone();

    // Spawn a task that can be cancelled
    let handle = tokio::spawn(async move {
        println!("   🔄 Task started, waiting for cancellation...");

        // Wait for either cancellation or a long sleep
        tokio::select! {
            _ = token_clone.cancelled() => {
                println!("   🛑 Task received cancellation signal!");
                "cancelled"
            }
            _ = sleep(Duration::from_secs(10)) => {
                println!("   ✅ Task completed normally");
                "completed"
            }
        }
    });

    // Let the task run briefly, then cancel it
    sleep(Duration::from_millis(100)).await;
    println!("   📢 Sending cancellation signal...");
    token.cancel();

    // Wait for the task to finish
    let result = handle.await?;
    println!("   📥 Task result: {}", result);

    println!("\n   💡 Key insight: Cancellation tokens provide cooperative cancellation:");
    println!("   • Tasks must check for cancellation periodically");
    println!("   • Clean shutdown is possible (no abrupt terminations)");
    println!("   • Multiple tasks can share one token for coordinated shutdown");
    println!("   ✅ Cancellation example completed\n");

    Ok(())
}

/// Example 4: Graceful shutdown
///
/// Shows how to implement a graceful shutdown pattern.
async fn graceful_shutdown_example() -> anyhow::Result<()> {
    println!("4️⃣  Graceful Shutdown Example");
    println!("   Implementing clean system shutdown\n");

    let shutdown_token = Arc::new(CancellationToken::new());
    let task_counter = Arc::new(AtomicUsize::new(0));
    let mut set = JoinSet::new();

    // Spawn multiple worker tasks
    println!("   🚀 Starting 3 worker tasks...");
    for i in 0..3 {
        let token = shutdown_token.clone();
        let counter = task_counter.clone();

        set.spawn(async move {
            println!("   🔧 Worker {} started", i);

            // Simulate periodic work
            loop {
                // Check for cancellation
                if token.is_cancelled() {
                    println!("   🛑 Worker {} shutting down...", i);
                    break;
                }

                // Do some work
                counter.fetch_add(1, Ordering::Relaxed);
                sleep(Duration::from_millis(50)).await;
            }

            println!("   ✅ Worker {} finished cleanly", i);
        });
    }

    // Let workers run for a bit
    sleep(Duration::from_millis(150)).await;
    println!(
        "\n   📊 Tasks completed before shutdown: {}",
        task_counter.load(Ordering::Relaxed)
    );

    // Trigger graceful shutdown
    println!("\n   📢 Initiating graceful shutdown...");
    shutdown_token.cancel();

    // Wait for all workers to finish
    println!("   ⏳ Waiting for workers to finish...");
    while let Some(result) = set.join_next().await {
        result?;
    }

    println!(
        "   📊 Total tasks completed: {}",
        task_counter.load(Ordering::Relaxed)
    );

    println!("\n   💡 Key insight: Graceful shutdown ensures:");
    println!("   • All in-flight work completes");
    println!("   • Resources are cleaned up properly");
    println!("   • No data loss or corruption");
    println!("   ✅ Graceful shutdown completed\n");

    Ok(())
}

/// Example 5: Select! for racing futures
///
/// Shows how to wait for the first of multiple futures to complete.
async fn select_example() -> anyhow::Result<()> {
    println!("5️⃣  Select! Pattern Example");
    println!("   Racing multiple futures\n");

    // Example 5a: Race between two operations
    println!("   a) Racing two operations (first one wins):");
    let result = tokio::select! {
        _ = sleep(Duration::from_millis(100)) => "fast",
        _ = sleep(Duration::from_millis(200)) => "slow",
    };
    println!("   ✅ Winner: {}", result);

    // Example 5b: Select with branching logic
    println!("\n   b) Select with different branches:");
    let operation1 = async {
        sleep(Duration::from_millis(50)).await;
        "operation 1"
    };

    let operation2 = async {
        sleep(Duration::from_millis(100)).await;
        "operation 2"
    };

    let result = tokio::select! {
        result1 = operation1 => {
            println!("   📥 Operation 1 completed first");
            result1
        }
        result2 = operation2 => {
            println!("   📥 Operation 2 completed first");
            result2
        }
    };
    println!("   📦 Result: {}", result);

    // Example 5c: Timeout with select
    println!("\n   c) Operation with timeout fallback:");
    let slow_operation = async {
        sleep(Duration::from_millis(200)).await;
        "completed"
    };

    let result = tokio::select! {
        _ = sleep(Duration::from_millis(50)) => "timed out",
        result = slow_operation => result,
    };
    println!("   📦 Result: {}", result);

    println!("\n   💡 Key insight: Select! enables powerful patterns:");
    println!("   • Implement timeouts without wrapper functions");
    println!("   • Race multiple operations and handle the winner");
    println!("   • Build complex async control flow");
    println!("   ✅ Select! example completed\n");

    Ok(())
}
