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
    clippy::future_not_send,
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
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::non_std_lazy_statics,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::significant_drop_tightening,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used
)]
#![cfg_attr(test, allow(clippy::float_cmp,))]
//! `RustyCode` ACP (Agent Client Protocol) Implementation
//!
//! This crate provides an implementation of the Agent Client Protocol
//! for `RustyCode`, making it compatible with ACP clients like Zed, VS Code, etc.
//!
//! # ACP Overview
//!
//! ACP is a standardized protocol for AI agent servers using JSON-RPC over stdio.
//! Specification: <https://agentclientprotocol.com/>
//!
//! # Usage
//!
//! ```bash
//! # Start the ACP server
//! rustycode-acp
//!
//! # Start in a specific directory
//! rustycode-acp --cwd /path/to/project
//! ```
//!
//! # Protocol Support
//!
//! - ✅ `initialize` - Protocol negotiation
//! - ✅ `session/new` - Create sessions
//! - ✅ `session/load` - Resume sessions
//! - ⏳ `session/prompt` - Process messages (basic support)
//! - ❌ Streaming responses (planned)
//! - ❌ Tool progress reporting (planned)

pub mod dispatcher;
pub mod llm_integration;
pub mod prompt_handler;
pub mod server;
pub mod tool_executor;
pub mod types;

pub use dispatcher::ACPDispatcher;
pub use prompt_handler::PromptHandler;
pub use server::ACPServer;
pub use types::*;

/// ACP protocol version
pub const ACP_PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lib_acp_protocol_version() {
        assert_eq!(ACP_PROTOCOL_VERSION, 1);
    }

    #[test]
    fn test_lib_re_exports_match_types_module() {
        // The lib re-exports ACP_PROTOCOL_VERSION which should match types::ACP_PROTOCOL_VERSION
        assert_eq!(ACP_PROTOCOL_VERSION, types::ACP_PROTOCOL_VERSION);
    }

    #[test]
    fn test_acp_server_new_via_reexport() {
        // Verify ACPServer can be constructed via the re-exported type
        let _server = ACPServer::new();
    }

    #[test]
    fn test_prompt_handler_default_via_reexport() {
        let _handler = PromptHandler::default();
    }
}
