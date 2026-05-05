//! Checkpoint trigger detection for dangerous operations.
//!
//! This module detects dangerous operations (like `rm`, `git reset --hard`, etc.)
//! that should trigger automatic checkpoint creation before execution.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_core::checkpoint_detector::ExecutionCheckpointDetector;
//!
//! let step_command = "rm -rf /path";
//! let should_checkpoint = ExecutionCheckpointDetector::should_checkpoint_before_step(&step_command);
//! assert!(should_checkpoint);
//!
//! if let Some(reason) = ExecutionCheckpointDetector::checkpoint_reason(&step_command) {
//!     println!("Checkpoint needed: {}", reason);
//! }
//! ```

/// Operations that trigger automatic checkpoint creation
const DANGEROUS_OPERATIONS: &[&str] = &[
    // File deletion
    "rm ",
    "rmdir",
    // Git destructive
    "git reset --hard",
    "git clean",
    "git push --force",
    "git push -f",
    // Disk operations
    "dd if=",
    "dd of=",
    "> /dev/",
    // Database/data destructive
    "drop table",
    "delete from",
    "truncate",
];

/// Detects dangerous operations and triggers checkpoint creation.
pub struct ExecutionCheckpointDetector;

impl ExecutionCheckpointDetector {
    /// Check if a command should trigger checkpoint creation
    ///
    pub fn should_checkpoint_before_step(command: &str) -> bool {
        let command = command.trim();

        DANGEROUS_OPERATIONS
            .iter()
            .any(|danger| command.contains(danger))
    }

    /// Get checkpoint reason if a dangerous operation is detected
    ///
    pub fn checkpoint_reason(command: &str) -> Option<String> {
        let command = command.trim();

        for danger in DANGEROUS_OPERATIONS {
            if command.contains(danger) {
                return Some(format!(
                    "Dangerous operation detected: '{}' - creating checkpoint before execution",
                    command
                ));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_rm_operations() {
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "rm -rf /path/to/dir"
        ));
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "rm important.txt"
        ));
    }

    #[test]
    fn test_detects_git_reset_hard() {
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git reset --hard origin/main"
        ));
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git reset --hard HEAD~1"
        ));
    }

    #[test]
    fn test_detects_git_clean() {
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git clean -fd"
        ));
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git clean -x"
        ));
    }

    #[test]
    fn test_detects_git_push_force() {
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git push --force origin main"
        ));
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git push -f origin main"
        ));
    }

    #[test]
    fn test_detects_rmdir() {
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "rmdir /path"
        ));
    }

    #[test]
    fn test_allows_safe_operations() {
        assert!(!ExecutionCheckpointDetector::should_checkpoint_before_step(
            "echo hello"
        ));
        assert!(!ExecutionCheckpointDetector::should_checkpoint_before_step(
            "ls -la"
        ));
        assert!(!ExecutionCheckpointDetector::should_checkpoint_before_step(
            "cat file.txt"
        ));
        assert!(!ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git status"
        ));
        assert!(!ExecutionCheckpointDetector::should_checkpoint_before_step(
            "git add ."
        ));
    }

    #[test]
    fn test_detects_multiple_dangerous_patterns() {
        let dangerous_commands = vec![
            "rm /tmp/file",
            "rm -rf /",
            "git reset --hard HEAD~1",
            "git clean -fd",
            "git push --force origin",
            "git push -f",
            "dd if=/dev/zero of=/dev/sda",
            "echo something > /dev/sda",
            "drop table users;",
            "delete from users;",
            "truncate table logs;",
            "rmdir empty_dir",
        ];

        for cmd in dangerous_commands {
            assert!(
                ExecutionCheckpointDetector::should_checkpoint_before_step(cmd),
                "Failed to detect dangerous operation: {}",
                cmd
            );
        }
    }

    #[test]
    fn test_checkpoint_reason_generation() {
        let cmd = "rm important.txt";
        let reason = ExecutionCheckpointDetector::checkpoint_reason(cmd);
        assert!(reason.is_some());
        let reason_str = reason.unwrap();
        assert!(reason_str.contains("rm important.txt"));
        assert!(reason_str.contains("checkpoint"));
    }

    #[test]
    fn test_checkpoint_reason_returns_none_for_safe_operations() {
        let cmd = "echo hello";
        let reason = ExecutionCheckpointDetector::checkpoint_reason(cmd);
        assert!(reason.is_none());
    }

    #[test]
    fn test_handles_whitespace() {
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "  rm -rf /path  "
        ));
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "\tgit reset --hard\t"
        ));
    }

    #[test]
    fn test_handles_quoted_commands() {
        // Even quoted commands should be detected
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "bash -c 'rm -rf /tmp'"
        ));
        assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
            "sh -c \"git reset --hard\""
        ));
    }
}
