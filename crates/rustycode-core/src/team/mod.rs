//! Team-based agent orchestration modules.
//!
//! This module houses the profiling, assembly, and coordination logic that
//! determines how agents collaborate on tasks.

pub mod agent_timeline;
pub mod architect;
pub mod briefing;
pub mod coordinator;
pub mod environment_bootstrap;
pub mod event_engine;
pub mod execution_trace;
pub mod executor;
pub mod interaction_visualizer;
pub mod meta_agent;
pub mod orchestrator;
pub mod plan_manager;
pub mod profiler;
pub mod prompt_optimization;
pub mod scalpel;
pub mod team_learnings;
pub mod team_runner;
pub mod team_status;
pub mod tmux_viz;
pub mod tool_generator;

pub use agent_timeline::{
    AgentState, AgentSummary, AgentTimeline, AgentTimelineSummary, AgentTrack, TaskStatus,
    TimelineEvent,
};
pub use architect::ArchitectPhase;
pub use environment_bootstrap::{ProfileCache, ProjectProfile, ProjectProfiler};
pub use event_engine::{AgentAction, AgentListener, EventEngine, TeamEventType};
pub use execution_trace::{
    DiscoveredPattern, ExecutionTrace, ExecutionTraceBuilder, PatternCategory, PatternMiner,
    PatternMinerStats, TaskOutcome, TurnTrace,
};
pub use executor::{
    extract_json, parse_turn, tools_for_role, ExecutionOutcome, ExecutorConfig, ParsedTurn,
    PostCheckResult, PreCheckResult, TeamExecutor,
};
pub use interaction_visualizer::{
    generate_flow_diagram, generate_full_visualization, generate_sequence_diagram,
    generate_statistics,
};
pub use meta_agent::{
    analyze_trace, ImprovementProposal, MetaAgent, MetaAgentStats, TraceAnalysis,
};
pub use orchestrator::{
    is_scalpel_appropriate, MockLLMClient, OrchestratorConfig, OrchestratorOutcome, TeamEvent,
    TeamLLMClient, TeamOrchestrator,
};
pub use plan_manager::{
    AdaptationChange, AdaptationTrigger, PlanAdaptation, PlanManager, PlanOutcome, PlanProgress,
    PlanStopReason, StepFailureAction,
};
pub use profiler::TaskProfiler;
pub use prompt_optimization::{
    format_for_briefing, generate_optimizations, select_relevant, OptimizationCategory,
    PromptOptimization,
};
pub use rustycode_protocol::agent_protocol::AgentRole;
pub use scalpel::ScalpelPhase;
pub use team_learnings::{LearningCategory, LearningEntry, TeamLearnings};
pub use team_runner::{TeamRunOutcome, TeamRunner, TeamRunnerConfig};
pub use team_status::TeamStatusRenderer;
pub use tmux_viz::{is_inside_tmux, TmuxAgentVisualizer};
pub use tool_generator::{
    ApiSpec, AuthType, EndpointSpec, GeneratedTool, ParamSpec, ToolGenerationContext,
    ToolGenerator, ValidationResult,
};
