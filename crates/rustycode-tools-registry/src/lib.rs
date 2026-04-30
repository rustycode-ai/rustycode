//! `RustyCode` Tools Registry - Tool discovery and registration.
//!
//! This crate provides the tool registry and discovery system:
//!
//! - **Tool Discovery**: Automatic discovery of available tools
//! - **Registration**: Dynamic tool registration and management
//! - **Metadata**: Tool metadata storage and querying
//! - **Plugins**: Plugin-based tool loading

pub mod discovery;
pub mod metadata;
pub mod plugin_loader;
pub mod registry;

pub use discovery::ToolDiscovery;
pub use metadata::MetadataProvider;
pub use registry::{RegistryConfig, ToolMetadata, ToolRegistry};
