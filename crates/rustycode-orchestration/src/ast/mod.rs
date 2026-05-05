//! Adaptive Structured Thinking (AST) module.
//!
//! AST is a right-sized thinking protocol for agentic task execution.
//! It combines classification, light research, skeleton-first planning,
//! rolling-wave expansion, execution without thinking, and verification
//! with local recovery.
//!
//! # Pipeline
//!
//! `CLASSIFY → RESEARCH → SKELETON → EXPAND → EXECUTE → VERIFY`
//!
//! # Complexity Routing
//!
//! | Complexity | Research | Milestones | Expansion |
//! |------------|----------|------------|-----------|
//! | TRIVIAL    | Skipped  | 1          | All       |
//! | MODERATE   | Quick    | 3-7        | All       |
//! | COMPLEX    | Full     | 3-7        | Rolling wave (2/batch) |
//!
//! # Quick Start
//!
//! ```no_run
//! use rustycode_orchestration::ast::{AstPipeline, AstConfig, VerificationStatus};
//!
//! let mut pipeline = AstPipeline::new("/path/to/workspace".into());
//! let status = pipeline.run("Fix typo in README.md").unwrap();
//! assert_eq!(status, VerificationStatus::Pass);
//! ```
//!
//! # See Also
//!
//! - [`AstPipeline`] — main pipeline controller
//! - [`CrewOrchestrator`] — crew-based orchestration with role handlers
//! - [`ContextLoader`] — smart prompt assembly with priority eviction
//! - [`TaskLedger`] — human-readable markdown task state
//! - [`ProgressStore`] — `SQLite` machine-readable task state
//!
//! Full guide: `docs/guides/AST-GUIDE.md`

pub mod bedd;
pub mod clarity;
pub mod classifier;
pub mod context_loader;
pub mod crew;
pub mod executor;
pub mod expander;
pub mod handlers;
pub mod hooks;
pub mod ledger;
pub mod pipeline;
pub mod progress_store;
pub mod prompt;
pub mod recovery;
pub mod research;
pub mod shared_memory;
pub mod skeleton;
pub mod tool_adapter;
pub mod tree;
pub mod types;
pub mod verifier;

pub use bedd::{
    BeddConfig, BeddFunnel, BeddResult, CritiqueRound, DiminishingReturnsDetector,
    EvaluatedProposal, Proposal, ProposalCritique, StopReason, Vote, VotingMethod, VotingResult,
};
pub use clarity::{
    ClarificationQuestion, ClarityConfig, ClarityDimension, ClarityReport, ClarityScore,
    ClarityScorer,
};
pub use classifier::TaskClassifier;
pub use context_loader::{
    AssembledPrompt, ContextFetcher, ContextLoader, ContextMetrics, StoreFetcher, WorkingSet,
};
pub use crew::{
    assign_subroles, dispatch_roles, ArtifactKind, ConsultationReport, CrewDispatcher, CrewHandoff,
    CrewRole, HandoffStatus, RoleDispatchConfig,
};
pub use executor::{ShellStepRunner, StepExecutor, StepRunner, MAX_RETRIES};
pub use expander::MilestoneExpander;
pub use handlers::{
    ArchitectHandler, BuilderHandler, ConsultantHandler, CrewOrchestrator, HandlerResult,
    InspectorHandler, ScoutHandler,
};
pub use hooks::{AstHookBridge, AstHookPayload, AstHookPoint, AstHookResponse, AstPhaseController};
pub use ledger::TaskLedger;
pub use pipeline::{assess_clarity, AstConfig, AstExecutionResult, AstPipeline};
pub use progress_store::{
    validate_milestone_transition, validate_phase_transition, ArtifactRecord, EventRecord,
    MilestoneRecord, ProgressStore, SubagentRunRecord, TaskRecord,
};
pub use prompt::{
    build_phase_prompt, detect_extra_sections, estimate_tokens, parse_ast_output,
    validate_phase_order, ParsedAstOutput, ParsedPhase, AST_SYSTEM_PROMPT,
};
pub use recovery::{
    FailureClassifier, FailureDiagnosis, FailureType, MilestoneRecovery, RecoveryOutcome,
    RecoveryStrategy,
};
pub use research::ResearchBriefGenerator;
pub use shared_memory::{AgentMemory, LedgerMemory, ProgressStoreMemory};
pub use skeleton::SkeletonBuilder;
pub use tool_adapter::{
    adapter, ClaudeCodeAdapter, CodexAdapter, GeminiAdapter, RustyCodeAdapter, ToolAdapter,
    ToolHarness,
};
pub use tree::{AstTree, AstTreeExplanation, AstTreeNode, AstTreeNodeKind, AstTreeStatus};
pub use types::{
    AstPhase, AstSnapshot, ComplexityLevel, ContextBrief, CriterionResult, ExecutionSegment,
    ExecutionStep, Milestone, MilestoneSkeleton, PhaseRoute, RecoveryAction, StepEvidence,
    SuccessCriterion, TaskAssessment, VerificationReport, VerificationStatus,
};
pub use verifier::Verifier;
