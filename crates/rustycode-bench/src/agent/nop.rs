//! No-op agent — does nothing (infrastructure test).

use super::BenchAgent;
use crate::environment::BenchEnvironment;

/// Agent that does nothing. Used for testing the benchmark infrastructure
/// without running any actual agent logic.
pub struct NopAgent;

#[async_trait::async_trait]
impl BenchAgent for NopAgent {
    fn name(&self) -> &'static str {
        "nop"
    }

    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> anyhow::Result<()> {
        Ok(())
    }

    async fn run(
        &mut self,
        _instruction: &str,
        _env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<()> {
        tracing::info!("NopAgent: doing nothing");
        Ok(())
    }
}
