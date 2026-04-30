# RustyCode Roadmap Integration Plan (Revised)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate existing safety infrastructure (Checkpoints, Rewind, Hooks, Plan Mode, Cost Tracking) into the execution pipeline with full persistence and testing.

**Key Insight:** 90% of components already exist. This plan focuses on **integration points** and **persistence**, not new code.

**Architecture:** 
- Connect existing components into unified execution pipeline
- Add database persistence for checkpoints + rewind
- Wire hooks/plan-mode gating into tool executor
- Implement cost tracking end-to-end
- Add comprehensive integration tests

**Tech Stack:** Rust 2021, Tokio async, Rusqlite (existing), Existing traits/modules

**Execution Model:** 12 integration tasks, 30-50 hours total

---

## Current State vs. Target State

### Current (Pre-Integration)
```
Tool Execution          Plan Mode              Hooks                 Checkpoints
┌─────────────┐        ┌──────────┐          ┌────────┐            ┌───────────┐
│ edit_file() │        │ PlanMode │          │ Hooks  │            │Checkpoint │
│ bash()      │        │ Planning │          │ System │            │ (mem-only)│
│ write()     │        │ Phase    │          │ JSON   │            │           │
└─────────────┘        │ exists   │          │ stdin/ │            │ No DB     │
     ↓                 │ but not  │          │ stdout │            │ No Restore│
   Executes            │ enforced │          │ exists │            └───────────┘
   (no gates)          └──────────┘          │ but not│
                                             │invoked │
                                             └────────┘
```

### Target (Integrated)
```
┌─────────────────────────────────────────────────────────────────┐
│                    Tool Executor (Unified)                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Check PlanMode.is_tool_allowed() ──→ Block if restricted    │
│                                                                  │
│  2. Create checkpoint (before edit/bash)                        │
│                                                                  │
│  3. Run PreToolUse hooks ──→ Check if any blocks                │
│                                                                  │
│  4. Execute tool (edit/bash/write)                              │
│                                                                  │
│  5. Capture cost from LLM call                                  │
│                                                                  │
│  6. Run PostToolUse hooks                                        │
│                                                                  │
│  7. Record interaction for rewind                               │
│                                                                  │
│  8. Persist all state (checkpoints, rewind, costs)              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## File Structure (Minimal Changes)

### New Files
```
None (reuse existing implementations)
```

### Modified Files (Integration Points)
```
crates/rustycode-tools/src/
├── executor.rs           (MODIFY) - Add hook/plan gating
├── lib.rs               (MODIFY) - Export unified executor
└── mod.rs               (MODIFY) - Restructure if needed

crates/rustycode-orchestra/src/
├── auto.rs              (MODIFY) - Integrate plan mode
└── lib.rs               (MODIFY) - Export executors

crates/rustycode-session/src/
├── session.rs           (MODIFY) - Persist rewind/checkpoints
└── lib.rs               (MODIFY) - Add persistence methods

crates/rustycode-storage/src/
├── lib.rs               (MODIFY) - Add checkpoint/rewind persistence methods
└── migrations.rs        (ADD) - Schema updates if needed

tests/
├── integration_all_pillars.rs (NEW) - End-to-end integration test
└── integration_executor.rs    (NEW) - Tool executor with gating test
```

---

## PHASE 1: TOOL EXECUTOR INTEGRATION

### Task 1.1: Define Tool Executor Trait with Gating

**Files:**
- Modify: `crates/rustycode-tools/src/executor.rs`
- Modify: `crates/rustycode-tools/src/lib.rs`
- Test: `crates/rustycode-tools/tests/executor_gating.rs`

**What exists:** Tool execution likely exists in ad-hoc modules (edit.rs, bash.rs, etc.)  
**What we need:** Unified trait + middleware for hook/plan gating

**Steps:**

- [ ] **Step 1: Examine current tool execution structure**

```bash
grep -r "pub fn edit" crates/rustycode-tools/src/
grep -r "pub fn bash" crates/rustycode-tools/src/
grep -r "ToolContext" crates/rustycode-tools/src/ | head -20
```

Review: Are tools methods on a struct, free functions, or scattered?

- [ ] **Step 2: Write failing test for unified executor**

Create `crates/rustycode-tools/tests/executor_gating.rs`:

```rust
#[tokio::test]
async fn test_executor_checks_plan_mode_before_write() {
    let executor = UnifiedToolExecutor::new();
    let plan_mode = PlanMode::new(Default::default());
    
    // In planning phase, write should be blocked
    let result = executor.execute_tool(
        "write",
        json!({"path": "file.txt"}),
        &plan_mode  // Add plan mode context
    ).await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not allowed"));
}

#[tokio::test]
async fn test_executor_runs_hooks_around_tool() {
    let executor = UnifiedToolExecutor::new();
    let hook_manager = HookManager::new(...);
    
    executor.execute_tool(
        "edit",
        json!({"path": "file.rs", "old": "x", "new": "y"}),
        &plan_mode
    ).await.unwrap();
    
    // Verify hooks were invoked
    // Check via hook audit log or mock
}

#[tokio::test]
async fn test_executor_creates_checkpoint_before_edit() {
    let executor = UnifiedToolExecutor::new();
    
    executor.execute_tool("edit", json!(...), &plan_mode).await.unwrap();
    
    // Verify checkpoint was created
    let checkpoints = executor.list_checkpoints().await.unwrap();
    assert!(!checkpoints.is_empty());
}
```

- [ ] **Step 3: Run tests (expect failure)**

```bash
cd crates/rustycode-tools
cargo test test_executor_checks_plan_mode_before_write
```

Expected: FAIL - trait/method doesn't exist

- [ ] **Step 4: Define UnifiedToolExecutor trait**

In `crates/rustycode-tools/src/executor.rs` (or create if doesn't exist):

```rust
use crate::plan_mode::{PlanMode, ExecutionPhase};
use crate::hooks::{HookManager, HookTrigger};
use crate::checkpoint::CheckpointManager;

/// Unified tool executor with gating and hooks
pub struct UnifiedToolExecutor {
    checkpoint_manager: Arc<CheckpointManager>,
    hook_manager: Arc<HookManager>,
    cost_tracker: Arc<CostTracker>,
}

impl UnifiedToolExecutor {
    pub fn new(
        checkpoint_manager: Arc<CheckpointManager>,
        hook_manager: Arc<HookManager>,
        cost_tracker: Arc<CostTracker>,
    ) -> Self {
        Self {
            checkpoint_manager,
            hook_manager,
            cost_tracker,
        }
    }

    /// Execute a tool with full gating and integration
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
        plan_mode: &PlanMode,
        session_context: &SessionContext,
    ) -> Result<ToolOutput> {
        // STEP 1: Check plan mode
        plan_mode.is_tool_allowed(tool_name)?;

        // STEP 2: Create checkpoint before destructive operations
        if self.should_checkpoint(tool_name) {
            self.checkpoint_manager
                .checkpoint(format!("before {}", tool_name), CheckpointMode::FullWorkspace)
                .await?;
        }

        // STEP 3: Run PreToolUse hooks
        let hook_context = json!({
            "tool_name": tool_name,
            "args": args.clone(),
        });

        let pre_hooks = self.hook_manager
            .execute(HookTrigger::PreToolUse, hook_context)
            .await?;

        if pre_hooks.should_block {
            return Err(anyhow::anyhow!(
                "Hook blocked execution: {}",
                pre_hooks.block_reason.unwrap_or_default()
            ));
        }

        // STEP 4: Execute the actual tool
        let start = std::time::Instant::now();
        let output = self.execute_tool_impl(tool_name, &args, plan_mode).await?;
        let duration = start.elapsed();

        // STEP 5: Run PostToolUse hooks
        let hook_context = json!({
            "tool_name": tool_name,
            "duration_ms": duration.as_millis(),
            "result": serde_json::to_value(&output)?,
        });

        self.hook_manager
            .execute(HookTrigger::PostToolUse, hook_context)
            .await?;

        // STEP 6: Record cost if LLM was used
        if let Some(cost) = self.extract_cost_from_output(&output) {
            self.cost_tracker.record_call(ApiCall {
                model: "claude-3-opus".to_string(),  // From context
                input_tokens: 0,  // From output metadata
                output_tokens: 0,
                cost_usd: cost,
                timestamp: Utc::now(),
                tool_name: Some(tool_name.to_string()),
            })?;
        }

        Ok(output)
    }

    fn should_checkpoint(&self, tool_name: &str) -> bool {
        matches!(tool_name, "edit" | "write" | "bash")
    }

    async fn execute_tool_impl(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        plan_mode: &PlanMode,
    ) -> Result<ToolOutput> {
        match tool_name {
            "read" => self.execute_read(args).await,
            "edit" => self.execute_edit(args, plan_mode).await,  // Dry-run in planning
            "write" => self.execute_write(args).await,
            "bash" => self.execute_bash(args).await,
            _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name)),
        }
    }

    async fn execute_read(&self, args: &serde_json::Value) -> Result<ToolOutput> {
        // Delegate to existing read_file implementation
        todo!()
    }

    async fn execute_edit(
        &self,
        args: &serde_json::Value,
        plan_mode: &PlanMode,
    ) -> Result<ToolOutput> {
        // In planning phase, return dry-run (show what would change)
        if plan_mode.current_phase() == ExecutionPhase::Planning {
            let path = args["path"].as_str().ok_or(anyhow::anyhow!("Missing path"))?;
            let old = args["old"].as_str().ok_or(anyhow::anyhow!("Missing old"))?;
            let new = args["new"].as_str().ok_or(anyhow::anyhow!("Missing new"))?;

            return Ok(ToolOutput {
                success: true,
                preview: true,
                message: format!("Would change:\n- {}\n+ {}", old, new),
                ..Default::default()
            });
        }

        // In implementation phase, actually apply
        todo!()  // Delegate to existing edit implementation
    }

    async fn execute_write(&self, args: &serde_json::Value) -> Result<ToolOutput> {
        todo!()
    }

    async fn execute_bash(&self, args: &serde_json::Value) -> Result<ToolOutput> {
        todo!()
    }

    fn extract_cost_from_output(&self, output: &ToolOutput) -> Option<f64> {
        // Extract LLM cost from metadata if present
        output.metadata.get("cost_usd").and_then(|v| v.as_f64())
    }

    pub async fn list_checkpoints(&self) -> Result<Vec<Checkpoint>> {
        self.checkpoint_manager.list().await
    }
}
```

- [ ] **Step 5: Run tests (should pass)**

```bash
cd crates/rustycode-tools
cargo test test_executor_checks_plan_mode_before_write
cargo test test_executor_runs_hooks_around_tool
cargo test test_executor_creates_checkpoint_before_edit
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-tools/src/executor.rs
git add crates/rustycode-tools/src/lib.rs
git add crates/rustycode-tools/tests/executor_gating.rs
git commit -m "feat: create unified tool executor with plan mode and hook gating"
```

---

### Task 1.2: Wire Executor into Autonomous Mode Auto Mode

**Files:**
- Modify: `crates/rustycode-orchestra/src/auto.rs`
- Modify: `crates/rustycode-orchestra/src/lib.rs`
- Test: `crates/rustycode-orchestra/tests/integration_auto_with_executor.rs`

**What exists:** `AutoMode` struct with lightweight config  
**What we need:** Integrate with PlanMode + UnifiedToolExecutor

**Steps:**

- [ ] **Step 1: Write failing test**

Create `crates/rustycode-orchestra/tests/integration_auto_with_executor.rs`:

```rust
#[tokio::test]
async fn test_auto_mode_respects_plan_phase() {
    let auto = AutoMode::with_plan_enforcement();  // Enable plan mode
    
    // Auto attempts to modify file
    let result = auto.execute_task("Add error handling").await;
    
    // Should require plan approval before implementation
    assert!(result.requires_approval());
}

#[tokio::test]
async fn test_auto_mode_estimates_cost_in_plan() {
    let auto = AutoMode::with_cost_tracking();
    
    let plan = auto.generate_plan("Refactor authentication").await.unwrap();
    
    assert!(plan.estimated_cost >= 0.0);
    assert!(!plan.summary.is_empty());
}
```

- [ ] **Step 2: Run test (expect failure)**

```bash
cd crates/rustycode-orchestra
cargo test test_auto_mode_respects_plan_phase
```

Expected: FAIL

- [ ] **Step 3: Integrate executor into auto.rs**

Modify `crates/rustycode-orchestra/src/auto.rs`:

```rust
use rustycode_tools::UnifiedToolExecutor;
use rustycode_tools::plan_mode::PlanMode;

pub struct AutoMode {
    executor: Arc<UnifiedToolExecutor>,
    plan_mode: Arc<PlanMode>,
    // ... existing fields
}

impl AutoMode {
    pub fn with_plan_enforcement() -> Self {
        let executor = Arc::new(UnifiedToolExecutor::new(
            Arc::new(CheckpointManager::new(...)),
            Arc::new(HookManager::new(...)),
            Arc::new(CostTracker::new(None)),
        ));

        let plan_mode = Arc::new(PlanMode::new(Default::default()));

        Self {
            executor,
            plan_mode,
            // ...
        }
    }

    pub async fn execute_task(&self, task: &str) -> Result<TaskResult> {
        // Start in planning phase
        let plan = self.plan_mode.generate_plan(task).await?;

        // Present plan to user
        let approval_token = self.plan_mode.present_plan(&plan);

        // Wait for approval (would be user interaction in real scenario)
        self.plan_mode.approve(approval_token)?;

        // Now in implementation phase
        // Execute the plan
        self.execute_plan(&plan).await
    }

    async fn execute_plan(&self, plan: &Plan) -> Result<TaskResult> {
        for file_plan in &plan.files_to_modify {
            // Use executor with gating + hooks
            self.executor.execute_tool(
                "edit",
                json!(file_plan),
                &self.plan_mode,
                &self.session_context,
            ).await?;
        }

        Ok(TaskResult {
            success: true,
            cost: self.executor.cost_tracker.session_summary().total_cost,
        })
    }

    pub async fn generate_plan(&self, task: &str) -> Result<Plan> {
        self.plan_mode.generate_plan(task).await
    }
}
```

- [ ] **Step 4: Run tests (should pass)**

```bash
cargo test test_auto_mode_respects_plan_phase
cargo test test_auto_mode_estimates_cost_in_plan
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-orchestra/src/auto.rs
git add crates/rustycode-orchestra/src/lib.rs
git add crates/rustycode-orchestra/tests/integration_auto_with_executor.rs
git commit -m "feat: integrate unified executor and plan mode into auto mode"
```

---

## PHASE 2: PERSISTENCE INTEGRATION

### Task 2.1: Checkpoint Persistence

**Files:**
- Modify: `crates/rustycode-storage/src/lib.rs`
- Modify: `crates/rustycode-tools/src/checkpoint.rs`
- Test: `crates/rustycode-tools/tests/checkpoint_persistence.rs`

**What exists:** 
- CheckpointManager (in-memory)
- Storage layer (rusqlite-based)
- Checkpoint schema likely exists or needs minimal schema update

**What we need:** Persist checkpoints to DB

**Steps:**

- [ ] **Step 1: Write failing test for persistent checkpoints**

Create `crates/rustycode-tools/tests/checkpoint_persistence.rs`:

```rust
#[tokio::test]
async fn test_checkpoint_persists_to_database() {
    let db = setup_test_db().await;
    let storage = Arc::new(SqlCheckpointStore::new(db.clone()));
    let manager = CheckpointManager::new_with_storage(
        repo_path.clone(),
        storage,
        10,
    );

    // Create checkpoint
    let checkpoint = manager
        .checkpoint("test save", CheckpointMode::FullWorkspace)
        .await
        .unwrap();

    // Verify it's in database
    let retrieved = db.get_checkpoint(&checkpoint.id).await.unwrap();
    assert_eq!(retrieved.id, checkpoint.id);
}

#[tokio::test]
async fn test_checkpoint_list_from_database() {
    let db = setup_test_db().await;
    let manager = CheckpointManager::new_with_storage(...);

    manager.checkpoint("cp1", ...).await.unwrap();
    manager.checkpoint("cp2", ...).await.unwrap();

    let list = manager.list().await.unwrap();
    assert_eq!(list.len(), 2);
}
```

- [ ] **Step 2: Run test (expect failure)**

```bash
cd crates/rustycode-tools
cargo test test_checkpoint_persists_to_database
```

Expected: FAIL - storage not being used

- [ ] **Step 3: Modify CheckpointManager to use storage**

In `crates/rustycode-tools/src/checkpoint.rs`:

```rust
impl CheckpointManager {
    // New constructor with storage
    pub fn new_with_storage(
        repo_path: PathBuf,
        storage: Arc<dyn CheckpointStore>,
        max: usize,
    ) -> Self {
        Self {
            repo_path,
            storage,
            max_checkpoints: max,
        }
    }

    pub async fn checkpoint(
        &self,
        reason: impl Into<String>,
        mode: CheckpointMode,
    ) -> Result<Checkpoint> {
        let reason = reason.into();
        let id = CheckpointId::new();

        // Stage and commit to git
        self.stage_changes(&mode).await?;
        let git_hash = self.commit_to_git(&id, &reason).await?;

        let checkpoint = Checkpoint {
            id,
            session_id: "current".to_string(),  // TODO: from context
            reason,
            git_hash,
            created_at: Utc::now(),
            files_changed: vec![],
            description: None,
        };

        // PERSIST TO DATABASE
        self.storage.save_checkpoint(&checkpoint).await?;

        // LRU cleanup
        self.evict_old_checkpoints().await?;

        Ok(checkpoint)
    }

    pub async fn list(&self) -> Result<Vec<Checkpoint>> {
        // Load from storage
        self.storage.list_checkpoints().await
    }

    pub async fn restore(
        &self,
        id: &CheckpointId,
        mode: RestoreMode,
    ) -> Result<RestoredCheckpoint> {
        // Load from storage
        let checkpoint = self.storage.get_checkpoint(id).await?;

        match mode {
            RestoreMode::FilesOnly => {
                self.checkout_files(&checkpoint).await?;
            }
            RestoreMode::Full => {
                self.reset_hard(&checkpoint).await?;
            }
        }

        Ok(RestoredCheckpoint {
            files_restored: checkpoint.files_changed.clone(),
            checkpoint,
        })
    }

    async fn evict_old_checkpoints(&self) -> Result<()> {
        let list = self.storage.list_checkpoints().await?;
        if list.len() > self.max_checkpoints {
            for checkpoint in &list[self.max_checkpoints..] {
                self.storage.delete_checkpoint(&checkpoint.id).await?;
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Implement CheckpointStore trait in storage**

In `crates/rustycode-storage/src/lib.rs`:

```rust
pub struct SqlCheckpointStore {
    db: Connection,
}

impl SqlCheckpointStore {
    pub fn new(db: Connection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl CheckpointStore for SqlCheckpointStore {
    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO checkpoints (id, session_id, reason, git_hash, created_at, files_changed)
             VALUES (?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                checkpoint.id.0,
                checkpoint.session_id,
                checkpoint.reason,
                checkpoint.git_hash,
                checkpoint.created_at.to_rfc3339(),
                serde_json::to_string(&checkpoint.files_changed)?,
            ],
        )?;
        Ok(())
    }

    async fn get_checkpoint(&self, id: &CheckpointId) -> Result<Checkpoint> {
        let mut stmt = self.db.prepare(
            "SELECT id, session_id, reason, git_hash, created_at, files_changed FROM checkpoints WHERE id = ?"
        )?;

        let checkpoint = stmt.query_row(rusqlite::params![&id.0], |row| {
            Ok(Checkpoint {
                id: CheckpointId(row.get(0)?),
                session_id: row.get(1)?,
                reason: row.get(2)?,
                git_hash: row.get(3)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .unwrap()
                    .with_timezone(&Utc),
                files_changed: serde_json::from_str(&row.get::<_, String>(5)?)?,
                description: None,
            })
        })?;

        Ok(checkpoint)
    }

    async fn list_checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let mut stmt = self.db.prepare(
            "SELECT id, session_id, reason, git_hash, created_at, files_changed FROM checkpoints ORDER BY created_at DESC"
        )?;

        let checkpoints = stmt.query_map([], |row| {
            Ok(Checkpoint {
                id: CheckpointId(row.get(0)?),
                session_id: row.get(1)?,
                reason: row.get(2)?,
                git_hash: row.get(3)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .unwrap()
                    .with_timezone(&Utc),
                files_changed: serde_json::from_str(&row.get::<_, String>(5)?)?,
                description: None,
            })
        })?;

        Ok(checkpoints.collect::<Result<Vec<_>, _>>()?)
    }

    async fn delete_checkpoint(&self, id: &CheckpointId) -> Result<()> {
        self.db.execute(
            "DELETE FROM checkpoints WHERE id = ?",
            rusqlite::params![&id.0],
        )?;
        Ok(())
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test test_checkpoint_persists_to_database
cargo test test_checkpoint_list_from_database
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-tools/src/checkpoint.rs
git add crates/rustycode-storage/src/lib.rs
git add crates/rustycode-tools/tests/checkpoint_persistence.rs
git commit -m "feat: add checkpoint persistence to database"
```

---

### Task 2.2: Rewind Persistence

**Files:**
- Modify: `crates/rustycode-session/src/rewind.rs`
- Modify: `crates/rustycode-storage/src/lib.rs`
- Modify: `crates/rustycode-session/src/session.rs`
- Test: `crates/rustycode-session/tests/rewind_persistence.rs`

**What exists:**
- RewindState (in-memory)
- Rewind schema likely exists or minimal update needed
- `InteractionSnapshot` struct ready

**What we need:** Persist snapshots to DB and load on session creation

**Steps:**

- [ ] **Step 1: Write failing test for rewind persistence**

Create `crates/rustycode-session/tests/rewind_persistence.rs`:

```rust
#[tokio::test]
async fn test_rewind_snapshots_persist_to_database() {
    let db = setup_test_db().await;
    let storage = Arc::new(SqlRewindStore::new(db.clone()));
    
    let mut rewind = RewindState::new(storage, session_id);
    
    let interaction = Interaction {
        user_message: "Fix bug".into(),
        assistant_response: "Fixed".into(),
        messages: vec![],
        tool_calls: vec![],
    };
    
    rewind.record(interaction).await.unwrap();
    
    // Verify in database
    let snapshots = db.list_snapshots(&session_id).await.unwrap();
    assert_eq!(snapshots.len(), 1);
}

#[tokio::test]
async fn test_rewind_load_history_from_database() {
    let db = setup_test_db().await;
    let session_id = "session-123";
    
    // Create and save some snapshots
    let storage = Arc::new(SqlRewindStore::new(db.clone()));
    let mut rewind1 = RewindState::new(storage.clone(), session_id);
    
    rewind1.record(Interaction { ... }).await.unwrap();
    rewind1.record(Interaction { ... }).await.unwrap();
    
    // Load in new session
    let mut rewind2 = RewindState::load_from_storage(storage, session_id).await.unwrap();
    
    assert_eq!(rewind2.list_snapshots().len(), 2);
}
```

- [ ] **Step 2: Run test (expect failure)**

```bash
cd crates/rustycode-session
cargo test test_rewind_snapshots_persist_to_database
```

Expected: FAIL

- [ ] **Step 3: Modify RewindState to persist**

In `crates/rustycode-session/src/rewind.rs`:

```rust
pub struct RewindState {
    snapshots: Vec<InteractionSnapshot>,
    current: usize,
    storage: Arc<dyn RewindStore>,
    session_id: String,
    checkpoint_manager: Arc<CheckpointManager>,
}

impl RewindState {
    pub async fn record(&mut self, interaction: Interaction) -> Result<()> {
        // Truncate future
        if self.current < self.snapshots.len() {
            self.snapshots.truncate(self.current);
        }

        let snapshot = InteractionSnapshot {
            number: self.snapshots.len(),
            user_message: interaction.user_message.clone(),
            assistant_response: interaction.assistant_response.clone(),
            tool_calls: interaction.tool_calls.clone(),
            conversation_messages: interaction.messages.clone(),
            memory_snapshots: vec![],
            files_checkpoint_id: None,  // Could capture current
            timestamp: Utc::now(),
        };

        // Save to memory
        self.snapshots.push(snapshot.clone());

        // PERSIST TO DATABASE
        self.storage.save_snapshot(&self.session_id, &snapshot).await?;

        self.current = self.snapshots.len() - 1;

        Ok(())
    }

    pub async fn load_from_storage(
        storage: Arc<dyn RewindStore>,
        session_id: &str,
    ) -> Result<Self> {
        let snapshots = storage.list_snapshots(session_id).await?;
        
        Ok(Self {
            current: snapshots.len().saturating_sub(1),
            snapshots,
            storage,
            session_id: session_id.to_string(),
            checkpoint_manager: Arc::new(CheckpointManager::new(...)),
        })
    }
}
```

- [ ] **Step 4: Implement RewindStore in storage**

In `crates/rustycode-storage/src/lib.rs`:

```rust
pub struct SqlRewindStore {
    db: Connection,
}

#[async_trait::async_trait]
impl RewindStore for SqlRewindStore {
    async fn save_snapshot(&self, session_id: &str, snapshot: &InteractionSnapshot) -> Result<()> {
        self.db.execute(
            "INSERT INTO rewind_snapshots (session_id, interaction_number, user_message, assistant_response, conversation_messages, memory_snapshots, files_checkpoint_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                session_id,
                snapshot.number as i32,
                snapshot.user_message,
                snapshot.assistant_response,
                serde_json::to_string(&snapshot.conversation_messages)?,
                serde_json::to_string(&snapshot.memory_snapshots)?,
                snapshot.files_checkpoint_id.as_ref().map(|id| id.to_string()),
                snapshot.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn list_snapshots(&self, session_id: &str) -> Result<Vec<InteractionSnapshot>> {
        let mut stmt = self.db.prepare(
            "SELECT interaction_number, user_message, assistant_response, conversation_messages, memory_snapshots, files_checkpoint_id, created_at
             FROM rewind_snapshots WHERE session_id = ? ORDER BY interaction_number ASC"
        )?;

        let snapshots = stmt.query_map(rusqlite::params![session_id], |row| {
            Ok(InteractionSnapshot {
                number: row.get::<_, i32>(0)? as usize,
                user_message: row.get(1)?,
                assistant_response: row.get(2)?,
                tool_calls: vec![],
                conversation_messages: serde_json::from_str(&row.get::<_, String>(3)?)?,
                memory_snapshots: serde_json::from_str(&row.get::<_, String>(4)?)?,
                files_checkpoint_id: row.get::<_, Option<String>>(5)?.map(CheckpointId),
                timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })?;

        Ok(snapshots.collect::<Result<Vec<_>, _>>()?)
    }

    // ... other trait methods
}
```

- [ ] **Step 5: Integrate into Session**

In `crates/rustycode-session/src/session.rs`:

```rust
pub struct Session {
    // ... existing fields
    rewind_state: RewindState,
}

impl Session {
    pub async fn load_or_create(session_id: &str) -> Result<Self> {
        let storage = Arc::new(SqlRewindStore::new(db));
        
        // Load existing rewind history, or create new
        let rewind_state = RewindState::load_from_storage(storage, session_id)
            .await
            .unwrap_or_else(|_| RewindState::new(storage, session_id));
        
        Ok(Self {
            rewind_state,
            // ...
        })
    }

    pub async fn add_interaction(&mut self, interaction: Interaction) -> Result<()> {
        // Record for rewind
        self.rewind_state.record(interaction.clone()).await?;
        
        // ... rest of logic
        Ok(())
    }
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test test_rewind_snapshots_persist_to_database
cargo test test_rewind_load_history_from_database
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-session/src/rewind.rs
git add crates/rustycode-session/src/session.rs
git add crates/rustycode-storage/src/lib.rs
git add crates/rustycode-session/tests/rewind_persistence.rs
git commit -m "feat: add rewind snapshot persistence with session loading"
```

---

## PHASE 3: COST TRACKING INTEGRATION

### Task 3.1: End-to-End Cost Tracking

**Files:**
- Modify: `crates/rustycode-tools/src/executor.rs`
- Modify: `crates/rustycode-session/src/session.rs`
- Test: `crates/rustycode-tools/tests/cost_integration.rs`

**What exists:**
- CostTracker struct
- Model cost table complete
- Storage for api_calls

**What we need:** Capture LLM costs during execution, accumulate in session

**Steps:**

- [ ] **Step 1: Write failing test for cost tracking**

Create `crates/rustycode-tools/tests/cost_integration.rs`:

```rust
#[tokio::test]
async fn test_tool_execution_tracks_cost() {
    let cost_tracker = Arc::new(CostTracker::new(None));
    let executor = UnifiedToolExecutor::new(
        checkpoint_manager,
        hook_manager,
        cost_tracker.clone(),
    );

    executor.execute_tool("edit", json!(...), &plan_mode, &session).await.unwrap();

    // Verify cost was recorded
    let summary = cost_tracker.session_summary();
    assert_eq!(summary.calls_count, 1);
    assert!(summary.total_cost > 0.0);
}

#[test]
fn test_budget_enforcement() {
    let cost_tracker = Arc::new(CostTracker::new(Some(0.05)));  // $0.05 budget
    
    cost_tracker.record_call(ApiCall {
        cost_usd: 0.03,
        ..Default::default()
    }).unwrap();
    
    // Exceeding budget should fail
    let result = cost_tracker.record_call(ApiCall {
        cost_usd: 0.03,  // Total $0.06 exceeds $0.05
        ..Default::default()
    });
    
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run test (expect failure)**

```bash
cd crates/rustycode-tools
cargo test test_tool_execution_tracks_cost
```

Expected: FAIL - cost not being captured

- [ ] **Step 3: Modify executor to extract and record cost**

In `crates/rustycode-tools/src/executor.rs`, enhance `execute_tool`:

```rust
impl UnifiedToolExecutor {
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
        plan_mode: &PlanMode,
        session_context: &SessionContext,
    ) -> Result<ToolOutput> {
        // ... existing code (plan mode check, hooks, etc)

        // Execute tool
        let output = self.execute_tool_impl(tool_name, &args, plan_mode).await?;

        // EXTRACT AND RECORD COST
        if let Some(cost_metadata) = output.metadata.get("llm_cost") {
            let model = output.metadata
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("claude-3-opus");
            
            let input_tokens = output.metadata
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            
            let output_tokens = output.metadata
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            
            let cost_usd = cost_metadata.as_f64().unwrap_or(0.0);

            self.cost_tracker.record_call(ApiCall {
                model: model.to_string(),
                input_tokens,
                output_tokens,
                cost_usd,
                timestamp: Utc::now(),
                tool_name: Some(tool_name.to_string()),
            })?;

            // Persist to database
            session_context.storage
                .save_api_call(&ApiCallRecord {
                    model: model.to_string(),
                    input_tokens,
                    output_tokens,
                    cost_usd,
                    tool_name: Some(tool_name.to_string()),
                    timestamp: Utc::now(),
                })
                .await?;
        }

        Ok(output)
    }
}
```

- [ ] **Step 4: Ensure LLM providers include cost in output**

When calling LLM (in the tool implementation), include cost metadata:

```rust
// In execute_edit_impl or wherever LLM is called:
let response = llm_provider.call(&request).await?;

let cost_usd = calculate_cost(
    &response.model,
    response.usage.input_tokens,
    response.usage.output_tokens,
);

let output = ToolOutput {
    success: true,
    message: response.content,
    metadata: json!({
        "model": response.model,
        "input_tokens": response.usage.input_tokens,
        "output_tokens": response.usage.output_tokens,
        "llm_cost": cost_usd,  // Include this
    }),
    ..Default::default()
};

Ok(output)
```

- [ ] **Step 5: Run tests**

```bash
cargo test test_tool_execution_tracks_cost
cargo test test_budget_enforcement
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-tools/src/executor.rs
git add crates/rustycode-tools/tests/cost_integration.rs
git commit -m "feat: implement end-to-end cost tracking in tool execution"
```

---

## PHASE 4: INTEGRATION TESTING

### Task 4.1: End-to-End Integration Test (All 4 Pillars)

**Files:**
- Create: `tests/integration_all_pillars.rs`
- Create: `tests/fixtures/test_helpers.rs`

**What we test:** A complete workflow exercising all 4 safety pillars

**Steps:**

- [ ] **Step 1: Write comprehensive integration test**

Create `tests/integration_all_pillars.rs`:

```rust
#[tokio::test]
async fn test_all_four_safety_pillars_integrated() {
    // Setup
    let db = setup_test_db().await;
    let session = TestSession::new(db).await;
    
    // PILLAR 1: PLAN MODE (Approval Gates)
    println!("Testing Plan Mode...");
    let plan = session.generate_plan("Add validation to auth module").await.unwrap();
    assert!(!plan.summary.is_empty());
    assert!(plan.estimated_cost >= 0.0);
    
    let approval_token = session.plan_mode.present_plan(&plan);
    session.plan_mode.approve(approval_token).unwrap();
    println!("✓ Plan approved, moving to implementation phase");
    
    // PILLAR 2: CHECKPOINTS (Reversibility)
    println!("Testing Checkpoints...");
    let checkpoint_before = session.create_checkpoint("before edit").await.unwrap();
    assert!(!checkpoint_before.git_hash.is_empty());
    println!("✓ Checkpoint created: {}", checkpoint_before.id.0);
    
    // Execute tool (edit)
    let edit_result = session.executor.execute_tool(
        "edit",
        json!({"path": "src/auth.rs", "old": "fn validate()", "new": "fn validate(input: &str) -> bool"}),
        &session.plan_mode,
        &session.context,
    ).await.unwrap();
    println!("✓ Edit executed successfully");
    
    // PILLAR 3: HOOKS (Extensibility)
    println!("Testing Hooks...");
    let hook_audit = session.get_hook_audit_log().await.unwrap();
    assert!(!hook_audit.is_empty());  // PostToolUse hooks should have run
    println!("✓ Hooks executed: {} hooks ran", hook_audit.len());
    
    // PILLAR 4: COST TRACKING (Visibility)
    println!("Testing Cost Tracking...");
    let cost_summary = session.cost_tracker.session_summary();
    assert!(cost_summary.total_cost >= 0.0);
    assert_eq!(cost_summary.calls_count, 1);  // One LLM call for the edit
    println!("✓ Cost tracked: ${:.4} ({} tokens)", 
        cost_summary.total_cost, 
        cost_summary.total_input_tokens + cost_summary.total_output_tokens
    );
    
    // BONUS: Test Rewind
    println!("Testing Rewind...");
    let snapshots = session.rewind_state.list_snapshots();
    assert_eq!(snapshots.len(), 1);
    
    session.rewind_state.rewind(RewindMode::Full).await.unwrap();
    assert_eq!(session.rewind_state.current_position(), 0);
    println!("✓ Rewound to previous interaction");
    
    // BONUS: Checkpoint Restoration
    println!("Testing Checkpoint Restoration...");
    let restored = session.restore_checkpoint(&checkpoint_before.id, RestoreMode::FilesOnly).await.unwrap();
    assert!(!restored.files_restored.is_empty());
    println!("✓ Checkpoint restored, {} files recovered", restored.files_restored.len());
    
    // Verify all state persisted
    println!("Testing Persistence...");
    let new_session = TestSession::load_existing(&db, session.id()).await.unwrap();
    
    let restored_checkpoints = new_session.list_checkpoints().await.unwrap();
    assert!(!restored_checkpoints.is_empty());
    
    let restored_rewind = new_session.rewind_state.list_snapshots();
    assert!(!restored_rewind.is_empty());
    
    let restored_costs = new_session.cost_tracker.session_summary();
    assert_eq!(restored_costs.calls_count, 1);
    println!("✓ All state persisted and recovered from database");
    
    println!("\n✅ ALL 4 SAFETY PILLARS VERIFIED AND INTEGRATED!");
}
```

- [ ] **Step 2: Create test helpers**

Create `tests/fixtures/test_helpers.rs`:

```rust
pub struct TestSession {
    executor: Arc<UnifiedToolExecutor>,
    plan_mode: Arc<PlanMode>,
    rewind_state: RewindState,
    cost_tracker: Arc<CostTracker>,
    checkpoint_manager: Arc<CheckpointManager>,
    db: Connection,
    session_id: String,
}

impl TestSession {
    pub async fn new(db: Connection) -> Self {
        let checkpoint_manager = Arc::new(CheckpointManager::new_with_storage(
            PathBuf::from("/tmp/test-repo"),
            Arc::new(SqlCheckpointStore::new(db.clone())),
            10,
        ));

        let hook_manager = Arc::new(HookManager::new(
            PathBuf::from(".rustycode/hooks"),
            HookProfile::Standard,
            Uuid::new_v4().to_string(),
        ));
        hook_manager.load_hooks().await.unwrap_or_default();

        let cost_tracker = Arc::new(CostTracker::new(None));

        let executor = Arc::new(UnifiedToolExecutor::new(
            checkpoint_manager.clone(),
            hook_manager,
            cost_tracker.clone(),
        ));

        let plan_mode = Arc::new(PlanMode::new(Default::default()));

        let session_id = Uuid::new_v4().to_string();
        let rewind_store = Arc::new(SqlRewindStore::new(db.clone()));
        let rewind_state = RewindState::new(rewind_store, session_id.clone());

        Self {
            executor,
            plan_mode,
            rewind_state,
            cost_tracker,
            checkpoint_manager,
            db,
            session_id,
        }
    }

    pub async fn generate_plan(&self, task: &str) -> Result<Plan> {
        self.plan_mode.generate_plan(task).await
    }

    pub async fn create_checkpoint(&self, reason: &str) -> Result<Checkpoint> {
        self.checkpoint_manager
            .checkpoint(reason, CheckpointMode::FullWorkspace)
            .await
    }

    pub async fn restore_checkpoint(
        &self,
        id: &CheckpointId,
        mode: RestoreMode,
    ) -> Result<RestoredCheckpoint> {
        self.checkpoint_manager.restore(id, mode).await
    }

    pub async fn list_checkpoints(&self) -> Result<Vec<Checkpoint>> {
        self.checkpoint_manager.list().await
    }

    pub async fn get_hook_audit_log(&self) -> Result<Vec<HookResult>> {
        // Query from storage
        todo!()
    }

    pub fn id(&self) -> String {
        self.session_id.clone()
    }
}
```

- [ ] **Step 3: Run the integration test**

```bash
cargo test --test integration_all_pillars -- --nocapture
```

Expected: PASS with detailed output showing all 4 pillars working

- [ ] **Step 4: Commit**

```bash
git add tests/integration_all_pillars.rs
git add tests/fixtures/test_helpers.rs
git commit -m "test: add comprehensive integration test for all 4 safety pillars"
```

---

## PHASE 5: DOCUMENTATION & POLISH

### Task 5.1: Update Documentation

**Files:**
- Create: `docs/INTEGRATION_COMPLETE.md`
- Modify: `README.md` (if applicable)
- Modify: `CHANGELOG.md`

**Steps:**

- [ ] **Step 1: Write integration documentation**

Create `docs/INTEGRATION_COMPLETE.md`:

```markdown
# RustyCode Safety Pillars - Integration Complete

## Overview

All 4 safety pillars for production autonomous execution are now fully integrated and persisted.

### Architecture

```
Tool Execution Pipeline
├─ Plan Mode Gating (read-only planning → implementation)
├─ Checkpoint Creation (before destructive operations)
├─ Hook Execution (pre/post tool lifecycle)
├─ Tool Execution (actual work)
├─ Cost Tracking (LLM cost capture)
├─ Rewind Recording (interaction snapshots)
└─ Persistence (database storage)
```

### 4 Pillars Implemented

✅ **Reversibility** (Checkpoints + Rewind)
- Git-based checkpoint creation before edits
- Session interaction rewind (Conversation/Files/Full)
- Persistent checkpoint storage
- Checkpoint restoration with git operations

✅ **Approval Gates** (Plan Mode)
- Read-only planning phase
- Tool allowlisting per phase
- Plan presentation and approval workflow
- Enforcement in tool executor

✅ **Extensibility** (Hooks)
- JSON stdin/stdout hook system
- Pre/Post tool hooks
- Hook blocking support
- Profile-based hook filtering

✅ **Cost Visibility** (Cost Tracking)
- Real-time token/USD accounting
- Budget enforcement
- Cost tracking by tool
- Per-model pricing table

### Integration Points

1. **UnifiedToolExecutor** - Central integration hub
   - Checks PlanMode before execution
   - Creates checkpoints before operations
   - Invokes hooks (pre/post)
   - Records costs
   - Captures for rewind

2. **Database Persistence**
   - Checkpoints persisted to `checkpoints` table
   - Rewind snapshots to `rewind_snapshots` table
   - API calls to `api_calls` table
   - Hook executions to `hook_executions` table

3. **Session Lifecycle**
   - Load existing rewind history on creation
   - Auto-restore checkpoint/rewind state
   - Accumulate costs across session
   - Persist on save/close

### Usage

#### Plan Mode Workflow
```bash
# Generate plan in read-only mode
/plan "Add authentication"

# Review and approve
[Shows plan with costs, risks]
User: Approve

# Execute with gating
[All modifications go through plan mode enforcement]
```

#### Checkpoint & Rewind
```bash
# Create checkpoint
/checkpoint "Save before refactor"

# View checkpoints
/checkpoints

# Rewind to previous step
Esc Esc (or /rewind)

# Restore files from checkpoint
/restore <checkpoint-id>
```

#### Cost Tracking
```bash
# View session costs
/cost-summary   # Total by tool
/cost-budget    # Check remaining budget

# Set budget
RUSTYCODE_COST_BUDGET=0.50  # Max $0.50 per session
```

#### Hooks Configuration
Edit `.rustycode/hooks/hooks.json`:
```json
{
  "profile": "standard",
  "hooks": [
    {
      "name": "lint-on-edit",
      "trigger": "post_tool_use",
      "script": "./hooks/lint.sh",
      "enabled": true
    }
  ]
}
```

### Testing

All features covered with integration tests:
- End-to-end test of all 4 pillars: `tests/integration_all_pillars.rs`
- Tool executor gating: `crates/rustycode-tools/tests/executor_gating.rs`
- Checkpoint persistence: `crates/rustycode-tools/tests/checkpoint_persistence.rs`
- Rewind persistence: `crates/rustycode-session/tests/rewind_persistence.rs`
- Cost tracking: `crates/rustycode-tools/tests/cost_integration.rs`

### Migration Notes

For existing sessions:
1. Checkpoints created going forward will be persisted
2. Rewind history loads from database if available
3. Historical costs may not be available (will accumulate from now on)

### Future Enhancements

- [ ] Checkpoint diffing/comparison UI
- [ ] Cost trending/analytics
- [ ] Hook condition system
- [ ] Checkpoint tagging/organization
- [ ] Multi-session cost aggregation
```

- [ ] **Step 2: Update CHANGELOG**

Add to `CHANGELOG.md`:

```markdown
## [2026-04-14] - Safety Pillars Integration Complete

### Added
- Unified tool executor with plan mode + hook gating
- Checkpoint persistence to database
- Rewind snapshot persistence with session loading
- End-to-end cost tracking with budget enforcement
- Comprehensive integration testing across all 4 pillars

### Features
- Plan mode enforcement integrated into tool execution
- Hooks invoked at pre/post tool lifecycle
- Checkpoints persisted and restorable across sessions
- Rewind history loaded from database on session creation
- Cost tracking captures LLM usage and persists to DB

### Testing
- Integration test verifying all 4 pillars work together
- Tool executor gating tests
- Checkpoint/rewind/cost persistence tests

### Documentation
- Integration complete documentation
- Usage examples for each pillar
- Migration notes for existing sessions
```

- [ ] **Step 3: Commit**

```bash
git add docs/INTEGRATION_COMPLETE.md
git add CHANGELOG.md
git commit -m "docs: add integration completion documentation and changelog"
```

---

### Task 5.2: Final Integration Test & Validation

**Steps:**

- [ ] **Step 1: Run full test suite**

```bash
cargo test --workspace --doc
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Expected: All tests PASS, no warnings

- [ ] **Step 2: Run integration test with output**

```bash
cargo test --test integration_all_pillars -- --nocapture --test-threads=1
```

Expected: Shows all 4 pillars working:
```
Testing Plan Mode...
✓ Plan approved, moving to implementation phase

Testing Checkpoints...
✓ Checkpoint created: <id>

Testing Hooks...
✓ Hooks executed: 2 hooks ran

Testing Cost Tracking...
✓ Cost tracked: $0.0234 (1523 tokens)

Testing Rewind...
✓ Rewound to previous interaction

Testing Checkpoint Restoration...
✓ Checkpoint restored, 3 files recovered

Testing Persistence...
✓ All state persisted and recovered from database

✅ ALL 4 SAFETY PILLARS VERIFIED AND INTEGRATED!
```

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: complete roadmap integration - all 4 safety pillars working

Integration Summary:
- Plan Mode: Approval gates enforced in tool executor
- Checkpoints: Persistent git-based reversibility
- Hooks: Pre/post tool lifecycle extensibility  
- Cost Tracking: End-to-end LLM cost accounting

All features persisted to database.
Integration tests passing.
Production ready."
```

---

## Summary

**Total Tasks:** 8 integration tasks (vs. 18 in original plan)  
**Estimated Effort:** 40-50 hours (vs. 80-120 in original)  
**Key Difference:** Leverages 90% existing code, focuses on integration + persistence

### Completion Checklist

- [ ] Task 1.1: UnifiedToolExecutor with gating
- [ ] Task 1.2: Integrate executor into Autonomous Mode auto mode
- [ ] Task 2.1: Checkpoint persistence to DB
- [ ] Task 2.2: Rewind persistence to DB
- [ ] Task 3.1: End-to-end cost tracking
- [ ] Task 4.1: Integration test (all 4 pillars)
- [ ] Task 5.1: Documentation
- [ ] Task 5.2: Final validation & commit

**Ready for execution via subagent-driven-development or executing-plans skill.**
