# Implementation Plan: Git Stash Support for RustyCode

**Date:** 2026-05-05
**Status:** Draft
**Scope:** Add `git_stash` tool family (stash, pop, list, drop, apply) to `rustycode-tools`

---

## Overview

RustyCode already has git tooling (`git_status`, `git_diff`, `git_commit`, `git_log`) implemented in `crates/rustycode-tools/src/providers/git.rs` and registered via `crates/rustycode-tools/src/providers/git_provider.rs`. This plan adds stash operations following the exact same patterns.

The stash tools allow the agent to temporarily shelve working-directory changes, restore them later, and manage multiple stash entries — critical for safe autonomous workflows that need to switch contexts or clean the working tree.

---

## Phase 1: Core Stash Tool Structs & Execution (git.rs)

**File:** `crates/rustycode-tools/src/providers/git.rs`

### New structs

```rust
pub struct GitStashTool;       // git stash push
pub struct GitStashPopTool;    // git stash pop
pub struct GitStashListTool;   // git stash list
pub struct GitStashDropTool;   // git stash drop
pub struct GitStashApplyTool;  // git stash apply
```

### Tool implementations

#### 1. `GitStashTool` — `git_stash`

- **Name:** `"git_stash"`
- **Permission:** `ToolPermission::Write` (modifies working tree)
- **Tags:** `[ToolTag::Ops]`
- **Defer loading:** `Some(true)`
- **Parameters schema:**
  ```json
  {
    "type": "object",
    "properties": {
      "message": {
        "type": "string",
        "description": "Optional description for the stash entry"
      },
      "include_untracked": {
        "type": "boolean",
        "description": "Include untracked files (git stash -u). Default: false"
      },
      "pathspec": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Optional list of paths to stash (git stash push -- <paths>)"
      }
    }
  }
  ```
- **Execute logic:**
  1. If `pathspec` is provided, validate each path via `validate_read_path` (same security pattern as `GitCommitTool`).
  2. Build args: `["stash", "push"]`
  3. If `message` → append `["-m", message]`
  4. If `include_untracked` → append `["-u"]` (must come before `--`)
  5. If `pathspec` → append `["--"]` then the validated paths
  6. Call `run_git(ctx, &args)`
  7. Parse output: if output contains "No local changes to save", return structured `{"stashed": false}`; otherwise return `{"stashed": true, "message": <message_or_auto>}`
- **Error handling:** Return `anyhow!("No changes to stash")` if nothing to stash and caller expects an error; or return structured result with `stashed: false`.

#### 2. `GitStashPopTool` — `git_stash_pop`

- **Name:** `"git_stash_pop"`
- **Permission:** `ToolPermission::Write`
- **Parameters schema:**
  ```json
  {
    "type": "object",
    "properties": {
      "index": {
        "type": "integer",
        "description": "Stash index to pop (default: 0, i.e. latest)"
      }
    }
  }
  ```
- **Execute logic:**
  1. Build args: `["stash", "pop"]`
  2. If `index` → append `[format!("stash@{{{}}}", index)]`
  3. Call `run_git(ctx, &args)`
  4. If exit code non-zero and stderr contains "CONFLICT" → return error with conflict details
  5. Return structured `{"popped": true, "index": index}`
- **Edge case:** Handle merge conflicts gracefully — return an error suggesting the user resolve conflicts or use `git_stash_apply` instead.

#### 3. `GitStashApplyTool` — `git_stash_apply`

- **Name:** `"git_stash_apply"`
- **Permission:** `ToolPermission::Write`
- **Parameters schema:**
  ```json
  {
    "type": "object",
    "properties": {
      "index": {
        "type": "integer",
        "description": "Stash index to apply (default: 0)"
      },
      "index_only": {
        "type": "boolean",
        "description": "Only restore staged changes (--index flag). Default: false"
      }
    }
  }
  ```
- **Execute logic:**
  1. Build args: `["stash", "apply"]`
  2. If `index_only` → append `["--index"]`
  3. If `index` → append `[format!("stash@{{{}}}", index)]`
  4. Call `run_git(ctx, &args)`
  5. Return structured `{"applied": true, "index": index, "conflicts": <bool>}`

#### 4. `GitStashListTool` — `git_stash_list`

- **Name:** `"git_stash_list"`
- **Permission:** `ToolPermission::Read`
- **Parameters schema:**
  ```json
  {
    "type": "object",
    "properties": {}
  }
  ```
- **Execute logic:**
  1. Call `run_git(ctx, &["stash", "list"])`
  2. Parse each line: format is `stash@{N}: On branch: message`
  3. Build structured output:
     ```json
     {
       "stashes": [
         {"index": 0, "branch": "main", "message": "WIP on main: abc1234 last commit msg"},
         ...
       ],
       "count": 2
     }
     ```
  4. If no stashes → return `{"stashes": [], "count": 0}`

#### 5. `GitStashDropTool` — `git_stash_drop`

- **Name:** `"git_stash_drop"`
- **Permission:** `ToolPermission::Write`
- **Parameters schema:**
  ```json
  {
    "type": "object",
    "properties": {
      "index": {
        "type": "integer",
        "description": "Stash index to drop (default: 0)"
      }
    }
  }
  ```
- **Execute logic:**
  1. Build args: `["stash", "drop"]`
  2. If `index` → append `[format!("stash@{{{}}}", index)]`
  3. Call `run_git(ctx, &args)`
  4. Return structured `{"dropped": true, "index": index}`
  5. If stash doesn't exist → return error

---

## Phase 2: Registration (git_provider.rs)

**File:** `crates/rustycode-tools/src/providers/git_provider.rs`

### Changes

Add imports and register all 5 new tools:

```rust
use crate::providers::git::{
    GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool,
    GitStashTool, GitStashPopTool, GitStashListTool, GitStashDropTool, GitStashApplyTool,
};

impl ToolProvider for GitProvider {
    fn register_tools(&self, registry: &mut ToolRegistry) -> Result<()> {
        // ... existing registrations ...
        registry.register(GitStashTool);
        registry.register(GitStashPopTool);
        registry.register(GitStashListTool);
        registry.register(GitStashDropTool);
        registry.register(GitStashApplyTool);
        Ok(())
    }
}
```

---

## Phase 3: Security Validation

**File:** `crates/rustycode-tools/src/providers/git.rs`

### Path validation for `GitStashTool`

When `pathspec` is provided, each path must be validated using `validate_read_path` to prevent path traversal attacks. This follows the exact pattern already used in `GitCommitTool::execute` and `GitDiffTool::execute`:

```rust
// In GitStashTool::execute, when pathspec is provided:
let paths = params["pathspec"]
    .as_array()
    .map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();

for p in &paths {
    validate_read_path(p).with_context(|| format!("invalid path in stash pathspec: {}", p))?;
}
```

No changes needed in `crates/rustycode-tools/src/security.rs` — the existing `validate_read_path` covers all our needs.

---

## Phase 4: Unit Tests

**File:** `crates/rustycode-tools/src/providers/git.rs` (in the existing `#[cfg(test)] mod tests` block)

All tests reuse the existing `create_test_repo()` and `create_context()` helpers. No new test infrastructure needed.

### Test cases for `GitStashTool`

| Test name | Description |
|-----------|-------------|
| `test_git_stash_basic` | Create dirty file, stash, verify working tree is clean |
| `test_git_stash_with_message` | Stash with custom message, verify in `stash list` output |
| `test_git_stash_include_untracked` | Create untracked file, stash with `-u`, verify it's included |
| `test_git_stash_no_changes` | Stash when tree is clean — verify `stashed: false` |
| `test_git_stash_pathspec` | Stash only specific files, verify others remain dirty |
| `test_git_stash_blocks_path_traversal` | Pass `../../../etc/passwd` in pathspec, verify error |

### Test cases for `GitStashPopTool`

| Test name | Description |
|-----------|-------------|
| `test_git_stash_pop_basic` | Stash, pop, verify changes restored |
| `test_git_stash_pop_specific_index` | Create 2 stashes, pop index 1 |
| `test_git_stash_pop_empty` | Pop when no stashes exist — verify error |
| `test_git_stash_pop_conflict` | Stash, modify same file, pop — verify conflict detection |

### Test cases for `GitStashApplyTool`

| Test name | Description |
|-----------|-------------|
| `test_git_stash_apply_basic` | Stash, apply, verify changes restored AND stash still exists |
| `test_git_stash_apply_with_index` | Stash with staged changes, apply with `--index`, verify staging restored |

### Test cases for `GitStashListTool`

| Test name | Description |
|-----------|-------------|
| `test_git_stash_list_empty` | List when no stashes — verify `count: 0` |
| `test_git_stash_list_multiple` | Create 3 stashes, list, verify `count: 3` and parse entries |
| `test_git_stash_list_parsing` | Verify structured output fields (index, branch, message) |

### Test cases for `GitStashDropTool`

| Test name | Description |
|-----------|-------------|
| `test_git_stash_drop_basic` | Create 2 stashes, drop index 0, verify only 1 remains |
| `test_git_stash_drop_specific_index` | Create 3 stashes, drop index 1, verify correct one removed |
| `test_git_stash_drop_invalid_index` | Drop a non-existent index — verify error |

### Integration test

| Test name | Description |
|-----------|-------------|
| `test_git_stash_full_workflow` | Full cycle: make changes → stash → verify clean → list → pop → verify restored → commit |

---

## Phase 5: Documentation & Orchestration Integration

### Files to update

1. **`crates/rustycode-orchestration/README.md`** — Add stash tools to the tool availability matrix.

2. **Tool descriptions** (already in each tool's `description()` method) — Ensure descriptions mention:
   - `git_stash`: "Save local modifications to a new stash entry and roll back the working tree."
   - `git_stash_pop`: "Restore the most recent stash entry and remove it from the stash list."
   - `git_stash_apply`: "Restore a stash entry without removing it from the stash list."
   - `git_stash_list`: "List all stash entries."
   - `git_stash_drop`: "Remove a stash entry from the stash list."

3. No changes needed in `rustycode-protocol` — stash operations use the same `ToolOutput` and structured JSON pattern as existing git tools.

---

## File Change Summary

| File | Action | Lines changed (est.) |
|------|--------|---------------------|
| `crates/rustycode-tools/src/providers/git.rs` | Add 5 tool structs + impls + ~20 tests | +600 |
| `crates/rustycode-tools/src/providers/git_provider.rs` | Add imports + register 5 tools | +10 |
| `crates/rustycode-orchestration/README.md` | Add stash tools to docs | +5 |
| **Total** | | **~615** |

---

## Dependency Graph

```
Phase 1 (Tool impls in git.rs)
    ↓
Phase 2 (Register in git_provider.rs)
    ↓
Phase 3 (Security — path validation, part of Phase 1)
    ↓
Phase 4 (Tests in git.rs — can start once Phase 1 compiles)
    ↓
Phase 5 (Docs — independent)
```

Phases 1 + 3 are done together (security validation is inline in the stash tool). Phase 4 can be written in parallel with Phase 2 but needs Phase 1 to compile.

---

## Out of Scope (Future Work)

- `git stash branch <name>` — creates a branch from a stash
- `git stash clear` — drops all stashes (dangerous, add later if needed)
- `git stash show` — diff stat for a stash (could be added to `git_stash_list` output)
- Stash entry inspection (full diff of a stash via `git stash show -p`)

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Merge conflicts on stash pop/apply | Return structured error with conflict details; suggest `git_stash_apply` over `git_stash_pop` |
| Path traversal via pathspec | Reuse existing `validate_read_path` security check |
| Data loss from `stash drop` | Tool description should warn; consider confirmation in future |
| Stash index out of bounds | Validate by checking `stash list` count before drop/apply, or rely on git's error message |
