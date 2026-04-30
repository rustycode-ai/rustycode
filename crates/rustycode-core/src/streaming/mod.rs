//! Unified streaming infrastructure for LLM provider responses
//!
//! This module provides shared utilities for processing provider responses
//! (which may be SSE or other streaming formats) by normalizing them to
//! `StreamEvent` from `rustycode-protocol`. It eliminates duplication between
//! the headless runtime and the TUI by providing a canonical event dispatch mechanism.
//!
//! Both consumers implement the `StreamingCallbacks` trait to handle semantic
//! events (text, thinking, tool completion) according to their own needs.

pub mod processor;
pub mod tool_state;

pub use processor::{SseEventProcessor, StreamEventProcessor, StreamingCallbacks};
pub use tool_state::{ToolAccumulator, ToolCall};
