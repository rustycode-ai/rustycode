//! Workspace context management
//!
//! Handles loading and managing workspace context for the LLM.

use std::path::PathBuf;

/// Workspace state
pub struct WorkspaceState {
    /// Whether workspace has been loaded
    pub loaded: bool,
    /// Workspace context for LLM
    pub context: Option<String>,
    /// Path to the workspace
    pub path: PathBuf,
}

impl WorkspaceState {
    /// Create a new workspace state
    pub fn new(path: PathBuf) -> Self {
        Self {
            loaded: false,
            context: None,
            path,
        }
    }

    /// Check if workspace is loaded
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Get the workspace context
    pub fn get_context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    /// Set the workspace context (after loading)
    pub fn set_context(&mut self, context: String) {
        self.context = Some(context);
        self.loaded = true;
    }

    /// Mark workspace as loaded
    pub fn mark_loaded(&mut self) {
        self.loaded = true;
    }

    /// Get workspace path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Clear the workspace state
    pub fn clear(&mut self) {
        self.loaded = false;
        self.context = None;
    }
}

/// Summary of workspace loading status
#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    /// Number of files indexed
    pub file_count: usize,
    /// Number of directories
    pub dir_count: usize,
    /// Total size in bytes
    pub total_size: u64,
    /// Whether loading is complete
    pub complete: bool,
}

impl WorkspaceSummary {
    /// Create a new summary
    pub fn new(file_count: usize, dir_count: usize, total_size: u64) -> Self {
        Self {
            file_count,
            dir_count,
            total_size,
            complete: true,
        }
    }

    /// Create an in-progress summary
    pub fn in_progress() -> Self {
        Self {
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            complete: false,
        }
    }

    /// Format as a display string
    pub fn display(&self) -> String {
        if self.complete {
            format!(
                "Workspace loaded: {} files, {} directories",
                self.file_count, self.dir_count
            )
        } else {
            "Loading workspace...".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_state_new() {
        let path = PathBuf::from("/test/path");
        let state = WorkspaceState::new(path);

        assert!(!state.is_loaded());
        assert_eq!(state.get_context(), None);
        assert_eq!(state.path(), &PathBuf::from("/test/path"));
    }

    #[test]
    fn test_workspace_state_set_context() {
        let mut state = WorkspaceState::new(PathBuf::from("/test"));
        state.set_context("context here".to_string());

        assert!(state.is_loaded());
        assert_eq!(state.get_context(), Some("context here"));
    }

    #[test]
    fn test_workspace_summary_complete() {
        let summary = WorkspaceSummary::new(100, 20, 50000);

        assert!(summary.complete);
        assert_eq!(summary.file_count, 100);
        assert_eq!(summary.dir_count, 20);
        assert_eq!(summary.total_size, 50000);

        let display = summary.display();
        assert!(display.contains("100 files"));
        assert!(display.contains("20 directories"));
    }

    #[test]
    fn test_workspace_summary_in_progress() {
        let summary = WorkspaceSummary::in_progress();

        assert!(!summary.complete);
        assert_eq!(summary.display(), "Loading workspace...");
    }
}
