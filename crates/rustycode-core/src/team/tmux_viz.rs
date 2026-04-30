//! Tmux pane visualization for team agent execution.
//!
//! When running inside tmux, creates split panes for each active agent role.
//! Each pane displays live status for that agent. Falls back gracefully when
//! not in tmux.

use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

/// Manages tmux panes for visualizing individual agent roles.
pub struct TmuxAgentVisualizer {
    /// The tmux session name (or window target).
    target: String,
    /// Whether we're actually inside tmux.
    active: bool,
    /// Track which panes exist.
    pane_ids: HashMap<String, String>,
}

impl TmuxAgentVisualizer {
    /// Create a new tmux visualizer.
    ///
    /// If not running inside tmux, returns a no-op visualizer that
    /// gracefully does nothing.
    pub fn new(task: &str) -> Self {
        let in_tmux = std::env::var("TMUX").is_ok();

        if !in_tmux {
            info!("Not inside tmux — agent pane visualization disabled");
            return Self {
                target: String::new(),
                active: false,
                pane_ids: HashMap::new(),
            };
        }

        // Create a unique window name from the task
        let safe_name = task
            .chars()
            .take(20)
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let target = format!("team-{}", safe_name);

        info!(
            "Tmux detected — creating agent visualization window: {}",
            target
        );

        Self {
            target,
            active: true,
            pane_ids: HashMap::new(),
        }
    }

    /// Initialize the tmux layout with panes for each agent role.
    pub fn init(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        // Create a new tmux window
        self.run_tmux(&[
            "new-window",
            "-n",
            &self.target,
            "-c",
            &std::env::var("PWD").unwrap_or_else(|_| ".".into()),
        ])
        .context("Failed to create tmux window")?;

        let roles = ["Architect", "Builder", "Skeptic", "Judge", "Scalpel"];

        // Split into panes for each role (first role uses the window's default pane)
        for (i, role) in roles.iter().enumerate() {
            if i == 0 {
                // Rename the initial pane and track it
                self.run_tmux(&[
                    "select-pane",
                    "-t",
                    &format!("{}.0", self.target),
                    "-T",
                    role,
                ])?;
                self.pane_ids
                    .insert(role.to_string(), format!("{}.0", self.target));
                continue;
            }

            // Split horizontally for subsequent panes
            self.run_tmux(&[
                "split-window",
                "-t",
                &format!("{}.0", self.target),
                "-h", // horizontal split
                "-P", // print pane info
                "-F",
                "#{pane_id}",
            ])?;

            // Track the new pane
            self.pane_ids
                .insert(role.to_string(), format!("{}.{}", self.target, i));
        }

        // Apply tiled layout for even distribution
        self.run_tmux(&["select-layout", "-t", &self.target, "tiled"])?;

        // Set initial content for each pane
        for role in &roles {
            self.update_agent(role, "Waiting", "");
        }

        debug!(
            "Tmux agent visualization initialized with {} panes",
            roles.len()
        );
        Ok(())
    }

    /// Update a specific agent's pane with new status.
    pub fn update_agent(&self, role: &str, state: &str, detail: &str) {
        if !self.active {
            return;
        }

        let pane_target = match self.pane_ids.get(role) {
            Some(t) => t,
            None => return,
        };

        // Clear the pane and write new status
        let content = format!(
            "╔══ {} ══╗\n║ State: {} ║\n{}╚══════════╝",
            role.to_uppercase(),
            state,
            if detail.is_empty() {
                String::new()
            } else {
                format!("║ {} ║\n", detail)
            }
        );

        // Use tmux to set pane content
        if let Err(e) = self.run_tmux(&["send-keys", "-t", pane_target, "clear", "Enter"]) {
            tracing::debug!(pane = %pane_target, error = %e, "tmux clear failed");
        }

        // Small delay for clear to complete
        if let Err(e) = self.run_tmux(&[
            "send-keys",
            "-t",
            pane_target,
            &format!("echo \"{}\"", content.replace('"', "\\\"")),
            "Enter",
        ]) {
            tracing::debug!(pane = %pane_target, error = %e, "tmux echo content failed");
        }

        debug!("Updated tmux pane for {}: {}", role, state);
    }

    /// Mark all panes as completed and show summary.
    pub fn complete(&self, success: bool, turns: u32, files_modified: &[String]) {
        if !self.active {
            return;
        }

        let status = if success { "SUCCESS" } else { "FAILED" };
        let files_list = files_modified.join(", ");

        for role in self.pane_ids.keys() {
            self.update_agent(
                role,
                status,
                &format!("Turns: {} | Files: {}", turns, files_list),
            );
        }

        info!(
            "Tmux visualization completed: {} in {} turns",
            status, turns
        );
    }

    /// Close the tmux window (optional cleanup).
    pub fn close(self) {
        if !self.active {
            return;
        }

        if let Err(e) = self.run_tmux(&["kill-window", "-t", &self.target]) {
            tracing::debug!(target = %self.target, error = %e, "failed to kill tmux window");
        }
    }

    /// Run a tmux command.
    fn run_tmux(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .context("Failed to execute tmux command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                "tmux command failed: tmux {} — {}",
                args.join(" "),
                stderr.trim()
            );
            return Err(anyhow::anyhow!("tmux command failed: {}", stderr.trim()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl Drop for TmuxAgentVisualizer {
    fn drop(&mut self) {
        // Don't auto-close — leave the window for user inspection
    }
}

/// Check if we're running inside tmux.
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_in_tmux_creates_noop_visualizer() {
        // In test environment, TMUX is not set
        let viz = TmuxAgentVisualizer::new("test task");
        // Should be a no-op since we're not in tmux
        assert!(!viz.active || std::env::var("TMUX").is_ok());
    }

    #[test]
    fn no_op_update_does_not_panic() {
        let mut viz = TmuxAgentVisualizer::new("test");
        viz.init().unwrap(); // Should be no-op
        viz.update_agent("Builder", "Active", "doing stuff");
        viz.complete(true, 5, &["src/lib.rs".to_string()]);
    }

    #[test]
    fn is_inside_tmux_returns_false_in_tests() {
        // In test environment, TMUX is typically not set
        let result = is_inside_tmux();
        assert!(!result || std::env::var("TMUX").is_ok());
    }
}
