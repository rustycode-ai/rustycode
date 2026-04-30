#![allow(
    clippy::redundant_clone,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]

use rustycode_storage::checkpoint::{GitCheckpointStorage, GitRewindSnapshot};
use rustycode_storage::CheckpointStorage;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

fn run_cmd(args: &[&str], dir: &std::path::Path) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command failed");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn file_specific_rewind_restores_files_only() {
    let dir = TempDir::new().unwrap();
    let repo_path = dir.path();

    // init repo
    let _ = run_cmd(&["init", repo_path.to_str().unwrap()], repo_path);

    // create file v1 and commit
    let file_path = repo_path.join("foo.txt");
    let mut f = File::create(&file_path).unwrap();
    writeln!(f, "v1").unwrap();
    let _ = run_cmd(
        &["-C", repo_path.to_str().unwrap(), "add", "foo.txt"],
        repo_path,
    );
    let _ = run_cmd(
        &[
            "-C",
            repo_path.to_str().unwrap(),
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "v1",
        ],
        repo_path,
    );

    // capture first commit
    let first = run_cmd(
        &["-C", repo_path.to_str().unwrap(), "rev-parse", "HEAD"],
        repo_path,
    );
    let first = first.trim().to_string();

    // modify file to v2 and commit
    fs::write(&file_path, "v2\n").unwrap();
    let _ = run_cmd(
        &["-C", repo_path.to_str().unwrap(), "add", "foo.txt"],
        repo_path,
    );
    let _ = run_cmd(
        &[
            "-C",
            repo_path.to_str().unwrap(),
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "v2",
        ],
        repo_path,
    );

    // sanity: file now v2
    let now = fs::read_to_string(&file_path).unwrap();
    assert!(now.contains("v2"));

    // perform file-specific rewind to first commit
    let storage = GitCheckpointStorage::from_path(repo_path.to_path_buf());
    let snapshot = GitRewindSnapshot {
        git_hash: first.clone(),
        files: vec!["foo.txt".to_string()],
    };
    storage
        .rewind_to_checkpoint(&snapshot)
        .expect("rewind failed");

    // file should be restored to v1
    let after = fs::read_to_string(&file_path).unwrap();
    assert!(after.contains("v1"), "file content not restored: {}", after);
}
