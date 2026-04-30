//! Plugin system for RustyCode TUI
//!
//! # Experimental Status
//!
//! **This plugin system is experimental and not yet ready for production use.**
//!
//! The architecture is designed, but key functionality is not yet implemented:
//!
//! - **No dynamic library loading**: Plugins are discovered from manifests but
//!   their code cannot be executed
//! - **No permission enforcement**: Permissions are parsed but not enforced
//! - **Commands are placeholders**: Slash commands return messages instead of
//!   executing real code
//!
//! ## Design Overview
//!
//! When complete, this system will allow:
//!
//! - Plugin discovery via `plugin.toml` manifests in `~/.rustycode/plugins/`
//! - Dynamic loading of compiled plugin libraries (`.so`, `.dylib`, `.dll`)
//! - Slash command registration from plugins
//! - Permission-based sandboxing for untrusted plugins
//! - UI for managing installed plugins
//!
//! ## Module Structure
//!
//! - [`api`]: Plugin API exposed to loaded plugins
//! - [`manager`]: Core plugin discovery and loading logic
//! - [`manifest`]: Plugin manifest (`plugin.toml`) parsing
//! - [`permissions`]: Permission types and checking
//! - [`ui`]: TUI components for plugin management

pub mod api;
pub mod manager;
pub mod manifest;
pub mod permissions;
pub mod ui;

pub use api::{CommandHandler, CommandResult, PluginAPI};
pub use manager::PluginManager;
pub use manifest::PluginManifest;
pub use permissions::{Permission, PluginPermissions};
pub use ui::PluginManagerUI;
