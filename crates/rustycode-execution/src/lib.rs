//! `RustyCode` Execution - Plan execution and orchestration.
//!
//! This crate provides the core execution engine for running plans and steps:
//!
//! - **Plan Execution**: Execute complete plans with error handling
//! - **Step Orchestration**: Coordinate individual step execution
//! - **Execution Monitoring**: Track execution progress and metrics
//! - **Error Recovery**: Handle execution failures and retries

pub mod execution_context;
pub mod execution_monitor;
pub mod executor;
pub mod plan_executor;
pub mod step_executor;

pub use executor::{ExecutionConfig, ExecutionResult, Executor};
pub use plan_executor::PlanExecutor;
pub use step_executor::StepExecutor;
