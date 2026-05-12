// ── Error Recovery System ────────────────────────────────────────────────────────

//! Error recovery system for graceful failure handling.
//!
//! This module provides intelligent error classification, recovery strategies, and
//! automatic fallback mechanisms to make the RustyCode runtime resilient to failures.
//!
//! ## Recovery Strategies
//!
//! The system supports four recovery strategies:
//!
//! - **Retry**: Automatically retry failed operations with exponential backoff
//! - **Skip**: Skip non-critical failures and continue execution
//! - **Abort**: Stop execution immediately for critical failures
//! - **Fallback**: Use alternative implementations or cached results
//!
//! ## Example
//!
//! ```rust
//! use rustycode_core::recovery::{
//!     RecoveryEngine, RecoveryStrategy,
//!     RecoveryConfig,
//! };
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let config = RecoveryConfig::default();
//! let engine = RecoveryEngine::new(config);
//!
//! // Attempt a recoverable operation
//! let result = engine.recover::<String, std::io::Error, _>(
//!     anyhow::anyhow!("Temporary network failure"),
//!     "network_operation",
//!     &|| async { Err::<String, _>(std::io::Error::new(std::io::ErrorKind::Other, "Failed")) },
//! ).await?;
//!
//! match result.strategy_used() {
//!     RecoveryStrategy::Retry => println!("Retried successfully"),
//!     RecoveryStrategy::Fallback => println!("Used fallback"),
//!     _ => {}
//! }
//! # Ok(())
//! # }
//! ```

pub mod checkpoint;
pub mod classification;
pub mod config;
pub mod detector;
pub mod engine;
pub mod result;
pub mod snapshot;
pub mod snapshot_validator;
pub mod state_machine;
pub mod store;
pub mod strategy;
pub mod validation;

// Re-exports for backward compatibility
pub use checkpoint::CheckpointRecovery;
pub use classification::{ErrorClassification, ErrorClassifier};
pub use config::{RecoveryConfig, RetryConfig};
pub use detector::ExecutionCheckpointDetector;
pub use engine::RecoveryEngine;
pub use result::{RecoveryLogEntry, RecoveryResult};
pub use snapshot::{CheckpointSnapshot, ExecutionPhase};
pub use snapshot_validator::{CheckpointValidator, ValidationReport};
pub use state_machine::{Recovery, RecoveryState};
pub use store::CheckpointStore;
pub use strategy::{ErrorCategory, RecoveryStrategy};
pub use validation::CheckpointValidator as SessionCheckpointValidator;
