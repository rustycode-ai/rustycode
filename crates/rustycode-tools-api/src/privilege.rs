//! Privilege gating trait for tool execution authorization.
//!
//! Complements the existing `ToolGate` (role-based) and `ToolPermission` enum
//! with a path/command-level authorization interface. Implementations wrap
//! existing `permission_classifier` and `security.rs` logic.

use std::path::Path;

/// Trait for checking whether an operation is permitted on a specific target.
///
/// Unlike `ToolGate` which checks role→tool access, `PrivilegeGate` checks
/// whether a specific path or command is allowed within the current session's
/// sandbox configuration.
pub trait PrivilegeGate: Send + Sync {
    /// Check if reading the given path is allowed.
    fn can_read(&self, path: &Path) -> bool;

    /// Check if writing to the given path is allowed.
    fn can_write(&self, path: &Path) -> bool;

    /// Check if executing the given command is allowed.
    fn can_execute(&self, cmd: &str) -> bool;
}

/// A permissive gate that allows everything. Useful for tests and
/// unrestricted sessions.
pub struct AllowAllGate;

impl PrivilegeGate for AllowAllGate {
    fn can_read(&self, _path: &Path) -> bool {
        true
    }
    fn can_write(&self, _path: &Path) -> bool {
        true
    }
    fn can_execute(&self, _cmd: &str) -> bool {
        true
    }
}

/// A deny-all gate used when sandboxing is maximally restrictive.
pub struct DenyAllGate;

impl PrivilegeGate for DenyAllGate {
    fn can_read(&self, _path: &Path) -> bool {
        false
    }
    fn can_write(&self, _path: &Path) -> bool {
        false
    }
    fn can_execute(&self, _cmd: &str) -> bool {
        false
    }
}

/// A workspace-bounded gate that allows operations only within a root directory.
pub struct WorkspaceGate {
    root: std::path::PathBuf,
}

impl WorkspaceGate {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn is_within(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}

impl PrivilegeGate for WorkspaceGate {
    fn can_read(&self, path: &Path) -> bool {
        self.is_within(path)
    }
    fn can_write(&self, path: &Path) -> bool {
        self.is_within(path)
    }
    fn can_execute(&self, _cmd: &str) -> bool {
        true // command execution is governed by ToolGate, not path-based
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_all_permits_everything() {
        let gate = AllowAllGate;
        assert!(gate.can_read(Path::new("/etc/passwd")));
        assert!(gate.can_write(Path::new("/etc/passwd")));
        assert!(gate.can_execute("rm -rf /"));
    }

    #[test]
    fn deny_all_blocks_everything() {
        let gate = DenyAllGate;
        assert!(!gate.can_read(Path::new("/tmp")));
        assert!(!gate.can_write(Path::new("/tmp")));
        assert!(!gate.can_execute("ls"));
    }

    #[test]
    fn workspace_gate_allows_within_root() {
        let gate = WorkspaceGate::new("/workspace");
        assert!(gate.can_read(Path::new("/workspace/src/main.rs")));
        assert!(gate.can_write(Path::new("/workspace/src/main.rs")));
        assert!(!gate.can_read(Path::new("/etc/passwd")));
        assert!(!gate.can_write(Path::new("/etc/passwd")));
    }
}
