//! Team mode orchestration integration
//!
//! Handles spawning and cancelling team orchestrators, and wiring events into the team panel.

use anyhow::{Context, Result};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Team mode handler for managing team orchestrator lifecycle
pub struct TeamModeHandler {
    /// Receiver for team orchestration events
    pub event_rx:
        Option<tokio::sync::broadcast::Receiver<rustycode_core::team::orchestrator::TeamEvent>>,
    /// Cancellation token for the running team task
    pub cancel_token: Option<Arc<AtomicBool>>,
}

impl TeamModeHandler {
    /// Create a new team mode handler
    pub fn new() -> Self {
        Self {
            event_rx: None,
            cancel_token: None,
        }
    }

    /// Spawn a TeamOrchestrator on a background thread
    pub fn spawn(
        &mut self,
        task: &str,
        team_panel: &mut crate::ui::team_panel::TeamPanel,
    ) -> Result<(
        String,
        Option<tokio::sync::broadcast::Receiver<rustycode_core::team::orchestrator::TeamEvent>>,
        Option<Arc<AtomicBool>>,
    )> {
        use rustycode_core::team::orchestrator::TeamOrchestrator;

        let cwd = std::env::current_dir().unwrap_or_default();

        // Load provider
        let (provider_type, model, v2_config) = rustycode_llm::load_provider_config_from_env()
            .context("Failed to load provider config for team mode")?;

        let provider =
            rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config)
                .context("Failed to create provider for team mode")?;

        let orchestrator = TeamOrchestrator::new(&cwd, provider, model.to_string());
        let event_rx = orchestrator.subscribe();

        // Get cancel token for cooperative cancellation
        let cancel_token = orchestrator.cancel_token();

        // Show the team panel
        team_panel.set_task(task);
        team_panel.visible = true;
        team_panel.reset();

        // Store the receiver for polling in the event loop
        self.event_rx = Some(event_rx);
        self.cancel_token = Some(cancel_token.clone());

        // Spawn the orchestrator on a background thread
        let task_owned = task.to_string();
        std::thread::spawn(move || {
            rustycode_shared_runtime::block_on_shared(async move {
                if let Err(e) = orchestrator.execute(&task_owned).await {
                    tracing::error!("Team orchestrator failed: {}", e);
                }
            });
        });

        let start_message = format!(
            "🤖 Team mode started: \"{}\"\n   Architect → Builder → Skeptic → Judge → Scalpel\n   Press Ctrl+G to toggle team panel | Esc to cancel",
            task
        );

        Ok((
            start_message,
            self.event_rx.take(),
            self.cancel_token.clone(),
        ))
    }

    /// Cancel a running team orchestrator
    pub fn cancel(&mut self, team_panel: &mut crate::ui::team_panel::TeamPanel) -> String {
        if let Some(token) = &self.cancel_token {
            token.store(true, std::sync::atomic::Ordering::SeqCst);
            team_panel.visible = false;
            self.event_rx = None;
            self.cancel_token = None;
            "⏹ Team task cancelled.".to_string()
        } else {
            "⚠ No team task is running.".to_string()
        }
    }

    /// Check if a team task is currently running
    pub fn is_running(&self) -> bool {
        self.cancel_token.is_some()
    }

    /// Take the event receiver (for wiring into event loop polling)
    pub fn take_event_rx(
        &mut self,
    ) -> Option<tokio::sync::broadcast::Receiver<rustycode_core::team::orchestrator::TeamEvent>>
    {
        self.event_rx.take()
    }

    /// Set the event receiver
    pub fn set_event_rx(
        &mut self,
        rx: tokio::sync::broadcast::Receiver<rustycode_core::team::orchestrator::TeamEvent>,
    ) {
        self.event_rx = Some(rx);
    }

    /// Take the cancel token
    pub fn take_cancel_token(&mut self) -> Option<Arc<AtomicBool>> {
        self.cancel_token.take()
    }

    /// Set the cancel token
    pub fn set_cancel_token(&mut self, token: Arc<AtomicBool>) {
        self.cancel_token = Some(token);
    }
}

impl Default for TeamModeHandler {
    fn default() -> Self {
        Self::new()
    }
}
