//! Info and utility commands: help, marketplace, skill, mcp, hook, theme

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;

fn plugin_summary(manager: &crate::plugin::PluginManager) -> String {
    let mut plugins = manager.plugins();
    plugins.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));

    if plugins.is_empty() {
        return "No plugins found.\n\nUse /plugin reload after adding a plugin manifest to ~/.rustycode/plugins/."
            .to_string();
    }

    let mut lines = vec!["Plugins:".to_string()];
    for plugin in plugins {
        let status = if plugin.enabled {
            "enabled"
        } else {
            "disabled"
        };
        let command_count = plugin.manifest.slash_commands.len();
        lines.push(format!(
            "  • {} v{} ({}, {} command{})",
            plugin.manifest.name,
            plugin.manifest.version,
            status,
            command_count,
            if command_count == 1 { "" } else { "s" }
        ));
    }

    lines.push(String::new());
    lines.push("Use /plugin info <name> for details.".to_string());
    lines.join("\n")
}

fn plugin_detail(manager: &crate::plugin::PluginManager, name: &str) -> String {
    let Some(plugin) = manager.plugin(name) else {
        return format!(
            "Plugin '{}' not found. Use /plugin list to see available plugins.",
            name
        );
    };

    let mut lines = vec![
        format!("Plugin: {}", plugin.manifest.name),
        format!("Version: {}", plugin.manifest.version),
        format!("Author: {}", plugin.manifest.author),
        format!(
            "Status: {}",
            if plugin.enabled {
                "enabled"
            } else {
                "disabled"
            }
        ),
        format!("Type: {:?}", plugin.manifest.plugin_type),
        format!("Description: {}", plugin.manifest.description),
    ];

    if !plugin.manifest.permissions.is_empty() {
        lines.push(format!(
            "Permissions: {}",
            plugin.manifest.permissions.join(", ")
        ));
    }

    if !plugin.manifest.slash_commands.is_empty() {
        lines.push("Commands:".to_string());
        for command in &plugin.manifest.slash_commands {
            lines.push(format!("  • /{} - {}", command.name, command.description));
        }
    }

    lines.join("\n")
}

fn plugin_usage() -> String {
    "Usage: /plugin [open|list|reload|info <name>|install <source>|update [name|all]|uninstall <name>|enable <name>|disable <name>]\n\
     Examples:\n\
     • /plugin open\n\
     • /plugin list\n\
     • /plugin install https://github.com/me/my-plugin.git\n\
     • /plugin update my-plugin\n\
     • /plugin info my-plugin\n\
     • /plugin enable my-plugin\n\
     • /plugin disable my-plugin"
        .to_string()
}

fn plugin_sync_result(
    manager: &std::sync::Arc<std::sync::RwLock<crate::plugin::PluginManager>>,
    action: impl FnOnce(&mut crate::plugin::PluginManager) -> anyhow::Result<String>,
) -> Result<CommandEffect> {
    let mut guard = manager.write().unwrap_or_else(|e| e.into_inner());
    action(&mut guard).map(CommandEffect::SystemMessage)
}

/// Handle /plugin command
pub fn handle_plugin_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() == 1 || parts[1] == "list" {
        return plugin_sync_result(ctx.plugin_manager, |manager| {
            let _ = manager.reload_from_disk();
            Ok(plugin_summary(manager))
        });
    }

    match parts[1] {
        "open" => Ok(CommandEffect::ShowPluginManager),
        "reload" => plugin_sync_result(ctx.plugin_manager, |manager| {
            let count = manager.reload_from_disk()?;
            Ok(format!("✓ Reloaded plugins ({})", count))
        }),
        "info" => {
            let Some(name) = parts.get(2) else {
                return Ok(CommandEffect::SystemMessage(plugin_usage()));
            };
            plugin_sync_result(ctx.plugin_manager, |manager| {
                let _ = manager.reload_from_disk();
                Ok(plugin_detail(manager, name))
            })
        }
        "install" => {
            let Some(source) = parts.get(2) else {
                return Ok(CommandEffect::SystemMessage(plugin_usage()));
            };

            let plugin_manager = ctx.plugin_manager.clone();
            let command_tx = ctx.command_tx;
            let source = (*source).to_string();
            let source_display = source.clone();

            std::thread::spawn(move || {
                let result = {
                    let mut manager = plugin_manager.write().unwrap_or_else(|e| e.into_inner());
                    manager.install_plugin(&source).map_err(|e| e.to_string())
                };

                let message = match result {
                    Ok(name) => format!("✓ Installed plugin '{}'", name),
                    Err(e) => format!("❌ Plugin install failed: {}", e),
                };
                let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(message));
            });

            Ok(CommandEffect::AsyncStarted(format!(
                "📦 Installing plugin from {}...",
                source_display
            )))
        }
        "update" => {
            let plugin_manager = ctx.plugin_manager.clone();
            let command_tx = ctx.command_tx;
            let target = parts.get(2).copied().unwrap_or("all").to_string();
            let target_display = target.clone();

            std::thread::spawn(move || {
                let result = {
                    let mut manager = plugin_manager.write().unwrap_or_else(|e| e.into_inner());
                    if target == "all" {
                        manager
                            .update_all_plugins()
                            .map(|updated| {
                                if updated.is_empty() {
                                    "✓ All plugins are already up to date".to_string()
                                } else {
                                    format!("✓ Updated plugins: {}", updated.join(", "))
                                }
                            })
                            .map_err(|e| e.to_string())
                    } else {
                        manager
                            .update_plugin(&target)
                            .map(|_| format!("✓ Updated plugin '{}'", target))
                            .map_err(|e| e.to_string())
                    }
                };

                let message = match result {
                    Ok(message) => message,
                    Err(e) => format!("❌ Plugin update failed: {}", e),
                };
                let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(message));
            });

            Ok(CommandEffect::AsyncStarted(format!(
                "⬆ Updating plugin{}...",
                if target_display == "all" { "s" } else { "" }
            )))
        }
        "uninstall" => {
            let Some(name) = parts.get(2) else {
                return Ok(CommandEffect::SystemMessage(plugin_usage()));
            };

            let plugin_manager = ctx.plugin_manager.clone();
            let command_tx = ctx.command_tx;
            let name = (*name).to_string();
            let name_display = name.clone();

            std::thread::spawn(move || {
                let result: Result<String, String> = (|| {
                    let mut manager = plugin_manager.write().unwrap_or_else(|e| e.into_inner());
                    let _ = manager.reload_from_disk();
                    manager.uninstall_plugin(&name).map_err(|e| e.to_string())?;
                    Ok(format!("✓ Uninstalled plugin '{}'", name))
                })();

                let message = match result {
                    Ok(message) => message,
                    Err(e) => format!("❌ Plugin uninstall failed: {}", e),
                };
                let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(message));
            });

            Ok(CommandEffect::AsyncStarted(format!(
                "🗑️ Removing plugin '{}'...",
                name_display
            )))
        }
        "enable" => {
            let Some(name) = parts.get(2) else {
                return Ok(CommandEffect::SystemMessage(plugin_usage()));
            };
            plugin_sync_result(ctx.plugin_manager, |manager| {
                let _ = manager.reload_from_disk();
                manager
                    .enable_plugin(name)
                    .map(|_| format!("✓ Enabled plugin '{}'", name))
                    .map_err(|e| anyhow::anyhow!("Plugin error: {}", e))
            })
        }
        "disable" => {
            let Some(name) = parts.get(2) else {
                return Ok(CommandEffect::SystemMessage(plugin_usage()));
            };
            plugin_sync_result(ctx.plugin_manager, |manager| {
                let _ = manager.reload_from_disk();
                manager
                    .disable_plugin(name)
                    .map(|_| format!("✓ Disabled plugin '{}'", name))
                    .map_err(|e| anyhow::anyhow!("Plugin error: {}", e))
            })
        }
        _ => Ok(CommandEffect::SystemMessage(plugin_usage())),
    }
}

/// Handle /help command
pub fn handle_help_command(_parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    Ok(CommandEffect::ShowHelp)
}

/// Handle /marketplace command
pub fn handle_marketplace_command(
    parts: &[&str],
    ctx: CommandContext<'_>,
) -> Result<CommandEffect> {
    let input = parts.join(" ");
    let input_clone = input.clone();

    let tx = ctx.command_tx;
    std::thread::spawn(move || {
        let result = rustycode_shared_runtime::block_on_shared(
            crate::slash_commands::marketplace::handle_marketplace_command(&input_clone),
        );

        match result {
            Ok(Some(output)) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Ok(None) => {
                // Command succeeded but produced no output
            }
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "Marketplace command failed: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::None)
}

/// Handle /skill and /skills commands
pub fn handle_skill_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
    let input_for_skill = if args.is_empty() {
        "/skill".to_string()
    } else {
        format!("/skill {}", args.join(" "))
    };

    let result =
        crate::slash_commands::skill::handle_skill_command(&input_for_skill, ctx.skill_manager);

    match result {
        Ok(Some(output)) => Ok(CommandEffect::SystemMessage(output)),
        Ok(None) => Ok(CommandEffect::None),
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "❌ Skill error: {}",
            e
        ))),
    }
}

/// Handle /mcp command
pub fn handle_mcp_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    // Check if this is a subcommand or just opening MCP mode
    if parts.len() < 2 || parts[1] == "open" {
        return Ok(CommandEffect::SystemMessage(
            "MCP Mode: Press Esc to close. Type 'list' for servers, 'status' for connection info."
                .to_string(),
        ));
    }

    let input = parts.join(" ");
    let input_clone = input.clone();
    let tx = ctx.command_tx;

    // Spawn thread with its own runtime for MCP commands
    std::thread::spawn(move || {
        let result = rustycode_shared_runtime::block_on_shared(
            crate::slash_commands::mcp::handle_mcp_command(&input_clone),
        );
        match result {
            Ok(Some(output)) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Ok(None) => {}
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "MCP error: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::AsyncStarted(
        "🔌 Loading MCP servers...".to_string(),
    ))
}

/// Handle /lsp command
pub fn handle_lsp_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        let help = "LSP (Language Server Protocol) - Code Intelligence Servers\n\
                    \n\
                    Commands:\n\
                    \n\
                    * /lsp status   - Show discovered and available LSP servers\n\
                    * /lsp start    - (automatic) LSP servers start when needed\n\
                    * /lsp stop     - (automatic) LSP servers are managed by RustyCode\n\
                    * /lsp help     - Show this help text";
        return Ok(CommandEffect::SystemMessage(help.to_string()));
    }

    let input = parts.join(" ");
    let input_clone = input.clone();
    let tx = ctx.command_tx;

    std::thread::spawn(move || {
        let result = rustycode_shared_runtime::block_on_shared(
            crate::slash_commands::lsp::handle_lsp_command(&input_clone),
        );
        match result {
            Ok(Some(output)) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Ok(None) => {}
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "LSP error: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::AsyncStarted(
        "📡 Checking LSP servers...".to_string(),
    ))
}

/// Handle /hook command
pub fn handle_hook_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let input = parts.join(" ");
    let result = crate::slash_commands::hook::handle_hook_command(&input);

    match result {
        Ok(Some(output)) => Ok(CommandEffect::SystemMessage(output)),
        Ok(None) => Ok(CommandEffect::None),
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "❌ Hook error: {}",
            e
        ))),
    }
}

/// Handle /theme command
pub fn handle_theme_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let args: Vec<&str> = parts[1..].to_vec();
    let result = crate::slash_commands::handle_theme_command(&args, ctx.theme_colors);

    let effect = match result {
        crate::slash_commands::ThemeCommandResult::Success(msg) => {
            CommandEffect::SystemMessage(format!("✓ {}", msg))
        }
        crate::slash_commands::ThemeCommandResult::List(msg) => CommandEffect::SystemMessage(msg),
        crate::slash_commands::ThemeCommandResult::Error(err) => {
            CommandEffect::SystemMessage(format!("❌ {}", err))
        }
    };

    Ok(effect)
}

/// Handle /stats command — display session statistics
pub fn handle_stats_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    use crate::ui::message::MessageRole;

    let turn_count = ctx
        .messages
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User))
        .count();

    // Count tools from messages
    let mut tool_count = 0usize;
    let mut tool_failures = 0usize;
    for msg in ctx.messages.iter() {
        if let Some(tools) = &msg.tool_executions {
            for tool in tools {
                tool_count += 1;
                if matches!(tool.status, crate::ui::message::ToolStatus::Failed) {
                    tool_failures += 1;
                }
            }
        }
    }

    let total_tokens = ctx.session_input_tokens + ctx.session_output_tokens;
    let context_pct = if ctx.context_monitor.max_tokens > 0 {
        ((total_tokens as f64 / ctx.context_monitor.max_tokens as f64) * 100.0).round() as usize
    } else {
        0
    };

    let stats = crate::slash_commands::stats::SessionStats {
        input_tokens: ctx.session_input_tokens,
        output_tokens: ctx.session_output_tokens,
        cost_usd: ctx.session_cost_usd,
        turn_count,
        tool_count,
        tool_failures,
        context_percentage: context_pct,
        context_tokens: total_tokens,
        context_limit: ctx.context_monitor.max_tokens,
        model: ctx.current_model.clone(),
        duration_secs: ctx.session_start.elapsed().as_secs(),
    };

    let result = crate::slash_commands::stats::handle_stats_command(&stats);
    Ok(CommandEffect::SystemMessage(result.display))
}

/// Handle /track command — show unified workspace progress
pub fn handle_track_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let cwd = ctx.cwd.to_path_buf();
    let tasks = ctx.workspace_tasks.clone();
    let agents = ctx.agent_manager.agents();
    let tx = ctx.command_tx;
    let detail_mode = matches!(parts.get(1), Some(&"full" | &"detail" | &"details"));

    std::thread::spawn(move || {
        let output = if detail_mode {
            crate::workspace::workspace_progress::render_workspace_progress(&cwd, &tasks, &agents)
        } else {
            crate::workspace::workspace_progress::render_workspace_progress_compact(
                &cwd, &tasks, &agents,
            )
        };
        let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
    });

    Ok(CommandEffect::AsyncStarted(if detail_mode {
        "📊 Collecting detailed workspace progress...".to_string()
    } else {
        "📊 Collecting compact workspace progress...".to_string()
    }))
}

/// Handle /cost command — display detailed cost breakdown
pub fn handle_cost_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let cost = ctx.session_cost_usd;
    let input = ctx.session_input_tokens;
    let output = ctx.session_output_tokens;

    let mut lines = Vec::new();
    lines.push(format!(
        "Session Cost: {}",
        if cost < 0.01 {
            format!("${:.4}", cost)
        } else {
            format!("${:.2}", cost)
        }
    ));
    lines.push(format!("Tokens: {} in / {} out", input, output));
    lines.push(format!("Model: {}", ctx.current_model));

    if cost > 0.0 && (input + output) > 0 {
        let cost_per_1k = (cost / (input + output) as f64) * 1000.0;
        lines.push(format!("Avg cost per 1K tokens: ${:.4}", cost_per_1k));
    }

    Ok(CommandEffect::SystemMessage(lines.join("\n")))
}

/// Handle /checkpoint command — display checkpoint status
pub fn handle_checkpoint_command(
    _parts: &[&str],
    _ctx: CommandContext<'_>,
) -> Result<CommandEffect> {
    // Checkpoint integration is available via the execution middleware.
    // For now, show guidance on checkpoint usage.
    let msg = "Checkpoints are auto-created before destructive operations (edit_file, write_file, bash).\n\
               Use /undo to revert the last file change.\n\
               Use /diff to see what changed recently."
        .to_string();
    Ok(CommandEffect::SystemMessage(msg))
}

/// Handle /feedback command — open browser to create a pre-filled GitHub issue
pub fn handle_feedback_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let user_text = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        String::new()
    };

    let title = if user_text.is_empty() {
        "Feedback".to_string()
    } else {
        user_text
    };

    let body = [
        "### What would you like to share?".to_string(),
        String::new(),
        String::new(),
        String::new(),
        "---".to_string(),
        format!("*Version: {}*", env!("CARGO_PKG_VERSION")),
        format!("*OS: {}*", std::env::consts::OS),
    ]
    .join("\n");

    let url = format!(
        "https://github.com/rustycode-ai/rustycode/issues/new?title={}&body={}",
        urlencoding::encode(&title),
        urlencoding::encode(&body),
    );

    let opened = rustycode_auth::open_url(&url).is_ok();

    let msg = if opened {
        format!(
            "Opening browser to create a feedback issue...\n\
             If the browser didn't open, visit:\n  {}",
            url
        )
    } else {
        format!("Could not open browser. Create an issue at:\n  {}", url)
    };

    Ok(CommandEffect::SystemMessage(msg))
}

/// Handle /skillify command — create a new skill from conversation context
pub fn handle_skillify_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let input = parts.join(" ");
    let result = crate::slash_commands::skillify::handle_skillify_command(&input, ctx.cwd);
    match result {
        Ok(Some(output)) => Ok(CommandEffect::SystemMessage(output)),
        Ok(None) => Ok(CommandEffect::None),
        Err(e) => Ok(CommandEffect::SystemMessage(format!("❌ {}", e))),
    }
}
