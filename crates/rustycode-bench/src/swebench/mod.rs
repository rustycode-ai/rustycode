//! SWE-bench evaluation support.
//!
//! Loads SWE-bench instances, clones repos at base commits, runs the agent,
//! and captures diffs as patches. Honest evaluation — no prompt tricks.

pub mod evaluator;
pub mod instance;
pub mod prediction;
pub mod runner;

pub use evaluator::{run_evaluation, EvalConfig, EvalResult};
pub use instance::SweBenchInstance;
pub use prediction::SweBenchPrediction;
pub use runner::{run_swebench, SweBenchConfig};
