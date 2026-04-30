//! Task and review commands: task/todo, review, compact, learnings

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;

/// Handle /compact command
pub fn handle_compact_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
    let action =
        crate::slash_commands::handle_compact_command(&args, ctx.messages, ctx.context_monitor)?;

    match action {
        crate::slash_commands::CompactAction::Compact => {
            let strategy = ctx.compaction_config.strategy;
            let old_tokens = ctx.context_monitor.current_tokens;
            match crate::slash_commands::execute_compaction(ctx.messages.clone(), strategy) {
                Ok(compacted) => {
                    let old_count = ctx.messages.len();
                    let new_count = compacted.len();
                    *ctx.messages = compacted;
                    ctx.context_monitor.update(ctx.messages);
                    let new_tokens = ctx.context_monitor.current_tokens;
                    let saved = old_tokens.saturating_sub(new_tokens);
                    *ctx.showing_compaction_preview = false;
                    *ctx.pending_compaction = false;
                    let fmt = |n: usize| -> String {
                        if n >= 1_000 {
                            format!("{:.0}k", n as f64 / 1_000.0)
                        } else {
                            n.to_string()
                        }
                    };
                    Ok(CommandEffect::SystemMessage(format!(
                        "💾 Compacted: {} → {} messages (saved ~{} tokens)",
                        old_count,
                        new_count,
                        fmt(saved)
                    )))
                }
                Err(e) => Ok(CommandEffect::SystemMessage(format!(
                    "⚠ Compaction failed: {}",
                    e
                ))),
            }
        }
        crate::slash_commands::CompactAction::ShowPreview(preview) => {
            *ctx.showing_compaction_preview = true;
            *ctx.pending_compaction = true;
            Ok(CommandEffect::SystemMessage(preview.format()))
        }
        crate::slash_commands::CompactAction::ShowStatus(status) => {
            Ok(CommandEffect::SystemMessage(status))
        }
        crate::slash_commands::CompactAction::SetThreshold(threshold) => {
            ctx.compaction_config.warning_threshold = threshold;
            ctx.context_monitor.warning_threshold = threshold;
            Ok(CommandEffect::SystemMessage(format!(
                "✓ Warning threshold set to {:.1}%",
                threshold * 100.0
            )))
        }
        crate::slash_commands::CompactAction::SetStrategy(strategy) => {
            ctx.compaction_config.strategy = strategy;
            Ok(CommandEffect::SystemMessage(format!(
                "✓ Compaction strategy set to {:?}",
                strategy
            )))
        }
        crate::slash_commands::CompactAction::Error(msg) => {
            Ok(CommandEffect::SystemMessage(format!("⚠ {}", msg)))
        }
    }
}

/// Handle /review command
pub fn handle_review_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let path = parts
        .get(1)
        .map(|s| s.to_string())
        .unwrap_or_else(|| ".".to_string());
    let path_clone = path.clone();

    let tx = ctx.command_tx;
    // Spawn thread with its own runtime for review
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create runtime for review: {}", e);
                return;
            }
        };
        let analyzer = crate::slash_commands::review::CodeReviewAnalyzer::new();
        let result = rt.block_on(analyzer.analyze_path(std::path::Path::new(&path)));

        match result {
            Ok(output) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "Review failed: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::AsyncStarted(format!(
        "Started review of {} in background...",
        path_clone
    )))
}

/// Handle /task and /todo commands
pub fn handle_task_todo_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let input = parts.join(" ");
    let result = crate::task_commands::handle_command(&input, ctx.workspace_tasks);

    match result {
        crate::task_commands::CommandResult::Success(output) => {
            Ok(CommandEffect::SystemMessage(output))
        }
        crate::task_commands::CommandResult::Error(err) => {
            // Show the actual error message to help users understand what went wrong
            let user_msg = format!("⚠️  {}", err);
            Ok(CommandEffect::SystemMessage(user_msg))
        }
        crate::task_commands::CommandResult::Consumed => Ok(CommandEffect::None),
    }
}

/// Handle /learnings command
pub fn handle_learnings_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        // Default to show
        let cwd = ctx.cwd.to_path_buf();
        let tx = ctx.command_tx;
        std::thread::spawn(move || {
            let result = rustycode_core::team::team_learnings::TeamLearnings::load(&cwd)
                .map(|l| l.get_all())
                .unwrap_or_else(|e| format!("Error loading learnings: {}", e));
            let _ = tx.send(crate::app::async_::SlashCommandResult::Success(result));
        });
        return Ok(CommandEffect::None);
    }

    let subcommand = parts[1].to_string();
    let parts_clone: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
    let cwd = ctx.cwd.to_path_buf();
    let tx = ctx.command_tx;

    std::thread::spawn(move || {
        use rustycode_core::team::team_learnings::{LearningCategory, TeamLearnings};

        let result = match subcommand.as_str() {
            "show" => match TeamLearnings::load(&cwd) {
                Ok(l) => l.get_all(),
                Err(e) => format!("Error loading learnings: {}", e),
            },
            "add" => {
                if parts_clone.len() < 4 {
                    "Usage: /learnings add --category <category> <content>".to_string()
                } else {
                    // Parse --category flag
                    let mut category = "what-worked";
                    let mut content_start = 2;
                    for (i, part) in parts_clone.iter().enumerate().skip(2) {
                        if part == "--category" || part == "-c" {
                            if let Some(cat) = parts_clone.get(i + 1) {
                                category = cat;
                                content_start = i + 2;
                                break;
                            }
                        }
                    }
                    let content = parts_clone[content_start..].join(" ");

                    let cat = match category {
                        "user-preference" | "user" => LearningCategory::UserPreference,
                        "codebase-quirk" | "quirk" => LearningCategory::CodebaseQuirk,
                        "what-worked" | "worked" => LearningCategory::WhatWorked,
                        "what-failed" | "failed" => LearningCategory::WhatFailed,
                        _ => LearningCategory::WhatWorked,
                    };

                    match TeamLearnings::load(&cwd) {
                        Ok(mut l) => {
                            l.record(cat, content, None);
                            if let Err(e) = l.save() {
                                format!("Error saving learning: {}", e)
                            } else {
                                "Learning recorded".to_string()
                            }
                        }
                        Err(e) => format!("Error loading learnings: {}", e),
                    }
                }
            }
            "clear" => match TeamLearnings::load(&cwd) {
                Ok(mut l) => {
                    l.clear();
                    if let Err(e) = l.save() {
                        format!("Error clearing learnings: {}", e)
                    } else {
                        "All learnings cleared".to_string()
                    }
                }
                Err(e) => format!("Error loading learnings: {}", e),
            },
            _ => format!(
                "Unknown learnings command: {}. Use: show, add, clear",
                subcommand
            ),
        };

        let _ = tx.send(crate::app::async_::SlashCommandResult::Success(result));
    });

    Ok(CommandEffect::None)
}
