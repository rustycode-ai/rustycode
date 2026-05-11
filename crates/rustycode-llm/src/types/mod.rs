//! Shared LLM types extracted from provider.rs.
//!
//! These types are re-exported from `provider.rs` for backward compatibility.
//! New code should import from `crate::types::*`.

pub mod config;
pub mod error;
pub mod message;
pub mod request;
pub mod response;
pub mod streaming;

// Re-export all public items from submodules for convenient access
pub use config::*;
pub use error::*;
pub use message::*;
pub use request::*;
pub use response::*;
// Note: streaming types (SSEEvent, ContentBlockType, ContentDelta) are pub(crate)
// and are NOT re-exported beyond the crate boundary. StreamChunk is pub.
pub use streaming::*;
