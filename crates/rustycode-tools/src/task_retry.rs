//! Task Retry Manager with Success Checks
//!
//! A retry manager for agent task execution that supports:
//! - Shell-based success verification between attempts
//! - On-failure cleanup hooks between retries
//! - Configurable timeouts for checks and cleanup
//! - Cross-platform command execution
//!
//! Ported from goose's `agents/retry.rs` with `RustyCode` adaptations:
//! - Standalone module (no `SessionConfig` dependency)
//! - Uses `RustyCode`'s existing `RetryConfig` from `rustycode-core`
//! - Cleaner error handling with anyhow
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::task_retry::{TaskRetryManager, TaskRetryConfig, SuccessCheck};
//!
//! let config = TaskRetryConfig::new(3)
//!     .with_check(SuccessCheck::shell("test -f output.txt"))
//!     .with_on_failure("rm -f output.txt")
//!     .with_timeout_secs(60);
//!
//! let mut manager = TaskRetryManager::new(config);
//!
//! loop {
//!     // Execute task...
//!     match manager.check_and_maybe_retry() {
//!         Ok(RetryOutcome::Success) => break,
//!         Ok(RetryOutcome::Retried(attempt)) => continue,
//!         Ok(RetryOutcome::MaxReached) => break,
//!         Err(e) => break,
//!     }
//! }
//! ```

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Default timeout for success check commands (seconds)
pub const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 60;

/// Default timeout for on-failure cleanup commands (seconds)
pub const DEFAULT_ON_FAILURE_TIMEOUT_SECS: u64 = 120;

/// Outcome of a retry evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryOutcome {
    /// All success checks passed — task completed successfully
    Success,
    /// Maximum retry attempts reached — giving up
    MaxReached,
    /// Task will be retried (contains current attempt number)
    Retried(u32),
}

/// A success check to verify task completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SuccessCheck {
    /// Run a shell command; success (exit 0) means the check passed
    Shell { command: String },
}

impl SuccessCheck {
    /// Create a shell-based success check
    pub fn shell(command: impl Into<String>) -> Self {
        Self::Shell {
            command: command.into(),
        }
    }
}

/// Configuration for task retry behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Success checks to run after each attempt
    pub checks: Vec<SuccessCheck>,

    /// Optional cleanup command to run between retries
    pub on_failure: Option<String>,

    /// Timeout for success check commands (seconds)
    pub check_timeout_secs: u64,

    /// Timeout for on-failure cleanup commands (seconds)
    pub on_failure_timeout_secs: u64,
}

impl TaskRetryConfig {
    pub const fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            checks: Vec::new(),
            on_failure: None,
            check_timeout_secs: DEFAULT_CHECK_TIMEOUT_SECS,
            on_failure_timeout_secs: DEFAULT_ON_FAILURE_TIMEOUT_SECS,
        }
    }

    /// Add a success check
    pub fn with_check(mut self, check: SuccessCheck) -> Self {
        self.checks.push(check);
        self
    }

    /// Set the on-failure cleanup command
    pub fn with_on_failure(mut self, command: impl Into<String>) -> Self {
        self.on_failure = Some(command.into());
        self
    }

    /// Set the success check timeout
    pub const fn with_timeout_secs(mut self, secs: u64) -> Self {
        self.check_timeout_secs = secs;
        self
    }

    /// Set the on-failure cleanup timeout
    pub const fn with_on_failure_timeout_secs(mut self, secs: u64) -> Self {
        self.on_failure_timeout_secs = secs;
        self
    }

    /// Check if there are any success checks configured
    pub const fn has_checks(&self) -> bool {
        !self.checks.is_empty()
    }
}

/// Manages retry state for task execution.
///
/// Tracks attempt count and orchestrates success checks,
/// on-failure hooks, and retry decisions.
pub struct TaskRetryManager {
    config: TaskRetryConfig,
    attempts: Arc<AtomicU32>,
}

impl TaskRetryManager {
    pub fn new(config: TaskRetryConfig) -> Self {
        Self {
            config,
            attempts: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Get the current attempt count
    pub fn attempts(&self) -> u32 {
        self.attempts.load(Ordering::Relaxed)
    }

    /// Reset the attempt counter
    pub fn reset(&self) {
        self.attempts.store(0, Ordering::Relaxed);
    }

    /// Check if retries are exhausted
    pub fn is_exhausted(&self) -> bool {
        self.attempts() >= self.config.max_retries
    }

    /// Evaluate success checks and decide whether to retry.
    ///
    /// Returns:
    /// - `RetryOutcome::Success` if all checks pass
    /// - `RetryOutcome::Retried(n)` if a retry is needed (after running `on_failure` hook)
    /// - `RetryOutcome::MaxReached` if max retries exhausted
    pub async fn evaluate(&self) -> Result<RetryOutcome> {
        // If no checks configured, consider it a success
        if !self.config.has_checks() {
            return Ok(RetryOutcome::Success);
        }

        // Run success checks
        let success = run_success_checks(
            &self.config.checks,
            Duration::from_secs(self.config.check_timeout_secs),
        )
        .await?;

        if success {
            info!("All success checks passed");
            return Ok(RetryOutcome::Success);
        }

        let current = self.attempts();

        // Check if max retries reached
        if current >= self.config.max_retries {
            warn!(
                "Maximum retry attempts ({}) exceeded",
                self.config.max_retries
            );
            return Ok(RetryOutcome::MaxReached);
        }

        // Run on-failure hook if configured
        if let Some(ref cmd) = self.config.on_failure {
            info!("Running on-failure cleanup: {cmd}");
            run_shell_command(
                cmd,
                Duration::from_secs(self.config.on_failure_timeout_secs),
            )
            .await?;
        }

        // Increment and report retry
        let new_attempt = self.attempts.fetch_add(1, Ordering::Relaxed) + 1;
        info!(
            "Retrying (attempt {}/{})",
            new_attempt, self.config.max_retries
        );

        Ok(RetryOutcome::Retried(new_attempt))
    }
}

/// Run all success checks, returning true if all pass
pub async fn run_success_checks(checks: &[SuccessCheck], timeout: Duration) -> Result<bool> {
    for check in checks {
        match check {
            SuccessCheck::Shell { command } => {
                let output = run_shell_command(command, timeout).await?;
                if !output.status.success() {
                    warn!(
                        "Success check failed: '{}' exited with {}, stderr: {}",
                        command,
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    );
                    return Ok(false);
                }
                debug!("Success check passed: '{command}'");
            }
        }
    }
    Ok(true)
}

/// Execute a shell command with timeout
pub async fn run_shell_command(command: &str, timeout: Duration) -> Result<std::process::Output> {
    debug!("Running shell command (timeout {timeout:?}): {command}");

    let future = async {
        if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null());
            crate::subprocess::configure_subprocess(&mut cmd);
            cmd.output().await
        } else {
            let mut cmd = Command::new(crate::subprocess::SHELL_INFO.binary);
            cmd.args([crate::subprocess::SHELL_INFO.exec_flag, command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null());
            crate::subprocess::configure_subprocess(&mut cmd);
            cmd.output().await
        }
    };

    let result = tokio::time::timeout(timeout, future).await;

    match result {
        Ok(Ok(output)) => {
            debug!(
                "Command completed: status={}, stdout={} bytes, stderr={} bytes",
                output.status,
                output.stdout.len(),
                output.stderr.len()
            );
            Ok(output)
        }
        Ok(Err(e)) => Err(anyhow!("Command execution failed: {e}")),
        Err(_) => Err(anyhow!("Command timed out after {timeout:?}: {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_builder() {
        let config = TaskRetryConfig::new(5)
            .with_check(SuccessCheck::shell("test -f output.txt"))
            .with_check(SuccessCheck::shell("grep success output.txt"))
            .with_on_failure("rm -f output.txt")
            .with_timeout_secs(120);

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.checks.len(), 2);
        assert_eq!(config.on_failure.as_deref(), Some("rm -f output.txt"));
        assert_eq!(config.check_timeout_secs, 120);
        assert!(config.has_checks());
    }

    #[test]
    fn test_retry_config_no_checks() {
        let config = TaskRetryConfig::new(3);
        assert!(!config.has_checks());
    }

    #[test]
    fn test_retry_manager_new() {
        let config = TaskRetryConfig::new(3);
        let manager = TaskRetryManager::new(config);
        assert_eq!(manager.attempts(), 0);
        assert!(!manager.is_exhausted());
    }

    #[test]
    fn test_retry_manager_exhausted() {
        let config = TaskRetryConfig::new(2);
        let manager = TaskRetryManager::new(config);
        assert!(!manager.is_exhausted());

        manager.attempts.fetch_add(2, Ordering::Relaxed);
        assert!(manager.is_exhausted());
    }

    #[test]
    fn test_retry_manager_reset() {
        let config = TaskRetryConfig::new(3);
        let manager = TaskRetryManager::new(config);
        manager.attempts.fetch_add(2, Ordering::Relaxed);
        assert_eq!(manager.attempts(), 2);

        manager.reset();
        assert_eq!(manager.attempts(), 0);
    }

    #[tokio::test]
    async fn test_evaluate_no_checks_returns_success() {
        let config = TaskRetryConfig::new(3);
        let manager = TaskRetryManager::new(config);

        let outcome = manager.evaluate().await.unwrap();
        assert_eq!(outcome, RetryOutcome::Success);
    }

    #[tokio::test]
    async fn test_evaluate_checks_pass() {
        let config = TaskRetryConfig::new(3).with_check(SuccessCheck::shell("true"));
        let manager = TaskRetryManager::new(config);

        let outcome = manager.evaluate().await.unwrap();
        assert_eq!(outcome, RetryOutcome::Success);
    }

    #[tokio::test]
    async fn test_evaluate_checks_fail_then_retry() {
        let config = TaskRetryConfig::new(3).with_check(SuccessCheck::shell("false"));
        let manager = TaskRetryManager::new(config);

        let outcome = manager.evaluate().await.unwrap();
        assert!(matches!(outcome, RetryOutcome::Retried(1)));
        assert_eq!(manager.attempts(), 1);
    }

    #[tokio::test]
    async fn test_evaluate_max_retries() {
        let config = TaskRetryConfig::new(2).with_check(SuccessCheck::shell("false"));
        let manager = TaskRetryManager::new(config);

        // Exhaust attempts
        manager.attempts.fetch_add(2, Ordering::Relaxed);

        let outcome = manager.evaluate().await.unwrap();
        assert_eq!(outcome, RetryOutcome::MaxReached);
    }

    #[tokio::test]
    async fn test_run_shell_command_success() {
        let output = run_shell_command("echo hello", Duration::from_secs(10))
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }

    #[tokio::test]
    async fn test_run_shell_command_failure() {
        let output = run_shell_command("false", Duration::from_secs(10))
            .await
            .unwrap();
        assert!(!output.status.success());
    }

    #[tokio::test]
    async fn test_run_shell_command_timeout() {
        let result = run_shell_command("sleep 10", Duration::from_millis(100)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_success_checks_all_pass() {
        let checks = vec![SuccessCheck::shell("true"), SuccessCheck::shell("echo ok")];
        let result = run_success_checks(&checks, Duration::from_secs(10))
            .await
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_success_checks_one_fails() {
        let checks = vec![SuccessCheck::shell("true"), SuccessCheck::shell("false")];
        let result = run_success_checks(&checks, Duration::from_secs(10))
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_on_failure_hook_runs() {
        let config = TaskRetryConfig::new(3)
            .with_check(SuccessCheck::shell("false"))
            .with_on_failure("echo cleanup");
        let manager = TaskRetryManager::new(config);

        let outcome = manager.evaluate().await.unwrap();
        assert!(matches!(outcome, RetryOutcome::Retried(1)));
    }

    #[tokio::test]
    async fn test_on_failure_hook_nonzero_still_retries() {
        // When on_failure cleanup command exits non-zero, retry still proceeds
        let config = TaskRetryConfig::new(3)
            .with_check(SuccessCheck::shell("false"))
            .with_on_failure("false"); // cleanup exits non-zero
        let manager = TaskRetryManager::new(config);

        let outcome = manager.evaluate().await.unwrap();
        // Retry should still happen even if cleanup failed
        assert!(matches!(outcome, RetryOutcome::Retried(1)));
    }

    #[test]
    fn test_retry_outcome_equality() {
        assert_eq!(RetryOutcome::Success, RetryOutcome::Success);
        assert_eq!(RetryOutcome::MaxReached, RetryOutcome::MaxReached);
        assert_eq!(RetryOutcome::Retried(1), RetryOutcome::Retried(1));
        assert_ne!(RetryOutcome::Success, RetryOutcome::MaxReached);
        assert_ne!(RetryOutcome::Retried(1), RetryOutcome::Retried(2));
    }
}
