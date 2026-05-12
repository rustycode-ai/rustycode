//! Plan execution and validation.
//!
//! Provides plan execution with validation to ensure only inspection tools
//! are used in plan mode, preventing destructive operations during planning.

pub mod generation;
pub mod validation;

pub use generation::{
    generate_plan_with_llm, generate_plan_with_llm_async, generate_smart_plan,
    generate_smart_plan_async, render_plan_markdown,
};
pub use validation::{ExecutionStep, PlanValidator};
