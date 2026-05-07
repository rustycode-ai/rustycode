//! Core slash command handlers: agent, team, plan

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;
use rustycode_runtime::multi_agent::AgentRoleExt;

/// Handle /agent commands
pub fn handle_agent_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        return Ok(CommandEffect::SystemMessage(
            "Agent commands:\n\
             • /agent list - Show all agents\n\
             • /agent spawn <role> <task> - Spawn a new agent\n\
             • /agent cancel <id> - Cancel a running agent\n\
             \n\
             Available roles: factual, senior, security, consistency, redundancy, performance, test, docs"
                .to_string(),
        ));
    }

    let subcommand = parts[1];

    match subcommand {
        "list" => {
            let agents = ctx.agent_manager.agents();

            if agents.is_empty() {
                return Ok(CommandEffect::SystemMessage(
                    "No agents running".to_string(),
                ));
            }

            let mut output = String::from("Active Agents:\n\n");

            for agent in &agents {
                output.push_str(&format!(
                    "{} [{}] {} - {}\n",
                    agent.status_icon(),
                    agent.id,
                    agent.role.name(),
                    agent.task
                ));

                if agent.status == crate::agents::AgentStatus::Running {
                    output.push_str(&format!("   Running for {}\n", agent.formatted_time()));
                } else if agent.status == crate::agents::AgentStatus::Completed {
                    if let Some(result) = &agent.result {
                        output.push_str("[OK] Completed\n");
                        if !result.issues.is_empty() {
                            output.push_str(&format!("   Issues: {}\n", result.issues.len()));
                        }
                        if !result.suggestions.is_empty() {
                            output.push_str(&format!(
                                "   Suggestions: {}\n",
                                result.suggestions.len()
                            ));
                        }
                    }
                } else if agent.status == crate::agents::AgentStatus::Failed {
                    if let Some(error) = &agent.error {
                        output.push_str(&format!("   [X] Error: {}\n", error));
                    }
                }
            }

            Ok(CommandEffect::SystemMessage(output))
        }

        "spawn" => {
            if parts.len() < 4 {
                return Ok(CommandEffect::SystemMessage(
                    "Usage: /agent spawn <role> <task>\n\
                     Example: /agent spawn security Review this code for vulnerabilities\n\n\
                     Available roles: factual, senior, security, consistency, redundancy, performance, test, docs"
                        .to_string(),
                ));
            }

            let role_str = parts[2];
            let task = parts[3..].join(" ");

            let role = match rustycode_runtime::multi_agent::AgentRole::from_name(role_str) {
                Some(r) => r,
                None => {
                    return Ok(CommandEffect::SystemMessage(format!(
                        "Unknown role: '{}'\nAvailable roles: factual, senior, security, consistency, redundancy, performance, test, docs",
                        role_str
                    )))
                }
            };

            // Build content from recent messages for context
            let content = ctx
                .messages
                .iter()
                .rev()
                .take(5)
                .rev()
                .map(|m| m.content.clone())
                .collect::<Vec<_>>()
                .join("\n");

            match ctx
                .agent_manager
                .spawn_agent(role, task.clone(), content, None)
            {
                Ok(id) => Ok(CommandEffect::SystemMessage(format!(
                    "🤖 Agent #{} ({}) spawned for: {}",
                    id, role_str, task
                ))),
                Err(e) => Ok(CommandEffect::SystemMessage(format!(
                    "Failed to spawn agent: {}",
                    e
                ))),
            }
        }

        "cancel" => {
            if parts.len() < 3 {
                return Ok(CommandEffect::SystemMessage(
                    "Usage: /agent cancel <id>".to_string(),
                ));
            }

            let id_str = parts[2];
            let id: usize = match id_str.parse() {
                Ok(id) => id,
                Err(_) => {
                    return Ok(CommandEffect::SystemMessage(format!(
                        "[X] Invalid agent ID: {}",
                        id_str
                    )))
                }
            };

            match ctx.agent_manager.cancel_agent(id) {
                Ok(()) => Ok(CommandEffect::SystemMessage(format!(
                    "[OK] Cancelled agent #{}",
                    id
                ))),
                Err(e) => Ok(CommandEffect::SystemMessage(format!(
                    "[X] Failed to cancel agent: {}",
                    e
                ))),
            }
        }

        _ => Ok(CommandEffect::SystemMessage(format!(
            "Unknown agent command: {}\nAvailable: list, spawn, cancel",
            subcommand
        ))),
    }
}

/// Handle /team command - run team mode with multi-agent collaboration
pub fn handle_team_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        return Ok(CommandEffect::SystemMessage(
            "Team Mode - Multi-agent collaboration\n\n\
             Usage: /team <task>\n\
                    /team stop\n\
                    /team status\n\n\
             Commands:\n\
             • /team <task>  - Start team orchestration\n\
             • /team stop    - Cancel running team task\n\
             • /team status  - Show current team status\n\n\
             When to use:\n\
             • High-risk tasks (auth, security, payments)\n\
             • Complex multi-file changes\n\
             • Production migrations\n\
             • Tasks needing built-in code review\n\n\
             Team composition:\n\
             - Architect: Plans structure (high-risk only)\n\
             - Builder: Implements changes\n\
             - Skeptic: Reviews for bugs\n\
             - Judge: Runs tests/checks\n\
             - Scalpel: Fixes errors\n\n\
             Shortcuts:\n\
             • Ctrl+G - Toggle team panel\n\
             • Esc - Cancel running team task\n\n\
             Plan Mode vs Team Mode:\n\
             - /plan - Discuss and design (no execution)\n\
             - /team - Execute with reviews (full automation)"
                .to_string(),
        ));
    }

    // Handle subcommands
    match parts[1] {
        "stop" | "cancel" => Ok(CommandEffect::CancelTeam),
        "status" => {
            // Build a status summary from available context
            let has_cancel = ctx.services.is_query_active();
            Ok(CommandEffect::SystemMessage(format!(
                "Team Status:\n  Cancel token active: {}\n  Use Ctrl+G to toggle panel",
                has_cancel
            )))
        }
        _ => {
            let task = parts[1..].join(" ");
            Ok(CommandEffect::StartTeam { task })
        }
    }
}

/// Handle /plan command - toggle between planning and implementation modes
pub fn handle_plan_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        return Ok(CommandEffect::SystemMessage(
            "Plan Mode - Design and discussion (no execution)\n\n\
             Usage: /plan <task>\n\n\
             When to use:\n\
             • Exploring approaches\n\
             • Architectural discussions\n\
             • Risk assessment\n\
             • Before committing to team mode\n\n\
             Plan Mode vs Team Mode:\n\
             - /plan - Discuss design, no changes\n\
             - /team - Execute with automated review\n\n\
             Example flow:\n\
             1. /plan Add user auth\n\
             2. Review proposed approach\n\
             3. Use /plan again to switch back to implementation mode"
                .to_string(),
        ));
    }

    let task = parts[1..].join(" ");

    Ok(CommandEffect::SystemMessage(format!(
        "📋 Plan mode: \"{}\"\n\n\
         The AI will:\n\
         • Analyze requirements\n\
         • Propose implementation approaches\n\
         • Identify risks and trade-offs\n\
         • Wait for your approval\n\n\
         No files will be modified.\n\
         Use /plan again to switch to implementation mode.",
        task
    )))
}

pub fn handle_yolo_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let task = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        String::new()
    };
    Ok(CommandEffect::SystemMessage(format!(
        "🚀 YOLO mode — fully autonomous, tools auto-approved.\n{}",
        if task.is_empty() {
            "Enter a task to execute.".to_string()
        } else {
            format!("Task: {}", task)
        }
    )))
}

pub fn handle_act_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let task = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        String::new()
    };
    Ok(CommandEffect::SystemMessage(format!(
        "⚡ ACT mode — execute with brief summaries, minimal approval.\n{}",
        if task.is_empty() {
            "Enter a task to execute.".to_string()
        } else {
            format!("Task: {}", task)
        }
    )))
}

pub fn handle_ask_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let task = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        String::new()
    };
    Ok(CommandEffect::SystemMessage(format!(
        "💬 ASK mode — tools require approval, full summaries.\n{}",
        if task.is_empty() {
            "Enter a task to execute.".to_string()
        } else {
            format!("Task: {}", task)
        }
    )))
}

/// Handle /clear command
pub fn handle_clear_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    ctx.messages.clear();
    ctx.current_stream_content.clear();
    *ctx.is_streaming = false;
    Ok(CommandEffect::ClearConversation)
}

/// Handle /workspace command
pub fn handle_workspace_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    ctx.services.start_workspace_loading()?;
    Ok(CommandEffect::SystemMessage(
        "Reloading workspace context".to_string(),
    ))
}

/// Handle /quit command
pub fn handle_quit_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    // Stop any active stream before quitting
    if *ctx.is_streaming {
        ctx.services.request_stop_stream();
        *ctx.is_streaming = false;
    }
    *ctx.running = false;
    Ok(CommandEffect::SystemMessage(
        "Quitting RustyCode".to_string(),
    ))
}

/// Handle /copilot-login command
pub fn handle_copilot_login(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let tx = ctx.command_tx;
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create runtime for copilot login: {}", e);
                return;
            }
        };
        let result = rt.block_on(crate::slash_commands::copilot::handle_copilot_login_command());

        match result {
            Ok(output) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "Copilot login failed: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::AsyncStarted(
        "Starting GitHub Copilot device flow...".to_string(),
    ))
}

/// Handle /harness command - long-running agent task harness
pub fn handle_harness_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        return Ok(CommandEffect::SystemMessage(
            "Harness - Long-running agent task framework\n\n\
             Usage: /harness <status|init|add|run>\n\n\
             Commands:\n\
             • /harness status - Show harness status and progress\n\
             • /harness init [path] - Initialize harness in directory\n\
             • /harness add <description> - Add a task to harness\n\
             • /harness run - Start/resume harness execution\n\n\
             Harness provides:\n\
             - Persistent task tracking across sessions\n\
             - Automatic progress logging\n\
             - Validation commands for task completion\n\
             - Crash recovery and resume capabilities"
                .to_string(),
        ));
    }

    let subcommand = parts[1];
    let cwd = std::env::current_dir().unwrap_or_default();

    match subcommand {
        "status" => {
            let harness_dir = cwd.join(".harness");
            let tasks_file = harness_dir.join("harness-tasks.json");

            if !tasks_file.exists() {
                return Ok(CommandEffect::SystemMessage(
                    "No harness found in current directory.\n\nUse /harness init to create one."
                        .to_string(),
                ));
            }

            match std::fs::read_to_string(&tasks_file) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(tasks) => {
                        let task_array = tasks.get("tasks").and_then(|t| t.as_array());
                        let total = task_array.map(|a| a.len()).unwrap_or(0);
                        let completed = task_array
                            .map(|a| {
                                a.iter()
                                    .filter(|t| {
                                        t.get("status").and_then(|s| s.as_str())
                                            == Some("completed")
                                    })
                                    .count()
                            })
                            .unwrap_or(0);
                        let failed = task_array
                            .map(|a| {
                                a.iter()
                                    .filter(|t| {
                                        t.get("status").and_then(|s| s.as_str()) == Some("failed")
                                    })
                                    .count()
                            })
                            .unwrap_or(0);
                        let pending = total.saturating_sub(completed + failed);

                        let session_count = tasks
                            .get("session_count")
                            .and_then(|s| s.as_u64())
                            .unwrap_or(0);
                        let last_session = tasks
                            .get("last_session")
                            .and_then(|s| s.as_str())
                            .unwrap_or("never");

                        let mut output = format!(
                            "Harness Status\n\n\
                             Tasks: {} total | {} completed | {} pending | {} failed\n\
                             Sessions: {}\n\
                             Last session: {}\n",
                            total, completed, pending, failed, session_count, last_session
                        );

                        if let Some(array) = task_array {
                            if !array.is_empty() {
                                output.push_str("\nTasks:\n");
                                for task in array.iter().take(10) {
                                    let id = task.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                                    let desc = task
                                        .get("description")
                                        .and_then(|d| d.as_str())
                                        .unwrap_or("(no description)");
                                    let status = task
                                        .get("status")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("unknown");
                                    let icon = match status {
                                        "completed" => "✓",
                                        "failed" => "✗",
                                        "running" => "▶",
                                        _ => "○",
                                    };
                                    output.push_str(&format!("  {} [{}] {}\n", icon, id, desc));
                                }
                                if array.len() > 10 {
                                    output.push_str(&format!(
                                        "  ... and {} more\n",
                                        array.len() - 10
                                    ));
                                }
                            }
                        }

                        Ok(CommandEffect::SystemMessage(output))
                    }
                    Err(e) => Ok(CommandEffect::SystemMessage(format!(
                        "Failed to parse harness tasks: {}",
                        e
                    ))),
                },
                Err(e) => Ok(CommandEffect::SystemMessage(format!(
                    "Failed to read harness tasks: {}",
                    e
                ))),
            }
        }
        "init" => {
            let path = parts.get(2).copied().unwrap_or(".");
            let project_path = if path == "." {
                cwd
            } else {
                std::path::PathBuf::from(path)
            };

            if let Err(e) = std::fs::create_dir_all(&project_path) {
                return Ok(CommandEffect::SystemMessage(format!(
                    "Failed to create directory: {}",
                    e
                )));
            }

            let harness_dir = project_path.join(".harness");
            if let Err(e) = std::fs::create_dir_all(&harness_dir) {
                return Ok(CommandEffect::SystemMessage(format!(
                    "Failed to create .harness directory: {}",
                    e
                )));
            }

            let tasks_file = harness_dir.join("harness-tasks.json");
            let initial_tasks = serde_json::json!({
                "version": 1,
                "created": chrono::Utc::now().to_rfc3339(),
                "session_config": {
                    "concurrency_mode": "exclusive",
                    "max_tasks_per_session": 10,
                    "max_sessions": 50
                },
                "tasks": [],
                "session_count": 0,
                "last_session": null
            });

            match std::fs::write(
                &tasks_file,
                serde_json::to_string_pretty(&initial_tasks).unwrap_or_default(),
            ) {
                Ok(_) => Ok(CommandEffect::SystemMessage(format!(
                    "✓ Harness initialized in {}",
                    project_path.display()
                ))),
                Err(e) => Ok(CommandEffect::SystemMessage(format!(
                    "Failed to write harness tasks: {}",
                    e
                ))),
            }
        }
        "add" => {
            if parts.len() < 3 {
                return Ok(CommandEffect::SystemMessage(
                    "Usage: /harness add <description> [--priority P0|P1|P2]".to_string(),
                ));
            }

            let harness_dir = cwd.join(".harness");
            let tasks_file = harness_dir.join("harness-tasks.json");

            if !tasks_file.exists() {
                return Ok(CommandEffect::SystemMessage(
                    "No harness found. Use /harness init first.".to_string(),
                ));
            }

            let description = parts[2..]
                .iter()
                .take_while(|p| !p.starts_with("--"))
                .copied()
                .collect::<Vec<_>>()
                .join(" ");

            let priority = parts
                .iter()
                .position(|p| p == &"--priority")
                .and_then(|idx| parts.get(idx + 1))
                .copied()
                .unwrap_or("P1");

            match std::fs::read_to_string(&tasks_file) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(mut tasks) => {
                        if let Some(array) = tasks.get_mut("tasks").and_then(|t| t.as_array_mut()) {
                            let id = format!("task-{}", array.len() + 1);
                            array.push(serde_json::json!({
                                "id": id,
                                "description": description,
                                "status": "pending",
                                "priority": priority,
                                "created": chrono::Utc::now().to_rfc3339(),
                                "attempts": 0,
                                "max_attempts": 3
                            }));

                            match std::fs::write(
                                &tasks_file,
                                serde_json::to_string_pretty(&tasks).unwrap_or_default(),
                            ) {
                                Ok(_) => Ok(CommandEffect::SystemMessage(format!(
                                    "✓ Added task '{}' to harness",
                                    id
                                ))),
                                Err(e) => Ok(CommandEffect::SystemMessage(format!(
                                    "Failed to save harness tasks: {}",
                                    e
                                ))),
                            }
                        } else {
                            Ok(CommandEffect::SystemMessage(
                                "Invalid harness tasks format".to_string(),
                            ))
                        }
                    }
                    Err(e) => Ok(CommandEffect::SystemMessage(format!(
                        "Failed to parse harness tasks: {}",
                        e
                    ))),
                },
                Err(e) => Ok(CommandEffect::SystemMessage(format!(
                    "Failed to read harness tasks: {}",
                    e
                ))),
            }
        }
        "run" => Ok(CommandEffect::SystemMessage(
            "Harness run requires the full CLI.\n\nUse: rustycode harness run".to_string(),
        )),
        _ => Ok(CommandEffect::SystemMessage(format!(
            "Unknown harness command: {}\n\nUsage: /harness <status|init|add|run>",
            subcommand
        ))),
    }
}
