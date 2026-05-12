//! Stream chunk handlers and result processors for the TUI.
//!
//! The `handle_stream_chunk` function is the main entry point that dispatches
//! incoming `StreamChunk` variants to focused handler functions.

mod event_msg;
mod helpers;
mod stream_approval;
mod stream_core;
mod stream_data;
mod stream_done;
mod stream_error;
mod stream_stopped;
mod stream_tools;
#[cfg(test)]
mod tests;
mod tool_result;
mod workspace;

// Public entry points used by service_polling.rs and workspace tests
pub(crate) use event_msg::handle_event_msg;
pub(crate) use stream_core::handle_stream_chunk;
pub use tool_result::handle_tool_result;
pub use workspace::handle_slash_command_result;
pub use workspace::handle_workspace_update;
