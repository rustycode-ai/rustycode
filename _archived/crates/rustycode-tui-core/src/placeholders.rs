//! Placeholder types for TUI state management
//!
//! These are temporary placeholders that will be replaced with actual implementations
//! as we migrate functionality from the monolithic TUI.

#[derive(Debug, Clone, Default)]
pub struct StreamingRenderBuffer;

#[derive(Debug, Clone, Default)]
pub struct ToolExecution;

#[derive(Debug, Clone, Default)]
pub struct WorkspaceTasks;

#[derive(Debug, Clone, Default)]
pub struct PipelineRegistry;

#[derive(Debug, Clone, Default)]
pub struct PipelineContext;

#[derive(Debug, Clone, Default)]
pub struct PipelineGuardian;

#[derive(Debug, Clone, Default)]
pub struct ScheduledPhaseEvent;

#[derive(Debug, Clone, Default)]
pub struct Animator;

#[derive(Debug, Clone, Default)]
pub struct TUIConfig;

#[derive(Debug, Clone, Default)]
pub struct ThemeColors;

#[derive(Debug, Clone, Default)]
pub struct SessionRecoveryManager;

#[derive(Debug, Clone, Default)]
pub struct Task;

#[derive(Debug, Clone, Default)]
pub struct Todo;

#[derive(Debug, Clone, Default)]
pub struct SearchState;

#[derive(Debug, Clone, Default)]
pub struct TagFilter;

#[derive(Debug, Clone, Default)]
pub struct FileFinder;

#[derive(Debug, Clone, Default)]
pub struct InputState;

/// Thin wrapper around `CostTracker` that provides a `Default` impl
/// for use as a placeholder in session state.
pub struct CostTracker(pub rustycode_llm::cost_tracker::CostTracker);

impl Default for CostTracker {
    fn default() -> Self {
        Self(rustycode_llm::cost_tracker::CostTracker::new(None))
    }
}

impl std::fmt::Debug for CostTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CostTracker").finish_non_exhaustive()
    }
}
