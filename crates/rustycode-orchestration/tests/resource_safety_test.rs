//! Integration tests for resource safety & concurrency guard system.
//!
//! Tests end-to-end scenarios: LockManager, ResourceGuard, IntentSignaler,
//! ConflictResolver, WorktreeManager, and Musician resource integration.

#![allow(
    clippy::unwrap_used,
    clippy::match_wildcard_for_single_variants,
    clippy::manual_let_else,
    clippy::doc_markdown,
    clippy::uninlined_format_args,
    clippy::redundant_clone,
    clippy::items_after_statements
)]

use rustycode_orchestration::bus::{BusHandle, OrchestrationEvent};
use rustycode_orchestration::execution_trace::ExecutionTrace;
use rustycode_orchestration::guard::{
    ConflictResolver, IntentSignaler, LockManager, LockResult, RequiredResources, Resource,
    WorktreeManager,
};
use rustycode_orchestration::musician::Musician;
use rustycode_orchestration::types::{OutputType, Step};
use std::path::PathBuf;

// ─── 1. LockManager + ResourceGuard Lifecycle ────────────────────────────────

mod lock_lifecycle {
    use super::*;

    #[test]
    fn test_read_lock_shared_access() {
        let lm = LockManager::in_memory();
        let rr = RequiredResources::new()
            .read("/src/main.rs")
            .read("/src/lib.rs");

        let holder = "agent-1";
        let result = lm.try_acquire(holder, &rr);
        assert!(matches!(result, LockResult::Acquired(_)));

        // Drop the guard to release
        drop(result);

        // Should be releasable now
        let result2 = lm.try_acquire("agent-2", &rr);
        assert!(matches!(result2, LockResult::Acquired(_)));
    }

    #[test]
    fn test_write_lock_exclusive() {
        let lm = LockManager::in_memory();
        let rr = RequiredResources::new().write("/src/main.rs");

        let guard = match lm.try_acquire("agent-1", &rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        // Second write attempt should conflict
        let rr2 = RequiredResources::new().write("/src/main.rs");
        let result = lm.try_acquire("agent-2", &rr2);
        assert!(matches!(result, LockResult::Conflicted { .. }));
        drop(guard);

        // Now should succeed
        let result2 = lm.try_acquire("agent-2", &rr2);
        assert!(matches!(result2, LockResult::Acquired(_)));
    }

    #[test]
    fn test_read_blocks_write() {
        let lm = LockManager::in_memory();
        let read_rr = RequiredResources::new().read("/shared/config.toml");

        let _read_guard = match lm.try_acquire("reader-1", &read_rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        let write_rr = RequiredResources::new().write("/shared/config.toml");
        let result = lm.try_acquire("writer-1", &write_rr);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_write_blocks_read() {
        let lm = LockManager::in_memory();
        let write_rr = RequiredResources::new().write("/shared/data.json");

        let _write_guard = match lm.try_acquire("writer-1", &write_rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        let read_rr = RequiredResources::new().read("/shared/data.json");
        let result = lm.try_acquire("reader-1", &read_rr);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_multiple_reads_allowed() {
        let lm = LockManager::in_memory();
        let rr1 = RequiredResources::new().read("/src/lib.rs");
        let rr2 = RequiredResources::new().read("/src/lib.rs");
        let rr3 = RequiredResources::new().read("/src/lib.rs");

        let g1 = match lm.try_acquire("reader-1", &rr1) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let g2 = match lm.try_acquire("reader-2", &rr2) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let g3 = match lm.try_acquire("reader-3", &rr3) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        drop(g1);
        drop(g2);
        drop(g3);

        // Write should now succeed
        let write_rr = RequiredResources::new().write("/src/lib.rs");
        let result = lm.try_acquire("writer-1", &write_rr);
        assert!(matches!(result, LockResult::Acquired(_)));
    }

    #[test]
    fn test_exec_resource_exclusive() {
        let lm = LockManager::in_memory();
        let rr = RequiredResources::new().exec("/usr/bin/cargo");

        let _guard = match lm.try_acquire("runner-1", &rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        // Second exec attempt conflicts
        let rr2 = RequiredResources::new().exec("/usr/bin/cargo");
        let result = lm.try_acquire("runner-2", &rr2);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_multi_resource_acquire_and_release() {
        let lm = LockManager::in_memory();
        let rr = RequiredResources::new()
            .read("/a.rs")
            .read("/b.rs")
            .write("/c.rs");

        let guard = match lm.try_acquire("multi-agent", &rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        // Individual resources should be locked
        assert!(lm.is_locked(&Resource::path("/a.rs")).is_some());
        assert!(lm.is_locked(&Resource::path("/b.rs")).is_some());
        assert!(lm.is_locked(&Resource::path("/c.rs")).is_some());

        drop(guard);

        // All released
        assert!(lm.is_locked(&Resource::path("/a.rs")).is_none());
        assert!(lm.is_locked(&Resource::path("/b.rs")).is_none());
        assert!(lm.is_locked(&Resource::path("/c.rs")).is_none());
    }

    #[test]
    fn test_conflict_reports_holder() {
        let lm = LockManager::in_memory();
        let rr1 = RequiredResources::new().write("/locked.rs");
        let _g = match lm.try_acquire("owner", &rr1) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        let rr2 = RequiredResources::new().write("/locked.rs");
        match lm.try_acquire("contender", &rr2) {
            LockResult::Conflicted { resource, holder } => {
                assert_eq!(holder, "owner");
                assert_eq!(resource, Resource::path("/locked.rs"));
            }
            _ => panic!("Expected Conflicted"),
        }
    }

    #[test]
    fn test_no_resources_needed_always_succeeds() {
        let lm = LockManager::in_memory();
        let rr = RequiredResources::new();
        let result = lm.try_acquire("agent", &rr);
        assert!(matches!(result, LockResult::Acquired(_)));
    }
}

// ─── 2. Intent Signaling + Conflict Resolution ──────────────────────────────

mod intent_and_conflict {
    use super::*;

    #[test]
    fn test_intent_signaler_broadcasts_intent() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let signaler = IntentSignaler::new(bus);

        let rr = RequiredResources::new()
            .read("/src/main.rs")
            .write("/src/output.rs");

        signaler.signal_intent("agent-a", &rr);

        let event = rx.try_recv().unwrap();
        match event {
            OrchestrationEvent::ResourceIntent { holder, resources } => {
                assert_eq!(holder, "agent-a");
                assert_eq!(resources.len(), 2);
            }
            _ => panic!("Expected ResourceIntent event"),
        }
    }

    #[test]
    fn test_conflict_resolver_reports_and_drains() {
        let bus = BusHandle::new(16);
        let resolver = ConflictResolver::new(bus.clone());
        let mut rx = bus.subscribe();

        resolver.report_conflict("agent-b", &Resource::path("/src/main.rs"), "agent-a");

        let conflicts = resolver.drain_conflicts(&mut rx);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].holder, "agent-b");
        assert_eq!(conflicts[0].conflict_with, "agent-a");
    }

    #[test]
    fn test_drain_empties_conflicts() {
        let bus = BusHandle::new(16);
        let resolver = ConflictResolver::new(bus.clone());
        let mut rx = bus.subscribe();

        resolver.report_conflict("a", &Resource::path("/x.rs"), "b");
        resolver.report_conflict("c", &Resource::path("/y.rs"), "d");

        let first = resolver.drain_conflicts(&mut rx);
        assert_eq!(first.len(), 2);

        let second = resolver.drain_conflicts(&mut rx);
        assert!(second.is_empty());
    }

    #[test]
    fn test_end_to_end_intent_then_conflict() {
        let bus = BusHandle::new(32);
        let signaler = IntentSignaler::new(bus.clone());
        let resolver = ConflictResolver::new(bus.clone());
        let lm = LockManager::in_memory();

        // Agent A signals intent and acquires
        let rr_a = RequiredResources::new().write("/shared/file.rs");
        signaler.signal_intent("agent-a", &rr_a);
        let _guard_a = match lm.try_acquire("agent-a", &rr_a) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        // Agent B signals intent and tries to acquire (should conflict)
        let rr_b = RequiredResources::new().write("/shared/file.rs");
        signaler.signal_intent("agent-b", &rr_b);
        match lm.try_acquire("agent-b", &rr_b) {
            LockResult::Conflicted { resource, holder } => {
                resolver.report_conflict("agent-b", &resource, &holder);
            }
            _ => panic!("Expected Conflicted"),
        }

        let mut rx = bus.subscribe();
        // Check that conflicts were reported
        resolver.report_conflict("agent-b", &Resource::path("/shared/file.rs"), "agent-a");
        let conflicts = resolver.drain_conflicts(&mut rx);
        assert_eq!(conflicts.len(), 1);
    }
}

// ─── 3. WorktreeManager Integration ─────────────────────────────────────────

mod worktree_integration {
    use super::*;

    #[test]
    fn test_assign_isolated_workspaces() {
        let tmp = tempfile::tempdir().unwrap();
        let lm = LockManager::in_memory();
        let wm = WorktreeManager::new(tmp.path().to_path_buf(), lm);

        let paths_a = vec![PathBuf::from("/project/src/main.rs")];
        let paths_b = vec![PathBuf::from("/project/src/lib.rs")];

        let guard_a = match wm.assign_workspace("agent-a", &paths_a) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let guard_b = match wm.assign_workspace("agent-b", &paths_b) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        assert!(wm.active_agents().contains(&"agent-a".to_string()));
        assert!(wm.active_agents().contains(&"agent-b".to_string()));

        drop(guard_a);
        drop(guard_b);
    }

    #[test]
    fn test_worktree_conflict_on_same_path() {
        let tmp = tempfile::tempdir().unwrap();
        let lm = LockManager::in_memory();
        let wm = WorktreeManager::new(tmp.path().to_path_buf(), lm);

        let paths = vec![PathBuf::from("/project/src/main.rs")];

        let _guard_a = match wm.assign_workspace("agent-a", &paths) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        let result = wm.assign_workspace("agent-b", &paths);
        assert!(matches!(result, LockResult::Conflicted { .. }));
    }

    #[test]
    fn test_worktree_release_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let lm = LockManager::in_memory();
        let wm = WorktreeManager::new(tmp.path().to_path_buf(), lm);

        let paths = vec![PathBuf::from("/project/src/main.rs")];

        {
            let _guard = match wm.assign_workspace("agent-a", &paths) {
                LockResult::Acquired(g) => g,
                _ => panic!("Expected Acquired"),
            };
            assert!(wm.active_agents().contains(&"agent-a".to_string()));
        }

        // After drop, should be able to reassign
        let guard_b = match wm.assign_workspace("agent-b", &paths) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired after release"),
        };
        assert!(wm.active_agents().contains(&"agent-b".to_string()));
        drop(guard_b);
    }

    #[test]
    fn test_worktree_base_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let lm = LockManager::in_memory();
        let wm = WorktreeManager::new(tmp.path().to_path_buf(), lm);
        assert_eq!(wm.base_dir(), tmp.path());
    }
}

// ─── 4. Musician + LockManager Integration ──────────────────────────────────

mod musician_resource_integration {
    use super::*;

    fn make_step(id: &str, tool: Option<&str>, resources: RequiredResources) -> Step {
        Step {
            id: id.into(),
            index: 0,
            description: "echo hello".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: tool.map(Into::into),
            retry_on_failure: false,
            required_resources: resources,
        }
    }

    #[tokio::test]
    async fn test_musician_step_with_read_resources() {
        let _bus = BusHandle::new(16);
        let musician = Musician::new();
        let step = make_step(
            "s-read",
            Some("bash"),
            RequiredResources::new().read("/tmp/resource-test-read.rs"),
        );
        let mut trace = ExecutionTrace::new("t-read".into());

        let result = musician.play_step(&step, &mut trace).await.unwrap();
        assert!(result.is_success());

        // Lock should be released after step completes
        let lm = musician.lock_manager();
        assert!(lm
            .is_locked(&Resource::path("/tmp/resource-test-read.rs"))
            .is_none());
    }

    #[tokio::test]
    async fn test_musician_step_blocked_by_external_lock() {
        let _bus = BusHandle::new(16);
        let musician = Musician::new();
        let lm = musician.lock_manager().clone();

        // Hold a write lock externally
        let external_rr = RequiredResources::new().write("/tmp/resource-test-blocked.rs");
        let _external_guard = match lm.try_acquire("external-agent", &external_rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        let step = make_step(
            "s-blocked",
            Some("bash"),
            RequiredResources::new().write("/tmp/resource-test-blocked.rs"),
        );
        let mut trace = ExecutionTrace::new("t-blocked".into());

        let result = musician.play_step(&step, &mut trace).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_musician_step_no_resources_succeeds() {
        let _bus = BusHandle::new(16);
        let musician = Musician::new();
        let step = make_step("s-free", Some("bash"), RequiredResources::new());
        let mut trace = ExecutionTrace::new("t-free".into());

        let result = musician.play_step(&step, &mut trace).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_musician_sequential_steps_same_resource() {
        let _bus = BusHandle::new(16);
        let musician = Musician::new();

        let rr = RequiredResources::new().read("/tmp/resource-test-seq.rs");
        let mut trace = ExecutionTrace::new("t-seq".into());

        // Step 1
        let step1 = make_step("s-seq-1", Some("bash"), rr.clone());
        let result1 = musician.play_step(&step1, &mut trace).await.unwrap();
        assert!(result1.is_success());

        // Step 2 — same resource, should succeed since step1 released it
        let step2 = make_step("s-seq-2", Some("bash"), rr);
        let result2 = musician.play_step(&step2, &mut trace).await.unwrap();
        assert!(result2.is_success());
    }

    #[tokio::test]
    async fn test_musician_with_custom_lock_manager() {
        let custom_lm = LockManager::in_memory();
        let _bus = BusHandle::new(16);
        let musician = Musician::new().with_lock_manager(custom_lm);

        let step = make_step(
            "s-custom-lm",
            Some("bash"),
            RequiredResources::new().read("/tmp/resource-test-custom.rs"),
        );
        let mut trace = ExecutionTrace::new("t-custom-lm".into());

        let result = musician.play_step(&step, &mut trace).await.unwrap();
        assert!(result.is_success());
    }
}

// ─── 5. Full Ensemble Resource Safety ───────────────────────────────────────

mod ensemble_resource_safety {
    use super::*;

    #[test]
    fn test_parallel_agents_isolated_resources() {
        let bus = BusHandle::new(32);
        let lm = LockManager::in_memory();

        // Simulate two agents working on different files
        let rr_a = RequiredResources::new()
            .read("/src/alpha.rs")
            .write("/build/alpha.o");

        let rr_b = RequiredResources::new()
            .read("/src/beta.rs")
            .write("/build/beta.o");

        let signaler = IntentSignaler::new(bus.clone());
        signaler.signal_intent("agent-alpha", &rr_a);
        signaler.signal_intent("agent-beta", &rr_b);

        let guard_a = match lm.try_acquire("agent-alpha", &rr_a) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let guard_b = match lm.try_acquire("agent-beta", &rr_b) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        drop(guard_a);
        drop(guard_b);
    }

    #[test]
    fn test_parallel_agents_conflict_detected() {
        let bus = BusHandle::new(32);
        let lm = LockManager::in_memory();
        let resolver = ConflictResolver::new(bus.clone());

        let rr_a = RequiredResources::new().write("/src/shared.rs");
        let rr_b = RequiredResources::new().write("/src/shared.rs");

        let signaler = IntentSignaler::new(bus.clone());
        signaler.signal_intent("agent-alpha", &rr_a);

        let _guard_a = match lm.try_acquire("agent-alpha", &rr_a) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        signaler.signal_intent("agent-beta", &rr_b);
        match lm.try_acquire("agent-beta", &rr_b) {
            LockResult::Conflicted { resource, holder } => {
                resolver.report_conflict("agent-beta", &resource, &holder);
            }
            _ => panic!("Expected Conflicted"),
        }

        let mut rx = bus.subscribe();
        resolver.report_conflict(
            "agent-beta",
            &Resource::path("/src/shared.rs"),
            "agent-alpha",
        );
        let conflicts = resolver.drain_conflicts(&mut rx);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].holder, "agent-beta");
        assert_eq!(conflicts[0].conflict_with, "agent-alpha");
    }

    #[test]
    fn test_shared_read_with_exclusive_write_later() {
        let lm = LockManager::in_memory();

        // Phase 1: Multiple readers
        let rr_read = RequiredResources::new().read("/src/types.rs");
        let g1 = match lm.try_acquire("reader-1", &rr_read) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };
        let g2 = match lm.try_acquire("reader-2", &rr_read) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        // Phase 2: Writer blocked while readers hold locks
        let rr_write = RequiredResources::new().write("/src/types.rs");
        assert!(matches!(
            lm.try_acquire("writer", &rr_write),
            LockResult::Conflicted { .. }
        ));

        // Phase 3: Readers done, writer succeeds
        drop(g1);
        drop(g2);
        let _write_guard = match lm.try_acquire("writer", &rr_write) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired after readers released"),
        };
    }
}
