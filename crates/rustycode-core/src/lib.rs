#![allow(
    clippy::assigning_clones,
    clippy::bool_to_int_with_if,
    clippy::borrow_as_ptr,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::derivable_impls,
    clippy::doc_markdown,
    clippy::elidable_lifetime_names,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::future_not_send,
    clippy::if_not_else,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_midpoint,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_bool,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::non_std_lazy_statics,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::self_only_used_in_recursion,
    clippy::semicolon_if_nothing_returned,
    clippy::set_contains_or_insert,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::suspicious_operation_groupings,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_lines,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding
)]
#![cfg_attr(test, allow(clippy::float_cmp,))]
//! RustyCode Core - Core runtime and execution logic.
//!
//! This crate provides the core functionality for the RustyCode system, including:
//!
//! - **Plan Validation**: Pre-execution validation of plans to prevent failures

//! - **Context Management**: Budget-aware context assembly and prioritization
//! - **Error Recovery**: Intelligent error classification and recovery strategies
//! - **Step Execution**: Orchestration of plan step execution with error handling
//! - **Event Publishing**: Integration with the event bus for observability
//!
//! ## Plan Validation
//!
//! The `validation` module provides comprehensive plan validation before execution:
//!
//! ```ignore
//! use rustycode_core::validation::validate_plan;
//! use rustycode_protocol::Plan;
//! use rustycode_tools::ToolRegistry;
//! use std::path::Path;
//!
//! // Validate a plan before execution
//! validate_plan(&plan, &tool_registry, workspace_root)?;
//! ```
//!
//! Validation checks include:
//! - No circular dependencies between steps
//! - All required tools are registered
//! - File paths are valid and within workspace
//! - Steps are properly ordered
//! - All required fields are present
//!
//! ## Error Recovery
//!
//! The `recovery` module provides intelligent error recovery with automatic retry,
//! fallback, and skip strategies:
//!
//! ```ignore
//! use rustycode_core::recovery::{RecoveryEngine, RecoveryConfig};
//! use anyhow::Result;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let config = RecoveryConfig::default().with_max_attempts(3);
//! let engine = RecoveryEngine::new(config);
//!
//! // Recover from errors automatically
//! let result = engine.recover(
//!     anyhow::anyhow!("Temporary failure"),
//!     "my_operation",
//!     &|| async { Err(anyhow::anyhow!("Failed")) },
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! Recovery strategies include:
//! - **Retry**: Automatically retry with exponential backoff
//! - **Fallback**: Use alternative implementations or cached results
//! - **Skip**: Skip non-critical failures
//! - **Abort**: Stop execution for critical errors

pub mod agent;
pub mod build_detection;
// Checkpoint modules consolidated into recovery/ (Phase 3)
pub mod context;
// context_management and context_prio consolidated into context/ (Phase 4)
pub mod deadlock;
pub mod edit_history;
pub mod error;
pub mod execution;
pub mod headless;
pub mod integration;
pub mod iteration_checkpoint;
pub mod plan;
pub mod plan_executor;
pub mod recovery;
pub mod rollout;
pub mod runtime;
pub mod session;
pub mod session_context;
pub mod snapshot;
pub mod streaming;
pub mod team;
pub mod tenacity;
pub mod todo_enforcer;
pub mod tool_result_storage;
pub mod ultrawork;
pub mod validation;
pub mod verification_gates;
pub mod workspace_memory;

pub use execution::{
    CheckpointingStepExecutor, ExecutionCheckpointStore, ExecutionConfig, ExecutionContext,
    StepExecutor, StepExecutorRegistry, ToolInvocationWrapper,
};
pub use plan_executor::{ExecutionOptions, ExecutionReport, PlanExecutor};
pub use recovery::{
    CheckpointSnapshot, CheckpointStore, ExecutionCheckpointDetector, ExecutionPhase, Recovery,
    RecoveryState,
};
pub use runtime::{CodeExcerpt, DoctorReport, PlanReport, RunReport, Runtime, ToolCallReport};
pub use rustycode_protocol::{Plan, PlanId, PlanStatus, PlanStep, StepStatus};
pub use session::{AiMode, MessageType, SessionState, ToolExecution, ToolStatus};
mod sleep;
pub use rustycode_shared_runtime as shared_runtime;

pub mod code_utils;

pub use code_utils::{
    estimate_tokens, is_supported_source, select_code_excerpts, should_skip_path, task_terms,
};

// Plan generation re-exports (moved to plan::generation)
pub use plan::generation::{
    generate_plan_with_llm, generate_plan_with_llm_async, generate_smart_plan,
    generate_smart_plan_async, render_plan_markdown,
};
