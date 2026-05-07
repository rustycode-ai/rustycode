//! Resource safety and concurrency guard layer.
//!
//! Prevents agents from interfering with one another by implementing advisory
//! file-locking and resource tracking. The [`LockManager`] tracks which
//! resources (files, directories, commands) are currently held by which agent,
//! and [`ResourceGuard`] provides RAII-style lock release.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// The type of access required for a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceAccess {
    /// Read-only access. Multiple readers can hold this simultaneously.
    Read,
    /// Exclusive write access. Only one holder at a time.
    Write,
    /// Execution access (e.g., running a command that modifies state).
    Exec,
}

/// A resource that can be locked.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    /// A file or directory path.
    Path(PathBuf),
    /// A named lock (e.g., "database", "build").
    Named(String),
}

impl Resource {
    /// Create a Path resource.
    pub fn path<P: AsRef<Path>>(p: P) -> Self {
        Self::Path(p.as_ref().to_path_buf())
    }

    /// Create a Named resource.
    pub fn named(name: &str) -> Self {
        Self::Named(name.to_string())
    }
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(p) => write!(f, "path:{}", p.display()),
            Self::Named(n) => write!(f, "named:{n}"),
        }
    }
}

/// Declares what resources a step needs and at what access level.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequiredResources {
    pub resources: Vec<(Resource, ResourceAccess)>,
}

impl RequiredResources {
    pub const fn new() -> Self {
        Self { resources: vec![] }
    }

    pub fn read<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.resources
            .push((Resource::path(path), ResourceAccess::Read));
        self
    }

    pub fn write<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.resources
            .push((Resource::path(path), ResourceAccess::Write));
        self
    }

    pub fn exec(mut self, name: &str) -> Self {
        self.resources
            .push((Resource::named(name), ResourceAccess::Exec));
        self
    }
}

/// Record of who holds a lock on what resource.
#[derive(Debug, Clone)]
#[allow(dead_code)] // acquired_at used for diagnostics
struct LockRecord {
    holder: String,
    access: ResourceAccess,
    acquired_at: chrono::DateTime<chrono::Utc>,
}

/// Conflict resolution strategy when a resource is already locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// Wait for the lock to be released (with a timeout).
    Wait,
    /// Abandon this step and report the conflict.
    Abandon,
}

/// Result of a lock acquisition attempt.
#[derive(Debug)]
pub enum LockResult {
    /// All resources were acquired successfully.
    Acquired(ResourceGuard),
    /// One or more resources are held by another agent.
    Conflicted { resource: Resource, holder: String },
}

/// RAII guard that releases all held resources when dropped.
#[derive(Debug)]
pub struct ResourceGuard {
    manager: Arc<Mutex<LockManagerInner>>,
    holder: String,
    acquired: Vec<(Resource, ResourceAccess)>,
    file_locks: Vec<File>, // Held for fs2 advisory locks
}

impl ResourceGuard {
    /// Which agent owns this guard.
    pub fn holder(&self) -> &str {
        &self.holder
    }

    /// What resources are currently held.
    pub fn held_resources(&self) -> &[(Resource, ResourceAccess)] {
        &self.acquired
    }
}

impl Drop for ResourceGuard {
    #[allow(clippy::significant_drop_tightening)]
    fn drop(&mut self) {
        let mut inner = self
            .manager
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (resource, _) in &self.acquired {
            inner.locks.remove(resource);
        }
        // File locks are released when File is dropped
        self.file_locks.clear();
        tracing::debug!(holder = %self.holder, count = self.acquired.len(), "ResourceGuard dropped, locks released");
    }
}

/// Inner state of the lock manager, protected by a Mutex.
#[derive(Debug, Default)]
struct LockManagerInner {
    locks: HashMap<Resource, LockRecord>,
}

/// Manages resource locks across concurrent agents.
///
/// Thread-safe via `Arc<Mutex<...>>`. Advisory file locks use `fs2` for
/// cross-process safety.
#[derive(Debug, Clone)]
pub struct LockManager {
    inner: Arc<Mutex<LockManagerInner>>,
    lock_dir: PathBuf,
}

impl LockManager {
    pub fn new(lock_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LockManagerInner::default())),
            lock_dir,
        }
    }

    /// Create a `LockManager` using a temp directory for lock files.
    pub fn in_memory() -> Self {
        Self::new(std::env::temp_dir().join("rustycode-orchestration-locks"))
    }

    /// Try to acquire all required resources for a holder.
    ///
    /// Returns `Acquired` on success or `Conflicted` if any resource is held
    /// exclusively by another agent.
    #[allow(clippy::significant_drop_tightening)]
    pub fn try_acquire(&self, holder: &str, requirements: &RequiredResources) -> LockResult {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = chrono::Utc::now();

        // Phase 1: Check for conflicts
        for (resource, access) in &requirements.resources {
            if let Some(record) = inner.locks.get(resource) {
                // Read access can share with other readers
                if *access == ResourceAccess::Read && record.access == ResourceAccess::Read {
                    continue;
                }
                // Any other combination conflicts
                if record.holder != holder {
                    return LockResult::Conflicted {
                        resource: resource.clone(),
                        holder: record.holder.clone(),
                    };
                }
            }
        }

        // Phase 2: Acquire file locks for Path resources that need Write/Exec
        let mut file_locks = Vec::new();
        for (resource, access) in &requirements.resources {
            if let Resource::Path(p) = resource {
                if *access == ResourceAccess::Write || *access == ResourceAccess::Exec {
                    if let Some(parent) = p.parent() {
                        if let Err(e) = std::fs::create_dir_all(self.lock_dir.join(parent)) {
                            tracing::warn!("Failed to create lock directory: {e}");
                        }
                    }
                    let lock_path = self.lock_dir.join(p).with_extension("lock");
                    if let Some(parent) = lock_path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            tracing::warn!("Failed to create lock directory: {e}");
                        }
                    }
                    match File::create(&lock_path) {
                        Ok(f) => {
                            if fs2::FileExt::try_lock_exclusive(&f).is_err() {
                                return LockResult::Conflicted {
                                    resource: resource.clone(),
                                    holder: "external-process".into(),
                                };
                            }
                            file_locks.push(f);
                        }
                        Err(_) => {
                            // If we can't create the lock file, proceed without
                            // advisory file lock (in-memory tracking still works)
                            tracing::warn!(path = %lock_path.display(), "Could not create advisory lock file");
                        }
                    }
                }
            }
        }

        // Phase 3: Record all acquisitions
        for (resource, access) in &requirements.resources {
            inner.locks.insert(
                resource.clone(),
                LockRecord {
                    holder: holder.to_string(),
                    access: *access,
                    acquired_at: now,
                },
            );
        }

        let acquired = requirements.resources.clone();
        let guard = ResourceGuard {
            manager: self.inner.clone(),
            holder: holder.to_string(),
            acquired,
            file_locks,
        };

        tracing::debug!(holder, count = guard.acquired.len(), "Resources acquired");
        LockResult::Acquired(guard)
    }

    /// Check if a resource is currently locked by anyone.
    pub fn is_locked(&self, resource: &Resource) -> Option<String> {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.locks.get(resource).map(|r| r.holder.clone())
    }

    /// List all currently held locks.
    pub fn held_locks(&self) -> Vec<(Resource, String, ResourceAccess)> {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner
            .locks
            .iter()
            .map(|(r, rec)| (r.clone(), rec.holder.clone(), rec.access))
            .collect()
    }

    /// Force-release all locks held by a specific agent (e.g., on crash recovery).
    pub fn force_release_all(&self, holder: &str) -> usize {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = inner.locks.len();
        inner.locks.retain(|_, rec| rec.holder != holder);
        before - inner.locks.len()
    }
}

/// Broadcasts resource acquisition intent before locking.
///
/// Agents publish their intent via the bus so other agents can detect
/// potential conflicts early and reorder their work to avoid deadlocks.
pub struct IntentSignaler {
    bus: crate::bus::BusHandle,
}

impl IntentSignaler {
    pub const fn new(bus: crate::bus::BusHandle) -> Self {
        Self { bus }
    }

    /// Signal intent to acquire resources. Other agents receive this
    /// as a `ResourceIntent` event and can adjust their plans.
    pub fn signal_intent(&self, holder: &str, requirements: &RequiredResources) {
        if requirements.resources.is_empty() {
            return;
        }
        self.bus
            .publish(crate::bus::OrchestrationEvent::ResourceIntent {
                holder: holder.to_string(),
                resources: requirements.resources.clone(),
            });
    }
}

/// Listens for resource conflict events and reports them.
pub struct ConflictResolver {
    bus: crate::bus::BusHandle,
}

/// A detected conflict between two agents.
#[derive(Debug, Clone)]
pub struct ConflictReport {
    pub holder: String,
    pub resource: Resource,
    pub conflict_with: String,
}

impl ConflictResolver {
    pub const fn new(bus: crate::bus::BusHandle) -> Self {
        Self { bus }
    }

    /// Publish a conflict event to the bus.
    pub fn report_conflict(&self, holder: &str, resource: &Resource, conflict_with: &str) {
        self.bus
            .publish(crate::bus::OrchestrationEvent::ResourceConflict {
                holder: holder.to_string(),
                resource: resource.clone(),
                conflict_with: conflict_with.to_string(),
            });
    }

    /// Collect recent conflict events from the bus.
    pub fn drain_conflicts(
        &self,
        rx: &mut tokio::sync::broadcast::Receiver<crate::bus::OrchestrationEvent>,
    ) -> Vec<ConflictReport> {
        let mut conflicts = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let crate::bus::OrchestrationEvent::ResourceConflict {
                holder,
                resource,
                conflict_with,
            } = event
            {
                conflicts.push(ConflictReport {
                    holder,
                    resource,
                    conflict_with,
                });
            }
        }
        conflicts
    }
}

/// Manages isolated git worktrees for concurrent agent work.
///
/// Each agent claiming a write path gets assigned to a worktree branch,
/// preventing interference with other agents' in-progress changes.
pub struct WorktreeManager {
    lock_manager: LockManager,
    base_dir: PathBuf,
}

impl WorktreeManager {
    pub const fn new(base_dir: PathBuf, lock_manager: LockManager) -> Self {
        Self {
            lock_manager,
            base_dir,
        }
    }

    /// Assign an agent to an isolated workspace for the given paths.
    ///
    /// Returns a guard that releases the workspace assignment when dropped.
    /// Returns `Conflicted` if another agent already holds write access to
    /// any of the specified paths.
    pub fn assign_workspace(&self, agent: &str, paths: &[PathBuf]) -> LockResult {
        let mut rr = RequiredResources::new();
        for p in paths {
            rr = rr.write(p);
        }
        // Also lock a named resource for the worktree itself
        rr = rr.exec(&format!("worktree-{agent}"));
        self.lock_manager.try_acquire(agent, &rr)
    }

    /// The base directory for worktree clones.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Check which agents currently have workspaces assigned.
    pub fn active_agents(&self) -> Vec<String> {
        let locks = self.lock_manager.held_locks();
        let mut agents: Vec<String> = locks
            .iter()
            .filter(|(_, _, access)| *access == ResourceAccess::Exec)
            .filter_map(|(r, holder, _)| {
                if let Resource::Named(n) = r {
                    if n.starts_with("worktree-") {
                        return Some(holder.clone());
                    }
                }
                None
            })
            .collect();
        agents.sort();
        agents.dedup();
        agents
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::match_wildcard_for_single_variants,
    clippy::manual_let_else,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_clone,
    clippy::needless_borrow,
    clippy::uninlined_format_args,
    clippy::similar_names,
    clippy::items_after_statements,
    clippy::collection_is_never_read
)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_display() {
        assert_eq!(
            Resource::path("/tmp/file.rs").to_string(),
            "path:/tmp/file.rs"
        );
        assert_eq!(Resource::named("database").to_string(), "named:database");
    }

    #[test]
    fn test_required_resources_builder() {
        let rr = RequiredResources::new()
            .read("/tmp/input.rs")
            .write("/tmp/output.rs")
            .exec("cargo-build");
        assert_eq!(rr.resources.len(), 3);
        assert_eq!(rr.resources[0].1, ResourceAccess::Read);
        assert_eq!(rr.resources[1].1, ResourceAccess::Write);
        assert_eq!(rr.resources[2].1, ResourceAccess::Exec);
    }

    #[test]
    fn test_lock_manager_acquire_read() {
        let mgr = LockManager::in_memory();
        let rr = RequiredResources::new().read("/tmp/guard-test-read.rs");
        let result = mgr.try_acquire("agent-1", &rr);
        assert!(matches!(result, LockResult::Acquired(_)));
        assert!(mgr
            .is_locked(&Resource::path("/tmp/guard-test-read.rs"))
            .is_some());
    }

    #[test]
    fn test_lock_guard_releases_on_drop() {
        let mgr = LockManager::in_memory();
        let rr = RequiredResources::new().read("/tmp/guard-test-release.rs");
        {
            let _guard = mgr.try_acquire("agent-1", &rr);
            assert!(mgr
                .is_locked(&Resource::path("/tmp/guard-test-release.rs"))
                .is_some());
        }
        assert!(mgr
            .is_locked(&Resource::path("/tmp/guard-test-release.rs"))
            .is_none());
    }

    #[test]
    fn test_multiple_readers_allowed() {
        let mgr = LockManager::in_memory();
        let rr1 = RequiredResources::new().read("/tmp/guard-test-shared.rs");
        let rr2 = RequiredResources::new().read("/tmp/guard-test-shared.rs");
        let _g1 = match mgr.try_acquire("reader-1", &rr1) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let result = mgr.try_acquire("reader-2", &rr2);
        assert!(matches!(result, LockResult::Acquired(_)));
    }

    #[test]
    fn test_write_excludes_readers() {
        let mgr = LockManager::in_memory();
        let rr_write = RequiredResources::new().write("/tmp/guard-test-excl-read.rs");
        let rr_read = RequiredResources::new().read("/tmp/guard-test-excl-read.rs");
        let _g1 = match mgr.try_acquire("writer-1", &rr_write) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let result = mgr.try_acquire("reader-1", &rr_read);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_write_excludes_other_writers() {
        let mgr = LockManager::in_memory();
        let path = format!("/tmp/guard-test-excl-write-{}.rs", uuid::Uuid::new_v4());
        let rr1 = RequiredResources::new().write(&path);
        let rr2 = RequiredResources::new().write(&path);
        let _g1 = match mgr.try_acquire("writer-1", &rr1) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let result = mgr.try_acquire("writer-2", &rr2);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_conflict_reports_holder() {
        let mgr = LockManager::in_memory();
        let path = format!("/tmp/guard-test-conflict-{}.rs", uuid::Uuid::new_v4());
        let rr1 = RequiredResources::new().write(&path);
        let rr2 = RequiredResources::new().read(&path);
        let _g1 = match mgr.try_acquire("agent-A", &rr1) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        if let LockResult::Conflicted { resource, holder } = mgr.try_acquire("agent-B", &rr2) {
            assert_eq!(holder, "agent-A");
            assert_eq!(resource, Resource::path(&path));
        } else {
            panic!("Expected Conflicted");
        }
    }

    #[test]
    fn test_force_release_without_drop() {
        let mgr = LockManager::in_memory();
        let path = format!("/tmp/guard-test-orphan-{}.rs", uuid::Uuid::new_v4());
        let rr = RequiredResources::new().write(&path);
        let guard = match mgr.try_acquire("crashed-agent", &rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        std::mem::forget(guard);
        assert!(mgr.is_locked(&Resource::path(&path)).is_some());
        let released = mgr.force_release_all("crashed-agent");
        assert_eq!(released, 1);
        assert!(mgr.is_locked(&Resource::path(&path)).is_none());
    }

    #[test]
    fn test_held_locks() {
        let mgr = LockManager::in_memory();
        let rr = RequiredResources::new()
            .read("/tmp/guard-test-held-a.rs")
            .write("/tmp/guard-test-held-b.rs");
        let _g = match mgr.try_acquire("agent-1", &rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let locks = mgr.held_locks();
        assert_eq!(locks.len(), 2);
    }

    #[test]
    fn test_named_resource() {
        let mgr = LockManager::in_memory();
        let rr = RequiredResources::new().exec("cargo-build");
        let _g = match mgr.try_acquire("agent-1", &rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        assert!(mgr.is_locked(&Resource::named("cargo-build")).is_some());
    }

    #[test]
    fn test_multiple_resources_partial_conflict() {
        let mgr = LockManager::in_memory();
        let path = format!("/tmp/guard-test-partial-{}.rs", uuid::Uuid::new_v4());
        let rr1 = RequiredResources::new().write(&path);
        let rr2 = RequiredResources::new()
            .read("/tmp/guard-test-partial-other.rs")
            .write(&path);
        let _g1 = match mgr.try_acquire("agent-1", &rr1) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let result = mgr.try_acquire("agent-2", &rr2);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_intent_signaler_publishes_event() {
        let bus = crate::bus::BusHandle::new(16);
        let mut rx = bus.subscribe();
        let signaler = IntentSignaler::new(bus);
        let rr = RequiredResources::new()
            .write("/tmp/intent-test.rs")
            .exec("build");
        signaler.signal_intent("agent-1", &rr);
        let event = rx.try_recv().unwrap();
        match event {
            crate::bus::OrchestrationEvent::ResourceIntent { holder, resources } => {
                assert_eq!(holder, "agent-1");
                assert_eq!(resources.len(), 2);
            }
            _ => panic!("Expected ResourceIntent event"),
        }
    }

    #[test]
    fn test_intent_signaler_skips_empty() {
        let bus = crate::bus::BusHandle::new(16);
        let mut rx = bus.subscribe();
        let signaler = IntentSignaler::new(bus);
        let rr = RequiredResources::new();
        signaler.signal_intent("agent-1", &rr);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_conflict_resolver_reports_and_drains() {
        let bus = crate::bus::BusHandle::new(16);
        let mut rx = bus.subscribe();
        let resolver = ConflictResolver::new(bus.clone());
        resolver.report_conflict("agent-1", &Resource::path("/tmp/conflict.rs"), "agent-2");
        let conflicts = resolver.drain_conflicts(&mut rx);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].holder, "agent-1");
        assert_eq!(conflicts[0].conflict_with, "agent-2");
    }

    #[test]
    fn test_worktree_manager_assign() {
        let lm = LockManager::in_memory();
        let wtm = WorktreeManager::new(PathBuf::from("/tmp/worktrees"), lm);
        let paths = vec![PathBuf::from("/tmp/agent-work/file.rs")];
        let result = wtm.assign_workspace("agent-1", &paths);
        assert!(matches!(result, LockResult::Acquired(_)));
    }

    #[test]
    fn test_worktree_manager_conflict() {
        let lm = LockManager::in_memory();
        let wtm = WorktreeManager::new(PathBuf::from("/tmp/worktrees"), lm);
        let paths = vec![PathBuf::from("/tmp/shared/file.rs")];
        let _g = match wtm.assign_workspace("agent-1", &paths) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let result = wtm.assign_workspace("agent-2", &paths);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_worktree_manager_active_agents() {
        let lm = LockManager::in_memory();
        let wtm = WorktreeManager::new(PathBuf::from("/tmp/worktrees"), lm);
        let paths1 = vec![PathBuf::from("/tmp/a.rs")];
        let paths2 = vec![PathBuf::from("/tmp/b.rs")];
        let _g1 = match wtm.assign_workspace("agent-1", &paths1) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let _g2 = match wtm.assign_workspace("agent-2", &paths2) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let agents = wtm.active_agents();
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&"agent-1".to_string()));
        assert!(agents.contains(&"agent-2".to_string()));
    }

    #[test]
    fn test_worktree_manager_release_on_drop() {
        let lm = LockManager::in_memory();
        let wtm = WorktreeManager::new(PathBuf::from("/tmp/worktrees"), lm);
        let paths = vec![PathBuf::from("/tmp/release-test.rs")];
        {
            let _g = wtm.assign_workspace("agent-1", &paths);
            assert_eq!(wtm.active_agents().len(), 1);
        }
        assert!(wtm.active_agents().is_empty());
    }

    // ─── Stress & Edge Case Tests ────────────────────────────────────

    #[test]
    fn test_lock_manager_many_concurrent_readers() {
        let lm = LockManager::in_memory();
        let mut guards = Vec::new();

        for i in 0..50 {
            let req = RequiredResources::new().read("/shared.rs");
            match lm.try_acquire(&format!("reader-{i}"), &req) {
                LockResult::Acquired(g) => guards.push(g),
                other => panic!("Should allow concurrent reads, got {:?}", other),
            }
        }
        assert_eq!(guards.len(), 50);
        drop(guards);
        // After releasing, a writer should succeed
        let req = RequiredResources::new().write("/shared.rs");
        assert!(matches!(
            lm.try_acquire("writer", &req),
            LockResult::Acquired(_)
        ));
    }

    #[test]
    fn test_lock_manager_acquire_release_cycle() {
        let lm = LockManager::in_memory();

        for i in 0..20 {
            let req = RequiredResources::new().write("/cycle.rs");
            let guard = lm.try_acquire(&format!("agent-{i}"), &req);
            assert!(matches!(guard, LockResult::Acquired(_)));
        }
    }

    #[test]
    fn test_conflict_resolver_drain_multiple() {
        let bus = crate::bus::BusHandle::new(64);
        let resolver = ConflictResolver::new(bus.clone());
        let mut rx = bus.subscribe();

        resolver.report_conflict("agent-1", &Resource::path("/contested.rs"), "agent-2");
        resolver.report_conflict("agent-1", &Resource::path("/contested.rs"), "agent-3");
        resolver.report_conflict("agent-4", &Resource::path("/other.rs"), "agent-5");

        let reports = resolver.drain_conflicts(&mut rx);
        assert_eq!(reports.len(), 3);
    }

    #[test]
    fn test_worktree_manager_many_agents() {
        let lm = LockManager::in_memory();
        let wtm = WorktreeManager::new(PathBuf::from("/tmp/worktrees"), lm);

        let mut guards = Vec::new();
        for i in 0..20 {
            let paths = vec![PathBuf::from(format!("/tmp/agent-{i}/src/main.rs"))];
            let g = wtm.assign_workspace(&format!("agent-{i}"), &paths);
            guards.push(g);
        }
        assert_eq!(wtm.active_agents().len(), 20);
    }

    #[test]
    fn test_resource_equality_and_hashing() {
        use std::collections::HashSet;
        let r1 = Resource::path("/same.rs");
        let r2 = Resource::path("/same.rs");
        let r3 = Resource::path("/different.rs");
        let r4 = Resource::named("build");

        assert_eq!(r1, r2);
        assert_ne!(r1, r3);
        assert_ne!(r1, r4);

        let mut set = HashSet::new();
        set.insert(r1.clone());
        set.insert(r2);
        assert_eq!(set.len(), 1);
        set.insert(r3);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_required_resources_builder_chaining() {
        let resources = RequiredResources::new()
            .read("/read.rs")
            .write("/write.rs")
            .exec("build");

        assert_eq!(resources.resources.len(), 3);
        assert_eq!(resources.resources[0].1, ResourceAccess::Read);
        assert_eq!(resources.resources[1].1, ResourceAccess::Write);
        assert_eq!(resources.resources[2].1, ResourceAccess::Exec);
    }

    #[test]
    fn test_lock_manager_named_resource_exclusion() {
        let lm = LockManager::in_memory();
        let req1 = RequiredResources::new().exec("database");
        let g1 = lm.try_acquire("agent-1", &req1);
        assert!(matches!(g1, LockResult::Acquired(_)));

        let req2 = RequiredResources::new().exec("database");
        let g2 = lm.try_acquire("agent-2", &req2);
        assert!(matches!(g2, LockResult::Conflicted { .. }));
    }
}
