//! Memory injection and compaction operations
//!
//! Handles auto-memory injection and context compaction.

use super::event_loop::TUI;

impl TUI {
    /// Get injection summary for display without modifying message
    ///
    /// This method checks what memories would be injected and returns a summary string
    /// for display purposes only.
    pub(crate) fn get_injection_summary_display(&self, user_message: &str) -> String {
        use crate::memory_injection::get_injection_summary;

        // Skip if auto-memory is not available
        let auto_memory = match &self.auto_memory {
            Some(mem) => mem,
            None => return String::new(),
        };

        // Skip if injection is disabled
        if !self.memory_injection_config.enabled {
            return String::new();
        }

        // Get all recent and important memories for scoring
        let recent_memories = auto_memory.recent(7); // Last 7 days
        let important_memories = auto_memory.important(0.6); // Importance > 0.6

        // Combine and deduplicate memories
        use std::collections::HashMap;
        let mut memory_map: HashMap<String, _> = HashMap::new();

        for memory in recent_memories
            .into_iter()
            .chain(important_memories.into_iter())
        {
            memory_map.entry(memory.id.clone()).or_insert(memory);
        }

        let all_memories: Vec<_> = memory_map.into_values().collect();

        // Check if we should inject memories
        if all_memories.is_empty() {
            return String::new();
        }

        // Get injection summary for display
        get_injection_summary(user_message, &all_memories, &self.memory_injection_config)
    }

    /// Inject relevant memories into user message if enabled
    ///
    /// This method automatically enhances user messages with relevant memory context
    /// by scoring memories against the message content and injecting high-confidence matches.
    pub(crate) fn inject_memory_if_needed(&mut self, user_message: &str) -> String {
        use crate::memory_injection::{get_injection_summary, inject_memories};

        // Skip if auto-memory is not available
        let auto_memory = match &self.auto_memory {
            Some(mem) => mem,
            None => return user_message.to_string(),
        };

        // Skip if injection is disabled
        if !self.memory_injection_config.enabled {
            return user_message.to_string();
        }

        // Get all recent and important memories for scoring
        let recent_memories = auto_memory.recent(7); // Last 7 days
        let important_memories = auto_memory.important(0.6); // Importance > 0.6

        // Combine and deduplicate memories
        use std::collections::HashMap;
        let mut memory_map: HashMap<String, _> = HashMap::new();

        for memory in recent_memories
            .into_iter()
            .chain(important_memories.into_iter())
        {
            memory_map.entry(memory.id.clone()).or_insert(memory);
        }

        let all_memories: Vec<_> = memory_map.into_values().collect();

        // Check if we should inject memories
        if all_memories.is_empty() {
            return user_message.to_string();
        }

        // Prepare injection
        let enhanced_message =
            inject_memories(user_message, &all_memories, &self.memory_injection_config);

        // Show injection summary to user
        let summary =
            get_injection_summary(user_message, &all_memories, &self.memory_injection_config);

        if !summary.is_empty() {
            // Add system message to show injection happened
            self.add_system_message(summary);
        }

        enhanced_message
    }

    /// Prepare a user message for sending: inject memories, then apply
    /// Active Reasoning Engine activation if the task is complex.
    pub(crate) fn prepare_message_for_send(&mut self, user_message: &str) -> String {
        let with_memory = self.inject_memory_if_needed(user_message);
        let result = crate::services::deep_thinking::analyze_and_transform(&with_memory);
        if result.activated {
            tracing::info!(reason = %result.reason, "Active Reasoning Engine activated");
        }
        result.content
    }

    /// Inject recent shell command history into the first user message (goose pattern).
    ///
    /// On the first message of a conversation, includes the last 10 commands from
    /// `~/.bash_history` (or zsh equivalent) as `<recent_commands>` context.
    /// This gives the AI awareness of what the user has been doing recently,
    /// improving context-aware responses.
    ///
    /// Note: Callers should check `is_first_user_message` BEFORE pushing the
    /// user message to `self.messages`, then pass that flag here.
    pub(crate) fn inject_shell_history_if_first_message(&self, message: &str) -> String {
        // Try to read shell history
        let shell_history = match self.read_recent_shell_commands(10) {
            Some(cmds) if !cmds.is_empty() => cmds,
            _ => return message.to_string(),
        };

        format!(
            "<recent_commands>\n{}\n</recent_commands>\n\n{}",
            shell_history.join("\n"),
            message
        )
    }

    /// Read recent commands from shell history files.
    ///
    /// Checks ~/.bash_history, ~/.zsh_history, and ~/.rustycode_command_history.
    /// Returns the most recent `count` commands, deduplicated.
    fn read_recent_shell_commands(&self, count: usize) -> Option<Vec<String>> {
        let home = std::env::var("HOME").ok()?;
        let mut commands = Vec::new();

        // Read rustycode command history (most relevant — commands typed in rustycode)
        let rustycode_path = std::path::Path::new(&home).join(".rustycode_command_history");
        if rustycode_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&rustycode_path) {
                for line in content
                    .lines()
                    .rev()
                    .filter(|l| !l.trim().is_empty())
                    .take(count)
                {
                    commands.push(line.to_string());
                }
            }
        }

        // If we got enough from rustycode history, use that
        if commands.len() >= count {
            return Some(commands.into_iter().take(count).collect());
        }

        // Otherwise, supplement with shell history
        let shell_history_paths = [
            std::path::Path::new(&home).join(".bash_history"),
            std::path::Path::new(&home).join(".zsh_history"),
        ];

        for path in &shell_history_paths {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    for line in content.lines().rev().filter(|l| !l.trim().is_empty()) {
                        // Strip zsh timestamp prefix if present: ": 1234567890:0;cmd"
                        let cmd = if line.starts_with(':') {
                            line.split(';').nth(1).unwrap_or(line).trim()
                        } else {
                            line.trim()
                        };
                        if cmd.is_empty() || cmd.len() > 200 {
                            continue;
                        }
                        // Skip commands that are just numbers or very short
                        if cmd.len() < 3 {
                            continue;
                        }
                        if !commands.contains(&cmd.to_string()) {
                            commands.push(cmd.to_string());
                        }
                        if commands.len() >= count {
                            return Some(commands);
                        }
                    }
                }
            }
        }

        if commands.is_empty() {
            None
        } else {
            Some(commands)
        }
    }

    pub(crate) fn check_auto_compaction(&mut self) {
        if !self.compaction_config.auto_compact_enabled {
            return;
        }

        if self.compaction_config.auto_compact_state.disabled {
            tracing::debug!(
                "Auto-compaction disabled after {} consecutive failures",
                self.compaction_config
                    .auto_compact_state
                    .consecutive_failures
            );
            return;
        }

        // Only compact when not streaming and user is idle (no active tools)
        if self.streaming.is_streaming || !self.active_tools.is_empty() {
            return;
        }

        let effective_max = self.compaction_config.effective_max_tokens();
        let threshold_tokens =
            (effective_max as f64 * self.compaction_config.warning_threshold) as usize;

        if self.context_monitor.current_tokens >= threshold_tokens {
            tracing::info!(
                "Token usage at {:.1}% ({}, / {}), executing auto-compaction",
                (self.context_monitor.current_tokens as f64 / effective_max as f64) * 100.0,
                self.context_monitor.current_tokens,
                effective_max
            );

            self.execute_compaction();
        }
    }

    /// Execute compaction with current strategy
    pub(crate) fn execute_compaction(&mut self) {
        use crate::slash_commands::execute_compaction as execute_compaction_fn;

        let strategy = self.compaction_config.strategy;

        tracing::debug!("Executing compaction with strategy: {:?}", strategy);
        self.toast_manager.info("Compacting context...");

        match execute_compaction_fn(self.messages.clone(), strategy) {
            Ok(compacted) => {
                let old_count = self.messages.len();
                let new_count = compacted.len();

                self.messages = compacted;

                // Clamp scroll position to valid range after compaction
                // (messages were removed, so indices may be stale)
                if self.selected_message >= self.messages.len() {
                    self.selected_message = self.messages.len().saturating_sub(1);
                }
                self.scroll_offset_line = 0;
                self.user_scrolled = false;

                self.context_monitor.update(&self.messages);
                self.token_budget.last_turn_input_tokens = self.context_monitor.current_tokens;
                self.compaction_config.auto_compact_state.on_success();

                tracing::debug!(
                    "Compaction complete: {} -> {} messages (saved {} messages)",
                    old_count,
                    new_count,
                    old_count.saturating_sub(new_count)
                );

                self.add_system_message(format!(
                    "💾 Context compacted: {} → {} messages",
                    old_count, new_count
                ));
                self.toast_manager.success(format!(
                    "Context compacted: {} → {} messages",
                    old_count, new_count
                ));
            }
            Err(e) => {
                self.compaction_config.auto_compact_state.on_failure();
                tracing::error!("Compaction failed: {}", e);
                self.add_system_message(format!("⚠ Compaction failed: {}", e));
                self.toast_manager
                    .error(format!("Compaction failed: {}", e));
            }
        }

        self.showing_compaction_preview = false;
        self.pending_compaction = false;
    }
}
