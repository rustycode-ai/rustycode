//! Shared LLM data types
//!
//! Pure data types used across crates for LLM operations. These types have
//! no async dependencies and are safe to use in any crate.

mod types;

pub use types::{Cost, TokenCount, Usage};
