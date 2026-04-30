#![allow(
    clippy::missing_const_for_fn,
    clippy::uninlined_format_args,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unused_async,
    clippy::needless_raw_string_hashes,
    clippy::format_push_string,
    clippy::cast_precision_loss,
    clippy::unnecessary_debug_formatting,
    clippy::option_if_let_else,
    clippy::redundant_closure_for_method_calls,
    clippy::map_unwrap_or,
    clippy::doc_markdown
)]
//! `RustyCode` Agents - Agent implementations for autonomous development.
//!
//! This crate provides various agent implementations for different development tasks:
//!
//! - **`CodeAgent`**: General-purpose coding agent
//! - **`ReviewAgent`**: Code review and analysis agent
//! - **`TestAgent`**: Test generation and execution agent
//! - **`DebugAgent`**: Debugging and troubleshooting agent

pub mod agent;
pub mod agents;
pub mod code_agent;
pub mod debug_agent;
pub mod review_agent;
pub mod test_agent;

pub use agent::{Agent, AgentConfig, AgentResult};
pub use code_agent::CodeAgent;
pub use debug_agent::DebugAgent;
pub use review_agent::ReviewAgent;
pub use test_agent::TestAgent;
