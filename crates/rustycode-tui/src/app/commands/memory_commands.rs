//! Memory management commands

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;

/// Handle /memory command
pub fn handle_memory_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        return Ok(CommandEffect::SystemMessage(
            "Memory commands:\n\
             /memory save <key> <value>    - Save a fact to memory\n\
             /memory recall <key>          - Retrieve from memory\n\
             /memory search <query>        - Search memories\n\
             /memory list                 - List all memories\n\
             /memory delete <key>         - Delete a memory\n\
             /memory clear                - Clear all memories\n\
             /memory inject [on|off]      - Toggle auto-injection\n\
             /memory inject threshold <n> - Set relevance threshold\n\
             /memory inject max <n>       - Set max memories to inject\n\
             /memory inject show <query>  - Preview what would be injected"
                .to_string(),
        ));
    }

    let subcommand = parts[1].to_string();
    let parts_clone: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
    let cwd = ctx.cwd.to_path_buf();

    // Check if this is an inject command (needs mutable access to config)
    if subcommand == "inject" {
        let inject_args = if parts_clone.len() > 1 {
            Some(parts_clone[1..].join(" "))
        } else {
            None
        };

        // Handle inject command directly with mutable access to config
        let result = crate::slash_commands::memory::handle_inject_command(
            &cwd,
            inject_args,
            ctx.memory_injection_config,
        );

        match result {
            Ok(msg) => Ok(CommandEffect::SystemMessage(msg)),
            Err(e) => Ok(CommandEffect::SystemMessage(format!(
                "❌ Memory injection error: {}",
                e
            ))),
        }
    } else {
        // Spawn thread with its own runtime for memory commands
        let tx = ctx.command_tx;
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create runtime for memory command: {}", e);
                    return;
                }
            };
            let result = match subcommand.as_str() {
                "save" => {
                    if parts_clone.len() < 3 {
                        Ok("❌ Usage: /memory save <key> <value>".to_string())
                    } else {
                        let key = parts_clone[1].clone();
                        let value = parts_clone[2..].join(" ");
                        crate::slash_commands::memory::into_anyhow_result(rt.block_on(
                            crate::slash_commands::memory::handle_save_command(&cwd, key, value),
                        ))
                    }
                }
                "recall" => {
                    if parts_clone.len() < 2 {
                        Ok("❌ Usage: /memory recall <key>".to_string())
                    } else {
                        let key = parts_clone[1].clone();
                        crate::slash_commands::memory::into_anyhow_result(rt.block_on(
                            crate::slash_commands::memory::handle_recall_command(&cwd, key),
                        ))
                    }
                }
                "search" => {
                    if parts_clone.len() < 2 {
                        Ok("❌ Usage: /memory search <query>".to_string())
                    } else {
                        let query = parts_clone[1..].join(" ");
                        rt.block_on(crate::slash_commands::memory::handle_search_command(
                            &cwd, query,
                        ))
                    }
                }
                "list" => rt.block_on(crate::slash_commands::memory::handle_list_command(&cwd)),
                "delete" => {
                    if parts_clone.len() < 2 {
                        Ok("❌ Usage: /memory delete <key>".to_string())
                    } else {
                        let key = parts_clone[1].clone();
                        crate::slash_commands::memory::into_anyhow_result(rt.block_on(
                            crate::slash_commands::memory::handle_delete_command(&cwd, key),
                        ))
                    }
                }
                "clear" => crate::slash_commands::memory::into_anyhow_result(
                    rt.block_on(crate::slash_commands::memory::handle_clear_command(&cwd)),
                ),
                _ => Ok(format!(
                    "❌ Unknown memory subcommand: {}\n\
                     Usage: /memory [save|recall|search|list|delete|clear|inject]",
                    subcommand
                )),
            };

            match result {
                Ok(output) => {
                    let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
                }
                Err(e) => {
                    let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                        "Memory command failed: {}",
                        e
                    )));
                }
            }
        });

        Ok(CommandEffect::AsyncStarted(
            "Processing memory command...".to_string(),
        ))
    }
}
