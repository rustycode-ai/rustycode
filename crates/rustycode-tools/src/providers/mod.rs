// providers/ — Tool implementation files (~28K LOC total)
//
// Each file implements one or more Tool trait providers. These are the
// concrete tool implementations that get registered into ToolRegistry.

pub mod apply_patch;
pub mod bash;
pub mod brief;
pub mod browser_fetch;
pub mod builtin;
pub mod check_integration;
pub mod cmd;
pub mod codesearch;
pub mod cron;
pub mod decompose;
pub mod delegation_tool;
pub mod docker;
pub mod docker_isolation;
pub mod edit;
pub mod explore;
pub mod fs;
pub mod git;
pub mod git_provider;
pub mod glob;
pub mod goal;
pub mod grep;
pub mod guide_research;
pub mod list_dir;
pub mod lsp;
pub mod mcp_resources;
pub mod multiedit;
pub mod notebook;
pub mod powershell;
pub mod question;
pub mod read_file;
pub mod reasoning_types;
pub mod repl;
pub mod send_message;
pub mod skill_discovery;
pub mod symbol;
pub mod task_output;
pub mod team;
pub mod tool_search;
pub mod validate_requirements;
pub mod web_fetch_tool;
pub mod web_search;
pub mod write_file;

// Re-exports for backward-compatible access via `rustycode_tools::TypeName`
#[allow(ambiguous_glob_reexports)]
pub use apply_patch::*;
#[allow(ambiguous_glob_reexports)]
pub use bash::*;
#[allow(ambiguous_glob_reexports)]
pub use brief::*;
#[allow(ambiguous_glob_reexports)]
pub use browser_fetch::*;
#[allow(ambiguous_glob_reexports)]
pub use check_integration::*;
#[allow(ambiguous_glob_reexports)]
pub use cmd::*;
#[allow(ambiguous_glob_reexports)]
pub use codesearch::*;
#[allow(ambiguous_glob_reexports)]
pub use cron::*;
#[allow(ambiguous_glob_reexports)]
pub use decompose::*;
#[allow(ambiguous_glob_reexports)]
pub use delegation_tool::*;
#[allow(ambiguous_glob_reexports)]
pub use docker::*;
#[allow(ambiguous_glob_reexports)]
pub use docker_isolation::*;
#[allow(ambiguous_glob_reexports)]
pub use edit::*;
#[allow(ambiguous_glob_reexports)]
pub use explore::*;
#[allow(ambiguous_glob_reexports)]
pub use git::*;
#[allow(ambiguous_glob_reexports)]
pub use glob::*;
#[allow(ambiguous_glob_reexports)]
pub use goal::*;
#[allow(ambiguous_glob_reexports)]
pub use grep::*;
#[allow(ambiguous_glob_reexports)]
pub use guide_research::*;
#[allow(ambiguous_glob_reexports)]
pub use list_dir::*;
#[allow(ambiguous_glob_reexports)]
pub use lsp::*;
#[allow(ambiguous_glob_reexports)]
pub use mcp_resources::*;
#[allow(ambiguous_glob_reexports)]
pub use multiedit::*;
#[allow(ambiguous_glob_reexports)]
pub use notebook::*;
#[allow(ambiguous_glob_reexports)]
pub use powershell::*;
#[allow(ambiguous_glob_reexports)]
pub use question::*;
#[allow(ambiguous_glob_reexports)]
pub use read_file::*;
#[allow(ambiguous_glob_reexports)]
pub use reasoning_types::*;
#[allow(ambiguous_glob_reexports)]
pub use repl::*;
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
pub use web_fetch_tool::*;
#[allow(ambiguous_glob_reexports)]
pub use web_search::*;
#[allow(ambiguous_glob_reexports)]
pub use write_file::*;
