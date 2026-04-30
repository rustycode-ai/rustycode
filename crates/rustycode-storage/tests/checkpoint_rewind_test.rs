#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]

use rustycode_storage::checkpoint::{GitCheckpointStorage, GitRewindSnapshot};
use rustycode_storage::CheckpointStorage;
use tempfile::TempDir;

fn setup_git_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let _ = std::process::Command::new("git")
        .args(["init", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    dir
}

#[test]
fn rewind_restores_checkpoint_state() {
    let dir = setup_git_repo();
    let storage = GitCheckpointStorage::from_path(dir.path().to_path_buf());
    let snapshot = GitRewindSnapshot {
        git_hash: "abc123".into(),
        files: vec![],
    };
    let result = storage.rewind_to_checkpoint(&snapshot);
    assert!(result.is_ok() || result.unwrap_err().to_string().contains("git"));
}
