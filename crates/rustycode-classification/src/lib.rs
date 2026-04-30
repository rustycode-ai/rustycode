//! Task complexity classification for routing through orchestration.
//!
//! Provides [`UnifiedTaskClassifier`] to score task complexity and select the
//! first specialized agent role.

pub mod classifier;
pub mod types;

pub use classifier::{LocalTaskClassifier, RoleRouter, UnifiedTaskClassifier};
pub use rustycode_protocol::agent_protocol::AgentRole;
pub use types::{
    ClassificationReason, ClassificationResult, ComplexitySignals, ComplexityTier, PatternQuery,
    StoredPattern, TaskClassification, TaskComplexity,
};
