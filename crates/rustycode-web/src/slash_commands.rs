//! Slash command handlers for the web version
//!
//! Provides implementations of the main slash commands available in TUI.

use crate::skills::WebSkillManager;
use serde::{Deserialize, Serialize};

/// Result of executing a slash command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    pub panel_update: Option<PanelUpdate>,
    pub refresh_skills: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelUpdate {
    pub panel_type: PanelType,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PanelType {
    Conversation,
    Skills,
    Marketplace,
    Memory,
    Stats,
}

/// Parse and execute a slash command
pub fn execute_command(input: &str, skill_manager: &mut WebSkillManager) -> CommandResult {
    let input = input.trim();

    if !input.starts_with('/') {
        return CommandResult {
            success: false,
            message: "Not a slash command".to_string(),
            panel_update: None,
            refresh_skills: false,
        };
    }

    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts.first().map(|s| &s[1..]).unwrap_or("");
    let args = &parts[1..];

    match cmd {
        "help" => handle_help(),
        "stats" => handle_stats(args),
        "skills" => handle_skills_browser(skill_manager),
        "skill" => handle_skill_command(args, skill_manager),
        "memory" => handle_memory_command(args),
        "marketplace" | "market" => handle_marketplace_command(args),
        "theme" => handle_theme_command(args),
        "compact" => handle_compact_command(args),
        "save" => handle_save_command(args),
        "load" => handle_load_command(args),
        "mcp" => handle_mcp_command(args),
        _ => CommandResult {
            success: false,
            message: format!(
                "Unknown command: /{}. Type /help for available commands.",
                cmd
            ),
            panel_update: None,
            refresh_skills: false,
        },
    }
}

fn handle_help() -> CommandResult {
    let help_text = r#"╶─ Slash Commands ─╴

/help              Show this help message
/stats             Show session statistics
/skills            Open skill browser
/skill <cmd>       Skill management
/memory            Memory operations
/marketplace       Browse marketplace
/theme             Theme settings
/compact           Context compaction
/save              Save conversation
/load              Load conversation
/mcp               MCP server management

Type /<command> -? for detailed help on each command.
"#;
    CommandResult {
        success: true,
        message: help_text.to_string(),
        panel_update: None,
        refresh_skills: false,
    }
}

fn handle_stats(_args: &[&str]) -> CommandResult {
    CommandResult {
        success: true,
        message: "╶─ Stats Panel ─╴\n\nUse the right panel to view stats.".to_string(),
        panel_update: Some(PanelUpdate {
            panel_type: PanelType::Stats,
            content: "Stats panel requires tool-server for token counting".to_string(),
        }),
        refresh_skills: false,
    }
}

fn handle_skills_browser(skill_manager: &WebSkillManager) -> CommandResult {
    CommandResult {
        success: true,
        message: "Opening skill browser...".to_string(),
        panel_update: Some(PanelUpdate {
            panel_type: PanelType::Skills,
            content: skill_manager.get_skills_for_panel(),
        }),
        refresh_skills: false,
    }
}

fn handle_skill_command(args: &[&str], skill_manager: &mut WebSkillManager) -> CommandResult {
    if args.is_empty() {
        return CommandResult {
            success: false,
            message: "Usage: /skill <list|activate|deactivate|run|info> <name>".to_string(),
            panel_update: None,
            refresh_skills: false,
        };
    }

    let subcmd = args[0];
    match subcmd {
        "list" => CommandResult {
            success: true,
            message: skill_manager.list_skills(),
            panel_update: None,
            refresh_skills: false,
        },
        "activate" => {
            if args.len() < 2 {
                return CommandResult {
                    success: false,
                    message: "Usage: /skill activate <name>".to_string(),
                    panel_update: None,
                    refresh_skills: false,
                };
            }
            match skill_manager.activate_skill(args[1]) {
                Ok(msg) => CommandResult {
                    success: true,
                    message: msg,
                    panel_update: Some(PanelUpdate {
                        panel_type: PanelType::Skills,
                        content: skill_manager.get_skills_for_panel(),
                    }),
                    refresh_skills: true,
                },
                Err(e) => CommandResult {
                    success: false,
                    message: e,
                    panel_update: None,
                    refresh_skills: false,
                },
            }
        }
        "deactivate" => {
            if args.len() < 2 {
                return CommandResult {
                    success: false,
                    message: "Usage: /skill deactivate <name>".to_string(),
                    panel_update: None,
                    refresh_skills: false,
                };
            }
            match skill_manager.deactivate_skill(args[1]) {
                Ok(msg) => CommandResult {
                    success: true,
                    message: msg,
                    panel_update: Some(PanelUpdate {
                        panel_type: PanelType::Skills,
                        content: skill_manager.get_skills_for_panel(),
                    }),
                    refresh_skills: true,
                },
                Err(e) => CommandResult {
                    success: false,
                    message: e,
                    panel_update: None,
                    refresh_skills: false,
                },
            }
        }
        "run" => {
            if args.len() < 2 {
                return CommandResult {
                    success: false,
                    message: "Usage: /skill run <name>".to_string(),
                    panel_update: None,
                    refresh_skills: false,
                };
            }
            match skill_manager.run_skill(args[1]) {
                Ok(msg) => CommandResult {
                    success: true,
                    message: msg,
                    panel_update: None,
                    refresh_skills: true,
                },
                Err(e) => CommandResult {
                    success: false,
                    message: e,
                    panel_update: None,
                    refresh_skills: false,
                },
            }
        }
        "info" => {
            if args.len() < 2 {
                return CommandResult {
                    success: false,
                    message: "Usage: /skill info <name>".to_string(),
                    panel_update: None,
                    refresh_skills: false,
                };
            }
            let skill = skill_manager.skills.iter().find(|s| s.name == args[1]);
            match skill {
                Some(s) => CommandResult {
                    success: true,
                    message: format!(
                        "╶─ {} ─╴\n\n{}\n\nCategory: {:?}\nStatus: {:?}\nRuns: {}",
                        s.name, s.description, s.category, s.status, s.run_count
                    ),
                    panel_update: None,
                    refresh_skills: false,
                },
                None => CommandResult {
                    success: false,
                    message: format!("Skill '{}' not found", args[1]),
                    panel_update: None,
                    refresh_skills: false,
                },
            }
        }
        _ => CommandResult {
            success: false,
            message: format!(
                "Unknown skill subcommand: {}. Use /skill list to see available commands.",
                subcmd
            ),
            panel_update: None,
            refresh_skills: false,
        },
    }
}

fn handle_memory_command(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult {
            success: true,
            message: "╶─ Memory Commands ─╴\n\n\
/memory list              Show all memories\n\
/memory add <text>        Add a memory\n\
/memory clear             Clear all memories\n\
/memory search <query>    Search memories"
                .to_string(),
            panel_update: None,
            refresh_skills: false,
        };
    }

    let subcmd = args[0];
    match subcmd {
        "list" => CommandResult {
            success: true,
            message: "No memories stored yet.".to_string(),
            panel_update: Some(PanelUpdate {
                panel_type: PanelType::Memory,
                content: "[]".to_string(),
            }),
            refresh_skills: false,
        },
        "add" => CommandResult {
            success: true,
            message: "Memory added (requires IndexedDB persistence).".to_string(),
            panel_update: None,
            refresh_skills: false,
        },
        "clear" => CommandResult {
            success: true,
            message: "All memories cleared.".to_string(),
            panel_update: None,
            refresh_skills: false,
        },
        _ => CommandResult {
            success: false,
            message: format!("Unknown memory command: {}", subcmd),
            panel_update: None,
            refresh_skills: false,
        },
    }
}

fn handle_marketplace_command(_args: &[&str]) -> CommandResult {
    let marketplace_content = r#"╶─ Marketplace ─╴

Browse and install community skills and tools.

[Coming Soon]
- Skill marketplace integration
- Tool extensions
- MCP server plugins

Use /skill install <name> when available.
"#;
    CommandResult {
        success: true,
        message: "Opening marketplace browser...".to_string(),
        panel_update: Some(PanelUpdate {
            panel_type: PanelType::Marketplace,
            content: marketplace_content.to_string(),
        }),
        refresh_skills: false,
    }
}

fn handle_theme_command(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult {
            success: true,
            message: "╶─ Available Themes ─╴\n\n\
nord          Arctic, north-blu (default)\n\
monokai      Monokai\n\
gruvbox      Gruvbox\n\
dracula      Dracula\n\
one          One Dark\n\n\
Usage: /theme <name>"
                .to_string(),
            panel_update: None,
            refresh_skills: false,
        };
    }

    CommandResult {
        success: true,
        message: format!("Theme '{}' selected. Reload to apply.", args[0]),
        panel_update: None,
        refresh_skills: false,
    }
}

fn handle_compact_command(_args: &[&str]) -> CommandResult {
    CommandResult {
        success: true,
        message:
            "Context compaction helps free up tokens by summarizing old conversation history.\n\n\
This feature requires tool-server for execution."
                .to_string(),
        panel_update: None,
        refresh_skills: false,
    }
}

fn handle_save_command(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult {
            success: false,
            message: "Usage: /save <filename>".to_string(),
            panel_update: None,
            refresh_skills: false,
        };
    }

    CommandResult {
        success: true,
        message: format!("Saving to '{}'. Requires IndexedDB persistence.", args[0]),
        panel_update: None,
        refresh_skills: false,
    }
}

fn handle_load_command(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult {
            success: false,
            message: "Usage: /load <filename>".to_string(),
            panel_update: None,
            refresh_skills: false,
        };
    }

    CommandResult {
        success: true,
        message: format!("Loading '{}'. Requires IndexedDB persistence.", args[0]),
        panel_update: None,
        refresh_skills: false,
    }
}

fn handle_mcp_command(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult {
            success: true,
            message: "╶─ MCP Commands ─╴\n\n\
/mcp list                  List connected MCP servers\n\
/mcp connect <url>         Connect to MCP server\n\
/mcp disconnect <name>    Disconnect an MCP server\n\n\
MCP (Model Context Protocol) enables extended tool capabilities."
                .to_string(),
            panel_update: None,
            refresh_skills: false,
        };
    }

    CommandResult {
        success: false,
        message: "MCP requires WebSocket connection. Use tool-server for MCP.".to_string(),
        panel_update: None,
        refresh_skills: false,
    }
}
