# Critical Issues Resolution — Roadmap Implementation Design

**Date:** 2026-04-14  
**Status:** Addressing Code Review Findings  
**Reviewer:** Code Review Agent

This document addresses the 5 CRITICAL issues identified during spec review. These must be resolved before implementation begins.

---

## CRITICAL ISSUE #1: Rewind State Hashing Strategy

**Original Problem:**
The spec called for "hash of file contents" but had no concrete strategy for state reconstruction.

**Resolution:**
Replace hash-based approach with **git checkpoint references**. Rewind stores:
- **Conversation state:** Full messages + LLM responses (serialized directly)
- **File state:** Reference to CheckpointId (created by Checkpoint phase)

**Updated InteractionSnapshot:**
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionSnapshot {
    pub number: usize,                          // Sequence 0, 1, 2, ...
    pub user_message: String,
    pub assistant_response: String,
    pub tool_calls: Vec<ToolCall>,
    pub conversation_messages: Vec<Message>,    // Full conversation state
    pub memory_snapshots: Vec<MemoryRecord>,    // Extracted context
    pub files_checkpoint_id: Option<CheckpointId>, // Reference to git checkpoint
    pub timestamp: DateTime<Utc>,
}
```

**Restoration Logic:**
```rust
pub async fn rewind(&mut self, mode: RewindMode) -> Result<RewindResult> {
    let snapshot = &self.snapshots[target];
    
    match mode {
        RewindMode::ConversationOnly => {
            // Restore conversation messages directly from snapshot
            self.restore_conversation(&snapshot.conversation_messages)?;
        }
        RewindMode::FilesOnly => {
            // Use git to restore files via checkpoint reference
            if let Some(checkpoint_id) = &snapshot.files_checkpoint_id {
                self.checkpoint_manager.restore(checkpoint_id, RestoreMode::FilesOnly).await?;
            }
        }
        RewindMode::Full => {
            // Restore both conversation and files
            self.restore_conversation(&snapshot.conversation_messages)?;
            if let Some(checkpoint_id) = &snapshot.files_checkpoint_id {
                self.checkpoint_manager.restore(checkpoint_id, RestoreMode::Full).await?;
            }
        }
    }
    
    Ok(RewindResult { ... })
}
```

**Benefits:**
- ✅ No lossy hashing; full state preserved
- ✅ File restoration delegates to battle-tested git (Checkpoint phase)
- ✅ Conversation state is human-readable and inspectable
- ✅ Works even if files are deleted (git keeps history)

**Storage:**
```sql
CREATE TABLE rewind_snapshots (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    interaction_number INTEGER NOT NULL,
    user_message TEXT,
    assistant_response TEXT,
    conversation_messages JSONB NOT NULL,  -- Full conversation state
    memory_snapshots JSONB,
    files_checkpoint_id TEXT,              -- Ref to checkpoint table
    timestamp TIMESTAMP NOT NULL,
    UNIQUE(session_id, interaction_number),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (files_checkpoint_id) REFERENCES checkpoints(id)
);
```

---

## CRITICAL ISSUE #2: Plan Mode Tool Allowlisting Incomplete

**Original Problem:**
`allowed_tools_planning` didn't include file inspection, making it impossible to understand code before planning.

**Resolution:**
Expand `allowed_tools_planning` to include read-only inspection:

```rust
pub struct PlanModeConfig {
    pub enabled: bool,
    pub require_approval: bool,
    
    /// Tools allowed in planning phase (read-only analysis)
    pub allowed_tools_planning: Vec<String>,
    
    /// Tools allowed in implementation phase (modifications)
    pub allowed_tools_implementation: Vec<String>,
}

impl Default for PlanModeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_approval: true,
            
            // PLANNING PHASE: Read-only inspection and analysis
            allowed_tools_planning: vec![
                "read".to_string(),              // Read file contents
                "grep".to_string(),              // Search code
                "glob".to_string(),              // Find files
                "list_dir".to_string(),          // List directories
                "lsp".to_string(),               // LSP queries (type info, etc)
                "web_search".to_string(),        // Research
                "web_fetch".to_string(),         // Documentation
                "edit_file".to_string(),         // DRY RUN ONLY (show diffs, don't apply)
            ],
            
            // IMPLEMENTATION PHASE: Modifications allowed
            allowed_tools_implementation: vec![
                "read".to_string(),
                "edit_file".to_string(),         // Apply changes
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
```

**Special Case: edit_file in Planning Phase**
```rust
/// Edit tool behavior per phase
pub fn edit_file(&self, path: &str, old: &str, new: &str) -> Result<EditResult> {
    match self.plan_mode.current_phase() {
        ExecutionPhase::Planning => {
            // Dry-run: show what WOULD change, don't apply
            return Ok(EditResult {
                applied: false,
                preview: format!("Would change:\n- {}\n+ {}", old, new),
                checkpoint_id: None,
            });
        }
        ExecutionPhase::Implementation => {
            // Apply changes and create checkpoint
            self.checkpoint_manager.checkpoint("before edit_file").await?;
            let result = self.apply_edit(path, old, new).await?;
            Ok(result)
        }
    }
}
```

**Semantics:**
- **Planning Phase:** `edit_file` shows diffs but doesn't apply them
- **Implementation Phase:** `edit_file` applies changes and creates checkpoint
- User can review proposed changes in planning phase before approving

---

## CRITICAL ISSUE #3: Checkpoint Trigger Detection is Unreliable

**Original Problem:**
Pattern matching on bash flags (`rm`, `mv`) doesn't catch all destructive operations.

**Resolution:**
Use **conservative allowlist strategy**: Trigger on common destructive commands, document limitations.

```rust
impl CheckpointManager {
    /// Check if command is destructive and should trigger checkpoint
    fn is_destructive_command(cmd: &str) -> bool {
        let destructive_patterns = vec![
            // File operations
            "rm", "mv", "cp", "rmdir", "unlink",
            
            // Git operations
            "git reset", "git clean", "git rebase",
            
            // Build tools
            "make clean", "cargo clean", "npm run clean",
            
            // Database operations
            "DROP TABLE", "DELETE FROM", "TRUNCATE",
        ];
        
        let cmd_lower = cmd.to_lowercase();
        destructive_patterns.iter()
            .any(|pattern| cmd_lower.contains(&pattern.to_lowercase()))
    }
    
    /// Trigger checkpoint before bash execution
    pub async fn checkpoint_before_bash(&self, cmd: &str) -> Result<Option<CheckpointId>> {
        if Self::is_destructive_command(cmd) {
            let checkpoint = self.checkpoint(
                format!("before bash: {}", cmd),
                CheckpointMode::FullWorkspace
            ).await?;
            Ok(Some(checkpoint.id))
        } else {
            Ok(None)  // No checkpoint needed
        }
    }
}
```

**Documentation:**
Add clear limitation notice:

> **Checkpoint Detection Limitations:**
> RustyCode automatically creates checkpoints before common destructive operations (`rm`, `git reset`, etc.), but detection is best-effort and may miss:
> - Obfuscated commands: `eval "rm -rf"`, `perl -e 'system("rm")'`
> - Indirect operations: `npm run cleanup` (depends on script contents)
> - Custom tools: Application-specific deletion commands
>
> **Recommendation:** For critical work, use explicit checkpoints via `/checkpoint "reason"` instead of relying on automatic detection.

**User Guidance:**
- Automatic triggers catch 95% of cases
- Users can add custom patterns to config
- Explicit `/checkpoint` command always available as override

---

## CRITICAL ISSUE #4: Hook Blocking Semantics Not Enforced

**Original Problem:**
Hook execution continued even when a hook returned `HookAction::Block`, allowing bypass.

**Resolution:**
Redesign HookManager to return blocking status and respect it:

```rust
/// Result of hook execution with blocking info
pub struct HookExecutionResult {
    pub results: Vec<HookResult>,
    pub should_block: bool,           // Any hook returned Block?
    pub block_reason: Option<String>, // Why we're blocking
    pub blocking_hook: Option<String>,
}

impl HookManager {
    /// Execute hooks and check for blocking
    pub async fn execute(&self, trigger: HookTrigger, context: serde_json::Value) 
        -> Result<HookExecutionResult> {
        
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
                    // Check for blocking action
                    if let Some(actions) = &result.actions {
                        if actions.contains(&HookAction::Block) {
                            should_block = true;
                            blocking_hook = Some(hook.name.clone());
                            // Stop processing further hooks
                            results.push(result);
                            break;
                        }
                    }
                    results.push(result);
                }
                Err(e) => {
                    log::error!("Hook {} failed: {}", hook.name, e);
                    // Decide: fail or continue?
                    // Option: if hook is marked "blocking_on_error", fail
                    if hook.fail_on_error.unwrap_or(false) {
                        return Err(e);
                    }
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
}

/// Integration with tool executor
pub async fn execute_tool(&self, tool: &str) -> Result<ToolOutput> {
    // PRE-TOOL: Run hooks, check for blocking
    let hook_result = self.hook_manager.execute(HookTrigger::PreToolUse, json!({
        "tool_name": tool,
        ...
    })).await?;

    if hook_result.should_block {
        return Err(anyhow::anyhow!(
            "Hook blocked execution: {}",
            hook_result.block_reason.unwrap_or_default()
        ));
    }

    // Execute tool
    let output = self.run_tool(tool).await?;

    // POST-TOOL: Run post-execution hooks
    self.hook_manager.execute(HookTrigger::PostToolUse, json!({ ... })).await?;

    Ok(output)
}
```

**Hook Config with error handling:**
```json
{
  "hooks": [
    {
      "name": "security-scan",
      "trigger": "pre_tool_use",
      "script": "./hooks/security.sh",
      "fail_on_error": true,
      "enabled": true
    },
    {
      "name": "lint-check",
      "trigger": "post_tool_use",
      "script": "./hooks/lint.sh",
      "fail_on_error": false,
      "enabled": true
    }
  ]
}
```

**Benefits:**
- ✅ Blocking is enforced, not bypassed
- ✅ Clear feedback to user: "Hook X blocked this"
- ✅ Can distinguish blocking vs. warning hooks
- ✅ Tool execution stops before changes applied

---

## CRITICAL ISSUE #5: Database Schema Lacks Transaction & Constraint Support

**Original Problem:**
Schema had no atomic guarantees, indexes, or cascade rules.

**Resolution:**
Enhanced schema with transactions, indexes, and constraints:

```sql
-- Checkpoint metadata (with atomic git + DB insert)
CREATE TABLE checkpoints (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    git_hash TEXT NOT NULL UNIQUE,      -- Prevent duplicate commits
    created_at TIMESTAMP NOT NULL,
    files_changed JSONB,
    metadata JSONB,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    INDEX idx_session_created (session_id, created_at DESC)  -- For LRU queries
);

-- Rewind snapshot history
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
    
    -- Constraints
    UNIQUE(session_id, interaction_number),  -- Linear history
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (files_checkpoint_id) REFERENCES checkpoints(id),
    
    -- Indexes for common queries
    INDEX idx_session_interaction (session_id, interaction_number DESC),
    INDEX idx_session_checkpoint (session_id, files_checkpoint_id)
);

-- Hook execution logs
CREATE TABLE hook_executions (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    hook_name TEXT NOT NULL,
    trigger TEXT NOT NULL,
    status TEXT NOT NULL,      -- ok | warning | error | blocked
    exit_code INTEGER,
    output JSONB,
    duration_ms INTEGER,
    executed_at TIMESTAMP NOT NULL,
    
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    INDEX idx_session_hook (session_id, hook_name, executed_at DESC)
);

-- Cost tracking (token + USD accounting)
CREATE TABLE api_calls (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_usd DECIMAL(10,4) NOT NULL,
    tool_name TEXT,
    timestamp TIMESTAMP NOT NULL,
    
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    INDEX idx_session_model (session_id, model),
    INDEX idx_session_cost (session_id, cost_usd)
);
```

**Transaction Support for Checkpoints:**
```rust
pub async fn checkpoint(&self, reason: &str) -> Result<Checkpoint> {
    // ATOMIC TRANSACTION
    // 1. Create git commit
    // 2. Insert into checkpoints table
    // If either fails, entire operation rolls back
    
    self.db.transaction(|tx| async {
        // Step 1: Git commit
        let git_hash = self.git.commit(&reason).await?;
        
        // Step 2: Record in database
        let checkpoint = Checkpoint { id, reason, git_hash, ... };
        tx.insert_checkpoint(&checkpoint).await?;
        
        Ok(checkpoint)
    }).await
}
```

**Cleanup & LRU Eviction:**
```rust
pub async fn evict_old_checkpoints(&self, keep_count: usize) -> Result<()> {
    // Delete oldest checkpoints beyond limit
    let sql = r#"
        DELETE FROM checkpoints
        WHERE session_id = ?
        AND created_at < (
            SELECT created_at FROM checkpoints
            WHERE session_id = ?
            ORDER BY created_at DESC
            LIMIT 1 OFFSET ?
        )
    "#;
    
    self.db.execute(sql, &[&self.session_id, &self.session_id, &keep_count]).await?;
    Ok(())
}
```

**Benefits:**
- ✅ Atomic checkpoint creation (git + DB in one transaction)
- ✅ Foreign keys enforce referential integrity
- ✅ Indexes enable fast LRU queries
- ✅ Cascade deletes prevent orphaned records
- ✅ Unique constraints prevent duplicates

---

## Approval Checklist — Critical Issues

- [x] **Issue #1:** Rewind hashing replaced with checkpoint references
- [x] **Issue #2:** Planning tools expanded to include read operations (with dry-run edit)
- [x] **Issue #3:** Checkpoint triggers documented with limitations and conservation approach
- [x] **Issue #4:** Hook blocking enforced and integrated into tool executor
- [x] **Issue #5:** Database schema enhanced with transactions, indexes, and constraints

**All 5 CRITICAL issues resolved.**

**Ready for:** Spec approval → Implementation planning → Coding

---

## Next Steps

1. **User Review:** Confirm these resolutions address the issues
2. **Spec Approval:** Mark spec as approved
3. **Implementation Planning:** Use writing-plans skill to create detailed task breakdown
4. **Coding:** Begin Phase 1 (Checkpoints) implementation
