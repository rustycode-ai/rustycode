//! Agent abstraction for benchmark execution.

mod code_agent;
mod nop;
mod observer;
mod oracle;
#[cfg(feature = "real-agent")]
pub mod real_agent;
pub mod registry;
pub mod tools;
mod tui_agent;

pub use code_agent::{CodeAgent, CodeAgentConfig};
pub use nop::NopAgent;
pub use oracle::OracleAgent;
#[cfg(feature = "real-agent")]
pub use real_agent::RealBenchAgent;
pub use tui_agent::TuiBenchAgent;

use crate::environment::BenchEnvironment;

/// Agent that executes a benchmark task inside a container.
///
/// Implementations range from oracle (runs solution.sh) to
/// code agent (uses LLM to solve the task).
#[async_trait::async_trait]
pub trait BenchAgent: Send + Sync {
    /// Agent identifier (e.g. "oracle", "code", "nop").
    fn name(&self) -> &'static str;

    /// Prepare the agent before running (e.g. upload solution files).
    async fn setup(&mut self, env: &mut dyn BenchEnvironment) -> anyhow::Result<()>;

    /// Execute the task inside the container environment.
    async fn run(
        &mut self,
        instruction: &str,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<()>;

    /// Return accumulated (input_tokens, output_tokens) from the last run() call.
    fn token_usage(&self) -> (u64, u64);
}
