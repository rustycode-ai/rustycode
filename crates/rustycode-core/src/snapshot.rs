//! Shadow git snapshot system for undo/revert of agent actions.
//!
//! Uses a separate hidden git repo (`.rustycode/snapshot/`) to track file
//! state without interfering with the user's own git history. Before each
//! agent action, a snapshot is taken. On revert, files are restored from
//! the snapshot.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A snapshot of file state at a point in time, identified by a git tree hash.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The git tree hash representing the file state
    pub hash: String,
}

/// Per-file diff between two snapshots.
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

/// Manages shadow git snapshots for a working directory.
pub struct SnapshotManager {
    worktree: PathBuf,
}

impl SnapshotManager {
    pub fn new(worktree: PathBuf) -> Self {
        Self { worktree }
    }

    /// Take a snapshot of the current file state.
    ///
    /// Returns a `Snapshot` with the tree hash that can be used to restore later.
    /// Creates the shadow git repo if it doesn't exist.
    pub fn track(&self) -> Result<Snapshot> {
        let git_dir = self.ensure_repo()?;
        self.git_add_all(&git_dir)?;
        let output = self.git_cmd(&git_dir, &["write-tree"])?;
        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Snapshot { hash })
    }

    /// Restore files to a previous snapshot state.
    ///
    /// Only restores files that were changed; leaves unrelated files untouched.
    pub fn restore(&self, snapshot: &Snapshot) -> Result<()> {
        let git_dir = self.ensure_repo()?;
        self.git_cmd(&git_dir, &["read-tree", &snapshot.hash])?;
        self.git_cmd(&git_dir, &["checkout-index", "-a", "-f"])?;
        Ok(())
    }

    /// Revert specific files from a set of patches.
    ///
    /// For each patch, restores files to the state in that patch's snapshot hash.
    pub fn revert_files(&self, patches: &[SnapshotPatch]) -> Result<()> {
        let git_dir = self.ensure_repo()?;
        let mut seen = HashSet::new();

        for patch in patches {
            for file in &patch.files {
                let relative = if Path::new(file).is_absolute() {
                    Path::new(file)
                        .strip_prefix(&self.worktree)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    file.clone()
                };

                if relative.is_empty() || !seen.insert(relative.clone()) {
                    continue;
                }

                // Try to checkout the file from the snapshot
                let checkout = self.git_cmd(&git_dir, &["checkout", &patch.hash, "--", &relative]);

                if checkout.is_err() {
                    // If file doesn't exist in snapshot, delete it locally
                    let exists = self
                        .git_cmd(&git_dir, &["ls-tree", &patch.hash, "--", &relative])
                        .ok()
                        .map(|out| !String::from_utf8_lossy(&out.stdout).trim().is_empty())
                        .unwrap_or(false);

                    if !exists {
                        let absolute = self.worktree.join(&relative);
                        if let Err(e) = fs::remove_file(&absolute) {
                            tracing::debug!(path = %absolute.display(), error = %e, "failed to remove stale snapshot file");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Compute diff between a snapshot and the current state.
    pub fn diff(&self, from_hash: &str) -> Result<Vec<FileDiff>> {
        let git_dir = self.ensure_repo()?;
        self.git_add_all(&git_dir)?;

        let output = self.git_cmd(
            &git_dir,
            &[
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.quotepath=false",
                "diff",
                "--no-ext-diff",
                "--numstat",
                from_hash,
                "--",
                ".",
            ],
        );

        let mut diffs = Vec::new();
        if let Ok(output) = output {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if line.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3 {
                    let is_binary = parts[0] == "-" && parts[1] == "-";
                    let additions = if is_binary {
                        0
                    } else {
                        parts[0].parse::<u64>().unwrap_or(0)
                    };
                    let deletions = if is_binary {
                        0
                    } else {
                        parts[1].parse::<u64>().unwrap_or(0)
                    };
                    diffs.push(FileDiff {
                        path: parts[2].to_string(),
                        additions,
                        deletions,
                    });
                }
            }
        }

        Ok(diffs)
    }

    /// Check if the shadow git repo exists.
    pub fn exists(&self) -> bool {
        self.git_dir().join("HEAD").exists()
    }

    /// Get the path to the shadow git directory.
    fn git_dir(&self) -> PathBuf {
        self.worktree.join(".rustycode").join("snapshot")
    }

    /// Ensure the shadow git repo exists, creating it if needed.
    fn ensure_repo(&self) -> Result<PathBuf> {
        let git_dir = self.git_dir();
        let parent = git_dir.parent().context("snapshot dir has no parent")?;
        fs::create_dir_all(parent)?;

        if !git_dir.join("HEAD").exists() {
            let output = Command::new("git")
                .arg("init")
                .arg("--quiet")
                .current_dir(&self.worktree)
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", &self.worktree)
                .output()
                .context("failed to init snapshot repo")?;

            if !output.status.success() {
                anyhow::bail!(
                    "failed to init snapshot repo: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }

            // Stable line endings
            if let Err(e) = self.git_cmd(&git_dir, &["config", "core.autocrlf", "false"]) {
                tracing::debug!(error = %e, "failed to set core.autocrlf in snapshot repo");
            }
        }

        Ok(git_dir)
    }

    /// Run a git command against the shadow repo.
    fn git_cmd(&self, git_dir: &Path, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new("git")
            .arg("--git-dir")
            .arg(git_dir)
            .arg("--work-tree")
            .arg(&self.worktree)
            .args(args)
            .current_dir(&self.worktree)
            .output()
            .with_context(|| format!("git {}", args.join(" ")))?;

        if !output.status.success() {
            anyhow::bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(output)
    }

    /// Add all files to the shadow repo index, excluding the snapshot dir itself.
    fn git_add_all(&self, git_dir: &Path) -> Result<()> {
        // Try multiple exclusion patterns (different git versions handle them differently)
        for args in [
            vec!["add", "-A", "--", ".", ":(exclude).rustycode/snapshot"],
            vec!["add", "-A", "--", ".", ":!/.rustycode/snapshot"],
            vec!["add", "-A", "--", ".", ":!.rustycode/snapshot"],
        ] {
            if self.git_cmd(git_dir, &args).is_ok() {
                return Ok(());
            }
        }

        anyhow::bail!("failed to add files to snapshot index")
    }
}

/// A patch that can be reverted — associates a snapshot hash with the files it changed.
#[derive(Debug, Clone)]
pub struct SnapshotPatch {
    pub hash: String,
    pub files: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        // Init a real git repo so snapshot can work
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(path)
            .output()
            .unwrap();

        tmp
    }

    #[test]
    fn test_snapshot_track_and_restore() {
        let tmp = setup_test_repo();
        let path = tmp.path();
        let mgr = SnapshotManager::new(path.to_path_buf());

        // Write initial file
        fs::write(path.join("hello.txt"), "hello world").unwrap();

        // Take snapshot
        let snap = mgr.track().unwrap();
        assert!(!snap.hash.is_empty());

        // Modify file
        fs::write(path.join("hello.txt"), "goodbye world").unwrap();
        assert_eq!(
            fs::read_to_string(path.join("hello.txt")).unwrap(),
            "goodbye world"
        );

        // Restore
        mgr.restore(&snap).unwrap();
        assert_eq!(
            fs::read_to_string(path.join("hello.txt")).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_snapshot_diff() {
        let tmp = setup_test_repo();
        let path = tmp.path();
        let mgr = SnapshotManager::new(path.to_path_buf());

        fs::write(path.join("a.txt"), "aaa\n").unwrap();
        let snap = mgr.track().unwrap();

        fs::write(path.join("a.txt"), "aaa\nbbb\n").unwrap();
        fs::write(path.join("b.txt"), "new file\n").unwrap();

        let diffs = mgr.diff(&snap.hash).unwrap();
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.path == "a.txt"));
        assert!(diffs.iter().any(|d| d.path == "b.txt"));
    }

    #[test]
    fn test_snapshot_revert_files() {
        let tmp = setup_test_repo();
        let path = tmp.path();
        let mgr = SnapshotManager::new(path.to_path_buf());

        fs::write(path.join("keep.txt"), "keep").unwrap();
        fs::write(path.join("change.txt"), "original").unwrap();
        let snap = mgr.track().unwrap();

        fs::write(path.join("change.txt"), "modified").unwrap();
        fs::write(path.join("new.txt"), "new file").unwrap();

        // Revert only change.txt
        mgr.revert_files(&[SnapshotPatch {
            hash: snap.hash.clone(),
            files: vec!["change.txt".to_string()],
        }])
        .unwrap();

        assert_eq!(
            fs::read_to_string(path.join("change.txt")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_to_string(path.join("new.txt")).unwrap(),
            "new file"
        ); // not reverted
    }

    #[test]
    fn test_snapshot_creates_hidden_repo() {
        let tmp = setup_test_repo();
        let path = tmp.path();
        let mgr = SnapshotManager::new(path.to_path_buf());

        assert!(!mgr.exists());
        mgr.track().unwrap();
        assert!(mgr.exists());
        assert!(path.join(".rustycode/snapshot/HEAD").exists());
    }

    #[test]
    fn test_snapshot_multiple_tracks() {
        let tmp = setup_test_repo();
        let path = tmp.path();
        let mgr = SnapshotManager::new(path.to_path_buf());

        fs::write(path.join("f.txt"), "v1").unwrap();
        let snap1 = mgr.track().unwrap();

        fs::write(path.join("f.txt"), "v2").unwrap();
        let snap2 = mgr.track().unwrap();

        // Hashes should differ
        assert_ne!(snap1.hash, snap2.hash);

        // Can restore to either
        mgr.restore(&snap1).unwrap();
        assert_eq!(fs::read_to_string(path.join("f.txt")).unwrap(), "v1");

        mgr.restore(&snap2).unwrap();
        assert_eq!(fs::read_to_string(path.join("f.txt")).unwrap(), "v2");
    }
}
