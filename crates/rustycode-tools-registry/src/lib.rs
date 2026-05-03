//! `RustyCode` Tools Registry - Tool discovery and registration.
//!
//! This crate provides the tool registry and discovery system:
//!
//! - **Tool Discovery**: Automatic discovery of available tools
//! - **Registration**: Dynamic tool registration and management
//! - **Metadata**: Tool metadata storage and querying
//! - **Plugins**: Plugin-based tool loading

pub mod dependency_resolver;
pub mod discovery;
pub mod metadata;
pub mod plugin_error;
pub mod plugin_loader;
pub mod plugin_manifest;
pub mod registry;

pub use dependency_resolver::DependencyResolver;
pub use discovery::ToolDiscovery;
pub use metadata::MetadataProvider;
pub use plugin_error::PluginError;
pub use plugin_loader::PluginLoader;
pub use plugin_manifest::{DependencySpec, PluginManifest};
pub use registry::{RegistryConfig, ToolMetadata, ToolRegistry};
