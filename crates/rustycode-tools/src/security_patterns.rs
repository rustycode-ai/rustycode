//! Re-export of security threat patterns from `rustycode-tools-security`.
//!
//! This module was previously a 748-line duplicate of
//! `rustycode-tools-security/src/patterns.rs`. It now re-exports the
//! canonical implementation to avoid divergence.

pub use rustycode_tools_security::patterns::*;
