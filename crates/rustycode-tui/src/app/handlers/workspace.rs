//! Workspace update and slash command result handlers.

use crate::app::async_::{SlashCommandResult, WorkspaceUpdate};
use crate::app::TUI;
use rustycode_protocol::{CommandEvent, EventMsg, WorkspaceEvent};
use tracing;

pub fn handle_workspace_update(tui: &mut TUI, update: WorkspaceUpdate) {
    let event = match &update {
        WorkspaceUpdate::ContextLoaded(context) => {
            // Guard: if workspace already loaded, this is a feedback loop
            // from the event channel re-processing. Skip to prevent
            // infinite message growth.
            if tui.workspace.workspace_loaded {
                tracing::debug!("Skipping duplicate ContextLoaded (workspace already loaded)");
                return;
            }
            tui.workspace.workspace_loaded = true;
            tui.workspace.workspace_context = Some(context.clone());
            tui.workspace.workspace_scan_progress = None;
            tracing::debug!("Workspace context loaded ({} bytes)", context.len());

            // Detect git branch for status bar
            tui.workspace.git_branch = std::process::Command::new("git")
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

            // Note: do NOT re-emit to event channel — that creates a
            // feedback loop (event_msg converts back to WorkspaceUpdate).
            None
        }
        WorkspaceUpdate::Notice(message) => {
            tracing::info!("Workspace notice: {}", message);
            tui.add_system_message(message.clone());

            Some(EventMsg::Workspace(WorkspaceEvent::Notice(message.clone())))
        }
        WorkspaceUpdate::ScanProgress { scanned, total } => {
            tracing::debug!("Workspace scan: {}/{}", scanned, total);
            let new_pct = if *total > 0 {
                ((*scanned as f64 / *total as f64 * 100.0).round() as u16).clamp(0, 100)
            } else {
                0
            };
            let old_pct = tui
                .workspace
                .workspace_scan_progress
                .map(|(old_scanned, old_total)| {
                    if old_total > 0 {
                        ((old_scanned as f64 / old_total as f64 * 100.0).round() as u16)
                            .clamp(0, 100)
                    } else {
                        0
                    }
                });

            tui.workspace.workspace_scan_progress = Some((*scanned, *total));

            // Only force a redraw when the visible progress indicator changes.
            // The scan can emit many raw progress events with the same displayed
            // percentage, and redrawing every one of them causes the startup
            // stutter we were seeing.
            if old_pct != Some(new_pct) {
                tui.sys.dirty = true;
            }

            Some(EventMsg::Workspace(WorkspaceEvent::ScanProgress {
                scanned: *scanned,
                total: *total,
            }))
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

            Some(EventMsg::Workspace(WorkspaceEvent::ScanComplete {
                file_count: *file_count,
                dir_count: *dir_count,
            }))
        }
        WorkspaceUpdate::Error(err) => {
            tracing::error!("Workspace loading error: {}", err);
            // User-friendly error (less technical)
            tui.add_system_message(
                "⚠️  Workspace loading issue - some features may be limited".to_string(),
            );
            tui.auto_scroll();

            Some(EventMsg::Workspace(WorkspaceEvent::Error(err.clone())))
        }
    };

    if let Some(event) = event {
        tui.integration.services.send_event(event);
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
    tui.sys.dirty = true;
    tui.auto_scroll();
}
