//! Routing module -- complexity classification and model tier selection.
//!
//! Provides [`ComplexityClassifier`] for scoring task descriptors and
//! [`ModelRouter`] for mapping complexity scores to [`ExecutionTier`] values.

pub mod complexity_classifier;
pub mod model_router;

pub use complexity_classifier::{ComplexityClassifier, TaskComplexity, TaskDescriptor};
pub use model_router::{ModelRouter, RoutingPolicy};
