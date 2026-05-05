//! Integration tests for checkpoint and recovery flow
//!
//! These tests validate the complete end-to-end checkpoint creation, recovery trigger
//! detection, and successful rewind operations using git repositories and dangerous
//! operation detection.

use anyhow::Result;
use rustycode_core::checkpoint_detector::ExecutionCheckpointDetector;
use rustycode_core::recovery::CheckpointRecovery;
use std::fs;
use tempfile::TempDir;

// Test helpers

/// Create a test git repository with initial commit
fn create_test_repo() -> Result<(TempDir, String)> {
    let temp = TempDir::new()?;
    let repo_path = temp.path().to_string_lossy().to_string();

    // Initialize git repo
    std::process::Command::new("git")
        .args(&["init"])
        .current_dir(&repo_path)
        .output()?;

    // Configure git user for commits
    std::process::Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(&repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(&repo_path)
        .output()?;

    Ok((temp, repo_path))
}

/// Helper to create a commit in the test repository
fn create_commit(repo_path: &str, filename: &str, content: &str) -> Result<()> {
    let file_path = format!("{}/{}", repo_path, filename);
    fs::write(&file_path, content)?;

    std::process::Command::new("git")
        .args(&["add", filename])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(&["commit", "-m", &format!("Add {}", filename)])
        .current_dir(repo_path)
        .output()?;

    Ok(())
}

// Integration Tests (5 core tests)

/// Test the complete checkpoint recovery flow from creation through rewind.
///
/// This test exercises:
/// 1. Repository initialization
/// 2. Creating an initial commit
/// 3. Checkpoint creation
/// 4. Dangerous operation detection
/// 5. File modification
/// 6. Successful rewind to checkpoint
#[tokio::test]
async fn test_full_checkpoint_recovery_flow() -> Result<()> {
    let (_temp, repo_path) = create_test_repo()?;

    // Create initial commit with important file
    let file_path = format!("{}/important.txt", repo_path);
    create_commit(&repo_path, "important.txt", "important data")?;

    // Create checkpoint at initial state
    let recovery = CheckpointRecovery::new(&repo_path)?;
    let checkpoint = recovery.create()?;
    assert!(!checkpoint.id.is_empty());
    assert!(checkpoint.checkpoint_git_hash.is_some());
    assert!(checkpoint.checkpoint_created_at.is_some());

    // Verify dangerous operation detection
    let dangerous_command = "rm important.txt";
    assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
        dangerous_command
    ));
    assert!(ExecutionCheckpointDetector::checkpoint_reason(dangerous_command).is_some());

    // Simulate file modification after checkpoint
    fs::write(&file_path, "deleted content")?;
    assert_eq!(fs::read_to_string(&file_path)?, "deleted content");

    // Verify we can rewind to checkpoint
    recovery.rewind(&checkpoint).await?;

    // File should be restored to original state
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, "important data");

    Ok(())
}

/// Test detection of multiple dangerous operations.
///
/// This test verifies that the checkpoint detector correctly identifies
/// a comprehensive set of dangerous operations that should trigger automatic
/// checkpoint creation.
#[test]
fn test_dangerous_operations_detected() {
    let dangerous_operations = vec![
        // File deletion
        "rm important.txt",
        "rm -rf /tmp",
        // Git destructive operations
        "git reset --hard origin/main",
        "git reset --hard HEAD",
        "git clean -fd",
        "git push --force origin master",
        "git push -f origin main",
        // Database operations
        "drop table users;",
        "delete from users;",
        "truncate table logs;",
    ];

    for cmd in &dangerous_operations {
        assert!(
            ExecutionCheckpointDetector::should_checkpoint_before_step(cmd),
            "Failed to detect dangerous operation: {}",
            cmd
        );

        let reason = ExecutionCheckpointDetector::checkpoint_reason(cmd);
        assert!(
            reason.is_some(),
            "No checkpoint reason generated for: {}",
            cmd
        );

        let reason_str = reason.unwrap();
        assert!(
            reason_str.contains("Dangerous operation detected"),
            "Reason does not mention dangerous operation: {}",
            reason_str
        );
    }
}

/// Test that safe operations are not incorrectly flagged as dangerous.
///
/// This test verifies that the checkpoint detector correctly allows
/// safe operations to proceed without triggering checkpoint creation.
#[test]
fn test_safe_operations_not_detected() {
    let safe_operations = vec![
        // Read operations
        "echo hello",
        "ls -la",
        "cat file.txt",
        "grep pattern file.txt",
        // Git safe operations
        "git status",
        "git add .",
        "git commit -m 'message'",
        "git log",
        "git diff",
        // Other safe operations
        "cargo build",
        "cargo test",
        "mkdir newdir",
        "touch newfile.txt",
    ];

    for cmd in &safe_operations {
        assert!(
            !ExecutionCheckpointDetector::should_checkpoint_before_step(cmd),
            "Incorrectly flagged as dangerous: {}",
            cmd
        );

        let reason = ExecutionCheckpointDetector::checkpoint_reason(cmd);
        assert!(
            reason.is_none(),
            "Incorrectly generated checkpoint reason for safe operation: {}",
            cmd
        );
    }
}

/// Test creating multiple sequential checkpoints and verifying state tracking.
///
/// This test exercises:
/// 1. Creating multiple checkpoints across state changes
/// 2. Verifying checkpoint isolation
/// 3. State tracking across multiple checkpoints
#[tokio::test]
async fn test_multiple_checkpoints() -> Result<()> {
    let (_temp, repo_path) = create_test_repo()?;

    // Create initial commit
    let file_path = format!("{}/counter.txt", repo_path);
    create_commit(&repo_path, "counter.txt", "state 0")?;

    let recovery = CheckpointRecovery::new(&repo_path)?;

    // Create initial checkpoint
    let checkpoint_0 = recovery.create()?;
    let hash_0 = checkpoint_0
        .checkpoint_git_hash
        .clone()
        .expect("checkpoint should have git hash");

    // Modify and create second checkpoint
    fs::write(&file_path, "state 1")?;
    std::process::Command::new("git")
        .args(&["add", "counter.txt"])
        .current_dir(&repo_path)
        .output()?;
    std::process::Command::new("git")
        .args(&["commit", "-m", "update to state 1"])
        .current_dir(&repo_path)
        .output()?;

    let checkpoint_1 = recovery.create()?;
    let hash_1 = checkpoint_1
        .checkpoint_git_hash
        .clone()
        .expect("checkpoint should have git hash");

    // Modify and create third checkpoint
    fs::write(&file_path, "state 2")?;
    std::process::Command::new("git")
        .args(&["add", "counter.txt"])
        .current_dir(&repo_path)
        .output()?;
    std::process::Command::new("git")
        .args(&["commit", "-m", "update to state 2"])
        .current_dir(&repo_path)
        .output()?;

    let checkpoint_2 = recovery.create()?;

    // Verify all checkpoints have unique hashes
    assert_ne!(hash_0, hash_1);
    assert_ne!(
        hash_1,
        checkpoint_2.checkpoint_git_hash.as_ref().unwrap().as_str()
    );

    // Verify all checkpoints have unique IDs
    assert_ne!(checkpoint_0.id, checkpoint_1.id);
    assert_ne!(checkpoint_1.id, checkpoint_2.id);

    // Rewind to first checkpoint and verify state
    recovery.rewind(&checkpoint_0).await?;
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, "state 0");

    // Rewind to second checkpoint and verify state
    recovery.rewind(&checkpoint_1).await?;
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, "state 1");

    // Rewind to third checkpoint and verify state
    recovery.rewind(&checkpoint_2).await?;
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, "state 2");

    Ok(())
}

/// Test checkpoint reason generation for detailed logging.
///
/// This test verifies that checkpoint reasons are properly generated
/// with clear, actionable messages that include the command and context.
#[test]
fn test_checkpoint_reason_generation() {
    let dangerous_commands = vec![
        "rm important.txt",
        "git reset --hard HEAD",
        "git clean -fd",
        "git push --force origin",
    ];

    for cmd in dangerous_commands {
        let reason = ExecutionCheckpointDetector::checkpoint_reason(cmd);
        assert!(reason.is_some(), "No reason for: {}", cmd);

        let reason_str = reason.unwrap();
        assert!(
            reason_str.contains("Dangerous operation"),
            "Reason should mention 'Dangerous operation': {}",
            reason_str
        );
        assert!(
            reason_str.contains(cmd),
            "Reason should include the command: {}",
            reason_str
        );
        assert!(
            reason_str.contains("checkpoint"),
            "Reason should mention 'checkpoint': {}",
            reason_str
        );
    }

    // Verify safe operations return None
    assert!(ExecutionCheckpointDetector::checkpoint_reason("echo hello").is_none());
    assert!(ExecutionCheckpointDetector::checkpoint_reason("ls -la").is_none());
}
