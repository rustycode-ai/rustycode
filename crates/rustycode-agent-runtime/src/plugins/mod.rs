//! Agent plugins — optional observers and modifiers for the agent loop.
//!
//! Plugins are opt-in. When no plugins are configured, the run_loop behaves
//! identically to a session with zero overhead.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

/// Context passed to plugins at each turn boundary.
pub struct TurnContext {
    pub turn: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub cwd: PathBuf,
}

/// A plugin that observes and can modify tool execution within the agent loop.
///
/// All methods have default no-op implementations so plugins only override
/// what they need.
#[async_trait]
pub trait AgentPlugin: Send + Sync {
    /// Called before the first turn.
    async fn on_start(&mut self, _ctx: &TurnContext) {}

    /// Called after each tool result. Can modify the output string.
    async fn on_tool_result(
        &mut self,
        _tool_name: &str,
        _tool_id: &str,
        _input: &Value,
        _output: &mut String,
    ) {
    }

    /// Called after each turn. Return `true` to stop the loop early.
    async fn should_stop(&mut self, _ctx: &TurnContext) -> bool {
        false
    }

    /// Called when the session completes.
    async fn on_done(&mut self, _ctx: &TurnContext) {}
}

mod early_stop;
mod repetition;
mod trace;

pub use early_stop::EarlyStopPolicy;
pub use repetition::RepetitionDetector;
pub use trace::ConversationTrace;
