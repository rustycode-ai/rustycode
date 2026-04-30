//! Demonstration of the Lifecycle Hook System
//!
//! This example shows how to use the lifecycle hooks to add cross-cutting
//! concerns like tracing, loop detection, and compaction to agent execution.

use rustycode_tools::lifecycle::{HookResult, LifecycleEvent, LifecycleHandler, LifecycleHooks};
use std::sync::Arc;

/// A handler that detects doom loops (repeated tool failures)
#[derive(Debug, Default)]
struct DoomLoopDetector {
    failure_count: std::sync::Mutex<std::collections::HashMap<String, usize>>,
    max_failures: usize,
}

impl LifecycleHandler for DoomLoopDetector {
    fn handle(&self, event: &LifecycleEvent) -> HookResult {
        if let LifecycleEvent::ToolCallEnd(payload) = event {
            if payload.is_error {
                let mut failures = self.failure_count.lock().unwrap();
                let count = failures.entry(payload.tool_name.clone()).or_insert(0);
                *count += 1;

                if *count >= self.max_failures {
                    return HookResult::Stop(format!(
                        "Doom loop detected: {} failed {} times",
                        payload.tool_name, count
                    ));
                }
            }
        }
        HookResult::Continue
    }
}

/// A handler that tracks token usage
#[derive(Debug, Default)]
struct TokenUsageTracker {
    total_input: std::sync::Mutex<u64>,
    total_output: std::sync::Mutex<u64>,
}

impl LifecycleHandler for TokenUsageTracker {
    fn handle(&self, event: &LifecycleEvent) -> HookResult {
        if let LifecycleEvent::Response(payload) = event {
            if let Some(usage) = &payload.usage {
                if let Some(input) = usage.input_tokens {
                    *self.total_input.lock().unwrap() += input;
                }
                if let Some(output) = usage.output_tokens {
                    *self.total_output.lock().unwrap() += output;
                }
            }
        }
        HookResult::Continue
    }
}

impl TokenUsageTracker {
    #[allow(dead_code)]
    fn get_totals(&self) -> (u64, u64) {
        let input = *self.total_input.lock().unwrap();
        let output = *self.total_output.lock().unwrap();
        (input, output)
    }
}

fn main() {
    // Create lifecycle hooks with multiple handlers
    let hooks = LifecycleHooks::new()
        .with_handler(Arc::new(DoomLoopDetector {
            max_failures: 3,
            ..Default::default()
        }))
        .with_handler(Arc::new(TokenUsageTracker::default()))
        .with_handler(Arc::new(
            rustycode_tools::lifecycle::TracingHandler::verbose(),
        ));

    // Simulate agent execution
    let conversation_id = "conv-123";
    let model = "claude-3.5-sonnet";

    // Start
    println!("=== Starting Agent Execution ===");
    match hooks.on_start(conversation_id, model) {
        HookResult::Continue => println!("✓ Execution started"),
        HookResult::Stop(reason) => {
            println!("✗ Execution stopped: {}", reason);
            return;
        }
        #[allow(unreachable_patterns)]
        _ => {}
    }

    // Turn 1: Request
    hooks.on_request(conversation_id, 1, 5);

    // Turn 1: Response
    hooks.on_response(conversation_id, 1, Some("tool_calls"), 2);

    // Tool call 1: Success
    hooks.on_tool_start(conversation_id, "read_file", Some("call-1"));
    hooks.on_tool_end(conversation_id, "read_file", Some("call-1"), false);

    // Tool call 2: Failure (1)
    hooks.on_tool_start(conversation_id, "bash", Some("call-2"));
    hooks.on_tool_end(conversation_id, "bash", Some("call-2"), true);

    // Turn 2: Request
    hooks.on_request(conversation_id, 2, 8);

    // Turn 2: Response
    hooks.on_response(conversation_id, 2, Some("tool_calls"), 1);

    // Tool call 3: Failure (2)
    hooks.on_tool_start(conversation_id, "bash", Some("call-3"));
    hooks.on_tool_end(conversation_id, "bash", Some("call-3"), true);

    // Turn 3: Request
    hooks.on_request(conversation_id, 3, 10);

    // Turn 3: Response
    hooks.on_response(conversation_id, 3, Some("tool_calls"), 1);

    // Tool call 4: Failure (3) - This should trigger doom loop detection
    hooks.on_tool_start(conversation_id, "bash", Some("call-4"));
    match hooks.on_tool_end(conversation_id, "bash", Some("call-4"), true) {
        HookResult::Continue => println!("✓ Tool execution completed"),
        HookResult::Stop(reason) => {
            println!("✗ Execution stopped: {}", reason);
            println!("=== Agent Execution Terminated ===");
            return;
        }
        #[allow(unreachable_patterns)]
        _ => {}
    }

    // End
    hooks.on_end(conversation_id, 3, 4);
    println!("=== Agent Execution Completed ===");
}
