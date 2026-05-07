//! Step execution abstraction shared across crates.
//!
//! Each crate that executes plan steps provides its own context type
//! and implements [`StepExecutor`] for that context.

use crate::{Conversation, PlanStep};
use anyhow::Result;

/// Generic step executor trait.
///
/// Crate-local context types (`ExecutionContext` in core, `ExecutionContext`
/// in execution) are plugged in via the `Ctx` type parameter, keeping the
/// trait itself in the shared protocol crate.
pub trait StepExecutor<Ctx>: Send + Sync {
    /// Execute a step and return the updated step with results.
    fn execute(
        &self,
        step: PlanStep,
        conversation: &mut Conversation,
        ctx: &Ctx,
    ) -> Result<PlanStep>;
}
