//! Plan execution and validation.
//!
//! Provides plan execution with validation to ensure only inspection tools
//! are used in plan mode, preventing destructive operations during planning.

pub mod validation;

pub use validation::{ExecutionStep, PlanValidator};
