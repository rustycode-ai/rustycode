#![allow(
    clippy::assigning_clones,
    clippy::bool_to_int_with_if,
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::clone_on_copy,
    clippy::collapsible_else_if,
    clippy::collection_is_never_read,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::duration_suboptimal_units,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::explicit_iter_loop,
    clippy::field_reassign_with_default,
    clippy::format_push_string,
    clippy::future_not_send,
    clippy::if_not_else,
    clippy::ignore_without_reason,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::implicit_hasher,
    clippy::imprecise_flops,
    clippy::items_after_statements,
    clippy::iter_on_single_items,
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
    clippy::ref_option,
    clippy::search_is_some,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
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
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq
)]
#![allow(dead_code, unused_imports)]
#![cfg_attr(test, allow(clippy::float_cmp,))]
//! Ratatui-based terminal interface for RustyCode.

mod agents;

mod logging;
pub mod theme;
mod unicode;

pub(crate) mod compaction_context;
pub(crate) mod memory;
#[cfg(feature = "vector-memory")]
pub(crate) mod memory_bridge;
pub(crate) mod services;
pub(crate) mod workspace;

pub(crate) mod slash_commands;

pub(crate) mod plugin;

pub mod ui;

pub mod app;

// Re-exports for integration tests
pub use app::auto_tool_parser;
pub use app::tasks;
pub use app::tool_helpers;

pub(crate) mod skills;

pub(crate) mod marketplace;

pub(crate) mod help;

pub(crate) mod tool_approval;

pub(crate) mod observability;

pub use crate::ui::diff_renderer::DiffRenderer;
pub use rustycode_ui_core::markdown::MarkdownRenderer;
pub use rustycode_ui_core::syntax_highlighter::SyntaxHighlighter;

use std::path::PathBuf;

use crate::logging::{info_log, init, log_level};

use anyhow::Result;

pub fn run(cwd: PathBuf, reconfigure: bool, resume: bool) -> Result<()> {
    if std::env::var("RUSTYCODE_TEST_MODE").is_ok() {
        return Ok(());
    }

    if let Err(e) = init() {
        tracing::error!("Failed to initialize logging: {}", e);
    } else {
        info_log!("RustyCode TUI starting");
        info_log!("Working directory: {}", cwd.display());
        debug_log!("Log level: {:?}", log_level());
    }

    use crate::app::TUI;
    use crate::services::agent_mode::AiMode;

    let config = crate::services::config::load_config();
    let initial_mode = if config.behavior.yolo_mode {
        AiMode::Yolo
    } else {
        AiMode::Ask
    };

    let (tx, _) = tokio::sync::broadcast::channel(1024);
    let event_receiver = tx.subscribe();
    let mut tui = TUI::new(cwd, initial_mode, reconfigure, event_receiver)?;

    if let Err(e) = tui.init_services() {
        tracing::warn!(
            "Service initialization failed (TUI will run in degraded mode): {}",
            e
        );
    }

    if resume {
        tui.resume_most_recent_session();
    }

    tui.run()
}
