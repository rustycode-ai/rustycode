//! Unified command dispatch for AppShell.
//!
//! Centralizes command routing from multiple sources (slash commands, keyboard shortcuts,
//! service-triggered changes) and routes them to features via the FeatureRegistry.
//!
//! Extends the existing CommandContext/CommandEffect pattern to work with the feature system.

use crate::app::features::FeatureRegistry;

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

/// Outcome of routing a command (legacy, used by existing code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingResult {
    /// Command is handled by a registered feature
    RoutedToFeature(&'static str),
    /// Command is a built-in (handled outside feature system)
    BuiltIn,
    /// Command not recognized
    NotFound,
}

/// Typed destination for a routed command.
///
/// Every known slash command maps to exactly one variant. This is the unified
/// routing result used by `route_destination()`, `dispatch_key_event()`, and
/// `dispatch_service_event()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandDestination {
    /// Feature will handle this command via the feature system.
    Feature {
        feature_id: &'static str,
        action: String,
    },
    /// Built-in command handled by legacy dispatch (not yet feature-based).
    BuiltIn { name: &'static str },
    /// Slash command dispatched via the existing handler in commands/mod.rs.
    LegacySlash { names: &'static [&'static str] },
}

/// Complete list of all slash command name groups from `REGISTERED_SLASH_COMMANDS`.
///
/// Each inner slice corresponds to one `SlashCommandPlugin` entry in
/// `commands/mod.rs`. Order does not matter for routing but is kept in
/// alphabetical-ish order for readability.
const ALL_SLASH_COMMAND_GROUPS: &[&[&str]] = &[
    &["/agent"],
    &["/team"],
    &["/plan"],
    &["/yolo", "/auto"],
    &["/act"],
    &["/ask"],
    &["/harness"],
    &["/clear"],
    &["/workspace"],
    &["/extract"],
    &["/rename"],
    &["/quit", "/exit", "/q"],
    &["/compact"],
    &["/review"],
    &["/save"],
    &["/load"],
    &["/memory"],
    &["/marketplace"],
    &["/plugin", "/plugins"],
    &["/task", "/todo"],
    &["/orchestra"],
    &["/help"],
    &["/copilot-login"],
    &["/theme", "/t"],
    &["/model"],
    &["/provider"],
    &["/skill", "/skills"],
    &["/skillify"],
    &["/mcp"],
    &["/lsp"],
    &["/hook"],
    &["/undo"],
    &["/diff"],
    &["/export"],
    &["/learnings"],
    &["/workers"],
    &["/cron"],
    &["/stats"],
    &["/track", "/progress"],
    &["/cost", "/usage"],
    &["/checkpoint", "/checkpoints"],
    &["/resume"],
    &["/tokens"],
    &["/retry"],
    &["/sessions"],
    &["/feedback", "/bug"],
];

/// Built-in commands that are handled directly by the TUI event loop or
/// through special handlers, not through the feature system.
const BUILT_IN_COMMANDS: &[&str] = &[
    "quit",
    "exit",
    "q",
    "clear",
    "workspace",
    "extract",
    "rename",
    "retry",
    "yolo",
    "auto",
];

/// Orchestrates unified command dispatch through the feature registry.
pub struct CommandDispatch;

impl CommandDispatch {
    /// Route a command invocation to the appropriate handler (legacy API).
    pub fn route(invocation: &CommandInvocation, registry: &FeatureRegistry) -> RoutingResult {
        if let Some(feature_id) = registry.command_feature(&invocation.name) {
            return RoutingResult::RoutedToFeature(feature_id);
        }

        if BUILT_IN_COMMANDS.contains(&invocation.name.as_str()) {
            return RoutingResult::BuiltIn;
        }

        if Self::is_legacy_slash_command(&invocation.name) {
            return RoutingResult::BuiltIn;
        }

        RoutingResult::NotFound
    }

    /// Route a command invocation to a typed `CommandDestination`.
    ///
    /// Returns:
    /// - `Feature { .. }` if the command is registered in the FeatureRegistry.
    /// - `BuiltIn { .. }` if the command is in the built-in list.
    /// - `LegacySlash { .. }` if the command is a known slash command but
    ///   not feature-based or built-in.
    /// - `None` if the command is not recognized.
    pub fn route_destination(
        invocation: &CommandInvocation,
        registry: &FeatureRegistry,
    ) -> Option<CommandDestination> {
        if let Some(feature_id) = registry.command_feature(&invocation.name) {
            return Some(CommandDestination::Feature {
                feature_id,
                action: invocation.name.clone(),
            });
        }

        if let Some(&name) = BUILT_IN_COMMANDS.iter().find(|&&c| c == invocation.name) {
            return Some(CommandDestination::BuiltIn { name });
        }

        if let Some(names) = Self::find_slash_group(&invocation.name) {
            return Some(CommandDestination::LegacySlash { names });
        }

        None
    }

    /// Dispatch a keyboard shortcut by looking up the key in the registry.
    ///
    /// Returns `Some(CommandDestination::Feature { .. })` if the key matches
    /// a registered keymap, or `None` if the key is not registered.
    pub fn dispatch_key_event(
        key_str: &str,
        registry: &FeatureRegistry,
    ) -> Option<CommandDestination> {
        registry
            .keymap_feature(key_str)
            .map(|(feature_id, action)| CommandDestination::Feature {
                feature_id,
                action: action.to_string(),
            })
    }

    /// Dispatch a service-triggered event.
    ///
    /// Stub for T12 — currently returns `None` for all events.
    pub fn dispatch_service_event(
        _event_type: &str,
        _registry: &FeatureRegistry,
    ) -> Option<CommandDestination> {
        None
    }

    /// Check if a command is registered (feature-based, built-in, or legacy slash).
    pub fn is_registered(name: &str, registry: &FeatureRegistry) -> bool {
        let invocation = CommandInvocation {
            name: name.to_string(),
            args: Vec::new(),
            source: CommandSource::SlashCommand,
        };
        Self::route(&invocation, registry) != RoutingResult::NotFound
    }

    fn is_legacy_slash_command(name: &str) -> bool {
        Self::find_slash_group(name).is_some()
    }

    fn find_slash_group(name: &str) -> Option<&'static [&'static str]> {
        let bare = name.strip_prefix('/').unwrap_or(name);
        ALL_SLASH_COMMAND_GROUPS
            .iter()
            .find(|group| {
                group
                    .iter()
                    .any(|&cmd| cmd.strip_prefix('/').unwrap_or(cmd) == bare)
            })
            .copied()
    }

    /// Get all registered command names from the registry.
    pub fn registered_commands(registry: &FeatureRegistry) -> impl Iterator<Item = String> + '_ {
        registry.commands().map(|s| s.to_string())
    }

    /// Get the complete set of all known slash command names (for validation/testing).
    pub fn all_known_slash_commands() -> impl Iterator<Item = &'static str> {
        ALL_SLASH_COMMAND_GROUPS
            .iter()
            .flat_map(|group| group.iter().copied())
    }

    /// Convert a `CommandDestination` to the legacy `RoutingResult`.
    pub fn destination_to_routing(dest: &CommandDestination) -> RoutingResult {
        match dest {
            CommandDestination::Feature { feature_id, .. } => {
                RoutingResult::RoutedToFeature(feature_id)
            }
            CommandDestination::BuiltIn { .. } | CommandDestination::LegacySlash { .. } => {
                RoutingResult::BuiltIn
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_destination_feature_equality() {
        let a = CommandDestination::Feature {
            feature_id: "plugin_manager",
            action: "/plugin".to_string(),
        };
        let b = CommandDestination::Feature {
            feature_id: "plugin_manager",
            action: "/plugin".to_string(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn command_destination_feature_inequality() {
        let a = CommandDestination::Feature {
            feature_id: "plugin_manager",
            action: "/plugin".to_string(),
        };
        let b = CommandDestination::Feature {
            feature_id: "other",
            action: "/plugin".to_string(),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn command_destination_builtin_equality() {
        let a = CommandDestination::BuiltIn { name: "quit" };
        let b = CommandDestination::BuiltIn { name: "quit" };
        assert_eq!(a, b);
    }

    #[test]
    fn command_destination_legacy_slash_equality() {
        let a = CommandDestination::LegacySlash {
            names: &["/quit", "/exit", "/q"],
        };
        let b = CommandDestination::LegacySlash {
            names: &["/quit", "/exit", "/q"],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn command_destination_different_variants_not_equal() {
        let a = CommandDestination::BuiltIn { name: "quit" };
        let b = CommandDestination::LegacySlash {
            names: &["/quit", "/exit", "/q"],
        };
        assert_ne!(a, b);
    }

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

    #[test]
    fn route_destination_returns_feature_for_registered_command() {
        let mut registry = crate::app::features::FeatureRegistry::new();
        registry.register_command("/plugin", "plugin_manager");

        let cmd = CommandInvocation {
            name: "/plugin".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let dest = CommandDispatch::route_destination(&cmd, &registry);
        assert_eq!(
            dest,
            Some(CommandDestination::Feature {
                feature_id: "plugin_manager",
                action: "/plugin".to_string(),
            })
        );
    }

    #[test]
    fn route_destination_returns_builtin_for_builtin_command() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "quit".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let dest = CommandDispatch::route_destination(&cmd, &registry);
        assert_eq!(dest, Some(CommandDestination::BuiltIn { name: "quit" }));
    }

    #[test]
    fn route_destination_returns_legacy_slash_for_known_command() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "/help".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let dest = CommandDispatch::route_destination(&cmd, &registry);
        assert!(matches!(dest, Some(CommandDestination::LegacySlash { .. })));
        if let Some(CommandDestination::LegacySlash { names }) = dest {
            assert!(names.contains(&"/help"));
        }
    }

    #[test]
    fn route_destination_returns_none_for_unknown() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "xyz_unknown".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        assert_eq!(CommandDispatch::route_destination(&cmd, &registry), None);
    }

    #[test]
    fn route_destination_strips_slash_for_matching() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "help".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let dest = CommandDispatch::route_destination(&cmd, &registry);
        assert!(matches!(dest, Some(CommandDestination::LegacySlash { .. })));
    }

    #[test]
    fn dispatch_key_event_returns_none_for_unregistered_key() {
        let registry = crate::app::features::FeatureRegistry::new();
        assert_eq!(
            CommandDispatch::dispatch_key_event("Ctrl+Z", &registry),
            None
        );
    }

    #[test]
    fn dispatch_key_event_returns_some_for_registered_keymap() {
        let mut registry = crate::app::features::FeatureRegistry::new();
        registry.register_keymap("Ctrl+Shift+M".to_string(), "plugin_manager", "toggle");

        let dest = CommandDispatch::dispatch_key_event("Ctrl+Shift+M", &registry);
        assert_eq!(
            dest,
            Some(CommandDestination::Feature {
                feature_id: "plugin_manager",
                action: "toggle".to_string(),
            })
        );
    }

    #[test]
    fn dispatch_key_event_returns_none_for_empty_key() {
        let registry = crate::app::features::FeatureRegistry::new();
        assert_eq!(CommandDispatch::dispatch_key_event("", &registry), None);
    }

    #[test]
    fn dispatch_key_event_multiple_keymaps() {
        let mut registry = crate::app::features::FeatureRegistry::new();
        registry.register_keymap("Ctrl+S".to_string(), "save_feature", "save");
        registry.register_keymap("Ctrl+W".to_string(), "worker_feature", "toggle");

        let dest_s = CommandDispatch::dispatch_key_event("Ctrl+S", &registry);
        assert_eq!(
            dest_s,
            Some(CommandDestination::Feature {
                feature_id: "save_feature",
                action: "save".to_string(),
            })
        );

        let dest_w = CommandDispatch::dispatch_key_event("Ctrl+W", &registry);
        assert_eq!(
            dest_w,
            Some(CommandDestination::Feature {
                feature_id: "worker_feature",
                action: "toggle".to_string(),
            })
        );
    }

    #[test]
    fn dispatch_service_event_returns_none_for_any_event() {
        let registry = crate::app::features::FeatureRegistry::new();
        assert_eq!(
            CommandDispatch::dispatch_service_event("tool_completed", &registry),
            None
        );
        assert_eq!(
            CommandDispatch::dispatch_service_event("stream_done", &registry),
            None
        );
        assert_eq!(CommandDispatch::dispatch_service_event("", &registry), None);
    }

    #[test]
    fn all_slash_commands_are_routable() {
        let registry = crate::app::features::FeatureRegistry::new();

        for name in CommandDispatch::all_known_slash_commands() {
            let invocation = CommandInvocation {
                name: name.to_string(),
                args: vec![],
                source: CommandSource::SlashCommand,
            };
            let dest = CommandDispatch::route_destination(&invocation, &registry);
            assert!(
                dest.is_some(),
                "slash command {name} should be routable but got None"
            );
        }
    }

    #[test]
    fn all_slash_commands_count() {
        let all: Vec<_> = CommandDispatch::all_known_slash_commands().collect();
        assert_eq!(
            all.len(),
            57,
            "expected 57 slash commands, got {}",
            all.len()
        );
    }

    #[test]
    fn all_slash_commands_route_to_valid_destination() {
        let mut registry = crate::app::features::FeatureRegistry::new();
        registry.register_command("/plugin", "plugin_manager");
        registry.register_command("/help", "help_feature");

        let feature_count = CommandDispatch::all_known_slash_commands()
            .filter(|name| {
                let cmd = CommandInvocation {
                    name: name.to_string(),
                    args: vec![],
                    source: CommandSource::SlashCommand,
                };
                matches!(
                    CommandDispatch::route_destination(&cmd, &registry),
                    Some(CommandDestination::Feature { .. })
                )
            })
            .count();

        assert!(
            feature_count >= 2,
            "expected at least 2 feature-routed commands, got {feature_count}"
        );
    }

    #[test]
    fn destination_to_routing_feature() {
        let dest = CommandDestination::Feature {
            feature_id: "plugin_manager",
            action: "/plugin".to_string(),
        };
        assert_eq!(
            CommandDispatch::destination_to_routing(&dest),
            RoutingResult::RoutedToFeature("plugin_manager")
        );
    }

    #[test]
    fn destination_to_routing_builtin() {
        let dest = CommandDestination::BuiltIn { name: "quit" };
        assert_eq!(
            CommandDispatch::destination_to_routing(&dest),
            RoutingResult::BuiltIn
        );
    }

    #[test]
    fn destination_to_routing_legacy_slash() {
        let dest = CommandDestination::LegacySlash { names: &["/help"] };
        assert_eq!(
            CommandDispatch::destination_to_routing(&dest),
            RoutingResult::BuiltIn
        );
    }

    #[test]
    fn find_slash_group_returns_full_alias_group() {
        let group = CommandDispatch::find_slash_group("quit");
        assert_eq!(group, Some(&["/quit", "/exit", "/q"][..]));
    }

    #[test]
    fn find_slash_group_returns_for_alias() {
        let group = CommandDispatch::find_slash_group("exit");
        assert_eq!(group, Some(&["/quit", "/exit", "/q"][..]));
    }

    #[test]
    fn find_slash_group_returns_none_for_unknown() {
        assert_eq!(CommandDispatch::find_slash_group("nonexistent"), None);
    }

    #[test]
    fn slash_command_agent_is_routable() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "/agent".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        assert!(CommandDispatch::route_destination(&cmd, &registry).is_some());
    }

    #[test]
    fn slash_command_team_is_routable() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd = CommandInvocation {
            name: "/team".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        assert!(CommandDispatch::route_destination(&cmd, &registry).is_some());
    }

    #[test]
    fn slash_command_yolo_and_auto_are_builtin() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd_yolo = CommandInvocation {
            name: "/yolo".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let cmd_auto = CommandInvocation {
            name: "/auto".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let dest_yolo = CommandDispatch::route_destination(&cmd_yolo, &registry);
        let dest_auto = CommandDispatch::route_destination(&cmd_auto, &registry);
        assert!(dest_yolo.is_some());
        assert!(dest_auto.is_some());
        assert!(matches!(
            dest_yolo,
            Some(CommandDestination::BuiltIn { .. })
        ));
        assert!(matches!(
            dest_auto,
            Some(CommandDestination::BuiltIn { .. })
        ));
    }

    #[test]
    fn slash_command_feedback_and_bug_are_same_group() {
        let registry = crate::app::features::FeatureRegistry::new();
        let cmd_fb = CommandInvocation {
            name: "/feedback".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let cmd_bug = CommandInvocation {
            name: "/bug".to_string(),
            args: vec![],
            source: CommandSource::SlashCommand,
        };
        let dest_fb = CommandDispatch::route_destination(&cmd_fb, &registry);
        let dest_bug = CommandDispatch::route_destination(&cmd_bug, &registry);
        assert!(dest_fb.is_some());
        assert!(dest_bug.is_some());
        if let (
            Some(CommandDestination::LegacySlash { names: names_fb }),
            Some(CommandDestination::LegacySlash { names: names_bug }),
        ) = (dest_fb, dest_bug)
        {
            assert_eq!(names_fb, names_bug);
        }
    }
}
