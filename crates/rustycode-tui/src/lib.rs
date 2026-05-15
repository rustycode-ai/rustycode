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

pub(crate) mod notifications;

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

    // Apply shell_environment_policy.set from config before anything else
    // so that API keys and base URLs are available for provider initialization.
    match rustycode_config::Config::load(&cwd) {
        Ok(cfg) => {
            let set_count = cfg.shell_environment_policy.set.len();
            if set_count > 0 {
                eprintln!(
                    "[rtk] Applied {} env vars from shell_environment_policy.set",
                    set_count
                );
            }
            cfg.shell_environment_policy.apply_to_env();
        }
        Err(e) => {
            eprintln!("[rtk] Warning: Failed to load config: {}", e);
        }
    }

    // Mark that we're running inside the TUI so tools that need interactive
    // input (stdin reads) can detect non-interactive context and auto-answer
    // instead of deadlocking the event loop.
    std::env::set_var("RUSTYCODE_TUI", "1");

    let t0 = std::time::Instant::now();

    if let Err(e) = init() {
        tracing::error!("Failed to initialize logging: {}", e);
    } else {
        info_log!("RustyCode TUI starting");
        info_log!("Working directory: {}", cwd.display());
        debug_log!("Log level: {:?}", log_level());
    }
    info_log!("[PERF] logging init took {}ms", t0.elapsed().as_millis());

    let t1 = std::time::Instant::now();
    use crate::app::TUI;
    use crate::services::agent_mode::AiMode;

    let config = crate::services::config::load_config();
    let initial_mode = if config.behavior.yolo_mode {
        AiMode::Yolo
    } else {
        AiMode::Ask
    };
    info_log!("[PERF] config load took {}ms", t1.elapsed().as_millis());

    let t2 = std::time::Instant::now();
    let (tx, _) = tokio::sync::broadcast::channel(1024);
    let event_receiver = tx.subscribe();
    let mut tui = TUI::new(cwd, initial_mode, reconfigure, event_receiver)?;
    info_log!("[PERF] TUI::new took {}ms", t2.elapsed().as_millis());

    info_log!("[PERF] pre-run setup took {}ms", t0.elapsed().as_millis());
    tui.run(resume)
}

/// Alternative entry point using the decomposed AppShell architecture.
///
/// Creates an [`AppShell`] with a registered [`PluginManagerFeature`] to
/// verify that the dual-path feature wiring compiles. Still bails because
/// the full shell event loop is not yet implemented.
#[cfg(feature = "app-shell")]
pub fn run_with_shell(_cwd: PathBuf, _reconfigure: bool, _resume: bool) -> Result<()> {
    use std::sync::{Arc, RwLock};

    use crate::app::features::plugin_manager::PluginManagerFeature;
    use crate::app::shell::AppShell;
    use crate::plugin::PluginManager;
    use crate::theme::Theme;

    let theme = Arc::new(Theme::default());
    let mut shell = AppShell::new(theme);

    let manager = Arc::new(RwLock::new(PluginManager::default()));
    let plugin_feature = PluginManagerFeature::new(manager);
    shell.register_feature(Box::new(plugin_feature));

    anyhow::bail!("app-shell entry point not yet implemented")
}
