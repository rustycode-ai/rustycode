//! Plan and milestone storage methods.
//!
//! Contains `impl Storage` methods for CRUD operations on plans and milestones.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::params;
use rustycode_protocol::{
    Milestone, MilestoneId, MilestoneStatus, Plan, PlanId, PlanStatus, PlanStep, SessionId,
};

use crate::search::milestone_from_row;
use crate::search::plan_from_row;
use crate::Storage;

impl Storage {
    // -- Plans --------------------------------------------------------------------

    pub fn insert_plan(&self, plan: &Plan) -> Result<()> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner).execute(
            "insert into plans (id, session_id, milestone_id, task, created_at, status, summary, approach, steps, files_to_modify, risks)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                plan.id.to_string(),
                plan.session_id.to_string(),
                plan.milestone_id.as_ref().map(ToString::to_string),
                plan.task,
                plan.created_at.to_rfc3339(),
                serde_json::to_string(&plan.status)?,
                plan.summary,
                plan.approach,
                serde_json::to_string(&plan.steps)?,
                serde_json::to_string(&plan.files_to_modify)?,
                serde_json::to_string(&plan.risks)?,
            ],
        )?;
        Ok(())
    }

    // -- Milestones ---------------------------------------------------------------

    pub fn insert_milestone(&self, milestone: &Milestone) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert into milestones (
                id, session_id, title, description, status, plan_ids, plan_dependencies,
                success_criteria, validation_command, created_at, updated_at, completed_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    milestone.id.to_string(),
                    milestone.session_id.to_string(),
                    milestone.title.clone(),
                    milestone.description.clone(),
                    serde_json::to_string(&milestone.status)?,
                    serde_json::to_string(&milestone.plan_ids)?,
                    serde_json::to_string(&milestone.plan_dependencies)?,
                    serde_json::to_string(&milestone.success_criteria)?,
                    milestone.validation_command.clone(),
                    milestone.created_at.to_rfc3339(),
                    milestone.updated_at.to_rfc3339(),
                    milestone.completed_at.as_ref().map(|ts| ts.to_rfc3339()),
                ],
            )?;
        Ok(())
    }

    pub fn load_milestone(&self, id: &MilestoneId) -> Result<Option<Milestone>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, title, description, status, plan_ids, plan_dependencies,
                    success_criteria, validation_command, created_at, updated_at, completed_at
             from milestones where id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id.to_string()], milestone_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_milestones(&self, session_id: &SessionId) -> Result<Vec<Milestone>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, title, description, status, plan_ids, plan_dependencies,
                    success_criteria, validation_command, created_at, updated_at, completed_at
             from milestones where session_id = ?1 order by created_at desc",
        )?;
        let rows = stmt.query_map(params![session_id.to_string()], milestone_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn update_milestone_status(
        &self,
        id: &MilestoneId,
        status: &MilestoneStatus,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let status_json = serde_json::to_string(status)?;
        let updated_at = Utc::now().to_rfc3339();
        if matches!(status, MilestoneStatus::Completed) {
            conn.execute(
                "update milestones set status = ?1, updated_at = ?2, completed_at = ?3 where id = ?4",
                params![status_json, updated_at, Utc::now().to_rfc3339(), id.to_string()],
            )?;
        } else {
            conn.execute(
                "update milestones set status = ?1, updated_at = ?2 where id = ?3",
                params![status_json, updated_at, id.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn add_plan_to_milestone(
        &self,
        milestone_id: &MilestoneId,
        plan_id: &PlanId,
    ) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tx = conn.transaction()?;

        let milestone_row = tx.query_row(
            "select plan_ids from milestones where id = ?1",
            params![milestone_id.to_string()],
            |row| row.get::<_, String>(0),
        )?;
        let mut plan_ids: Vec<PlanId> = serde_json::from_str(&milestone_row)
            .context("failed to deserialize milestone plan ids")?;
        if !plan_ids.contains(plan_id) {
            plan_ids.push(plan_id.clone());
        }

        tx.execute(
            "update milestones set plan_ids = ?1, updated_at = ?2 where id = ?3",
            params![
                serde_json::to_string(&plan_ids)?,
                Utc::now().to_rfc3339(),
                milestone_id.to_string(),
            ],
        )?;
        tx.execute(
            "update plans set milestone_id = ?1 where id = ?2",
            params![milestone_id.to_string(), plan_id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn milestone_plans(&self, milestone_id: &MilestoneId) -> Result<Vec<Plan>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, milestone_id, task, created_at, status, summary, approach,
                    steps, files_to_modify, risks
             from plans where milestone_id = ?1 order by created_at asc",
        )?;
        let rows = stmt.query_map(params![milestone_id.to_string()], plan_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn update_plan_status(&self, plan_id: &PlanId, status: &PlanStatus) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "update plans set status = ?1 where id = ?2",
                params![serde_json::to_string(status)?, plan_id.to_string()],
            )?;
        Ok(())
    }

    pub fn load_plan(&self, plan_id: &PlanId) -> Result<Option<Plan>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, milestone_id, task, created_at, status, summary, approach, steps, files_to_modify, risks
             from plans where id = ?1",
        )?;
        let mut rows = stmt.query_map(params![plan_id.to_string()], plan_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_plans(&self, session_id: &SessionId) -> Result<Vec<Plan>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, milestone_id, task, created_at, status, summary, approach, steps, files_to_modify, risks
             from plans where session_id = ?1 order by created_at desc",
        )?;
        let rows = stmt.query_map(params![session_id.to_string()], plan_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn all_plans(&self, limit: usize) -> Result<Vec<Plan>> {
        let limit = i64::try_from(limit)?;
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, session_id, milestone_id, task, created_at, status, summary, approach, steps, files_to_modify, risks
             from plans order by created_at desc limit ?1",
        )?;
        let rows = stmt.query_map(params![limit], plan_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    /// Update the execution status of a specific step within a plan
    ///
    /// This method allows tracking the progress of individual steps during plan execution.
    /// It serializes the entire steps array with the updated step back to the database.
    ///
    pub fn update_plan_step(
        &self,
        plan_id: &PlanId,
        step_index: usize,
        step: &PlanStep,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Load current plan inside the same lock to prevent TOCTOU
        let steps_json: String = conn
            .query_row(
                "select steps from plans where id = ?1",
                params![plan_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| anyhow::anyhow!("plan not found: {e}"))?;

        let mut steps: Vec<PlanStep> =
            serde_json::from_str(&steps_json).context("failed to deserialize plan steps")?;

        if step_index >= steps.len() {
            anyhow::bail!(
                "step index {} out of bounds (plan has {} steps)",
                step_index,
                steps.len()
            );
        }

        steps[step_index] = step.clone();

        let updated_json =
            serde_json::to_string(&steps).context("failed to serialize plan steps")?;

        conn.execute(
            "update plans set steps = ?1 where id = ?2",
            params![updated_json, plan_id.to_string()],
        )?;

        Ok(())
    }
}
