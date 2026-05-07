//! Conflict detection types and detector.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::util::git_output;

/// Types of merge conflicts that can be detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConflictType {
    MarkerConflict,
    BothModified,
    DeleteModify,
    RenameModify,
    BinaryConflict,
    SubmoduleConflict,
    Unknown,
}

impl std::fmt::Display for ConflictType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MarkerConflict => write!(f, "marker conflict"),
            Self::BothModified => write!(f, "both modified"),
            Self::DeleteModify => write!(f, "delete/modify"),
            Self::RenameModify => write!(f, "rename/modify"),
            Self::BinaryConflict => write!(f, "binary conflict"),
            Self::SubmoduleConflict => write!(f, "submodule conflict"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Severity level of a conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ConflictSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for ConflictSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// A single detected conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub file_path: PathBuf,
    pub conflict_type: ConflictType,
    pub severity: ConflictSeverity,
    pub description: String,
    pub resolution_strategy: String,
    pub conflict_lines: Vec<usize>,
    pub conflicting_branch: Option<String>,
    pub commits: Vec<String>,
}

/// Conflict detection report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictReport {
    pub conflicts: Vec<Conflict>,
    pub repository_root: PathBuf,
    pub current_branch: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub merge_in_progress: bool,
    pub merge_branches: Vec<String>,
}

impl ConflictReport {
    pub const fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    pub fn has_critical_conflicts(&self) -> bool {
        self.conflicts
            .iter()
            .any(|c| c.severity == ConflictSeverity::Critical)
    }
}

/// Conflict detector for git repositories
pub struct ConflictDetector {
    pub(crate) repository_root: PathBuf,
}

impl ConflictDetector {
    pub fn new(path: &Path) -> Result<Self> {
        let root_str = git_output(path, &["rev-parse", "--show-toplevel"])?;
        let repository_root = PathBuf::from(root_str.trim());

        Ok(Self { repository_root })
    }

    pub fn detect_conflicts(&self) -> Result<ConflictReport> {
        // Simplified conflict detection
        Ok(ConflictReport {
            conflicts: Vec::new(),
            repository_root: self.repository_root.clone(),
            current_branch: None,
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: Vec::new(),
        })
    }

    pub fn detect_conflicts_with_branch(&self, _branch_name: &str) -> Result<ConflictReport> {
        Ok(ConflictReport {
            conflicts: Vec::new(),
            repository_root: self.repository_root.clone(),
            current_branch: None,
            detected_at: Utc::now(),
            merge_in_progress: false,
            merge_branches: Vec::new(),
        })
    }
}
