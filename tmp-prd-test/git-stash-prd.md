# PRD: Git Stash Support for RustyCode

**Document ID**: RC-PRD-2026-0042  
**Date**: 2026-05-05  
**Status**: Draft  
**Target Release**: v0.14.0

---

## 1. Overview

RustyCode currently supports `git_status`, `git_diff`, and `git_log` as first-class tools in the **Ops** tool profile (see `crates/rustycode-tools-api/src/tool_selection.rs`, `ToolProfile::OPS`). However, there is no support for `git stash` — the ability to temporarily shelve uncommitted changes and restore them later.

This is a critical workflow gap. When RustyCode's autonomous agent needs to switch branches or test a theory, it must either commit half-finished work (polluting history) or discard it (losing progress). Git stash is the canonical solution.

### Goals

1. Expose `git stash` operations as typed Rust tools in `crates/rustycode-tools`.
2. Integrate stash tools into the **Ops** tool profile alongside existing git tools.
3. Provide the LLM with enough context to use stashes safely during autonomous operation.

### Non-Goals

- Not building a general-purpose git porcelain library — only stash operations.
- Not replacing the `bash` tool for advanced git operations (rebase, bisect, etc.).
- Not implementing git worktrees at this time.

---

## 2. Background & Codebase Context

### Current Git Tool Surface

From `crates/rustycode-tools-api/src/tool_selection.rs`:

```rust
const OPS: &[&str] = &[
    "bash",
    "read_file",
    "list_dir",
    "grep",
    "glob",
    "git_status",   // exists
    "git_diff",     // exists
    "git_log",      // exists
    // git_stash_*   MISSING
];
```

### Tool Infrastructure

- **Trait**: `Tool` in `crates/rustycode-tools/src/lib.rs` with `name()`, `description()`, `permission()`, `parameters_schema()`, `execute()`.
- **Tags**: `ToolTag::Ops` in `crates/rustycode-tools-api/src/lib.rs` for git/bash/deployment tools.
- **Permission model**: `ToolPermission` enum (`Read`, `Write`, `Execute`) in `crates/rustycode-tools-api/src/lib.rs`.
- **Security**: Command validation in security module must clear stash-related git invocations.

### Patterns to Follow

Existing tool implementations (e.g., `GetTool` in `crates/rustycode-tools/src/api.rs`):

```rust
pub struct GetTool;
impl Tool for GetTool {
    fn name(&self) -> &'static str { "http_get" }
    fn description(&self) -> &'static str { "Execute HTTP GET requests" }
    fn permission(&self) -> ToolPermission { ToolPermission::Read }
    fn parameters_schema(&self) -> Value { json!({...}) }
    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("OK"))
    }
}
```

---

## 3. User Stories

### US-1: Stash Current Changes

| Field | Value |
|-------|-------|
| **ID** | US-1 |
| **Priority** | **P0 — Critical** |
| **As a** | developer using RustyCode (or the autonomous agent acting on my behalf) |
| **I want** | to stash my current uncommitted changes with an optional message |
| **So that** | I can cleanly switch context (branch, experiment, hotfix) without committing half-finished work |

**Acceptance Criteria**:

- [ ] `git_stash_push` tool is registered in the tool registry
- [ ] Tool accepts optional `message` parameter (string, maps to `git stash push -m "<msg>"`)
- [ ] Tool accepts optional `pathspec` parameter (array of strings, maps to `git stash push -- <paths>`)
- [ ] Tool accepts optional `include_untracked` boolean (maps to `--include-untracked`, default `false`)
- [ ] Tool accepts optional `keep_index` boolean (maps to `--keep-index`, default `false`)
- [ ] Returns structured `ToolOutput` with stash ref (e.g., `stash@{0}`), branch name, and file summary
- [ ] Returns clear error if working directory is already clean ("nothing to stash")
- [ ] Security validation passes the `git stash push` command through the security module
- [ ] Tool is tagged `ToolTag::Ops`
- [ ] Tool permission is `ToolPermission::Write` (mutates working tree)

---

### US-2: Restore Stashed Changes

| Field | Value |
|-------|-------|
| **ID** | US-2 |
| **Priority** | **P0 — Critical** |
| **As a** | developer |
| **I want** | to pop or apply a stash to restore shelved changes |
| **So that** | I can resume work after switching context |

**Acceptance Criteria**:

- [ ] `git_stash_pop` tool removes stash entry after applying
- [ ] `git_stash_apply` tool keeps stash entry after applying
- [ ] Both accept optional `index` parameter (integer, maps to `stash@{N}`, default `0`)
- [ ] Returns structured output with applied files summary
- [ ] Returns clear error on merge conflict, including conflicting file paths
- [ ] Returns clear error if specified stash index doesn't exist
- [ ] Both tagged `ToolTag::Ops`, permission `ToolPermission::Write`

---

### US-3: List Stashes

| Field | Value |
|-------|-------|
| **ID** | US-3 |
| **Priority** | **P1 — High** |
| **As a** | developer |
| **I want** | to see all my stashes in a structured format |
| **So that** | I can decide which stash to restore or drop |

**Acceptance Criteria**:

- [ ] `git_stash_list` tool registered
- [ ] Returns array of stash entries, each with: index, message, branch, date
- [ ] Returns empty array (not error) when no stashes exist
- [ ] Tagged `ToolTag::Ops`, permission `ToolPermission::Read`
- [ ] Output is machine-parseable JSON for LLM consumption

---

## 4. Architecture & Implementation Plan

### 4.1 New Module

Create `crates/rustycode-tools/src/git_stash.rs` — all stash tool structs and shared helpers.

### 4.2 Tool Inventory

| Tool Name | Rust Struct | Permission | Tag | Maps To |
|-----------|-------------|-----------|-----|---------|
| `git_stash_push` | `GitStashPushTool` | `Write` | `Ops` | `git stash push` |
| `git_stash_pop` | `GitStashPopTool` | `Write` | `Ops` | `git stash pop` |
| `git_stash_apply` | `GitStashApplyTool` | `Write` | `Ops` | `git stash apply` |
| `git_stash_list` | `GitStashListTool` | `Read` | `Ops` | `git stash list` |

### 4.3 Shared Infrastructure

A private `run_git_stash()` helper in `git_stash.rs` that:

1. Validates the git command via security module command validation
2. Spawns `git stash <subcommand>` via `tokio::process::Command`
3. Captures stdout, stderr, and exit code
4. Parses output into structured `ToolOutput`
5. Adds contextual error messages via `anyhow::Context`

### 4.4 Registration Points

| File | Change |
|------|--------|
| `crates/rustycode-tools/src/lib.rs` | `pub mod git_stash;` + register 4 tools |
| `crates/rustycode-tools-api/src/tool_selection.rs` | Add stash tools to `OPS` and `ALL_TOOLS` const arrays |
| `crates/rustycode-tools-api/src/tiers.rs` | Add stash tools to appropriate tool sets |

### 4.5 Structured Output Format

All stash tools return JSON-structured `ToolOutput`:

```json
{
  "tool": "git_stash_push",
  "result": {
    "stash_ref": "stash@{0}",
    "branch": "feature/my-branch",
    "message": "WIP: refactor auth module",
    "files_changed": 5,
    "files": [
      {"path": "src/auth.rs", "status": "modified"},
      {"path": "src/auth_test.rs", "status": "new file"}
    ]
  }
}
```

---

## 5. Security Considerations

1. **Command injection**: Sanitize all user-provided parameters (message, pathspec) before passing to `git`.
2. **Path traversal**: Pathspec arguments validated against workspace root via existing path validation.
3. **Destructive operations**: Set `destructive_hint: true` in `ToolAnnotations` for destructive stash ops.
4. **No secrets in stash messages**: Validate stash messages against secret-scanning rules (see `commit_msg.rs`).

---

## 6. Testing Plan

| Test | Description |
|------|-------------|
| `test_stash_push_clean_dir` | Stashing a clean directory returns "nothing to stash" error |
| `test_stash_push_with_message` | Stash with `-m` includes message in output |
| `test_stash_push_include_untracked` | `--include-untracked` flag respected |
| `test_stash_push_pathspec` | Stashing specific files only |
| `test_stash_pop_success` | Pop applies and removes stash |
| `test_stash_pop_conflict` | Pop with conflict returns structured conflict info |
| `test_stash_list_empty` | Empty stash list returns empty array |
| `test_stash_list_multiple` | Multiple stashes listed correctly |

---

## 7. Priority & Phasing

| Phase | Stories | Tools | Est. Effort |
|-------|---------|-------|-------------|
| **Phase 1** (P0) | US-1, US-2 | `push`, `pop`, `apply` | 3 days |
| **Phase 2** (P1) | US-3 | `list` | 1 day |

**Total estimated effort**: 4 engineering days

---

## 8. Open Questions

| # | Question | Status |
|---|----------|--------|
| 1 | Should stash tools use separate structs (matching `api.rs` pattern) or a shared base? Resolved — separate structs. | Resolved |
| 2 | Should `git_stash_push` auto-detect and warn about large binary files? | Open |
| 3 | Should the autonomous agent auto-stash before branch switches? | Open |

---

## 9. Success Metrics

- All acceptance criteria pass
- `cargo clippy --workspace --all-targets -- -D warnings` passes with new code
- `cargo test --workspace` passes with new tests
- No regression in existing git tool behavior (`git_status`, `git_diff`, `git_log`)
- LLM successfully uses stash tools in autonomous mode during manual testing

---

## 10. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-05-05 | 0.1.0 | Initial draft |
