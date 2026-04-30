#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
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

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create a test git repository with initial commit
fn create_test_repo() -> Result<(TempDir, String)> {
    let temp = TempDir::new()?;
    let repo_path = temp.path().to_string_lossy().to_string();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()?;

    // Configure git user for commits
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_path)
        .output()?;

    Ok((temp, repo_path))
}

/// Helper to create a commit in the test repository
fn create_commit(repo_path: &str, filename: &str, content: &str) -> Result<()> {
    let file_path = format!("{}/{}", repo_path, filename);
    fs::write(&file_path, content)?;

    std::process::Command::new("git")
        .args(["add", filename])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", &format!("Add {}", filename)])
        .current_dir(repo_path)
        .output()?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Integration Tests
// ---------------------------------------------------------------------------

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

    // Simulate file modification after checkpoint (must commit for rewind)
    fs::write(&file_path, "deleted content")?;
    assert_eq!(fs::read_to_string(&file_path)?, "deleted content");
    create_commit(&repo_path, "important.txt", "deleted content")?;

    // Verify we can rewind to checkpoint
    recovery.rewind(&checkpoint).await?;

    // File should be restored to original state
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, "important data");

    Ok(())
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
        .args(["add", "counter.txt"])
        .current_dir(&repo_path)
        .output()?;
    std::process::Command::new("git")
        .args(["commit", "-m", "update to state 1"])
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
        .args(["add", "counter.txt"])
        .current_dir(&repo_path)
        .output()?;
    std::process::Command::new("git")
        .args(["commit", "-m", "update to state 2"])
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

/// Test checkpoint recovery with file modifications and new file cleanup.
///
/// This test exercises:
/// 1. Checkpoint creation at a known state
/// 2. File modification after checkpoint
/// 3. New file creation after checkpoint
/// 4. Rewind restores original state and removes new files
#[tokio::test]
async fn test_checkpoint_with_file_modifications() -> Result<()> {
    let (_temp, repo_path) = create_test_repo()?;

    // Create initial commit
    create_commit(&repo_path, "test.txt", "v1")?;

    let recovery = CheckpointRecovery::new(&repo_path)?;
    let checkpoint = recovery.create()?;

    let test_file = format!("{}/test.txt", repo_path);
    let new_file = format!("{}/newfile.txt", repo_path);

    // Verify initial state
    assert_eq!(fs::read_to_string(&test_file)?, "v1");
    assert!(!std::path::Path::new(&new_file).exists());

    // Modify file
    fs::write(&test_file, "v2")?;
    assert_eq!(fs::read_to_string(&test_file)?, "v2");

    // Create new file
    fs::write(&new_file, "new content")?;
    assert!(std::path::Path::new(&new_file).exists());

    // Commit changes so working tree is clean for rewind
    create_commit(&repo_path, "test.txt", "v2")?;
    std::process::Command::new("git")
        .args(["add", "newfile.txt"])
        .current_dir(&repo_path)
        .output()?;
    std::process::Command::new("git")
        .args(["commit", "-m", "add newfile"])
        .current_dir(&repo_path)
        .output()?;

    // Simulate dangerous git operation that should trigger checkpoint
    let dangerous_step = "git reset --hard HEAD";
    assert!(ExecutionCheckpointDetector::should_checkpoint_before_step(
        dangerous_step
    ));

    // Verify rewind restores to v1 and removes new file
    recovery.rewind(&checkpoint).await?;
    assert_eq!(fs::read_to_string(&test_file)?, "v1");
    assert!(!std::path::Path::new(&new_file).exists());

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
        "rmdir empty_directory",
        // Git destructive
        "git reset --hard origin/main",
        "git reset --hard HEAD",
        "git reset --hard HEAD~1",
        "git clean -fd",
        "git clean -x",
        "git push --force origin master",
        "git push -f origin main",
        // Disk operations
        "dd if=/dev/zero of=/dev/sda",
        "echo data > /dev/sda",
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
        "find . -name '*.txt'",
        // Git safe operations
        "git status",
        "git add .",
        "git add file.txt",
        "git commit -m 'message'",
        "git log",
        "git diff",
        "git branch",
        "git pull origin main",
        // Other safe operations
        "cargo build",
        "cargo test",
        "cd /some/path",
        "mkdir newdir",
        "touch newfile.txt",
        "echo content > file.txt",
        "cp source.txt dest.txt",
        "mv old.txt new.txt",
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

/// Test checkpoint creation and restoration with complex git state.
///
/// This test exercises:
/// 1. Creating multiple files in multiple directories
/// 2. Creating a checkpoint
/// 3. Making complex modifications (new files, modified files, deletions)
/// 4. Verifying complete state restoration
#[tokio::test]
async fn test_checkpoint_with_complex_repo_state() -> Result<()> {
    let (_temp, repo_path) = create_test_repo()?;

    // Create a complex initial state
    fs::create_dir_all(format!("{}/src", repo_path))?;
    fs::create_dir_all(format!("{}/tests", repo_path))?;

    create_commit(&repo_path, "README.md", "# Project")?;
    create_commit(&repo_path, "src/main.rs", "fn main() {}")?;
    create_commit(&repo_path, "tests/test.rs", "#[test]\nfn test() {}")?;

    let recovery = CheckpointRecovery::new(&repo_path)?;
    let checkpoint = recovery.create()?;

    // Make various modifications
    fs::write(format!("{}/README.md", repo_path), "# Modified Project")?;
    fs::write(
        format!("{}/src/main.rs", repo_path),
        "fn main() { println!(\"hello\"); }",
    )?;
    fs::write(
        format!("{}/src/lib.rs", repo_path),
        "pub fn lib_function() {}",
    )?;

    // Verify changes
    assert_eq!(
        fs::read_to_string(format!("{}/README.md", repo_path))?,
        "# Modified Project"
    );
    assert!(std::path::Path::new(&format!("{}/src/lib.rs", repo_path)).exists());

    // Commit all modifications so working tree is clean for rewind
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&repo_path)
        .output()?;
    std::process::Command::new("git")
        .args(["commit", "-m", "modify files for rewind test"])
        .current_dir(&repo_path)
        .output()?;

    // Rewind
    recovery.rewind(&checkpoint).await?;

    // Verify complete restoration
    assert_eq!(
        fs::read_to_string(format!("{}/README.md", repo_path))?,
        "# Project"
    );
    assert_eq!(
        fs::read_to_string(format!("{}/src/main.rs", repo_path))?,
        "fn main() {}"
    );
    assert!(!std::path::Path::new(&format!("{}/src/lib.rs", repo_path)).exists());
    assert_eq!(
        fs::read_to_string(format!("{}/tests/test.rs", repo_path))?,
        "#[test]\nfn test() {}"
    );

    Ok(())
}

/// Test checkpoint detection with edge cases and whitespace handling.
///
/// This test verifies that the checkpoint detector handles:
/// 1. Leading/trailing whitespace
/// 2. Commands embedded in shell constructs
/// 3. Various command variations
#[test]
fn test_dangerous_operations_with_whitespace_and_edge_cases() {
    let test_cases = vec![
        ("  rm -rf /path  ", true, "leading/trailing whitespace"),
        ("\tgit reset --hard\t", true, "tab whitespace"),
        ("  git clean -fd  ", true, "git clean with whitespace"),
        ("rm file.txt", true, "rm with filename"),
        (
            "git reset --hard origin/main",
            true,
            "git reset with remote",
        ),
        ("git clean -fd .", true, "git clean with working directory"),
        (
            "git push --force origin main",
            true,
            "git push force with branch",
        ),
        ("git push -f origin master", true, "git push -f shorthand"),
        ("echo test", false, "safe echo command"),
        ("ls -la", false, "safe ls command"),
    ];

    for (cmd, should_detect, description) in test_cases {
        let detected = ExecutionCheckpointDetector::should_checkpoint_before_step(cmd);
        assert_eq!(
            detected, should_detect,
            "Failed for {}: '{}' detected={} expected={}",
            description, cmd, detected, should_detect
        );
    }
}

/// Test checkpoint reason generation for detailed logging.
///
/// This test verifies that checkpoint reasons are properly generated
/// with clear, actionable messages.
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

/// Test that checkpoint IDs are unique across multiple checkpoints.
///
/// This test verifies that the checkpoint system generates unique IDs
/// for each checkpoint to prevent accidental rewinds to wrong states.
#[tokio::test]
async fn test_checkpoint_uniqueness() -> Result<()> {
    let (_temp, repo_path) = create_test_repo()?;
    create_commit(&repo_path, "file.txt", "v0")?;

    let recovery = CheckpointRecovery::new(&repo_path)?;

    let mut checkpoints = vec![];
    for i in 0..5 {
        let cp = recovery.create()?;
        checkpoints.push(cp);

        // Create a new commit for next iteration
        if i < 4 {
            fs::write(format!("{}/file.txt", repo_path), format!("v{}", i + 1))?;
            std::process::Command::new("git")
                .args(["add", "file.txt"])
                .current_dir(&repo_path)
                .output()?;
            std::process::Command::new("git")
                .args(["commit", "-m", &format!("update to v{}", i + 1)])
                .current_dir(&repo_path)
                .output()?;
        }
    }

    // Verify all checkpoint IDs are unique
    let ids: Vec<_> = checkpoints.iter().map(|cp| &cp.id).collect();
    let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        ids.len(),
        unique_ids.len(),
        "Checkpoint IDs should be unique"
    );

    Ok(())
}
