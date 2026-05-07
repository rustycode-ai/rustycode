// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! AST pipeline-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use rustycode_protocol::SessionId;
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AstPhaseEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub session_id: SessionId,
    /// e.g. "Classify", "Research", "Skeleton"
    pub phase: String,
    pub phase_index: usize,
    pub total_phases: usize,
    pub task_summary: String,
    pub phase_elapsed_ms: u64,
    pub total_elapsed_ms: u64,
    pub milestones_completed: usize,
    pub milestones_total: usize,
    pub success: bool,
}

impl AstPhaseEvent {
    pub fn new(
        session_id: SessionId,
        phase: String,
        phase_index: usize,
        total_phases: usize,
        task_summary: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            phase,
            phase_index,
            total_phases,
            task_summary,
            phase_elapsed_ms: 0,
            total_elapsed_ms: 0,
            milestones_completed: 0,
            milestones_total: 0,
            success: false,
        }
    }
}

impl Event for AstPhaseEvent {
    fn event_type(&self) -> &'static str {
        "ast.phase"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
