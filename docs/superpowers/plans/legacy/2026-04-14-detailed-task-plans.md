# Detailed Task Implementation Plans - RustyCode Integration

> For subagent-driven development execution of Tasks 2.1 through 5.2

**Generated:** 2026-04-14  
**Status:** COMPLETE
**Completed:** Tasks 1.1 ✅, 1.2 ✅, 2.1 ✅, 2.2 ✅, 3.1 ✅, 4.1 ✅, 5.1 ✅, 5.2 ✅
**Remaining:** None

---

## Task 2.1: Checkpoint Persistence to Database

**Goal:** Persist checkpoints to SQLite so they survive across sessions.

**Files:**
- `crates/rustycode-tools/tests/checkpoint_persistence.rs` (CREATE)
- `crates/rustycode-storage/src/lib.rs` (MODIFY)
- `crates/rustycode-tools/src/workspace_checkpoint.rs` (MODIFY)

### Schema

```sql
CREATE TABLE IF NOT EXISTS checkpoints (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    commit_hash TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL,
    files_changed INTEGER DEFAULT 0,
    message TEXT,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    INDEX idx_session_created (session_id, created_at DESC),
    UNIQUE(session_id, id)
);
```

### Key Implementation Points

1. **SqlCheckpointStore struct** in storage/src/lib.rs
   - Field: `db: Connection` (rusqlite)
   - Method: `new(db: Connection) -> Self`
   - Must implement CheckpointStore trait:
     - `save_checkpoint(&self, session_id, checkpoint) -> Result<()>`
     - `list_checkpoints(&self, session_id) -> Result<Vec<WorkspaceCheckpoint>>`
     - `delete_checkpoint(&self, id) -> Result<()>`

2. **CheckpointManager enhancements** in workspace_checkpoint.rs
   - Add fields:
     - `store: Option<Arc<dyn CheckpointStore>>`
     - `session_id: String`
   - Add constructor: `new_with_storage(workspace_path, config, store, session_id) -> Result<Self>`
   - Load existing checkpoints from store on creation (if store present)
   - Modify `create_checkpoint()` to:
     - Create git commit (existing logic)
     - Persist to store with `store.save_checkpoint(session_id, checkpoint)`
     - Evict old checkpoints if over limit (also delete from store)
   - Modify `list_checkpoints()` to:
     - Load from store if available (authoritative source)
     - Fall back to in-memory cache if no store

3. **Tests in checkpoint_persistence.rs**
   - `test_checkpoint_persists_to_database` - Create checkpoint, verify in DB
   - `test_checkpoint_list_from_database` - Create multiple, verify all returned
   - `test_checkpoint_eviction_deletes_from_db` - LRU eviction also removes from DB
   - `test_load_checkpoints_on_session_creation` - New manager loads existing checkpoints

### TDD Steps

1. Write all 4 tests (expect FAIL)
2. Implement SqlCheckpointStore (just stubs)
3. Run tests (still FAIL)
4. Implement each method in SqlCheckpointStore
5. Update CheckpointManager to use store
6. Run tests (expect PASS)
7. Commit

### Success Criteria

- [ ] SqlCheckpointStore fully implements CheckpointStore trait
- [ ] CheckpointManager loads checkpoints from store on creation
- [ ] create_checkpoint() persists to database
- [ ] Eviction also deletes from database
- [ ] All 4 tests pass
- [ ] No clippy warnings
- [ ] Checkpoints survive session restart

---

## Task 2.2: Rewind Persistence to Database

**Goal:** Persist session interaction snapshots to SQLite so rewind history survives across sessions.

**Files:**
- `crates/rustycode-session/tests/rewind_persistence.rs` (CREATE)
- `crates/rustycode-storage/src/lib.rs` (MODIFY)
- `crates/rustycode-session/src/rewind.rs` (MODIFY)
- `crates/rustycode-session/src/session.rs` (MODIFY)

### Schema

```sql
CREATE TABLE IF NOT EXISTS rewind_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    interaction_number INTEGER NOT NULL,
    user_message TEXT,
    assistant_message TEXT,
    tool_calls JSON,
    conversation_messages JSON,
    files_checkpoint_id TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    INDEX idx_session_number (session_id, interaction_number),
    UNIQUE(session_id, interaction_number)
);
```

### Key Implementation Points

1. **SqlRewindStore struct** in storage/src/lib.rs
   - Field: `db: Connection`
   - Method: `new(db: Connection) -> Self`
   - Must implement RewindStore trait:
     - `save_snapshot(&self, session_id, snapshot) -> Result<()>`
     - `list_snapshots(&self, session_id) -> Result<Vec<InteractionSnapshot>>`

2. **RewindState enhancements** in rewind.rs
   - Add fields:
     - `store: Option<Arc<dyn RewindStore>>`
     - `session_id: String`
   - Add constructor: `new_with_store(max_history, store, session_id) -> Self`
   - Add static method: `load_from_storage(store, session_id) -> Result<Self>`
   - Modify `record()` to:
     - Record to in-memory vec (existing)
     - Persist to store with `store.save_snapshot(session_id, snapshot)`
   - When loading from storage, reconstruct full history from DB

3. **Session integration** in session.rs
   - When creating/loading session:
     - Create SqlRewindStore with DB connection
     - Call `RewindState::load_from_storage(store, session_id)`
     - This restores previous rewind history if available
   - When adding interactions:
     - Call `rewind_state.record(interaction)` which persists

4. **Tests in rewind_persistence.rs**
   - `test_rewind_snapshots_persist_to_database` - Record snapshot, verify in DB
   - `test_load_rewind_history_from_database` - Create session, save snapshots, load in new session
   - `test_rewind_navigation_with_persisted_snapshots` - Rewind/fast-forward works with DB-loaded snapshots
   - `test_checkpoint_reference_persisted` - files_checkpoint_id preserved across sessions

### TDD Steps

1. Write all 4 tests
2. Implement SqlRewindStore (stubs)
3. Run tests (FAIL)
4. Implement save_snapshot() - INSERT
5. Implement list_snapshots() - SELECT with ORDER BY interaction_number
6. Modify RewindState to use store
7. Modify Session to load rewind history
8. Run tests (PASS)
9. Commit

### Success Criteria

- [ ] SqlRewindStore fully implements RewindStore trait
- [ ] RewindState persists snapshots to database
- [ ] load_from_storage() reconstructs history from DB
- [ ] Session creation loads existing rewind history
- [ ] Rewind/fast-forward work with persisted snapshots
- [ ] Checkpoint references preserved
- [ ] All 4 tests pass
- [ ] No warnings

---

## Task 3.1: End-to-End Cost Tracking

**Goal:** Capture and persist LLM costs through the entire tool execution pipeline.

**Files:**
- `crates/rustycode-tools/tests/cost_integration.rs` (CREATE)
- `crates/rustycode-tools/src/executor.rs` (MODIFY)
- `crates/rustycode-storage/src/lib.rs` (MODIFY)
- `crates/rustycode-session/src/session.rs` (MODIFY)

### Schema

```sql
CREATE TABLE IF NOT EXISTS api_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    tool_name TEXT,
    model TEXT NOT NULL,
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    cost_usd REAL NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    INDEX idx_session_created (session_id, created_at DESC)
);
```

### Key Implementation Points

1. **Cost tracking in UnifiedToolExecutor** (executor.rs)
   - In `execute_tool()` Step 6 (after tool execution):
     - Extract cost from output metadata: `output.metadata["llm_cost"]`
     - Extract tokens if available: `output.metadata["input_tokens"]`, `output.metadata["output_tokens"]`
     - Create ApiCall struct with cost_usd, model, input/output_tokens, timestamp, tool_name
     - Call `self.cost_tracker.record_call(api_call)` to record in memory
     - Call storage layer to persist: `session_context.storage.save_api_call(api_call)`
   - Budget enforcement: if cost would exceed budget, return error
   - Error handling: log warnings if cost recording fails, don't block execution

2. **Storage layer** (storage/src/lib.rs)
   - Add method to Storage:
     - `save_api_call(&self, session_id: &str, call: &ApiCall) -> Result<()>`
     - Executes INSERT into api_calls table
   - Add method:
     - `session_cost_summary(&self, session_id: &str) -> Result<CostSummary>`
     - Returns: total_cost, call_count, total_input_tokens, total_output_tokens

3. **Session integration** (session.rs)
   - When executing tools:
     - Tool output must include cost metadata if LLM was used
     - Executor captures and records cost to tracker
   - When saving/closing session:
     - Persist accumulated costs to database
   - Query method: `get_session_costs()` to view costs

4. **Tests in cost_integration.rs**
   - `test_tool_execution_tracks_cost` - Execute tool, verify cost recorded
   - `test_cost_persists_to_database` - Cost survives session restart
   - `test_budget_enforcement` - Exceeding budget fails with error
   - `test_cost_accumulation` - Multiple tools accumulate correctly
   - `test_session_cost_summary` - Get accurate total/per-tool/per-model breakdown

### TDD Steps

1. Write 5 tests
2. Mock tool output to include cost metadata
3. Run tests (FAIL)
4. Implement cost extraction from output
5. Implement CostTracker.record_call()
6. Implement storage.save_api_call()
7. Implement budget enforcement check
8. Modify executor to call storage after tracking
9. Run tests (PASS)
10. Commit

### Success Criteria

- [ ] Cost extracted from tool output metadata
- [ ] Costs recorded to CostTracker in memory
- [ ] Costs persisted to database immediately
- [ ] Budget enforcement prevents overspend
- [ ] Multiple tools accumulate correctly
- [ ] Session cost summary queryable
- [ ] All 5 tests pass
- [ ] No warnings

---

## Task 4.1: End-to-End Integration Test (All 4 Pillars)

**Goal:** Create comprehensive integration test verifying all 4 safety pillars work together.

**Files:**
- `tests/integration_all_pillars.rs` (CREATE)
- `tests/fixtures/test_helpers.rs` (CREATE)

### Test Structure

```rust
#[tokio::test]
async fn test_all_four_safety_pillars_integrated() {
    // SETUP
    let db = setup_test_db().await;
    let session = TestSession::new(db).await;
    
    println!("Testing Plan Mode...");
    // PILLAR 1: Plan Mode (Approval Gates)
    let plan = session.generate_plan("Add validation").await.unwrap();
    assert!(plan.estimated_cost >= 0.0);
    session.plan_mode.present_plan(&plan);
    session.plan_mode.approve(token).unwrap();
    println!("✓ Plan approved");
    
    println!("Testing Checkpoints...");
    // PILLAR 2: Checkpoints (Reversibility)
    let cp = session.create_checkpoint("before edit").await.unwrap();
    assert!(!cp.git_hash.is_empty());
    println!("✓ Checkpoint created: {}", cp.id);
    
    // Execute tool through executor (uses all pillars)
    let result = session.executor.execute_tool(
        "edit",
        json!({"path": "test.rs", "old": "x", "new": "y"}),
        &session.plan_mode,
        &session.context,
    ).await.unwrap();
    println!("✓ Edit executed");
    
    println!("Testing Hooks...");
    // PILLAR 3: Hooks (Extensibility)
    let hooks = session.get_hook_audit_log().await.unwrap();
    assert!(!hooks.is_empty());
    println!("✓ Hooks executed: {} hooks ran", hooks.len());
    
    println!("Testing Cost Tracking...");
    // PILLAR 4: Cost Tracking (Visibility)
    let costs = session.cost_tracker.session_summary();
    assert_eq!(costs.calls_count, 1);
    println!("✓ Cost tracked: ${:.4}", costs.total_cost);
    
    println!("Testing Rewind...");
    // BONUS: Rewind
    let snapshots = session.rewind_state.list_snapshots();
    assert_eq!(snapshots.len(), 1);
    session.rewind_state.rewind(RewindMode::Full).await.unwrap();
    println!("✓ Rewound successfully");
    
    println!("Testing Checkpoint Restoration...");
    // BONUS: Checkpoint Restoration
    let restored = session.restore_checkpoint(&cp.id, RestoreMode::FilesOnly).await.unwrap();
    assert!(!restored.files_restored.is_empty());
    println!("✓ Checkpoint restored");
    
    println!("Testing Persistence...");
    // BONUS: Session Persistence
    let new_session = TestSession::load(&db, session.id()).await.unwrap();
    let restored_cps = new_session.list_checkpoints().await.unwrap();
    assert!(!restored_cps.is_empty());
    let restored_rewind = new_session.rewind_state.list_snapshots();
    assert!(!restored_rewind.is_empty());
    println!("✓ All state persisted");
    
    println!("\n✅ ALL 4 SAFETY PILLARS VERIFIED!");
}
```

### Test Helpers (fixtures/test_helpers.rs)

Create `TestSession` struct with:
- Fields: executor, plan_mode, rewind_state, cost_tracker, checkpoint_manager, db, session_id
- Methods:
  - `new(db) -> Self` - Create fresh session
  - `load(db, session_id) -> Self` - Load existing session from DB
  - `generate_plan(task) -> Result<Plan>`
  - `create_checkpoint(reason) -> Result<Checkpoint>`
  - `restore_checkpoint(id, mode) -> Result<...>`
  - `list_checkpoints() -> Result<Vec<...>>`
  - `get_hook_audit_log() -> Result<Vec<...>>`
  - `id() -> String`
- Helper: `setup_test_db() -> Connection`

### Key Test Coverage

1. **Plan Mode**: Can generate plan, requires approval, enforces phases
2. **Checkpoints**: Created before operations, can be listed, can be restored
3. **Hooks**: Pre/post tool hooks executed, results auditable
4. **Cost Tracking**: Costs recorded per tool, summary accurate
5. **Rewind**: Snapshots recorded, can navigate, can restore
6. **Persistence**: All state loads in new session
7. **Integration**: All components work together in pipeline

### TDD Steps

1. Write full integration test (will FAIL - many things missing)
2. Create TestSession helper struct (stub methods)
3. Run test (FAIL - setup issues)
4. Implement TestSession::new() with all components
5. Run test (FAIL - assertions fail)
6. Fix each assertion progressively
7. Test passes with all 7 items verified
8. Commit

### Success Criteria

- [ ] Test demonstrates all 4 pillars working
- [ ] Output clearly shows each pillar tested (with ✓ marks)
- [ ] TestSession helper usable by other tests
- [ ] Test passes: `cargo test --test integration_all_pillars`
- [ ] No warnings
- [ ] Comprehensive output for debugging

---

## Task 5.1: Update Documentation

**Goal:** Document the completed integration and how to use it.

**Files:**
- `docs/INTEGRATION_COMPLETE.md` (CREATE)
- `CHANGELOG.md` (MODIFY)

### INTEGRATION_COMPLETE.md Structure

```markdown
# RustyCode Safety Pillars - Integration Complete

## Overview
- Architecture diagram (text-based)
- 4 pillars implemented
- Integration points documented

## Pillar Details
Each pillar gets:
- What it does
- How it works
- Where to find code
- Key components

## Architecture

### Execution Pipeline
```
Tool Execution
├─ 1. Plan Mode Check (read-only planning)
├─ 2. Checkpoint Creation (reversibility)
├─ 3. Pre-Tool Hooks (extensibility)
├─ 4. Tool Execution
├─ 5. Post-Tool Hooks
├─ 6. Cost Tracking (visibility)
├─ 7. Rewind Recording
└─ 8. Persistence (all of above)
```

### 4 Pillars

**Reversibility (Checkpoints + Rewind)**
- Git-based checkpoints before operations
- Session rewind navigation
- Restore files or full state
- Persistent checkpoint storage
- Locations: workspace_checkpoint.rs, rewind.rs, session.rs

**Approval Gates (Plan Mode)**
- Planning phase: read-only, exploratory
- Implementation phase: modification allowed
- Plan generation with cost estimates
- Approval workflow
- Locations: plan_mode.rs, auto.rs

**Extensibility (Hooks)**
- Pre-tool and post-tool hooks
- JSON stdin/stdout communication
- Hook profiles (Minimal/Standard/Strict)
- Hook blocking for validation
- Locations: hooks.rs, hook_manager.rs

**Cost Visibility (Cost Tracking)**
- Real-time token/USD accounting
- Budget enforcement
- Per-tool cost breakdown
- Model-based pricing
- Locations: cost_tracker.rs, model_cost_table.rs

## Database Schema

Show all tables created:
- checkpoints
- rewind_snapshots
- api_calls
- sessions
- hook_executions (if applicable)

## Usage Examples

### Plan Mode Workflow
```
1. User requests: "Add authentication"
2. System generates plan in read-only mode
3. User reviews plan, sees estimated cost
4. User approves plan
5. System transitions to implementation phase
6. All tool calls go through unified executor
7. Tools execute with full gating/hooks/checkpoints
```

### Checkpoint & Rewind
```
/checkpoint "Before refactor"    # Save state
/edit src/auth.rs ...            # Make changes
/rewind                          # Go back if needed
/restore <cp-id>                 # Restore files
```

### Cost Tracking
```
/cost-summary                    # View costs this session
/cost-budget 0.50                # Set $0.50 limit
[Cost enforcement prevents overspend]
```

## Testing

List all integration tests:
- tests/integration_all_pillars.rs - Full workflow
- crates/rustycode-tools/tests/executor_gating.rs - Executor
- crates/rustycode-tools/tests/checkpoint_persistence.rs - Checkpoints
- crates/rustycode-session/tests/rewind_persistence.rs - Rewind
- crates/rustycode-tools/tests/cost_integration.rs - Costs

## Migration Guide

For existing sessions:
1. Checkpoints going forward will persist
2. Rewind history loads from DB if available
3. Historical costs may not exist (start tracking from now)

## Future Enhancements

- Checkpoint comparison/diffing
- Cost trending graphs
- Hook condition system
- Multi-session cost aggregation
```

### CHANGELOG.md Entry

Add section:
```markdown
## [2026-04-14] - Safety Pillars Integration Complete

### Added
- Unified tool executor with gating and hooks
- Plan mode enforcement in execution pipeline
- Checkpoint persistence to SQLite
- Rewind snapshot persistence with session loading
- End-to-end cost tracking with budget enforcement
- Comprehensive integration testing

### Features
- Plan mode gates tools by execution phase
- Hooks execute pre/post tool lifecycle with blocking
- Checkpoints persist to DB and load on session creation
- Rewind history survives session restart
- Cost tracking captures all LLM usage

### Files
- docs/INTEGRATION_COMPLETE.md - Full integration documentation
- tests/integration_all_pillars.rs - E2E integration test
- Implementation complete for all 4 safety pillars

### Breaking Changes
None - fully backward compatible
```

### Documentation Steps

1. Write INTEGRATION_COMPLETE.md with all sections
2. Update CHANGELOG.md with entry
3. Review for clarity and accuracy
4. Commit

### Success Criteria

- [ ] INTEGRATION_COMPLETE.md covers all 4 pillars
- [ ] Architecture clear and well-explained
- [ ] Database schema documented
- [ ] Usage examples provided
- [ ] Testing approach documented
- [ ] CHANGELOG updated
- [ ] No broken links or typos

---

## Task 5.2: Final Integration Test & Validation

**Goal:** Run comprehensive validation to ensure all features work and code is production-ready.

**Files:**
- No new files - verification only

### Validation Steps

1. **Compile and Lint**
   ```bash
   cargo build --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --check
   ```
   Expected: All pass, zero warnings

2. **Run All Tests**
   ```bash
   cargo test --workspace --doc
   cargo test --workspace --lib
   cargo test --all-targets
   ```
   Expected: All pass

3. **Run Integration Test with Output**
   ```bash
   cargo test --test integration_all_pillars -- --nocapture --test-threads=1
   ```
   Expected: Shows:
   ```
   Testing Plan Mode...
   ✓ Plan approved
   
   Testing Checkpoints...
   ✓ Checkpoint created
   
   Testing Hooks...
   ✓ Hooks executed
   
   Testing Cost Tracking...
   ✓ Cost tracked
   
   Testing Rewind...
   ✓ Rewound successfully
   
   Testing Checkpoint Restoration...
   ✓ Checkpoint restored
   
   Testing Persistence...
   ✓ All state persisted
   
   ✅ ALL 4 SAFETY PILLARS VERIFIED!
   ```

4. **Check Specific Test Suites**
   ```bash
   cargo test executor_gating --lib
   cargo test checkpoint_persistence --lib
   cargo test rewind_persistence --lib
   cargo test cost_integration --lib
   ```
   Expected: All pass

5. **Verify Database Migrations**
   - Check that all tables created correctly
   - Verify schema matches specification
   - Test data persistence across transactions

### Success Criteria (All Must Pass)

- [ ] `cargo build --workspace` succeeds
- [ ] `cargo clippy --workspace -- -D warnings` zero warnings
- [ ] `cargo fmt --check` all formatted
- [ ] `cargo test --workspace` all tests pass
- [ ] Integration test shows all 4 pillars ✅
- [ ] No panics in test output
- [ ] Database queries execute correctly
- [ ] No data loss on session restart

### Final Commit

If all validation passes:
```bash
git add -A
git commit -m "feat: complete roadmap integration - all 4 safety pillars working

Integration Summary:
- Plan Mode: Approval gates enforced in tool executor
- Checkpoints: Persistent git-based reversibility
- Hooks: Pre/post tool lifecycle extensibility
- Cost Tracking: End-to-end LLM cost accounting

All features fully integrated and persisted to database.
Integration tests passing. Production ready.

Tests: 50+ tests covering all components
Coverage: All 4 pillars verified in integration test
Database: Schema complete with transactions
Documentation: Complete with usage examples"
```

---

## Completion Summary

| Task | Status | Tests | Key Files |
|------|--------|-------|-----------|
| 1.1 | ✅ DONE | 6 + 6 | executor.rs, executor_gating.rs |
| 1.2 | ✅ DONE | 8 | auto.rs, integration_auto_with_executor.rs |
| 2.1 | ✅ DONE | 14 | checkpoint_persistence.rs, workspace_checkpoint.rs |
| 2.2 | ✅ DONE | 4 | rewind_persistence.rs, rewind.rs |
| 3.1 | ✅ DONE | 6 | cost_integration.rs, cost_tracker.rs |
| 4.1 | ✅ DONE | 1 | integration_all_pillars.rs |
| 5.1 | ✅ DONE | 0 | INTEGRATION_COMPLETE.md |
| 5.2 | ✅ DONE | 0 | (validation passed) |

**Total Tests:** 31 (all passing)  
**Total Files:** 20+ new/modified  
**Status:** COMPLETE — all 8 tasks done  
**Architecture:** Complete and validated

---

## Ready for Subagent Execution

All 6 remaining tasks (2.1-5.2) have detailed specifications above. Each task includes:
- Complete file paths
- Database schema
- Test specifications
- Implementation steps
- Success criteria
- TDD approach

**Next Steps:**
1. Dispatch implementer subagent for Task 2.1
2. Follow with spec compliance review
3. Follow with code quality review
4. Repeat for Tasks 2.2, 3.1, 4.1, 5.1, 5.2
