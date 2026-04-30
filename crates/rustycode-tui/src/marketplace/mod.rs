//! Marketplace system for skills, tools, and MCP servers
//!
//! This module provides a marketplace functionality similar to an app store,
//! allowing users to browse, search, install, and manage community-created
//! extensions for the RustyCode TUI.

pub mod client;
pub mod index;
pub mod installer;
pub mod registry;
pub mod updates;

// Re-export commonly used types
pub use index::{ItemType, MarketplaceItem, UpdateAvailable};
pub use registry::{RegistryManager, RegistryStatistics};
