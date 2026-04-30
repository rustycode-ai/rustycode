//! Session lifecycle commands: clear, quit, save, load, workspace, extract, rename, resume, tokens, retry

use super::CommandContext;
use super::CommandEffect;
use crate::ui::message::MessageRole;
use anyhow::Result;

/// Format a token count for display
fn format_tokens(count: usize) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// Handle /extract command
pub fn handle_extract_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let sub_cmd = parts.get(1).copied().unwrap_or("");
    if sub_cmd == "undo" || sub_cmd == "revert" {
        if let Some((old_tasks, old_todos)) = ctx.last_extraction.take() {
            ctx.workspace_tasks.tasks = old_tasks;
            ctx.workspace_tasks.todos = old_todos;
            if let Err(e) = crate::tasks::save_tasks(ctx.workspace_tasks) {
                return Ok(CommandEffect::SystemMessage(format!(
                    "Failed to save reverted tasks: {}",
                    e
                )));
            }

            Ok(CommandEffect::SystemMessage(
                "✅ Successfully reverted the last task extraction.".to_string(),
            ))
        } else {
            Ok(CommandEffect::SystemMessage(
                "⚠️ No recent task extraction to revert.".to_string(),
            ))
        }
    } else {
        Ok(CommandEffect::SystemMessage(
            "Usage: /extract undo".to_string(),
        ))
    }
}

/// Handle /rename command
pub fn handle_rename_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let title = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        "Untitled Session".to_string()
    };

    let serialized: Vec<crate::session::SerializedMessage> = ctx
        .messages
        .iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::User => crate::session::SerializedMessageType::User,
                MessageRole::Assistant => crate::session::SerializedMessageType::AI,
                MessageRole::System => crate::session::SerializedMessageType::System,
            };
            crate::session::SerializedMessage {
                role,
                content: m.content.clone(),
            }
        })
        .collect();

    match crate::session::save_current_session(&title, &serialized) {
        Ok(_) => Ok(CommandEffect::SystemMessage(format!(
            "Session renamed: {}",
            title
        ))),
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "Failed to rename session: {}",
            e
        ))),
    }
}

/// Handle /save command
pub fn handle_save_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let name = parts.get(1).map(|s| s.to_string());

    // Serialize current messages for saving
    let serialized: Vec<crate::session::SerializedMessage> = ctx
        .messages
        .iter()
        .map(|m| crate::session::SerializedMessage {
            role: match m.role {
                MessageRole::User => crate::session::SerializedMessageType::User,
                MessageRole::Assistant => crate::session::SerializedMessageType::AI,
                MessageRole::System => crate::session::SerializedMessageType::System,
            },
            content: m.content.clone(),
        })
        .collect();

    let tx = ctx.command_tx;
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create runtime for save command: {}", e);
                return;
            }
        };
        let result = rt.block_on(crate::slash_commands::save::handle_save_command(
            name,
            &serialized,
        ));

        match result {
            Ok(output) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "Save failed: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::AsyncStarted(
        "Saving session in background...".to_string(),
    ))
}

/// Handle /load command
pub fn handle_load_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let session_id = match parts.get(1) {
        Some(id) => id.to_string(),
        None => {
            // List available sessions
            let sessions = crate::session::load_session_history_list("Current Session", 0);
            if sessions.is_empty() {
                return Ok(CommandEffect::SystemMessage(
                    "No saved sessions found".to_string(),
                ));
            }
            let mut output = String::from("Available sessions:\n");
            for (i, session) in sessions.iter().enumerate() {
                output.push_str(&format!("  {}. {}\n", i + 1, session.title));
            }
            output.push_str("\nUse /load <id> to load a session");
            return Ok(CommandEffect::SystemMessage(output));
        }
    };

    // Load synchronously — it's just file I/O
    match crate::session::load_session(&session_id) {
        Ok((name, serialized_messages, age)) => {
            let msg_count = serialized_messages.len();
            // Convert serialized messages to TUI Message types
            let messages: Vec<crate::ui::message::Message> = serialized_messages
                .into_iter()
                .map(|sm| {
                    let role = match sm.role {
                        crate::session::SerializedMessageType::User => MessageRole::User,
                        crate::session::SerializedMessageType::AI => MessageRole::Assistant,
                        crate::session::SerializedMessageType::System => MessageRole::System,
                        crate::session::SerializedMessageType::Tool => MessageRole::System,
                    };
                    crate::ui::message::Message::new(role, sm.content)
                })
                .collect();

            Ok(CommandEffect::LoadSession {
                name,
                messages,
                summary: format!("Resumed session from {} — {} messages", age, msg_count),
            })
        }
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "Failed to load session '{}': {}",
            session_id, e
        ))),
    }
}

pub fn handle_resume_command(_parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let sessions = crate::session::load_session_history_list("Current Session", 0);

    // Skip synthetic "current" entry (id="current") — it has no backing file.
    // Take the first real saved session instead.
    let real_session = sessions.iter().find(|s| s.id != "current");

    let Some(target) = real_session else {
        return Ok(CommandEffect::SystemMessage(
            "No saved sessions found to resume".to_string(),
        ));
    };

    let session_id = target.id.clone();

    match crate::session::load_session(&session_id) {
        Ok((name, serialized_messages, age)) => {
            let msg_count = serialized_messages.len();
            let messages: Vec<crate::ui::message::Message> = serialized_messages
                .into_iter()
                .map(|sm| {
                    let role = match sm.role {
                        crate::session::SerializedMessageType::User => MessageRole::User,
                        crate::session::SerializedMessageType::AI => MessageRole::Assistant,
                        crate::session::SerializedMessageType::System => MessageRole::System,
                        crate::session::SerializedMessageType::Tool => MessageRole::System,
                    };
                    crate::ui::message::Message::new(role, sm.content)
                })
                .collect();

            Ok(CommandEffect::LoadSession {
                name,
                messages,
                summary: format!("Resumed session from {} — {} messages", age, msg_count),
            })
        }
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "Failed to resume session: {}",
            e
        ))),
    }
}

pub fn handle_tokens_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let input = ctx.session_input_tokens;
    let output = ctx.session_output_tokens;
    let total = input + output;
    let cost = ctx.session_cost_usd;

    let output = format!(
        "Token Usage (session)\n\
         \n\
         Input:   {} ({})\n\
         Output:  {} ({})\n\
         Total:   {} ({})\n\
         Cost:    ${:.4}\n\
         Model:   {}",
        format_tokens(input),
        input,
        format_tokens(output),
        output,
        format_tokens(total),
        total,
        cost,
        ctx.current_model,
    );

    Ok(CommandEffect::SystemMessage(output))
}

pub fn handle_retry_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let last_user_msg = ctx
        .messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, MessageRole::User));

    match last_user_msg {
        Some(_msg) => {
            if *ctx.is_streaming {
                return Ok(CommandEffect::SystemMessage(
                    "Cannot retry while streaming".to_string(),
                ));
            }
            Ok(CommandEffect::RetryLastMessage)
        }
        None => Ok(CommandEffect::SystemMessage(
            "No user message to retry".to_string(),
        )),
    }
}
