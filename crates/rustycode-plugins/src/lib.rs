//! Plugin system foundation for `RustyCode`
//!
//! This module provides the core traits and registry for dynamically loading and managing plugins.
//! It supports three types of plugins:
//! - `ToolPlugin`: Tools that can be used by the agent
//! - `AgentPlugin`: Agents that can execute tasks
//! - `LLMProviderPlugin`: LLM providers for model integration
//!
//! # Quick Start
//!
//! ```ignore
//! use rustycode_plugins::{PluginRegistry, ToolPlugin, PluginMetadata};
//!
//! let mut registry = PluginRegistry::new();
//! // Register plugins...
//! ```

// Workspace lint allowances for patterns used throughout this crate.
#![allow(
    clippy::missing_fields_in_debug,
    clippy::missing_const_for_fn,
    clippy::or_fun_call,
    clippy::redundant_closure_for_method_calls,
    clippy::significant_drop_tightening,
    clippy::str_to_string,
    clippy::trait_duplication_in_bounds,
    clippy::type_repetition_in_bounds,
    clippy::unnecessary_literal_bound,
    clippy::use_self
)]
#![cfg_attr(
    test,
    allow(
        clippy::cast_precision_loss,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::unwrap_used
    )
)]

pub mod config;
pub mod dependency_resolver;
pub mod error;
pub mod lifecycle;
pub mod manifest;
pub mod metadata;
pub mod registry;
pub mod status;
pub mod traits;

pub use config::{ConfigBuilder, ConfigValue, PluginConfig, SensitiveValue};
pub use dependency_resolver::DependencyResolver;
pub use error::PluginError;
pub use lifecycle::PluginLifecycleManager;
pub use manifest::{DependencySpec, PluginManifest};
pub use metadata::PluginMetadata;
pub use registry::PluginRegistry;
pub use status::PluginStatus;
pub use traits::{AgentPlugin, LLMProviderPlugin, ToolPlugin};

#[cfg(test)]
mod tests;
