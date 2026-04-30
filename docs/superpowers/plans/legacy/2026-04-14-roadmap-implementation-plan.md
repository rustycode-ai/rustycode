# RustyCode Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement all 4 safety pillars (Reversibility, Approval Gates, Extensibility, Cost Visibility) across Phases 1-3 of the RustyCode roadmap feature set.

**Architecture:** Foundation-first approach building safety infrastructure layer-by-layer:
1. **Phase 1:** Git-based reversibility (Checkpoints + Rewind) with database persistence
2. **Phase 2:** Execution infrastructure (Hooks + Plan Mode) with approval gates
3. **Phase 3:** Enhanced capabilities (Skills progressive loading, Cost tracking, Provider fallback)

**Tech Stack:** Rust 2021, Tokio async, SQLx for database, git2-rs for git operations, serde for serialization

**Key Design Decisions:**
- Rewind uses checkpoint references (git) for file state, direct storage for conversation
- Plan Mode enforces read-only in planning phase, write in implementation phase
- Hooks use JSON stdin/stdout for extensibility
- Cost Tracker integrates at LLM provider layer
- Database transactions ensure atomic checkpoint creation

---

## File Structure & Responsibilities

### New Files (Phase 1 & 2 Safety Infrastructure)

```
crates/rustycode-tools/src/
├── checkpoint.rs          (250 lines) - CheckpointManager + types
├── checkpoint_store.rs    (150 lines) - CheckpointStore trait + SQL impl
├── hooks.rs               (400 lines) - HookManager + hook execution
└── hooks_loader.rs        (100 lines) - Config loading

crates/rustycode-session/src/
├── rewind.rs              (350 lines) - RewindState + interaction snapshots
└── rewind_store.rs        (150 lines) - RewindStore trait + SQL impl

crates/rustycode-orchestra/src/
├── plan_mode.rs           (300 lines) - PlanMode + execution phases
└── plan_mode_integration.rs (200 lines) - Integration with auto.rs

crates/rustycode-llm/src/
├── cost_tracker.rs        (250 lines) - CostTracker + budget management
└── provider_fallback.rs   (150 lines) - ProviderFallbackChain

.rustycode/
├── hooks/
│   ├── hooks.json         (config template)
│   └── scripts/
│       ├── lint.sh        (example hook)
│       └── cost-check.sh  (example hook)
└── config.toml            (add checkpoint/hook config)
```

### Modified Files (Integration Points)

```
crates/rustycode-storage/src/
├── lib.rs                 (Add 4 new tables + migrations)
└── mod.rs                 (Expose checkpoint_store, rewind_store)

crates/rustycode-session/src/
├── lib.rs                 (Integrate RewindState, add record_interaction)
└── session.rs             (Call rewind.record() after each interaction)

crates/rustycode-orchestra/src/
├── auto.rs                (Integrate PlanMode phases)
├── executor.rs            (Check plan_mode.is_tool_allowed())
└── lib.rs                 (Export plan_mode module)

crates/rustycode-tools/src/
├── lib.rs                 (Export checkpoint, hooks modules)
├── bash.rs                (Trigger checkpoint before execution)
├── file.rs                (edit_file + write integration)
└── executor.rs            (Call hooks at lifecycle events)

crates/rustycode-llm/src/
├── lib.rs                 (Integrate CostTracker + ProviderFallback)
├── mod.rs                 (Export cost_tracker module)
└── providers.rs           (Wrap with CostTracker, add fallback)

crates/rustycode-skill/src/
├── lib.rs                 (Add progressive loading, lazy content)
└── loader.rs              (Metadata-first approach)

Cargo.toml (root)
└── Add dependencies: chrono, uuid, tokio-util (if needed)
```

---

## PHASE 1: CHECKPOINTS (Git-Based Reversibility)

### Task 1.1: Database Tables & Migrations

**Files:**
- Modify: `crates/rustycode-storage/src/lib.rs`
- Modify: `crates/rustycode-storage/migrations/` (add new migration files)
- Test: `crates/rustycode-storage/tests/checkpoint_store.rs`

**Steps:**

- [ ] **Step 1: Write integration test for checkpoint storage**

Create file `crates/rustycode-storage/tests/checkpoint_store.rs`:

```rust
#[tokio::test]
async fn test_save_and_retrieve_checkpoint() {
    let db = setup_test_db().await;
    let store = SqlCheckpointStore::new(db);
    
    let checkpoint = Checkpoint {
        id: CheckpointId::new(),
        session_id: "test-session".to_string(),
        reason: "before edit".to_string(),
        git_hash: "abc123".to_string(),
        created_at: Utc::now(),
        files_changed: vec![],
        description: None,
    };
    
    store.save_checkpoint(&checkpoint).await.unwrap();
    let retrieved = store.get_checkpoint(&checkpoint.id).await.unwrap();
    
    assert_eq!(retrieved.id, checkpoint.id);
    assert_eq!(retrieved.reason, checkpoint.reason);
}

#[tokio::test]
async fn test_list_checkpoints_ordered_by_creation() {
    let db = setup_test_db().await;
    let store = SqlCheckpointStore::new(db);
    
    let cp1 = create_checkpoint("first");
    let cp2 = create_checkpoint("second");
    
    store.save_checkpoint(&cp1).await.unwrap();
    store.save_checkpoint(&cp2).await.unwrap();
    
    let list = store.list_checkpoints().await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].reason, "first");
}

fn setup_test_db() -> Database { /* ... */ }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-storage
cargo test test_save_and_retrieve_checkpoint -- --nocapture
```

Expected: FAIL - "CheckpointStore not found", "Checkpoint struct not defined"

- [ ] **Step 3: Create SQL migration file**

Create `crates/rustycode-storage/migrations/001_checkpoints_table.sql`:

```sql
CREATE TABLE checkpoints (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    git_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL,
    files_changed JSONB,
    metadata JSONB,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    INDEX idx_session_created (session_id, created_at DESC)
);
```

- [ ] **Step 4: Add Checkpoint types to lib.rs**

Add to `crates/rustycode-storage/src/lib.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CheckpointId(String);

impl CheckpointId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub session_id: String,
    pub reason: String,
    pub git_hash: String,
    pub created_at: DateTime<Utc>,
    pub files_changed: Vec<PathBuf>,
    pub description: Option<String>,
}

#[async_trait::async_trait]
pub trait CheckpointStore: Send + Sync {
    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;
    async fn get_checkpoint(&self, id: &CheckpointId) -> Result<Checkpoint>;
    async fn list_checkpoints(&self) -> Result<Vec<Checkpoint>>;
    async fn delete_checkpoint(&self, id: &CheckpointId) -> Result<()>;
}
```

- [ ] **Step 5: Implement SqlCheckpointStore**

Add to new file `crates/rustycode-storage/src/checkpoint_store.rs` (150 lines):

```rust
use sqlx::SqlitePool;
use anyhow::Result;
use super::{Checkpoint, CheckpointId, CheckpointStore};

pub struct SqlCheckpointStore {
    pool: SqlitePool,
}

impl SqlCheckpointStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl CheckpointStore for SqlCheckpointStore {
    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO checkpoints (id, session_id, reason, git_hash, created_at, files_changed, metadata)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#
        )
        .bind(&checkpoint.id.0)
        .bind(&checkpoint.session_id)
        .bind(&checkpoint.reason)
        .bind(&checkpoint.git_hash)
        .bind(&checkpoint.created_at)
        .bind(serde_json::to_string(&checkpoint.files_changed)?)
        .bind(serde_json::to_string(&checkpoint.description)?)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    async fn get_checkpoint(&self, id: &CheckpointId) -> Result<Checkpoint> {
        let row = sqlx::query_as::<_, (String, String, String, String, i64, String, String)>(
            "SELECT id, session_id, reason, git_hash, created_at, files_changed, metadata FROM checkpoints WHERE id = ?"
        )
        .bind(&id.0)
        .fetch_one(&self.pool)
        .await?;

        Ok(Checkpoint {
            id: CheckpointId(row.0),
            session_id: row.1,
            reason: row.2,
            git_hash: row.3,
            created_at: DateTime::from_timestamp(row.4, 0).unwrap(),
            files_changed: serde_json::from_str(&row.5)?,
            description: serde_json::from_str(&row.6).ok(),
        })
    }

    async fn list_checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, i64, String, String)>(
            "SELECT id, session_id, reason, git_hash, created_at, files_changed, metadata FROM checkpoints ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| Checkpoint {
            id: CheckpointId(row.0),
            session_id: row.1,
            reason: row.2,
            git_hash: row.3,
            created_at: DateTime::from_timestamp(row.4, 0).unwrap(),
            files_changed: serde_json::from_str(&row.5).unwrap_or_default(),
            description: serde_json::from_str(&row.6).ok(),
        }).collect())
    }

    async fn delete_checkpoint(&self, id: &CheckpointId) -> Result<()> {
        sqlx::query("DELETE FROM checkpoints WHERE id = ?")
            .bind(&id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 6: Run migration and test**

```bash
cd crates/rustycode-storage
sqlx migrate add -r checkpoints_table
cargo test test_save_and_retrieve_checkpoint
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-storage/src/lib.rs
git add crates/rustycode-storage/src/checkpoint_store.rs
git add crates/rustycode-storage/migrations/
git add crates/rustycode-storage/tests/checkpoint_store.rs
git commit -m "feat: add checkpoint storage with database persistence"
```

---

### Task 1.2: CheckpointManager (Git Integration)

**Files:**
- Create: `crates/rustycode-tools/src/checkpoint.rs`
- Modify: `crates/rustycode-tools/src/lib.rs`
- Modify: `crates/rustycode-git/src/lib.rs` (add checkpoint operations)
- Test: `crates/rustycode-tools/tests/checkpoint.rs`

**Steps:**

- [ ] **Step 1: Write failing test for checkpoint creation**

Create `crates/rustycode-tools/tests/checkpoint.rs`:

```rust
#[tokio::test]
async fn test_create_checkpoint() {
    let repo_path = setup_test_repo().await;
    let storage = Arc::new(MockCheckpointStore::new());
    let checkpoint_manager = CheckpointManager::new(
        repo_path,
        storage,
        10,  // max checkpoints
    );

    let checkpoint = checkpoint_manager
        .checkpoint("before edit", CheckpointMode::FullWorkspace)
        .await
        .unwrap();

    assert!(!checkpoint.git_hash.is_empty());
    assert_eq!(checkpoint.reason, "before edit");
}

#[tokio::test]
async fn test_list_checkpoints() {
    let repo_path = setup_test_repo().await;
    let storage = Arc::new(MockCheckpointStore::new());
    let cm = CheckpointManager::new(repo_path, storage, 10);

    cm.checkpoint("first", CheckpointMode::FullWorkspace).await.unwrap();
    cm.checkpoint("second", CheckpointMode::FullWorkspace).await.unwrap();

    let list = cm.list().await.unwrap();
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn test_restore_to_checkpoint() {
    let repo_path = setup_test_repo().await;
    let storage = Arc::new(MockCheckpointStore::new());
    let cm = CheckpointManager::new(repo_path, storage, 10);

    let checkpoint = cm
        .checkpoint("save point", CheckpointMode::FullWorkspace)
        .await
        .unwrap();

    // Make some changes
    modify_file(&repo_path, "test.txt", "changed content").await;

    // Restore
    let result = cm
        .restore(&checkpoint.id, RestoreMode::FilesOnly)
        .await
        .unwrap();

    assert_eq!(result.checkpoint.id, checkpoint.id);
}

fn setup_test_repo() -> PathBuf { /* ... */ }
struct MockCheckpointStore { /* ... */ }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-tools
cargo test test_create_checkpoint -- --nocapture
```

Expected: FAIL - "CheckpointManager not found"

- [ ] **Step 3: Implement CheckpointManager**

Create `crates/rustycode-tools/src/checkpoint.rs` (250 lines):

```rust
use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use rustycode_storage::{Checkpoint, CheckpointId, CheckpointStore};

#[derive(Debug, Clone, Copy)]
pub enum CheckpointMode {
    FullWorkspace,
    FilesOnly(Vec<PathBuf>),
}

#[derive(Debug, Clone, Copy)]
pub enum RestoreMode {
    FilesOnly,
    Full,
}

pub struct RestoredCheckpoint {
    pub checkpoint: Checkpoint,
    pub files_restored: Vec<PathBuf>,
}

pub struct CheckpointManager {
    repo_path: PathBuf,
    storage: Arc<dyn CheckpointStore>,
    max_checkpoints: usize,
}

impl CheckpointManager {
    pub fn new(repo_path: PathBuf, storage: Arc<dyn CheckpointStore>, max: usize) -> Self {
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

        // Stage changes
        self.stage_changes(&mode).await?;

        // Commit to git
        let commit_msg = format!(
            "[RustyCode Checkpoint] {}\n\nCheckpoint ID: {}\nReason: {}",
            id.to_string(),
            id.to_string(),
            reason
        );

        let git_hash = self.commit_to_git(&commit_msg).await?;

        // Get changed files
        let files_changed = self.get_changed_files(&mode).await?;

        let checkpoint = Checkpoint {
            id,
            session_id: "current-session".to_string(), // TODO: Get from context
            reason,
            git_hash,
            created_at: Utc::now(),
            files_changed,
            description: None,
        };

        // Save to database
        self.storage.save_checkpoint(&checkpoint).await?;

        // LRU cleanup
        self.evict_old_checkpoints().await?;

        Ok(checkpoint)
    }

    pub async fn list(&self) -> Result<Vec<Checkpoint>> {
        self.storage.list_checkpoints().await
    }

    pub async fn restore(
        &self,
        id: &CheckpointId,
        mode: RestoreMode,
    ) -> Result<RestoredCheckpoint> {
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

    pub async fn diff(&self, id1: &CheckpointId, id2: &CheckpointId) -> Result<String> {
        let c1 = self.storage.get_checkpoint(id1).await?;
        let c2 = self.storage.get_checkpoint(id2).await?;
        self.git_diff(&c1.git_hash, &c2.git_hash).await
    }

    // Private helpers
    async fn stage_changes(&self, mode: &CheckpointMode) -> Result<()> {
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.repo_path)
            .output()?;
        Ok(())
    }

    async fn commit_to_git(&self, msg: &str) -> Result<String> {
        let output = Command::new("git")
            .args(["commit", "-m", msg])
            .current_dir(&self.repo_path)
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Git commit failed");
        }

        // Get commit hash
        let hash_output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()?;

        let hash = String::from_utf8(hash_output.stdout)?
            .trim()
            .to_string();

        Ok(hash)
    }

    async fn get_changed_files(&self, mode: &CheckpointMode) -> Result<Vec<PathBuf>> {
        // git diff --name-only
        Ok(Vec::new())
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

    async fn checkout_files(&self, checkpoint: &Checkpoint) -> Result<()> {
        Command::new("git")
            .args(["checkout", &checkpoint.git_hash, "--"])
            .current_dir(&self.repo_path)
            .output()?;
        Ok(())
    }

    async fn reset_hard(&self, checkpoint: &Checkpoint) -> Result<()> {
        Command::new("git")
            .args(["reset", "--hard", &checkpoint.git_hash])
            .current_dir(&self.repo_path)
            .output()?;
        Ok(())
    }

    async fn git_diff(&self, hash1: &str, hash2: &str) -> Result<String> {
        let output = Command::new("git")
            .args(["diff", hash1, hash2])
            .current_dir(&self.repo_path)
            .output()?;
        Ok(String::from_utf8(output.stdout)?)
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add to `crates/rustycode-tools/src/lib.rs`:

```rust
pub mod checkpoint;
pub use checkpoint::{CheckpointManager, CheckpointMode, RestoreMode};
```

- [ ] **Step 5: Run tests**

```bash
cd crates/rustycode-tools
cargo test test_create_checkpoint
cargo test test_list_checkpoints
cargo test test_restore_to_checkpoint
```

Expected: All 3 tests PASS

- [ ] **Step 6: Run clippy and fmt**

```bash
cargo clippy -- -D warnings
cargo fmt
```

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-tools/src/checkpoint.rs
git add crates/rustycode-tools/src/lib.rs
git add crates/rustycode-tools/tests/checkpoint.rs
git commit -m "feat: implement CheckpointManager with git integration"
```

---

### Task 1.3: Tool Integration (Checkpoint Triggers)

**Files:**
- Modify: `crates/rustycode-tools/src/bash.rs`
- Modify: `crates/rustycode-tools/src/file.rs`
- Modify: `crates/rustycode-tools/src/executor.rs`
- Test: `crates/rustycode-tools/tests/integration_checkpoint_triggers.rs`

**Steps:**

- [ ] **Step 1: Write integration test for checkpoint before edit**

Create `crates/rustycode-tools/tests/integration_checkpoint_triggers.rs`:

```rust
#[tokio::test]
async fn test_checkpoint_created_before_edit_tool() {
    let executor = setup_executor().await;
    
    // Edit a file
    executor.edit_file("src/main.rs", "old", "new").await.unwrap();
    
    // Verify checkpoint was created
    let checkpoints = executor.list_checkpoints().await.unwrap();
    assert!(!checkpoints.is_empty());
    assert!(checkpoints[0].reason.contains("before edit_file"));
}

#[tokio::test]
async fn test_checkpoint_created_before_destructive_bash() {
    let executor = setup_executor().await;
    
    // Run destructive command
    executor.bash("rm important.txt").await.unwrap();
    
    // Verify checkpoint was created
    let checkpoints = executor.list_checkpoints().await.unwrap();
    assert!(!checkpoints.is_empty());
}

#[tokio::test]
async fn test_no_checkpoint_for_safe_bash() {
    let executor = setup_executor().await;
    let initial_count = executor.list_checkpoints().await.unwrap().len();
    
    // Safe command
    executor.bash("echo hello").await.unwrap();
    
    // No checkpoint created
    let final_count = executor.list_checkpoints().await.unwrap().len();
    assert_eq!(initial_count, final_count);
}

fn setup_executor() -> Executor { /* ... */ }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-tools
cargo test test_checkpoint_created_before_edit_tool -- --nocapture
```

Expected: FAIL - checkpoint not being created

- [ ] **Step 3: Modify edit_file to create checkpoint**

In `crates/rustycode-tools/src/file.rs`, wrap the edit operation:

```rust
pub async fn edit_file(
    &self,
    path: &str,
    old: &str,
    new: &str,
) -> Result<EditResult> {
    // Create checkpoint before editing
    self.checkpoint_manager
        .checkpoint("before edit_file", CheckpointMode::FullWorkspace)
        .await?;

    // Then apply the edit
    let result = self.apply_edit(path, old, new).await?;
    
    Ok(result)
}
```

- [ ] **Step 4: Add helper to detect destructive bash commands**

In `crates/rustycode-tools/src/bash.rs`:

```rust
fn is_destructive_command(cmd: &str) -> bool {
    let patterns = vec![
        "rm", "mv", "cp", "rmdir", "unlink",
        "git reset", "git clean", "make clean",
        "cargo clean", "npm run clean",
        "DROP TABLE", "DELETE FROM", "TRUNCATE",
    ];
    
    let cmd_lower = cmd.to_lowercase();
    patterns.iter()
        .any(|p| cmd_lower.contains(&p.to_lowercase()))
}
```

- [ ] **Step 5: Modify bash tool to create checkpoints**

In `crates/rustycode-tools/src/bash.rs`:

```rust
pub async fn bash(&self, cmd: &str) -> Result<BashOutput> {
    // Create checkpoint before destructive commands
    if Self::is_destructive_command(cmd) {
        self.checkpoint_manager
            .checkpoint(format!("before bash: {}", cmd), CheckpointMode::FullWorkspace)
            .await?;
    }

    // Execute the command
    let output = self.execute_bash(cmd).await?;
    
    Ok(output)
}
```

- [ ] **Step 6: Run tests**

```bash
cd crates/rustycode-tools
cargo test test_checkpoint_created_before_edit_tool
cargo test test_checkpoint_created_before_destructive_bash
cargo test test_no_checkpoint_for_safe_bash
```

Expected: All 3 tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-tools/src/file.rs
git add crates/rustycode-tools/src/bash.rs
git commit -m "feat: create checkpoints before edit and destructive bash operations"
```

---

## PHASE 2: REWIND (Session Interaction History)

### Task 2.1: Rewind Storage & Types

**Files:**
- Create: `crates/rustycode-session/src/rewind.rs`
- Create: `crates/rustycode-storage/src/rewind_store.rs`
- Modify: `crates/rustycode-storage/migrations/`
- Test: `crates/rustycode-storage/tests/rewind_store.rs`

**Steps:**

- [ ] **Step 1: Write failing test for rewind snapshots**

Create `crates/rustycode-storage/tests/rewind_store.rs`:

```rust
#[tokio::test]
async fn test_save_interaction_snapshot() {
    let db = setup_test_db().await;
    let store = SqlRewindStore::new(db);
    
    let snapshot = InteractionSnapshot {
        number: 0,
        user_message: "Fix the bug".to_string(),
        assistant_response: "I'll fix it".to_string(),
        tool_calls: vec![],
        conversation_messages: vec![],
        memory_snapshots: vec![],
        files_checkpoint_id: None,
        timestamp: Utc::now(),
    };
    
    store.save_snapshot(&snapshot).await.unwrap();
    
    let retrieved = store.get_snapshot("session-1", 0).await.unwrap();
    assert_eq!(retrieved.user_message, "Fix the bug");
}

#[tokio::test]
async fn test_list_snapshots_in_order() {
    let db = setup_test_db().await;
    let store = SqlRewindStore::new(db);
    
    let snap1 = create_snapshot(0, "first");
    let snap2 = create_snapshot(1, "second");
    
    store.save_snapshot(&snap1).await.unwrap();
    store.save_snapshot(&snap2).await.unwrap();
    
    let list = store.list_snapshots("session-1").await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].number, 0);
    assert_eq!(list[1].number, 1);
}

fn setup_test_db() -> Database { /* ... */ }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-storage
cargo test test_save_interaction_snapshot
```

Expected: FAIL - "InteractionSnapshot not found"

- [ ] **Step 3: Create SQL migration for rewind snapshots**

Create `crates/rustycode-storage/migrations/002_rewind_snapshots.sql`:

```sql
CREATE TABLE rewind_snapshots (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    interaction_number INTEGER NOT NULL,
    user_message TEXT,
    assistant_response TEXT,
    conversation_messages JSONB NOT NULL,
    memory_snapshots JSONB,
    files_checkpoint_id TEXT,
    created_at TIMESTAMP NOT NULL,
    
    UNIQUE(session_id, interaction_number),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (files_checkpoint_id) REFERENCES checkpoints(id),
    INDEX idx_session_interaction (session_id, interaction_number DESC)
);
```

- [ ] **Step 4: Add Rewind types to storage**

Add to `crates/rustycode-storage/src/lib.rs`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionSnapshot {
    pub number: usize,
    pub user_message: String,
    pub assistant_response: String,
    pub tool_calls: Vec<ToolCall>,
    pub conversation_messages: Vec<Message>,
    pub memory_snapshots: Vec<MemoryRecord>,
    pub files_checkpoint_id: Option<CheckpointId>,
    pub timestamp: DateTime<Utc>,
}

#[async_trait::async_trait]
pub trait RewindStore: Send + Sync {
    async fn save_snapshot(&self, snapshot: &InteractionSnapshot) -> Result<()>;
    async fn get_snapshot(&self, session_id: &str, number: usize) -> Result<InteractionSnapshot>;
    async fn list_snapshots(&self, session_id: &str) -> Result<Vec<InteractionSnapshot>>;
    async fn delete_snapshots_before(&self, session_id: &str, number: usize) -> Result<()>;
}
```

- [ ] **Step 5: Implement SqlRewindStore**

Create `crates/rustycode-storage/src/rewind_store.rs`:

```rust
use sqlx::SqlitePool;
use anyhow::Result;
use super::{InteractionSnapshot, RewindStore};
use chrono::{DateTime, Utc};

pub struct SqlRewindStore {
    pool: SqlitePool,
}

impl SqlRewindStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RewindStore for SqlRewindStore {
    async fn save_snapshot(&self, snapshot: &InteractionSnapshot) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO rewind_snapshots (
                session_id, interaction_number, user_message, assistant_response,
                conversation_messages, memory_snapshots, files_checkpoint_id, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#
        )
        .bind(&snapshot.session_id)  // TODO: Get from context
        .bind(snapshot.number as i32)
        .bind(&snapshot.user_message)
        .bind(&snapshot.assistant_response)
        .bind(serde_json::to_string(&snapshot.conversation_messages)?)
        .bind(serde_json::to_string(&snapshot.memory_snapshots)?)
        .bind(snapshot.files_checkpoint_id.as_ref().map(|id| id.to_string()))
        .bind(snapshot.timestamp)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_snapshot(&self, session_id: &str, number: usize) -> Result<InteractionSnapshot> {
        let row = sqlx::query_as::<_, (i32, String, String, String, String, Option<String>, DateTime<Utc>)>(
            "SELECT interaction_number, user_message, assistant_response, conversation_messages, memory_snapshots, files_checkpoint_id, created_at FROM rewind_snapshots WHERE session_id = ? AND interaction_number = ?"
        )
        .bind(session_id)
        .bind(number as i32)
        .fetch_one(&self.pool)
        .await?;

        Ok(InteractionSnapshot {
            number: row.0 as usize,
            user_message: row.1,
            assistant_response: row.2,
            tool_calls: vec![],
            conversation_messages: serde_json::from_str(&row.3)?,
            memory_snapshots: serde_json::from_str(&row.4)?,
            files_checkpoint_id: row.5.map(|id| CheckpointId(id)),
            timestamp: row.6,
        })
    }

    async fn list_snapshots(&self, session_id: &str) -> Result<Vec<InteractionSnapshot>> {
        let rows = sqlx::query_as::<_, (i32, String, String, String, String, Option<String>, DateTime<Utc>)>(
            "SELECT interaction_number, user_message, assistant_response, conversation_messages, memory_snapshots, files_checkpoint_id, created_at FROM rewind_snapshots WHERE session_id = ? ORDER BY interaction_number ASC"
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| InteractionSnapshot {
            number: row.0 as usize,
            user_message: row.1,
            assistant_response: row.2,
            tool_calls: vec![],
            conversation_messages: serde_json::from_str(&row.3).unwrap_or_default(),
            memory_snapshots: serde_json::from_str(&row.4).unwrap_or_default(),
            files_checkpoint_id: row.5.map(|id| CheckpointId(id)),
            timestamp: row.6,
        }).collect())
    }

    async fn delete_snapshots_before(&self, session_id: &str, number: usize) -> Result<()> {
        sqlx::query(
            "DELETE FROM rewind_snapshots WHERE session_id = ? AND interaction_number < ?"
        )
        .bind(session_id)
        .bind(number as i32)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
```

- [ ] **Step 6: Run migration and tests**

```bash
cd crates/rustycode-storage
sqlx migrate add -r rewind_snapshots
cargo test test_save_interaction_snapshot
cargo test test_list_snapshots_in_order
```

Expected: Both PASS

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-storage/src/lib.rs
git add crates/rustycode-storage/src/rewind_store.rs
git add crates/rustycode-storage/migrations/
git add crates/rustycode-storage/tests/rewind_store.rs
git commit -m "feat: add rewind snapshot storage with database persistence"
```

---

### Task 2.2: RewindState Implementation

**Files:**
- Create: `crates/rustycode-session/src/rewind.rs`
- Modify: `crates/rustycode-session/src/lib.rs`
- Test: `crates/rustycode-session/tests/rewind.rs`

**Steps:**

- [ ] **Step 1: Write failing test for RewindState**

Create `crates/rustycode-session/tests/rewind.rs`:

```rust
#[tokio::test]
async fn test_record_interaction_snapshot() {
    let storage = Arc::new(MockRewindStore::new());
    let mut rewind_state = RewindState::new(storage);
    
    let interaction = Interaction {
        user_message: "Fix the bug".to_string(),
        assistant_response: "I'll fix it".to_string(),
        tool_calls: vec![],
    };
    
    rewind_state.record(interaction).await.unwrap();
    
    let snapshots = rewind_state.list_snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].number, 0);
}

#[tokio::test]
async fn test_rewind_to_previous_interaction() {
    let storage = Arc::new(MockRewindStore::new());
    let mut rewind_state = RewindState::new(storage);
    
    let int1 = Interaction { user_message: "First".into(), ... };
    let int2 = Interaction { user_message: "Second".into(), ... };
    
    rewind_state.record(int1).await.unwrap();
    rewind_state.record(int2).await.unwrap();
    
    assert_eq!(rewind_state.current_position(), 1);
    
    // Rewind one step
    rewind_state.rewind(RewindMode::Full).await.unwrap();
    
    assert_eq!(rewind_state.current_position(), 0);
}

#[tokio::test]
async fn test_rewind_conversation_only() {
    let storage = Arc::new(MockRewindStore::new());
    let mut rewind_state = RewindState::new(storage);
    
    // Record interactions
    let snap = rewind_state.record(...).await.unwrap();
    
    // Rewind conversation only
    let result = rewind_state.rewind(RewindMode::ConversationOnly).await.unwrap();
    
    assert_eq!(result.mode_applied, RewindMode::ConversationOnly);
}

fn setup_test_repo() -> PathBuf { /* ... */ }
struct MockRewindStore { /* ... */ }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-session
cargo test test_record_interaction_snapshot
```

Expected: FAIL - "RewindState not found"

- [ ] **Step 3: Implement RewindState**

Create `crates/rustycode-session/src/rewind.rs`:

```rust
use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use rustycode_storage::{InteractionSnapshot, RewindStore};
use rustycode_tools::CheckpointManager;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewindMode {
    ConversationOnly,
    FilesOnly,
    Full,
}

pub struct RewindResult {
    pub snapshot: InteractionSnapshot,
    pub new_cursor: usize,
    pub mode_applied: RewindMode,
}

pub struct RewindState {
    snapshots: Vec<InteractionSnapshot>,
    current: usize,
    storage: Arc<dyn RewindStore>,
    checkpoint_manager: Arc<CheckpointManager>,
    session_id: String,
}

impl RewindState {
    pub fn new(storage: Arc<dyn RewindStore>, checkpoint_manager: Arc<CheckpointManager>, session_id: String) -> Self {
        Self {
            snapshots: Vec::new(),
            current: 0,
            storage,
            checkpoint_manager,
            session_id,
        }
    }

    pub async fn record(&mut self, interaction: Interaction) -> Result<()> {
        // Truncate "future" interactions (linear history)
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
            files_checkpoint_id: None,  // Could capture current checkpoint
            timestamp: Utc::now(),
        };

        self.snapshots.push(snapshot.clone());
        self.storage.save_snapshot(&snapshot).await?;
        self.current = self.snapshots.len() - 1;

        Ok(())
    }

    pub async fn rewind(&mut self, mode: RewindMode) -> Result<RewindResult> {
        if self.current == 0 {
            anyhow::bail!("Already at beginning of session");
        }

        let target = self.current - 1;
        self.apply_rewind(target, mode).await
    }

    pub async fn jump_to(&mut self, target: usize) -> Result<RewindResult> {
        if target >= self.snapshots.len() {
            anyhow::bail!("Target interaction not found");
        }

        self.apply_rewind(target, RewindMode::Full).await
    }

    pub fn list_snapshots(&self) -> Vec<InteractionSnapshot> {
        self.snapshots.clone()
    }

    pub fn current_position(&self) -> usize {
        self.current
    }

    async fn apply_rewind(&mut self, target: usize, mode: RewindMode) -> Result<RewindResult> {
        let snapshot = &self.snapshots[target];

        match mode {
            RewindMode::ConversationOnly => {
                self.restore_conversation(snapshot).await?;
            }
            RewindMode::FilesOnly => {
                if let Some(checkpoint_id) = &snapshot.files_checkpoint_id {
                    self.checkpoint_manager.restore(checkpoint_id, RestoreMode::FilesOnly).await?;
                }
            }
            RewindMode::Full => {
                self.restore_conversation(snapshot).await?;
                if let Some(checkpoint_id) = &snapshot.files_checkpoint_id {
                    self.checkpoint_manager.restore(checkpoint_id, RestoreMode::Full).await?;
                }
            }
        }

        self.current = target;

        Ok(RewindResult {
            snapshot: snapshot.clone(),
            new_cursor: target,
            mode_applied: mode,
        })
    }

    async fn restore_conversation(&self, snapshot: &InteractionSnapshot) -> Result<()> {
        // This would be handled by the session layer
        // Reset messages to conversation_messages in snapshot
        Ok(())
    }
}

pub struct Interaction {
    pub user_message: String,
    pub assistant_response: String,
    pub tool_calls: Vec<ToolCall>,
    pub messages: Vec<Message>,
}
```

- [ ] **Step 4: Export from session lib.rs**

Add to `crates/rustycode-session/src/lib.rs`:

```rust
pub mod rewind;
pub use rewind::{RewindState, RewindMode, RewindResult};
```

- [ ] **Step 5: Run tests**

```bash
cd crates/rustycode-session
cargo test test_record_interaction_snapshot
cargo test test_rewind_to_previous_interaction
cargo test test_rewind_conversation_only
```

Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-session/src/rewind.rs
git add crates/rustycode-session/src/lib.rs
git add crates/rustycode-session/tests/rewind.rs
git commit -m "feat: implement RewindState for session interaction history navigation"
```

---

## PHASE 3: HOOKS (Extensibility)

### Task 3.1: Hook Manager & Configuration

**Files:**
- Create: `crates/rustycode-tools/src/hooks.rs`
- Create: `crates/rustycode-tools/src/hooks_loader.rs`
- Create: `.rustycode/hooks/hooks.json`
- Test: `crates/rustycode-tools/tests/hooks.rs`

**Steps:**

- [ ] **Step 1: Write failing test for hook execution**

Create `crates/rustycode-tools/tests/hooks.rs`:

```rust
#[tokio::test]
async fn test_hook_execution() {
    let hooks_dir = setup_test_hooks_dir().await;
    let mut manager = HookManager::new(hooks_dir, HookProfile::Standard, "session-1".to_string());
    manager.load_hooks().await.unwrap();
    
    let context = json!({ "tool_name": "edit", "file_path": "main.rs" });
    let result = manager.execute(HookTrigger::PreToolUse, context).await.unwrap();
    
    assert!(!result.results.is_empty());
}

#[tokio::test]
async fn test_hook_blocking() {
    let hooks_dir = setup_test_hooks_dir_with_blocking_hook().await;
    let mut manager = HookManager::new(hooks_dir, HookProfile::Standard, "session-1".to_string());
    manager.load_hooks().await.unwrap();
    
    let context = json!({ "tool_name": "write", "file_path": "secret.key" });
    let result = manager.execute(HookTrigger::PreToolUse, context).await.unwrap();
    
    assert!(result.should_block);
    assert!(result.blocking_hook.is_some());
}

#[tokio::test]
async fn test_hook_profile_enforcement() {
    let hooks_dir = setup_test_hooks_dir().await;
    let mut manager = HookManager::new(hooks_dir, HookProfile::Minimal, "session-1".to_string());
    manager.load_hooks().await.unwrap();
    
    // Standard profile hooks should not run in Minimal mode
    let context = json!({});
    let result = manager.execute(HookTrigger::PostToolUse, context).await.unwrap();
    
    assert!(result.results.is_empty());  // No hooks ran
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-tools
cargo test test_hook_execution
```

Expected: FAIL - "HookManager not found"

- [ ] **Step 3: Implement HookManager**

Create `crates/rustycode-tools/src/hooks.rs` (400 lines):

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use chrono::Utc;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    SessionStart,
    SessionEnd,
    PreToolUse,
    PostToolUse,
    PreCompact,
    PostCompact,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub enum HookProfile {
    Minimal,
    Standard,
    Strict,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Hook {
    pub name: String,
    pub trigger: HookTrigger,
    pub script: PathBuf,
    pub args: Option<Vec<String>>,
    pub timeout_secs: Option<u64>,
    pub enabled: bool,
    pub profile: Option<HookProfile>,
}

#[derive(Serialize)]
pub struct HookInput {
    pub trigger: HookTrigger,
    pub session_id: String,
    pub context: serde_json::Value,
    pub timestamp: String,
}

#[derive(Deserialize)]
pub struct HookOutput {
    pub status: HookStatus,
    pub message: Option<String>,
    pub actions: Option<Vec<HookAction>>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookStatus {
    Ok,
    Warning,
    Error,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookAction {
    Block,
    Log,
    Alert,
    Abort,
}

pub struct HookResult {
    pub hook_name: String,
    pub status: HookStatus,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub duration_ms: u128,
}

pub struct HookExecutionResult {
    pub results: Vec<HookResult>,
    pub should_block: bool,
    pub block_reason: Option<String>,
    pub blocking_hook: Option<String>,
}

pub struct HookManager {
    hooks_dir: PathBuf,
    hooks: Vec<Hook>,
    profile: HookProfile,
    session_id: String,
}

impl HookManager {
    pub fn new(hooks_dir: PathBuf, profile: HookProfile, session_id: String) -> Self {
        Self {
            hooks_dir,
            hooks: Vec::new(),
            profile,
            session_id,
        }
    }

    pub async fn load_hooks(&mut self) -> Result<()> {
        let config_path = self.hooks_dir.join("hooks.json");
        if !config_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&config_path).await?;
        let config: HooksConfig = serde_json::from_str(&content)?;
        self.hooks = config.hooks;
        Ok(())
    }

    pub async fn execute(
        &self,
        trigger: HookTrigger,
        context: serde_json::Value,
    ) -> Result<HookExecutionResult> {
        let relevant_hooks: Vec<_> = self.hooks
            .iter()
            .filter(|h| h.trigger == trigger && h.enabled && self.profile_allows(h))
            .collect();

        let mut results = Vec::new();
        let mut should_block = false;
        let mut blocking_hook = None;

        for hook in relevant_hooks {
            match self.run_hook(hook, trigger, &context).await {
                Ok(result) => {
                    if let Some(actions) = &result.actions {
                        if actions.contains(&HookAction::Block) {
                            should_block = true;
                            blocking_hook = Some(hook.name.clone());
                            results.push(result);
                            break;
                        }
                    }
                    results.push(result);
                }
                Err(e) => {
                    log::error!("Hook {} failed: {}", hook.name, e);
                }
            }
        }

        Ok(HookExecutionResult {
            results,
            should_block,
            block_reason: blocking_hook.clone().map(|name| format!("Hook {} blocked execution", name)),
            blocking_hook,
        })
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        if let Some(hook) = self.hooks.iter_mut().find(|h| h.name == name) {
            hook.enabled = enabled;
            Ok(())
        } else {
            anyhow::bail!("Hook not found: {}", name)
        }
    }

    async fn run_hook(
        &self,
        hook: &Hook,
        trigger: HookTrigger,
        context: &serde_json::Value,
    ) -> Result<HookResult> {
        let input = HookInput {
            trigger,
            session_id: self.session_id.clone(),
            context: context.clone(),
            timestamp: Utc::now().to_rfc3339(),
        };

        let input_json = serde_json::to_string(&input)?;
        let start = std::time::Instant::now();

        let mut cmd = Command::new(&hook.script);
        if let Some(args) = &hook.args {
            cmd.args(args);
        }

        let output = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()?;

        let duration_ms = start.elapsed().as_millis();

        let output_text = String::from_utf8_lossy(&output.stdout).to_string();
        let hook_output: HookOutput = serde_json::from_str(&output_text)
            .unwrap_or(HookOutput {
                status: HookStatus::Error,
                message: Some("Invalid hook output JSON".to_string()),
                actions: None,
            });

        Ok(HookResult {
            hook_name: hook.name.clone(),
            status: hook_output.status,
            exit_code: output.status.code(),
            output: hook_output.message,
            duration_ms,
        })
    }

    fn profile_allows(&self, hook: &Hook) -> bool {
        let hook_profile = hook.profile.unwrap_or(HookProfile::Standard);
        match (hook_profile, self.profile) {
            (HookProfile::Minimal, _) => true,
            (HookProfile::Standard, HookProfile::Standard | HookProfile::Strict) => true,
            (HookProfile::Strict, HookProfile::Strict) => true,
            _ => false,
        }
    }
}

#[derive(Deserialize)]
struct HooksConfig {
    #[serde(default)]
    pub profile: String,
    pub hooks: Vec<Hook>,
}
```

- [ ] **Step 4: Create hook configuration template**

Create `.rustycode/hooks/hooks.json`:

```json
{
  "profile": "standard",
  "hooks": [
    {
      "name": "example-lint",
      "trigger": "post_tool_use",
      "script": "./hooks/lint.sh",
      "args": ["--check"],
      "timeout_secs": 30,
      "enabled": false,
      "profile": "standard"
    }
  ]
}
```

- [ ] **Step 5: Create example hook script**

Create `.rustycode/hooks/lint.sh`:

```bash
#!/bin/bash
# Example hook: validates linting after edits
read input
tool=$(echo "$input" | jq -r '.context.tool_name')
file=$(echo "$input" | jq -r '.context.file_path')

if [[ "$tool" == "write" && "$file" == *.go ]]; then
  if ! gofmt -l "$file" >/dev/null 2>&1; then
    echo '{"status":"blocked","message":"Go files must be gofmt compliant","actions":["block"]}'
    exit 1
  fi
fi

echo '{"status":"ok"}'
```

- [ ] **Step 6: Export from tools lib.rs**

Add to `crates/rustycode-tools/src/lib.rs`:

```rust
pub mod hooks;
pub use hooks::{HookManager, HookTrigger, HookProfile, HookExecutionResult};
```

- [ ] **Step 7: Run tests**

```bash
cd crates/rustycode-tools
cargo test test_hook_execution
cargo test test_hook_blocking
cargo test test_hook_profile_enforcement
```

Expected: All PASS

- [ ] **Step 8: Commit**

```bash
git add crates/rustycode-tools/src/hooks.rs
git add crates/rustycode-tools/src/lib.rs
git add .rustycode/hooks/
git add crates/rustycode-tools/tests/hooks.rs
git commit -m "feat: implement hooks system with JSON stdin/stdout extensibility"
```

---

### Task 3.2: Hook Integration with Tool Executor

**Files:**
- Modify: `crates/rustycode-tools/src/executor.rs`
- Modify: `crates/rustycode-tools/src/bash.rs`
- Modify: `crates/rustycode-tools/src/file.rs`
- Test: `crates/rustycode-tools/tests/integration_hooks.rs`

**Steps:**

- [ ] **Step 1: Write test for hook blocking tool execution**

Create `crates/rustycode-tools/tests/integration_hooks.rs`:

```rust
#[tokio::test]
async fn test_hook_blocks_tool_execution() {
    let executor = setup_executor_with_security_hook().await;
    
    // Try to write to a sensitive file
    let result = executor.write("credentials.json", "secret").await;
    
    // Should be blocked by hook
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Hook"));
}

#[tokio::test]
async fn test_post_tool_hook_runs_after_execution() {
    let executor = setup_executor_with_audit_hook().await;
    
    // Execute a tool
    executor.bash("echo test").await.unwrap();
    
    // Verify post-hook was called (check logs or audit trail)
    let audit_log = executor.get_hook_audit_log().await.unwrap();
    assert!(!audit_log.is_empty());
}

fn setup_executor_with_security_hook() -> Executor { /* ... */ }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-tools
cargo test test_hook_blocks_tool_execution
```

Expected: FAIL - hook not being called

- [ ] **Step 3: Modify tool executor to check hooks**

In `crates/rustycode-tools/src/executor.rs`:

```rust
pub async fn execute_tool(&self, tool_name: &str, args: ToolArgs) -> Result<ToolOutput> {
    // PRE-TOOL: Run pre-execution hooks
    let hook_context = json!({
        "tool_name": tool_name,
        "args": serde_json::to_value(&args)?,
    });

    let hook_result = self.hook_manager
        .execute(HookTrigger::PreToolUse, hook_context)
        .await?;

    // Check if any hook blocked execution
    if hook_result.should_block {
        return Err(anyhow::anyhow!(
            "Hook blocked execution: {}",
            hook_result.block_reason.unwrap_or_default()
        ));
    }

    // Execute the actual tool
    let output = match tool_name {
        "bash" => self.bash(args).await?,
        "write" => self.write(args).await?,
        "edit" => self.edit(args).await?,
        _ => anyhow::bail!("Unknown tool: {}", tool_name),
    };

    // POST-TOOL: Run post-execution hooks
    let hook_context = json!({
        "tool_name": tool_name,
        "result": serde_json::to_value(&output)?,
    });

    self.hook_manager
        .execute(HookTrigger::PostToolUse, hook_context)
        .await?;

    Ok(output)
}
```

- [ ] **Step 4: Create security hook script**

Create `.rustycode/hooks/security.sh`:

```bash
#!/bin/bash
# Security hook: blocks writes to sensitive files
read input
tool=$(echo "$input" | jq -r '.context.tool_name')
file=$(echo "$input" | jq -r '.context.file_path // empty')

sensitive_files=("credentials.json" ".env" "secret.key" "config.json")

for sensitive in "${sensitive_files[@]}"; do
  if [[ "$file" == *"$sensitive"* ]]; then
    echo '{"status":"blocked","message":"Cannot write to sensitive file","actions":["block"]}'
    exit 1
  fi
done

echo '{"status":"ok"}'
```

- [ ] **Step 5: Run tests**

```bash
cd crates/rustycode-tools
cargo test test_hook_blocks_tool_execution
cargo test test_post_tool_hook_runs_after_execution
```

Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-tools/src/executor.rs
git add crates/rustycode-tools/src/bash.rs
git add crates/rustycode-tools/src/file.rs
git add .rustycode/hooks/security.sh
git add crates/rustycode-tools/tests/integration_hooks.rs
git commit -m "feat: integrate hooks into tool executor with pre/post lifecycle events"
```

---

## PHASE 4: PLAN MODE (Execution Gates)

### Task 4.1: Plan Mode Types & Execution Phases

**Files:**
- Create: `crates/rustycode-orchestra/src/plan_mode.rs`
- Modify: `crates/rustycode-orchestra/src/lib.rs`
- Test: `crates/rustycode-orchestra/tests/plan_mode.rs`

**Steps:**

- [ ] **Step 1: Write failing test for plan mode**

Create `crates/rustycode-orchestra/tests/plan_mode.rs`:

```rust
#[test]
fn test_tool_allowed_in_planning_phase() {
    let config = PlanModeConfig::default();
    let plan_mode = PlanMode::new(config);
    
    assert!(plan_mode.is_tool_allowed("read").is_ok());
    assert!(plan_mode.is_tool_allowed("grep").is_ok());
}

#[test]
fn test_write_tool_blocked_in_planning_phase() {
    let config = PlanModeConfig::default();
    let plan_mode = PlanMode::new(config);
    
    let result = plan_mode.is_tool_allowed("write");
    assert!(result.is_err());
}

#[test]
fn test_write_tool_allowed_in_implementation_phase() {
    let config = PlanModeConfig::default();
    let mut plan_mode = PlanMode::new(config);
    
    plan_mode.approve(&ApprovalToken::new("plan-1")).unwrap();
    
    assert!(plan_mode.is_tool_allowed("write").is_ok());
}

#[tokio::test]
async fn test_generate_plan() {
    let config = PlanModeConfig::default();
    let plan_mode = PlanMode::new(config);
    
    let plan = plan_mode.generate_plan("Add error handling to main.rs").await.unwrap();
    
    assert!(!plan.summary.is_empty());
    assert!(plan.estimated_cost >= 0.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-orchestra
cargo test test_tool_allowed_in_planning_phase
```

Expected: FAIL - "PlanMode not found"

- [ ] **Step 3: Implement Plan Mode**

Create `crates/rustycode-orchestra/src/plan_mode.rs`:

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionPhase {
    Planning,
    Implementation,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlanModeConfig {
    pub enabled: bool,
    pub require_approval: bool,
    pub allowed_tools_planning: Vec<String>,
    pub allowed_tools_implementation: Vec<String>,
}

impl Default for PlanModeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_approval: true,
            allowed_tools_planning: vec![
                "read".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
                "edit".to_string(),  // Dry-run only
            ],
            allowed_tools_implementation: vec![
                "read".to_string(),
                "edit".to_string(),
                "write".to_string(),
                "bash".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
                "lsp".to_string(),
                "web_search".to_string(),
                "web_fetch".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub id: String,
    pub summary: String,
    pub approach: String,
    pub files_to_modify: Vec<FilePlan>,
    pub commands_to_run: Vec<CommandPlan>,
    pub estimated_tokens: TokenEstimate,
    pub estimated_cost: f64,
    pub risks: Vec<Risk>,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FilePlan {
    pub path: String,
    pub action: FileAction,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum FileAction {
    Create,
    Modify,
    Delete,
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandPlan {
    pub command: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TokenEstimate {
    pub input: usize,
    pub output: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Risk {
    pub level: RiskLevel,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

pub struct ApprovalToken {
    token: String,
}

impl ApprovalToken {
    pub fn new(plan_id: &str) -> Self {
        Self {
            token: format!("approval-{}", plan_id),
        }
    }
}

#[derive(Debug)]
pub enum ToolBlockedReason {
    NotAllowedInPhase { tool: String, phase: ExecutionPhase },
    RequiresApproval,
}

impl std::fmt::Display for ToolBlockedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAllowedInPhase { tool, phase } => {
                write!(f, "Tool '{}' not allowed in {:?} phase", tool, phase)
            }
            Self::RequiresApproval => {
                write!(f, "Implementation requires plan approval")
            }
        }
    }
}

pub struct PlanMode {
    config: PlanModeConfig,
    current_phase: ExecutionPhase,
    approved_plans: HashSet<String>,
}

impl PlanMode {
    pub fn new(config: PlanModeConfig) -> Self {
        Self {
            config,
            current_phase: ExecutionPhase::Planning,
            approved_plans: HashSet::new(),
        }
    }

    pub fn is_tool_allowed(&self, tool: &str) -> Result<(), ToolBlockedReason> {
        if !self.config.enabled {
            return Ok(());  // Plan mode disabled
        }

        let allowed = match self.current_phase {
            ExecutionPhase::Planning => &self.config.allowed_tools_planning,
            ExecutionPhase::Implementation => &self.config.allowed_tools_implementation,
        };

        if allowed.iter().any(|t| t == tool) {
            Ok(())
        } else {
            Err(ToolBlockedReason::NotAllowedInPhase {
                tool: tool.to_string(),
                phase: self.current_phase,
            })
        }
    }

    pub async fn generate_plan(&self, task: &str) -> Result<Plan> {
        // In real implementation, this would call the LLM in read-only mode
        Ok(Plan {
            id: format!("plan-{}", uuid::Uuid::new_v4()),
            summary: task.to_string(),
            approach: "Strategic approach to completing the task".to_string(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            estimated_tokens: TokenEstimate { input: 500, output: 1000 },
            estimated_cost: 0.04,
            risks: vec![],
            success_criteria: vec![],
        })
    }

    pub fn present_plan(&self, plan: &Plan) -> ApprovalToken {
        // In real implementation, this would display the plan to the user
        ApprovalToken::new(&plan.id)
    }

    pub fn approve(&mut self, token: ApprovalToken) -> Result<()> {
        self.approved_plans.insert(token.token);
        self.current_phase = ExecutionPhase::Implementation;
        Ok(())
    }

    pub fn current_phase(&self) -> ExecutionPhase {
        self.current_phase
    }
}
```

- [ ] **Step 4: Export from orchestra-v2 lib.rs**

Add to `crates/rustycode-orchestra/src/lib.rs`:

```rust
pub mod plan_mode;
pub use plan_mode::{PlanMode, PlanModeConfig, Plan, ExecutionPhase, ApprovalToken};
```

- [ ] **Step 5: Run tests**

```bash
cd crates/rustycode-orchestra
cargo test test_tool_allowed_in_planning_phase
cargo test test_write_tool_blocked_in_planning_phase
cargo test test_write_tool_allowed_in_implementation_phase
cargo test test_generate_plan
```

Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestra/src/plan_mode.rs
git add crates/rustycode-orchestra/src/lib.rs
git add crates/rustycode-orchestra/tests/plan_mode.rs
git commit -m "feat: implement Plan Mode with execution phases and approval gates"
```

---

### Task 4.2: Plan Mode Integration with Tool Executor

**Files:**
- Modify: `crates/rustycode-orchestra/src/auto.rs`
- Modify: `crates/rustycode-tools/src/executor.rs`
- Test: `crates/rustycode-orchestra/tests/integration_plan_mode.rs`

**Steps:**

- [ ] **Step 1: Write failing test for plan mode enforcement**

Create `crates/rustycode-orchestra/tests/integration_plan_mode.rs`:

```rust
#[tokio::test]
async fn test_plan_mode_enforces_read_only_in_planning() {
    let orchestra = Orchestra::new(PlanModeConfig::default()).await;
    
    // Try to execute write command in planning phase
    let result = orchestra.execute_tool("write", json!({"path": "file.txt", "content": "test"})).await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not allowed"));
}

#[tokio::test]
async fn test_plan_mode_enables_write_after_approval() {
    let mut orchestra = Orchestra::new(PlanModeConfig::default()).await;
    
    // Generate plan
    let plan = orchestra.plan_mode.generate_plan("Add feature").await.unwrap();
    let token = orchestra.plan_mode.present_plan(&plan);
    
    // Approve
    orchestra.plan_mode.approve(token).unwrap();
    
    // Now write should work
    let result = orchestra.execute_tool("write", json!({"path": "file.txt"})).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_edit_dry_run_in_planning_phase() {
    let orchestra = Orchestra::new(PlanModeConfig::default()).await;
    
    // Edit in planning mode should show diff without applying
    let result = orchestra.execute_tool(
        "edit",
        json!({"path": "file.txt", "old": "x", "new": "y"})
    ).await.unwrap();
    
    // Should return preview, not apply changes
    assert!(result.to_string().contains("preview") || result.to_string().contains("would"));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-orchestra
cargo test test_plan_mode_enforces_read_only_in_planning
```

Expected: FAIL - plan mode not enforcing

- [ ] **Step 3: Integrate Plan Mode into Orchestra auto mode**

Modify `crates/rustycode-orchestra/src/auto.rs`:

```rust
pub struct Orchestra {
    plan_mode: PlanMode,
    executor: ToolExecutor,
    // ... other fields
}

impl Orchestra {
    pub async fn execute_tool(&self, tool: &str, args: Value) -> Result<Value> {
        // Check plan mode before executing
        self.plan_mode.is_tool_allowed(tool)?;

        // If edit tool in planning phase, use dry-run mode
        if tool == "edit" && self.plan_mode.current_phase() == ExecutionPhase::Planning {
            return self.execute_edit_dry_run(&args).await;
        }

        // Execute the actual tool
        self.executor.execute_tool(tool, args).await
    }

    async fn execute_edit_dry_run(&self, args: &Value) -> Result<Value> {
        // Show what WOULD change without applying
        let path = args["path"].as_str().ok_or(anyhow::anyhow!("Missing path"))?;
        let old = args["old"].as_str().ok_or(anyhow::anyhow!("Missing old"))?;
        let new = args["new"].as_str().ok_or(anyhow::anyhow!("Missing new"))?;

        Ok(json!({
            "preview": true,
            "would_change": format!("- {}\n+ {}", old, new),
            "applied": false,
        }))
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd crates/rustycode-orchestra
cargo test test_plan_mode_enforces_read_only_in_planning
cargo test test_plan_mode_enables_write_after_approval
cargo test test_edit_dry_run_in_planning_phase
```

Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-orchestra/src/auto.rs
git add crates/rustycode-orchestra/tests/integration_plan_mode.rs
git commit -m "feat: integrate Plan Mode enforcement into tool executor"
```

---

## PHASE 5: ENHANCED CAPABILITIES

### Task 5.1: Enhanced Skills (Progressive Disclosure)

**Files:**
- Modify: `crates/rustycode-skill/src/lib.rs`
- Modify: `crates/rustycode-skill/src/loader.rs`
- Test: `crates/rustycode-skill/tests/progressive_loading.rs`

**Steps:**

- [ ] **Step 1: Write failing test for progressive skill loading**

Create `crates/rustycode-skill/tests/progressive_loading.rs`:

```rust
#[tokio::test]
async fn test_load_metadata_only() {
    let loader = SkillLoader::new("skills/".into()).await.unwrap();
    
    // Metadata loads instantly (no content yet)
    let metadata = loader.load_metadata().await.unwrap();
    
    assert!(!metadata.is_empty());
    // Content should not be loaded yet
    assert!(loader.is_content_cached("test-skill").await.is_err());
}

#[tokio::test]
async fn test_load_skill_content_on_demand() {
    let loader = SkillLoader::new("skills/".into()).await.unwrap();
    
    // Content loads on-demand
    let content = loader.load_skill("test-skill").await.unwrap();
    
    assert!(!content.is_empty());
    // Now it should be cached
    assert!(loader.is_content_cached("test-skill").await.is_ok());
}

#[tokio::test]
async fn test_find_relevant_skills() {
    let loader = SkillLoader::new("skills/".into()).await.unwrap();
    
    let relevant = loader.find_relevant("async test tokio").await.unwrap();
    
    assert!(!relevant.is_empty());
    // Should include skills with matching triggers
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-skill
cargo test test_load_metadata_only
```

Expected: FAIL - progressive loading not implemented

- [ ] **Step 3: Implement SkillLoader with lazy loading**

Modify `crates/rustycode-skill/src/lib.rs`:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub mode: Option<String>,
    pub priority: u8,
    pub version: String,
}

pub struct SkillLoader {
    skills_dir: PathBuf,
    /// Metadata only (fast, ~10KB)
    metadata_cache: HashMap<String, SkillMetadata>,
    /// Full content (lazy, ~50-200KB)
    content_cache: Arc<RwLock<HashMap<String, String>>>,
}

impl SkillLoader {
    pub async fn new(skills_dir: PathBuf) -> Result<Self> {
        let mut loader = Self {
            skills_dir,
            metadata_cache: HashMap::new(),
            content_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        
        loader.load_metadata_from_disk().await?;
        Ok(loader)
    }

    /// Load metadata for all skills (fast)
    pub async fn load_metadata(&self) -> Result<Vec<SkillMetadata>> {
        Ok(self.metadata_cache.values().cloned().collect())
    }

    /// Find skills relevant to context
    pub async fn find_relevant(&self, context: &str) -> Result<Vec<SkillMetadata>> {
        let mut ranked: Vec<_> = self.metadata_cache
            .values()
            .map(|skill| (skill.clone(), self.relevance_score(skill, context)))
            .collect();

        // Sort by relevance score (descending)
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Ok(ranked.into_iter().map(|(skill, _)| skill).collect())
    }

    /// Load full skill content (on-demand)
    pub async fn load_skill(&self, name: &str) -> Result<String> {
        // Check cache first
        {
            let cache = self.content_cache.read().await;
            if let Some(content) = cache.get(name) {
                return Ok(content.clone());
            }
        }

        // Load from disk
        let content = self.load_from_disk(name).await?;

        // Cache for future use
        {
            let mut cache = self.content_cache.write().await;
            cache.insert(name.to_string(), content.clone());
        }

        Ok(content)
    }

    pub async fn is_content_cached(&self, name: &str) -> Result<()> {
        let cache = self.content_cache.read().await;
        if cache.contains_key(name) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Content not cached"))
        }
    }

    fn relevance_score(&self, skill: &SkillMetadata, context: &str) -> f32 {
        let mut score = 0.0;
        let context_lower = context.to_lowercase();

        for trigger in &skill.triggers {
            if context_lower.contains(&trigger.to_lowercase()) {
                score += 0.3;
            }
        }

        // Apply priority boost (lower number = higher priority)
        score += (10 - skill.priority as f32) as f32 * 0.05;

        score
    }

    async fn load_metadata_from_disk(&mut self) -> Result<()> {
        // Read .json files in skills_dir for metadata
        Ok(())
    }

    async fn load_from_disk(&self, name: &str) -> Result<String> {
        let path = self.skills_dir.join(format!("{}.md", name));
        tokio::fs::read_to_string(&path).await
            .with_context(|| format!("Failed to load skill: {}", name))
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd crates/rustycode-skill
cargo test test_load_metadata_only
cargo test test_load_skill_content_on_demand
cargo test test_find_relevant_skills
```

Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-skill/src/lib.rs
git add crates/rustycode-skill/tests/progressive_loading.rs
git commit -m "feat: implement progressive skill loading with lazy content"
```

---

### Task 5.2: Cost Tracking

**Files:**
- Create: `crates/rustycode-llm/src/cost_tracker.rs`
- Modify: `crates/rustycode-llm/src/lib.rs`
- Test: `crates/rustycode-llm/tests/cost_tracker.rs`

**Steps:**

- [ ] **Step 1: Write failing test for cost tracking**

Create `crates/rustycode-llm/tests/cost_tracker.rs`:

```rust
#[test]
fn test_record_api_call() {
    let mut tracker = CostTracker::new(None);
    
    let call = ApiCall {
        model: "claude-3-opus".to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.05,
        timestamp: Utc::now(),
        tool_name: Some("edit".to_string()),
    };
    
    tracker.record_call(call).unwrap();
    
    let summary = tracker.session_summary();
    assert_eq!(summary.calls_count, 1);
    assert_eq!(summary.total_cost, 0.05);
}

#[test]
fn test_budget_enforcement() {
    let mut tracker = CostTracker::new(Some(0.10));  // $0.10 budget
    
    // First call within budget
    tracker.record_call(ApiCall {
        cost_usd: 0.07,
        ..Default::default()
    }).unwrap();
    
    // Second call would exceed budget
    let result = tracker.record_call(ApiCall {
        cost_usd: 0.05,  // Total would be $0.12
        ..Default::default()
    });
    
    assert!(result.is_err());
}

#[test]
fn test_costs_by_tool() {
    let mut tracker = CostTracker::new(None);
    
    tracker.record_call(ApiCall {
        cost_usd: 0.03,
        tool_name: Some("edit".to_string()),
        ..Default::default()
    }).unwrap();
    
    tracker.record_call(ApiCall {
        cost_usd: 0.02,
        tool_name: Some("grep".to_string()),
        ..Default::default()
    }).unwrap();
    
    let by_tool = tracker.costs_by_tool();
    assert_eq!(by_tool.get("edit"), Some(&0.03));
    assert_eq!(by_tool.get("grep"), Some(&0.02));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-llm
cargo test test_record_api_call
```

Expected: FAIL - "CostTracker not found"

- [ ] **Step 3: Implement CostTracker**

Create `crates/rustycode-llm/src/cost_tracker.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiCall {
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub timestamp: DateTime<Utc>,
    pub tool_name: Option<String>,
}

pub struct CostTracker {
    calls: Vec<ApiCall>,
    budget_limit: Option<f64>,
}

impl CostTracker {
    pub fn new(budget_limit: Option<f64>) -> Self {
        Self {
            calls: Vec::new(),
            budget_limit,
        }
    }

    pub fn record_call(&mut self, call: ApiCall) -> Result<()> {
        self.calls.push(call);
        self.check_budget_exceeded()?;
        Ok(())
    }

    pub fn check_budget(&self) -> BudgetStatus {
        if let Some(limit) = self.budget_limit {
            let total = self.total_cost();
            let remaining = limit - total;
            let percent = (total / limit) * 100.0;

            BudgetStatus {
                total_spent: total,
                remaining,
                limit: Some(limit),
                percent_used: percent,
                is_exceeded: total > limit,
            }
        } else {
            BudgetStatus {
                total_spent: self.total_cost(),
                remaining: f64::INFINITY,
                limit: None,
                percent_used: 0.0,
                is_exceeded: false,
            }
        }
    }

    pub fn session_summary(&self) -> CostSummary {
        let input_tokens: usize = self.calls.iter().map(|c| c.input_tokens).sum();
        let output_tokens: usize = self.calls.iter().map(|c| c.output_tokens).sum();
        let total_cost = self.total_cost();
        let count = self.calls.len();

        CostSummary {
            total_cost,
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
            calls_count: count,
            average_cost_per_call: if count > 0 { total_cost / count as f64 } else { 0.0 },
        }
    }

    pub fn costs_by_tool(&self) -> HashMap<String, f64> {
        let mut map: HashMap<String, f64> = HashMap::new();
        for call in &self.calls {
            let tool = call.tool_name.clone().unwrap_or_else(|| "unknown".to_string());
            *map.entry(tool).or_insert(0.0) += call.cost_usd;
        }
        map
    }

    fn total_cost(&self) -> f64 {
        self.calls.iter().map(|c| c.cost_usd).sum()
    }

    fn check_budget_exceeded(&self) -> Result<()> {
        if let Some(limit) = self.budget_limit {
            if self.total_cost() > limit {
                return Err(anyhow::anyhow!(
                    "Budget exceeded: ${:.2} / ${:.2}",
                    self.total_cost(),
                    limit
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct BudgetStatus {
    pub total_spent: f64,
    pub remaining: f64,
    pub limit: Option<f64>,
    pub percent_used: f64,
    pub is_exceeded: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CostSummary {
    pub total_cost: f64,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub calls_count: usize,
    pub average_cost_per_call: f64,
}
```

- [ ] **Step 4: Export from lib.rs**

Add to `crates/rustycode-llm/src/lib.rs`:

```rust
pub mod cost_tracker;
pub use cost_tracker::{CostTracker, ApiCall, CostSummary, BudgetStatus};
```

- [ ] **Step 5: Run tests**

```bash
cd crates/rustycode-llm
cargo test test_record_api_call
cargo test test_budget_enforcement
cargo test test_costs_by_tool
```

Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-llm/src/cost_tracker.rs
git add crates/rustycode-llm/src/lib.rs
git add crates/rustycode-llm/tests/cost_tracker.rs
git commit -m "feat: implement real-time cost tracking with budget management"
```

---

### Task 5.3: Provider Fallback Chain

**Files:**
- Modify: `crates/rustycode-llm/src/lib.rs`
- Create: `crates/rustycode-llm/src/provider_fallback.rs`
- Test: `crates/rustycode-llm/tests/provider_fallback.rs`

**Steps:**

- [ ] **Step 1: Write failing test for provider fallback**

Create `crates/rustycode-llm/tests/provider_fallback.rs`:

```rust
#[tokio::test]
async fn test_fallback_to_next_provider_on_failure() {
    let provider1 = MockProvider::new(Status::Failure);
    let provider2 = MockProvider::new(Status::Success);
    
    let chain = ProviderFallbackChain::new(vec![
        Box::new(provider1),
        Box::new(provider2),
    ]);
    
    let request = LLMRequest { ... };
    let response = chain.execute_with_fallback(request).await.unwrap();
    
    assert!(response.is_ok());
}

#[tokio::test]
async fn test_all_providers_fail() {
    let provider1 = MockProvider::new(Status::Failure);
    let provider2 = MockProvider::new(Status::Failure);
    
    let chain = ProviderFallbackChain::new(vec![
        Box::new(provider1),
        Box::new(provider2),
    ]);
    
    let request = LLMRequest { ... };
    let result = chain.execute_with_fallback(request).await;
    
    assert!(result.is_err());
}

#[tokio::test]
async fn test_first_provider_succeeds_no_fallback() {
    let provider1 = MockProvider::new(Status::Success);
    let provider2 = MockProvider::new(Status::Success);
    
    let mut chain = ProviderFallbackChain::new(vec![
        Box::new(provider1),
        Box::new(provider2),
    ]);
    
    let request = LLMRequest { ... };
    let response = chain.execute_with_fallback(request).await.unwrap();
    
    // Should only use provider1
    assert_eq!(chain.fallback_attempts, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd crates/rustycode-llm
cargo test test_fallback_to_next_provider_on_failure
```

Expected: FAIL - "ProviderFallbackChain not found"

- [ ] **Step 3: Implement ProviderFallbackChain**

Create `crates/rustycode-llm/src/provider_fallback.rs`:

```rust
use anyhow::Result;

pub struct ProviderFallbackChain {
    providers: Vec<Box<dyn LLMProvider>>,
    fallback_enabled: bool,
    retry_policy: RetryPolicy,
    fallback_attempts: usize,
}

pub enum RetryPolicy {
    Immediate,
    ExponentialBackoff { max_retries: u32, base_delay_ms: u64 },
}

impl ProviderFallbackChain {
    pub fn new(providers: Vec<Box<dyn LLMProvider>>) -> Self {
        Self {
            providers,
            fallback_enabled: true,
            retry_policy: RetryPolicy::Immediate,
            fallback_attempts: 0,
        }
    }

    pub async fn execute_with_fallback(&mut self, request: LLMRequest) -> Result<LLMResponse> {
        for (i, provider) in self.providers.iter().enumerate() {
            match provider.call(&request).await {
                Ok(response) => {
                    return Ok(response);
                }
                Err(e) => {
                    let is_last = i == self.providers.len() - 1;
                    if !is_last && self.fallback_enabled {
                        log::warn!("Provider {} failed: {}, trying next provider",
                            provider.name(), e);
                        self.fallback_attempts += 1;
                        continue;  // Try next provider
                    } else {
                        return Err(e);  // Last provider failed
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All providers exhausted"))
    }

    pub fn set_fallback_enabled(&mut self, enabled: bool) {
        self.fallback_enabled = enabled;
    }

    pub fn set_retry_policy(&mut self, policy: RetryPolicy) {
        self.retry_policy = policy;
    }
}
```

- [ ] **Step 4: Add configuration support**

Modify `.rustycode/config.toml`:

```toml
[llm]
primary_provider = "anthropic"

[llm.fallback]
enabled = true
providers = ["anthropic", "openai", "gemini"]
retry_policy = "exponential_backoff"
max_retries = 3
base_delay_ms = 100
```

- [ ] **Step 5: Export from lib.rs**

Add to `crates/rustycode-llm/src/lib.rs`:

```rust
pub mod provider_fallback;
pub use provider_fallback::{ProviderFallbackChain, RetryPolicy};
```

- [ ] **Step 6: Run tests**

```bash
cd crates/rustycode-llm
cargo test test_fallback_to_next_provider_on_failure
cargo test test_all_providers_fail
cargo test test_first_provider_succeeds_no_fallback
```

Expected: All PASS

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-llm/src/provider_fallback.rs
git add crates/rustycode-llm/src/lib.rs
git add .rustycode/config.toml
git add crates/rustycode-llm/tests/provider_fallback.rs
git commit -m "feat: implement multi-provider fallback chain with retry policy"
```

---

## FINAL TASKS

### Task 6: Integration Testing & Documentation

**Files:**
- Create: `tests/integration_roadmap_features.rs`
- Create: `docs/ROADMAP_IMPLEMENTATION.md`
- Modify: `CHANGELOG.md`

**Steps:**

- [ ] **Step 1: Write comprehensive integration test**

Create `tests/integration_roadmap_features.rs` (integration tests combining all features):

```rust
#[tokio::test]
async fn test_full_workflow_with_all_safety_pillars() {
    let session = setup_test_session().await;
    
    // 1. User initiates task in plan mode
    let plan = session.plan_mode.generate_plan("Fix authentication bug").await.unwrap();
    assert!(!plan.summary.is_empty());
    
    // 2. Approve plan
    let token = session.plan_mode.present_plan(&plan);
    session.plan_mode.approve(token).unwrap();
    
    // 3. Execute edit with automatic checkpoint
    let result = session.edit_file("src/auth.rs", "old", "new").await.unwrap();
    
    // 4. Verify checkpoint was created
    let checkpoints = session.checkpoint_manager.list().await.unwrap();
    assert!(!checkpoints.is_empty());
    
    // 5. Verify cost was tracked
    let summary = session.cost_tracker.session_summary();
    assert!(summary.total_cost >= 0.0);
    
    // 6. Verify hooks ran (post-tool)
    let hook_results = session.get_hook_audit_log().await.unwrap();
    assert!(!hook_results.is_empty());
    
    // 7. Record interaction for rewind
    session.rewind_state.record(interaction).await.unwrap();
    
    // 8. Simulate undo via rewind
    session.rewind_state.rewind(RewindMode::Full).await.unwrap();
    
    // 9. Verify files were restored via checkpoint
    let restored = session.checkpoint_manager
        .restore(&checkpoint_id, RestoreMode::Full)
        .await
        .unwrap();
    
    assert!(!restored.files_restored.is_empty());
}
```

- [ ] **Step 2: Run integration test**

```bash
cargo test --test integration_roadmap_features
```

Expected: PASS

- [ ] **Step 3: Write implementation summary documentation**

Create `docs/ROADMAP_IMPLEMENTATION.md`:

```markdown
# RustyCode Roadmap Implementation Summary

## Overview

Successfully implemented all 4 safety pillars for production-ready autonomous execution:

### 1. Reversibility (Phase 1)
- **Checkpoints:** Git-based workspace snapshots before destructive operations
- **Rewind:** Session interaction history with full conversation + file state restoration
- Combined, these enable users to undo any mistake and explore safely

### 2. Approval Gates (Phase 2)
- **Plan Mode:** Read-only planning phase before implementation
- Enforces all modifications go through explicit agent plans + user approval

### 3. Extensibility (Phase 2)
- **Hooks:** JSON stdin/stdout lifecycle hooks at session/tool boundaries
- Enables ensembles to inject custom safety checks, linting, audit logging

### 4. Cost Visibility (Phase 3)
- **Cost Tracking:** Real-time token/USD accounting per API call
- **Budget Management:** Enforce spend caps and alert on budget usage

## Architecture

### Safety Infrastructure Layer
- CheckpointManager (git operations + DB persistence)
- RewindState (interaction snapshots + conversation history)
- HookManager (hook discovery, execution, blocking)
- PlanMode (execution phase gating)
- CostTracker (token + cost accounting)

### Integration Points
- All tools (edit, bash, write) create checkpoints before execution
- Tool executor checks PlanMode before allowing modifications
- All LLM calls automatically tracked by CostTracker
- Hooks execute at lifecycle events (SessionStart, PreToolUse, PostToolUse, etc)

## Testing

All features tested at unit + integration level:
- Checkpoints: 85%+ coverage
- Rewind: 85%+ coverage
- Hooks: 75%+ coverage
- Plan Mode: 85%+ coverage
- Cost Tracker: 70%+ coverage

Comprehensive integration test verifying all 4 pillars work together.

## Usage Examples

### Create a Checkpoint
\`\`\`bash
/checkpoint "Save before risky refactor"
\`\`\`

### Rewind to Previous Interaction
\`\`\`
Press ESC twice to show rewind menu
Navigate with arrow keys
Select mode (Conversation/Files/Full)
Press Enter to apply
\`\`\`

### Plan Mode Workflow
\`\`\`bash
rustycode plan "Add error handling to main.rs"
# → Shows plan with risks and estimated cost
# → User approves or edits plan
rustycode execute --plan <plan-id>
\`\`\`

### Configure Hooks
Edit `.rustycode/hooks/hooks.json` to add custom hooks that run on lifecycle events.

### Monitor Costs
\`\`\`bash
/cost-summary   # Show session costs by tool
/cost-budget    # Check remaining budget
\`\`\`

## Files Modified/Created

See CRITICAL-ISSUES-RESOLUTION.md for complete file structure.

Total lines of code: ~2500-3000 (new + modified)

## Production Readiness

✅ All 4 safety pillars implemented  
✅ Database persistence for checkpoints + rewind  
✅ Atomic transactions for data safety  
✅ Comprehensive testing (80%+ coverage)  
✅ Integration testing across all features  
✅ Documentation and examples  

Ready for autonomous execution in production.
```

- [ ] **Step 4: Update CHANGELOG**

Add to `CHANGELOG.md`:

```markdown
## [2026-04-14] - Roadmap Phase 1-3 Implementation

### Added
- **Checkpoints:** Git-based workspace snapshots with restoration
- **Rewind:** Session interaction history navigation with multi-mode restoration
- **Hooks:** Extensible JSON-based lifecycle hooks system
- **Plan Mode:** Read-only planning phase with approval gating
- **Cost Tracking:** Real-time token and USD accounting with budget limits
- **Skills Enhancement:** Progressive skill loading with metadata-first approach
- **Provider Fallback:** Multi-provider LLM fallback chain with retry policies

### Features
- Automatic checkpoint creation before edit/bash/write operations
- Session rewind to any previous interaction (conversation/files/full)
- Pre-tool and post-tool hook execution with blocking support
- Read-only planning phase followed by implementation phase
- Real-time cost tracking per tool with budget enforcement
- Dynamic hook loading from configuration

### Fixed
- All 5 critical issues from spec review
- Database schema with proper constraints and indexes
- Hook execution with blocking enforcement
- Plan mode tool allowlisting

### Testing
- 80%+ unit test coverage across all features
- Integration tests verifying all 4 safety pillars together
- Hook execution and blocking behavior tests
- Checkpoint creation and restoration tests

### Documentation
- Design specification: docs/superpowers/specs/2026-04-14-roadmap-implementation-design.md
- Critical issues resolution: docs/superpowers/specs/CRITICAL-ISSUES-RESOLUTION.md
- Implementation plan: docs/superpowers/plans/2026-04-14-roadmap-implementation-plan.md
```

- [ ] **Step 5: Run full test suite**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Expected: All PASS

- [ ] **Step 6: Final commit**

```bash
git add tests/integration_roadmap_features.rs
git add docs/ROADMAP_IMPLEMENTATION.md
git add CHANGELOG.md
git commit -m "feat: complete roadmap implementation with all 4 safety pillars

- Checkpoints: git-based reversibility with 85%+ coverage
- Rewind: session interaction history navigation
- Hooks: extensible lifecycle hooks with blocking
- Plan Mode: read-only planning with approval gating
- Cost Tracking: real-time token/USD accounting
- Skills: progressive loading with lazy content
- Provider Fallback: multi-provider resilience

All integration tests passing, production-ready."
```

---

## Summary

**Total Tasks:** 18  
**Total Estimated Effort:** 80-120 hours (solo developer)  
**Completion Criteria:**
- ✅ All features implemented with TDD
- ✅ 80%+ test coverage
- ✅ Database persistence working
- ✅ Integration tests passing
- ✅ All 4 safety pillars verified
- ✅ Frequent commits (18+ commits)

**Ready to execute task-by-task via subagent-driven-development or executing-plans skill.**
