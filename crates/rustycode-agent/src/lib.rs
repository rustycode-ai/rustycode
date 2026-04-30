//! `rustycode-agent` — the shared thin LLM↔tool loop.
//!
//! Three-layer architecture:
//!
//! ```text
//! Interface (TUI / CLI / Bench)
//!      │ implements AgentEvents
//! AgentSession::run()          ← this crate
//!      │ uses
//! LLMProvider + ToolRegistry + CodeIntelligence
//! ```
//!
//! The loop has no heuristics. No nudges. No behavioral injection.
//! The model drives behavior; the loop enforces hard limits only.
//! `CodeIntelligence` provides structural reality — the model sees
//! what changed, what depends on what, and decides for itself.

mod context;
mod intelligence;
mod session;
mod tool_exec;
pub(crate) mod turn;

pub use context::{clean_assistant_text, prune_messages};
pub use intelligence::{
    ChangeType, CodeIntelligence, CodeLocation, FileChange, LocalIntelligence, NoopIntelligence,
    SymbolRef,
};
pub use rustycode_protocol::stream_event::ApprovalDecision;
pub use session::{AgentConfig, AgentEvents, AgentResult, AgentSession, StoppedReason};
