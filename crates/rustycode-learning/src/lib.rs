#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::if_not_else,
    clippy::ignored_unit_patterns,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_midpoint,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::non_std_lazy_statics,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::significant_drop_tightening,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unchecked_time_subtraction,
    clippy::unnecessary_literal_bound,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used
)]
#![cfg_attr(test, allow(clippy::float_cmp, clippy::uninlined_format_args,))]
//! # `RustyCode` Continuous Learning System (Instincts)
//!
//! This crate provides pattern extraction, storage, and automatic application
//! for learned behaviors in the `RustyCode` system.
//!
//! ## Features
//!
//! - **Pattern Extraction**: Extract reusable patterns from sessions
//! - **Pattern Storage**: Persistent storage for learned patterns
//! - **Auto-Application**: Automatically apply learned patterns
//! - **Learning Loop**: Continuous improvement through feedback
//! - **Built-in Patterns**: Pre-configured patterns for common workflows
//!

pub mod actions;
pub mod builtin;
pub mod error;
pub mod extractor;
pub mod learning_loop;
pub mod patterns;
pub mod storage;
pub mod triggers;

// Re-export main types
pub use actions::{ActionResult, Change, ChangeType};
pub use builtin::BuiltinPatterns;
pub use error::{ExtractionError, LearningError, StorageError};
pub use extractor::InstinctExtractor;
pub use learning_loop::{Feedback, LearningLoop, LearningReport, UpdateReport};
pub use patterns::{
    Instinct, Pattern, PatternCategory, SuggestedAction, TriggerCondition, TriggerType,
};
pub use storage::PatternStorage;
pub use triggers::{Context, TriggerMatcher};
