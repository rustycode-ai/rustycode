# Skill Activation System - Testing Results

## Status: System Exists But Broken

The skill activation system has all the pieces in place, but they're not wired together. Tests reveal 4 critical bugs:

---

## Bug #1: Tool Scope is Empty (CRITICAL)

**Impact:** Activated skills are invisible to LLM providers

```
Step 2: Check if skill is active
  ✓ code-reviewer is active                    ← Skill IS active

Step 4: Check tool scope  
  ⚠️ Tool scope is EMPTY (BUG!)                ← But NOT in tool scope!
```

**Root Cause:** 
- `ActivationManager.activate()` adds to `activation.active_skills` (internal HashMap)
- But `SkillManager.active_tool_scope()` reads from `session_skill_ids` instead
- These are NOT the same — `session_skill_ids` is populated only by `remember_active_skill()` calls in some paths

**Location:** `crates/rustycode-skill/src/manager.rs` line 282-293

```rust
pub fn active_tool_scope(&self) -> Vec<String> {
    let mut combined = Vec::new();
    for skill_id in &self.session_skill_ids {  // ← WRONG: Uses session_skill_ids, not activation.active_skills()
        if let Some(def) = self.registry.get(skill_id) {
            let tools = resolve_allowed_tools(def);
            combined.extend(tools);
        }
    }
    combined
}
```

**Fix:** Use `self.activation.active_skills()` instead of `self.session_skill_ids`

---

## Bug #2: Context Scoring is Inconsistent

**Impact:** Some contexts don't recommend skills

```
Input: "I'm getting an error, can you help debug?"
Recommendations:
  (EMPTY - should recommend 'debugger')
```

**Root Cause:** The `score_skill()` method in `ActivationManager` doesn't match on "debug" keyword in debugger's description. The scoring algorithm is too strict.

**Location:** `crates/rustycode-skill/src/activation.rs` line 232-255

The issue is the context "error" doesn't match "debugging helper" strong enough.

---

## Bug #3: Path-Based Activation Not Working

**Impact:** No skills activate for file types (e.g., `.rs` files)

```
Input: "Rust files" files
  - src/main.rs
  - lib/utils.rs
No skills activated
```

**Root Cause:** The test skills don't have `activation.mode: conditional` or `activation.paths` set. Path-based activation only works if skills are registered with conditional paths. None of the test skills have this.

**Fix:** Test skills need:
```yaml
activation:
  mode: conditional
  paths:
    - "*.rs"
    - "src/**/*.rs"
```

---

## Bug #4: Budget Enforcement Ignored

**Impact:** Skills activate despite exceeding budget

```
Budget: 1,000 tokens (very tight)
Attempting to activate high-effort skill...
  ⚠️ Skill activated despite tight budget
```

**Root Cause:** The `BudgetEnforcer` has an issue with enforcement. Investigation needed.

**Location:** `crates/rustycode-skill/src/budget.rs`

---

## Architecture Issues

### 1. Two Separate Tracking Systems

The system has conflicting state management:
- `ActivationManager.active` — HashMap tracking what's activated
- `SkillManager.session_skill_ids` — Vec tracking sessions
- `BudgetEnforcer` — Another budget tracking system

These don't stay in sync.

### 2. Missing Integration Points

The activation system is never called during actual task execution:
- `Runtime.run()` in `rustycode-core` doesn't call `activate_for_context()`
- No hook into task processing to trigger smart activation
- LLM providers don't know about active skills

### 3. No Conditional Skill Setup

The bundled test skills don't use the `activation.paths` feature, so path-based activation can't be tested.

---

## Test Results Summary

| Test | Result | Status |
|------|--------|--------|
| Context-based activation | Partial (1/3 skills) | ⚠️ BROKEN |
| Path-based activation | 0/1 | ❌ NOT WORKING |
| Tool scope integration | Empty | ❌ CRITICAL BUG |
| Budget enforcement | Not enforced | ❌ BROKEN |

---

## Next Steps

### Phase 1: Fix Core Bugs (Priority 1)
1. [ ] Fix `active_tool_scope()` to use `activation.active_skills()` 
2. [ ] Improve context scoring algorithm
3. [ ] Verify budget enforcement
4. [ ] Add proper test skills with conditional paths

### Phase 2: Integration (Priority 2)
1. [ ] Hook `activate_for_context()` into `Runtime.run()`
2. [ ] Wire active skills to LLM provider setup
3. [ ] Add skill activation events to event bus
4. [ ] Update TUI to show active skills

### Phase 3: End-to-End Testing (Priority 3)
1. [ ] Full integration test with actual task execution
2. [ ] Test with real LLM provider
3. [ ] Verify skills are available as tools during execution

---

## Files to Modify

**Critical:**
- `crates/rustycode-skill/src/manager.rs` — Fix `active_tool_scope()`
- `crates/rustycode-skill/src/activation.rs` — Improve scoring algorithm
- `crates/rustycode-skill/src/budget.rs` — Fix budget enforcement

**Important:**
- `crates/rustycode-core/src/runtime/mod.rs` — Add activation hook
- `crates/rustycode-llm/src/lib.rs` — Accept active skills
- Tests — Add proper test fixtures with conditional paths
