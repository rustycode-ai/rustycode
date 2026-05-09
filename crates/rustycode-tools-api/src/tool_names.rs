//! Re-export of canonical tool name constants from `rustycode-protocol`.
//!
//! The canonical definitions live in `rustycode-protocol/src/tool_names.rs`.
//! This module re-exports them so crates depending on `rustycode-tools-api`
//! can use either `rustycode_protocol::tool_names` or
//! `rustycode_tools_api::tool_names` interchangeably.

pub use rustycode_protocol::tool_names::*;
