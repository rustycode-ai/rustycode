// providers/ — Tool implementation files (~28K LOC total)
//
// Each file implements one or more Tool trait providers. These are the
// concrete tool implementations that get registered into ToolRegistry.

pub mod apply_patch;
pub mod bash;
pub mod brief;
pub mod builtin;
pub mod check_integration;
pub mod claude_text_editor;
pub mod codesearch;
pub mod compile_time;
pub mod cron;
pub mod database;
pub mod decompose;
pub mod docker;
pub mod docker_isolation;
pub mod edit;
pub mod fs;
pub mod git;
pub mod git_provider;
pub mod guide_research;
pub mod lsp;
pub mod mcp_resources;
pub mod multiedit;
pub mod notebook;
pub mod question;
pub mod reasoning_types;
pub mod repl;
pub mod search;
pub mod search_replace;
pub mod send_message;
pub mod skill_discovery;
pub mod symbol;
pub mod task_output;
pub mod team;
pub mod tool_search;
pub mod validate_requirements;
pub mod web_search;

// Re-exports for backward-compatible access via `rustycode_tools::TypeName`
#[allow(ambiguous_glob_reexports)]
pub use apply_patch::*;
#[allow(ambiguous_glob_reexports)]
pub use bash::*;
#[allow(ambiguous_glob_reexports)]
pub use brief::*;
#[allow(ambiguous_glob_reexports)]
pub use check_integration::*;
#[allow(ambiguous_glob_reexports)]
pub use claude_text_editor::*;
#[allow(ambiguous_glob_reexports)]
pub use codesearch::*;
// compile_time defines its own Tool/ToolPermission types that shadow the
// crate-level re-exports. Skip glob re-export; consumers access it via
// providers::compile_time:: directly.
#[allow(ambiguous_glob_reexports)]
pub use cron::*;
#[allow(ambiguous_glob_reexports)]
pub use database::*;
#[allow(ambiguous_glob_reexports)]
pub use decompose::*;
#[allow(ambiguous_glob_reexports)]
pub use docker::*;
#[allow(ambiguous_glob_reexports)]
pub use docker_isolation::*;
#[allow(ambiguous_glob_reexports)]
pub use edit::*;
#[allow(ambiguous_glob_reexports)]
pub use fs::*;
#[allow(ambiguous_glob_reexports)]
pub use git::*;
#[allow(ambiguous_glob_reexports)]
pub use guide_research::*;
#[allow(ambiguous_glob_reexports)]
pub use lsp::*;
#[allow(ambiguous_glob_reexports)]
pub use mcp_resources::*;
#[allow(ambiguous_glob_reexports)]
pub use multiedit::*;
#[allow(ambiguous_glob_reexports)]
pub use notebook::*;
#[allow(ambiguous_glob_reexports)]
pub use question::*;
#[allow(ambiguous_glob_reexports)]
pub use reasoning_types::*;
#[allow(ambiguous_glob_reexports)]
pub use repl::*;
#[allow(ambiguous_glob_reexports)]
pub use search::*;
#[allow(ambiguous_glob_reexports)]
pub use search_replace::*;
#[allow(ambiguous_glob_reexports)]
pub use send_message::*;
#[allow(ambiguous_glob_reexports)]
pub use skill_discovery::*;
#[allow(ambiguous_glob_reexports)]
pub use symbol::*;
#[allow(ambiguous_glob_reexports)]
pub use task_output::*;
#[allow(ambiguous_glob_reexports)]
pub use team::*;
#[allow(ambiguous_glob_reexports)]
pub use tool_search::*;
#[allow(ambiguous_glob_reexports)]
pub use validate_requirements::*;
#[allow(ambiguous_glob_reexports)]
pub use web_search::*;
