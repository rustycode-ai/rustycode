// Workspace lint allowances for rustycode-tools.
//
// This is a large legacy crate with 50+ modules. Many patterns (builder &
// method-chaining APIs, long functions, Debug-derived diagnostics, trait-based
// dispatch with &self receivers) are intentional and would require a
// coordinated refactor to change. We allow those lints at the crate root
// rather than sprinkling hundreds of per-site annotations.
//
// unsafe_code: required for Linux-specific subprocess management
// (prctl/PR_SET_PDEATHSIG in subprocess.rs, behind #[cfg(target_os = "linux")]).
#![allow(unsafe_code)]
#![allow(
    clippy::assigning_clones,
    clippy::bool_to_int_with_if,
    clippy::branches_sharing_code,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::comparison_chain,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_link_with_quotes,
    clippy::duration_suboptimal_units,
    clippy::elidable_lifetime_names,
    clippy::expect_used,
    clippy::fn_params_excessive_bools,
    clippy::format_push_string,
    clippy::if_not_else,
    clippy::implicit_clone,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::literal_string_with_formatting_args,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::semicolon_if_nothing_returned,
    clippy::set_contains_or_insert,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::suspicious_operation_groupings,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_lines,
    clippy::trivial_regex,
    clippy::trivially_copy_pass_by_ref,
    clippy::unchecked_time_subtraction,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding
)]
#![cfg_attr(
    test,
    allow(
        clippy::doc_lazy_continuation,
        clippy::doc_markdown,
        clippy::float_cmp,
        clippy::uninlined_format_args,
    )
)]

// Re-export core types from rustycode_tools_api so internal modules can
// import them as `crate::Tool`, `crate::ToolContext`, etc.
pub use rustycode_tools_api::{
    CancellationToken, FileReadState, MessageSender, PlatformEnv, ProviderCaps, RuntimeEnv, Tool,
    ToolContext, ToolCtx, ToolFilter, ToolGate, ToolInfo, ToolName, ToolOutput, ToolPermission,
    ToolProfile, ToolRegistry, ToolRouter, ToolSelector, ToolTag,
};

// Modules
pub mod executor;
pub mod indexing;
pub mod providers;
pub mod registry;
pub mod registry_builder;
pub mod security;
pub mod side_effects;

// Re-exports from moved modules - be specific to avoid ambiguous glob re-exports
pub use crate::executor::{
    auto_tool::{AutoTool, AutoToolConfig, AutoToolContext},
    batch::BatchTool,
    cache::{CacheConfig, CacheKey, CacheMetrics, CacheStats, CachedToolResult, ToolCache},
    convoy::ConvoyDispatcher,
    // Inspectors
    inspector::{
        BudgetInspector, PermissionInspector, RateLimitInspector, RepetitionInspector,
        SecurityInspector,
    },
    // Manager types (moved out of inspector)
    manager::{InspectionAction, InspectionResult, ToolCallInfo, ToolInspectionManager},
    middleware::{ExecutionMiddleware, MiddlewareConfig, MiddlewareState, PlanModeState},
    task::{SubAgentRunner, TaskTool},
    tool_shim::{
        extract_tool_calls, extract_tool_calls_with_config, format_tools_for_prompt,
        is_valid_function_name, tool_calls_to_text, ExtractedToolCall, ExtractionSource,
        ExtractorConfig, ToolCallExtractor,
    },
};
#[cfg(feature = "vector-memory")]
pub use crate::indexing::SemanticSearchTool;
pub use crate::indexing::{CodeIndex, RepoMap};
pub use crate::providers::*;
pub use crate::registry_builder::{RegistryBuilder, ToolProvider};
pub use crate::security::*;
pub use crate::side_effects::{RecoveryStatus, SideEffect, SideEffectLedger, SideEffectType};

// Re-export unified hook types for convenient access
pub use hooks::config::{CompiledHook, ConfigLoader, MatcherGroup, UnifiedHooksConfig};
pub use hooks::matcher::ToolMatcher;
pub use hooks::protocol::{
    HookDecision, HookEvent, HookProtocolInput, HookProtocolOutput, PermissionResult,
    PostToolUseResult, PreToolUseResult,
};
pub use hooks::{HookManager, HookProfile, HookTrigger};

// Root-level exports that were moved, need to be updated to point to new modules
// Example:
// pub use crate::security::permission::*;

pub mod telemetry;
pub mod workspace;

// Other legacy modules that haven't been moved yet
pub mod app_paths {
    pub use crate::workspace::paths::*;
}
pub mod browser_pool;
pub mod checkpoint;
pub mod compaction;
pub mod config_migration;
pub mod directory_trust;
pub mod doom_loop;
pub mod edit_format;
pub mod egress_detector;
pub mod executable_integration;
pub mod executable_search;
pub mod file_formatter {
    pub use crate::workspace::formatter::*;
}
pub mod file_reference;
pub mod file_snapshot {
    pub use crate::workspace::snapshot::*;
}
pub mod file_suggest;
pub mod guardian;
pub mod hints_loader {
    pub use crate::workspace::hints::*;
}
pub mod hooks;
pub mod image;
pub mod image_detect;
pub mod json_repair;
pub mod large_response;
pub mod lifecycle {
    pub use crate::telemetry::lifecycle::*;
}
pub mod line_endings;
pub mod log_rotation;
pub mod markdown_stream;
pub mod native_tools;
pub mod notebook;
pub mod observation_layer {
    pub use crate::telemetry::observation::*;
}
pub mod osv_check;
pub mod permission_classifier;
pub mod plan_management;
pub mod plan_templates;
pub mod plugin;
pub mod plugin_manager;
pub mod project_tracker;
pub mod prompt_template;
pub mod recipes;
pub mod security_patterns;
pub mod slash_commands;
pub mod streaming {
    pub use crate::telemetry::streaming::*;
}
pub mod structured_output;
pub mod subprocess;
pub mod task_retry;
#[cfg(test)]
pub mod test_helpers;
pub mod testing;
pub mod text_summary;
pub mod todo_read;
pub mod token_counter;
pub mod tool_arg_coercion;
pub mod transform;
pub mod truncation;
pub mod workspace_checkpoint {
    pub use crate::workspace::checkpoint::*;
}
pub mod yaml_format;

// Modules that require additional dependencies or are conditionally available.
// api.rs uses only re-exported types, so it is always available.
pub mod api;
pub mod code_review;

// Re-export Checkpoint trait so `crate::Checkpoint` resolves.
pub use checkpoint::Checkpoint;

pub mod worktree {
    pub use crate::workspace::worktree::*;
}

// Permission functions are now in executor/permission.rs
pub use executor::{check_permission, check_sandbox_path, check_tool_permission, ToolDispatcher};

// Default registry functions are now in registry/default.rs
pub use registry::{default_registry, default_registry_filtered};
