//! Unified command dispatch for AppShell.
//!
//! Centralizes command routing from multiple sources (slash commands, keyboard shortcuts,
//! service-triggered changes) and routes them to features via the FeatureRegistry.
//!
//! Extends the existing CommandContext/CommandEffect pattern to work with the feature system.

use crate::app::features::FeatureRegistry;
use anyhow::Result;

/// Identifies the source of a command invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSource {
    /// User typed a slash command (e.g., `/help`)
    SlashCommand,
    /// User pressed a keyboard shortcut
    KeyboardShortcut,
    /// Service triggered a UI change programmatically
    Service,
}

/// A command invocation that can be routed to a feature.
#[derive(Debug, Clone)]
pub struct CommandInvocation {
    /// The command name (e.g., "help", "plugin", "model")
    pub name: String,
    /// Optional arguments for the command
    pub args: Vec<String>,
    /// Where the command came from
    pub source: CommandSource,
}

/// Outcome of routing a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingResult {
    /// Command is handled by a registered feature
    RoutedToFeature(&'static str),
    /// Command is a built-in (handled outside feature system)
    BuiltIn,
    /// Command not recognized
    NotFound,
}

/// Orchestrates unified command dispatch through the feature registry.
///
/// CommandDispatch:
/// 1. Accepts command invocations from multiple sources
/// 2. Routes to the appropriate feature via FeatureRegistry
/// 3. Handles built-in commands that aren't feature-based
/// 4. Provides centralized dispatch logic to replace inline match arms
///
/// This allows gradual migration from the monolithic command dispatcher
/// to the feature-based system.
pub struct CommandDispatch;

impl CommandDispatch {
    /// Route a command invocation to the appropriate handler.
    ///
    /// Returns the feature ID that should handle this command, or a result
    /// indicating whether it's a built-in or unrecognized command.
    pub fn route(invocation: &CommandInvocation, registry: &FeatureRegistry) -> RoutingResult {
        // Try to find the command in the feature registry first
        if let Some(feature_id) = registry.command_feature(&invocation.name) {
            return RoutingResult::RoutedToFeature(feature_id);
        }

        // List of built-in commands that are not feature-based (yet)
        // These are commands that are handled directly by the TUI event loop
        // or through special handlers, not through the feature system.
        const BUILT_IN_COMMANDS: &[&str] = &[
            "quit",      // Exit the application
            "exit",      // Exit the application (alias)
            "q",         // Exit the application (alias)
            "clear",     // Clear conversation
            "workspace", // Workspace management
            "extract",   // Extract tasks/todos
            "rename",    // Rename session
            "retry",     // Retry last message
            "yolo",      // Autonomous mode
            "auto",      // Autonomous mode (alias)
        ];

        if BUILT_IN_COMMANDS.contains(&invocation.name.as_str()) {
            return RoutingResult::BuiltIn;
        }

        RoutingResult::NotFound
    }

    /// Check if a command is registered (feature-based or built-in).
    pub fn is_registered(name: &str, registry: &FeatureRegistry) -> bool {
        let invocation = CommandInvocation {
            name: name.to_string(),
            args: Vec::new(),
            source: CommandSource::SlashCommand,
        };
        Self::route(&invocation, registry) != RoutingResult::NotFound
    }

    /// Get all registered command names from the registry.
    pub fn registered_commands(registry: &FeatureRegistry) -> impl Iterator<Item = String> + '_ {
        registry.commands().map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_invocation_has_source() {
        let cmd = CommandInvocation {
            name: "help".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        assert_eq!(cmd.source, CommandSource::SlashCommand);
    }

    #[test]
    fn routing_result_distinguishes_types() {
        assert_ne!(RoutingResult::BuiltIn, RoutingResult::NotFound);
        assert_ne!(
            RoutingResult::BuiltIn,
            RoutingResult::RoutedToFeature("test")
        );
    }

    #[test]
    fn command_dispatch_routes_built_in_commands() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "quit".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        assert_eq!(
            CommandDispatch::route(&cmd, &registry),
            RoutingResult::BuiltIn
        );
    }

    #[test]
    fn command_dispatch_returns_not_found_for_unknown() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "unknown_command_xyz".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        assert_eq!(
            CommandDispatch::route(&cmd, &registry),
            RoutingResult::NotFound
        );
    }

    #[test]
    fn command_source_copy_is_cheap() {
        let source = CommandSource::KeyboardShortcut;
        let _copy = source;
        let _another = source;
    }

    #[test]
    fn is_registered_identifies_built_in_commands() {
        let registry = crate::app::features::FeatureRegistry::new();
        assert!(CommandDispatch::is_registered("quit", &registry));
        assert!(CommandDispatch::is_registered("clear", &registry));
        assert!(!CommandDispatch::is_registered(
            "xyz_nonexistent",
            &registry
        ));
    }

    #[test]
    fn command_invocation_with_args() {
        let cmd = CommandInvocation {
            name: "load".to_string(),
            args: vec!["session1".to_string(), "session2".to_string()],
            source: CommandSource::SlashCommand,
        };
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0], "session1");
    }
}
