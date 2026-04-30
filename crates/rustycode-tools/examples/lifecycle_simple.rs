//! Simple demonstration of the Lifecycle Hook System

use rustycode_tools::lifecycle::{HookResult, LifecycleEvent, LifecycleHandler, LifecycleHooks};
use std::sync::Arc;

/// A custom handler that prints events to stdout
struct PrintHandler;

impl LifecycleHandler for PrintHandler {
    fn handle(&self, event: &LifecycleEvent) -> HookResult {
        match event {
            LifecycleEvent::Start(p) => {
                println!(
                    "🚀 Agent started: {} (model: {})",
                    p.conversation_id, p.model
                );
            }
            LifecycleEvent::Request(p) => {
                println!(
                    "📤 Sending request (turn {}, {} messages)",
                    p.turn, p.message_count
                );
            }
            LifecycleEvent::Response(p) => {
                println!(
                    "📥 Got response (turn {}, {} tools, finish: {:?})",
                    p.turn, p.tool_call_count, p.finish_reason
                );
            }
            LifecycleEvent::ToolCallStart(p) => {
                println!("🔧 Executing tool: {}", p.tool_name);
            }
            LifecycleEvent::ToolCallEnd(p) => {
                if p.is_error {
                    println!("❌ Tool failed: {}", p.tool_name);
                } else {
                    println!("✅ Tool succeeded: {}", p.tool_name);
                }
            }
            LifecycleEvent::End(p) => {
                println!(
                    "🏁 Agent finished: {} turns, {} tools",
                    p.total_turns, p.total_tool_calls
                );
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
        HookResult::Continue
    }
}

fn main() {
    let hooks = LifecycleHooks::new().with_handler(Arc::new(PrintHandler));

    let conv_id = "demo-123";
    let model = "claude-3.5";

    // Simulate a simple agent execution
    hooks.on_start(conv_id, model);

    hooks.on_request(conv_id, 1, 3);
    hooks.on_response(conv_id, 1, Some("tool_calls"), 2);

    hooks.on_tool_start(conv_id, "read_file", Some("call-1"));
    hooks.on_tool_end(conv_id, "read_file", Some("call-1"), false);

    hooks.on_tool_start(conv_id, "bash", Some("call-2"));
    hooks.on_tool_end(conv_id, "bash", Some("call-2"), false);

    hooks.on_request(conv_id, 2, 6);
    hooks.on_response(conv_id, 2, Some("stop"), 0);

    hooks.on_end(conv_id, 2, 2);
}
