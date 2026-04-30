//! Handler for the `agent` CLI subcommand.
//!
//! Provides autonomous task execution with LLM reasoning through
//! planning, stepping, and session reset. Supports AST (Adaptive
//! Structured Thinking) mode for complex task decomposition.

use crate::commands::cli_args::AgentCommand;
use anyhow::Result;
use rustycode_protocol::SessionId;
use rustycode_runtime::AsyncRuntime;
use std::path::Path;
use std::str::FromStr;

pub async fn execute(runtime: &AsyncRuntime, cwd: &Path, command: AgentCommand) -> Result<()> {
    match command {
        AgentCommand::New {
            task,
            mode,
            use_ast,
        } => {
            if use_ast {
                return execute_ast(&task, cwd).await;
            }

            // Parse mode string if provided
            let working_mode = match mode.as_deref() {
                Some("auto") => None, // Let intent classification decide
                Some(m) => match rustycode_protocol::WorkingMode::from_str(m) {
                    Ok(mode) => Some(mode),
                    Err(_) => {
                        println!("Warning: Unknown mode '{}', using auto", m);
                        None
                    }
                },
                None => None,
            };

            println!("Starting agentic session for task: {}", task);
            if let Some(ref m) = working_mode {
                println!("Using mode: {}", m);
            } else {
                println!("Using auto mode (intent-based selection)");
            }

            let session = runtime.start_planning(cwd, &task).await?;
            println!("Session created: {}", session.session.id);
            println!("Agent will reason about this task autonomously.");
            println!("Use `agent step <session_id>` to execute steps.");
        }
        AgentCommand::Step { session_id } => {
            let sid = SessionId::parse(&session_id)?;
            let session = runtime
                .load_session(&sid)
                .await
                .map_err(|e| anyhow::anyhow!("Session not found '{}': {}", session_id, e))?;
            let task = session.task.clone();
            println!("Executing step in session: {} (task: {})", session_id, task);
            runtime.run_agent(&sid, &task).await?;
            println!("Step completed.");
        }
        AgentCommand::Reset { session_id } => {
            let _sid = SessionId::parse(&session_id)?;
            println!("Reset requested for session: {}", session_id);
            eprintln!(
                "Warning: agent reset is not yet implemented. Session state was not cleared."
            );
        }
    }
    Ok(())
}

/// Execute a task using the AST (Adaptive Structured Thinking) pipeline.
#[allow(clippy::unused_async)]
async fn execute_ast(task: &str, cwd: &Path) -> Result<()> {
    println!("AST mode: {}", task);
    println!(
        "Running 6-phase pipeline: CLASSIFY → RESEARCH → SKELETON → EXPAND → EXECUTE → VERIFY"
    );
    println!();

    let workspace = cwd.to_path_buf();
    let harness = rustycode_orchestration::ast::ToolHarness::ClaudeCode;

    match rustycode_orchestration::execute_with_ast(task, workspace, harness) {
        Ok(result) => {
            if let Some(ref assessment) = result.assessment {
                println!("Task: {}", assessment.task_summary);
                println!("Complexity: {:?}", assessment.complexity);
            }
            println!("Status: {:?}", result.status);
            println!(
                "Milestones completed: {}/{}",
                result.completed_milestones.len(),
                result.completed_milestones.len() + result.consultant_escalation.len()
            );
            if !result.consultant_escalation.is_empty() {
                println!(
                    "Consultant escalation: milestones {:?}",
                    result.consultant_escalation
                );
            }
            println!("Ledger: {}", result.ledger_path.display());
            if let Some(ref report) = result.report {
                println!();
                for cr in &report.results {
                    println!("  [{:?}] {}", cr.status, cr.criterion.description);
                }
            }
        }
        Err(e) => {
            eprintln!("AST pipeline failed: {}", e);
        }
    }
    Ok(())
}
