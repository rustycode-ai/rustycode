// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Event types for the `RustyCode` event bus
//!
//! This module defines the core event types used throughout `RustyCode`.

use crate::Event;
use chrono::{DateTime, Utc};
use rustycode_protocol::{ContextPlan, MilestoneId, MilestoneStatus, Plan, PlanId, SessionId};
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

/// Event emitted when a new session is started
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionStartedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Task description
    pub task: String,

    /// Additional detail
    pub detail: String,
}

impl SessionStartedEvent {
    /// Create a new `SessionStartedEvent`
    ///
    pub fn new(session_id: SessionId, task: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            task,
            detail,
        }
    }
}

impl Event for SessionStartedEvent {
    fn event_type(&self) -> &'static str {
        "session.started"
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

/// Event emitted when context is assembled for a session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextAssembledEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub context_plan: ContextPlan,

    /// Additional detail
    pub detail: String,
}

impl ContextAssembledEvent {
    /// Create a new `ContextAssembledEvent`
    ///
    pub fn new(session_id: SessionId, context_plan: ContextPlan, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            context_plan,
            detail,
        }
    }
}

impl Event for ContextAssembledEvent {
    fn event_type(&self) -> &'static str {
        "context.assembled"
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

/// Event emitted when a tool is executed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolExecutedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub tool_name: String,

    /// Tool arguments
    pub arguments: serde_json::Value,

    /// Success flag
    pub success: bool,

    /// Output text
    pub output: String,

    /// Error message (if any)
    pub error: Option<String>,
}

impl ToolExecutedEvent {
    /// Create a new `ToolExecutedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        tool_name: String,
        arguments: serde_json::Value,
        success: bool,
        output: String,
        error: Option<String>,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            tool_name,
            arguments,
            success,
            output,
            error,
        }
    }
}

impl Event for ToolExecutedEvent {
    fn event_type(&self) -> &'static str {
        "tool.executed"
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

// Pre-compact event emitted for a session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreCompactEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub session_id: SessionId,
    // Path where the compaction snapshot was persisted
    pub snapshot_path: String,
    // Optional detail message
    pub detail: String,
}

impl PreCompactEvent {
    pub fn new(session_id: SessionId, snapshot_path: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            snapshot_path,
            detail,
        }
    }
}

impl Event for PreCompactEvent {
    fn event_type(&self) -> &'static str {
        "compaction.pre"
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

// Post-compact event emitted after compaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PostCompactEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub session_id: SessionId,
    // Indicates whether the restore succeeded
    pub restored: bool,
    pub detail: String,
}

impl PostCompactEvent {
    pub fn new(session_id: SessionId, restored: bool, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            restored,
            detail,
        }
    }
}

impl Event for PostCompactEvent {
    fn event_type(&self) -> &'static str {
        "compaction.post"
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

/// Event emitted when a plan is created
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanCreatedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Plan details
    pub plan: Plan,

    /// Additional detail
    pub detail: String,
}

impl PlanCreatedEvent {
    pub fn new(session_id: SessionId, plan: Plan, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan,
            detail,
        }
    }
}

impl Event for PlanCreatedEvent {
    fn event_type(&self) -> &'static str {
        "plan.created"
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

/// Event emitted when a plan is approved
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanApprovedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Additional detail
    pub detail: String,
}

impl PlanApprovedEvent {
    pub fn new(session_id: SessionId, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            detail,
        }
    }
}

impl Event for PlanApprovedEvent {
    fn event_type(&self) -> &'static str {
        "plan.approved"
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

/// Event emitted when a plan is rejected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanRejectedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Additional detail
    pub detail: String,
}

impl PlanRejectedEvent {
    pub fn new(session_id: SessionId, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            detail,
        }
    }
}

impl Event for PlanRejectedEvent {
    fn event_type(&self) -> &'static str {
        "plan.rejected"
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

/// Event emitted when a session is completed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCompletedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Task that was completed
    pub task: String,

    /// Final session status
    pub status: String,

    /// Additional detail
    pub detail: String,
}

impl SessionCompletedEvent {
    /// Create a new `SessionCompletedEvent`
    ///
    pub fn new(session_id: SessionId, task: String, status: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            task,
            status,
            detail,
        }
    }
}

impl Event for SessionCompletedEvent {
    fn event_type(&self) -> &'static str {
        "session.completed"
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

/// Event emitted when a session fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionFailedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Task that failed
    pub task: String,

    /// Error message
    pub error: String,

    /// Additional detail
    pub detail: String,
}

impl SessionFailedEvent {
    /// Create a new `SessionFailedEvent`
    ///
    pub fn new(session_id: SessionId, task: String, error: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            task,
            error,
            detail,
        }
    }
}

impl Event for SessionFailedEvent {
    fn event_type(&self) -> &'static str {
        "session.failed"
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

/// Event emitted when session mode changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModeChangedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Previous mode
    pub old_mode: String,

    pub new_mode: String,

    /// Additional detail
    pub detail: String,
}

impl ModeChangedEvent {
    /// Create a new `ModeChangedEvent`
    ///
    pub fn new(session_id: SessionId, old_mode: String, new_mode: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            old_mode,
            new_mode,
            detail,
        }
    }
}

impl Event for ModeChangedEvent {
    fn event_type(&self) -> &'static str {
        "mode.changed"
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

/// Event emitted when plan execution starts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionStartedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub plan_id: PlanId,

    /// Number of steps to execute
    pub step_count: usize,

    /// Additional detail
    pub detail: String,
}

impl PlanExecutionStartedEvent {
    /// Create a new `PlanExecutionStartedEvent`
    ///
    pub fn new(session_id: SessionId, plan_id: PlanId, step_count: usize, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan_id,
            step_count,
            detail,
        }
    }
}

impl Event for PlanExecutionStartedEvent {
    fn event_type(&self) -> &'static str {
        "plan.execution.started"
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

/// Event emitted when plan execution completes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionCompletedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub plan_id: PlanId,

    /// Number of steps executed
    pub steps_executed: usize,

    /// Number of steps that succeeded
    pub steps_succeeded: usize,

    /// Number of steps that failed
    pub steps_failed: usize,

    /// Additional detail
    pub detail: String,
}

impl PlanExecutionCompletedEvent {
    /// Create a new `PlanExecutionCompletedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        plan_id: PlanId,
        steps_executed: usize,
        steps_succeeded: usize,
        steps_failed: usize,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan_id,
            steps_executed,
            steps_succeeded,
            steps_failed,
            detail,
        }
    }
}

impl Event for PlanExecutionCompletedEvent {
    fn event_type(&self) -> &'static str {
        "plan.execution.completed"
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

/// Event emitted when plan execution fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionFailedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub plan_id: PlanId,

    /// Error message
    pub error: String,

    /// Step index where failure occurred
    pub failed_at_step: Option<usize>,

    /// Additional detail
    pub detail: String,
}

impl PlanExecutionFailedEvent {
    /// Create a new `PlanExecutionFailedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        plan_id: PlanId,
        error: String,
        failed_at_step: Option<usize>,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan_id,
            error,
            failed_at_step,
            detail,
        }
    }
}

impl Event for PlanExecutionFailedEvent {
    fn event_type(&self) -> &'static str {
        "plan.execution.failed"
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

/// Event emitted when a milestone changes execution progress.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneProgressEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub milestone_id: MilestoneId,

    pub milestone_title: String,

    pub status: MilestoneStatus,

    pub plans_total: usize,

    pub plans_completed: usize,

    pub current_plan_summary: String,

    pub action_hint: String,
}

impl MilestoneProgressEvent {
    /// Create a new `MilestoneProgressEvent`.
    pub fn new(
        session_id: SessionId,
        milestone_id: MilestoneId,
        milestone_title: String,
        status: MilestoneStatus,
        plans_total: usize,
        plans_completed: usize,
        current_plan_summary: String,
        action_hint: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            milestone_id,
            milestone_title,
            status,
            plans_total,
            plans_completed,
            current_plan_summary,
            action_hint,
        }
    }
}

impl Event for MilestoneProgressEvent {
    fn event_type(&self) -> &'static str {
        "milestone.progress"
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

/// Event emitted when a tool is blocked due to planning mode restrictions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolBlockedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Tool name that was blocked
    pub tool_name: String,

    /// Tool arguments
    pub arguments: serde_json::Value,

    /// Reason for blocking
    pub reason: String,

    /// Additional detail
    pub detail: String,
}

impl ToolBlockedEvent {
    /// Create a new `ToolBlockedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        tool_name: String,
        arguments: serde_json::Value,
        reason: String,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            tool_name,
            arguments,
            reason,
            detail,
        }
    }
}

impl Event for ToolBlockedEvent {
    fn event_type(&self) -> &'static str {
        "tool.blocked"
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

/// Event emitted when a doctor inspection is completed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InspectionCompletedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    /// Working directory that was inspected
    pub working_dir: String,

    pub git_status: String,

    /// Number of LSP servers found
    pub lsp_server_count: usize,

    /// Number of memory entries
    pub memory_entry_count: usize,

    /// Number of skills discovered
    pub skill_count: usize,

    /// Additional detail
    pub detail: String,
}

impl InspectionCompletedEvent {
    /// Create a new `InspectionCompletedEvent`
    ///
    pub fn new(
        working_dir: String,
        git_status: String,
        lsp_server_count: usize,
        memory_entry_count: usize,
        skill_count: usize,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            working_dir,
            git_status,
            lsp_server_count,
            memory_entry_count,
            skill_count,
            detail,
        }
    }
}

impl Event for InspectionCompletedEvent {
    fn event_type(&self) -> &'static str {
        "inspection.completed"
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillActivatedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub trigger: String,
    pub activated_at: DateTime<Utc>,
}

impl SkillActivatedEvent {
    pub fn new(skill_name: String, trigger: String) -> Self {
        let now = Utc::now();
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: now,
            activated_at: now,
            skill_name,
            trigger,
        }
    }
}

impl Event for SkillActivatedEvent {
    fn event_type(&self) -> &'static str {
        "skill.activated"
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillDeactivatedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub reason: String,
    pub duration_secs: f64,
    pub tokens_used: u64,
}

impl SkillDeactivatedEvent {
    pub fn new(skill_name: String, reason: String, duration_secs: f64, tokens_used: u64) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            skill_name,
            reason,
            duration_secs,
            tokens_used,
        }
    }
}

impl Event for SkillDeactivatedEvent {
    fn event_type(&self) -> &'static str {
        "skill.deactivated"
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillSuggestedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub reason: String,
    pub score: f64,
    pub unmatched_signals: Vec<String>,
}

impl SkillSuggestedEvent {
    pub fn new(
        skill_name: String,
        reason: String,
        score: f64,
        unmatched_signals: Vec<String>,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            skill_name,
            reason,
            score,
            unmatched_signals,
        }
    }
}

impl Event for SkillSuggestedEvent {
    fn event_type(&self) -> &'static str {
        "skill.suggested"
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillQualityAssessedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub grade: String,
    pub telemetry_score: f64,
    pub graph_score: f64,
    pub intake_score: f64,
    pub routing_score: f64,
}

impl SkillQualityAssessedEvent {
    pub fn new(
        skill_name: String,
        grade: String,
        telemetry_score: f64,
        graph_score: f64,
        intake_score: f64,
        routing_score: f64,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            skill_name,
            grade,
            telemetry_score,
            graph_score,
            intake_score,
            routing_score,
        }
    }
}

impl Event for SkillQualityAssessedEvent {
    fn event_type(&self) -> &'static str {
        "skill.quality.assessed"
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

    #[test]
    fn test_session_started_event() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        assert_eq!(event.event_type(), "session.started");
        assert!(event.event_id.is_some());
        assert!(event.timestamp <= Utc::now());
    }

    #[test]
    fn test_context_assembled_event() {
        let context_plan = ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![],
        };

        let event =
            ContextAssembledEvent::new(SessionId::new(), context_plan, "context ready".to_string());

        assert_eq!(event.event_type(), "context.assembled");
        assert!(event.event_id.is_some());
        assert!(event.timestamp <= Utc::now());
    }

    #[test]
    fn test_tool_executed_event() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "read_file".to_string(),
            serde_json::json!({ "path": "/test" }),
            true,
            "success".to_string(),
            None,
        );

        assert_eq!(event.event_type(), "tool.executed");
        assert!(event.event_id.is_some());
        assert!(event.timestamp <= Utc::now());
        assert!(event.success);
        assert!(event.error.is_none());
    }

    #[test]
    fn test_event_serialization() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        let serialized = Event::serialize(&event);
        assert!(serialized.is_object());
        assert!(serialized.get("timestamp").is_some());
        assert!(serialized.get("task").is_some());
    }

    #[test]
    fn test_event_downcast() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        let as_any: &dyn Any = event.as_any();
        assert!(as_any.is::<SessionStartedEvent>());

        let downcasted = as_any.downcast_ref::<SessionStartedEvent>();
        assert!(downcasted.is_some());
        assert_eq!(downcasted.unwrap().task, "test task");
    }

    #[test]
    fn test_event_clone_box() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "session.started");

        let as_any: &dyn Any = boxed.as_any();
        assert!(as_any.is::<SessionStartedEvent>());
    }

    #[test]
    fn test_session_completed_event() {
        let event = SessionCompletedEvent::new(
            SessionId::new(),
            "Analyze codebase".into(),
            "completed".into(),
            "Done".into(),
        );
        assert_eq!(event.event_type(), "session.completed");
        assert_eq!(event.status, "completed");
        assert!(event.event_id.is_some());
    }

    #[test]
    fn test_session_failed_event() {
        let event = SessionFailedEvent::new(
            SessionId::new(),
            "Build project".into(),
            "Network error".into(),
            "Connection refused".into(),
        );
        assert_eq!(event.event_type(), "session.failed");
        assert_eq!(event.error, "Network error");
    }

    #[test]
    fn test_mode_changed_event() {
        let event = ModeChangedEvent::new(
            SessionId::new(),
            "chat".into(),
            "planning".into(),
            "User requested planning".into(),
        );
        assert_eq!(event.event_type(), "mode.changed");
        assert_eq!(event.old_mode, "chat");
        assert_eq!(event.new_mode, "planning");
    }

    #[test]
    fn test_tool_blocked_event() {
        let event = ToolBlockedEvent::new(
            SessionId::new(),
            "write_file".into(),
            serde_json::json!({"path": "test.txt"}),
            "Planning mode".into(),
            "Not permitted".into(),
        );
        assert_eq!(event.event_type(), "tool.blocked");
        assert_eq!(event.tool_name, "write_file");
        assert_eq!(event.reason, "Planning mode");
    }

    #[test]
    fn test_tool_executed_with_error() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "bash".into(),
            serde_json::json!({"command": "false"}),
            false,
            String::new(),
            Some("exit code 1".into()),
        );
        assert!(!event.success);
        assert_eq!(event.error.as_deref(), Some("exit code 1"));
    }

    #[test]
    fn test_plan_approved_event() {
        let event = PlanApprovedEvent::new(SessionId::new(), "User approved".into());
        assert_eq!(event.event_type(), "plan.approved");
    }

    #[test]
    fn test_plan_rejected_event() {
        let event = PlanRejectedEvent::new(SessionId::new(), "User rejected".into());
        assert_eq!(event.event_type(), "plan.rejected");
    }

    #[test]
    fn test_plan_execution_started_event() {
        let event = PlanExecutionStartedEvent::new(
            SessionId::new(),
            PlanId::new(),
            5,
            "Starting execution".into(),
        );
        assert_eq!(event.event_type(), "plan.execution.started");
        assert_eq!(event.step_count, 5);
    }

    #[test]
    fn test_plan_execution_completed_event() {
        let event = PlanExecutionCompletedEvent::new(
            SessionId::new(),
            PlanId::new(),
            5,
            4,
            1,
            "Done with errors".into(),
        );
        assert_eq!(event.event_type(), "plan.execution.completed");
        assert_eq!(event.steps_executed, 5);
        assert_eq!(event.steps_succeeded, 4);
        assert_eq!(event.steps_failed, 1);
    }

    #[test]
    fn test_plan_execution_failed_event() {
        let event = PlanExecutionFailedEvent::new(
            SessionId::new(),
            PlanId::new(),
            "Tool timeout".into(),
            Some(3),
            "Step 3 timed out".into(),
        );
        assert_eq!(event.event_type(), "plan.execution.failed");
        assert_eq!(event.failed_at_step, Some(3));
    }

    #[test]
    fn test_inspection_completed_event() {
        let event = InspectionCompletedEvent::new(
            "/project".into(),
            "main branch, clean".into(),
            2,
            5,
            3,
            "Done".into(),
        );
        assert_eq!(event.event_type(), "inspection.completed");
        assert_eq!(event.lsp_server_count, 2);
        assert_eq!(event.memory_entry_count, 5);
        assert_eq!(event.skill_count, 3);
    }

    #[test]
    fn test_session_started_serialization_roundtrip() {
        let event = SessionStartedEvent::new(SessionId::new(), "test".into(), "detail".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SessionStartedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.task, "test");
        assert_eq!(decoded.detail, "detail");
    }

    #[test]
    fn test_tool_executed_serialization_roundtrip() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "bash".into(),
            serde_json::json!({"cmd": "ls"}),
            true,
            "output".into(),
            None,
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ToolExecutedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.tool_name, "bash");
        assert!(decoded.success);
    }

    #[test]
    fn test_mode_changed_serialization_roundtrip() {
        let event = ModeChangedEvent::new(
            SessionId::new(),
            "chat".into(),
            "code".into(),
            "switched".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ModeChangedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.old_mode, "chat");
        assert_eq!(decoded.new_mode, "code");
    }

    #[test]
    fn test_inspection_completed_serialization() {
        let event = InspectionCompletedEvent::new(
            "/tmp".into(),
            "clean".into(),
            0,
            0,
            0,
            "no items".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("working_dir").is_some());
        assert!(json.get("lsp_server_count").is_some());
    }

    // ── Serialization roundtrip tests for all event types ─────────────

    #[test]
    fn test_session_completed_serialization_roundtrip() {
        let event = SessionCompletedEvent::new(
            SessionId::new(),
            "task".into(),
            "completed".into(),
            "done".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SessionCompletedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.task, "task");
        assert_eq!(decoded.status, "completed");
        assert_eq!(decoded.detail, "done");
    }

    #[test]
    fn test_session_failed_serialization_roundtrip() {
        let event = SessionFailedEvent::new(
            SessionId::new(),
            "task".into(),
            "err msg".into(),
            "detail".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SessionFailedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.task, "task");
        assert_eq!(decoded.error, "err msg");
    }

    #[test]
    fn test_plan_approved_serialization_roundtrip() {
        let event = PlanApprovedEvent::new(SessionId::new(), "approved".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanApprovedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.detail, "approved");
    }

    #[test]
    fn test_plan_rejected_serialization_roundtrip() {
        let event = PlanRejectedEvent::new(SessionId::new(), "rejected".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanRejectedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.detail, "rejected");
    }

    #[test]
    fn test_plan_execution_started_serialization_roundtrip() {
        let event =
            PlanExecutionStartedEvent::new(SessionId::new(), PlanId::new(), 10, "starting".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionStartedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.step_count, 10);
        assert_eq!(decoded.detail, "starting");
    }

    #[test]
    fn test_plan_execution_completed_serialization_roundtrip() {
        let event = PlanExecutionCompletedEvent::new(
            SessionId::new(),
            PlanId::new(),
            8,
            7,
            1,
            "done".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionCompletedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.steps_executed, 8);
        assert_eq!(decoded.steps_succeeded, 7);
        assert_eq!(decoded.steps_failed, 1);
    }

    #[test]
    fn test_plan_execution_failed_serialization_roundtrip() {
        let event = PlanExecutionFailedEvent::new(
            SessionId::new(),
            PlanId::new(),
            "timeout".into(),
            Some(5),
            "step 5 failed".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionFailedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.error, "timeout");
        assert_eq!(decoded.failed_at_step, Some(5));
    }

    #[test]
    fn test_plan_execution_failed_no_step_serialization_roundtrip() {
        let event = PlanExecutionFailedEvent::new(
            SessionId::new(),
            PlanId::new(),
            "unknown".into(),
            None,
            "no step info".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionFailedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.failed_at_step, None);
    }

    #[test]
    fn test_tool_blocked_serialization_roundtrip() {
        let event = ToolBlockedEvent::new(
            SessionId::new(),
            "write_file".into(),
            serde_json::json!({"path": "/tmp/test"}),
            "Planning mode".into(),
            "not allowed".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ToolBlockedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.tool_name, "write_file");
        assert_eq!(decoded.reason, "Planning mode");
    }

    #[test]
    fn test_context_assembled_serialization_roundtrip() {
        let context_plan = ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![],
        };
        let event =
            ContextAssembledEvent::new(SessionId::new(), context_plan.clone(), "ready".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ContextAssembledEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.context_plan, context_plan);
        assert_eq!(decoded.detail, "ready");
    }

    #[test]
    fn test_inspection_completed_serialization_roundtrip() {
        let event = InspectionCompletedEvent::new(
            "/project".into(),
            "dirty".into(),
            2,
            10,
            4,
            "complete".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: InspectionCompletedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.working_dir, "/project");
        assert_eq!(decoded.git_status, "dirty");
        assert_eq!(decoded.lsp_server_count, 2);
        assert_eq!(decoded.memory_entry_count, 10);
        assert_eq!(decoded.skill_count, 4);
    }

    // ── event_id None handling and serde edge cases ─────────────

    #[test]
    fn test_session_started_event_id_is_some() {
        let event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        assert!(event.event_id.is_some());
        // Verify it roundtrips with Some uuid
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("event_id").unwrap().is_string());
    }

    #[test]
    fn test_session_started_event_id_none_serialization() {
        // Construct event with event_id = None manually
        let mut event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        event.event_id = None;
        let json = serde_json::to_value(&event).unwrap();
        // skip_serializing_if should omit event_id
        assert!(json.get("event_id").is_none());
        // Should still deserialize correctly
        let decoded: SessionStartedEvent = serde_json::from_value(json).unwrap();
        assert!(decoded.event_id.is_none());
    }

    #[test]
    fn test_tool_executed_with_error_serialization_roundtrip() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "bash".into(),
            serde_json::json!({"cmd": "rm -rf /"}),
            false,
            "failed output".into(),
            Some("permission denied".into()),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ToolExecutedEvent = serde_json::from_value(json).unwrap();
        assert!(!decoded.success);
        assert_eq!(decoded.error, Some("permission denied".to_string()));
    }

    #[test]
    fn test_tool_executed_no_error_serialization() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "read".into(),
            serde_json::json!(null),
            true,
            "ok".into(),
            None,
        );
        let json = serde_json::to_value(&event).unwrap();
        // error field should be null (not omitted)
        assert!(json.get("error").is_some());
        let decoded: ToolExecutedEvent = serde_json::from_value(json).unwrap();
        assert!(decoded.error.is_none());
    }

    #[test]
    fn test_event_serialize_produces_valid_json() {
        let events: Vec<Box<dyn Event>> = vec![
            Box::new(SessionStartedEvent::new(
                SessionId::new(),
                "t".into(),
                "d".into(),
            )),
            Box::new(SessionCompletedEvent::new(
                SessionId::new(),
                "t".into(),
                "ok".into(),
                "d".into(),
            )),
            Box::new(SessionFailedEvent::new(
                SessionId::new(),
                "t".into(),
                "err".into(),
                "d".into(),
            )),
            Box::new(ModeChangedEvent::new(
                SessionId::new(),
                "a".into(),
                "b".into(),
                "d".into(),
            )),
            Box::new(ToolExecutedEvent::new(
                SessionId::new(),
                "tool".into(),
                serde_json::json!({}),
                true,
                "out".into(),
                None,
            )),
            Box::new(ToolBlockedEvent::new(
                SessionId::new(),
                "tool".into(),
                serde_json::json!({}),
                "reason".into(),
                "d".into(),
            )),
            Box::new(PlanApprovedEvent::new(SessionId::new(), "d".into())),
            Box::new(PlanRejectedEvent::new(SessionId::new(), "d".into())),
            Box::new(InspectionCompletedEvent::new(
                "/tmp".into(),
                "clean".into(),
                0,
                0,
                0,
                "d".into(),
            )),
            Box::new(SkillActivatedEvent::new("test".into(), "auto".into())),
            Box::new(SkillDeactivatedEvent::new(
                "test".into(),
                "done".into(),
                10.0,
                100,
            )),
            Box::new(SkillSuggestedEvent::new(
                "test".into(),
                "match".into(),
                0.8,
                vec![],
            )),
            Box::new(SkillQualityAssessedEvent::new(
                "test".into(),
                "good".into(),
                0.9,
                0.7,
                0.8,
                0.6,
            )),
        ];

        for event in &events {
            let serialized = event.serialize();
            assert!(
                !serialized.is_null(),
                "serialize() returned Null for {}",
                event.event_type()
            );
            assert!(
                serialized.get("timestamp").is_some(),
                "missing timestamp for {}",
                event.event_type()
            );
        }
    }

    #[test]
    fn test_session_started_equality() {
        let sid = SessionId::new();
        let e1 = SessionStartedEvent::new(sid, "task".into(), "detail".into());
        let mut e2 = e1.clone();
        // Force same event_id for equality check
        e2.event_id = e1.event_id;
        e2.timestamp = e1.timestamp;
        assert_eq!(e1, e2);
    }

    #[test]
    fn test_session_started_inequality() {
        let e1 = SessionStartedEvent::new(SessionId::new(), "task1".into(), "detail".into());
        let e2 = SessionStartedEvent::new(SessionId::new(), "task2".into(), "detail".into());
        assert_ne!(e1, e2);
    }

    #[test]
    fn test_skill_activated_event() {
        let event = SkillActivatedEvent::new("code-review".to_string(), "path-match".to_string());
        assert_eq!(event.event_type(), "skill.activated");
        assert!(event.event_id.is_some());
        assert_eq!(event.skill_name, "code-review");
        assert_eq!(event.trigger, "path-match");
    }

    #[test]
    fn test_skill_deactivated_event() {
        let event = SkillDeactivatedEvent::new(
            "code-review".to_string(),
            "session-ended".to_string(),
            120.5,
            5000,
        );
        assert_eq!(event.event_type(), "skill.deactivated");
        assert_eq!(event.duration_secs, 120.5);
        assert_eq!(event.tokens_used, 5000);
    }

    #[test]
    fn test_skill_suggested_event() {
        let event = SkillSuggestedEvent::new(
            "tdd".to_string(),
            "file-pattern-match".to_string(),
            0.85,
            vec!["test-file".to_string()],
        );
        assert_eq!(event.event_type(), "skill.suggested");
        assert_eq!(event.score, 0.85);
        assert_eq!(event.unmatched_signals.len(), 1);
    }

    #[test]
    fn test_skill_quality_assessed_event() {
        let event = SkillQualityAssessedEvent::new(
            "code-review".to_string(),
            "good".to_string(),
            0.9,
            0.7,
            0.8,
            0.6,
        );
        assert_eq!(event.event_type(), "skill.quality.assessed");
        assert_eq!(event.telemetry_score, 0.9);
    }

    #[test]
    fn test_skill_activated_serialization_roundtrip() {
        let event = SkillActivatedEvent::new("test-skill".to_string(), "manual".to_string());
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["skill_name"], "test-skill");
        assert_eq!(json["trigger"], "manual");
        let decoded: SkillActivatedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.skill_name, "test-skill");
    }

    #[test]
    fn test_skill_deactivated_serialization_roundtrip() {
        let event =
            SkillDeactivatedEvent::new("test-skill".to_string(), "timeout".to_string(), 30.0, 1000);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["duration_secs"], 30.0);
        let decoded: SkillDeactivatedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.tokens_used, 1000);
    }

    #[test]
    fn test_skill_suggested_serialization_roundtrip() {
        let event = SkillSuggestedEvent::new(
            "tdd".to_string(),
            "context-match".to_string(),
            0.9,
            vec!["a".to_string(), "b".to_string()],
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SkillSuggestedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.unmatched_signals, vec!["a", "b"]);
    }

    #[test]
    fn test_skill_quality_assessed_serialization_roundtrip() {
        let event = SkillQualityAssessedEvent::new(
            "review".to_string(),
            "excellent".to_string(),
            0.95,
            0.88,
            0.92,
            0.85,
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SkillQualityAssessedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.grade, "excellent");
    }

    // ── PreCompactEvent tests ─────────────

    #[test]
    fn test_pre_compact_event() {
        let event = PreCompactEvent::new(
            SessionId::new(),
            "/snapshots/snap-1.json".to_string(),
            "Pre-compact snapshot".to_string(),
        );
        assert_eq!(event.event_type(), "compaction.pre");
        assert_eq!(event.snapshot_path, "/snapshots/snap-1.json");
        assert!(event.event_id.is_some());
    }

    #[test]
    fn test_pre_compact_event_serde_roundtrip() {
        let event = PreCompactEvent::new(
            SessionId::new(),
            "/tmp/snap.db".to_string(),
            "saving state".to_string(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PreCompactEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.snapshot_path, "/tmp/snap.db");
        assert_eq!(decoded.detail, "saving state");
    }

    #[test]
    fn test_pre_compact_event_clone_box() {
        let event =
            PreCompactEvent::new(SessionId::new(), "/path".to_string(), "detail".to_string());
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "compaction.pre");
    }

    #[test]
    fn test_pre_compact_event_as_any_downcast() {
        let event =
            PreCompactEvent::new(SessionId::new(), "/path".to_string(), "detail".to_string());
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<PreCompactEvent>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().snapshot_path, "/path");
    }

    // ── PostCompactEvent tests ─────────────

    #[test]
    fn test_post_compact_event_restored() {
        let event =
            PostCompactEvent::new(SessionId::new(), true, "Restored from snapshot".to_string());
        assert_eq!(event.event_type(), "compaction.post");
        assert!(event.restored);
    }

    #[test]
    fn test_post_compact_event_not_restored() {
        let event = PostCompactEvent::new(SessionId::new(), false, "Restore failed".to_string());
        assert!(!event.restored);
    }

    #[test]
    fn test_post_compact_event_serde_roundtrip() {
        let event = PostCompactEvent::new(SessionId::new(), true, "ok".to_string());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PostCompactEvent = serde_json::from_value(json).unwrap();
        assert!(decoded.restored);
        assert_eq!(decoded.detail, "ok");
    }

    #[test]
    fn test_post_compact_event_clone_box() {
        let event = PostCompactEvent::new(SessionId::new(), false, "detail".to_string());
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "compaction.post");
    }

    #[test]
    fn test_post_compact_event_as_any_downcast() {
        let event = PostCompactEvent::new(SessionId::new(), true, "detail".to_string());
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<PostCompactEvent>();
        assert!(downcast.is_some());
        assert!(downcast.unwrap().restored);
    }

    // ── PlanCreatedEvent tests ─────────────

    fn make_plan() -> Plan {
        Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "test".to_string(),
            created_at: Utc::now(),
            status: rustycode_protocol::PlanStatus::Draft,
            summary: "test plan".to_string(),
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
        }
    }

    #[test]
    fn test_plan_created_event() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "New plan created".to_string());
        assert_eq!(event.event_type(), "plan.created");
        assert!(event.event_id.is_some());
        assert_eq!(event.detail, "New plan created");
    }

    #[test]
    fn test_plan_created_event_clone_box() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "d".to_string());
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "plan.created");
    }

    #[test]
    fn test_plan_created_event_as_any_downcast() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "d".to_string());
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<PlanCreatedEvent>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().detail, "d");
    }

    #[test]
    fn test_plan_created_event_serialize_valid_json() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "created".to_string());
        let val = Event::serialize(&event);
        assert!(!val.is_null());
        assert!(val.get("timestamp").is_some());
        assert_eq!(val["detail"], "created");
    }

    // ── Event trait object tests for harness and ensemble events via bus ─────

    #[test]
    fn test_pre_compact_serialization_in_trait_object_list() {
        let event =
            PreCompactEvent::new(SessionId::new(), "/path".to_string(), "detail".to_string());
        let serialized = Event::serialize(&event);
        assert!(!serialized.is_null());
        assert!(serialized.get("timestamp").is_some());
        assert_eq!(serialized["snapshot_path"], "/path");
    }

    #[test]
    fn test_post_compact_serialization_in_trait_object_list() {
        let event = PostCompactEvent::new(SessionId::new(), true, "detail".to_string());
        let serialized = Event::serialize(&event);
        assert!(!serialized.is_null());
        assert!(serialized.get("restored").is_some());
    }
}
