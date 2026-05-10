//! Oracle agent — runs the pre-written solution script.

use std::path::PathBuf;

use anyhow::{bail, Context};

use super::BenchAgent;
use crate::environment::BenchEnvironment;

/// Agent that runs the oracle solution (solve.sh) from the task.
///
/// This is used for infrastructure validation: if the oracle solution
/// passes verification, the environment and verifier are working correctly.
pub struct OracleAgent {
    /// Path to the solution directory (contains solve.sh).
    solution_dir: PathBuf,
}

impl OracleAgent {
    #[must_use]
    pub const fn new(solution_dir: PathBuf) -> Self {
        Self { solution_dir }
    }
}

#[async_trait::async_trait]
impl BenchAgent for OracleAgent {
    fn name(&self) -> &'static str {
        "oracle"
    }

    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> anyhow::Result<()> {
        // Nothing to do in setup — the script is read directly in run()
        Ok(())
    }

    async fn run(
        &mut self,
        _instruction: &str,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<()> {
        let solve_script = self.solution_dir.join("solve.sh");
        if !solve_script.exists() {
            bail!("Oracle solution not found at {}", solve_script.display());
        }

        tracing::info!("Running oracle solution...");
        // exec_script rewrites container paths for native env, runs as-is for Docker
        let result = env
            .exec_script(&solve_script, 600)
            .await
            .context("oracle solution execution failed")?;

        if !result.success() {
            tracing::warn!(
                "Oracle solution exited with code {}: {}",
                result.exit_code,
                result.stderr
            );
        }

        tracing::info!("Oracle solution completed (exit {})", result.exit_code);
        Ok(())
    }

    fn token_usage(&self) -> (u64, u64) {
        // Oracle agent runs a script, no LLM involved
        (0, 0)
    }
}
