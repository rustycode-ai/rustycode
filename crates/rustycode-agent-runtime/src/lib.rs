//! `rustycode-agent-runtime` — the shared thin LLM↔tool loop.
//!
//! Three-layer architecture:
//!
//! ```text
//! Interface (TUI / CLI / Bench)
//!      │
//! AgentSession::run()          ← this crate
//!      │  uses
//! LLMProvider + ToolRegistry + CodeIntelligence
//! ```

mod context;
pub mod event_convert;
mod intelligence;
pub mod plugins;
pub mod provider_context;
mod session;
mod tool_exec;
pub(crate) mod turn;

pub use context::{clean_assistant_text, prune_messages};
pub use intelligence::{
    ChangeType, CodeIntelligence, CodeLocation, FileChange, LocalIntelligence, NoopIntelligence,
    SymbolRef,
};
pub use plugins::{
    AgentPlugin, ConversationTrace, EarlyStopPolicy, LifecyclePlugin, OffboardingResult,
    RepetitionDetector, TurnContext,
};
pub use rustycode_protocol::stream_event::ApprovalDecision;
pub use session::{
    recommended_max_tokens, AgentConfig, AgentEvents, AgentResult, AgentSession, StoppedReason,
};
