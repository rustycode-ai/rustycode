# RustyCode Roadmap Implementation Design

**Date:** 2026-04-14  
**Author:** Claude Code  
**Status:** Design Review  
**Scope:** Phases 1-4 of feature roadmap (Checkpoints, Rewind, Hooks, Plan Mode, Skills, Cost Tracking, Provider Fallback, Subagents, Compaction)

---

## Executive Summary

This document describes the detailed implementation strategy for the RustyCode feature roadmap, following **Approach A: Foundation First**. The implementation prioritizes building user-facing safety features (Checkpoints, Rewind) before execution infrastructure (Hooks, Plan Mode), enabling safe autonomous execution through:

1. **Reversibility** — Git-based checkpoints + interaction rewind
2. **Approval Gates** — Plan Mode enforces planning before implementation
3. **Extensibility** — Hooks system for custom safety checks
4. **Cost Visibility** — Real-time token and USD tracking

**Key Decisions:**
- Solo developer, parallel-track design with dependency-driven sequencing
- All four safety pillars required before production deployment
- Phases 1-2 are critical path; Phases 3-4 are lower priority but implementable in parallel
- New components integrate with existing architecture (no major refactoring)

---

## 1. ARCHITECTURE & INTEGRATION

### 1.1 Layered Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  CLI/TUI Layer (rustycode-cli, rustycode-tui)              │
│  - Commands: /checkpoint, /plan, /hook-list, /rewind       │
├─────────────────────────────────────────────────────────────┤
│  Control Plane (Autonomous Mode + Core)                              │
│  - PlanMode execution gates                                 │
│  - Checkpoint triggers                                      │
│  - Rewind orchestration                                     │
├─────────────────────────────────────────────────────────────┤
│  Safety Infrastructure Layer (NEW)                          │
│  ├─ CheckpointManager (git-based snapshots)                │
│  ├─ RewindState (interaction history)                      │
│  ├─ HookManager (lifecycle scripts)                        │
│  ├─ SkillProgressiveLoader (metadata + on-demand)          │
│  └─ CostTracker (token + $$ accounting)                    │
├─────────────────────────────────────────────────────────────┤
│  Storage Layer (rustycode-storage)                          │
│  ├─ Checkpoint metadata table                              │
│  ├─ Rewind history snapshots                               │
│  ├─ Hook execution logs                                    │
│  └─ Cost tracking records                                  │
├─────────────────────────────────────────────────────────────┤
│  Tool Execution (rustycode-tools)                           │
│  - Checkpoint triggers before/after                        │
│  - Hook execution at tool lifecycle                        │
│  - Rewind state capture                                    │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 Integration Points

| Component | Location | Integration | Purpose |
|-----------|----------|-------------|---------|
| CheckpointManager | `rustycode-tools/src/checkpoint.rs` | Tool executor | Snapshot before destructive ops |
| RewindState | `rustycode-session/src/rewind.rs` | Session lifecycle | Record interactions for navigation |
| HookManager | `rustycode-tools/src/hooks.rs` | Tool executor | Trigger at lifecycle events |
| PlanMode | `rustycode-orchestra/src/plan_mode.rs` | Autonomous Mode auto mode | Gate execution flow |
| SkillLoader Enhancement | `rustycode-skill/src/lib.rs` | Skill system | Progressive disclosure |
| CostTracker | `rustycode-llm/src/cost_tracker.rs` | LLM providers | Capture tokens + cost |
| ProviderFallback | `rustycode-llm/src/lib.rs` | LLM providers | Automatic fallback chain |

### 1.3 New Database Tables

All tables added to `rustycode-storage`:

```sql
-- Checkpoint metadata (git-based recovery)
CREATE TABLE checkpoints (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    git_hash TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    files_changed JSONB,
    metadata JSONB,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

-- Rewind snapshot history
CREATE TABLE rewind_snapshots (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    interaction_number INTEGER NOT NULL,
    user_message TEXT,
    assistant_response TEXT,
    conversation_hash TEXT,
    files_hash TEXT,
    metadata JSONB,
    created_at TIMESTAMP NOT NULL,
    UNIQUE(session_id, interaction_number),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

-- Hook execution logs (audit trail)
CREATE TABLE hook_executions (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    hook_name TEXT NOT NULL,
    trigger TEXT NOT NULL,
    exit_code INTEGER,
    output JSONB,
    executed_at TIMESTAMP NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

-- Cost tracking (token + USD accounting)
CREATE TABLE api_calls (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cost_usd DECIMAL(10,4),
    tool_name TEXT,
    timestamp TIMESTAMP NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);
```

---

## 2. PHASE 1: CHECKPOINTS & REWIND

### 2.1 Checkpoints (Git-Based Workspace Snapshots)

**Purpose:** Automatically snapshot workspace state as git commits before potentially destructive operations. Users can inspect diffs and restore any checkpoint.

**Files to Create/Modify:**
- New: `crates/rustycode-tools/src/checkpoint.rs` (250-300 lines)
- New: `crates/rustycode-storage/src/checkpoint_store.rs` (100-150 lines)
- Modify: `crates/rustycode-git/src/lib.rs` (add checkpoint git ops)
- Modify: `crates/rustycode-tools/src/lib.rs` (export CheckpointManager)

**Core Types:**

```rust
// crates/rustycode-tools/src/checkpoint.rs

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Unique checkpoint identifier
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CheckpointId(String);

impl CheckpointId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

/// Immutable checkpoint metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub reason: String,                    // "before edit_file", etc
    pub created_at: DateTime<Utc>,
    pub git_hash: String,                  // Commit SHA
    pub files_changed: Vec<PathBuf>,       // Files in checkpoint
    pub description: Option<String>,
}

/// Checkpoint management
pub struct CheckpointManager {
    repo_path: PathBuf,
    storage: Arc<dyn CheckpointStore>,     // Trait object for testability
    max_checkpoints: usize,                // LRU eviction
}

#[derive(Debug)]
pub enum CheckpointMode {
    FullWorkspace,
    FilesOnly(Vec<PathBuf>),
}

#[derive(Debug)]
pub enum RestoreMode {
    FilesOnly,                             // Don't reset git HEAD
    Full,                                  // git reset --hard
}

pub struct RestoredCheckpoint {
    pub checkpoint: Checkpoint,
    pub files_restored: Vec<PathBuf>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(repo_path: PathBuf, storage: Arc<dyn CheckpointStore>, max: usize) -> Self {
        Self {
            repo_path,
            storage,
            max_checkpoints: max,
        }
    }

    /// Create checkpoint before potentially destructive operation
    pub async fn checkpoint(
        &self,
        reason: impl Into<String>,
        mode: CheckpointMode,
    ) -> Result<Checkpoint> {
        let reason = reason.into();
        let id = CheckpointId::new();

        // Stage changes
        self.stage_changes(&mode).await?;

        // Commit
        let git_hash = self.commit_checkpoint(&id, &reason).await?;

        // Query changed files
        let files_changed = self.get_changed_files(&mode).await?;

        let checkpoint = Checkpoint {
            id,
            reason,
            created_at: Utc::now(),
            git_hash,
            files_changed,
            description: None,
        };

        // Store metadata
        self.storage.save_checkpoint(&checkpoint).await?;

        // LRU eviction
        self.evict_old_checkpoints().await?;

        Ok(checkpoint)
    }

    /// List available checkpoints
    pub async fn list(&self) -> Result<Vec<Checkpoint>> {
        self.storage.list_checkpoints().await
    }

    /// Restore workspace to a specific checkpoint
    pub async fn restore(&self, id: &CheckpointId, mode: RestoreMode) -> Result<RestoredCheckpoint> {
        let checkpoint = self.storage.get_checkpoint(id).await?;

        match mode {
            RestoreMode::FilesOnly => {
                // git checkout <commit> -- <files>
                self.checkout_files(&checkpoint).await?;
            }
            RestoreMode::Full => {
                // git reset --hard <commit>
                self.reset_hard(&checkpoint).await?;
            }
        }

        Ok(RestoredCheckpoint {
            files_restored: checkpoint.files_changed.clone(),
            checkpoint,
        })
    }

    /// Show diff between two checkpoints
    pub async fn diff(&self, id1: &CheckpointId, id2: &CheckpointId) -> Result<String> {
        let c1 = self.storage.get_checkpoint(id1).await?;
        let c2 = self.storage.get_checkpoint(id2).await?;
        self.git_diff(&c1.git_hash, &c2.git_hash).await
    }

    // Private helpers
    async fn stage_changes(&self, mode: &CheckpointMode) -> Result<()> { /* git add */ }
    async fn commit_checkpoint(&self, id: &CheckpointId, reason: &str) -> Result<String> { /* git commit */ }
    async fn get_changed_files(&self, mode: &CheckpointMode) -> Result<Vec<PathBuf>> { /* git diff */ }
    async fn evict_old_checkpoints(&self) -> Result<()> { /* LRU cleanup */ }
    async fn checkout_files(&self, checkpoint: &Checkpoint) -> Result<()> { /* git checkout */ }
    async fn reset_hard(&self, checkpoint: &Checkpoint) -> Result<()> { /* git reset */ }
    async fn git_diff(&self, hash1: &str, hash2: &str) -> Result<String> { /* git diff */ }
}

/// Storage trait for checkpoint metadata
#[async_trait::async_trait]
pub trait CheckpointStore: Send + Sync {
    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;
    async fn get_checkpoint(&self, id: &CheckpointId) -> Result<Checkpoint>;
    async fn list_checkpoints(&self) -> Result<Vec<Checkpoint>>;
    async fn delete_checkpoint(&self, id: &CheckpointId) -> Result<()>;
}
```

**CLI Commands:**
```bash
# Create named checkpoint
/checkpoint "Save before risky refactor"

# List checkpoints
/checkpoints

# Restore to checkpoint
/restore <checkpoint-id>

# Show diff between checkpoints
/diff <id1> <id2>

# Delete old checkpoints
/checkpoint-cleanup --keep 10
```

**TUI Display:**
```
Checkpoints (5 total)
├─ checkpoint-abc123  [2:15 PM] before edit_file (main.rs)
├─ checkpoint-def456  [2:10 PM] before bash: cargo test
├─ checkpoint-ghi789  [2:05 PM] user-initiated: "Save before refactor"
├─ checkpoint-jkl012  [2:00 PM] before write (3 files)
└─ checkpoint-mno345  [1:55 PM] session start
```

**Trigger Points:**
- Before `edit_file` tool
- Before `bash` tool with flags: `-x`, `rm`, `mv` (destructive)
- Before `write` tool (multiple files)
- Before `delete` operations
- Manual: `/checkpoint "reason"`

---

### 2.2 Rewind (Session Interaction History)

**Purpose:** Navigate and restore to any previous interaction point in the conversation, with options to rewind conversation only, files only, or everything.

**Files to Create/Modify:**
- New: `crates/rustycode-session/src/rewind.rs` (300-350 lines)
- New: `crates/rustycode-storage/src/rewind_store.rs` (100-150 lines)
- Modify: `crates/rustycode-session/src/lib.rs` (integrate RewindState)

**Core Types:**

```rust
// crates/rustycode-session/src/rewind.rs

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Snapshot of a single interaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionSnapshot {
    pub number: usize,                     // Sequence 0, 1, 2, ...
    pub user_message: String,
    pub assistant_response: String,
    pub tool_calls: Vec<ToolCall>,
    pub conversation_hash: String,        // Hash of full conversation state
    pub files_hash: String,                // Hash of file contents
    pub timestamp: DateTime<Utc>,
}

/// Rewind state manager
pub struct RewindState {
    snapshots: Vec<InteractionSnapshot>,
    current: usize,                        // Current cursor position
    storage: Arc<dyn RewindStore>,
}

#[derive(Debug, Clone, Copy)]
pub enum RewindMode {
    ConversationOnly,                      // Rewind messages, keep files
    FilesOnly,                             // Rewind files, keep messages
    Full,                                  // Rewind everything
}

pub struct RewindResult {
    pub snapshot: InteractionSnapshot,
    pub new_cursor: usize,
    pub mode_applied: RewindMode,
}

impl RewindState {
    /// Create new rewind state for a session
    pub fn new(storage: Arc<dyn RewindStore>) -> Self {
        Self {
            snapshots: Vec::new(),
            current: 0,
            storage,
        }
    }

    /// Record current interaction for rewind capability
    pub async fn record(&mut self, interaction: Interaction) -> Result<()> {
        // Truncate any "future" interactions (linear rewind only)
        if self.current < self.snapshots.len() {
            self.snapshots.truncate(self.current);
        }

        let snapshot = InteractionSnapshot {
            number: self.snapshots.len(),
            user_message: interaction.user_message.clone(),
            assistant_response: interaction.assistant_response.clone(),
            tool_calls: interaction.tool_calls.clone(),
            conversation_hash: self.hash_conversation(&interaction).to_string(),
            files_hash: self.hash_files().await?,
            timestamp: Utc::now(),
        };

        self.snapshots.push(snapshot.clone());
        self.storage.save_snapshot(&snapshot).await?;
        self.current = self.snapshots.len() - 1;

        Ok(())
    }

    /// Rewind to previous interaction
    pub async fn rewind(&mut self, mode: RewindMode) -> Result<RewindResult> {
        if self.current == 0 {
            return Err(anyhow::anyhow!("Already at beginning of session"));
        }

        let target = self.current - 1;
        self.apply_rewind(target, mode).await
    }

    /// Jump to specific interaction number
    pub async fn jump_to(&mut self, target: usize) -> Result<RewindResult> {
        if target >= self.snapshots.len() {
            return Err(anyhow::anyhow!("Target interaction not found"));
        }

        self.apply_rewind(target, RewindMode::Full).await
    }

    /// List available snapshots
    pub fn list_snapshots(&self) -> Vec<InteractionSnapshot> {
        self.snapshots.clone()
    }

    /// Get current cursor position
    pub fn current_position(&self) -> usize {
        self.current
    }

    // Private helpers
    async fn apply_rewind(&mut self, target: usize, mode: RewindMode) -> Result<RewindResult> {
        let snapshot = &self.snapshots[target];

        match mode {
            RewindMode::ConversationOnly => {
                // Restore conversation state (handled by session layer)
                self.restore_conversation(snapshot).await?;
            }
            RewindMode::FilesOnly => {
                // Restore files from snapshot hash
                self.restore_files(snapshot).await?;
            }
            RewindMode::Full => {
                // Restore both
                self.restore_conversation(snapshot).await?;
                self.restore_files(snapshot).await?;
            }
        }

        self.current = target;

        Ok(RewindResult {
            snapshot: snapshot.clone(),
            new_cursor: target,
            mode_applied: mode,
        })
    }

    fn hash_conversation(&self, interaction: &Interaction) -> u64 {
        let mut hasher = DefaultHasher::new();
        interaction.hash(&mut hasher);
        hasher.finish()
    }

    async fn hash_files(&self) -> Result<String> {
        // Walk directory tree, hash file contents
        Ok(String::new())  // Simplified
    }

    async fn restore_conversation(&self, snapshot: &InteractionSnapshot) -> Result<()> {
        // Handled by session layer
        Ok(())
    }

    async fn restore_files(&self, snapshot: &InteractionSnapshot) -> Result<()> {
        // Use file hashes to reconstruct state
        Ok(())
    }
}

/// Storage trait for rewind snapshots
#[async_trait::async_trait]
pub trait RewindStore: Send + Sync {
    async fn save_snapshot(&self, snapshot: &InteractionSnapshot) -> Result<()>;
    async fn get_snapshot(&self, number: usize) -> Result<InteractionSnapshot>;
    async fn list_snapshots(&self, session_id: &str) -> Result<Vec<InteractionSnapshot>>;
    async fn delete_snapshots_before(&self, number: usize) -> Result<()>;
}
```

**TUI Integration:**
```
User presses Esc twice:

┌─────────────────────────────────────────┐
│ Rewind Menu (ESC to close)              │
├─────────────────────────────────────────┤
│ [↑↓] Navigate  [↵] Rewind  [⇥] Mode    │
├─────────────────────────────────────────┤
│  2  [3:05 PM] User: "Fix the error"    │
│ >1  [3:00 PM] User: "Add error handling" │  ← Current
│  0  [2:55 PM] Session started           │
├─────────────────────────────────────────┤
│ Mode: Full  (Tab to switch)             │
│ ConversationOnly | FilesOnly | Full     │
└─────────────────────────────────────────┘
```

**CLI Commands:**
```bash
# Rewind one step
/rewind

# Jump to specific interaction
/rewind 5

# List rewind points
/rewind-history

# Rewind conversation only (keep file changes)
/rewind --mode conversation
```

---

## 3. PHASE 2: HOOKS & PLAN MODE

### 3.1 Hooks System (Extensibility)

**Purpose:** Execute user scripts at lifecycle events with JSON stdin/stdout. Enables custom linting, safety checks, audit logging, budget alerts.

**Files to Create/Modify:**
- New: `crates/rustycode-tools/src/hooks.rs` (400-450 lines)
- New: `.rustycode/hooks/hooks.json` (config template)
- Modify: `crates/rustycode-tools/src/lib.rs` (export HookManager)

**Core Types:**

```rust
// crates/rustycode-tools/src/hooks.rs

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// Hook lifecycle triggers
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    SessionStart,
    SessionEnd,
    PreToolUse,        // Before tool execution
    PostToolUse,       // After tool execution
    PreCompact,        // Before context compaction
    PostCompact,       // After context compaction
    Error,             // On error
}

/// Hook execution profiles (security level)
#[derive(Clone, Copy, Debug)]
pub enum HookProfile {
    Minimal,           // Only essential hooks
    Standard,          // Default hooks
    Strict,            // All hooks including security
}

/// Hook definition from config
#[derive(Clone, Debug, Deserialize)]
pub struct Hook {
    pub name: String,
    pub trigger: HookTrigger,
    pub script: PathBuf,                   // Path to executable
    pub args: Option<Vec<String>>,
    pub timeout_secs: Option<u64>,
    pub enabled: bool,
    pub profile: Option<HookProfile>,      // Minimum profile to run
}

/// Context passed to hook via stdin
#[derive(Serialize)]
pub struct HookInput {
    pub trigger: HookTrigger,
    pub session_id: String,
    pub context: serde_json::Value,        // Tool name, file path, etc
    pub timestamp: String,
}

/// Hook script output
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookAction {
    Block,             // Prevent tool execution
    Log,               // Log the event
    Alert,             // User alert
    Abort,             // Abort session
}

/// Hook execution result
pub struct HookResult {
    pub hook_name: String,
    pub status: HookStatus,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub duration_ms: u128,
}

/// Hook manager
pub struct HookManager {
    hooks_dir: PathBuf,
    hooks: Vec<Hook>,
    profile: HookProfile,
    session_id: String,
}

impl HookManager {
    /// Create new hook manager
    pub fn new(hooks_dir: PathBuf, profile: HookProfile, session_id: String) -> Self {
        Self {
            hooks_dir,
            hooks: Vec::new(),
            profile,
            session_id,
        }
    }

    /// Load hooks from config
    pub async fn load_hooks(&mut self) -> Result<()> {
        let config_path = self.hooks_dir.join("hooks.json");
        if !config_path.exists() {
            return Ok(());  // No hooks configured
        }

        let content = tokio::fs::read_to_string(&config_path).await?;
        let config: HooksConfig = serde_json::from_str(&content)?;

        self.hooks = config.hooks;
        Ok(())
    }

    /// Execute hooks for a trigger event
    pub async fn execute(&self, trigger: HookTrigger, context: serde_json::Value) -> Result<Vec<HookResult>> {
        let relevant_hooks: Vec<_> = self.hooks
            .iter()
            .filter(|h| h.trigger == trigger && h.enabled && self.profile_allows(h))
            .collect();

        if relevant_hooks.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for hook in relevant_hooks {
            match self.run_hook(hook, trigger, &context).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    eprintln!("Hook {} failed: {}", hook.name, e);
                    // Don't fail entirely, collect errors
                }
            }
        }

        Ok(results)
    }

    /// Enable/disable a hook
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        if let Some(hook) = self.hooks.iter_mut().find(|h| h.name == name) {
            hook.enabled = enabled;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Hook not found: {}", name))
        }
    }

    // Private helpers
    async fn run_hook(&self, hook: &Hook, trigger: HookTrigger, context: &serde_json::Value) -> Result<HookResult> {
        let input = HookInput {
            trigger,
            session_id: self.session_id.clone(),
            context: context.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
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

**Hook Configuration** (`.rustycode/hooks/hooks.json`):
```json
{
  "profile": "standard",
  "hooks": [
    {
      "name": "lint-on-edit",
      "trigger": "post_tool_use",
      "script": "./hooks/lint.sh",
      "args": ["--strict"],
      "timeout_secs": 30,
      "enabled": true,
      "profile": "standard"
    },
    {
      "name": "cost-alert",
      "trigger": "post_tool_use",
      "script": "./hooks/cost-check.sh",
      "enabled": true,
      "profile": "standard"
    },
    {
      "name": "security-scan",
      "trigger": "post_tool_use",
      "script": "./hooks/security.sh",
      "enabled": true,
      "profile": "strict"
    }
  ]
}
```

**Example Hook Script** (`.rustycode/hooks/lint.sh`):
```bash
#!/bin/bash
# Hook script: validates linting after edits
# stdin: HookInput JSON
# stdout: HookOutput JSON

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

---

### 3.2 Plan Mode (Execution Gates)

**Purpose:** Enforce a planning phase before implementation. Agent analyzes task in read-only mode, generates a plan with risks/costs, waits for user approval before executing changes.

**Files to Create/Modify:**
- New: `crates/rustycode-orchestra/src/plan_mode.rs` (300-350 lines)
- Modify: `crates/rustycode-orchestra/src/auto.rs` (integrate phases)
- Modify: `crates/rustycode-tools/src/lib.rs` (restrict tools in planning)

**Core Types:**

```rust
// crates/rustycode-orchestra/src/plan_mode.rs

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Execution phases for plan-first workflow
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionPhase {
    Planning,           // Read-only analysis
    Implementation,     // Actual changes
}

/// Plan mode configuration
#[derive(Clone, Debug, Deserialize)]
pub struct PlanModeConfig {
    pub enabled: bool,
    pub require_approval: bool,
    pub allowed_tools_planning: Vec<String>,     // read, grep, ls, etc
    pub allowed_tools_implementation: Vec<String>, // all tools
}

/// Default plan mode config
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
                "web_search".to_string(),
                "web_fetch".to_string(),
                "lsp".to_string(),
            ],
            allowed_tools_implementation: vec![
                "read".to_string(),
                "edit".to_string(),
                "write".to_string(),
                "bash".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "list_dir".to_string(),
            ],
        }
    }
}

/// Structured plan produced by agent
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub summary: String,                        // 1-2 sentence description
    pub approach: String,                       // Detailed approach
    pub files_to_modify: Vec<FilePlan>,
    pub commands_to_run: Vec<CommandPlan>,
    pub estimated_tokens: TokenEstimate,
    pub estimated_cost: f64,                    // USD
    pub risks: Vec<Risk>,
    pub success_criteria: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FilePlan {
    pub path: String,
    pub action: FileAction,                     // Create | Modify | Delete
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

/// Approval token (opaque identifier)
#[derive(Clone, Debug)]
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

/// Plan mode manager
pub struct PlanMode {
    config: PlanModeConfig,
    current_phase: ExecutionPhase,
    approved_plans: HashSet<String>,  // Approved plan IDs
}

impl PlanMode {
    /// Create new plan mode manager
    pub fn new(config: PlanModeConfig) -> Self {
        Self {
            config,
            current_phase: ExecutionPhase::Planning,
            approved_plans: HashSet::new(),
        }
    }

    /// Check if tool is allowed in current phase
    pub fn is_tool_allowed(&self, tool: &str) -> Result<(), ToolBlockedReason> {
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

    /// Generate plan (read-only execution)
    pub async fn generate_plan(&self, task: &str) -> Result<Plan> {
        // Set execution context to planning phase
        // Execute agent with restricted toolset
        // Return plan
        Ok(Plan {
            summary: task.to_string(),
            approach: "".to_string(),
            files_to_modify: Vec::new(),
            commands_to_run: Vec::new(),
            estimated_tokens: TokenEstimate { input: 0, output: 0 },
            estimated_cost: 0.0,
            risks: Vec::new(),
            success_criteria: Vec::new(),
        })
    }

    /// Present plan to user for approval
    pub fn present_plan(&self, plan: &Plan) -> ApprovalToken {
        // Display plan, risks, costs
        // Wait for user approval
        ApprovalToken::new("plan-1")
    }

    /// Approve and transition to implementation phase
    pub fn approve(&mut self, token: ApprovalToken) -> Result<()> {
        self.approved_plans.insert(token.token);
        self.current_phase = ExecutionPhase::Implementation;
        Ok(())
    }

    /// Get current execution phase
    pub fn current_phase(&self) -> ExecutionPhase {
        self.current_phase
    }
}

#[derive(Debug)]
pub enum ToolBlockedReason {
    NotAllowedInPhase {
        tool: String,
        phase: ExecutionPhase,
    },
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
```

**CLI Commands:**
```bash
# Plan-first workflow
$ rustycode plan "add error handling to main.rs"
# Generates plan, shows risks and costs, waits for approval

# Execute approved plan
$ rustycode execute --plan <plan-id>

# Auto mode with plan-first enforcement
$ rustycode auto --plan-first "refactor authentication module"

# Skip planning (dangerous, requires confirmation)
$ rustycode auto --no-plan "simple documentation fix"
```

**Plan Presentation (TUI):**
```
╔════════════════════════════════════════════════════════╗
║ PLAN: Add error handling to main.rs                    ║
╠════════════════════════════════════════════════════════╣
║                                                        ║
║ Summary: Add Result<> error handling to main function ║
║          and propagate errors properly                 ║
║                                                        ║
║ Files to Modify:                                       ║
║  • src/main.rs (modify)                               ║
║                                                        ║
║ Risks:                                                 ║
║  🔴 HIGH: Changes to main entry point                 ║
║  🟡 MEDIUM: May break existing error recovery         ║
║                                                        ║
║ Estimated Costs:                                       ║
║  • Input tokens: ~500                                  ║
║  • Output tokens: ~1000                                ║
║  • Estimated cost: $0.04                               ║
║                                                        ║
║ [A] Approve and execute  [E] Edit plan  [C] Cancel    ║
╚════════════════════════════════════════════════════════╝
```

---

## 4. PHASE 3: PARALLEL FEATURES

### 4.1 Enhanced Skills (Progressive Disclosure)

**Modification to `crates/rustycode-skill/src/lib.rs`:**

Progressive loading reduces context bloat by loading metadata first, content on-demand.

```rust
// Enhancement to rustycode-skill/src/lib.rs

#[derive(Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,          // Keywords: "test", "async"
    pub mode: Option<String>,           // "code", "plan", "debug"
    pub priority: u8,                   // 1-10, lower = higher
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
    /// Load only metadata (instant)
    pub async fn load_metadata(&self) -> Result<Vec<SkillMetadata>> {
        // Read .json sidecar files
    }

    /// Find relevant skills for context (metadata-based)
    pub async fn find_relevant(&self, context: &str) -> Result<Vec<SkillMetadata>> {
        // Score by trigger matching + priority
    }

    /// Load full skill content on-demand
    pub async fn load_skill(&self, name: &str) -> Result<String> {
        // Check content_cache, then read file if missing
    }

    fn relevance_score(&self, skill: &SkillMetadata, context: &str) -> f32 {
        // Fuzzy match triggers, apply priority
    }
}
```

**Benefit:** Metadata for 100 skills = ~1MB total. Instead of loading all content (~20MB), load only relevant skills on-demand.

---

### 4.2 Cost Tracking (Real-time Token Accounting)

**New file: `crates/rustycode-llm/src/cost_tracker.rs`:**

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Single API call record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiCall {
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub timestamp: DateTime<Utc>,
    pub tool_name: Option<String>,      // Which tool triggered this
}

/// Cost tracker for current session
pub struct CostTracker {
    calls: Vec<ApiCall>,
    budget_limit: Option<f64>,          // Optional spend cap
}

impl CostTracker {
    pub fn new(budget_limit: Option<f64>) -> Self {
        Self {
            calls: Vec::new(),
            budget_limit,
        }
    }

    /// Record an LLM API call
    pub fn record_call(&mut self, call: ApiCall) -> Result<()> {
        self.calls.push(call);
        self.check_budget_exceeded()?;
        Ok(())
    }

    /// Check remaining budget
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

    /// Get cost summary for current session
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

    /// Get costs broken down by tool
    pub fn costs_by_tool(&self) -> HashMap<String, f64> {
        let mut map: HashMap<String, f64> = HashMap::new();
        for call in &self.calls {
            let tool = call.tool_name.clone().unwrap_or("unknown".to_string());
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

**Integration:** Each LLM provider call automatically records cost. Dashboard shows real-time spending in TUI status bar.

---

### 4.3 Provider Fallback (Multi-Provider Resilience)

**Enhancement to `crates/rustycode-llm/src/lib.rs`:**

```rust
// Fallback chain for multi-provider resilience

pub struct ProviderFallbackChain {
    providers: Vec<Box<dyn LLMProvider>>,
    fallback_enabled: bool,
    retry_policy: RetryPolicy,
}

pub enum RetryPolicy {
    Immediate,
    ExponentialBackoff { max_retries: u32, base_delay_ms: u64 },
}

impl ProviderFallbackChain {
    /// Execute request with automatic fallback to next provider
    pub async fn execute_with_fallback(&self, request: LLMRequest) -> Result<LLMResponse> {
        for (i, provider) in self.providers.iter().enumerate() {
            match provider.call(&request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    let is_last = i == self.providers.len() - 1;
                    if !is_last && self.fallback_enabled {
                        log::warn!("Provider {} failed, trying next: {}", provider.name(), e);
                        continue;  // Try next provider
                    } else {
                        return Err(e);  // Last provider failed or fallback disabled
                    }
                }
            }
        }
        Err("All providers exhausted".into())
    }
}
```

**Configuration** (`.rustycode/config.toml`):
```toml
[llm]
primary_provider = "anthropic"

[llm.fallback]
enabled = true
providers = ["anthropic", "openai", "gemini"]  # Try in order
retry_policy = "exponential_backoff"
max_retries = 3
```

---

## 5. PHASE 4: ADVANCED FEATURES

### 4.1 Subagents
Deferred to Phase 4 (lower priority, higher complexity). Would enable parallel task execution with independent contexts.

### 4.2 Compaction
Deferred to Phase 4. Would enhance context compaction for long sessions.

---

## 6. DATA FLOW & SESSION LIFECYCLE

Typical session flow:

```
1. SESSION START
   ├─ HookManager.execute(SessionStart)
   ├─ SkillLoader.load_metadata() [lightweight]
   ├─ RewindState.new()
   ├─ CostTracker.new(budget_limit)
   └─ CheckpointManager.new()

2. USER: "Fix bug in authentication"
   ├─ PlanMode.generate_plan() [read-only tools only]
   ├─ Present plan to user
   └─ Wait for approval

3. USER APPROVES
   ├─ PlanMode.approve()
   ├─ Transition to implementation phase
   └─ Enable write/edit tools

4. BEFORE EACH MODIFICATION
   ├─ CheckpointManager.checkpoint("before edit_file")
   └─ Record in checkpoint storage

5. EXECUTE MODIFICATION
   ├─ Tool executor runs (write/edit/bash)
   ├─ LLM call made (if needed)
   ├─ CostTracker.record_call()
   └─ Hook: PostToolUse trigger

6. RECORD INTERACTION
   ├─ RewindState.record(interaction)
   ├─ Store in rewind storage
   └─ Update TUI cursor position

7. PERIODICALLY
   ├─ CostTracker.check_budget() → warn if approaching limit
   ├─ Hook: PreCompact/PostCompact triggers (if compacting)
   └─ TUI displays status (cost, checkpoints, rewind position)

8. ERROR OCCURS
   ├─ HookManager.execute(Error)
   ├─ User can /checkpoint "save state"
   ├─ User can /rewind to previous step
   ├─ Or /restore to git checkpoint
   └─ Or /plan "recovery approach"

9. SESSION END
   ├─ HookManager.execute(SessionEnd)
   ├─ CostTracker.finalize_summary()
   ├─ CheckpointManager.evict_old() [LRU cleanup]
   ├─ RewindState.persist()
   └─ Close session
```

---

## 7. TESTING STRATEGY

Each feature tested at unit + integration level:

```bash
# Phase 1
cargo test -p rustycode-tools checkpoint
cargo test -p rustycode-session rewind

# Phase 2
cargo test -p rustycode-tools hooks
cargo test -p rustycode-orchestra plan_mode

# Phase 3
cargo test -p rustycode-skill progressive_loading
cargo test -p rustycode-llm cost_tracker
cargo test -p rustycode-llm provider_fallback

# Integration
cargo test --test integration --workspace

# Full suite
cargo test --workspace
```

**Coverage Targets:**
- Checkpoint: 85%+ (critical safety)
- Rewind: 85%+ (critical safety)
- Hooks: 75%+ (extensibility)
- Plan Mode: 85%+ (execution safety)
- Cost Tracker: 70%+ (informational)
- Skills: 65%+ (progressive loader)
- Provider Fallback: 75%+ (resilience)

---

## 8. IMPLEMENTATION SEQUENCE (Approach A: Foundation First)

| # | Phase | Feature | Files | Complexity | Priority |
|---|-------|---------|-------|-----------|---------|
| 1 | 1a | Checkpoints | checkpoint.rs, checkpoint_store.rs | Medium | **1** |
| 2 | 1b | Rewind | rewind.rs, rewind_store.rs | Medium | **2** |
| 3 | 2a | Hooks | hooks.rs + config | Medium | **3** |
| 4 | 2b | Plan Mode | plan_mode.rs + modifications | Medium | **4** |
| 5 | 3a | Skills Enhancement | skill/lib.rs modifications | Low | **5** |
| 6 | 3b | Cost Tracking | cost_tracker.rs | Low | **6** |
| 7 | 3c | Provider Fallback | llm/lib.rs modifications | Low | **7** |
| 8 | 4a | Subagents | subagent.rs (deferred) | High | 8 |
| 9 | 4b | Compaction | memory/lib.rs (deferred) | Medium | 9 |

**Estimated effort:** ~2500-3000 LOC, 80-120 hours for solo developer

---

## 9. SUCCESS CRITERIA

### Phase 1 Complete (Reversibility)
- [ ] Checkpoints create git commits before modifications
- [ ] List/restore/diff commands work
- [ ] Automatic triggers work (before edit/bash/write)
- [ ] 85%+ test coverage
- [ ] Manual testing: recover from bad edit

### Phase 2 Complete (Execution Safety)
- [ ] Rewind navigation works in TUI (Esc twice)
- [ ] Plan Mode restricts tools in planning phase
- [ ] Hooks execute at lifecycle events
- [ ] Hook scripts can block/alert
- [ ] 85%+ test coverage (plan mode), 75%+ (hooks)
- [ ] Manual testing: plan-first workflow works

### Phase 3 Complete (Enhanced)
- [ ] Skills load metadata first, content on-demand
- [ ] Cost Tracker records all API calls
- [ ] Budget warnings work
- [ ] Provider fallback tries next on failure
- [ ] 70%+ test coverage each

### All Four Pillars
- [ ] Reversibility (checkpoints + rewind)
- [ ] Approval gates (plan mode)
- [ ] Extensibility (hooks)
- [ ] Cost visibility (cost tracking)

---

## 10. RISKS & MITIGATIONS

| Risk | Mitigation |
|------|-----------|
| Git repo gets bloated with checkpoints | LRU eviction, max checkpoint count config |
| Rewind state takes too much disk | Only store hashes + deltas, not full files |
| Hooks block execution unexpectedly | Default to allow, configurable profiles |
| Plan Mode slows down fast iterations | Ability to skip with `--no-plan` flag |
| Cost tracking adds overhead | Minimal overhead (just record calls) |

---

## Approval Checklist

- [x] Architecture is sound
- [x] Integration points are clear
- [x] All four safety pillars addressed
- [x] Parallel tracks identified
- [x] Testing strategy defined
- [x] Success criteria clear
- [x] Risks documented

**Ready to proceed to implementation planning.**
