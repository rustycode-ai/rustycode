//! Workspace update and slash command result handlers.

use crate::app::async_::{SlashCommandResult, WorkspaceUpdate};
use crate::app::TUI;
use tracing;

pub fn handle_workspace_update(tui: &mut TUI, update: WorkspaceUpdate) {
    match update {
        WorkspaceUpdate::ContextLoaded(context) => {
            tui.workspace_loaded = true;
            tui.workspace_context = Some(context.clone()); // Store workspace context!
            tui.workspace_scan_progress = None; // Clear progress
            tracing::debug!("Workspace context loaded ({} bytes)", context.len());

            // Detect git branch for status bar
            tui.git_branch = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        if !branch.is_empty() && branch != "HEAD" {
                            return Some(branch);
                        }
                    }
                    None
                });

            // Add system notification
            tui.add_system_message(format!(
                "Workspace loaded ({} files indexed)",
                context.lines().count()
            ));
        }
        WorkspaceUpdate::Notice(message) => {
            tracing::info!("Workspace notice: {}", message);
            tui.add_system_message(message);
        }
        WorkspaceUpdate::ScanProgress { scanned, total } => {
            tracing::debug!("Workspace scan: {}/{}", scanned, total);
            let new_pct = if total > 0 {
                ((scanned as f64 / total as f64 * 100.0).round() as u16).clamp(0, 100)
            } else {
                0
            };
            let old_pct = tui.workspace_scan_progress.map(|(old_scanned, old_total)| {
                if old_total > 0 {
                    ((old_scanned as f64 / old_total as f64 * 100.0).round() as u16).clamp(0, 100)
                } else {
                    0
                }
            });

            tui.workspace_scan_progress = Some((scanned, total));

            // Only force a redraw when the visible progress indicator changes.
            // The scan can emit many raw progress events with the same displayed
            // percentage, and redrawing every one of them causes the startup
            // stutter we were seeing.
            if old_pct != Some(new_pct) {
                tui.dirty = true;
            }
        }
        WorkspaceUpdate::ScanComplete {
            file_count,
            dir_count,
        } => {
            tracing::debug!(
                "Workspace scan complete: {} files, {} dirs",
                file_count,
                dir_count
            );
        }
        WorkspaceUpdate::Error(err) => {
            tracing::error!("Workspace loading error: {}", err);
            // User-friendly error (less technical)
            tui.add_system_message(
                "⚠️  Workspace loading issue - some features may be limited".to_string(),
            );
            tui.auto_scroll();
        }
    }
}

pub fn handle_slash_command_result(tui: &mut TUI, result: SlashCommandResult) {
    match result {
        SlashCommandResult::Success(output) => {
            tui.add_system_message(output);
        }
        SlashCommandResult::Error(err) => {
            tui.add_system_message(format!("Command failed: {}", err));
        }
        SlashCommandResult::LoadedSession { .. } => {
            // This variant should not arrive here — LoadSession is handled
            // via CommandEffect in the synchronous path. Log if it does.
            tracing::warn!(
                "LoadedSession arrived via async result — should use CommandEffect instead"
            );
        }
    }
    tui.dirty = true;
    tui.auto_scroll();
}
