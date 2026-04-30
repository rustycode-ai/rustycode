//! Verifier abstraction for parsing benchmark test results.

pub mod native;
pub mod pass_at_k;
pub mod reward;
pub mod script;

pub use native::NativeVerifier;
pub use pass_at_k::compute_pass_at_k;
pub use reward::{CtrfReport, RewardResult};
pub use script::ScriptVerifier;

use crate::environment::BenchEnvironment;

/// Verifies benchmark task results by running test scripts in the container.
#[async_trait::async_trait]
pub trait Verifier: Send + Sync {
    /// Run verification and return a reward score (0.0 to 1.0).
    async fn verify(&self, env: &mut dyn BenchEnvironment) -> anyhow::Result<f64>;
}
