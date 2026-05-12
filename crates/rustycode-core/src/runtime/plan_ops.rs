//! Runtime planning operations.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use rustycode_protocol::{Plan, PlanId, PlanStatus, Session, SessionId};

use super::{PlanReport, Runtime};

impl Runtime {
    /// Start planning a task (sync version).
    pub fn start_planning(&self, cwd: &Path, task: &str) -> Result<PlanReport> {
        let session = Session::builder().task(task.to_string()).build();
        self.storage.insert_session(&session)?;

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: task.to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: String::new(),
            approach: String::new(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
            milestone_id: None,
        };
        let plan_path = cwd.join("plan.md");

        // Write plan skeleton to disk
        let mut md = String::new();
        md.push_str(&format!("# Plan: {}\n\n", &plan.task));
        md.push_str("## Approach\n\n");
        if plan.approach.is_empty() {
            md.push_str("To be determined\n\n");
        } else {
            md.push_str(&plan.approach);
            md.push_str("\n\n");
        }
        md.push_str("## Steps\n\n");
        if plan.steps.is_empty() {
            md.push_str("To be determined\n\n");
        } else {
            for step in &plan.steps {
                md.push_str(&format!("### {}. {}\n", step.order + 1, &step.title));
                md.push_str(&step.description);
                md.push('\n');
                if !step.tools.is_empty() {
                    md.push_str(&format!("**Tools:** {}\n", step.tools.join(", ")));
                }
                if !step.expected_outcome.is_empty() {
                    md.push_str(&format!(
                        "**Expected outcome:** {}\n",
                        step.expected_outcome
                    ));
                }
                md.push('\n');
            }
        }
        md.push_str("## Files to Modify\n\n");
        if plan.files_to_modify.is_empty() {
            md.push_str("To be determined\n\n");
        } else {
            for f in &plan.files_to_modify {
                md.push_str(&format!("- {f}\n"));
            }
            md.push('\n');
        }
        md.push_str("## Risks\n\n");
        if plan.risks.is_empty() {
            md.push_str("None identified\n");
        } else {
            for r in &plan.risks {
                md.push_str(&format!("- {r}\n"));
            }
        }
        self.storage.insert_plan(&plan)?;

        std::fs::write(&plan_path, &md)
            .with_context(|| format!("failed to write plan to {}", plan_path.display()))?;

        self.publish_session_started(
            session.id.clone(),
            session.task.clone(),
            format!("task={} mode=planning", task),
        );

        Ok(PlanReport {
            session,
            plan,
            plan_path,
        })
    }

    /// Start planning a task (async version for the runtime layer).
    pub async fn start_planning_async(&self, cwd: &Path, task: &str) -> Result<PlanReport> {
        let cwd = cwd.to_path_buf();
        let task = task.to_string();
        // Delegate to the sync version via spawn_blocking
        let config = self.config.clone();
        let inner =
            Runtime::load_from_parts(config, Arc::clone(&self.tools), Arc::clone(&self.bus))?;
        let report = tokio::task::spawn_blocking(move || inner.start_planning(&cwd, &task))
            .await
            .map_err(|e| anyhow::anyhow!(e))??;
        Ok(report)
    }

    /// Approve a plan for execution.
    pub fn approve_plan(&self, session_id: &SessionId, _cwd: &Path) -> Result<()> {
        let plans = self.storage.list_plans(session_id)?;
        if let Some(plan) = plans.first() {
            self.storage
                .update_plan_status(&plan.id, &PlanStatus::Approved)?;
        }
        Ok(())
    }

    /// Reject a plan.
    pub fn reject_plan(&self, session_id: &SessionId) -> Result<()> {
        let plans = self.storage.list_plans(session_id)?;
        if let Some(plan) = plans.first() {
            self.storage
                .update_plan_status(&plan.id, &PlanStatus::Rejected)?;
        }
        Ok(())
    }

    /// List all plans up to a limit.
    pub fn all_plans(&self, limit: usize) -> Result<Vec<Plan>> {
        self.storage.all_plans(limit)
    }

    /// Update a specific plan step.
    pub fn update_plan_step(
        &self,
        plan_id: &PlanId,
        step_index: usize,
        step: &rustycode_protocol::PlanStep,
    ) -> Result<()> {
        self.storage.update_plan_step(plan_id, step_index, step)
    }

    /// Load a plan by ID.
    pub fn load_plan(&self, plan_id: &PlanId) -> Result<Option<Plan>> {
        self.storage.load_plan(plan_id)
    }

    /// Load the plan associated with a session.
    pub fn load_plan_for_session(&self, session_id: &SessionId) -> Result<Option<Plan>> {
        let plans = self.storage.list_plans(session_id)?;
        Ok(plans.into_iter().next())
    }

    /// Execute the next pending step in a plan.
    pub fn execute_plan_step(&self, _session_id: &SessionId) -> Result<()> {
        // Stub: plan step execution requires plan state tracking
        Ok(())
    }
}
