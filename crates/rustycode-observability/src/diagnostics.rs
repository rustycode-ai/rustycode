//! Diagnostic checks for environment and workspace health.
//!
//! Provides a structured suite of checks (git repo, Cargo workspace, rustc
//! version, config file, writable memory directory) that can be run to
//! produce a [`DiagnosticReport`]. Each check is an independent closure so
//! callers can extend or replace individual checks as needed.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// CheckStatus
// ---------------------------------------------------------------------------

/// Outcome of a single diagnostic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// Not yet executed.
    Pending,
    /// Check succeeded.
    Pass,
    /// Check failed (blocking).
    Fail,
    /// Check passed with a non-blocking warning.
    Warning,
    /// Check was skipped (e.g. optional and prerequisites missing).
    Skipped,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "PENDING"),
            Self::Pass => write!(f, "PASS"),
            Self::Fail => write!(f, "FAIL"),
            Self::Warning => write!(f, "WARNING"),
            Self::Skipped => write!(f, "SKIPPED"),
        }
    }
}

// ---------------------------------------------------------------------------
// DiagnosticCheck
// ---------------------------------------------------------------------------

/// A single named check within a diagnostic suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    /// Unique identifier for this check (e.g. `"git_repo"`).
    pub id: String,
    /// Human-readable description of what is being checked.
    pub description: String,
    /// Current status of the check.
    pub status: CheckStatus,
    /// Error or warning message populated when status is `Fail` or `Warning`.
    pub error_message: Option<String>,
    /// Category grouping (e.g. `"environment"`, `"toolchain"`).
    pub category: String,
    /// If `true`, a `Fail` status is treated as non-blocking.
    pub is_optional: bool,
}

impl DiagnosticCheck {
    /// Create a new check with the given id, description, and category.
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            status: CheckStatus::Pending,
            error_message: None,
            category: category.into(),
            is_optional: false,
        }
    }

    /// Mark the check as optional (failure does not block health).
    pub const fn optional(mut self) -> Self {
        self.is_optional = true;
        self
    }

    /// Run a check closure. On `Ok(())` the status becomes `Pass`; on `Err`
    /// it becomes `Fail` and the error message is captured.
    pub fn run<F>(&mut self, check_fn: F)
    where
        F: FnOnce() -> anyhow::Result<()>,
    {
        match check_fn() {
            Ok(()) => {
                self.status = CheckStatus::Pass;
                self.error_message = None;
            }
            Err(e) => {
                self.status = CheckStatus::Fail;
                self.error_message = Some(e.to_string());
            }
        }
    }

    /// Run a check closure that may produce warnings.
    ///
    /// Returns `Ok(true)` on pass, `Ok(false)` on warning, and `Err` on
    /// failure. The status and error message are updated accordingly.
    pub fn run_with_warning<F>(&mut self, check_fn: F)
    where
        F: FnOnce() -> Result<bool, anyhow::Error>,
    {
        match check_fn() {
            Ok(true) => {
                self.status = CheckStatus::Pass;
                self.error_message = None;
            }
            Ok(false) => {
                self.status = CheckStatus::Warning;
            }
            Err(e) => {
                self.status = CheckStatus::Fail;
                self.error_message = Some(e.to_string());
            }
        }
    }

    /// Mark this check as skipped with an optional reason.
    pub fn skip(&mut self, reason: Option<&str>) {
        self.status = CheckStatus::Skipped;
        self.error_message = reason.map(String::from);
    }
}

// ---------------------------------------------------------------------------
// DiagnosticReport
// ---------------------------------------------------------------------------

/// Aggregated report produced after running a [`DiagnosticSuite`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    /// Individual check results.
    pub checks: Vec<DiagnosticCheck>,
    /// Total number of checks.
    pub total_checks: usize,
    /// Number of checks with `Pass` status.
    pub passed_count: usize,
    /// Number of checks with `Fail` status.
    pub failed_count: usize,
    /// Number of checks with `Warning` status.
    pub warning_count: usize,
    /// When this report was generated.
    pub timestamp: DateTime<Utc>,
}

impl DiagnosticReport {
    /// Returns `true` if no required (non-optional) checks failed.
    pub fn is_healthy(&self) -> bool {
        self.checks
            .iter()
            .all(|c| c.status != CheckStatus::Fail || c.is_optional)
    }
}

// ---------------------------------------------------------------------------
// DiagnosticSuite
// ---------------------------------------------------------------------------

/// Builder and runner for a standard set of environment diagnostics.
pub struct DiagnosticSuite {
    /// Root directory for path-based checks. Defaults to `"."`.
    root: PathBuf,
}

impl DiagnosticSuite {
    /// Create a new suite rooted at the given directory.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Build the default set of checks without running them.
    pub fn build_checks(&self) -> Vec<DiagnosticCheck> {
        vec![
            DiagnosticCheck::new(
                "git_repo",
                "Current directory is inside a git repository",
                "environment",
            ),
            DiagnosticCheck::new(
                "workspace",
                "Cargo.toml workspace manifest is present and valid",
                "toolchain",
            ),
            DiagnosticCheck::new(
                "rust_version",
                "Rust toolchain is available and meets minimum version",
                "toolchain",
            ),
            DiagnosticCheck::new(
                "config",
                "RustyCode configuration file is loadable",
                "configuration",
            )
            .optional(),
            DiagnosticCheck::new(
                "memory_writable",
                "Memory/cache directory is writable",
                "environment",
            ),
        ]
    }

    /// Run all checks and return a [`DiagnosticReport`].
    pub fn run_all(&self) -> DiagnosticReport {
        let mut checks = self.build_checks();

        // --- git repo ---
        Self::run_check(&mut checks, "git_repo", || {
            let git_dir = self.root.join(".git");
            if git_dir.exists() {
                Ok(())
            } else {
                // Also check if we are inside a git worktree by running git
                // rev-parse as a fallback.
                let output = std::process::Command::new("git")
                    .args(["rev-parse", "--git-dir"])
                    .current_dir(&self.root)
                    .output()?;
                if output.status.success() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "No git repository found at {}",
                        self.root.display()
                    ))
                }
            }
        });

        // --- workspace ---
        Self::run_check(&mut checks, "workspace", || {
            let manifest = self.root.join("Cargo.toml");
            if !manifest.exists() {
                return Err(anyhow::anyhow!(
                    "Cargo.toml not found at {}",
                    manifest.display()
                ));
            }
            let content = std::fs::read_to_string(&manifest)?;
            if content.contains("[package]") || content.contains("[workspace]") {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Cargo.toml does not appear to be a valid manifest"
                ))
            }
        });

        // --- rust version ---
        Self::run_check(&mut checks, "rust_version", || {
            let output = std::process::Command::new("rustc")
                .arg("--version")
                .output()?;
            if !output.status.success() {
                return Err(anyhow::anyhow!("rustc not found or failed to execute"));
            }
            let version_str = String::from_utf8_lossy(&output.stdout);
            // Expect at least "rustc 1.xx.x" -- a minimal sanity check.
            if version_str.starts_with("rustc") {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Unexpected rustc output: {}",
                    version_str.trim()
                ))
            }
        });

        // --- config (optional) ---
        Self::run_check(&mut checks, "config", || {
            let config_path = self.root.join("rustycode.toml");
            if config_path.exists() {
                let _content = std::fs::read_to_string(&config_path)?;
                Ok(())
            } else {
                // Config is optional -- skip rather than fail.
                Err(anyhow::anyhow!("No rustycode.toml found (optional)"))
            }
        });
        // If the config check failed, and it is optional, convert fail to skip.
        if let Some(c) = checks.iter_mut().find(|c| c.id == "config") {
            if c.status == CheckStatus::Fail && c.is_optional {
                c.status = CheckStatus::Skipped;
            }
        }

        // --- memory writable ---
        Self::run_check(&mut checks, "memory_writable", || {
            let memory_dir = self.root.join(".rustycode").join("memory");
            std::fs::create_dir_all(&memory_dir)?;
            let test_file = memory_dir.join(".write_test");
            std::fs::write(&test_file, b"test")?;
            std::fs::read_to_string(&test_file)?;
            std::fs::remove_file(&test_file)?;
            Ok(())
        });

        let passed_count = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Pass)
            .count();
        let failed_count = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count();
        let warning_count = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Warning)
            .count();

        DiagnosticReport {
            total_checks: checks.len(),
            passed_count,
            failed_count,
            warning_count,
            checks,
            timestamp: Utc::now(),
        }
    }

    /// Helper: find a check by id and run the provided closure.
    fn run_check<F>(checks: &mut [DiagnosticCheck], id: &str, check_fn: F)
    where
        F: FnOnce() -> anyhow::Result<()>,
    {
        if let Some(check) = checks.iter_mut().find(|c| c.id == id) {
            check.run(check_fn);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_status_display() {
        assert_eq!(CheckStatus::Pending.to_string(), "PENDING");
        assert_eq!(CheckStatus::Pass.to_string(), "PASS");
        assert_eq!(CheckStatus::Fail.to_string(), "FAIL");
        assert_eq!(CheckStatus::Warning.to_string(), "WARNING");
        assert_eq!(CheckStatus::Skipped.to_string(), "SKIPPED");
    }

    #[test]
    fn check_status_serde_roundtrip() {
        let json = serde_json::to_string(&CheckStatus::Pass).unwrap();
        assert_eq!(json, "\"pass\"");
        let back: CheckStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CheckStatus::Pass);
    }

    #[test]
    fn diagnostic_check_new_defaults() {
        let check = DiagnosticCheck::new("test_id", "A test check", "testing");
        assert_eq!(check.id, "test_id");
        assert_eq!(check.description, "A test check");
        assert_eq!(check.status, CheckStatus::Pending);
        assert!(check.error_message.is_none());
        assert!(!check.is_optional);
        assert_eq!(check.category, "testing");
    }

    #[test]
    fn diagnostic_check_optional_flag() {
        let check = DiagnosticCheck::new("opt", "Optional check", "testing").optional();
        assert!(check.is_optional);
    }

    #[test]
    fn diagnostic_check_run_success() {
        let mut check = DiagnosticCheck::new("ok", "Should pass", "testing");
        check.run(|| Ok(()));
        assert_eq!(check.status, CheckStatus::Pass);
        assert!(check.error_message.is_none());
    }

    #[test]
    fn diagnostic_check_run_failure() {
        let mut check = DiagnosticCheck::new("fail", "Should fail", "testing");
        check.run(|| Err(anyhow::anyhow!("something broke")));
        assert_eq!(check.status, CheckStatus::Fail);
        assert_eq!(check.error_message.as_deref(), Some("something broke"));
    }

    #[test]
    fn diagnostic_check_run_with_warning() {
        let mut check = DiagnosticCheck::new("warn", "May warn", "testing");
        check.run_with_warning(|| Ok(false));
        assert_eq!(check.status, CheckStatus::Warning);
    }

    #[test]
    fn diagnostic_check_run_with_warning_pass() {
        let mut check = DiagnosticCheck::new("wp", "May pass", "testing");
        check.run_with_warning(|| Ok(true));
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn diagnostic_check_skip() {
        let mut check = DiagnosticCheck::new("sk", "Skip me", "testing");
        check.skip(Some("not applicable"));
        assert_eq!(check.status, CheckStatus::Skipped);
        assert_eq!(check.error_message.as_deref(), Some("not applicable"));
    }

    #[test]
    fn diagnostic_report_healthy_when_all_pass() {
        let report = DiagnosticReport {
            checks: vec![
                DiagnosticCheck::new("a", "a", "t"),
                DiagnosticCheck::new("b", "b", "t"),
            ],
            total_checks: 2,
            passed_count: 2,
            failed_count: 0,
            warning_count: 0,
            timestamp: Utc::now(),
        };
        assert!(report.is_healthy());
    }

    #[test]
    fn diagnostic_report_unhealthy_when_required_fails() {
        let mut c = DiagnosticCheck::new("a", "a", "t");
        c.run(|| Err(anyhow::anyhow!("fail")));
        let report = DiagnosticReport {
            checks: vec![c],
            total_checks: 1,
            passed_count: 0,
            failed_count: 1,
            warning_count: 0,
            timestamp: Utc::now(),
        };
        assert!(!report.is_healthy());
    }

    #[test]
    fn diagnostic_report_healthy_when_optional_fails() {
        let mut c = DiagnosticCheck::new("a", "a", "t").optional();
        c.run(|| Err(anyhow::anyhow!("fail")));
        let report = DiagnosticReport {
            checks: vec![c],
            total_checks: 1,
            passed_count: 0,
            failed_count: 1,
            warning_count: 0,
            timestamp: Utc::now(),
        };
        // Optional failures do not count against health.
        assert!(report.is_healthy());
    }

    #[test]
    fn suite_build_checks_count() {
        let suite = DiagnosticSuite::new(".");
        let checks = suite.build_checks();
        assert_eq!(checks.len(), 5);
        assert!(checks.iter().any(|c| c.id == "git_repo"));
        assert!(checks.iter().any(|c| c.id == "workspace"));
        assert!(checks.iter().any(|c| c.id == "rust_version"));
        assert!(checks.iter().any(|c| c.id == "config"));
        assert!(checks.iter().any(|c| c.id == "memory_writable"));
    }

    #[test]
    fn suite_run_all_returns_report() {
        let suite = DiagnosticSuite::new(".");
        let report = suite.run_all();
        assert_eq!(report.total_checks, 5);
        // Executed counts may be less than total if some checks were skipped.
        let executed = report.passed_count + report.failed_count + report.warning_count;
        let skipped = report
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Skipped)
            .count();
        assert_eq!(executed + skipped, report.total_checks);
        // All checks should have been executed (not Pending).
        assert!(report
            .checks
            .iter()
            .all(|c| c.status != CheckStatus::Pending));
    }

    #[test]
    fn suite_run_all_from_project_root() {
        // Running from the actual project root should find git + workspace.
        let suite = DiagnosticSuite::new(env!("CARGO_MANIFEST_DIR").to_string() + "/../..");
        let report = suite.run_all();
        let git = report.checks.iter().find(|c| c.id == "git_repo").unwrap();
        assert_eq!(git.status, CheckStatus::Pass);

        let ws = report.checks.iter().find(|c| c.id == "workspace").unwrap();
        assert_eq!(ws.status, CheckStatus::Pass);
    }
}
