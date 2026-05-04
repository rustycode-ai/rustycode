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
pub mod responses;
pub mod responses_sse;
pub mod sse;
pub mod tools;
pub mod types;

#[cfg(feature = "ws")]
pub mod responses_ws;

// Re-export commonly used items
pub use errors::{build_request_with_auth, map_http_error};
pub use messages::{convert_messages_simple, convert_messages_to_responses_input, convert_messages_with_system};
pub use response::build_completion_response;
pub use responses::{build_responses_completion_response, ResponsesApiInputItem, ResponsesApiReasoning, ResponsesApiReasoningSummary, ResponsesApiRequest, ResponsesApiResponse, ResponsesApiTool};
pub use responses_sse::{dispatch_responses_event, extract_responses_error, parse_responses_sse_lines, ResponsesSseState};
pub use sse::{parse_openai_sse_lines, SseParseConfig, SseParseState};
pub use tools::format_tool_calls_to_content;
pub use types::*;
