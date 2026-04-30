//! Skill lifecycle state management
//!
//! This module defines the comprehensive lifecycle states for skills,
//! including installation, activation, execution, and error states.
//! It provides state transition validation and tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Complete lifecycle state of a skill
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SkillLifecycleState {
    /// Skill is available in marketplace but not installed
    NotInstalled,

    /// Skill is installed locally but not configured for auto-triggering
    Installed,

    /// Skill is installed and auto-triggering is enabled
    Active,

    /// Skill is installed but auto-triggering is temporarily disabled
    Inactive,

    /// Skill is currently executing
    Running,

    /// Skill last execution failed with error message
    Error,

    /// Skill is being updated
    Updating,

    /// Skill installation is incomplete or corrupted
    Corrupted,
}

impl SkillLifecycleState {
    /// Get display icon for this state
    pub fn icon(self) -> &'static str {
        match self {
            Self::NotInstalled => "📦",
            Self::Installed => "🧩",
            Self::Active => "⚡",
            Self::Inactive => "💤",
            Self::Running => "🔄",
            Self::Error => "❌",
            Self::Updating => "⏬",
            Self::Corrupted => "⚠️",
        }
    }

    /// Get display color name for this state
    pub fn color_name(self) -> &'static str {
        match self {
            Self::NotInstalled => "gray",
            Self::Installed => "blue",
            Self::Active => "green",
            Self::Inactive => "yellow",
            Self::Running => "cyan",
            Self::Error => "red",
            Self::Updating => "magenta",
            Self::Corrupted => "red",
        }
    }

    /// Check if state allows skill execution
    pub fn can_execute(self) -> bool {
        matches!(self, Self::Installed | Self::Active | Self::Inactive)
    }

    /// Check if state is stable (not transitional)
    pub fn is_stable(self) -> bool {
        matches!(
            self,
            Self::NotInstalled | Self::Installed | Self::Active | Self::Inactive | Self::Error
        )
    }
}

/// Installation metadata for a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationMetadata {
    /// Source repository URL
    pub source_url: String,

    /// Installation timestamp
    pub installed_at: DateTime<Utc>,

    /// Last update check timestamp
    pub last_update_check: Option<DateTime<Utc>>,

    /// Installed version (commit hash, tag, or semver)
    pub installed_version: String,

    /// Latest available version (if known)
    pub latest_version: Option<String>,

    /// Whether an update is available
    pub update_available: bool,

    /// Installation path
    pub install_path: std::path::PathBuf,
}

impl InstallationMetadata {
    /// Create new installation metadata
    pub fn new(source_url: String, install_path: std::path::PathBuf) -> Self {
        Self {
            source_url,
            installed_at: Utc::now(),
            last_update_check: None,
            installed_version: "unknown".to_string(),
            latest_version: None,
            update_available: false,
            install_path,
        }
    }

    /// Mark update check as performed
    pub fn mark_update_check(&mut self, latest_version: String) {
        self.last_update_check = Some(Utc::now());
        self.latest_version = Some(latest_version.clone());
        self.update_available = self.installed_version != latest_version;
    }

    /// Update installed version
    pub fn update_version(&mut self, new_version: String) {
        self.installed_version = new_version;
        self.update_available = false;
    }
}

/// Runtime statistics for skill execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStatistics {
    /// Total number of executions
    pub run_count: usize,

    /// Number of successful executions
    pub success_count: usize,

    /// Number of failed executions
    pub error_count: usize,

    /// Last execution timestamp
    pub last_run_at: Option<DateTime<Utc>>,

    /// Last success timestamp
    pub last_success_at: Option<DateTime<Utc>>,

    /// Last error message
    pub last_error: Option<String>,

    /// Average execution duration in milliseconds
    pub avg_duration_ms: Option<u64>,
}

impl SkillStatistics {
    /// Create new statistics
    pub fn new() -> Self {
        Self {
            run_count: 0,
            success_count: 0,
            error_count: 0,
            last_run_at: None,
            last_success_at: None,
            last_error: None,
            avg_duration_ms: None,
        }
    }

    /// Record a successful execution
    pub fn record_success(&mut self, duration_ms: u64) {
        self.run_count += 1;
        self.success_count += 1;
        self.last_run_at = Some(Utc::now());
        self.last_success_at = Some(Utc::now());
        self.last_error = None;

        // Update average duration
        if let Some(avg) = self.avg_duration_ms {
            self.avg_duration_ms = Some(
                (avg * (self.success_count - 1) as u64 + duration_ms) / self.success_count as u64,
            );
        } else {
            self.avg_duration_ms = Some(duration_ms);
        }
    }

    /// Record a failed execution
    pub fn record_error(&mut self, error: String) {
        self.run_count += 1;
        self.error_count += 1;
        self.last_run_at = Some(Utc::now());
        self.last_error = Some(error);
    }

    /// Calculate success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.run_count == 0 {
            100.0
        } else {
            (self.success_count as f64 / self.run_count as f64) * 100.0
        }
    }

    /// Get formatted last run time
    pub fn last_run_display(&self) -> String {
        if let Some(last_run) = self.last_run_at {
            let now = Utc::now();
            let duration = now.signed_duration_since(last_run);

            if duration.num_seconds() < 60 {
                format!("{}s ago", duration.num_seconds())
            } else if duration.num_minutes() < 60 {
                format!("{}m ago", duration.num_minutes())
            } else if duration.num_hours() < 24 {
                format!("{}h ago", duration.num_hours())
            } else {
                format!("{}d ago", duration.num_days())
            }
        } else {
            "Never".to_string()
        }
    }
}

impl Default for SkillStatistics {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete lifecycle information for a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLifecycle {
    /// Current lifecycle state
    pub state: SkillLifecycleState,

    /// Installation metadata (if installed)
    pub installation: Option<InstallationMetadata>,

    /// Execution statistics
    pub statistics: SkillStatistics,

    /// Whether skill is pinned (prevents auto-update)
    pub pinned: bool,

    /// Whether skill is a built-in core skill
    pub builtin: bool,
}

impl SkillLifecycle {
    /// Create new lifecycle for marketplace skill (not installed)
    pub fn marketplace() -> Self {
        Self {
            state: SkillLifecycleState::NotInstalled,
            installation: None,
            statistics: SkillStatistics::new(),
            pinned: false,
            builtin: false,
        }
    }

    /// Create new lifecycle for installed skill
    pub fn installed(source_url: String, install_path: std::path::PathBuf) -> Self {
        Self {
            state: SkillLifecycleState::Installed,
            installation: Some(InstallationMetadata::new(source_url, install_path)),
            statistics: SkillStatistics::new(),
            pinned: false,
            builtin: false,
        }
    }

    /// Create new lifecycle for built-in skill
    pub fn builtin_skill() -> Self {
        Self {
            state: SkillLifecycleState::Installed,
            installation: None,
            statistics: SkillStatistics::new(),
            pinned: true,
            builtin: true,
        }
    }

    /// Transition to active state
    pub fn activate(&mut self) -> Result<(), String> {
        match self.state {
            SkillLifecycleState::Installed | SkillLifecycleState::Inactive => {
                self.state = SkillLifecycleState::Active;
                Ok(())
            }
            SkillLifecycleState::Active => Ok(()),
            _ => Err(format!("Cannot activate skill in state: {:?}", self.state)),
        }
    }

    /// Transition to inactive state
    pub fn deactivate(&mut self) -> Result<(), String> {
        match self.state {
            SkillLifecycleState::Active => {
                self.state = SkillLifecycleState::Inactive;
                Ok(())
            }
            SkillLifecycleState::Inactive => Ok(()),
            _ => Err(format!(
                "Cannot deactivate skill in state: {:?}",
                self.state
            )),
        }
    }

    /// Transition to running state
    pub fn mark_running(&mut self) -> Result<(), String> {
        if !self.state.can_execute() {
            return Err(format!("Cannot execute skill in state: {:?}", self.state));
        }
        self.state = SkillLifecycleState::Running;
        Ok(())
    }

    /// Transition from running to success/error
    pub fn mark_completed(&mut self, success: bool, error: Option<String>) {
        if success {
            self.statistics.record_success(0);
            self.state = if self.state == SkillLifecycleState::Running {
                SkillLifecycleState::Active
            } else {
                self.state
            };
        } else {
            if let Some(err) = error {
                self.statistics.record_error(err);
            }
            self.state = SkillLifecycleState::Error;
        }
    }

    /// Check if update is available
    pub fn has_update(&self) -> bool {
        self.installation
            .as_ref()
            .map(|inst| inst.update_available && !self.pinned)
            .unwrap_or(false)
    }

    /// Get version display string
    pub fn version_display(&self) -> String {
        if let Some(installation) = &self.installation {
            if let Some(latest) = &installation.latest_version {
                format!("{} (latest: {})", installation.installed_version, latest)
            } else {
                installation.installed_version.clone()
            }
        } else {
            "N/A".to_string()
        }
    }

    /// Check if skill can be updated
    pub fn can_update(&self) -> bool {
        !self.builtin && self.installation.is_some()
    }
}

impl Default for SkillLifecycle {
    fn default() -> Self {
        Self::marketplace()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_state_icons() {
        assert_eq!(SkillLifecycleState::NotInstalled.icon(), "📦");
        assert_eq!(SkillLifecycleState::Installed.icon(), "🧩");
        assert_eq!(SkillLifecycleState::Active.icon(), "⚡");
        assert_eq!(SkillLifecycleState::Inactive.icon(), "💤");
        assert_eq!(SkillLifecycleState::Running.icon(), "🔄");
        assert_eq!(SkillLifecycleState::Error.icon(), "❌");
    }

    #[test]
    fn test_state_transitions() {
        let mut lifecycle = SkillLifecycle::installed(
            "https://github.com/test/skill".to_string(),
            std::path::PathBuf::from("/test/skill"),
        );

        // Can activate
        assert!(lifecycle.activate().is_ok());
        assert_eq!(lifecycle.state, SkillLifecycleState::Active);

        // Can deactivate
        assert!(lifecycle.deactivate().is_ok());
        assert_eq!(lifecycle.state, SkillLifecycleState::Inactive);

        // Cannot activate from not installed
        let mut marketplace = SkillLifecycle::marketplace();
        assert!(marketplace.activate().is_err());
    }

    #[test]
    fn test_statistics_success_rate() {
        let mut stats = SkillStatistics::new();

        stats.record_success(100);
        stats.record_success(200);
        stats.record_error("Test error".to_string());

        assert_eq!(stats.run_count, 3);
        assert_eq!(stats.success_count, 2);
        assert_eq!(stats.error_count, 1);
        assert!((stats.success_rate() - 66.66).abs() < 0.01);
    }

    #[test]
    fn test_installation_metadata() {
        let mut meta = InstallationMetadata::new(
            "https://github.com/test/skill".to_string(),
            std::path::PathBuf::from("/test"),
        );

        assert!(!meta.update_available);
        assert_eq!(meta.installed_version, "unknown");

        meta.mark_update_check("v2.0.0".to_string());
        assert!(meta.update_available);
        assert_eq!(meta.latest_version, Some("v2.0.0".to_string()));

        meta.update_version("v2.0.0".to_string());
        assert!(!meta.update_available);
    }

    #[test]
    fn test_lifecycle_marketplace() {
        let lifecycle = SkillLifecycle::marketplace();
        assert_eq!(lifecycle.state, SkillLifecycleState::NotInstalled);
        assert!(!lifecycle.can_update());
        assert!(!lifecycle.has_update());
    }

    #[test]
    fn test_lifecycle_builtin() {
        let lifecycle = SkillLifecycle::builtin_skill();
        assert_eq!(lifecycle.state, SkillLifecycleState::Installed);
        assert!(lifecycle.builtin);
        assert!(lifecycle.pinned);
        assert!(!lifecycle.can_update());
    }
}
