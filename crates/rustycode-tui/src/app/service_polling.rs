//! Service polling operations

use super::async_::RecvStatus;
use super::event_loop::TUI;
use crate::app::pipeline::ScheduledPhaseEvent;
use anyhow::Result;
use rustycode_core::team::orchestrator::TeamEvent;

impl TUI {
    /// Poll all services (ONE item per frame each)
    ///
    /// WARNING: This runs in the main UI thread. Keep logic non-blocking and batch
    /// processing to ensure the TUI remains responsive (60FPS). Do not increase
    /// batch sizes without verifying it doesn't cause frame drops.
    pub(crate) fn poll_services(&mut self) -> Result<()> {
        let debug_enabled = crate::logging::is_debug_enabled();
        let poll_start = std::time::Instant::now();

        // Inline poll implementation to avoid closure borrow issues

        // Poll LLM stream chunks — drain up to 8 per frame for smooth streaming.
        // Higher values (32) caused visible stuttering on multi-turn conversations:
        // the LLM sends text faster on subsequent turns, so 20-30 chunks accumulate
        // between frames and all appear at once. Capping at 8 spreads text across
        // more frames for a smoother feel. Tool/state chunks are still processed.
        //
        // We collect chunks into a vec first to release the channel borrow before
        // passing self to the handler (avoids E0499 double-mutable-borrow).
        const MAX_STREAM_CHUNKS_PER_FRAME: usize = 8;
        let mut had_stream = false;
        let mut channel_disconnected = false;
        {
            let mut chunks: Vec<crate::app::async_::StreamChunk> = Vec::new();
            if let Some(channel) = self.services.stream_channel_mut() {
                for _ in 0..MAX_STREAM_CHUNKS_PER_FRAME {
                    match channel.try_recv_ex() {
                        RecvStatus::Item(chunk) => chunks.push(chunk),
                        RecvStatus::Empty => break,
                        RecvStatus::Disconnected => {
                            // Channel disconnected — sender dropped without sending Done
                            channel_disconnected = true;
                            break;
                        }
                    }
                }
            }
            // Process all drained chunks unconditionally — do NOT break on
            // `is_streaming` going false. `handle_stream_chunk(Done)` toggles
            // `is_streaming` false→true (via auto-queued messages), so the flag
            // is not a reliable stream-end signal within a single batch. The
            // channel is bounded (cap 100) and we cap at 8 chunks/frame.
            for chunk in chunks {
                crate::app::handlers::handle_stream_chunk(self, chunk);
                had_stream = true;
            }
        }

        // If channel disconnected while streaming, force cleanup to prevent
        // the TUI from being stuck in is_streaming=true forever.
        if channel_disconnected && self.streaming.is_streaming {
            tracing::warn!("Stream channel disconnected without Done — forcing cleanup");
            self.reset_streaming_state();
            self.active_tools.clear();
            self.services.complete_query();
            self.update_terminal_title();
            self.add_system_message(
                "⚠ Stream connection lost unexpectedly. You can retry.".to_string(),
            );
            self.dirty = true;
        }

        // Poll tool results — drain up to 8 per frame (tools are heavier)
        let mut had_tool = false;
        let mut tool_count = 0usize;
        {
            let mut results: Vec<crate::app::async_::ToolResult> = Vec::new();
            if let Some(channel) = self.services.tool_channel_mut() {
                for _ in 0..8 {
                    match channel.try_recv() {
                        Some(result) => results.push(result),
                        None => break,
                    }
                }
            }
            for result in results {
                crate::app::handlers::handle_tool_result(self, result);
                had_tool = true;
                tool_count += 1;
            }
        }

        // Poll workspace updates
        let had_workspace = {
            let update = self
                .services
                .workspace_channel_mut()
                .and_then(|ch| ch.try_recv());
            match update {
                Some(update) => {
                    crate::app::handlers::handle_workspace_update(self, update);
                    true
                }
                None => false,
            }
        };

        // Poll slash command results
        let had_command = {
            let result = self
                .services
                .command_channel_mut()
                .and_then(|ch| ch.try_recv());
            match result {
                Some(result) => {
                    crate::app::handlers::handle_slash_command_result(self, result);
                    true
                }
                None => false,
            }
        };

        // Log if we processed any events (for debugging)
        if had_stream || had_tool || had_workspace || had_command {
            crate::debug_log!(
                "Processed service events: stream={} tool={} tool_count={} workspace={} command={} elapsed_ms={}",
                had_stream,
                had_tool,
                tool_count,
                had_workspace,
                had_command,
                poll_start.elapsed().as_millis()
            );
        }

        // Poll team events (from TeamOrchestrator broadcast channel)
        self.poll_team_events();

        // Poll pipeline cron scheduler events
        self.poll_scheduler_events();

        // Poll worker registry updates
        self.poll_worker_registry();

        // Poll background bash command result
        let bash_result = {
            let mut store = self
                .streaming
                .pending_bash_result
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            store.take()
        };
        if let Some(text) = bash_result {
            // Truncate long output for display
            let display = if text.len() > 4000 {
                let byte_limit = text.floor_char_boundary(4000);
                let end = text[..byte_limit].rfind('\n').unwrap_or(byte_limit);
                format!(
                    "{}\n... ({} chars truncated)",
                    &text[..end],
                    text.len() - end
                )
            } else {
                text
            };
            self.add_system_message(format!("✓ {}", display));
            self.auto_scroll();
            self.dirty = true;
        }

        if debug_enabled {
            let elapsed = poll_start.elapsed();
            if elapsed > std::time::Duration::from_millis(2) {
                crate::debug_log!(
                    "Service poll ran long: elapsed_ms={} stream={} tool={} tool_count={} workspace={} command={}",
                    elapsed.as_millis(),
                    had_stream,
                    had_tool,
                    tool_count,
                    had_workspace,
                    had_command
                );
            }
        }

        Ok(())
    }

    /// Poll team events from the orchestrator
    fn poll_team_events(&mut self) {
        if let Some(ref mut rx) = self.team_handler.event_rx {
            let mut team_messages: Vec<String> = Vec::new();
            // Drain all available team events (they're small and cheap to process)
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        self.team_panel.handle_event(&event);
                        self.dirty = true;

                        // Collect chat messages for key events (applied after loop
                        // to avoid borrow conflicts with team_panel)
                        match &event {
                            TeamEvent::AgentActivated { role, turn, reason } => {
                                team_messages.push(format!(
                                    "[Team] {} activated (turn {}): {}",
                                    role, turn, reason
                                ));
                            }
                            TeamEvent::Insight { role, message } => {
                                team_messages.push(format!("[Team/{}] {}", role, message));
                            }
                            TeamEvent::TaskCompleted {
                                success,
                                turns,
                                files_modified,
                                ..
                            } => {
                                let status = if *success { "SUCCESS" } else { "FAILED" };
                                let files_msg = if files_modified.is_empty() {
                                    String::new()
                                } else {
                                    format!("\n   Files: {}", files_modified.join(", "))
                                };
                                team_messages.push(format!(
                                    "[Team] {} in {} turns.{}",
                                    status, turns, files_msg
                                ));
                            }
                            TeamEvent::CodeChanged { files, author, .. } => {
                                team_messages.push(format!(
                                    "[Team] {} modified: {}",
                                    author,
                                    files.join(", ")
                                ));
                            }
                            TeamEvent::CompilationFailed { errors, .. } => {
                                team_messages.push(format!(
                                    "[Team] Compilation failed: {} error(s)",
                                    errors.len()
                                ));
                            }
                            _ => {} // Other events handled by panel only
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        // Channel closed — orchestrator finished
                        self.team_handler.event_rx = None;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!("Team event receiver lagged by {} events", n);
                        continue;
                    }
                }
            }
            // Apply collected messages (separate from team_panel borrow)
            for msg in team_messages {
                self.add_system_message(msg);
            }
        }
    }

    /// Poll worker registry and update worker panel
    fn poll_worker_registry(&mut self) {
        use rustycode_protocol::worker_registry::global_worker_registry;

        // Skip polling if the worker panel isn't visible and there are no active agents
        // This avoids needless global_worker_registry() calls every frame
        if !self.worker_panel.visible && self.agent_manager.agents().is_empty() {
            return;
        }

        let registry = global_worker_registry();
        let workers = registry.list();

        let prev_count = self.worker_panel.total_workers();
        self.worker_panel.update_from_workers(&workers);

        // Mark dirty only when worker count or panel visibility changed
        if prev_count != workers.len() || (!workers.is_empty() && self.worker_panel.visible) {
            self.dirty = true;
        }
    }

    /// Poll pipeline cron scheduler events (drain all available)
    fn poll_scheduler_events(&mut self) {
        // Take the receiver temporarily to avoid borrow conflicts with self
        let rx = match self.scheduler_rx.take() {
            Some(rx) => rx,
            None => return,
        };

        let mut events: Vec<ScheduledPhaseEvent> = Vec::new();
        let mut disconnected = false;

        loop {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    tracing::warn!("Scheduler event channel disconnected");
                    disconnected = true;
                    break;
                }
            }
        }

        // Put the receiver back (unless disconnected)
        if !disconnected {
            self.scheduler_rx = Some(rx);
        }

        // Process collected events (no borrow conflict now)
        for event in events {
            self.handle_scheduled_phase_event(event);
            self.dirty = true;
        }
    }

    fn handle_scheduled_phase_event(&mut self, event: ScheduledPhaseEvent) {
        match event {
            ScheduledPhaseEvent::PhaseReady { phase_id, .. } => {
                if self.active_scheduled_phases.len() >= self.max_concurrent_phases {
                    tracing::warn!(
                        "Scheduler: skipping phase '{}' — concurrency limit ({}) reached",
                        phase_id,
                        self.max_concurrent_phases
                    );
                    self.add_system_message(format!(
                        "⏳ Scheduled phase '{}' skipped — max concurrent phases ({}) reached",
                        phase_id, self.max_concurrent_phases
                    ));
                    return;
                }
                self.active_scheduled_phases.insert(phase_id.clone());
                self.add_system_message(format!("⏰ Scheduled phase '{}' triggered", phase_id));
                self.auto_scroll();
            }
            ScheduledPhaseEvent::PhaseStarting {
                phase_id,
                cron_expr,
            } => {
                self.active_scheduled_phases.insert(phase_id.clone());
                self.add_system_message(format!(
                    "⏰ Scheduled phase '{}' starting (cron: {})",
                    phase_id, cron_expr
                ));
                self.auto_scroll();
            }
            ScheduledPhaseEvent::PhaseCompleted { phase_id, duration } => {
                self.active_scheduled_phases.remove(&phase_id);
                self.add_system_message(format!(
                    "✅ Scheduled phase '{}' completed ({:.1}s)",
                    phase_id,
                    duration.as_secs_f64()
                ));
                self.auto_scroll();
            }
            ScheduledPhaseEvent::PhaseFailed { phase_id, error } => {
                self.active_scheduled_phases.remove(&phase_id);
                self.add_system_message(format!(
                    "❌ Scheduled phase '{}' failed: {}",
                    phase_id, error
                ));
                self.auto_scroll();
            }
            ScheduledPhaseEvent::PhaseSkipped { phase_id, reason } => {
                self.add_system_message(format!(
                    "⏭ Scheduled phase '{}' skipped: {}",
                    phase_id, reason
                ));
                self.auto_scroll();
            }
            ScheduledPhaseEvent::SchedulerError { phase_id, error } => {
                self.active_scheduled_phases.remove(&phase_id);
                self.add_system_message(format!(
                    "❌ Scheduler error for phase '{}': {}",
                    phase_id, error
                ));
                self.auto_scroll();
            }
            ScheduledPhaseEvent::SchedulerStarted { phase_count } => {
                self.add_system_message(format!(
                    "🔄 Pipeline cron scheduler started ({} phases scheduled)",
                    phase_count
                ));
            }
            ScheduledPhaseEvent::SchedulerStopped => {
                self.add_system_message("⏹ Pipeline cron scheduler stopped".to_string());
            }
        }
    }
}
