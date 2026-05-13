# Plan: Unify Execution Paths for Benchmark Consistency

## Status: Not Started

**Last updated**: 2026-05-13

| Phase | Description | Status | Notes |
|-------|-------------|--------|-------|
| 0 | Naming Cleanup (Zero Risk) | ✅ Done | `execute_legacy_streaming` → `execute_agent_streaming`, `stream_llm_response` → `run_agent_session_stream` |
| 1 | Extract CodeAgent Heuristics as Plugins | ❌ Not started | `plugins/` dir doesn't exist yet |
| 2 | Replace CodeAgent with AgentSession | ❌ Not started | `code_agent.rs` still 1319 lines |
| 3 | TUI Bench Agent | ❌ Not started | Blocked on Phase 2 |
| 4 | Cleanup | ❌ Not started | Blocked on Phase 3 |

### Related work completed (not part of this plan)
- `d4940dd20` feat(indexing): reclassify Function → Method inside impl/class/trait
- `8e4be6afd` fix: add impl_item query extraction, harden LRU cache and edit history
- `148f2a0b1` feat(tools): fix Function vs Method classification in tree-sitter extraction
  - Added `reclassify_methods()` post-pass in `query_extractor.rs`
  - Expanded `Lang` enum: Java, Cpp, Scala support
  - Extracted `parse_with_treesitter()` into `parser.rs`
  - Tested on 480 files across 4 real projects (rustycode, candle, ag2, A2UI)

---

## Context

The user wants benchmarks to measure the actual TUI code path, not a separate implementation. Investigation revealed the gap is **narrower than expected**: the TUI already uses `AgentSession` (misleadingly named `execute_legacy_streaming`). The only true duplicate is `CodeAgent` in the bench crate — a 950-line hand-rolled loop that duplicates what `AgentSession` does, plus adds benchmark-specific heuristics.

**Problem**: CodeAgent doesn't use AgentSession, so benchmarks measure a different code path than what TUI users experience. Naming is misleading (`execute_legacy_streaming` is not legacy).

**Outcome**: All paths (TUI, headless, bench) share AgentSession. Benchmarks can measure the TUI code path. CodeAgent's useful heuristics become reusable plugins.

---

## Phase 0: Naming Cleanup (Zero Risk)

Rename misleading names so the architecture is self-documenting.

**Files:**
- `crates/rustycode-tui/src/app/service_integration.rs`
- `crates/rustycode-tui/src/app/streaming/response.rs`

**Changes:**
1. Rename `execute_legacy_streaming` → `execute_agent_streaming` (line 564)
2. Update comment at line 549 to say "agent streaming" not "legacy streaming"
3. Collapse `stream_llm_response` (line 636) — it's a one-line wrapper. Inline it or rename to `run_agent_session_stream`.

**Verify:** `cargo test -p rustycode-tui` passes. TUI streaming unchanged.

---

## Phase 1: Extract CodeAgent Heuristics as AgentSession Plugins (Low Risk)

Make CodeAgent's useful heuristics available as pluggable components.

**New files:**
- `crates/rustycode-agent-runtime/src/plugins/mod.rs`
- `crates/rustycode-agent-runtime/src/plugins/repetition.rs`
- `crates/rustycode-agent-runtime/src/plugins/early_stop.rs`
- `crates/rustycode-agent-runtime/src/plugins/trace.rs`

**Changes:**
1. Define `AgentPlugin` trait:
   ```rust
   #[async_trait]
   pub trait AgentPlugin: Send {
       async fn on_tool_result(&mut self, tool_name: &str, input: &Value, output: &mut String) {}
       async fn should_stop(&mut self, ctx: &TurnContext) -> bool { false }
   }
   ```
2. Extract `RepetitionDetector` from `code_agent.rs:832-878` — tracks recent bash commands and tool fingerprints, appends warnings
3. Extract `EarlyStopPolicy` from `code_agent.rs:806-939` — detects thrashing (turns since last edit, same-file count, error streaks)
4. Extract `ConversationTrace` from `code_agent.rs:704-748,882-887` — writes markdown trace incrementally
5. Add `plugins: Vec<Box<dyn AgentPlugin>>` to `AgentSession` with builder method `with_plugin()`
6. In `session.rs` `run_loop()`, invoke plugins after tool execution and after each turn

**Verify:** Unit tests for each plugin. `cargo test -p rustycode-agent-runtime` passes. Existing behavior unchanged with no plugins configured.

---

## Phase 2: Replace CodeAgent with AgentSession (Medium Risk)

**File:** `crates/rustycode-bench/src/agent/code_agent.rs`

**Changes:**
1. Rewrite `CodeAgent::run()` to create an `AgentSession` with bench plugins enabled (RepetitionDetector + EarlyStopPolicy + ConversationTrace)
2. Use `BenchObserver` (from `real_agent.rs`) as the `AgentEvents` implementation for metrics
3. Keep CodeAgent's prompt framing (intent classification, turns budget) in the system prompt construction
4. Delete the 950-line hand-rolled loop
5. Remove the `real-agent` feature gate distinction — both CodeAgent and RealBenchAgent now use AgentSession with different plugin configs

**Verify:** Run benchmark on 3+ tasks comparing old vs new CodeAgent. Token counts, turn counts, and outcomes should be comparable. Conversation trace output should contain equivalent info.

---

## Phase 3: TUI Bench Agent (Low Risk)

Enable benchmarks to measure the exact TUI code path.

**New file:** `crates/rustycode-bench/src/agent/tui_agent.rs`

**Changes:**
1. Create `TuiBenchAgent` that configures AgentSession identically to `stream_llm_response_agent`:
   - Same system prompt construction (workspace context, project instructions, memory)
   - Same `AgentConfig::from_env()` defaults
   - Same `ToolTier::Full` activation
   - Same tool registry construction
   - Streaming mode enabled (matches TUI)
2. Register as `"tui"` agent in `AgentRegistry`
3. Add TUI-relevant metrics: time to first token, inter-turn latency, token throughput

**Verify:** `cargo run -p rustycode-bench -- --agent tui --model <model> --task <task>` produces benchmark results.

---

## Phase 4: Cleanup

1. Remove dead `session_agent.rs` in bench crate (not declared in `mod.rs`)
2. Unify provider-creation utility (currently duplicated across CodeAgent, RealBenchAgent, and TUI)
3. Update doc comments to state AgentSession is the shared loop
4. Verify `grep -rn "execute_legacy\|stream_llm_response[^_]" crates/` returns zero results

---

## Dependency Graph

```
Phase 0 (renames) ← independent, do first
    ↓
Phase 1 (plugins) ← additive, no behavior change
    ↓
Phase 2 (replace CodeAgent) ← depends on plugins
    ↓
Phase 3 (TUI bench agent) ← depends on Phase 2 for unified base
    ↓
Phase 4 (cleanup) ← last
```

## Critical Files

| File | Role |
|------|------|
| `crates/rustycode-agent-runtime/src/session.rs` | Shared AgentSession loop — needs plugin hooks |
| `crates/rustycode-bench/src/agent/code_agent.rs` | 1319-line duplicate — must replace |
| `crates/rustycode-bench/src/agent/real_agent.rs` | Pattern to follow (already uses AgentSession) |
| `crates/rustycode-tui/src/app/streaming/response.rs` | TUI's AgentSession integration (reference) |
| `crates/rustycode-tui/src/app/streaming/adapter.rs` | StreamEvent→StreamChunk translation |
| `crates/rustycode-tui/src/app/service_integration.rs` | Misleading naming to fix |
