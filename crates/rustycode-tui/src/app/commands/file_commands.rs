//! File operation commands: undo, diff, export

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;

/// Handle /undo command - revert the most recent file snapshot batch
pub fn handle_undo_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    match ctx.file_undo_stack.pop() {
        Some(batch) => {
            let mut reverted = Vec::new();
            let mut errors = Vec::new();

            for (path, old_content) in &batch {
                match std::fs::write(path, old_content) {
                    Ok(()) => {
                        let name = std::path::Path::new(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        reverted.push(name);
                    }
                    Err(e) => {
                        errors.push(format!("{}: {}", path, e));
                    }
                }
            }

            let mut msg = format!(
                "Reverted {} file(s): {}",
                reverted.len(),
                reverted.join(", ")
            );
            if !errors.is_empty() {
                msg.push_str(&format!("\nErrors: {}", errors.join("; ")));
            }
            Ok(CommandEffect::SystemMessage(msg))
        }
        None => Ok(CommandEffect::SystemMessage("Nothing to undo".to_string())),
    }
}

/// Handle /diff command - show git diff of uncommitted changes
pub fn handle_diff_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let git_check = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(ctx.cwd)
        .output();

    match git_check {
        Ok(out) if out.status.success() => {}
        _ => {
            return Ok(CommandEffect::SystemMessage(
                "Not a git repository — /diff requires a git repo".to_string(),
            ));
        }
    }

    let output = std::process::Command::new("git")
        .arg("diff")
        .current_dir(ctx.cwd)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();

            if !stderr.is_empty() {
                return Ok(CommandEffect::SystemMessage(format!(
                    "git diff error: {}",
                    stderr.trim()
                )));
            }

            if stdout.trim().is_empty() {
                let staged = std::process::Command::new("git")
                    .args(["diff", "--cached"])
                    .current_dir(ctx.cwd)
                    .output();

                let staged_out = staged
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_default();

                if staged_out.trim().is_empty() {
                    Ok(CommandEffect::SystemMessage(
                        "No uncommitted changes".to_string(),
                    ))
                } else {
                    Ok(CommandEffect::SystemMessage(staged_out))
                }
            } else {
                const MAX_DIFF_LEN: usize = 10_000;
                if stdout.len() > MAX_DIFF_LEN {
                    let truncate_at = stdout.floor_char_boundary(MAX_DIFF_LEN);
                    // Split at last newline for clean diff hunk boundary
                    let end = stdout[..truncate_at].rfind('\n').unwrap_or(truncate_at);
                    Ok(CommandEffect::SystemMessage(format!(
                        "{}\n\n... (truncated, {} bytes total)",
                        &stdout[..end],
                        stdout.len()
                    )))
                } else {
                    Ok(CommandEffect::SystemMessage(stdout))
                }
            }
        }
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "Failed to run git diff: {}",
            e
        ))),
    }
}

/// Handle /export command - export conversation to markdown file
pub fn handle_export_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let export_dir = crate::app::clipboard_export::get_default_export_dir();

    match crate::app::clipboard_export::export_conversation_to_file(ctx.messages, &export_dir) {
        Ok(path) => Ok(CommandEffect::SystemMessage(format!(
            "Conversation exported to {}",
            path.display()
        ))),
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "Export failed: {}",
            e
        ))),
    }
}
