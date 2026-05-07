//! Row mapper functions for converting database rows to domain types.
//!
//! These functions are shared across multiple store modules and are used
//! to map `rusqlite::Row` values to their corresponding protocol types.

use chrono::{DateTime, Utc};
use rustycode_protocol::{
    Milestone, MilestoneId, Plan, PlanDependency, PlanId, PlanStep, Session, SessionId,
    ToolApprovalMode,
};

/// Map a database row to a `Session`.
pub(crate) fn session_from_row(row: &rusqlite::Row) -> rusqlite::Result<Session> {
    let id: String = row.get(0)?;
    let task: String = row.get(1)?;
    let created_at: String = row.get(2)?;
    let mode_str: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let plan_path: Option<String> = row.get(5)?;
    Ok(Session {
        id: SessionId::parse(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        task,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc),
        mode: serde_json::from_str(&mode_str).map_err(|e| {
            tracing::warn!("Failed to deserialize session mode '{}': {}", mode_str, e);
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
        status: serde_json::from_str(&status_str).map_err(|e| {
            tracing::warn!(
                "Failed to deserialize session status '{}': {}",
                status_str,
                e
            );
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        plan_path,
        tool_approval_mode: ToolApprovalMode::default(),
        execution_trace: None,
    })
}

/// Map a database row to a `Milestone`.
pub(crate) fn milestone_from_row(row: &rusqlite::Row) -> rusqlite::Result<Milestone> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let title: String = row.get(2)?;
    let description: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let plan_ids_str: String = row.get(5)?;
    let dependencies_str: String = row.get(6)?;
    let success_criteria_str: String = row.get(7)?;
    let validation_command: Option<String> = row.get(8)?;
    let created_at: String = row.get(9)?;
    let updated_at: String = row.get(10)?;
    let completed_at: Option<String> = row.get(11)?;

    let to_sql_err = |e: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    };

    Ok(Milestone {
        id: MilestoneId::parse(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        session_id: SessionId::parse(&session_id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?,
        title,
        description,
        status: serde_json::from_str(&status_str).unwrap_or_default(),
        plan_ids: serde_json::from_str::<Vec<PlanId>>(&plan_ids_str).map_err(to_sql_err)?,
        plan_dependencies: serde_json::from_str::<Vec<PlanDependency>>(&dependencies_str)
            .map_err(to_sql_err)?,
        success_criteria: serde_json::from_str::<Vec<String>>(&success_criteria_str)
            .map_err(to_sql_err)?,
        validation_command,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc),
        completed_at: completed_at
            .as_deref()
            .map(|ts| {
                DateTime::parse_from_rfc3339(ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            11,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })
            })
            .transpose()?,
    })
}

/// Map a database row to a `Plan`.
pub(crate) fn plan_from_row(row: &rusqlite::Row) -> rusqlite::Result<Plan> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let milestone_id: Option<String> = row.get(2)?;
    let task: String = row.get(3)?;
    let created_at: String = row.get(4)?;
    let status_str: String = row.get(5)?;
    let summary: String = row.get(6)?;
    let approach: String = row.get(7)?;
    let steps_str: String = row.get(8)?;
    let files_str: String = row.get(9)?;
    let risks_str: String = row.get(10)?;

    let to_sql_err = |e: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    };

    Ok(Plan {
        id: PlanId::parse(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        session_id: SessionId::parse(&session_id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?,
        milestone_id: milestone_id
            .as_deref()
            .map(|value| {
                MilestoneId::parse(value).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            })
            .transpose()?,
        task,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc),
        status: serde_json::from_str(&status_str).unwrap_or_default(),
        summary,
        approach,
        steps: serde_json::from_str::<Vec<PlanStep>>(&steps_str).map_err(to_sql_err)?,
        files_to_modify: serde_json::from_str::<Vec<String>>(&files_str).map_err(to_sql_err)?,
        risks: serde_json::from_str::<Vec<String>>(&risks_str).map_err(to_sql_err)?,
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
        task_profile: None,
    })
}

/// A search hit from conversation full-text search.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationSearchHit {
    /// Session ID where the match was found
    pub session_id: String,
    /// When this snapshot was captured
    pub captured_at: String,
    /// Text snippet around the match
    pub snippet: String,
}
