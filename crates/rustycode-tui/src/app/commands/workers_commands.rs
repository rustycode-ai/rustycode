//! Worker and Cron slash command handlers

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;
use rustycode_protocol::cron_registry::global_cron_registry;
use rustycode_protocol::worker_registry::{global_worker_registry, WorkerStatus};

/// Handle /workers commands
pub fn handle_workers_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let registry = global_worker_registry();

    if parts.len() < 2 || parts[1] == "list" {
        let workers = registry.list();

        if workers.is_empty() {
            return Ok(CommandEffect::SystemMessage(
                "No workers (sub-agents) tracked.\n\n\
                 Workers are automatically created when you use:\n\
                 • spawn_agent tool - delegates to specialized agents\n\
                 • /agent spawn - spawns agent for specific task\n\n\
                 Use /workers help for more commands."
                    .to_string(),
            ));
        }

        let mut output = String::from("📊 Worker Status:\n\n");
        output.push_str(&format!("Total: {} workers\n\n", workers.len()));

        // Group by status
        let spawning = registry.workers_by_status(WorkerStatus::Spawning);
        let running = registry.workers_by_status(WorkerStatus::Running);
        let finished = registry.workers_by_status(WorkerStatus::Finished);
        let failed = registry.workers_by_status(WorkerStatus::Failed);

        if !spawning.is_empty() {
            output.push_str(&format!("🔄 Spawning: {}\n", spawning.len()));
            for w in &spawning {
                output.push_str(&format!(
                    "   {} - {}{}\n",
                    w.worker_id,
                    w.task_description.as_deref().unwrap_or("No task"),
                    if w.trust_gate_cleared {
                        " [trust OK]"
                    } else {
                        ""
                    }
                ));
            }
            output.push('\n');
        }

        if !running.is_empty() {
            output.push_str(&format!("⚙️ Running: {}\n", running.len()));
            for w in &running {
                output.push_str(&format!(
                    "   {} - {}\n",
                    w.worker_id,
                    w.task_description.as_deref().unwrap_or("Processing")
                ));
            }
            output.push('\n');
        }

        if !finished.is_empty() {
            output.push_str(&format!("✅ Finished: {}\n", finished.len()));
            for w in &finished {
                output.push_str(&format!(
                    "   {} - {}{}\n",
                    w.worker_id,
                    w.result_summary.as_deref().unwrap_or("Completed"),
                    if let Some(count) = w
                        .events
                        .iter()
                        .filter(|e| matches!(
                            e,
                            rustycode_protocol::worker_registry::WorkerEvent::TaskCompleted { .. }
                        ))
                        .count()
                        .checked_sub(1)
                    {
                        format!(" ({} retries)", count)
                    } else {
                        String::new()
                    }
                ));
            }
            output.push('\n');
        }

        if !failed.is_empty() {
            output.push_str(&format!("❌ Failed: {}\n", failed.len()));
            for w in &failed {
                output.push_str(&format!(
                    "   {} - {} [{}]\n",
                    w.worker_id,
                    w.task_description.as_deref().unwrap_or("Failed"),
                    w.last_error
                        .as_ref()
                        .map(|e| e.kind.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ));
            }
            output.push('\n');
        }

        output.push_str("\n💡 Tip: Workers are tracked automatically when using spawn_agent tool.");

        return Ok(CommandEffect::SystemMessage(output));
    }

    match parts[1] {
        "help" => Ok(CommandEffect::SystemMessage(
            "Worker Management Commands:\n\n\
             /workers list          - Show all workers and their status\n\
             /workers help          - Show this help message\n\n\
             Workers (sub-agents) are automatically tracked when:\n\
             • LLM calls spawn_agent tool\n\
             • You use /agent spawn command\n\n\
             Status icons:\n\
             🔄 Spawning - Worker is initializing\n\
             ⚙️ Running  - Worker is processing task\n\
             ✅ Finished - Worker completed successfully\n\
             ❌ Failed   - Worker encountered error"
                .to_string(),
        )),
        _ => Ok(CommandEffect::SystemMessage(format!(
            "Unknown /workers subcommand: '{}'\nUse /workers help for available commands.",
            parts[1]
        ))),
    }
}

/// Handle /cron commands
pub fn handle_cron_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let registry = global_cron_registry();

    if parts.len() < 2 || parts[1] == "list" {
        let entries = registry.list(false);

        if entries.is_empty() {
            return Ok(CommandEffect::SystemMessage(
                "No scheduled cron tasks.\n\n\
                 Cron tasks run automatically on a schedule (e.g., daily tests).\n\
                 Use /cron help for more commands."
                    .to_string(),
            ));
        }

        let mut output = String::from("⏰ Scheduled Tasks:\n\n");
        output.push_str(&format!("Total: {} entries\n\n", entries.len()));

        let enabled = registry.enabled();
        let disabled: Vec<_> = entries.iter().filter(|e| !e.enabled).cloned().collect();

        if !enabled.is_empty() {
            output.push_str(&format!("🟢 Enabled: {}\n", enabled.len()));
            for entry in &enabled {
                output.push_str(&format!(
                    "   {} [{}]\n      Schedule: {}\n      Prompt: {}\n",
                    entry.cron_id,
                    entry.description.as_deref().unwrap_or("No description"),
                    entry.schedule,
                    entry.prompt
                ));
                if let Some(last) = entry.last_run_at {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let ago = secs.saturating_sub(last);
                    output.push_str(&format!(
                        "      Last run: {} seconds ago ({} runs)\n",
                        ago, entry.run_count
                    ));
                } else {
                    output.push_str(&format!(
                        "      Last run: Never ({} runs queued)\n",
                        entry.run_count
                    ));
                }
                output.push('\n');
            }
        }

        if !disabled.is_empty() {
            output.push_str(&format!("⚪ Disabled: {}\n", disabled.len()));
            for entry in &disabled {
                output.push_str(&format!(
                    "   {} [{}] - {}\n",
                    entry.cron_id,
                    entry.schedule,
                    entry.description.as_deref().unwrap_or("No description")
                ));
            }
        }

        output.push_str("\n💡 Tip: Use /cron help to learn how to create scheduled tasks.");

        return Ok(CommandEffect::SystemMessage(output));
    }

    match parts[1] {
        "help" => Ok(CommandEffect::SystemMessage(
            "Cron Management Commands:\n\n\
             /cron list           - Show all scheduled tasks\n\
             /cron help           - Show this help message\n\n\
             Cron tasks run autonomously on a schedule.\n\
             Schedule format: 5-field cron expression\n\
             Example: \"0 9 * * *\" = daily at 9am\n\n\
             Fields: minute hour day-of-month month day-of-week\n\
             * = any, 0-59 (min), 0-23 (hour), 1-31 (day), 1-12 (month), 0-6 (weekday)"
                .to_string(),
        )),
        _ => Ok(CommandEffect::SystemMessage(format!(
            "Unknown /cron subcommand: '{}'\nUse /cron help for available commands.",
            parts[1]
        ))),
    }
}
