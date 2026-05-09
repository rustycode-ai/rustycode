pub mod agent;
pub mod code_agent;
pub mod debug_agent;
pub mod orchestrator;
pub mod patterns;
pub mod review_agent;
pub mod subagent;
pub mod test_agent;

/// Embedded agent definitions compiled from `agents/*.md` at build time.
mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_agents.rs"));
}

pub use agent::*;
pub use code_agent::*;
pub use debug_agent::*;
pub use embedded::embedded_agents;
pub use orchestrator::*;
pub use patterns::*;
pub use review_agent::*;
pub use subagent::*;
pub use test_agent::*;
