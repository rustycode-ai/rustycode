//! Workspace feature module
//!
//! Handles workspace/project state and context management.
//! Owns workspace-related state (current task, project files, execution context).
//!
//! ## State
//! - `WorkspaceState`: Tracks current workspace, open files, task execution context
//!
//! ## Events Handled
//! - `TuiEvent::Service(EventMsg)`: Workspace updates and task completion
//! - `TuiEvent::Tick`: Periodic workspace refresh
//!
//! ## Surfaces
//! - "workspace": File explorer and project context view
//!
//! ## Rendering
//! Renders workspace structure, open files, and execution context

use crate::app::features::{
    FeatureRegistry, RenderCtx, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use chrono::{DateTime, Utc};
use ratatui::Frame;

/// Representation of a workspace file/directory entry
#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    /// File path relative to workspace root
    pub path: String,
    /// Whether this is a directory
    pub is_dir: bool,
    /// Last modification time
    pub modified_at: Option<DateTime<Utc>>,
}

/// Execution context for the current task/step
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Current task/step being executed
    pub current_task: String,
    /// Step number in the current task
    pub step: usize,
    /// Total steps for this task
    pub total_steps: usize,
    /// Execution status: "pending", "running", "complete", "error"
    pub status: String,
}

/// Workspace state management
#[derive(Default)]
pub struct WorkspaceState {
    /// Root workspace path
    pub root_path: Option<String>,
    /// Files in the workspace (from most recent scan)
    pub files: Vec<WorkspaceEntry>,
    /// Currently open files (for tab-like display)
    pub open_files: Vec<String>,
    /// Current active/focused file
    pub active_file: Option<String>,
    /// Execution context for the current operation
    pub execution_context: Option<ExecutionContext>,
    /// Whether the workspace needs a refresh
    pub needs_refresh: bool,
}

impl WorkspaceState {
    /// Create a new workspace state
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize workspace with a root path
    pub fn initialize(&mut self, root_path: impl Into<String>) {
        self.root_path = Some(root_path.into());
        self.needs_refresh = true;
    }

    /// Update the list of files in the workspace
    pub fn update_files(&mut self, files: Vec<WorkspaceEntry>) {
        self.files = files;
        self.needs_refresh = false;
    }

    /// Open a file in the workspace
    pub fn open_file(&mut self, path: impl Into<String>) {
        let path_str = path.into();
        if !self.open_files.contains(&path_str) {
            self.open_files.push(path_str.clone());
        }
        self.active_file = Some(path_str);
    }

    /// Close a file from the open files list
    pub fn close_file(&mut self, path: &str) {
        self.open_files.retain(|p| p != path);
        if self.active_file.as_deref() == Some(path) {
            self.active_file = self.open_files.last().cloned();
        }
    }

    /// Set the execution context
    pub fn set_execution_context(&mut self, context: ExecutionContext) {
        self.execution_context = Some(context);
    }

    /// Clear the execution context (e.g., on task completion)
    pub fn clear_execution_context(&mut self) {
        self.execution_context = None;
    }

    /// Mark workspace as needing a refresh
    pub fn mark_dirty(&mut self) {
        self.needs_refresh = true;
    }

    /// Get the number of open files
    pub fn open_file_count(&self) -> usize {
        self.open_files.len()
    }

    /// Reset workspace state
    pub fn reset(&mut self) {
        self.root_path = None;
        self.files.clear();
        self.open_files.clear();
        self.active_file = None;
        self.execution_context = None;
        self.needs_refresh = false;
    }
}

/// Workspace feature for project context and file management
pub struct WorkspaceFeature {
    state: WorkspaceState,
    surface: SurfaceId,
}

impl WorkspaceFeature {
    /// Create a new workspace feature
    pub fn new() -> Self {
        Self {
            state: WorkspaceState::new(),
            surface: SurfaceId::new("workspace"),
        }
    }
}

impl Default for WorkspaceFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiFeature for WorkspaceFeature {
    fn id(&self) -> &'static str {
        "workspace"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(self.surface, self.id());
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        match event {
            TuiEvent::Service(_event_msg) => {
                // TODO: Handle workspace update events
                // - WorkspaceUpdate: update_files()
                // - TaskUpdate: set_execution_context()
                // - CompletionEvent: clear_execution_context()
                Vec::new()
            }
            TuiEvent::Tick => {
                // TODO: Refresh workspace if needed
                if self.state.needs_refresh {
                    // Trigger workspace scan
                    vec![]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn render(&self, surface: SurfaceId, _frame: &mut Frame, _ctx: &RenderCtx) {
        if surface == self.surface {
            // TODO: Implement workspace rendering
            // - Render file tree with icons
            // - Show open file tabs
            // - Display execution context / current task status
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_state_new_is_empty() {
        let state = WorkspaceState::new();
        assert!(state.root_path.is_none());
        assert_eq!(state.files.len(), 0);
        assert_eq!(state.open_files.len(), 0);
    }

    #[test]
    fn workspace_state_initializes_root() {
        let mut state = WorkspaceState::new();
        state.initialize("/home/project");
        assert_eq!(state.root_path, Some("/home/project".to_string()));
        assert!(state.needs_refresh);
    }

    #[test]
    fn workspace_state_manages_files() {
        let mut state = WorkspaceState::new();
        let files = vec![
            WorkspaceEntry {
                path: "src/main.rs".to_string(),
                is_dir: false,
                modified_at: None,
            },
            WorkspaceEntry {
                path: "src".to_string(),
                is_dir: true,
                modified_at: None,
            },
        ];
        state.update_files(files);
        assert_eq!(state.files.len(), 2);
        assert!(!state.needs_refresh);
    }

    #[test]
    fn workspace_state_manages_open_files() {
        let mut state = WorkspaceState::new();
        state.open_file("src/main.rs");
        state.open_file("src/lib.rs");

        assert_eq!(state.open_file_count(), 2);
        assert_eq!(state.active_file, Some("src/lib.rs".to_string()));

        state.close_file("src/lib.rs");
        assert_eq!(state.open_file_count(), 1);
        assert_eq!(state.active_file, Some("src/main.rs".to_string()));
    }

    #[test]
    fn workspace_state_prevents_duplicate_open() {
        let mut state = WorkspaceState::new();
        state.open_file("src/main.rs");
        state.open_file("src/main.rs");

        assert_eq!(state.open_file_count(), 1);
    }

    #[test]
    fn workspace_state_tracks_execution_context() {
        let mut state = WorkspaceState::new();
        let context = ExecutionContext {
            current_task: "Refactor module".to_string(),
            step: 2,
            total_steps: 5,
            status: "running".to_string(),
        };
        state.set_execution_context(context);

        assert!(state.execution_context.is_some());
        assert_eq!(state.execution_context.as_ref().unwrap().step, 2);

        state.clear_execution_context();
        assert!(state.execution_context.is_none());
    }

    #[test]
    fn workspace_feature_has_id() {
        let feature = WorkspaceFeature::new();
        assert_eq!(feature.id(), "workspace");
    }

    #[test]
    fn workspace_feature_registers_surface() {
        let feature = WorkspaceFeature::new();
        let mut reg = crate::app::features::FeatureRegistry::new();
        feature.register(&mut reg);
        assert_eq!(
            reg.surface_feature(SurfaceId::new("workspace")),
            Some("workspace")
        );
    }

    #[test]
    fn workspace_state_resets_all() {
        let mut state = WorkspaceState::new();
        state.initialize("/project");
        state.open_file("main.rs");
        state.mark_dirty();

        state.reset();

        assert!(state.root_path.is_none());
        assert_eq!(state.files.len(), 0);
        assert_eq!(state.open_files.len(), 0);
        assert!(!state.needs_refresh);
    }
}
