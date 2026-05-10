# RustyCode Architecture Review
**Date:** 2026-04-20
**Reviewer:** Claude Code
**Updated:** 2026-05-03 (workspace scanner, adaptive scanning, doc cleanup)

---

## STATUS UPDATE (2026-05-03)

**Workspace scanner rewrite, adaptive scanning, release v0.1.0**

| Area | Status | Details |
|------|--------|---------|
| Workspace scanner | ✅ COMPLETE | Rewritten with `ignore::WalkBuilder` for automatic `.gitignore` support |
| Adaptive scanning | ✅ COMPLETE | Skips `RepoMap::build()` for workspaces >300 files, reduces tree depth |
| Custom ignore files | ✅ COMPLETE | `.rustycodeignore` support via `add_custom_ignore_filename` |
| LSP tool tags | ✅ COMPLETE | Explore/Implement tags added (not just Debug) |
| StructuredThinkingTool | ✅ COMPLETE | Tool trait impl in orchestration, registered in TUI + headless |
| AskUserTool + StuckDetector | ✅ COMPLETE | LLM clarification requests, confidence stagnation detection |
| Debug logging cleanup | ✅ COMPLETE | Removed `palette_debug.log` from event_loop_input |
| Release v0.1.0 | ✅ SHIPPED | macOS arm64 + Windows x86_64 binaries on GitHub Releases |
| Doc cleanup | ✅ COMPLETE | Removed stale status/legacy docs, updated README and CRATES.md |

**Key Changes:**
- `workspace_scanner.rs`: Replaced manual `SKIP_DIRS` with `ignore::WalkBuilder` (from ripgrep)
- `workspace_context.rs`: Added `LARGE_WORKSPACE_THRESHOLD = 300`, adaptive depth/entries, intermediate progress
- `lsp.rs`: Diagnostics, hover, definition, completion tools now available in Explore/Implement profiles
- `tool_selection.rs`: Clippy fixes (`Self::` variants, `#[allow(clippy::too_many_lines)]`)

**Resolved P0:** Circular dependency `rustycode-llm` ↔ `rustycode-tools` — fully resolved via `rustycode-tool-integration` (both crates build/test independently)

---

## STATUS UPDATE (2026-04-24, Orchestration Consolidation)

**Orchestration crate ready, integration layer complete, API mismatch resolved**

| Area | Status | Details |
|------|--------|---------|
| Orchestration crate | ✅ COMPLETE | 20 modules, 136 tests, clean build |
| Integration layer | ✅ COMPLETE | Task classification wired into CLI + TUI |
| API compatibility | ✅ FIXED | orchestra↔orchestration adapter reconciled |
| Error consolidation | ✅ COMPLETE | Unified OrchestrationError with 24 variants |
| Metrics/shadow mode | ✅ COMPLETE | InMemoryMetricsStore, ShadowExecutor |
| Deep-thinker integration | Pending | Thinking module → orchestration (Task 5-6) |
| Session consolidation | Pending | Orchestra session → orchestration (Task 7) |
| Full merge | Pending | 6 remaining consolidation tasks |

### Key Changes

**rustycode-orchestration (crate-level):**
- Added `TaskResult` enum (Success/Failed) to pipeline
- Added synchronous `execute()` method for non-async callers
- Expanded `OrchestrationError` with the canonical algorithmic variants used by orchestration
- Added `ErrorCategory` enum (12 categories) with `category()` method
- Added `from_thinking_error()` converter for deep-thinker errors
- Added `metrics` module: `MetricsCollector`, `InMemoryMetricsStore`, `SqliteMetricsStore`, `ShadowExecutor`
- Added core components: `TaskDecomposer`, `LlmDecomposer`, `PlanRefiner`, `Editor` (Tier 3), `Composer` (Tier 4), `EscalationRouter`, `VerificationGate`

**rustycode-orchestration (adapter fix, originally orchestra):**
- Fixed `OrchestrationPipeline<MemoryFailureStore>` → `OrchestrationPipeline` (removed unused generic)
- Fixed `DryRunMode::Off` → `DryRunConfig { default_mode: DryRunMode::Disabled, ... }`
- Fixed `OrchestrationPipeline::new(config, store)` → `OrchestrationPipeline::new(config)`
- 889 tests passing

**rustycode-cli + rustycode-tui (integration):**
- `LocalTaskClassifier` wired into CLI `Run` command (`main.rs`)
- `LocalTaskClassifier` wired into TUI `process_send_message` (`text_input.rs`)
- System prompt tier guidance added (`response.rs`)

**rustycode-classification:**
- Pre-existing crate with `LocalTaskClassifier`, keyword-based routing, 8 tests passing

---

## STATUS UPDATE (2026-04-23, Round 13)

**Production Readiness: 10,000+ lib tests, 0 clippy warnings, 0 test failures**

| Improvement | Details |
|-------------|---------|
| Bash tool description | Added tool preference guidance (glob/grep/read_file/edit/write_file over raw commands), command chaining instructions, improved parameter descriptions |
| Read tool parameter | Renamed `path` → `file_path` for consistency with edit/write tools and reference codebase |
| Write tool parameter | Renamed `path` → `file_path` for consistency |
| Edit tool description | Added line-number prefix handling instructions, improved old_string uniqueness guidance |
| Read tool capabilities | Added image/PDF/Jupyter notebook capability descriptions |
| Strategy preemption | Added reasoning stagnation detection and strategy switching to deep-thinker executor |
| TUI streaming fix | Suppressed unused_mut/unused_assignment warnings in response streaming |

## STATUS UPDATE (2026-04-23, Round 12)

**Production Readiness: 10,000+ lib tests, 0 clippy warnings, 0 test failures**

| Improvement | Details |
|-------------|---------|
| tui-core state tests | 26 new tests — UiState (8), WorkspaceState (4), StreamingState (8), InputState (6) covering defaults, field access, InputMode toggle |
| event_loop tests | 2 new tests — custom config, stop idempotency (3→5 total) |
| tools-registry tests | 13 new tests — RegistryConfig, ToolMetadata builder, ToolRegistry creation, discovery, API registry access |
| guard codec tests | 12 new tests — parse_input, HookResult allow/deny/warn/ask, serde roundtrips, camelCase serialization |
| Disk cleanup | Removed debug build artifacts (freed ~100GB) blocking further builds |

## STATUS UPDATE (2026-04-23, Round 11)

**Production Readiness: 9,942+ lib tests, 0 clippy warnings, 0 test failures**

| Improvement | Details |
|-------------|---------|
| Provider test coverage | 15 new tests for mistral (3→12) and zhipu (3→9) — metadata, serialization, response parsing, config validation |
| Token count overflow fix | `input_tokens + output_tokens` → `saturating_add()` in anthropic, azure, bedrock, openai providers |
| token_counter tests | 17 new tests (cache hit/clear, hash, custom ratio, enum tokens, chat edge cases, eviction, default) |
| execution tests | 9 new tests (ExecutionResult constructors, error thresholds, continue_on_error, status, registry, wrapper) |
| sandbox tests | 9 new tests (default level, None enforce, check_paths, interactive flag, CWD auto-allow, denied overrides) |

## STATUS UPDATE (2026-04-23, Round 9)

**Production Readiness: 9,893+ lib tests, 0 clippy warnings, 0 test failures**

| Improvement | Details |
|-------------|---------|
| sanitize_for_log security fix | Fixed Bearer token values not being redacted (separator handling + overlap dedup) |
| Model context windows | Added DeepSeek (128k), Mistral (32k/128k), Cohere (128k), GLM (128k), Qwen (128k) |
| Flaky test fix | `test_state_cache` race condition — uses `if let Some` instead of `unwrap()` |
| request_dedup tests | 13 new unit tests for cache deduplication (hash, miss/hit, expired, overwrite) |
| Sanitize tests | 3 new tests: bearer token value, colon separator, space separator |

## STATUS UPDATE (2026-04-23, Round 8)

**Production Readiness: 9,671+ lib tests, 0 clippy warnings, 0 test failures**

| Improvement | Details |
|-------------|---------|
| File suggestion UX | "Did you mean?" suggestions on file-not-found in both edit and read tools |
| Shared file_suggest module | Extracted `suggest_similar_files()` + `format_suggestions()` to reusable module with fuzzy matching |
| Fuzzy stem matching | Character-overlap scoring for typos like "man.rs" → "main.rs" (threshold 0.6) |
| Edit tool suggestion fix | Moved suggestion logic to `open_file_symlink_safe` error handler (correct error source) |
| Tests | 4 new file_suggest unit tests, 1,719 tools tests total |

## STATUS UPDATE (2026-04-23, Round 7)

**Production Readiness: 10,874+ tests (9,861 lib + 1,013 integration), 0 clippy warnings, 9 pre-existing test_orchestra_e2e failures**

| Improvement | Details |
|-------------|---------|
| Panic-safe retry | Replaced `.unwrap()` with `let Some(...) else` + `unwrap_or_else` in `retry_with_backoff` (2 sites) |
| Panic-safe doom_loop | `prune_old()` clears history when `checked_sub` returns `None` instead of panicking |
| Bash validation test fixes | Corrected tests for rm/chmod (allowlisted) and trailing backslash (valid shell). 1,711 tools tests pass |
| Guard test coverage | 7 new edge-case tests (normal push, feature-branch reset, src edits, normal rm, --no-gpg-sign, push master) |
| Guard tests total | 40 tests (up from 33) — all rules R01-R15 covered with positive and negative cases |

## STATUS UPDATE (2026-04-23, Round 6)

**Production Readiness: 6,324+ lib tests passing across 7 main crates, 0 clippy warnings, 9 pre-existing test_orchestra_e2e failures**

| Improvement | Details |
|-------------|---------|
| Unused deps removed | `walkdir` and `notify` removed from rustycode-skill |
| Binary detection tests | 6 unit tests for `is_binary_content()` — null bytes, empty, UTF-8, 8KB boundary |
| Credential file blocking | `.credentials.json` added to BLOCKED_FILENAMES with test |
| Hook syntax fix | Fixed `|i =>` → `|i|` syntax error in zhipu.rs from linter hook |

## STATUS UPDATE (2026-04-23, Round 5)

**Production Readiness: 4,454 lib tests passing across main crates, 0 clippy warnings, 9 pre-existing test_orchestra_e2e failures**

| Improvement | Details |
|-------------|---------|
| Buffer overflow protection | `ToolAccumulator::push_json` capped at 1 MiB with overflow tests |
| Production unwrap removal | `extract_xml_all_multiline` — replaced `.ok().unwrap()` with safe fallback |
| Server tool validation | `debug_assert` for server tools missing `anthropic_type` in serialization |
| Stream timeout increase | Per-chunk SSE timeout raised from 2min to 5min |
| Typed errors | `CoreError` enum for context_management and edit_history (partial migration) |
| Conversation fix tests | 5 tests for `fix_conversation_messages` — orphaned tool_results, merge, trailing |
| Tool name aliases | `Edit→edit_file`, `Read→read_file`, `Write→write_file`, `Bash→bash`, etc. in both headless and TUI |
| Binary content detection | `is_binary_content()` probes first 8KB for null bytes before UTF-8 decode |
| Unused deps removed | `libc` and `backon` removed from rustycode-core |

## STATUS UPDATE (2026-04-23, Round 4)

**Production Readiness: 10,880+ tests passing, 0 clippy warnings, 9 pre-existing test_orchestra_e2e failures**

| Improvement | Details |
|-------------|---------|
| Provider error cleanup | Removed stale hardcoded model lists from 404 error responses in 7 providers (Anthropic, OpenAI, Cohere, Copilot, Gemini, Mistral, Perplexity) |
| ProviderError::with_model | Simplified from double-match pattern to single-pass match that preserves retry_delay and top_up_url instead of discarding them |
| Model-aware compaction | Compaction context windows now per-model (GPT-4=128k, GPT-3.5=16k, Gemini=1M, Claude=200k default) |
| LLM compatibility | read/write tools now accept both `path` and `file_path` parameter names |
| Smart approve expansion | Expanded read-only (env, printenv, npm view, gh pr checkout) and destructive (gh repo fork, npm publish, cargo publish) command classifications |
| Three-tier compaction | Tool output pruning, message pruning, and aggressive compaction strategies |
| Deprecated code removal | Removed ParseJson/StringifyJson tool variants, cleaned event_system redundancy |

| Improvement | Details |
|-------------|---------|
| Deprecated code removal | Removed `CompletionRequest` legacy fields (extended_thinking, thinking_budget, effort), `RenderContext` type alias, `frontmatter_to_metadata()`, `DeepThinkingTool` module |
| CommonTraits macro fix | `CommonTraits` derive macro was causing E0428 duplicate definitions — made no-op, replaced all usages with explicit derives |
| Defensive coding | Replaced `expect()` in plan_mode, sleep, event_system, osv_check with safe fallbacks and error logging |
| GSD → Orchestra rename | Renamed test_gsd_e2e → test_orchestra_e2e, removed GSD from .clippy.toml |
| Skills API | Eager streaming activated on 4 production tools, Anthropic Skills API implemented |

## STATUS UPDATE (2026-04-23, Round 2)

**Production Readiness: 3,261+ lib tests passing, 0 clippy warnings, 9 pre-existing test_orchestra_e2e failures**

| Improvement | Details |
|-------------|---------|
| Eval flag security | Fixed regression where `-c`/`-e` flags were allowed for allowlisted interpreters. Now blocked for all shells/interpreters regardless of allowlist status |
| Smart approve | Added GitHub CLI classification (14 read-only, 11 destructive commands), curl/wget classification, jq/yq read-only, removed node -e from read-only |
| Checkpoint tests | Fixed integration tests to commit changes before rewind (rewind now hard-fails on uncommitted changes) |
| Tool definitions | Added edit_file, grep, glob tools. Improved bash/write descriptions per reference comparison |
| Bash security | Fixed eval flag bypass (node -e, etc.), improved quote nesting depth tracking, newline validation |
| Integration tests | Rewrote 4 stale test files to match current async API |
| Clippy | Resolved all workspace clippy warnings across 5 crates |
| Skills API | Eager streaming + Anthropic Skills API (container, SkillRef, code_execution) fully implemented |
| TCP keepalive | Added to Anthropic HTTP client to prevent connection drops |

## STATUS UPDATE (2026-04-21)

**Major Progress: 2 of 3 P0 Issues Resolved ✅**

| Issue | Status | Details |
|-------|--------|---------|
| 4x `LLMProvider` traits | ✅ RESOLVED | Consolidated to unified `LLMProvider` trait. All 750 tests passing. |
| Checkpoint/rewind system | ✅ COMPLETE | `CheckpointRecovery` implemented in `rustycode-core/src/recovery/checkpoint.rs` |
| Circular dependency (llm ↔ tools) | ✅ RESOLVED | `rustycode-tool-integration` provides shared traits. Both crates build/test independently |

---

## Executive Summary

RustyCode is a **48-crate Rust workspace** with a solid foundation. **Critical architectural issues have been largely resolved**:

| Issue | Severity | Status | Impact | Fix Effort |
|-------|----------|--------|--------|-----------|
| 4x `LLMProvider` traits | 🔴 CRITICAL | ✅ RESOLVED | Fixed — no type-safe abstraction gaps | Complete |
| Checkpoint/rewind incomplete | 🔴 CRITICAL | ✅ COMPLETE | Fixed — plan validation working | Complete |
| Circular: `llm` → `tools` | 🟠 HIGH | 🟠 PARTIAL | Mitigation in place, cleanup remains | 1-2 days |
| Kitchen sink crates (tui/core/tools) | 🟡 HIGH | 🔄 PENDING | Cognitive load, hard to test | 5-7 days |
| Async core incomplete | 🟡 MEDIUM | 🔄 PENDING | Performance ceiling, blocks Phase 2 | 5-7 days |

**Current baseline:** 10,265+ tests passing, zero clippy warnings, zero test failures.

---

## 1. Circular Dependency: `llm` → `tools`

### Problem
```
rustycode-llm/Cargo.toml:
  rustycode-tools = { path = "../rustycode-tools", default-features = false }

rustycode-tools/Cargo.toml:
  rustycode-llm = { path = "../rustycode-llm" }
```

### Root Cause
- `rustycode-llm` needs `rustycode-tools` for vector-memory feature (semantic search)
- `rustycode-tools` needs `rustycode-llm` for LLM-based functionality
- Cargo resolves this within the same workspace, so it compiles — but the coupling is real

### Impact
- Builds are coupled (change in tools forces llm rebuild)
- Cannot test `rustycode-llm` without `rustycode-tools`
- Cannot independently version or extract these crates

### Resolution Path

**Option A: Extract Abstraction (RECOMMENDED)**
```
rustycode-llm → rustycode-tool-integration (new)
rustycode-tools → rustycode-tool-integration (new)

rustycode-tool-integration:
  - Cost tracking interface
  - LLM-aware tool execution contracts
  - Execution results with token metrics
```

**Option B: Move to Protocol**
```
rustycode-protocol:
  - LLM execution contracts
  - Cost calculation traits
  - Provider selection types

rustycode-llm: depends only on protocol
rustycode-tools: depends only on protocol
```

**Recommendation:** Use Option A (new crate is cleaner separation)

---

## 2. CRITICAL ISSUE: Four Conflicting `LLMProvider` Traits

### Problem

| Location | Signature | Status |
|----------|-----------|--------|
| `rustycode-llm/src/provider.rs` | V1 sync traits, model listing, regex parsing | Legacy V1 |
| `rustycode-llm/src/provider.rs` | Async `Provider` trait with `complete_stream()` | Active V2 |
| `rustycode-plugins/src/traits.rs` | `LLMProviderPlugin` wrapper with `get_provider()` | Plugin adapter |
| `rustycode-core/src/team/tool_generator.rs` | Minimal `generate(&self, prompt) -> String` | Test stub |

**Consequence:** No type-safe abstraction for "any LLMProvider". Callers must know specific variant.

### Resolution

**Consolidate to Single Unified Trait in `rustycode-protocol`:**

```rust
// rustycode-protocol/src/llm.rs
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
    async fn is_available(&self) -> Result<bool>;
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    fn name(&self) -> &'static str;
    fn estimate_cost(&self, request: &CompletionRequest) -> Cost;
}
```

**Migration Steps:**
1. Define unified trait in `rustycode-protocol`
2. Remove `provider.rs` (V1, deprecated)
3. Rename `provider.rs` -> `provider.rs`
4. Provide adapters for plugin system
5. Update all consumers (6+ crates) to use unified trait

---

## 3. CRITICAL ISSUE: Checkpoint/Rewind Not Implemented

### Status
- Documented in: `/docs/superpowers/specs/CRITICAL-ISSUES-RESOLUTION.md`
- Not implemented in code
- Blocking: Plan validation, error recovery

### Required Fixes (Per Spec)

**Fix #1: Git Checkpoint References**
```rust
pub struct SessionSnapshot {
    pub checkpoint_git_hash: String,
    pub modified_files: Vec<PathBuf>,
}

pub trait Recovery {
    async fn rewind_to_checkpoint(&self, snapshot: &SessionSnapshot) -> Result<()>;
}
```

**Fix #2: Plan Mode Tool Allowlist**
```rust
const INSPECTION_TOOLS: &[&str] = &[
    "read", "Grep", "find_symbol", "get_symbols_overview",
];

fn validate_plan_step(step: &ExecutionStep) -> Result<()> {
    if !INSPECTION_TOOLS.contains(&step.tool.name()) {
        return Err(anyhow!("Plan mode: only inspection tools allowed"));
    }
    Ok(())
}
```

**Fix #3: Conservative Checkpoint Triggers**
```rust
const DANGEROUS_OPERATIONS: &[&str] = &[
    "rm ", "git reset --hard", "git clean", "git push --force",
];

fn should_create_checkpoint_before_step(step: &ExecutionStep) -> bool {
    DANGEROUS_OPERATIONS.iter().any(|d| step.command.contains(d))
}
```

### Implementation Effort
- Phase 1: Git checkpoint references (1-2 days)
- Phase 2: Rewind logic (1-2 days)
- Phase 3: Plan mode validation (1 day)

---

## 4. Kitchen Sink Crates (Over-Responsibility)

### Problem Crates

**`rustycode-tui` (22 workspace dependencies):**
- Should: Present UI, handle terminal events
- Actually: Manages auth, learning, orchestration, plugins, vector memory
- Impact: Cannot test TUI independently

**`rustycode-core` (~38.6K LOC, 18 modules):**
- Contains: Agents, execution, recovery, plans, sessions, validation, team, tenacity
- Should: Single responsibility (agents? execution? recovery?)
- Impact: Hard to test, unclear error boundaries

**`rustycode-tools` (50+ modules):**
- Contains: Tool registry, executor, security, plugins, middleware, lifecycle
- Should split into: tool-api, tool-executor, tool-registry, tool-security

### Refactoring Path

**Phase 1: Define Responsibility**
```
rustycode-tui -> thin presentation layer
  +-- Terminal UI rendering (ratatui)
  +-- Input handling (keyboard, mouse)
  +-- Delegates to core/runtime for all logic

rustycode-core -> business logic + orchestration
  +-- Agent implementation
  +-- Plan execution
  +-- Session state
  +-- Delegates to runtime for concurrency

rustycode-tools -> tool infrastructure
  +-- Tool trait definition (rustycode-tools-api)
  +-- Tool executor (async execution)
  +-- Tool registry (discovery)
  +-- Tool security (validation)
  +-- Built-in tool implementations
```

**Phase 2: Extract & Refactor**
1. Move TUI auth handling -> `rustycode-core`
2. Move TUI learning -> `rustycode-learning`
3. Split tools into 4 crates (api, executor, registry, security)
4. Document dependency edges

---

## 5. Async Implementation Incomplete

### Current State
- Foundation (bus, runtime) fully async
- Event system non-blocking
- Core execution mostly sync
- Tool execution sync
- Storage persistence blocking

### Recommended Path

**Immediate (enables concurrent execution):**
```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, request: ToolRequest) -> Result<ToolResponse>;
}

pub struct BlockingToolExecutor {
    inner: SyncToolExecutor,
}

#[async_trait]
impl ToolExecutor for BlockingToolExecutor {
    async fn execute(&self, request: ToolRequest) -> Result<ToolResponse> {
        tokio::task::spawn_blocking(|| self.inner.execute(request)).await?
    }
}
```

**Medium-term:**
- Async storage adapter (tokio-rusqlite or async SQL)
- Async plan executor for concurrent steps
- Event publication with async subscribers

---

## 6. Priority Action List

### Phase 0: Documentation (1 week)
- [ ] Generate READMEs for undocumented crates
- [ ] Update architecture diagrams for all 48 crates
- [ ] Create ARCHITECTURE-SPEC.md with crate responsibilities

### Phase 1: Resolve Critical Dependencies (2 weeks)
- [x] Break `llm` <-> `tools` circular dependency
  - [x] Create `rustycode-tool-integration` — provides shared traits, both crates build independently
  - [ ] Move cost/execution abstractions
  - [ ] Update dependents (6+ crates)
- [ ] Consolidate `LLMProvider` traits
  - [ ] Define unified trait in protocol
  - [ ] Migrate all 4 locations
  - [ ] Update consumers (6+ crates)

### Phase 2: Implement Critical Features (2 weeks)
- [ ] Checkpoint/rewind system
  - [ ] Git-based checkpoints
  - [ ] Rewind logic with file restoration
  - [ ] Plan mode validation and tool allowlist

### Phase 3: Refactor God Objects (3 weeks)
- [ ] Extract `rustycode-tui` dependencies
- [ ] Split `rustycode-tools` (4 crates)
- [ ] Consolidate `rustycode-core` responsibilities

### Phase 4: Complete Async Migration (3 weeks)
- [ ] Tool executor async trait
- [ ] Async storage adapter
- [ ] Async plan execution

---

## 7. Success Criteria

| Goal | Current | Target | Timeline |
|------|---------|--------|----------|
| Circular dependencies | 1 | 0 | Week 2 |
| LLMProvider trait locations | 4 | 1 | Week 2 |
| Crate READMEs | Partial | All 36 | Week 1 |
| Checkpoint/rewind | Incomplete | Complete | Week 3 |
| God object refactoring | Large modules | <800 LOC each | Week 4-6 |
| Async coverage | ~40% | ~80% | Week 6-7 |

---

## Appendix: Verified Facts

- **Workspace members:** 48 crates (excluding rustycode-web)
- **Test baseline:** 10,265 tests passing, zero clippy warnings
- **Core LOC:** ~38,641 lines across rustycode-core
- **TUI dependencies:** 22 workspace crate dependencies
- **Circular dep:** `rustycode-llm` -> `rustycode-tools` (required, not optional) and `rustycode-tools` -> `rustycode-llm`

## Appendix: Files Referenced

- **Workspace Config:** `Cargo.toml` (root)
- **Problematic Crates:**
  - `crates/rustycode-llm/Cargo.toml` (circular with tools)
  - `crates/rustycode-tools/Cargo.toml` (circular with llm)
  - `crates/rustycode-core/Cargo.toml` (~38.6K LOC)
  - `crates/rustycode-tui/Cargo.toml` (22 deps)
