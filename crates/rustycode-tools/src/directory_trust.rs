//! Hierarchical directory trust system.
//!
//! When a user trusts a directory, all subdirectories inherit that trust.
//! Trust stops at git repository boundaries — trusting a parent outside the
//! git root does NOT extend into the repo.
//!
//! Two trust levels:
//! - **Session**: Trust lasts only for the current session (in-memory)
//! - **Persistent**: Trust is saved and survives restarts (file-backed)

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Whether trust is session-scoped or persistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TrustLevel {
    /// Trust lasts only for the current session.
    Session,
    /// Trust is persisted to disk and survives restarts.
    Persistent,
}

/// A single trust entry recording that a directory was trusted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    /// The directory path that was trusted (canonical).
    pub path: PathBuf,
    /// When the trust was granted.
    pub granted_at: chrono::DateTime<chrono::Utc>,
    /// Session or persistent.
    pub level: TrustLevel,
}

/// Manages hierarchical directory trust with git-boundary awareness.
pub struct DirectoryTrust {
    /// Session-only trusted directories.
    session_trusted: HashSet<PathBuf>,
    /// Persistently trusted directories (loaded from disk).
    persistent_trusted: HashSet<PathBuf>,
    /// Git repository root (trust boundary). If None, no boundary enforcement.
    git_root: Option<PathBuf>,
    /// Cache of trust lookups for performance.
    cache: std::cell::RefCell<HashSet<PathBuf>>,
}

impl DirectoryTrust {
    /// Create a new trust manager for the given working directory.
    ///
    /// Automatically detects the git root as a trust boundary.
    pub fn new(cwd: &Path) -> Self {
        let git_root = detect_git_root(cwd);
        Self {
            session_trusted: HashSet::new(),
            persistent_trusted: HashSet::new(),
            git_root,
            cache: std::cell::RefCell::new(HashSet::new()),
        }
    }

    /// Create with an explicit git root (for testing).
    pub fn with_git_root(cwd: &Path, git_root: Option<PathBuf>) -> Self {
        let _ = cwd;
        Self {
            session_trusted: HashSet::new(),
            persistent_trusted: HashSet::new(),
            git_root,
            cache: std::cell::RefCell::new(HashSet::new()),
        }
    }

    /// Trust a directory (and all its subdirectories) for the given level.
    pub fn trust(&mut self, path: &Path, level: TrustLevel) {
        // Store the path as-is; canonicalization is handled at comparison time
        let key = path.to_path_buf();
        match level {
            TrustLevel::Session => {
                self.session_trusted.insert(key.clone());
            }
            TrustLevel::Persistent => {
                self.persistent_trusted.insert(key.clone());
            }
        }
        // Invalidate cache since trust changed
        self.cache.borrow_mut().clear();
    }

    /// Revoke trust for a directory.
    pub fn revoke(&mut self, path: &Path) {
        self.session_trusted.remove(path);
        self.persistent_trusted.remove(path);
        self.cache.borrow_mut().clear();
    }

    /// Check if a path is trusted (either directly or via parent inheritance).
    ///
    /// Trust propagates downward: if `/home/user/project` is trusted, then
    /// `/home/user/project/src` is also trusted. Trust does NOT cross git
    /// boundaries.
    pub fn is_trusted(&self, path: &Path) -> bool {
        // Check cache first (using display form for cache key)
        if self.cache.borrow().contains(&path.to_path_buf()) {
            return true;
        }

        let result = self.check_trust(path);
        if result {
            self.cache.borrow_mut().insert(path.to_path_buf());
        }
        result
    }

    fn check_trust(&self, path: &Path) -> bool {
        // Check both session and persistent trust sets
        let all_trusted: Vec<&PathBuf> = self
            .session_trusted
            .iter()
            .chain(self.persistent_trusted.iter())
            .collect();

        for trusted_dir in &all_trusted {
            if self.path_inherits_trust(trusted_dir, path) {
                // Trust inherited — but check git boundary
                if self.respects_git_boundary(trusted_dir, path) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if `path` is inside or equal to `trusted_dir`.
    ///
    /// Handles macOS symlink differences (/var → /private/var) by trying
    /// multiple canonicalization strategies.
    fn path_inherits_trust(&self, trusted_dir: &Path, path: &Path) -> bool {
        // Strategy 1: Direct comparison (works when both use same path form)
        if path == trusted_dir || path.starts_with(trusted_dir) {
            return true;
        }

        // Strategy 2: Canonicalize both (works when both exist on disk)
        if let (Ok(canonical_trusted), Ok(canonical_path)) = (
            std::fs::canonicalize(trusted_dir),
            std::fs::canonicalize(path),
        ) {
            if canonical_path == canonical_trusted || canonical_path.starts_with(&canonical_trusted)
            {
                return true;
            }
        }

        // Strategy 3: Handle macOS /var → /private/var symlink issue.
        // The trusted dir was stored raw (e.g., from tempdir which may return
        // /var/... on macOS). canonicalize resolves it to /private/var/...
        // The child path was built by joining onto the raw parent, so it also
        // uses /var/... We need to check if child starts with raw trusted dir.
        //
        // Since Strategy 1 already checked this, the remaining case is:
        // trusted_dir was canonicalized elsewhere but path uses non-canonical form.
        // Solution: canonicalize trusted_dir, then check if the path starts with
        // it by resolving path's parent chain and comparing.
        if let Ok(canonical_trusted) = std::fs::canonicalize(trusted_dir) {
            // Try canonicalizing path's existing prefix
            if let Ok(canonical_path) = std::fs::canonicalize(path) {
                if canonical_path == canonical_trusted
                    || canonical_path.starts_with(&canonical_trusted)
                {
                    return true;
                }
            }

            // Path may not exist — walk up to find the deepest existing ancestor
            // and check if it's under the canonical trusted dir
            let mut current = path.to_path_buf();
            let mut suffix = PathBuf::new();
            loop {
                if current == canonical_trusted || current.starts_with(&canonical_trusted) {
                    return true;
                }
                if let Ok(canonical_current) = std::fs::canonicalize(&current) {
                    if canonical_current == canonical_trusted
                        || canonical_current.starts_with(&canonical_trusted)
                    {
                        return true;
                    }
                }
                match (current.file_name(), current.parent()) {
                    (Some(name), Some(parent)) => {
                        suffix = PathBuf::from(name).join(&suffix);
                        current = parent.to_path_buf();
                    }
                    _ => break,
                }
            }
        }

        false
    }

    /// Check if trust propagation from `trusted` to `target` respects the
    /// git boundary. Trust does NOT cross git roots.
    fn respects_git_boundary(&self, trusted: &Path, target: &Path) -> bool {
        match &self.git_root {
            None => true, // No git root — no boundary
            Some(root) => {
                let trusted_in_repo = self.is_within(trusted, root);
                let target_in_repo = self.is_within(target, root);

                match (trusted_in_repo, target_in_repo) {
                    (true, true) => true,   // Both in repo — trust propagates
                    (false, false) => true, // Both outside — trust propagates
                    (true, false) => false, // Trusted in repo, target outside — stop at boundary
                    (false, true) => false, // Trusted outside repo, target inside — stop at boundary
                }
            }
        }
    }

    /// Check if a path is within a given root (handling canonicalization).
    fn is_within(&self, path: &Path, root: &Path) -> bool {
        if path.starts_with(root) || path == root {
            return true;
        }
        // Handle macOS /private/tmp vs /tmp symlinks
        if let (Ok(canonical_path), Ok(canonical_root)) =
            (std::fs::canonicalize(path), std::fs::canonicalize(root))
        {
            canonical_path.starts_with(&canonical_root) || canonical_path == canonical_root
        } else {
            false
        }
    }

    /// List all trusted directories.
    pub fn trusted_directories(&self) -> Vec<TrustEntry> {
        let now = chrono::Utc::now();
        let mut entries = Vec::new();
        for path in &self.session_trusted {
            entries.push(TrustEntry {
                path: path.clone(),
                granted_at: now,
                level: TrustLevel::Session,
            });
        }
        for path in &self.persistent_trusted {
            entries.push(TrustEntry {
                path: path.clone(),
                granted_at: now,
                level: TrustLevel::Persistent,
            });
        }
        entries
    }

    /// Get the git root boundary, if detected.
    pub fn git_root(&self) -> Option<&Path> {
        self.git_root.as_deref()
    }

    /// Clear all session trust (e.g., when session ends).
    pub fn clear_session(&mut self) {
        self.session_trusted.clear();
        self.cache.borrow_mut().clear();
    }

    /// Load persistent trust entries from a file.
    pub fn load_persistent(&mut self, path: &Path) -> Result<(), TrustError> {
        if !path.exists() {
            return Ok(());
        }
        let content =
            std::fs::read_to_string(path).map_err(|e| TrustError::Io(path.to_path_buf(), e))?;
        let entries: Vec<TrustEntry> = serde_json::from_str(&content)
            .map_err(|e| TrustError::Parse(path.to_path_buf(), e.to_string()))?;

        for entry in entries {
            if entry.level == TrustLevel::Persistent {
                self.persistent_trusted.insert(entry.path);
            }
        }
        self.cache.borrow_mut().clear();
        Ok(())
    }

    /// Save persistent trust entries to a file.
    pub fn save_persistent(&self, path: &Path) -> Result<(), TrustError> {
        let entries: Vec<TrustEntry> = self
            .persistent_trusted
            .iter()
            .map(|p| TrustEntry {
                path: p.clone(),
                granted_at: chrono::Utc::now(),
                level: TrustLevel::Persistent,
            })
            .collect();

        let content = serde_json::to_string_pretty(&entries)
            .map_err(|e| TrustError::Serialize(e.to_string()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| TrustError::Io(parent.to_path_buf(), e))?;
        }

        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, &content).map_err(|e| TrustError::Io(tmp_path.clone(), e))?;
        std::fs::rename(&tmp_path, path).map_err(|e| TrustError::Io(path.to_path_buf(), e))?;

        Ok(())
    }
}

/// Detect the git repository root from the current directory.
fn detect_git_root(path: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return None;
    }

    Some(PathBuf::from(root))
}

/// Errors for trust operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TrustError {
    #[error("IO error for {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("Parse error for {0}: {1}")]
    Parse(PathBuf, String),
    #[error("Serialization error: {0}")]
    Serialize(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trust(git_root: Option<&Path>) -> DirectoryTrust {
        DirectoryTrust::with_git_root(
            std::env::current_dir().unwrap().as_path(),
            git_root.map(|p| p.to_path_buf()),
        )
    }

    #[test]
    fn direct_trust_works() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut trust = make_trust(None);
        trust.trust(dir, TrustLevel::Session);
        assert!(trust.is_trusted(dir));
    }

    #[test]
    fn child_inherits_trust() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let child = parent.join("src").join("module");

        let mut trust = make_trust(None);
        trust.trust(parent, TrustLevel::Session);

        assert!(trust.is_trusted(&child));
    }

    #[test]
    fn unrelated_dir_not_trusted() {
        let tmp = tempfile::tempdir().unwrap();
        let trusted = tmp.path();
        let other = tempfile::tempdir().unwrap();

        let mut trust = make_trust(None);
        trust.trust(trusted, TrustLevel::Session);

        assert!(!trust.is_trusted(other.path()));
    }

    #[test]
    fn trust_stops_at_git_boundary() {
        let repo = tempfile::tempdir().unwrap();
        let git_root = repo.path();

        // Create a git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(git_root)
            .output()
            .unwrap();

        let outside = repo.path().parent().unwrap();

        let mut trust = make_trust(Some(git_root));
        trust.trust(outside, TrustLevel::Session);

        // Trusting outside should NOT extend into the repo
        assert!(!trust.is_trusted(git_root));
    }

    #[test]
    fn trust_propagates_within_repo() {
        let repo = tempfile::tempdir().unwrap();
        let git_root = repo.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(git_root)
            .output()
            .unwrap();

        let src_dir = git_root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let mut trust = make_trust(Some(git_root));
        trust.trust(git_root, TrustLevel::Session);

        // Trusting the git root propagates to children within repo
        assert!(trust.is_trusted(&src_dir));
    }

    #[test]
    fn revoke_removes_trust() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let mut trust = make_trust(None);
        trust.trust(dir, TrustLevel::Session);
        assert!(trust.is_trusted(dir));

        trust.revoke(dir);
        assert!(!trust.is_trusted(dir));
    }

    #[test]
    fn clear_session_keeps_persistent() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let mut trust = make_trust(None);
        trust.trust(dir, TrustLevel::Persistent);
        trust.clear_session();

        // Persistent trust survives session clear
        assert!(trust.is_trusted(dir));
    }

    #[test]
    fn persistent_trust_survives_save_load() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let trust_file = tmp.path().join("trust.json");

        // Save
        let mut trust = make_trust(None);
        trust.trust(dir, TrustLevel::Persistent);
        trust.save_persistent(&trust_file).unwrap();

        // Load into fresh instance
        let mut trust2 = make_trust(None);
        trust2.load_persistent(&trust_file).unwrap();
        assert!(trust2.is_trusted(dir));
    }

    #[test]
    fn session_and_persistent_both_work() {
        let tmp = tempfile::tempdir().unwrap();
        let dir1 = tmp.path().join("session_dir");
        let dir2 = tmp.path().join("persistent_dir");
        std::fs::create_dir_all(&dir1).unwrap();
        std::fs::create_dir_all(&dir2).unwrap();

        let mut trust = make_trust(None);
        trust.trust(&dir1, TrustLevel::Session);
        trust.trust(&dir2, TrustLevel::Persistent);

        assert!(trust.is_trusted(&dir1));
        assert!(trust.is_trusted(&dir2));
    }
}
