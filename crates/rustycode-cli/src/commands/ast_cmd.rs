use crate::commands::cli_args::AstCommand;
use anyhow::{Context, Result};
use std::path::Path;

#[allow(clippy::unused_async)]
pub async fn execute(cwd: &Path, command: AstCommand) -> Result<()> {
    match command {
        AstCommand::Run {
            task,
            harness,
            dry_run,
            ..
        } => execute_run(cwd, &task, &harness, dry_run)?,
        AstCommand::Status => execute_status(cwd)?,
        AstCommand::Ledger => execute_ledger(cwd)?,
    }
    Ok(())
}

fn execute_run(cwd: &Path, task: &str, harness_name: &str, dry_run: bool) -> Result<()> {
    let harness = match harness_name {
        "claude-code" => rustycode_orchestration::ast::ToolHarness::ClaudeCode,
        "rustycode" => rustycode_orchestration::ast::ToolHarness::RustyCode,
        "gemini" => rustycode_orchestration::ast::ToolHarness::GeminiCli,
        "codex" => rustycode_orchestration::ast::ToolHarness::Codex,
        _ => anyhow::bail!(
            "Unknown harness '{}'. Available: claude-code, rustycode, gemini, codex\n\
             Run `rustycode ast status` to check configured harnesses.",
            harness_name
        ),
    };

    let workspace = cwd.to_path_buf();
    let result = if dry_run {
        rustycode_orchestration::execute_with_ast_dry_run(task, workspace, harness)
            .with_context(|| format!("AST dry-run failed for task: {task}"))?
    } else {
        rustycode_orchestration::execute_with_ast(task, workspace, harness)
            .with_context(|| format!("AST pipeline failed for task: {task}"))?
    };

    if let Some(ref assessment) = result.assessment {
        let complexity_str = match assessment.complexity {
            rustycode_orchestration::ast::ComplexityLevel::Trivial => "Trivial",
            rustycode_orchestration::ast::ComplexityLevel::Moderate => "Moderate",
            rustycode_orchestration::ast::ComplexityLevel::Complex => "Complex",
        };
        println!(
            "Complexity: {} — {}",
            complexity_str, assessment.task_summary
        );
    }
    let status_str = match result.status {
        rustycode_orchestration::ast::VerificationStatus::Pass => "Pass",
        rustycode_orchestration::ast::VerificationStatus::Partial => "Partial",
        rustycode_orchestration::ast::VerificationStatus::Fail => "Fail",
    };
    println!(
        "Status: {} ({} milestones)",
        status_str,
        result.completed_milestones.len()
    );
    if !result.consultant_escalation.is_empty() {
        eprintln!(
            "⚠ Escalated to consultant ({} items)",
            result.consultant_escalation.len()
        );
        eprintln!("  Review escalated items before proceeding.");
    }
    println!("Ledger: {}", result.ledger_path.display());
    if let Some(ref report) = result.report {
        let passed = report
            .results
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    rustycode_orchestration::ast::VerificationStatus::Pass
                )
            })
            .count();
        let total = report.results.len();
        println!();
        println!("Verification ({}/{} passed):", passed, total);
        for cr in &report.results {
            let icon = match cr.status {
                rustycode_orchestration::ast::VerificationStatus::Pass => "✓",
                rustycode_orchestration::ast::VerificationStatus::Partial => "~",
                rustycode_orchestration::ast::VerificationStatus::Fail => "✗",
            };
            let status_label = match cr.status {
                rustycode_orchestration::ast::VerificationStatus::Pass => "PASS",
                rustycode_orchestration::ast::VerificationStatus::Partial => "PARTIAL",
                rustycode_orchestration::ast::VerificationStatus::Fail => "FAIL",
            };
            println!("  {} [{}] {}", icon, status_label, cr.criterion.description);
        }
        if passed < total {
            eprintln!("⚠ Some verifications did not pass. Review details above.");
        }
    }
    Ok(())
}

fn execute_status(cwd: &Path) -> Result<()> {
    let ledger_dir = cwd.join(".ast");
    if !ledger_dir.exists() {
        eprintln!("No AST data found in current workspace.");
        eprintln!("Run `rustycode ast run --task \"description\"` to start.");
        return Ok(());
    }

    let entries: Vec<_> = std::fs::read_dir(&ledger_dir)
        .with_context(|| format!("Failed to read AST directory: {}", ledger_dir.display()))?
        .filter_map(|e| e.ok())
        .collect();

    if entries.is_empty() {
        eprintln!("AST directory exists but is empty.");
        return Ok(());
    }

    println!("Status: {}", cwd.display());

    let db_path = ledger_dir.join("progress.db");
    if db_path.exists() {
        let store = rustycode_orchestration::ast::ProgressStore::open(&db_path)
            .with_context(|| format!("Failed to open progress database: {}", db_path.display()))?;
        let tasks = store
            .list_active_tasks()
            .context("Failed to list active tasks from progress database")?;
        if tasks.is_empty() {
            println!("No active tasks.");
        } else {
            for task in &tasks {
                println!("Task: {} (phase: {})", task.title, task.current_phase);
                let milestones = store
                    .milestones_for_task(&task.id)
                    .with_context(|| format!("Failed to get milestones for task: {}", task.id))?;
                for m in &milestones {
                    println!("  Milestone {}: {} - {}", m.ordinal, m.title, m.status);
                }
            }
        }
    } else {
        println!("Ledger directory: {}", ledger_dir.display());
        for entry in &entries {
            println!("  {}", entry.path().display());
        }
    }
    Ok(())
}

fn execute_ledger(cwd: &Path) -> Result<()> {
    let ledger_dir = cwd.join(".ast");
    let ledger_path = ledger_dir.join("LEDGER.md");

    if let Some(content) = std::fs::read_to_string(&ledger_path)
        .with_context(|| format!("Failed to read ledger at {}", ledger_path.display()))
        .ok()
        .filter(|s| !s.is_empty())
    {
        println!("{}", content);
    } else if ledger_dir.exists() {
        eprintln!("Ledger directory exists but LEDGER.md not found.");
        eprintln!("Directory: {}", ledger_dir.display());
        let entries: Vec<_> = std::fs::read_dir(&ledger_dir)
            .with_context(|| format!("Failed to read AST directory: {}", ledger_dir.display()))?
            .filter_map(|e| e.ok())
            .collect();
        for entry in &entries {
            println!("  {}", entry.file_name().to_string_lossy());
        }
    } else {
        eprintln!("No AST ledger found in current workspace.");
        eprintln!("Run `rustycode ast run --task \"description\"` to create one.");
    }
    Ok(())
}
