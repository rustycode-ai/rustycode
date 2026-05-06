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
use rustycode_protocol::{AgentRole, ToolCall, ToolResult};
pub use rustycode_tools_api::{
    CancellationToken, FileReadState, Tool, ToolContext, ToolGate, ToolInfo, ToolOutput,
    ToolPermission, ToolProfile, ToolRegistry, ToolSelector, ToolTag,
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
    // Note: DecomposeProblemTool from executor is not re-exported to avoid conflict with providers
    inspector::{
        BudgetInspector, InspectionAction, InspectionResult, PermissionInspector,
        RateLimitInspector, RepetitionInspector, SecurityInspector, ToolCallInfo,
        ToolInspectionManager,
    },
    middleware::{ExecutionMiddleware, MiddlewareConfig, MiddlewareState, PlanModeState},
    task::{SubAgentRunner, TaskTool},
    tool_shim::{
        extract_tool_calls, extract_tool_calls_with_config, format_tools_for_prompt,
        is_valid_function_name, sanitize_function_name, tool_calls_to_text, ExtractedToolCall,
        ExtractionSource, ExtractorConfig, ToolCallExtractor,
    },
};
#[cfg(feature = "vector-memory")]
pub use crate::indexing::SemanticSearchTool;
pub use crate::indexing::{CodeIndex, RepoMap};
pub use crate::providers::*;
pub use crate::registry_builder::{RegistryBuilder, ToolProvider};
pub use crate::security::*;
pub use crate::side_effects::{RecoveryStatus, SideEffect, SideEffectLedger, SideEffectType};

// Root-level exports that were moved, need to be updated to point to new modules
// Example:
// pub use crate::security::permission::*;

// Other legacy modules that haven't been moved yet
pub mod app_paths;
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
pub mod file_formatter;
pub mod file_reference;
pub mod file_snapshot;
pub mod file_suggest;
pub mod guardian;
pub mod hints_loader;
pub mod hooks;
pub mod image;
pub mod image_detect;
pub mod json_repair;
pub mod large_response;
pub mod lifecycle;
pub mod line_endings;
pub mod log_rotation;
pub mod markdown_stream;
pub mod native_tools;
pub mod notebook;
pub mod observation_layer;
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
pub mod skills;
pub mod slash_commands;
pub mod smart_approve;
pub mod streaming;
pub mod structured_output;
pub mod subprocess;
pub mod task_retry;
#[cfg(test)]
pub mod test_helpers;
pub mod testing;
pub mod text_summary;
pub mod todo;
pub mod todo_read;
pub mod token_counter;
pub mod tool_arg_coercion;
pub mod transform;
pub mod truncation;
pub mod workspace_checkpoint;
pub mod yaml_format;

// Modules that require additional dependencies or are conditionally available.
// api.rs uses only re-exported types, so it is always available.
pub mod api;
pub mod code_review;

// Re-export Checkpoint trait so `crate::Checkpoint` resolves.
pub use checkpoint::Checkpoint;

// worktree module depends on rustycode_runtime which is not a dependency of this crate.
// Gate behind an always-false cfg to avoid compilation errors.
#[cfg(any())]
pub mod worktree;

// -- Missing items expected by provider files --

/// Check if a tool is permitted in the given session mode.
/// In Planning mode, only read-only tools are allowed.
/// In Executing mode, all tools are permitted.
pub fn check_tool_permission(tool_name: &str, mode: rustycode_protocol::SessionMode) -> bool {
    match mode {
        rustycode_protocol::SessionMode::Planning => {
            matches!(
                tool_name,
                "read_file"
                    | "list_dir"
                    | "grep"
                    | "search"
                    | "glob"
                    | "find"
                    | "inspect"
                    | "lsp_diagnostics"
                    | "lsp_hover"
                    | "lsp_definition"
                    | "lsp_completion"
                    | "lsp_document_symbols"
                    | "lsp_references"
                    | "lsp_full_diagnostics"
                    | "lsp_code_actions"
                    | "lsp_rename"
                    | "lsp_formatting"
                    | "lsp_get_symbols_overview"
                    | "lsp_find_symbol"
                    | "lsp_replace_symbol_body"
                    | "lsp_insert_before_symbol"
                    | "lsp_insert_after_symbol"
                    | "lsp_safe_delete_symbol"
                    | "lsp_rename_symbol"
                    | "lsp_analyze_symbol"
                    | "lsp_extract_symbol"
                    | "lsp_inline_symbol"
                    | "memory_search"
                    | "memory_list"
                    | "skill_list"
                    | "doctor"
                    | "git_status"
                    | "git_log"
                    | "git_diff"
            )
        }
        rustycode_protocol::SessionMode::Executing => true,
        _ => true,
    }
}

/// Check if the given permission level is allowed by the context's `max_permission`.
///
/// Permission hierarchy: None < Read < Write < Execute < Network.
/// Returns an error if the required permission exceeds the context's allowance.
pub fn check_permission(permission: ToolPermission, ctx: &ToolContext) -> anyhow::Result<()> {
    let required = permission_level(&permission);
    let allowed = permission_level(&ctx.max_permission);
    if required > allowed {
        anyhow::bail!(
            "permission denied: tool requires {:?} but context allows {:?}",
            permission,
            ctx.max_permission
        );
    }
    Ok(())
}

/// Check if a path is allowed under sandbox rules.
///
/// Validates against:
/// 1. Denied paths (always blocked)
/// 2. Allowed paths (whitelist, if configured)
/// 3. Blocked path components (.ssh, .gnupg, .aws, etc.)
pub fn check_sandbox_path(path: &std::path::Path, ctx: &ToolContext) -> anyhow::Result<()> {
    // Check denied paths first
    for denied in &ctx.sandbox.denied_paths {
        if path.starts_with(denied) {
            anyhow::bail!(
                "sandbox: path '{}' is under denied prefix '{}'",
                path.display(),
                denied.display()
            );
        }
    }

    // Check allowed paths (whitelist mode if configured)
    if let Some(allowed) = &ctx.sandbox.allowed_paths {
        let permitted = allowed.iter().any(|prefix| path.starts_with(prefix));
        if !permitted {
            anyhow::bail!(
                "sandbox: path '{}' is outside allowed directories",
                path.display()
            );
        }
    }

    // Check blocked path components (.ssh, .gnupg, .aws, etc.)
    for component in path.components() {
        if let std::path::Component::Normal(os_str) = component {
            if let Some(s) = os_str.to_str() {
                if security::validation::BLOCKED_PATH_COMPONENTS.contains(&s) {
                    anyhow::bail!(
                        "sandbox: path contains blocked component '{}' for security reasons",
                        s
                    );
                }
            }
        }
    }

    Ok(())
}

/// Numeric level for permission comparison. Higher = more permissive.
const fn permission_level(p: &ToolPermission) -> u8 {
    match p {
        ToolPermission::None => 0,
        ToolPermission::Read => 1,
        ToolPermission::Write => 2,
        ToolPermission::Execute => 3,
        ToolPermission::Network => 4,
        _ => 0,
    }
}

/// Tool executor combining a registry and a context.
/// Used by the `auto_tool` module for programmatic tool invocation.
pub struct ToolExecutor {
    pub registry: std::sync::Arc<ToolRegistry>,
    pub context: ToolContext,
}

impl ToolExecutor {
    pub const fn new(registry: std::sync::Arc<ToolRegistry>, context: ToolContext) -> Self {
        Self { registry, context }
    }

    /// Create a new executor from a working directory with an empty registry.
    pub fn from_cwd(cwd: std::path::PathBuf) -> Self {
        Self {
            registry: std::sync::Arc::new(ToolRegistry::new()),
            context: ToolContext::new(cwd),
        }
    }

    /// List all registered tools.
    pub fn list(&self) -> Vec<ToolInfo> {
        self.registry.list()
    }

    /// Execute a tool call using the registry and stored context.
    pub fn execute(&self, call: &ToolCall) -> ToolResult {
        self.registry.execute(call, &self.context)
    }

    /// Builder: set the agent role on the context.
    pub fn with_role(mut self, role: AgentRole) -> Self {
        self.context = self.context.with_role(role);
        self
    }

    /// Builder: attach a plan gate on the context.
    pub fn with_plan_gate(mut self, gate: std::sync::Arc<dyn ToolGate>) -> Self {
        self.context = self.context.with_plan_gate(gate);
        self
    }

    /// Execute a tool call using the registry. The optional `_session` parameter
    /// is ignored in this stub implementation.
    pub fn execute_with_session(&self, call: &ToolCall, _session: Option<()>) -> ToolResult {
        self.registry.execute(call, &self.context)
    }
}

impl Clone for ToolExecutor {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            context: self.context.clone(),
        }
    }
}

impl rustycode_tool_integration::tool_executor::ToolExecutorApi for ToolExecutor {
    fn list(&self) -> Vec<rustycode_tool_integration::tool_executor::ToolInfo> {
        self.registry
            .list()
            .into_iter()
            .map(|info| rustycode_tool_integration::tool_executor::ToolInfo {
                name: info.name,
                description: info.description,
                parameters_schema: info.parameters_schema,
                permission: match info.permission {
                    ToolPermission::None => rustycode_protocol::ToolPermission::Blocked,
                    ToolPermission::Read => rustycode_protocol::ToolPermission::Read,
                    ToolPermission::Write => rustycode_protocol::ToolPermission::Write,
                    ToolPermission::Execute => rustycode_protocol::ToolPermission::Execute,
                    ToolPermission::Network => rustycode_protocol::ToolPermission::Execute,
                    _ => rustycode_protocol::ToolPermission::RequiresConfirmation,
                },
                defer_loading: info.defer_loading,
            })
            .collect()
    }

    fn execute(&self, call: &rustycode_protocol::ToolCall) -> rustycode_protocol::ToolResult {
        self.registry.execute(call, &self.context)
    }
}

/// Create a default tool registry with all zero-config built-in tools.
///
/// Includes all built-in tools except those requiring runtime state (todo, semantic search, agent).
/// Stateful tools must be registered separately by the caller.
pub fn default_registry() -> ToolRegistry {
    use crate::providers::apply_patch::ApplyPatchTool;
    use crate::providers::brief::BriefTool;
    use crate::providers::browser_fetch::BrowserFetchTool;
    use crate::providers::codesearch::CodeSearchTool;
    use crate::providers::edit::EditFile;
    use crate::providers::lsp::*;
    use crate::providers::multiedit::MultiEditTool;
    use crate::providers::notebook::NotebookEditTool;
    use crate::providers::search::{GlobTool, GrepTool};
    use crate::providers::send_message::SendMessageTool;
    use crate::providers::task_output::{TaskOutputTool, TaskStopTool};
    use crate::providers::tool_search::ToolSearchTool;
    use crate::providers::web_fetch_tool::WebFetchTool;
    use crate::providers::web_search::WebSearchTool;
    use crate::providers::{
        BashTool, CmdTool, FindTool, GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool,
        InspectTool, ListDirTool, PowerShellTool, QuestionTool, ReadFileTool, WriteFileTool,
    };

    let mut reg = ToolRegistry::new();

    // Core file system tools
    reg.register(ReadFileTool);
    reg.register(WriteFileTool);
    reg.register(ListDirTool);
    reg.register(EditFile);

    // Search & exploration
    reg.register(GrepTool);
    reg.register(GlobTool);
    reg.register(FindTool);
    reg.register(InspectTool);
    reg.register(CodeSearchTool);
    reg.register(ApplyPatchTool);

    // Command execution — register platform-appropriate shells
    reg.register(BashTool);
    if crate::providers::powershell::find_pwsh().is_some() {
        reg.register(PowerShellTool);
    }
    if crate::providers::cmd::find_cmd().is_some() {
        reg.register(CmdTool);
    }

    // Git tools
    reg.register(GitStatusTool);
    reg.register(GitDiffTool);
    reg.register(GitLogTool);
    reg.register(GitCommitTool);

    // LSP (Language Server Protocol) tools - code intelligence
    reg.register(LspDiagnosticsTool);
    reg.register(LspHoverTool);
    reg.register(LspDefinitionTool);
    reg.register(LspCompletionTool);
    reg.register(LspDocumentSymbolsTool);
    reg.register(LspReferencesTool);
    reg.register(LspFullDiagnosticsTool);
    reg.register(LspCodeActionsTool);
    reg.register(LspRenameTool);
    reg.register(LspFormattingTool);
    reg.register(LspGetSymbolsOverviewTool);
    reg.register(LspFindSymbolTool);
    reg.register(LspReplaceSymbolBodyTool);
    reg.register(LspInsertBeforeSymbolTool);
    reg.register(LspInsertAfterSymbolTool);
    reg.register(LspSafeDeleteSymbolTool);
    reg.register(LspRenameSymbolTool);
    reg.register(LspAnalyzeSymbolTool);
    reg.register(LspExtractSymbolTool);
    reg.register(LspInlineSymbolTool);
    reg.register(LspWorkspaceSymbolsTool);

    // Web & search
    reg.register(WebFetchTool);
    reg.register(BrowserFetchTool);
    reg.register(WebSearchTool);

    // Edit tools
    reg.register(MultiEditTool);
    reg.register(NotebookEditTool);

    // Task management & communication
    reg.register(BriefTool);
    reg.register(SendMessageTool);
    reg.register(TaskOutputTool);
    reg.register(TaskStopTool);
    reg.register(ToolSearchTool);

    // Interactive tools
    reg.register(QuestionTool);

    reg
}
