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
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_collect,
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
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unchecked_time_subtraction,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used
)]
#![cfg_attr(test, allow(clippy::float_cmp, clippy::uninlined_format_args,))]
//! # `RustyCode` Session Management
//!
//! This crate provides advanced session and message management with compaction,
//! summarization, and efficient serialization for the `RustyCode` system.
//!
//! ## Features
//!
//! - **Rich Message Types**: Support for text, tool calls, images, reasoning, code, and diffs
//! - **Session Management**: Track conversations with metadata and context
//! - **Smart Compaction**: Multiple strategies for reducing token usage
//! - **Efficient Serialization**: Binary format with zstd compression
//! - **Streaming Support**: Handle streaming LLM responses
//!
//! ## Example
//!
//! ```rust
//! use rustycode_session::{Session, Message, MessageRole};
//!
//! let mut session = Session::new("My Session".to_string());
//! session.add_message(Message::user("Hello, world!".to_string()));
//! session.add_message(Message::assistant("Hi! How can I help?".to_string()));
//!
//! println!("Session has {} messages", session.message_count());
//! println!("Estimated tokens: {}", session.token_count());
//! ```

pub mod compaction;
pub mod message;
pub mod rewind;
pub mod serialization;
pub mod session;
pub mod session_manager;
pub mod summary;

// Re-export main types
pub use compaction::{
    CompactionEngine, CompactionError, CompactionReport, CompactionSnapshot, CompactionStrategy,
};
pub use message::{Message, MessageMetadata, MessagePart, MessageRole};
pub use rewind::{
    create_snapshot, create_snapshot_with_checkpoint, InteractionId, InteractionSnapshot,
    RewindMode, RewindResult, RewindState, RewindStore, ToolCallRecord,
};
pub use serialization::{SerializationFormat, SessionSerializer};
pub use session::{Session, SessionId, SessionMetadata, SessionStatus};
pub use summary::{Summary, SummaryGenerator};
