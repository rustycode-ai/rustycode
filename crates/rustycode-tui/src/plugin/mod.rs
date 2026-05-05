//! Experimental plugin system for RustyCode TUI.
//!
//! Discovery and manifest parsing work; dynamic library loading and permission
//! enforcement are not yet implemented.

pub mod api;
pub mod manager;
pub mod manifest;
pub mod permissions;
pub mod ui;

pub use api::{CommandHandler, PluginCommandResult as CommandResult, PluginAPI};
pub use manager::PluginManager;
pub use manifest::PluginManifest;
pub use permissions::{Permission, PluginPermissions};
pub use ui::PluginManagerUI;
