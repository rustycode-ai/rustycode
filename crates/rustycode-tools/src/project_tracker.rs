//! Project Tracker
//!
//! Tracks project metadata (path, last access time, last instruction,
//! last session ID) with JSON persistence. Useful for project switching,
//! recent project lists, and session continuity.
//!
//! Inspired by goose's `project_tracker.rs` in `goose-cli`.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::project_tracker::{ProjectTracker, update_project_tracker};
//!
//! // Convenience function - updates tracker for current directory
//! update_project_tracker(Some("fix the auth bug"), Some("session-123")).ok();
//!
//! // Manual usage
//! let mut tracker = ProjectTracker::load().unwrap();
//! tracker.update_project(Path::new("/tmp/myproject"), Some("hello"), None).unwrap();
//! let projects = tracker.list_projects();
//! ```

use crate::workspace::paths::AppPaths;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata about a tracked project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Absolute path to the project directory
    pub path: String,
    /// Last time the project was accessed
    pub last_accessed: DateTime<Utc>,
    /// Last instruction sent (if available)
    pub last_instruction: Option<String>,
    /// Last session ID associated with this project
    pub last_session_id: Option<String>,
}

/// Tracks project information with JSON persistence.
///
/// Data is stored in `{data_dir}/projects.json`. Projects are keyed
/// by their absolute path string.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectTracker {
    projects: HashMap<String, ProjectInfo>,
    /// Override path for testing (not serialized).
    #[serde(skip)]
    path_override: Option<PathBuf>,
}

impl ProjectTracker {
    /// Get the path to the projects.json file.
    fn projects_file_path(&self) -> Result<PathBuf> {
        if let Some(ref override_path) = self.path_override {
            return Ok(override_path.clone());
        }
        let path = AppPaths::in_data_dir("projects.json");
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        Ok(path)
    }

    /// Load the project tracker from disk.
    ///
    /// Returns an empty tracker if the file doesn't exist yet.
    pub fn load() -> Result<Self> {
        let dummy = Self {
            projects: HashMap::new(),
            path_override: None,
        };
        let path = dummy.projects_file_path()?;

        if path.exists() {
            let content = fs::read_to_string(&path)
                .context(format!("Failed to read projects file: {}", path.display()))?;
            let mut tracker: Self =
                serde_json::from_str(&content).context("Failed to parse projects.json")?;
            tracker.path_override = None;
            Ok(tracker)
        } else {
            Ok(dummy)
        }
    }

    /// Load the project tracker from a specific file path.
    ///
    /// Useful for testing with isolated temp directories.
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        if path.exists() {
            let content = fs::read_to_string(path)
                .context(format!("Failed to read projects file: {}", path.display()))?;
            let mut tracker: Self =
                serde_json::from_str(&content).context("Failed to parse projects.json")?;
            tracker.path_override = Some(path.clone());
            Ok(tracker)
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            Ok(Self {
                projects: HashMap::new(),
                path_override: Some(path.clone()),
            })
        }
    }

    /// Save the project tracker to disk.
    pub fn save(&self) -> Result<()> {
        let path = self.projects_file_path()?;
        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &json).context(format!(
            "Failed to write projects file: {}",
            tmp_path.display()
        ))?;
        fs::rename(&tmp_path, &path).context(format!(
            "Failed to rename projects file: {}",
            path.display()
        ))?;
        Ok(())
    }

    /// Update or create a project entry.
    ///
    /// Updates the `last_accessed` timestamp and optionally the instruction
    /// and session ID if provided (non-None values overwrite).
    pub fn update_project(
        &mut self,
        project_dir: &Path,
        instruction: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        let dir_str = project_dir.to_string_lossy().to_string();

        let info = self.projects.entry(dir_str.clone()).or_insert(ProjectInfo {
            path: dir_str,
            last_accessed: Utc::now(),
            last_instruction: None,
            last_session_id: None,
        });

        info.last_accessed = Utc::now();

        if let Some(instr) = instruction {
            info.last_instruction = Some(instr.to_string());
        }

        if let Some(id) = session_id {
            info.last_session_id = Some(id.to_string());
        }

        self.save()
    }

    /// Remove a project entry by path.
    ///
    /// Returns true if a project was removed, false if it wasn't tracked.
    pub fn remove_project(&mut self, project_dir: &Path) -> Result<bool> {
        let dir_str = project_dir.to_string_lossy().to_string();
        let removed = self.projects.remove(&dir_str).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// List all tracked projects, sorted by `last_accessed` (most recent first).
    pub fn list_projects(&self) -> Vec<&ProjectInfo> {
        let mut projects: Vec<&ProjectInfo> = self.projects.values().collect();
        projects.sort_by_key(|a| std::cmp::Reverse(a.last_accessed));
        projects
    }

    /// Get info for a specific project by path.
    pub fn project(&self, project_dir: &Path) -> Option<&ProjectInfo> {
        let dir_str = project_dir.to_string_lossy().to_string();
        self.projects.get(&dir_str)
    }

    /// Get the number of tracked projects.
    pub fn len(&self) -> usize {
        self.projects.len()
    }

    /// Check if there are no tracked projects.
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }
}

/// Convenience function: update the project tracker for the current working directory.
///
pub fn update_project_tracker(instruction: Option<&str>, session_id: Option<&str>) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let mut tracker = ProjectTracker::load()?;
    tracker.update_project(&current_dir, instruction, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a tracker that uses a unique temp dir for isolation.
    fn isolated_tracker() -> (ProjectTracker, TempDir) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("projects.json");
        let tracker = ProjectTracker {
            projects: HashMap::new(),
            path_override: Some(path),
        };
        (tracker, temp)
    }

    #[test]
    fn test_load_empty_tracker() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("projects.json");
        let tracker = ProjectTracker::load_from(&path).unwrap();
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_update_project_creates_entry() {
        let (mut tracker, _temp) = isolated_tracker();

        tracker
            .update_project(Path::new("/tmp/test-project"), Some("build it"), None)
            .unwrap();

        assert_eq!(tracker.len(), 1);
        let info = tracker.project(Path::new("/tmp/test-project")).unwrap();
        assert_eq!(info.path, "/tmp/test-project");
        assert_eq!(info.last_instruction, Some("build it".to_string()));
        assert!(info.last_session_id.is_none());
    }

    #[test]
    fn test_update_project_persists() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("projects.json");

        let mut tracker = ProjectTracker::load_from(&path).unwrap();
        tracker
            .update_project(Path::new("/tmp/project-a"), None, Some("sess-1"))
            .unwrap();

        // Reload from same path
        let reloaded = ProjectTracker::load_from(&path).unwrap();
        assert_eq!(reloaded.len(), 1);
        let info = reloaded.project(Path::new("/tmp/project-a")).unwrap();
        assert_eq!(info.last_session_id, Some("sess-1".to_string()));
    }

    #[test]
    fn test_update_existing_project() {
        let (mut tracker, _temp) = isolated_tracker();

        tracker
            .update_project(Path::new("/tmp/project"), Some("first"), None)
            .unwrap();
        tracker
            .update_project(Path::new("/tmp/project"), Some("second"), Some("sess-2"))
            .unwrap();

        assert_eq!(tracker.len(), 1);
        let info = tracker.project(Path::new("/tmp/project")).unwrap();
        assert_eq!(info.last_instruction, Some("second".to_string()));
        assert_eq!(info.last_session_id, Some("sess-2".to_string()));
    }

    #[test]
    fn test_remove_project() {
        let (mut tracker, _temp) = isolated_tracker();

        tracker
            .update_project(Path::new("/tmp/a"), None, None)
            .unwrap();
        tracker
            .update_project(Path::new("/tmp/b"), None, None)
            .unwrap();

        assert!(tracker.remove_project(Path::new("/tmp/a")).unwrap());
        assert!(!tracker
            .remove_project(Path::new("/tmp/nonexistent"))
            .unwrap());
        assert_eq!(tracker.len(), 1);
    }

    #[test]
    fn test_list_projects_sorted_by_access_time() {
        let (mut tracker, _temp) = isolated_tracker();

        tracker
            .update_project(Path::new("/tmp/first"), None, None)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker
            .update_project(Path::new("/tmp/second"), None, None)
            .unwrap();

        let projects = tracker.list_projects();
        assert_eq!(projects.len(), 2);
        // Most recently accessed should be first
        assert_eq!(projects[0].path, "/tmp/second");
        assert_eq!(projects[1].path, "/tmp/first");
    }

    #[test]
    fn test_get_project_not_found() {
        let (tracker, _temp) = isolated_tracker();
        assert!(tracker.project(Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn test_project_info_serialization() {
        let info = ProjectInfo {
            path: "/tmp/test".to_string(),
            last_accessed: Utc::now(),
            last_instruction: Some("do something".to_string()),
            last_session_id: Some("sess-abc".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ProjectInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.path, deserialized.path);
        assert_eq!(info.last_instruction, deserialized.last_instruction);
    }
}
