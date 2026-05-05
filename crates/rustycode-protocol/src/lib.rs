#![allow(
    clippy::doc_markdown,
    clippy::doc_link_with_quotes,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::use_self,
    clippy::redundant_closure_for_method_calls,
    clippy::uninlined_format_args,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::unreadable_literal,
    clippy::assigning_clones,
    clippy::derive_partial_eq_without_eq,
    clippy::manual_let_else,
    clippy::significant_drop_tightening,
    clippy::match_same_arms,
    clippy::unused_self,
    clippy::redundant_else,
    clippy::needless_continue,
    clippy::branches_sharing_code,
    clippy::map_unwrap_or,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::no_effect_underscore_binding,
    clippy::redundant_clone,
    clippy::needless_collect,
    clippy::too_many_lines,
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    clippy::expect_used
)]
#![cfg_attr(
    test,
    allow(
        clippy::default_trait_access,
        clippy::similar_names,
        clippy::unwrap_used,
    )
)]

//! # RustyCode Protocol Types
//!
//! This crate provides the core data structures and protocol types for the RustyCode system.
//! It defines the domain models used throughout the application, including sessions, plans,
//! events, and tool execution.
//!
//! ## Sortable ID System
//!
//! All entities use time-sortable, compact identifiers that:
//! - **Sort chronologically** - IDs can be sorted by creation time
//! - **Are human-readable** - Prefixes indicate entity type (sess_, evt_, plan_, etc.)
//! - **Are compact** - 15-30 characters vs 36 for UUIDs
//! - **Prevent collisions** - Random components prevent duplicate IDs
//!
//! ### ID Types
//!
//! - [`SessionId`] - Unique identifier for sessions (prefix: `sess_`)
//! - [`PlanId`] - Unique identifier for plans (prefix: `plan_`)
//! - [`EventId`] - Unique identifier for events (prefix: `evt_`)
//! - [`MemoryId`] - Unique identifier for memory entries (prefix: `mem_`)
//! - [`SkillId`] - Unique identifier for skills (prefix: `skl_`)
//! - [`ToolId`] - Unique identifier for tools (prefix: `tool_`)
//! - [`FileId`] - Unique identifier for files (prefix: `file_`)
//! - [`SortableId`] - Generic sortable ID (custom prefix)
//!
//! ### Example: Creating and Using IDs
//!
//! ```ignore
//! use rustycode_protocol::{SessionId, PlanId};
//!
//! // Create a new session ID
//! let session_id = SessionId::new();
//! println!("Session ID: {}", session_id); // e.g., "sess_3w8qN5zX2yK9bF8pD3m"
//!
//! // Parse an ID from a string
//! let parsed = SessionId::parse("sess_3w8qN5zX2yK9bF8pD3m").unwrap();
//!
//! // IDs are sortable and comparable
//! let id1 = SessionId::new();
//! let id2 = SessionId::new();
//! assert!(id1 < id2); // Earlier IDs sort before later ones
//!
//! // Extract timestamp
//! let timestamp = session_id.timestamp();
//! println!("Created at: {:?}", timestamp);
//! ```
//!
//! ## Module Organization
//!
//! The protocol types are organized into focused modules:
//!
//! - [`session`] - Session management types ([`Session`], [`SessionMode`], [`SessionStatus`])
//! - [`plan`] - Plan execution types ([`Plan`], [`PlanStatus`], [`PlanStep`])
//! - [`event`] - Event types ([`EventKind`], [`SessionEvent`])
//! - [`context`] - Context types ([`ContextSectionKind`], [`ContextSection`], [`ContextPlan`])
//! - [`tool`] - Tool execution types ([`ToolCall`], [`ToolResult`], [`ToolMetadata`])
//! - [`message`] - Message types ([`Message`], [`MessageContent`], [`Conversation`])
//! - [`llm`] - LLM provider and types
//!
//! ## Session Management
//!
//! Sessions represent the primary workflow unit in RustyCode:
//!
//! ```ignore
//! use rustycode_protocol::{Session, SessionMode, SessionStatus};
//!
//! let session = Session::builder()
//!     .task("Implement a new feature")
//!     .with_mode(SessionMode::Planning)
//!     .build();
//! ```
//!
//! ## Plan Execution
//!
//! Plans structure the implementation of tasks into ordered steps:
//!
//! ```ignore
//! use rustycode_protocol::{Plan, PlanStatus, PlanStep};
//! ```

// Re-export ID types from rustycode-id
//
// These are time-sortable, compact identifiers used throughout the system.
// See the crate-level documentation for details on the ID system.
pub use rustycode_id::{EventId, FileId, MemoryId, PlanId, SessionId, SkillId, SortableId, ToolId};

// ============================================================================
// MODULE DECLARATIONS
// ============================================================================

// Session management types
pub mod session;

// Plan execution types
pub mod plan;

// Event types
pub mod event;

// Context assembly types
pub mod context;

// Tool execution types
pub mod tool;
pub mod tool_filter;

// Message and conversation types
pub mod message;

// Selective message inclusion with priority-based filtering
pub mod message_selector;

// LLM configuration types
#[cfg(not(target_arch = "wasm32"))]
pub mod llm;

// Working modes for specialized agent behavior
pub mod modes;

// Intent classification for agent behavior adjustment
pub mod intent;
pub mod interpreter;
pub mod task_routing;

// Runtime permission modes for controlling tool approval
pub mod permission_modes;

// Explore-Plan-Act lifecycle types
pub mod execution_phase;
pub mod execution_plan;

// ...

pub use execution_phase::{ExecutionPhase, PhaseSkipConfig, PhaseTransitionError};
pub use execution_plan::{ExecutionPlan, ExecutionPlanBuilder, PlanValidationError};

// Team-based agent orchestration types
pub mod team;

// Agent communication protocol
pub mod agent_protocol;

// Agent registry and specialist agent generation
pub mod agent_registry;

// Worker registry for sub-agent lifecycle tracking
pub mod worker_registry;

// Stream events from the agent loop (raw, granular events)
pub mod stream_event;

// Session events (persisted, enriched)
pub mod session_event;

// Permission roles for tool gating
pub mod permission_role;

// Convoy planning types
pub mod convoy_plan;

// Cron registry for scheduled autonomous tasks
pub mod cron_registry;

// Team registry for agent grouping
pub mod team_registry;

// Conversation fixing pipeline
pub mod conversation_fixer;

// Harness execution results
pub mod executor_result;

// Generic data structures
pub mod circular_buffer;
pub mod query_guard;
// Claude Code-compatible hook protocol types
pub mod hooks;

// Compaction shared types (hybrid compaction pipeline)
pub mod compaction;

// Shared frontmatter parsing utilities
pub mod frontmatter;
// Re-exports for frontmatter helpers
pub use frontmatter::{
    normalize_paths, parse_frontmatter_map, split_frontmatter, FrontmatterMap, FrontmatterValue,
};

// ============================================================================
// PUBLIC RE-EXPORTS
// ============================================================================

// Session types
pub use session::{
    Session, SessionBuilder, SessionMode, SessionSnapshot, SessionState, SessionStatus,
    ToolApprovalMode,
};

// Plan types
pub use plan::{Plan, PlanStatus, PlanStep, StepStatus, StepToolExecution};

// Event types
pub use event::{EventKind, SessionEvent};

// Stream event types (raw agent events)
pub use stream_event::{ApprovalDecision, StreamEvent, StreamPlanStep};

// Session event types (persisted, enriched)
pub use session_event::SyncEvent;

// Context types
pub use context::{ContextPlan, ContextSection, ContextSectionKind};

// Tool types
pub use tool::{ToolCall, ToolMetadata, ToolPermission, ToolResult};

// Message types
pub use message::{
    CacheControl, CacheType, ContentBlock, Conversation, ImageSource, Message, MessageContent,
    MessageMetadata,
};

// Selective message inclusion with priority-based filtering
pub use message_selector::{
    filter_messages, MessagePriority, MessageSelector, PriorityBreakdown, SelectionConfig,
    SelectionResult,
};

// LLM types
#[cfg(not(target_arch = "wasm32"))]
pub use llm::{CompletionRequest, CompletionResponse, Cost, LLMProvider, ModelInfo, TokenCount};

// Working modes
pub use modes::WorkingMode;

// Intent classification
pub use intent::{
    classify_intent, classify_intent_with_confidence, IntentAssessment, IntentCategory,
};
pub use task_routing::{
    parse_handoff_block, render_handoff_block, TaskRoutingAction, TaskRoutingDecision,
    TaskRoutingHandoff, TaskWorkflow, HANDOFF_END, HANDOFF_START,
};

// Harness execution results
pub use executor_result::ExecutionResult;

// Permission modes
pub use permission_modes::{
    PermissionBehavior, PermissionDecision, PermissionMode, PermissionRule, PermissionRuleSet,
    PermissionRuleSource,
};
// Team types
pub use team::{
    AgentAttitude, ApproachCategory, ApproachFingerprint, AttemptOutcome, AttemptSummary, Briefing,
    BuilderTurn, BurdenOfProof, Escalation, EscalationLevel, EscalationOption, Familiarity,
    FeedbackTone, FileChange, FileSnippet, JudgeTurn, ProfileSignal, ProgressDelta, ReachLevel,
    RefutedClaim, Reversibility, ReviewDepth, RiskLevel, RoleBriefing, SignalKind, SkepticTurn,
    SkepticVerdict, StopReason, TaskProfile, TeamConfig, TeamLoopState, TeamRole, TestSummary,
    TokenBudget, ToolSet, TrustContext, TrustEvent, TrustEventKind, TrustScore, VerificationState,
    VetoAction,
};

// Agent protocol types
pub use agent_protocol::{
    AgentMessage, AgentRole, AgentSignals, ArchitectMessage, BuilderMessage, CompileError,
    DependencyChanges, DependencySpec, EscalationRequest, EscalationTarget,
    FileChange as ProtocolFileChange, InterfaceDeclaration, IssueSeverity, JudgeMessage,
    LinesChanged, ModuleAction, ModuleDeclaration, ScalpelMessage, SkepticMessage,
    SkepticVerdict as ProtocolSkepticVerdict, StructuralCompliance, StructuralDeclaration,
    SurgicalFix, ValidationResult, ARCHITECT_MIN_CONFIDENCE, BUILDER_MIN_CONFIDENCE,
    SCALPEL_MAX_FILES, SCALPEL_MAX_LINES_PER_FILE,
};

// Agent registry types
pub use agent_registry::{
    global_agent_registry, AgentInfo, AgentKind, AgentRegistry, AgentSelection, SpecialistAgent,
    SpecialistType, TaskAgentMatch,
};

// Worker registry types
pub use worker_registry::{
    global_worker_registry, Worker, WorkerEvent, WorkerFailure, WorkerFailureKind, WorkerRegistry,
    WorkerStatus,
};

// Permission roles
pub use permission_role::{PermissionRole, ToolBlockedReason};

// Convoy plan types
pub use convoy_plan::{CommandPlan, ConvoyPlan, ConvoyRisk, FilePlan, PlanApproval};

// Cron registry types
pub use cron_registry::{global_cron_registry, CronEntry, CronRegistry};

// Team registry types
pub use team_registry::{global_team_registry, Team, TeamRegistry, TeamStatus};

// Generic data structures
pub use circular_buffer::CircularBuffer;
pub use query_guard::{QueryGuard, QueryState};

// Hook protocol types
pub use hooks::{
    default_timeout, tool_matches_matcher, FlatHookEntry, HookConfigEntry, HookDecision, HookDef,
    HookEntry, HookEvent, HookExecutionResult, HookExitCode, HookInput, HookInputBuilder,
    HookOutput, HookSpecificOutput, HookType, HooksConfig, MatcherGroup, SingleHookResult,
    DEFAULT_HOOK_TIMEOUT_SECS, ENV_CLAUDE_PROJECT_DIR,
};

/// Extension methods for SessionId
pub trait SessionIdExt {
    /// Create a SessionBuilder with the specified mode
    fn with_mode(self, mode: SessionMode) -> SessionBuilder;
}

impl SessionIdExt for SessionId {
    fn with_mode(self, mode: SessionMode) -> SessionBuilder {
        SessionBuilder::new().with_mode(mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_extension() {
        let session = SessionId::new()
            .with_mode(SessionMode::Planning)
            .task("test task")
            .build();
        assert_eq!(session.mode, SessionMode::Planning);
        assert_eq!(session.status, SessionStatus::Planning);
    }

    #[test]
    fn test_re_exports() {
        // Verify all key types are re-exported
        let _session: Session = Session::builder().task("test").build();
        let _mode = SessionMode::Planning;
        let _status = SessionStatus::Created;

        let plan_id = PlanId::new();
        let event_id = EventId::new();
        let session_id = SessionId::new();

        // IDs should be unique
        assert_ne!(plan_id.to_string(), event_id.to_string());
        assert_ne!(plan_id.to_string(), session_id.to_string());
    }
}
