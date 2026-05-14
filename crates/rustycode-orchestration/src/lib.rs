// Test code uses unwrap/expect liberally for assertions — suppress pedantic warnings.
#![cfg_attr(
    test,
    allow(
        unknown_lints,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::single_match_else,
        clippy::ptr_arg,
        clippy::format_in_format_args,
        clippy::let_and_return,
        clippy::match_single_binding,
        clippy::bool_to_int_with_if,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::semicolon_if_nothing_returned,
        clippy::let_unit_value,
    )
)]
// Pre-existing patterns throughout the crate
#![allow(
    clippy::format_push_string,
    clippy::nonminimal_bool,
    clippy::if_not_else,
    clippy::missing_const_for_fn,
    clippy::cast_precision_loss,
    clippy::assigning_clones,
    clippy::clone_on_copy,
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::needless_borrows_for_generic_args,
    clippy::needless_collect,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::unused_self,
    clippy::use_self
)]

//! Tiered model orchestration for terminal-bench and complex task solving.
//!
//! This crate provides a model-agnostic, tiered orchestration pipeline where:
//! - **The Musicians (Tier 2)** execute the raw instructions.
//! - **The Editor (Tier 3)** reviews performance and patches mistakes.
//! - **The Composer (Tier 4)** re-composes core logic.
//! - **The Conductor** manages the symphony lifecycle.

pub mod agent_executor;
pub mod ask_user_tool;
pub mod ast;
pub mod autonomous;
pub mod autonomy;
pub mod bootstrap_service;
pub mod bus;
pub mod cache;
pub mod composer;
pub mod conductor;
pub mod config;
pub mod context;
pub mod cost_table;
pub mod delegation;
pub mod domain_context;
pub mod dummy_provider;
pub mod editor;
pub mod ensemble_strategy;
pub mod error;
pub mod error_signal;
pub mod execution_limits;
pub mod execution_trace;
pub mod executor;
pub mod executor_integration;
pub mod failure_store;
pub mod fork_join;
pub mod guard;
pub mod handoff;
pub mod harness;
pub mod hook_points;
pub mod isolation;

// Multi-agent context forwarding types
pub mod agent_context;
pub mod agent_outcome;
pub mod judge;
pub mod mailbox_router;
pub mod mailbox_sender;
pub mod milestone_prompt;
#[cfg(test)]
pub mod mock_provider_for_tests;
pub mod model_registry;
pub mod musician;
pub mod optimization_metrics;
pub mod orchestra_paths;
pub mod orchestrator;
pub mod phase;
pub mod phase_lifecycle;
pub mod pipeline;
pub mod plan_mode;
pub mod plan_refiner;
pub mod project_descriptor;
pub mod quality_detector;
pub mod reasoning_store;
pub mod recovery;
pub mod router;
pub mod routing;
pub mod routing_metrics;
pub mod schema;
pub mod session;
pub mod shared_workspace;
pub mod skeptic;
pub mod skill_exclusion;
pub mod skill_gotchas;
pub mod skill_quality;
pub mod state_derivation;
pub mod state_machine;
pub mod strategy_selector;
pub mod structured_thinking_tool;
pub mod structured_thinking_tool_impl;
pub mod summary;
pub mod supervisor;
pub mod task_context;
pub mod task_decomposer;
pub mod task_dispatcher;
pub mod task_runner;
pub mod thinking;
pub mod thinking_state;
pub mod tool_tiers;
pub mod types;
pub mod verification_gates;

// Registries (moved from rustycode-protocol — mutable runtime state doesn't belong in a types crate)
pub mod agent_registry;
pub mod cron_registry;
pub mod team_registry;
pub mod worker_registry;

pub use agent_executor::AgentSessionExecutor;
pub use anyhow::{anyhow, bail, Context, Result as AnyhowResult};
pub use autonomy::{
    AutonomyConfig, AutonomyDecider, AutonomyDecision, AutonomyLevel, ControlTuning, OperationType,
    TaskCategory, TaskTypeClassifier,
};
pub use bus::{BusHandle, MessageBus, OrchestrationEvent};
pub use composer::Composer;
pub use conductor::Conductor;
pub use context::{
    analyze_cache_efficiency, base_dir, budget_usage_percent, chars_per_token, chunk_by_relevance,
    classify_section, clear_cache, compress_prompt, compress_to_target, compute_budgets,
    compute_cache_hit_rate, content_fits_budget, count_tokens, count_tokens_sync, distill_single,
    distill_summaries, estimate_cache_savings, estimate_tokens_for_provider, format_chunks,
    init_token_counter, inline_template, is_accurate_counting_available, load_prompt,
    load_template, optimize_for_caching, parse_token_provider, prompt_loader, remaining_budget,
    reorder_for_caching, resolve_executor_context_window, score_chunks, section as prompt_section,
    set_base_dir, split_into_chunks, truncate_at_section_boundary, BudgetAllocation,
    CacheOptimizedPrompt, CacheUsage, Chunk, ChunkOptions, ChunkResult, CompressionLevel,
    CompressionOptions, CompressionResult, ContentRole, DistillationResult, ModelInfo,
    PromptSection, Provider as CacheProvider, RelevanceOptions, SectionCounts,
    SectionRole as CacheSectionRole, TaskCountRange, TokenProvider, TruncationResult,
};
pub use domain_context::DomainContext;
pub use dummy_provider::DummyLlmProvider;
pub use editor::Editor;
pub use ensemble_strategy::{
    EnsembleStrategy, ParticipantSpec, ReasoningStrategy, StrategyKind, StrategyOutcome,
};
pub use error::{OrchestrationError, OrchestrationErrorCategory, Result};
pub use error_signal::{ErrorCategory, ErrorClassifier, ErrorSignal, SignalCategory};
pub use fork_join::{
    ContextSnapshot, ForkJoinConfig, ForkJoinExecutor, ForkJoinResult, ForkResult, ForkSpec,
};
pub use handoff::{BudgetSummary, CodeSnippet, HandoffBuilder, HandoffPackage};
pub use harness::{TieredExecutionResult, TieredHarness};
pub use hook_points::{HookContext, HookPoint, HookRegistry, HookResult};
pub use isolation::{
    auto_worktree_branch, classify_tool, generate_worktree_name, in_worktree, original_base,
    ContextBudget, IsolationConfig, TierIsolation, ToolCapability, ToolPolicy, Worktree,
    WorktreeLock, WorktreeManager,
};
pub use judge::{
    build_judge_prompt, BuiltInRubrics, JudgeConfig, JudgeGrade, JudgeParseError, JudgeRubric,
    JudgeVerdict,
};
pub use mailbox_router::{MailboxError, MailboxRouter};
pub use milestone_prompt::{
    build_milestone_prompt, parse_milestone_response, MilestonePromptResult,
};
pub use musician::Musician;
pub use orchestrator::StepOrchestrator;
pub use phase_lifecycle::PhaseLifecycleManager;
pub use project_descriptor::{
    Boundary, Convention, ProjectDescriptor, Severity, TechComponent, ToolConfig, ValidationWarning,
};
pub use quality_detector::QualityDetector;
pub use reasoning_store::ReasoningStore;
pub use recovery::{
    ActivityEvent, ActivityLog, ActivityType, CrashLock, SessionForensics, UnitRuntimeRecord,
    UnitStatus,
};
pub use schema::{OutputSchema, SchemaValidationResult, TierSchema};
pub use shared_workspace::SharedWorkspace;
pub use skill_exclusion::{
    ConditionEvaluator, ExclusionClause, ExclusionClauseSet, ExclusionCondition, ExclusionContext,
    ExclusionResult,
};
pub use skill_gotchas::{Gotcha, GotchaRegistry, GotchaSeverity};
pub use skill_quality::{
    DimensionScore, QualityDimension, QualityThreshold, SkillQualityReport, SkillQualityScorer,
};
pub use strategy_selector::StrategySelector;
pub use structured_thinking_tool::{
    execute_with_ast, execute_with_ast_dry_run, should_use_ast, StructuredThinkingToolSchema,
};
pub use supervisor::{
    RuleBasedSupervisor, SupervisionDirective, SupervisionEvent, Supervisor, TaskSnapshot,
};
pub use tool_tiers::{
    capability_for_tool, default_tool_set, extended_tool_set, tier_for_tool, ToolActivationManager,
    ToolTier, UsageTracker,
};
pub use types::QualityScore;
pub use types::{
    Difficulty, ExecutionTier, OutputType, Step, StepResult, StructuredThought, TaskOutcome,
    ThoughtMetadata, ThoughtType,
};
pub use verification_gates::{
    JudgeVerificationStrategy, SchemaVerificationStrategy, VerificationGateRegistry,
    VerificationOutcome, VerificationStrategy,
};
// Replaced by direct initialization logic.
pub use delegation::{
    DelegationConfig, DelegationContext, DelegationPlanner, EnsemblePlan, SpawnDecision, TaskRole,
    TaskSpec,
};
pub use orchestra_paths::{
    agent_dir, app_root, build_milestone_file_name, milestones_dir, orchestra_root,
    resolve_milestone_file, resolve_milestone_path, resolve_slice_file, resolve_task_file,
    resolve_task_files, resolve_tasks_dir,
};
pub use phase::Phase;
pub use plan_mode::{ApprovalToken, PlanMode, PlanModeConfig, PlanModeError};
pub use router::{RoutingDecision, RoutingRequest, TaskRouter};
pub use routing_metrics::{ExecutionResult, ModelChoice, RoutingMetrics};
pub use state_derivation::{
    MilestoneRef, MilestoneState, OrchestraState, SliceRef, SliceState, StateDeriver, TaskRef,
    TaskState,
};
pub use task_decomposer::{
    decompose_local, detect_task_type, extract_concepts, DecomposedTask, Decomposer,
    TaskDecomposer, TaskType,
};
pub use task_dispatcher::{TaskDispatcher, TaskResult};
pub use task_runner::{TaskRunResult, TaskRunner};

// Optimization module re-exports.
pub use cache::{CacheMetrics, PromptCacheManager};
pub use config::{OrchestrationConfig, ParallelExecutionConfig, PromptCachingConfig};
pub use executor::{ParallelExecutor, StreamingToolExecutor, ToolExecution, ToolResult};
pub use optimization_metrics::OptimizationMetrics;
pub use routing::{
    ComplexityClassifier, ModelRouter, RoutingPolicy, TaskComplexity, TaskDescriptor,
};
pub use summary::{ResultSummarizer, SummaryConfig};

// Registry re-exports (moved from rustycode-protocol)
pub use agent_registry::{
    global_agent_registry, AgentInfo, AgentKind, AgentRegistry, AgentSelection, SpecialistAgent,
    SpecialistType, TaskAgentMatch,
};
pub use cron_registry::{global_cron_registry, CronEntry, CronRegistry};
pub use team_registry::{global_team_registry, Team, TeamRegistry, TeamStatus};
pub use worker_registry::{
    global_worker_registry, Worker, WorkerEvent, WorkerFailure, WorkerFailureKind, WorkerRegistry,
    WorkerStatus,
};
