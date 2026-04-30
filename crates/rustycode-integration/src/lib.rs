#![allow(clippy::doc_markdown)]

//! `RustyCode` Integration Layer
//!
//! Integration metrics for orchestration vs. legacy execution comparison.
//!
//! Task classification and execution routing now live in `rustycode-classification`,
//! `rustycode-runtime`, and `rustycode-orchestration`.

pub mod metrics;
pub mod router;
pub mod shadow_mode;

pub use metrics::*;
pub use router::*;
pub use shadow_mode::*;
