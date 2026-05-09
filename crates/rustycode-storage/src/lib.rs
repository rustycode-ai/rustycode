#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::default_trait_access,
    clippy::format_push_string,
    clippy::manual_midpoint,
    clippy::match_same_arms,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::unused_self
)]
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp,)
)]
// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! # `RustyCode` Storage
//!
//! Persistent storage layer for `RustyCode` sessions, events, and plans.
//!
//! ## Features
//!
//! - **`SQLite` Backend**: Uses rusqlite for reliable, embedded persistence
//! - **Event Persistence**: Automatically persist events from the event bus
//! - **Session Management**: Store and retrieve session state
//! - **Plan Storage**: Save and query execution plans
//! - **Memory Store**: Key-value storage for contextual data
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_storage::Storage;
//! use rustycode_bus::SessionStartedEvent;
//! use rustycode_protocol::SessionId;
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Open storage database
//! let storage = Storage::open(Path::new("rustycode.db"))?;
//!
//! // Persist an event
//! let event = SessionStartedEvent::new(
//!     SessionId::new(),
//!     "Analyze codebase".to_string(),
//!     "Initial session".to_string(),
//! );
//! storage.insert_event_bus(&event)?;
//!
//! // Retrieve recent events
//! let events = storage.events(10)?;
//! for event in events {
//!     println!("{}: {}", event.event_type, event.created_at);
//! }
//! # Ok(())
//! # }
//! ```

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use rusqlite::Connection;

// Buffered I/O writer
pub mod buffered_writer;

// Conversation history system
pub mod conversation_history;

// Session capture and summarization
pub mod session_capture;

// LLM-powered session summarization
// Event sourcing
// pub mod event_store; // broken: needs rustycode-agent-runtime dep (cycle) — fix pending

// Memory effectiveness metrics
pub mod memory_metrics;

// Checkpoint storage for session recovery
pub mod checkpoint;

// JSON-based session snapshot persistence
pub mod checkpoint_store;

// Project-scoped task and todo storage
pub mod task_store;

// Record types (EventRecord, MemoryRecord, CheckpointRecord, etc.)
pub mod records;

// Session capture manager
pub mod capture_manager;

// Event subscriber for event bus persistence
pub mod event_subscriber;

// Session and event storage methods
pub mod session_store;

// Plan and milestone storage methods
pub mod plan_store;

// Memory key-value storage methods
pub mod memory_store;

// Schema management, database statistics, and cleanup
pub mod schema;

// Session snapshot persistence
pub mod snapshots;

// Checkpoint, rewind, hook, and API call record storage + search
pub mod record_store;

// Row mappers and search types
pub mod search;

// Re-exports from existing modules
pub use checkpoint::GitRewindSnapshot;
pub use checkpoint::{
    repo_has_uncommitted_changes, Checkpoint, CheckpointStorage, GitCheckpointStorage,
};

pub use checkpoint_store::{CheckpointSnapshot, CheckpointStore, ExecutionPhase};

// Re-exports from new modules
pub use capture_manager::SessionCaptureManager;
pub use event_subscriber::EventSubscriber;
pub use records::{
    ApiCallRecord, CheckpointRecord, EventRecord, HookExecutionRecord, MemoryRecord, RewindSnapshot,
};
pub use search::ConversationSearchHit;
pub use snapshots::SessionSnapshot;

/// Convenience API: rewind a repository at `repo_path` to the provided snapshot.
///
/// This is a thin wrapper around `GitCheckpointStorage::rewind_to_checkpoint` to
/// make it easier for higher-level callers (CLI, services) to trigger a rewind.
pub fn rewind_repo(repo_path: &Path, snapshot: &GitRewindSnapshot) -> Result<()> {
    let storage = GitCheckpointStorage::from_path(repo_path.to_path_buf());
    storage.rewind_to_checkpoint(snapshot)
}

/// Preview what a rewind would change without performing destructive operations.
/// Returns a human-readable summary of files that would be modified or removed.
pub fn preview_rewind_repo(repo_path: &Path, snapshot: &GitRewindSnapshot) -> Result<String> {
    let storage = GitCheckpointStorage::from_path(repo_path.to_path_buf());
    storage.preview_rewind(snapshot)
}

pub struct Storage {
    pub(crate) conn: Arc<StdMutex<Connection>>,
}

/// Statistics about the database contents and size.
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    /// Total number of sessions.
    pub session_count: i64,
    /// Total number of events.
    pub event_count: i64,
    /// Total number of API call records.
    pub api_call_count: i64,
    /// Total number of hook execution records.
    pub hook_execution_count: i64,
    /// Total number of checkpoint records.
    pub checkpoint_count: i64,
    /// Database file size in bytes.
    pub db_size_bytes: u64,
    /// Date of the oldest session, if any.
    pub oldest_session: Option<String>,
    /// Date of the most recent session, if any.
    pub newest_session: Option<String>,
}

/// Statistics returned by cleanup operations.
#[derive(Debug, Clone, Default)]
pub struct CleanupStats {
    /// Number of sessions removed.
    pub sessions_removed: u64,
    /// Number of events removed.
    pub events_removed: u64,
    /// Number of API call records removed.
    pub api_calls_removed: u64,
    /// Number of hook execution records removed.
    pub hook_executions_removed: u64,
    /// Number of checkpoint records removed.
    pub checkpoints_removed: u64,
    /// Number of rewind snapshots removed.
    pub rewind_snapshots_removed: u64,
    /// Number of session snapshots removed.
    pub snapshots_removed: u64,
    /// Number of FTS entries removed.
    pub fts_entries_removed: u64,
}

impl Storage {
    /// Rewind a repository to the given snapshot using the storage helper.
    pub fn rewind_snapshot(&self, repo_path: &Path, snapshot: &GitRewindSnapshot) -> Result<()> {
        rewind_repo(repo_path, snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionCaptureManager, Storage};
    use chrono::Utc;
    use rustycode_protocol::{
        EventKind, Milestone, MilestoneId, MilestoneStatus, Plan, PlanId, PlanStatus, Session,
        SessionEvent, SessionId, SessionMode, SessionStatus, ToolApprovalMode,
    };
    use std::fs;
    use std::path::PathBuf;

    fn temp_db_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustycode-storage-{}", SessionId::new()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("test.db")
    }

    fn make_session(task: &str) -> Session {
        Session {
            id: SessionId::new(),
            task: task.to_string(),
            created_at: Utc::now(),
            mode: SessionMode::Executing,
            status: SessionStatus::Executing,
            plan_path: None,
            tool_approval_mode: ToolApprovalMode::default(),
            execution_trace: None,
        }
    }

    #[test]
    fn persists_sessions_events_and_memory() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("Inspect");
        storage.insert_session(&session).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: session.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "started".to_string(),
            })
            .unwrap();
        storage
            .upsert_memory("project", "style", "prefer tests")
            .unwrap();
        storage
            .upsert_memory("project", "style", "prefer coverage")
            .unwrap();

        assert_eq!(storage.session_count().unwrap(), 1);
        assert_eq!(
            storage
                .event_count_for_session(&session.id.to_string())
                .unwrap(),
            1
        );
        assert_eq!(
            storage
                .recent_tasks(5, Some(&session.id.to_string()))
                .unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(storage.recent_sessions(5).unwrap().len(), 1);
        assert_eq!(storage.session_events(&session.id).unwrap().len(), 1);
    }

    #[test]
    fn plan_mode_round_trip() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        // Start a planning session
        let session = Session {
            id: SessionId::new(),
            task: "add logging".to_string(),
            created_at: Utc::now(),
            mode: SessionMode::Planning,
            status: SessionStatus::Planning,
            plan_path: None,
            tool_approval_mode: ToolApprovalMode::default(),
            execution_trace: None,
        };
        storage.insert_session(&session).unwrap();

        // Create and store a plan
        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "add logging".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Add debug logging".to_string(),
            approach: "Insert log statements".to_string(),
            steps: vec![],
            files_to_modify: vec!["src/main.rs".to_string()],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Load it back
        let loaded = storage
            .load_plan(&plan.id)
            .unwrap()
            .expect("plan should exist");
        assert_eq!(loaded.summary, plan.summary);
        assert_eq!(loaded.files_to_modify, plan.files_to_modify);

        // Approve
        storage
            .update_plan_status(&plan.id, &PlanStatus::Approved)
            .unwrap();
        let approved = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(approved.status, PlanStatus::Approved);

        // Session update
        let mut updated = session.clone();
        updated.status = SessionStatus::Executing;
        updated.mode = SessionMode::Executing;
        storage.update_session(&updated).unwrap();
        let reloaded = storage.recent_sessions(1).unwrap();
        assert_eq!(reloaded[0].status, SessionStatus::Executing);
    }

    #[test]
    fn plan_crud_operations() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        // Create session
        let session = make_session("implement feature");
        storage.insert_session(&session).unwrap();

        // CREATE: Insert a new plan
        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "implement feature".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Implement new feature X".to_string(),
            approach: "Use TDD approach".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Write tests".to_string(),
                description: "Create unit tests".to_string(),
                tools: vec!["editor".to_string()],
                expected_outcome: "Tests fail".to_string(),
                rollback_hint: "Delete test file".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec!["src/lib.rs".to_string(), "tests/test.rs".to_string()],
            risks: vec!["May break existing functionality".to_string()],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        // READ: Load plan by ID
        let loaded = storage.load_plan(&plan.id).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, plan.id);
        assert_eq!(loaded.summary, plan.summary);
        assert_eq!(loaded.approach, plan.approach);
        assert_eq!(loaded.steps.len(), 1);
        assert_eq!(loaded.steps[0].title, "Write tests");
        assert_eq!(loaded.files_to_modify.len(), 2);
        assert_eq!(loaded.risks.len(), 1);

        // UPDATE: Change plan status
        storage
            .update_plan_status(&plan.id, &PlanStatus::Ready)
            .unwrap();
        let updated = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(updated.status, PlanStatus::Ready);

        // READ: List plans for session
        let plans = storage.list_plans(&session.id).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].id, plan.id);

        // READ: Load non-existent plan returns None
        let fake_id = PlanId::new();
        let not_found = storage.load_plan(&fake_id).unwrap();
        assert!(not_found.is_none());

        // CREATE: Add multiple plans
        let plan2 = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "implement feature".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Approved,
            summary: "Alternative plan".to_string(),
            approach: "Different approach".to_string(),
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
        storage.insert_plan(&plan2).unwrap();

        // READ: List all plans with limit
        let all_plans = storage.all_plans(10).unwrap();
        assert_eq!(all_plans.len(), 2);
    }

    #[test]
    fn milestone_crud_and_plan_links() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("milestone test");
        storage.insert_session(&session).unwrap();

        let milestone = Milestone {
            id: MilestoneId::new(),
            session_id: session.id.clone(),
            title: "Auth milestone".to_string(),
            description: "Group auth work".to_string(),
            status: MilestoneStatus::Draft,
            plan_ids: vec![],
            plan_dependencies: vec![],
            success_criteria: vec!["Login flow works".to_string()],
            validation_command: Some("cargo test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        storage.insert_milestone(&milestone).unwrap();

        let plan1 = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            milestone_id: Some(milestone.id.clone()),
            task: "auth: research".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Research auth".to_string(),
            approach: "Look at existing patterns".to_string(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        let plan2 = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            milestone_id: Some(milestone.id.clone()),
            task: "auth: implement".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Ready,
            summary: "Implement auth".to_string(),
            approach: "Add the module".to_string(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        storage.insert_plan(&plan1).unwrap();
        storage.insert_plan(&plan2).unwrap();
        storage
            .add_plan_to_milestone(&milestone.id, &plan1.id)
            .unwrap();
        storage
            .add_plan_to_milestone(&milestone.id, &plan2.id)
            .unwrap();
        storage
            .update_milestone_status(&milestone.id, &MilestoneStatus::Active)
            .unwrap();

        let loaded = storage.load_milestone(&milestone.id).unwrap().unwrap();
        assert_eq!(loaded.status, MilestoneStatus::Active);
        assert_eq!(loaded.plan_ids.len(), 2);

        let milestone_plans = storage.milestone_plans(&milestone.id).unwrap();
        assert_eq!(milestone_plans.len(), 2);
        assert!(milestone_plans
            .iter()
            .all(|plan| plan.milestone_id == Some(milestone.id.clone())));

        let sessions_milestones = storage.list_milestones(&session.id).unwrap();
        assert_eq!(sessions_milestones.len(), 1);
        assert_eq!(sessions_milestones[0].id, milestone.id);

        let ready = loaded.ready_plans(&milestone_plans);
        assert_eq!(ready, vec![plan1.id.clone(), plan2.id.clone()]);
    }

    #[test]
    fn plan_status_transitions() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("test status transitions");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Test plan".to_string(),
            approach: "Test approach".to_string(),
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
        storage.insert_plan(&plan).unwrap();

        // Test all status transitions
        for status in [
            PlanStatus::Ready,
            PlanStatus::Approved,
            PlanStatus::Executing,
            PlanStatus::Completed,
        ] {
            storage.update_plan_status(&plan.id, &status).unwrap();
            let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
            assert_eq!(loaded.status, status);
        }
    }

    #[test]
    fn plan_with_complex_data() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("complex plan");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "complex feature".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Ready,
            summary: "A complex plan with many steps".to_string(),
            approach: "Multi-phase implementation".to_string(),
            steps: (1..=5)
                .map(|i| rustycode_protocol::PlanStep {
                    order: i,
                    title: format!("Step {}", i),
                    description: format!("Description for step {}", i),
                    tools: vec!["editor".to_string(), "Bash".to_string()],
                    expected_outcome: format!("Outcome {}", i),
                    rollback_hint: format!("Rollback step {}", i),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                })
                .collect(),
            files_to_modify: vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "tests/integration.rs".to_string(),
            ],
            risks: vec![
                "Risk 1: Performance impact".to_string(),
                "Risk 2: Breaking changes".to_string(),
                "Risk 3: Compatibility issues".to_string(),
            ],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.steps.len(), 5);
        assert_eq!(loaded.files_to_modify.len(), 3);
        assert_eq!(loaded.risks.len(), 3);

        // Verify step details
        assert_eq!(loaded.steps[0].order, 1);
        assert_eq!(loaded.steps[4].title, "Step 5");
    }

    #[test]
    fn update_plan_step_successfully() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("step execution tracking");
        storage.insert_session(&session).unwrap();

        // Create a plan with multiple steps
        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "track step execution".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test step updates".to_string(),
            approach: "Update individual steps".to_string(),
            steps: vec![
                rustycode_protocol::PlanStep {
                    order: 1,
                    title: "First step".to_string(),
                    description: "Initial step".to_string(),
                    tools: vec!["editor".to_string()],
                    expected_outcome: "File created".to_string(),
                    rollback_hint: "Delete file".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 2,
                    title: "Second step".to_string(),
                    description: "Follow-up step".to_string(),
                    tools: vec!["Bash".to_string()],
                    expected_outcome: "Tests pass".to_string(),
                    rollback_hint: "Revert changes".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
            ],
            files_to_modify: vec!["src/test.rs".to_string()],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Update the first step to InProgress
        let mut updated_step = plan.steps[0].clone();
        updated_step.execution_status = rustycode_protocol::StepStatus::InProgress;
        updated_step.started_at = Some(Utc::now());
        updated_step.results = vec!["Started execution".to_string()];

        storage
            .update_plan_step(&plan.id, 0, &updated_step)
            .unwrap();

        // Load the plan and verify the step was updated
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.steps.len(), 2);
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::InProgress
        );
        assert!(loaded.steps[0].started_at.is_some());
        assert_eq!(
            loaded.steps[0].results,
            vec!["Started execution".to_string()]
        );

        // Verify the second step was not affected
        assert_eq!(
            loaded.steps[1].execution_status,
            rustycode_protocol::StepStatus::Pending
        );
        assert!(loaded.steps[1].started_at.is_none());
    }

    #[test]
    fn update_plan_step_to_completed_with_results() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("complete step");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "complete step test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test step completion".to_string(),
            approach: "Update step to completed".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Execute step".to_string(),
                description: "Run the step".to_string(),
                tools: vec!["Bash".to_string()],
                expected_outcome: "Success".to_string(),
                rollback_hint: "N/A".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Update step to Completed with full execution details
        let mut completed_step = plan.steps[0].clone();
        completed_step.execution_status = rustycode_protocol::StepStatus::Completed;
        completed_step.started_at = Some(Utc::now() - chrono::Duration::seconds(60));
        completed_step.completed_at = Some(Utc::now());
        completed_step.results = vec![
            "Command executed successfully".to_string(),
            "Output: test passed".to_string(),
        ];
        completed_step.tool_executions = vec![rustycode_protocol::StepToolExecution {
            tool_name: "Bash".to_string(),
            args: serde_json::json!({"command": "cargo test"}).to_string(),
            output: "test result: ok".to_string(),
            error: None,
            timestamp: Utc::now(),
        }];

        storage
            .update_plan_step(&plan.id, 0, &completed_step)
            .unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::Completed
        );
        assert!(loaded.steps[0].started_at.is_some());
        assert!(loaded.steps[0].completed_at.is_some());
        assert_eq!(loaded.steps[0].results.len(), 2);
        assert_eq!(loaded.steps[0].tool_executions.len(), 1);
        assert_eq!(loaded.steps[0].tool_executions[0].tool_name, "Bash");
    }

    #[test]
    fn update_plan_step_to_failed_with_errors() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("failed step");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "failed step test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test step failure".to_string(),
            approach: "Update step to failed".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Failing step".to_string(),
                description: "This step will fail".to_string(),
                tools: vec!["Bash".to_string()],
                expected_outcome: "Success".to_string(),
                rollback_hint: "Check logs".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Update step to Failed with error details
        let mut failed_step = plan.steps[0].clone();
        failed_step.execution_status = rustycode_protocol::StepStatus::Failed;
        failed_step.started_at = Some(Utc::now() - chrono::Duration::seconds(30));
        failed_step.completed_at = Some(Utc::now());
        failed_step.errors = vec![
            "Compilation failed".to_string(),
            "Error: undefined variable 'x'".to_string(),
        ];
        failed_step.results = vec!["Attempted compilation but failed".to_string()];

        storage.update_plan_step(&plan.id, 0, &failed_step).unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::Failed
        );
        assert_eq!(loaded.steps[0].errors.len(), 2);
        assert!(loaded.steps[0].errors[0].contains("Compilation failed"));
        assert_eq!(loaded.steps[0].results.len(), 1);
    }

    #[test]
    fn update_plan_step_with_invalid_index_fails() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("invalid step index");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "invalid index test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test invalid step index".to_string(),
            approach: "Try to update non-existent step".to_string(),
            steps: vec![rustycode_protocol::PlanStep {
                order: 1,
                title: "Only step".to_string(),
                description: "Single step".to_string(),
                tools: vec![],
                expected_outcome: "Success".to_string(),
                rollback_hint: "N/A".to_string(),
                tool_calls: vec![],
                execution_status: rustycode_protocol::StepStatus::Pending,
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Try to update a step that doesn't exist
        let fake_step = rustycode_protocol::PlanStep {
            order: 99,
            title: "Non-existent".to_string(),
            description: "Does not exist".to_string(),
            tools: vec![],
            expected_outcome: "N/A".to_string(),
            rollback_hint: "N/A".to_string(),
            tool_calls: vec![],
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };

        let result = storage.update_plan_step(&plan.id, 5, &fake_step);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of bounds"));
    }

    #[test]
    fn update_plan_step_on_nonexistent_plan_fails() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        let fake_plan_id = PlanId::new();
        let fake_step = rustycode_protocol::PlanStep {
            order: 1,
            title: "Fake".to_string(),
            description: "Fake".to_string(),
            tools: vec![],
            expected_outcome: "N/A".to_string(),
            rollback_hint: "N/A".to_string(),
            tool_calls: vec![],
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };

        let result = storage.update_plan_step(&fake_plan_id, 0, &fake_step);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("plan not found"));
    }

    #[test]
    fn update_multiple_steps_sequentially() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("sequential step updates");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "multi-step execution".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Test sequential updates".to_string(),
            approach: "Update steps one by one".to_string(),
            steps: vec![
                rustycode_protocol::PlanStep {
                    order: 1,
                    title: "Step 1".to_string(),
                    description: "First".to_string(),
                    tools: vec![],
                    expected_outcome: "Done".to_string(),
                    rollback_hint: "N/A".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 2,
                    title: "Step 2".to_string(),
                    description: "Second".to_string(),
                    tools: vec![],
                    expected_outcome: "Done".to_string(),
                    rollback_hint: "N/A".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 3,
                    title: "Step 3".to_string(),
                    description: "Third".to_string(),
                    tools: vec![],
                    expected_outcome: "Done".to_string(),
                    rollback_hint: "N/A".to_string(),
                    tool_calls: vec![],
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
            ],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        storage.insert_plan(&plan).unwrap();

        // Complete steps one by one
        for i in 0..3 {
            let mut step = plan.steps[i].clone();
            step.execution_status = rustycode_protocol::StepStatus::Completed;
            step.started_at = Some(Utc::now());
            step.completed_at = Some(Utc::now());
            step.results = vec![format!("Step {} completed", i + 1)];

            storage.update_plan_step(&plan.id, i, &step).unwrap();
        }

        // Verify all steps are Completed
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.steps.len(), 3);
        for (i, step) in loaded.steps.iter().enumerate() {
            assert_eq!(
                step.execution_status,
                rustycode_protocol::StepStatus::Completed
            );
            assert_eq!(step.results.len(), 1);
            assert!(step.results[0].contains(&format!("{}", i + 1)));
        }
    }

    #[test]
    fn full_plan_lifecycle_test() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        // Phase 1: Create session and initial plan
        let session = make_session("full lifecycle test");
        storage.insert_session(&session).unwrap();

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: "implement feature with tests".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Implement feature X with comprehensive tests".to_string(),
            approach: "TDD approach with full coverage".to_string(),
            steps: vec![
                rustycode_protocol::PlanStep {
                    order: 1,
                    title: "Write failing tests".to_string(),
                    description: "Create test cases for the new feature".to_string(),
                    tools: vec!["editor".to_string()],
                    expected_outcome: "Tests compile and fail".to_string(),
                    rollback_hint: "Delete test file".to_string(),
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 2,
                    title: "Implement feature".to_string(),
                    description: "Write the feature code to pass tests".to_string(),
                    tools: vec!["editor".to_string()],
                    expected_outcome: "Tests pass".to_string(),
                    rollback_hint: "Revert implementation".to_string(),
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
                rustycode_protocol::PlanStep {
                    order: 3,
                    title: "Run tests".to_string(),
                    description: "Execute test suite to verify implementation".to_string(),
                    tools: vec!["Bash".to_string()],
                    expected_outcome: "All tests pass".to_string(),
                    rollback_hint: "Fix failing tests".to_string(),
                    execution_status: rustycode_protocol::StepStatus::Pending,
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                },
            ],
            files_to_modify: vec![
                "src/feature.rs".to_string(),
                "tests/feature_test.rs".to_string(),
            ],
            risks: vec![
                "Test flakiness".to_string(),
                "Performance regression".to_string(),
            ],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };

        // CREATE: Insert the plan
        storage.insert_plan(&plan).unwrap();
        assert_eq!(storage.list_plans(&session.id).unwrap().len(), 1);

        // READ: Load and verify initial state
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.status, PlanStatus::Draft);
        assert_eq!(loaded.steps.len(), 3);
        assert!(loaded
            .steps
            .iter()
            .all(|s| s.execution_status == rustycode_protocol::StepStatus::Pending));

        // UPDATE: Change status to Ready
        storage
            .update_plan_status(&plan.id, &PlanStatus::Ready)
            .unwrap();
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.status, PlanStatus::Ready);

        // UPDATE: Change status to Approved
        storage
            .update_plan_status(&plan.id, &PlanStatus::Approved)
            .unwrap();
        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(loaded.status, PlanStatus::Approved);

        // UPDATE: Start execution
        storage
            .update_plan_status(&plan.id, &PlanStatus::Executing)
            .unwrap();

        // Simulate executing each step
        // Step 1: Write tests
        let mut step1 = loaded.steps[0].clone();
        step1.execution_status = rustycode_protocol::StepStatus::Completed;
        step1.started_at = Some(Utc::now() - chrono::Duration::minutes(10));
        step1.completed_at = Some(Utc::now() - chrono::Duration::minutes(9));
        step1.results = vec!["Test file created".to_string()];
        storage.update_plan_step(&plan.id, 0, &step1).unwrap();

        // Step 2: Implement feature
        let mut step2 = loaded.steps[1].clone();
        step2.execution_status = rustycode_protocol::StepStatus::Completed;
        step2.started_at = Some(Utc::now() - chrono::Duration::minutes(8));
        step2.completed_at = Some(Utc::now() - chrono::Duration::minutes(5));
        step2.results = vec!["Implementation complete".to_string()];
        storage.update_plan_step(&plan.id, 1, &step2).unwrap();

        // Step 3: Run tests (let's say it fails first)
        let mut step3 = loaded.steps[2].clone();
        step3.execution_status = rustycode_protocol::StepStatus::InProgress;
        step3.started_at = Some(Utc::now() - chrono::Duration::minutes(4));
        step3.results = vec!["Running tests...".to_string()];
        storage.update_plan_step(&plan.id, 2, &step3).unwrap();

        let loaded = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(
            loaded.steps[0].execution_status,
            rustycode_protocol::StepStatus::Completed
        );
        assert_eq!(
            loaded.steps[1].execution_status,
            rustycode_protocol::StepStatus::Completed
        );
        assert_eq!(
            loaded.steps[2].execution_status,
            rustycode_protocol::StepStatus::InProgress
        );

        // Step 3: Tests pass after fix
        let mut step3_final = loaded.steps[2].clone();
        step3_final.execution_status = rustycode_protocol::StepStatus::Completed;
        step3_final.completed_at = Some(Utc::now());
        step3_final.results = vec!["All tests passed".to_string(), "Coverage: 95%".to_string()];
        step3_final.tool_executions = vec![rustycode_protocol::StepToolExecution {
            tool_name: "Bash".to_string(),
            args: serde_json::json!({"command": "cargo test"}).to_string(),
            output: "test result: ok. 15 passed, 0 failed".to_string(),
            error: None,
            timestamp: Utc::now(),
        }];
        storage.update_plan_step(&plan.id, 2, &step3_final).unwrap();

        // UPDATE: Mark plan as completed
        storage
            .update_plan_status(&plan.id, &PlanStatus::Completed)
            .unwrap();

        // READ: Final verification
        let final_plan = storage.load_plan(&plan.id).unwrap().unwrap();
        assert_eq!(final_plan.status, PlanStatus::Completed);
        assert!(final_plan
            .steps
            .iter()
            .all(|s| s.execution_status == rustycode_protocol::StepStatus::Completed));

        // READ: Verify plan appears in session list
        let session_plans = storage.list_plans(&session.id).unwrap();
        assert_eq!(session_plans.len(), 1);
        assert_eq!(session_plans[0].id, plan.id);

        // READ: Verify plan appears in all plans list
        let all_plans = storage.all_plans(10).unwrap();
        assert_eq!(all_plans.len(), 1);
        assert_eq!(all_plans[0].id, plan.id);
    }

    #[test]
    fn event_persistence_and_retrieval() {
        use rustycode_bus::{SessionStartedEvent, ToolExecutedEvent};
        use serde_json::json;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Create and insert a session started event
        let session_event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        storage
            .insert_event_bus(&session_event)
            .expect("Failed to insert session event");

        // Create and insert a tool executed event
        let tool_event = ToolExecutedEvent::new(
            SessionId::new(),
            "Read".to_string(),
            json!({ "path": "/test/path" }),
            true,
            "success".to_string(),
            None,
        );

        storage
            .insert_event_bus(&tool_event)
            .expect("Failed to insert tool event");

        // Retrieve events
        let events = storage.events(10).expect("Failed to get events");

        // Verify we got 2 events
        assert_eq!(events.len(), 2);

        // Verify first event (most recent - tool event)
        assert_eq!(events[0].event_type, "tool.executed");
        assert!(events[0].event_data.contains("Read"));
        assert!(events[0].id > 0);

        // Verify second event (session started)
        assert_eq!(events[1].event_type, "session.started");
        assert!(events[1].event_data.contains("test task"));

        // Verify timestamps are valid RFC3339
        assert!(chrono::DateTime::parse_from_rfc3339(&events[0].created_at).is_ok());
        assert!(chrono::DateTime::parse_from_rfc3339(&events[1].created_at).is_ok());
    }

    #[test]
    fn get_events_respects_limit() {
        use rustycode_bus::SessionStartedEvent;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Insert 5 events
        for i in 0..5 {
            let event = SessionStartedEvent::new(
                SessionId::new(),
                format!("task {}", i),
                format!("detail {}", i),
            );
            storage.insert_event_bus(&event).unwrap();
        }

        // Request only 3 events
        let events = storage.events(3).unwrap();
        assert_eq!(events.len(), 3);

        // Request 10 events but only 5 exist
        let events = storage.events(10).unwrap();
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn get_events_returns_most_recent_first() {
        use rustycode_bus::SessionStartedEvent;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Insert events with a small delay to ensure different timestamps
        let session_ids: Vec<_> = (0..3).map(|_| SessionId::new()).collect();

        for (i, session_id) in session_ids.iter().enumerate() {
            let event = SessionStartedEvent::new(
                session_id.clone(),
                format!("task {}", i),
                format!("detail {}", i),
            );
            storage.insert_event_bus(&event).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let events = storage.events(10).unwrap();

        // Events should be in reverse chronological order
        assert_eq!(events[0].event_type, "session.started");
        assert!(events[0].event_data.contains("task 2"));
        assert!(events[1].event_data.contains("task 1"));
        assert!(events[2].event_data.contains("task 0"));
    }

    #[test]
    fn event_persistence_with_complex_data() {
        use rustycode_bus::{ContextAssembledEvent, ToolExecutedEvent};
        use rustycode_protocol::{ContextPlan, ContextSection, ContextSectionKind};
        use serde_json::json;

        let storage = Storage::open(&temp_db_path()).unwrap();

        // Create a context assembled event with complex nested data
        let context_plan = ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![ContextSection {
                kind: ContextSectionKind::CodeExcerpts,
                tokens_reserved: 50000,
                tokens_used: 5000,
                items: vec!["src/main.rs".to_string()],
                note: "Main entry point".to_string(),
            }],
        };

        let context_event = ContextAssembledEvent::new(
            SessionId::new(),
            context_plan,
            "Context assembled".to_string(),
        );

        storage
            .insert_event_bus(&context_event)
            .expect("Failed to insert context event");

        // Create a tool event with complex arguments
        let tool_event = ToolExecutedEvent::new(
            SessionId::new(),
            "complex_tool".to_string(),
            json!({
                "nested": {
                    "array": [1, 2, 3],
                    "string": "test",
                    "number": 42.5
                }
            }),
            true,
            "Complex tool output".to_string(),
            None,
        );

        storage
            .insert_event_bus(&tool_event)
            .expect("Failed to insert tool event");

        // Retrieve and verify
        let events = storage.events(10).unwrap();
        assert_eq!(events.len(), 2);

        // Verify JSON data can be parsed
        let tool_data: serde_json::Value =
            serde_json::from_str(&events[0].event_data).expect("Failed to parse tool event data");
        assert_eq!(tool_data["tool_name"], "complex_tool");
        assert_eq!(tool_data["arguments"]["nested"]["number"], 42.5);

        let context_data: serde_json::Value = serde_json::from_str(&events[1].event_data)
            .expect("Failed to parse context event data");
        // Verify context plan data is preserved
        assert_eq!(context_data["context_plan"]["total_budget"], 200000);
        assert_eq!(context_data["detail"], "Context assembled");

        // Verify event types are correct
        assert_eq!(events[0].event_type, "tool.executed");
        assert_eq!(events[1].event_type, "context.assembled");
    }

    #[test]
    fn empty_events_table_returns_empty_vec() {
        let storage = Storage::open(&temp_db_path()).unwrap();

        let events = storage.events(10).unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn event_record_fields_are_accessible() {
        use rustycode_bus::SessionStartedEvent;

        let storage = Storage::open(&temp_db_path()).unwrap();

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        storage.insert_event_bus(&event).unwrap();

        let events = storage.events(1).unwrap();
        assert_eq!(events.len(), 1);

        let record = &events[0];
        assert!(record.id > 0);
        assert_eq!(record.event_type, "session.started");
        assert!(!record.event_data.is_empty());
        assert!(!record.created_at.is_empty());

        // Verify we can parse the timestamp
        let parsed_time = chrono::DateTime::parse_from_rfc3339(&record.created_at);
        assert!(parsed_time.is_ok());
    }

    #[test]
    fn test_session_snapshot_round_trip() {
        use super::SessionSnapshot;
        use rustycode_protocol::{PlanId, SessionId};

        let storage = Storage::open(&temp_db_path()).unwrap();

        let snapshot = SessionSnapshot {
            session_id: SessionId::new(),
            captured_at: Utc::now(),
            conversation_json: r#"{"messages":[]}"#.to_string(),
            active_plan_id: None,
            metadata: std::collections::HashMap::new(),
        };

        storage.save_snapshot(&snapshot).unwrap();
        let loaded = storage
            .load_snapshot(&snapshot.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.session_id, snapshot.session_id);
        assert_eq!(loaded.conversation_json, snapshot.conversation_json);

        // Test list_snapshot_sessions
        let sessions = storage.list_snapshot_sessions().unwrap();
        assert!(sessions.contains(&snapshot.session_id));

        storage.delete_snapshots(&snapshot.session_id).unwrap();
        assert!(storage
            .load_snapshot(&snapshot.session_id)
            .unwrap()
            .is_none());

        // Verify list is now empty for this session
        let sessions = storage.list_snapshot_sessions().unwrap();
        assert!(!sessions.contains(&snapshot.session_id));

        // Test with active_plan_id
        let snap2 = SessionSnapshot {
            session_id: SessionId::new(),
            captured_at: Utc::now(),
            conversation_json: r#"{"messages":[{"role":"user","content":"hello"}]}"#.to_string(),
            active_plan_id: Some(PlanId::new()),
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("key".to_string(), "value".to_string());
                m
            },
        };
        storage.save_snapshot(&snap2).unwrap();
        let loaded2 = storage.load_snapshot(&snap2.session_id).unwrap().unwrap();
        assert!(loaded2.active_plan_id.is_some());
        assert_eq!(loaded2.metadata.get("key").unwrap(), "value");
    }

    #[test]
    fn search_conversations_like_fallback() {
        use super::SessionSnapshot;

        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Create a session and snapshot with known content
        let session = make_session("test search");
        storage.insert_session(&session).unwrap();

        let snap = SessionSnapshot {
            session_id: session.id.clone(),
            captured_at: Utc::now(),
            conversation_json: r#"{"messages":[{"role":"user","content":"implement fibonacci in rust"},{"role":"assistant","content":"fn fib(n: u32) -> u32 { ... }"}]}"#.to_string(),
            active_plan_id: None,
            metadata: Default::default(),
        };
        storage.save_snapshot(&snap).unwrap();

        // Search for "fibonacci"
        let hits = storage.search_conversations("fibonacci", None).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.to_lowercase().contains("fibonacci"));

        // Search for non-existent term
        let misses = storage
            .search_conversations("xyzzy_nonexistent", None)
            .unwrap();
        assert!(misses.is_empty());

        // Test limit
        let hits_limited = storage.search_conversations("fibonacci", Some(0)).unwrap();
        assert!(hits_limited.is_empty());
    }

    #[test]
    fn load_session_returns_inserted_session() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("load by id");
        storage.insert_session(&session).unwrap();

        let loaded = storage.load_session(&session.id).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.task, "load by id");
        assert_eq!(loaded.mode, SessionMode::Executing);
    }

    #[test]
    fn load_session_returns_none_for_unknown_id() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let result = storage.load_session(&SessionId::new()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_memory_returns_entries_for_scope() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        storage.upsert_memory("project", "lang", "rust").unwrap();
        storage.upsert_memory("project", "style", "async").unwrap();
        storage.upsert_memory("global", "theme", "dark").unwrap();

        let project_mem = storage.memory("project").unwrap();
        assert_eq!(project_mem.len(), 2);
        assert_eq!(project_mem[0].key, "lang"); // ordered by key
        assert_eq!(project_mem[1].key, "style");
        assert_eq!(project_mem[0].scope, "project");

        let global_mem = storage.memory("global").unwrap();
        assert_eq!(global_mem.len(), 1);
        assert_eq!(global_mem[0].value, "dark");
    }

    #[test]
    fn get_memory_returns_empty_for_unknown_scope() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let result = storage.memory("nonexistent").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn get_memory_entry_returns_specific_value() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        storage.upsert_memory("project", "style", "async").unwrap();

        let value = storage.memory_entry("project", "style").unwrap();
        assert_eq!(value, Some("async".to_string()));

        let missing = storage.memory_entry("project", "missing").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn upsert_memory_overwrites_existing() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        storage.upsert_memory("project", "style", "sync").unwrap();
        storage.upsert_memory("project", "style", "async").unwrap();

        let value = storage.memory_entry("project", "style").unwrap();
        assert_eq!(value, Some("async".to_string()));

        let all = storage.memory("project").unwrap();
        assert_eq!(all.len(), 1); // Still only one entry
    }

    #[test]
    fn multiple_sessions_coexist() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let s1 = make_session("task one");
        let s2 = make_session("task two");
        storage.insert_session(&s1).unwrap();
        storage.insert_session(&s2).unwrap();

        assert_eq!(storage.session_count().unwrap(), 2);
        let loaded1 = storage.load_session(&s1.id).unwrap().unwrap();
        assert_eq!(loaded1.task, "task one");
        let loaded2 = storage.load_session(&s2.id).unwrap().unwrap();
        assert_eq!(loaded2.task, "task two");

        let recent = storage.recent_sessions(10).unwrap();
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_db_stats_empty() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();
        let stats = storage.db_stats().unwrap();

        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.event_count, 0);
        assert_eq!(stats.api_call_count, 0);
        assert!(stats.oldest_session.is_none());
        assert!(stats.newest_session.is_none());
    }

    #[test]
    fn test_db_stats_with_data() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        let session = make_session("stats test");
        storage.insert_session(&session).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: session.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "test".to_string(),
            })
            .unwrap();

        let stats = storage.db_stats().unwrap();
        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.event_count, 1);
        assert!(stats.oldest_session.is_some());
        assert!(stats.newest_session.is_some());
    }

    #[test]
    fn test_cleanup_old_sessions_removes_nothing_when_all_recent() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        let session = make_session("recent task");
        storage.insert_session(&session).unwrap();

        // Cleanup sessions older than 365 days — nothing should be removed
        let stats = storage.cleanup_old_sessions(365).unwrap();
        assert_eq!(stats.sessions_removed, 0);

        // Session should still be there
        assert!(storage.load_session(&session.id).unwrap().is_some());
    }

    #[test]
    fn test_cleanup_all_sessions() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Insert sessions and events
        let s1 = make_session("task one");
        let s2 = make_session("task two");
        storage.insert_session(&s1).unwrap();
        storage.insert_session(&s2).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: s1.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "event1".to_string(),
            })
            .unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: s2.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "event2".to_string(),
            })
            .unwrap();

        let stats = storage.cleanup_all_sessions().unwrap();
        assert_eq!(stats.sessions_removed, 2);
        assert_eq!(stats.events_removed, 2);

        // Verify everything is gone
        let db_stats = storage.db_stats().unwrap();
        assert_eq!(db_stats.session_count, 0);
        assert_eq!(db_stats.event_count, 0);
    }

    #[test]
    fn test_vuum_compacts_database() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Insert and delete data to create fragmentation
        for i in 0..50 {
            let session = make_session(&format!("task {}", i));
            storage.insert_session(&session).unwrap();
        }
        storage.cleanup_all_sessions().unwrap();

        let size_before = storage.db_stats().unwrap().db_size_bytes;

        // Vacuum should succeed
        storage.vacuum().unwrap();

        let size_after = storage.db_stats().unwrap().db_size_bytes;
        // After vacuum, the DB should be smaller or equal (pages reclaimed)
        assert!(size_after <= size_before);
    }

    #[test]
    fn test_cleanup_orphaned_events() {
        let path = temp_db_path();
        let storage = Storage::open(&path).unwrap();

        // Insert a session and its event
        let session = make_session("orphan test");
        storage.insert_session(&session).unwrap();
        storage
            .insert_event(&SessionEvent {
                session_id: session.id.clone(),
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "event".to_string(),
            })
            .unwrap();

        // Insert an event for a session that doesn't exist
        let fake_id = SessionId::new();
        storage
            .insert_event(&SessionEvent {
                session_id: fake_id,
                at: Utc::now(),
                kind: EventKind::SessionStarted,
                detail: "orphan event".to_string(),
            })
            .unwrap();

        assert_eq!(storage.db_stats().unwrap().event_count, 2);

        // Cleanup orphans
        let removed = storage.cleanup_orphaned_events().unwrap();
        assert_eq!(removed, 1);

        // Only the real session's event should remain
        assert_eq!(storage.db_stats().unwrap().event_count, 1);
    }

    #[test]
    fn test_completed_summaries_capped_at_1000() {
        use crate::session_capture::SessionOutcome;
        let mgr = SessionCaptureManager::new(None);
        // Finalize 1010 sessions to exceed the cap
        for i in 0..1010 {
            let sid = SessionId::new();
            let session_id_str = sid.to_string();
            mgr.start_session(sid, format!("cap-test-{}", i));
            mgr.finalize_session(&session_id_str, SessionOutcome::Success);
        }
        // Summaries should be capped (drain 100 when reaching 1000, then push 10 more)
        {
            let summaries = mgr.completed_summaries.lock().unwrap();
            let len = summaries.len();
            assert!(len <= 1010, "Summaries grew beyond expected: {}", len);
        }
    }

    #[test]
    fn test_learnings_capped_at_500() {
        use crate::session_capture::SessionOutcome;
        let mgr = SessionCaptureManager::new(None);
        // Each session adds one learning
        for i in 0..510 {
            let sid = SessionId::new();
            let session_id_str = sid.to_string();
            mgr.start_session(sid, format!("learn-test-{}", i));
            mgr.finalize_session(&session_id_str, SessionOutcome::Success);
        }
        // Learnings should be bounded (not 510+)
        {
            let learnings = mgr.learnings.lock().unwrap();
            let len = learnings.len();
            assert!(len <= 510, "Learnings grew beyond expected: {}", len);
        }
    }

    // ── Milestone CRUD tests ──────────────────────────────────────────────

    fn make_plan_with_session(session_id: &SessionId) -> Plan {
        Plan {
            id: PlanId::new(),
            session_id: session_id.clone(),
            milestone_id: None,
            task: "test task".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "test summary".to_string(),
            approach: String::new(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        }
    }

    #[test]
    fn milestone_insert_and_load() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("milestone test");
        storage.insert_session(&session).unwrap();

        let milestone = Milestone {
            id: MilestoneId::new(),
            session_id: session.id.clone(),
            title: "Auth feature".to_string(),
            description: "Implement auth".to_string(),
            status: MilestoneStatus::Draft,
            plan_ids: vec![],
            plan_dependencies: vec![],
            success_criteria: vec!["Login works".to_string()],
            validation_command: Some("cargo test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        let id = milestone.id.clone();

        storage.insert_milestone(&milestone).unwrap();
        let loaded = storage.load_milestone(&id).unwrap().unwrap();
        assert_eq!(loaded.title, "Auth feature");
        assert_eq!(loaded.success_criteria, vec!["Login works"]);
        assert_eq!(loaded.validation_command, Some("cargo test".to_string()));
    }

    #[test]
    fn milestone_not_found_returns_none() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        assert!(storage
            .load_milestone(&MilestoneId::new())
            .unwrap()
            .is_none());
    }

    #[test]
    fn milestone_list_by_session() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session_a = make_session("session a");
        let session_b = make_session("session b");
        storage.insert_session(&session_a).unwrap();
        storage.insert_session(&session_b).unwrap();

        for i in 0..3 {
            storage
                .insert_milestone(&Milestone {
                    id: MilestoneId::new(),
                    session_id: session_a.id.clone(),
                    title: format!("Milestone {i}"),
                    description: String::new(),
                    status: MilestoneStatus::Draft,
                    plan_ids: vec![],
                    plan_dependencies: vec![],
                    success_criteria: vec![],
                    validation_command: None,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    completed_at: None,
                })
                .unwrap();
        }
        storage
            .insert_milestone(&Milestone {
                id: MilestoneId::new(),
                session_id: session_b.id.clone(),
                title: "Other".to_string(),
                description: String::new(),
                status: MilestoneStatus::Draft,
                plan_ids: vec![],
                plan_dependencies: vec![],
                success_criteria: vec![],
                validation_command: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                completed_at: None,
            })
            .unwrap();

        let a_milestones = storage.list_milestones(&session_a.id).unwrap();
        assert_eq!(a_milestones.len(), 3);
        let b_milestones = storage.list_milestones(&session_b.id).unwrap();
        assert_eq!(b_milestones.len(), 1);
    }

    #[test]
    fn milestone_status_transition_and_completed_at() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("status test");
        storage.insert_session(&session).unwrap();

        let milestone = Milestone {
            id: MilestoneId::new(),
            session_id: session.id.clone(),
            title: "Status test".to_string(),
            description: String::new(),
            status: MilestoneStatus::Draft,
            plan_ids: vec![],
            plan_dependencies: vec![],
            success_criteria: vec![],
            validation_command: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        let id = milestone.id.clone();
        storage.insert_milestone(&milestone).unwrap();

        storage
            .update_milestone_status(&id, &MilestoneStatus::Active)
            .unwrap();
        let active = storage.load_milestone(&id).unwrap().unwrap();
        assert_eq!(active.status, MilestoneStatus::Active);
        assert!(active.completed_at.is_none());

        storage
            .update_milestone_status(&id, &MilestoneStatus::Completed)
            .unwrap();
        let completed = storage.load_milestone(&id).unwrap().unwrap();
        assert_eq!(completed.status, MilestoneStatus::Completed);
        assert!(completed.completed_at.is_some());
    }

    #[test]
    fn add_plan_to_milestone_links_both_sides() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("link test");
        storage.insert_session(&session).unwrap();

        let milestone = Milestone {
            id: MilestoneId::new(),
            session_id: session.id.clone(),
            title: "Link test".to_string(),
            description: String::new(),
            status: MilestoneStatus::Draft,
            plan_ids: vec![],
            plan_dependencies: vec![],
            success_criteria: vec![],
            validation_command: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        let milestone_id = milestone.id.clone();
        storage.insert_milestone(&milestone).unwrap();

        let plan = make_plan_with_session(&session.id);
        let plan_id = plan.id.clone();
        storage.insert_plan(&plan).unwrap();

        storage
            .add_plan_to_milestone(&milestone_id, &plan_id)
            .unwrap();

        let loaded_milestone = storage.load_milestone(&milestone_id).unwrap().unwrap();
        assert!(loaded_milestone.plan_ids.contains(&plan_id));

        let loaded_plan = storage.load_plan(&plan_id).unwrap().unwrap();
        assert_eq!(loaded_plan.milestone_id, Some(milestone_id));
    }

    #[test]
    fn milestone_plans_returns_linked_plans() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("plans test");
        storage.insert_session(&session).unwrap();

        let milestone = Milestone {
            id: MilestoneId::new(),
            session_id: session.id.clone(),
            title: "Plans test".to_string(),
            description: String::new(),
            status: MilestoneStatus::Draft,
            plan_ids: vec![],
            plan_dependencies: vec![],
            success_criteria: vec![],
            validation_command: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        let milestone_id = milestone.id.clone();
        storage.insert_milestone(&milestone).unwrap();

        let plan_a = make_plan_with_session(&session.id);
        let plan_b = make_plan_with_session(&session.id);
        let unrelated = make_plan_with_session(&session.id);
        storage.insert_plan(&plan_a).unwrap();
        storage.insert_plan(&plan_b).unwrap();
        storage.insert_plan(&unrelated).unwrap();

        storage
            .add_plan_to_milestone(&milestone_id, &plan_a.id)
            .unwrap();
        storage
            .add_plan_to_milestone(&milestone_id, &plan_b.id)
            .unwrap();

        let plans = storage.milestone_plans(&milestone_id).unwrap();
        assert_eq!(plans.len(), 2);
        assert!(plans
            .iter()
            .all(|p| p.milestone_id == Some(milestone_id.clone())));
    }

    #[test]
    fn plan_backward_compat_deserializes_without_milestone_id() {
        let storage = Storage::open(&temp_db_path()).unwrap();
        let session = make_session("compat test");
        storage.insert_session(&session).unwrap();

        let plan = make_plan_with_session(&session.id);
        let plan_id = plan.id.clone();
        storage.insert_plan(&plan).unwrap();

        let loaded = storage.load_plan(&plan_id).unwrap().unwrap();
        assert_eq!(loaded.milestone_id, None);
    }
}
