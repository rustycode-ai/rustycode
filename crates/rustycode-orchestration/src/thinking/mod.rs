//! Integrated thinking module (Graph-of-Thoughts).

pub mod activator;
pub mod budget;
pub mod convergence;
pub mod core;
pub mod executor;
pub mod executor_with_persistence;
pub mod operations;
pub mod persistence;
pub mod persistence_helpers;
pub mod prompting;
pub mod selection;
pub mod strategies;

pub use activator::{
    ActivationSignals, ConservativeActivationPolicy, DefaultActivationPolicy, SignalRisk,
    SignalTier, ThinkingActivationPolicy,
};
pub use budget::ThinkingBudget;
pub use core::{
    error::{Error, Result},
    graph::ReasoningGraph,
    types::{Operation, Thought, ThoughtId},
};
pub use executor::ThinkingExecutor;
pub use operations::OperationExecutor;
pub use persistence::{SerializedGraph, SessionManager};
pub use persistence_helpers::{deserialize_graph, serialize_graph};
pub use prompting::PromptContext;
