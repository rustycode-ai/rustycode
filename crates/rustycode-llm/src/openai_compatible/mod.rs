//! Shared abstractions for OpenAI-compatible providers
//!
//! This module provides common types, conversion functions, and utilities
//! for providers that implement the OpenAI API specification.
//!
//! ## Supported Providers
//! - OpenAI (full feature set)
//! - Azure OpenAI
//! - Together AI
//! - OpenRouter
//! - Zhipu
//! - Perplexity
//! - GitHub Copilot
//!
//! ## Usage
//! ```rust,ignore
//! use rustycode_llm::openai_compatible::{
//!     OpenAiCompatibleRequest, OpenAiCompatibleMessage,
//!     convert_messages_simple, build_completion_response
//! };
//! ```

pub mod errors;
pub mod messages;
pub mod response;
pub mod sse;
pub mod tools;
pub mod types;

// Re-export commonly used items
pub use errors::map_http_error;
pub use messages::{convert_messages_simple, convert_messages_with_system};
pub use response::build_completion_response;
pub use sse::{parse_openai_sse_lines, SseParseConfig};
pub use tools::format_tool_calls_to_content;
pub use types::*;
