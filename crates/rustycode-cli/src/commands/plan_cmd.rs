//! Plan command handler
//!
//! Extracted from main.rs to isolate plan-related CLI logic.

use crate::commands::cli_args::PlanCommand;
use anyhow::Result;
use rustycode_protocol::{PlanId, PlanStatus, SessionId};
use rustycode_runtime::AsyncRuntime;
use rustycode_team::plan_manager::PlanManager;
use rustycode_team::profiler::TaskProfiler;
use std::path::Path;

use crate::prompt::{Confirm, Prompt};

/// Resolve an ID string to a `SessionId`.
///
/// Accepts either a session ID (`sess_*`) or a plan ID (`plan_*`).
/// When a plan ID is given, loads the plan and extracts its session ID.
async fn resolve_session_id(runtime: &AsyncRuntime, id: &str) -> Result<SessionId> {
    if id.starts_with("sess_") {
        Ok(SessionId::parse(id)?)
    } else if id.starts_with("plan_") {
        let plan_id = PlanId::parse(id).map_err(|e| anyhow::anyhow!("invalid plan ID: {e}"))?;
        match runtime.load_plan(&plan_id).await? {
            Some(plan) => Ok(plan.session_id),
            None => anyhow::bail!("Plan {id} not found"),
        }
    } else {
        anyhow::bail!("Invalid ID format: expected sess_* or plan_*, got {id}")
    }
}

/// Execute a `plan` subcommand.
pub async fn execute(
    runtime: &AsyncRuntime,
    cwd: &Path,
    command: PlanCommand,
    format: &str,
) -> Result<()> {
    match command {
        PlanCommand::Preview { task } => {
            let profiler = TaskProfiler::new();
            let profile = profiler.profile(&task);

            println!("Task Profile");
            println!("============");
            println!("Risk level    : {:?}", profile.risk);
            println!("Reach         : {:?}", profile.reach);
            println!("Familiarity   : {:?}", profile.familiarity);
            println!("Reversibility : {:?}", profile.reversibility);
            println!("Strategy      : {:?}", profile.strategy);
            if !profile.signals.is_empty() {
                println!("Signals:");
                for sig in &profile.signals {
                    println!("  - {} (weight: {:.1})", sig.evidence, sig.weight);
                }
            }
            println!();

            let session_id = SessionId::new();
            let mgr = PlanManager::create_plan(session_id, &task, &profile);
            let plan = mgr.plan();

            println!("Generated Plan");
            println!("==============");
            println!("Approach: {}", plan.approach);
            println!("Steps ({} total):", plan.steps.len());
            for (i, step) in plan.steps.iter().enumerate() {
                println!("  {}. {} — {}", i + 1, step.title, step.description);
                if !step.tools.is_empty() {
                    println!("     Tools: {}", step.tools.join(", "));
                }
            }
            if !plan.risks.is_empty() {
                println!("Identified risks:");
                for risk in &plan.risks {
                    println!("  - {}", risk);
                }
            }
        }
        PlanCommand::New { task } => {
            let report = runtime.start_planning(cwd, &task).await?;
            println!("Planning session created.");
            println!("  Session ID : {}", report.session.id);
            println!("  Plan ID    : {}", report.plan.id);
            println!("  Plan file  : {}", report.plan_path.display());
            println!();
            println!("Edit the plan file, then run:");
            println!("  rustycode plan approve {}", report.session.id);
        }
        PlanCommand::List { limit } => {
            let plans = runtime.all_plans(limit).await?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&plans)?);
            } else if plans.is_empty() {
                println!("No plans found.");
            } else {
                for p in &plans {
                    println!("{:<38}  {:10}  {}", p.id, format!("{:?}", p.status), p.task);
                }
            }
        }
        PlanCommand::Show { id } => {
            let plan_id =
                PlanId::parse(&id).map_err(|e| anyhow::anyhow!("invalid plan ID: {e}"))?;
            match runtime.load_plan(&plan_id).await? {
                Some(plan) => println!("{}", serde_json::to_string_pretty(&plan)?),
                None => println!("Plan {id} not found."),
            }
        }
        PlanCommand::Approve { session_id } => {
            let sid = resolve_session_id(runtime, &session_id).await?;

            let confirmed = Confirm::new(format!("Approve plan for session {}?", sid))
                .with_default(true)
                .prompt()?;

            if confirmed {
                runtime.approve_plan(&sid, cwd).await?;
                println!("Plan approved. Session {sid} is now in Executing mode.");
            } else {
                println!("Plan approval cancelled.");
            }
        }
        PlanCommand::Reject { session_id } => {
            let sid = resolve_session_id(runtime, &session_id).await?;

            let confirmed = Confirm::new(format!("Reject plan for session {}?", sid))
                .with_default(false)
                .prompt()?;

            if confirmed {
                runtime.reject_plan(&sid, cwd).await?;
                println!("Plan rejected.");
            } else {
                println!("Plan rejection cancelled.");
            }
        }
        PlanCommand::Execute { session_id } => {
            let sid = SessionId::parse(&session_id)?;
            match runtime.load_plan_for_session(&sid).await? {
                Some(plan) => {
                    if plan.status != PlanStatus::Approved && plan.status != PlanStatus::Executing {
                        println!(
                            "Plan must be approved first. Current status: {:?}",
                            plan.status
                        );
                    } else {
                        println!("Executing plan step...");
                        runtime.execute_plan_step(&sid).await?;

                        if let Some(updated_plan) = runtime.load_plan_for_session(&sid).await? {
                            println!("Step completed.");
                            if let Some(idx) = updated_plan.current_step_index {
                                println!("  Current step: {}/{}", idx, updated_plan.steps.len());
                            }
                            if let Some(error) = &updated_plan.execution_error {
                                println!("  ⚠ Error: {}", error);
                            } else {
                                println!("  ✓ Success");
                            }
                        }
                    }
                }
                None => println!("Plan not found for session {session_id}."),
            }
        }
        PlanCommand::Status { session_id } => {
            let sid = SessionId::parse(&session_id)?;
            match runtime.load_plan_for_session(&sid).await? {
                Some(plan) => {
                    println!("Plan Status: {:?}", plan.status);
                    println!("Task: {}", plan.task);
                    println!("Steps: {}", plan.steps.len());

                    if let Some(idx) = plan.current_step_index {
                        println!("Current Step: {} of {}", idx + 1, plan.steps.len());
                        if idx < plan.steps.len() {
                            let step = &plan.steps[idx];
                            println!("  Description: {}", step.description);
                            println!("  Status: {:?}", step.execution_status);
                            if !step.errors.is_empty() {
                                println!("  Errors:");
                                for error in &step.errors {
                                    println!("    - {}", error);
                                }
                            }
                        }
                    } else {
                        println!("Execution not started.");
                    }

                    if let Some(start) = plan.execution_started_at {
                        println!("Execution started: {}", start);
                        if let Some(completed) = plan.execution_completed_at {
                            println!("Execution completed: {}", completed);
                        }
                    }

                    if let Some(error) = &plan.execution_error {
                        println!("⚠ Execution error: {}", error);
                    }
                }
                None => println!("Plan not found for session {session_id}."),
            }
        }
    }

    Ok(())
}
