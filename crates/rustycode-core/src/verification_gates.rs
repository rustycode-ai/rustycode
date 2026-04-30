//! Verification gates for task completion.
//!
//! This module provides verification frameworks to prevent agents from claiming
//! success without actually testing their work. It addresses the pattern where
//! agents perform partial work (e.g., build without install) and claim success.

use serde::{Deserialize, Serialize};

/// Priority level for a verification check
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationPriority {
    /// Must pass - task is not done without this
    Critical,
    /// Should pass - task is mostly done but needs this
    Important,
    /// Nice to have - improves confidence but not required
    Nice,
}

/// Result of a single verification check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// What was being verified
    pub check: String,
    /// Whether the check passed
    pub passed: bool,
    /// Output from the verification command (if any)
    pub output: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Status of task verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Verification not started yet
    NotStarted,
    /// Verification in progress
    InProgress,
    /// All checks passed
    Passed,
    /// Some checks failed
    Failed,
}

/// A single verification check to run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCheck {
    /// What to verify (human-readable description)
    pub check: String,
    /// Command to run to verify (e.g., "python -c 'import mymodule'")
    pub command: String,
    /// Expected output pattern (if Any, just needs to return 0)
    pub expected_pattern: Option<String>,
    /// Priority of this check
    pub priority: VerificationPriority,
}

impl VerificationCheck {
    /// Create a critical verification check
    pub fn critical(check: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            command: command.into(),
            expected_pattern: None,
            priority: VerificationPriority::Critical,
        }
    }

    /// Create an important verification check
    pub fn important(check: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            command: command.into(),
            expected_pattern: None,
            priority: VerificationPriority::Important,
        }
    }

    /// Add expected output pattern to check
    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.expected_pattern = Some(pattern.into());
        self
    }
}

/// Verification report for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    /// Overall status
    pub status: VerificationStatus,
    /// Individual check results
    pub results: Vec<VerificationResult>,
    /// Whether all critical checks passed
    pub critical_passed: bool,
    /// Whether all important checks passed
    pub important_passed: bool,
    /// Summary message
    pub summary: String,
}

impl VerificationReport {
    /// Create an empty report (not started)
    pub fn new() -> Self {
        Self {
            status: VerificationStatus::NotStarted,
            results: Vec::new(),
            critical_passed: true,
            important_passed: true,
            summary: "Verification not started".to_string(),
        }
    }

    /// Create a report from verification results
    pub fn from_results(results: Vec<VerificationResult>, checks: &[VerificationCheck]) -> Self {
        let mut critical_passed = true;
        let mut important_passed = true;

        // Build mapping of checks by description
        let check_map: std::collections::HashMap<&str, &VerificationCheck> =
            checks.iter().map(|c| (c.check.as_str(), c)).collect();

        // Check which checks failed
        for result in &results {
            if !result.passed {
                if let Some(check) = check_map.get(result.check.as_str()) {
                    match check.priority {
                        VerificationPriority::Critical => critical_passed = false,
                        VerificationPriority::Important => important_passed = false,
                        VerificationPriority::Nice => {}
                    }
                }
            }
        }

        let all_passed = results.iter().all(|r| r.passed);
        let status = if all_passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        };

        let failed_count = results.iter().filter(|r| !r.passed).count();
        let summary = if all_passed {
            format!("✓ All {} verification checks passed", results.len())
        } else {
            format!(
                "✗ {} of {} verification checks failed",
                failed_count,
                results.len()
            )
        };

        Self {
            status,
            results,
            critical_passed,
            important_passed,
            summary,
        }
    }

    /// Check if task is verified (all critical checks passed)
    pub fn is_verified(&self) -> bool {
        self.critical_passed && self.status == VerificationStatus::Passed
    }

    /// Get human-readable summary
    pub fn summary(&self) -> String {
        self.summary.clone()
    }
}

impl Default for VerificationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Common verification checks for Python packages
pub fn python_package_checks(package_name: &str) -> Vec<VerificationCheck> {
    vec![
        VerificationCheck::critical(
            format!("Module '{}' can be imported", package_name),
            format!("python3 -c 'import {}; print(\"OK\")'", package_name),
        )
        .with_pattern("OK"),
        VerificationCheck::important(
            format!("Package '{}' is installed", package_name),
            format!("pip show {}", package_name),
        ),
    ]
}

/// Common verification checks for build systems
pub fn build_system_checks() -> Vec<VerificationCheck> {
    vec![VerificationCheck::critical(
        "Build artifacts exist (.so, .pyd files)",
        "find . -name '*.so' -o -name '*.pyd' | head -1".to_string(),
    )]
}

/// Common verification checks for servers
pub fn server_checks(port: u16) -> Vec<VerificationCheck> {
    vec![VerificationCheck::critical(
        format!("Server listening on port {}", port),
        format!(
            "netstat -tuln | grep :{} || ss -tuln | grep :{}",
            port, port
        ),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_check_critical() {
        let check = VerificationCheck::critical("test", "echo test");
        assert_eq!(check.priority, VerificationPriority::Critical);
    }

    #[test]
    fn test_verification_check_with_pattern() {
        let check = VerificationCheck::critical("test", "echo test").with_pattern("test.*");
        assert_eq!(check.expected_pattern, Some("test.*".to_string()));
    }

    #[test]
    fn test_verification_report_all_passed() {
        let results = vec![
            VerificationResult {
                check: "check1".to_string(),
                passed: true,
                output: Some("ok".to_string()),
                error: None,
            },
            VerificationResult {
                check: "check2".to_string(),
                passed: true,
                output: None,
                error: None,
            },
        ];

        let checks = vec![
            VerificationCheck::critical("check1", "cmd1"),
            VerificationCheck::critical("check2", "cmd2"),
        ];

        let report = VerificationReport::from_results(results, &checks);
        assert_eq!(report.status, VerificationStatus::Passed);
        assert!(report.critical_passed);
        assert!(report.is_verified());
    }

    #[test]
    fn test_verification_report_critical_failed() {
        let results = vec![VerificationResult {
            check: "check1".to_string(),
            passed: false,
            output: None,
            error: Some("failed".to_string()),
        }];

        let checks = vec![VerificationCheck::critical("check1", "cmd1")];

        let report = VerificationReport::from_results(results, &checks);
        assert_eq!(report.status, VerificationStatus::Failed);
        assert!(!report.critical_passed);
        assert!(!report.is_verified());
    }

    #[test]
    fn test_verification_report_important_failed() {
        let results = vec![
            VerificationResult {
                check: "critical1".to_string(),
                passed: true,
                output: None,
                error: None,
            },
            VerificationResult {
                check: "important1".to_string(),
                passed: false,
                output: None,
                error: Some("not important enough".to_string()),
            },
        ];

        let checks = vec![
            VerificationCheck::critical("critical1", "cmd1"),
            VerificationCheck::important("important1", "cmd2"),
        ];

        let report = VerificationReport::from_results(results, &checks);
        assert_eq!(report.status, VerificationStatus::Failed);
        assert!(report.critical_passed);
        assert!(!report.important_passed);
        // Important checks failed but critical passed - task is incomplete
        assert!(!report.is_verified());
    }

    #[test]
    fn test_python_package_checks() {
        let checks = python_package_checks("pytest");
        assert!(checks.len() >= 2);
        assert!(checks[0].check.contains("pytest"));
        assert_eq!(checks[0].priority, VerificationPriority::Critical);
    }

    #[test]
    fn test_build_system_checks() {
        let checks = build_system_checks();
        assert!(!checks.is_empty());
        assert_eq!(checks[0].priority, VerificationPriority::Critical);
    }

    #[test]
    fn test_server_checks() {
        let checks = server_checks(8000);
        assert!(!checks.is_empty());
        assert!(checks[0].check.contains("8000"));
    }
}
