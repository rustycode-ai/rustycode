//! Workspace State Management
//!
//! Manages workspace-related state including tasks, git status, and scanning progress.

/// Workspace state for project management
#[derive(Debug)]
pub struct WorkspaceState {
    /// Workspace loading status
    pub loaded: bool,
    pub context: Option<String>,

    /// Task management
    pub tasks: crate::WorkspaceTasks,

    /// Git information
    pub git_branch: Option<String>,

    /// Scanning progress
    pub scan_progress: Option<(usize, usize)>, // (scanned, total)
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            loaded: false,
            context: None,
            tasks: crate::WorkspaceTasks,
            git_branch: None,
            scan_progress: None,
        }
    }
}

/// Pipeline state for orchestrated execution
#[derive(Debug)]
pub struct PipelineState {
    /// Pipeline registry and context
    pub registry: crate::PipelineRegistry,
    pub context: crate::PipelineContext,
    pub guardian: crate::PipelineGuardian,

    /// Scheduled phases
    pub scheduler_rx: Option<std::sync::mpsc::Receiver<crate::ScheduledPhaseEvent>>,
    pub scheduler_tx: Option<std::sync::mpsc::Sender<crate::ScheduledPhaseEvent>>,
    pub active_scheduled_phases: std::collections::HashSet<String>,
    pub max_concurrent_phases: usize,

    /// Plan mode
    pub plan_mode: rustycode_orchestration::plan_mode::PlanMode,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            registry: crate::PipelineRegistry,
            context: crate::PipelineContext,
            guardian: crate::PipelineGuardian,
            scheduler_rx: None,
            scheduler_tx: None,
            active_scheduled_phases: std::collections::HashSet::new(),
            max_concurrent_phases: 3,
            plan_mode: rustycode_orchestration::plan_mode::PlanMode::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_state_default() {
        let state = WorkspaceState::default();
        assert!(!state.loaded);
        assert!(state.context.is_none());
        assert!(state.git_branch.is_none());
        assert!(state.scan_progress.is_none());
    }

    #[test]
    fn test_workspace_state_loaded() {
        let state = WorkspaceState {
            loaded: true,
            context: Some("my project".to_string()),
            git_branch: Some("main".to_string()),
            ..WorkspaceState::default()
        };
        assert!(state.loaded);
        assert_eq!(state.context.as_deref(), Some("my project"));
        assert_eq!(state.git_branch.as_deref(), Some("main"));
    }

    #[test]
    fn test_workspace_scan_progress() {
        let state = WorkspaceState {
            scan_progress: Some((50, 100)),
            ..WorkspaceState::default()
        };
        let (scanned, total) = state.scan_progress.unwrap_or((0, 0));
        assert_eq!(scanned, 50);
        assert_eq!(total, 100);
    }

    #[test]
    fn test_pipeline_state_default() {
        let state = PipelineState::default();
        assert!(state.scheduler_rx.is_none());
        assert!(state.scheduler_tx.is_none());
        assert!(state.active_scheduled_phases.is_empty());
        assert_eq!(state.max_concurrent_phases, 3);
    }
}
