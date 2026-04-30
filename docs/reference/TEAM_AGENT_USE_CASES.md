# Ensemble Agent Swarm Use Cases

## Overview

The ensemble agent system orchestrates multiple specialized agents (Architect, Builder, Skeptic, Judge, Scalpel, Coordinator) to complete software engineering tasks. This document defines use cases and validates when the swarm adds value vs. single-agent execution.

---

## Use Case Matrix

| Use Case | Risk | Architect | Skeptic | Judge | Scalpel | Value Add |
|----------|------|-----------|---------|-------|---------|-----------|
| 1. Complex Feature | High | ✅ | ✅ | ✅ | ⚠️ | HIGH |
| 2. Security Fix | Critical | ✅ | ✅ | ✅ | ✅ | HIGH |
| 3. Refactoring | High | ✅ | ✅ | ✅ | ⚠️ | HIGH |
| 4. Bug Investigation | Moderate | ⚠️ | ✅ | ✅ | ⚠️ | MEDIUM |
| 5. Compile Error Fix | Low | ❌ | ❌ | ✅ | ✅ | HIGH |
| 6. Quick Typo/Doc | Low | ❌ | ❌ | ❌ | ❌ | LOW |
| 7. Test Addition | Moderate | ❌ | ✅ | ✅ | ❌ | MEDIUM |
| 8. Cross-Module Change | High | ✅ | ✅ | ✅ | ⚠️ | HIGH |

Legend: ✅ Always | ⚠️ Conditional | ❌ Skipped

---

## Use Case Details

### 1. Complex Feature Implementation

**Trigger**: Task requires new modules, interfaces, or dependencies

**Example**: "Add JWT authentication to the API"

**Agent Flow**:
```
TaskProfiler → Risk: High
    ↓
Architect → Declares: auth module, JWT dependency, traits
    ↓
Builder → Implements auth module
    ↓
Skeptic → Verifies: claims match code, deps declared
    ↓
Judge → cargo check + cargo test
    ↓
Coordinator → Track trust, detect doom loop
```

**Why Swarm Wins**:
- Architect prevents scope creep with upfront declaration
- Skeptic catches hallucinated APIs before tests run
- Judge provides empirical verification
- Single agent would likely miss edge cases

---

### 2. Security-Fix Implementation

**Trigger**: Security vulnerability, auth bug, data integrity issue

**Example**: "Fix XSS vulnerability in user input handling"

**Agent Flow**:
```
TaskProfiler → Risk: Critical, Attitude: Strict
    ↓
Architect → Declares: input sanitization module, dependencies
    ↓
Builder → Implements sanitization
    ↓
Skeptic → Adversarial review, forensic depth
    ↓
Judge → Full test suite + security tests
    ↓
Coordinator → Low patience, escalate on first failure
```

**Why Swarm Wins**:
- Critical risk demands multiple perspectives
- Strict attitude prevents rushed fixes
- Skeptic delivers adversarial review
- Coordinator enforces low tolerance for failures

---

### 3. Refactoring with Structural Changes

**Trigger**: "Split monolithic module", "Extract trait for X"

**Example**: "Extract Storage trait from Database module"

**Agent Flow**:
```
TaskProfiler → Risk: High (wide reach)
    ↓
Architect → Declares: new trait, which modules implement it
    ↓
Builder → Extracts trait, updates implementors
    ↓
Skeptic → Verifies: all implementors updated, no breaking changes
    ↓
Judge → cargo check (type errors = immediate feedback)
    ↓
Scalpel → Fix any compile errors from refactoring
```

**Why Swarm Wins**:
- Architect defines interface boundaries before changes
- Skeptic ensures no module breaks the contract
- Scalpel handles inevitable type errors surgically
- Single agent might break consumers

---

### 4. Bug Investigation + Fix

**Trigger**: "Test failing", "Production bug report"

**Example**: "test_validate_token fails with timeout"

**Agent Flow**:
```
TaskProfiler → Risk: Moderate (depends on area)
    ↓
[Architect skipped for Moderate risk]
    ↓
Builder → Investigates, proposes fix
    ↓
Skeptic → Reviews: is root cause addressed?
    ↓
Judge → Run specific failing test
    ↓
[Scalpel if compile error]
```

**Why Swarm Wins**:
- Skeptic prevents wrong fix from being applied
- Judge provides test-specific feedback
- Coordinator tracks if same approach repeats (doom loop)

---

### 5. Compile Error Resolution (Scalpel Specialty)

**Trigger**: cargo check fails after Builder change

**Example**: "error[E0308]: mismatched types"

**Agent Flow**:
```
Judge → cargo check fails
    ↓
Scalpel → Targeted fix (max 10 lines/file)
    ↓
Judge → Re-verify compilation
    ↓
[Repeat or escalate to Builder if not scalpel-appropriate]
```

**Why Swarm Wins**:
- Scalpel is constrained (no redesign, surgical only)
- Faster than full Builder turn
- Prevents "while I'm here" refactoring
- Clear escalation path if fix requires redesign

---

### 6. Quick Fix (Fast Path)

**Trigger**: Typos, comments, docs, trivial changes

**Example**: "Fix typo in error message"

**Agent Flow**:
```
TaskProfiler → Risk: Low
    ↓
[Architect skipped]
    ↓
[Judge skipped for trivial changes]
    ↓
Builder → Make change
    ↓
[Skeptic skipped]
```

**Why Single Agent Wins**:
- Overhead of swarm > benefit
- Low risk, easily reversible
- Fast path completes in 1-2 turns

---

### 7. Test Addition

**Trigger**: "Add tests for module X"

**Example**: "Add unit tests for auth::validate_token"

**Agent Flow**:
```
TaskProfiler → Risk: Moderate
    ↓
[Architect skipped - no structural changes]
    ↓
Builder → Writes tests
    ↓
Skeptic → Verifies: tests cover edge cases, not just happy path
    ↓
Judge → cargo test (must pass)
```

**Why Swarm Wins**:
- Skeptic ensures test quality, not just test existence
- Judge verifies tests actually pass
- Prevents false-positive tests

---

### 8. Cross-Module Integration

**Trigger**: "Wire up X to Y", "Integrate new service"

**Example**: "Connect cache layer to user service"

**Agent Flow**:
```
TaskProfiler → Risk: High (multiple modules)
    ↓
Architect → Declares: interfaces between modules, dependency changes
    ↓
Builder → Implements integration points
    ↓
Skeptic → Verifies: both modules use same interface
    ↓
Judge → Integration tests
    ↓
Coordinator → Track progress across modules
```

**Why Swarm Wins**:
- Architect defines contract before implementation
- Prevents "works in isolation, fails together"
- Skeptic ensures interface consistency
- Coordinator tracks multi-module progress

---

## Agent Lifetime Model

### Start Conditions

| Agent | Trigger |
|-------|---------|
| **TaskProfiler** | Task received (always runs first) |
| **Architect** | Risk = High/Critical OR Builder escalation |
| **Builder** | Every step (unless escalated) |
| **Skeptic** | Risk >= Moderate AND after Builder turn |
| **Judge** | Every step (local, no LLM) |
| **Scalpel** | Judge reports compile/type errors |
| **Coordinator** | Entire task lifetime (orchestrates) |

### Stop Conditions

| Agent | Stop Trigger |
|-------|--------------|
| **Architect** | After producing StructuralDeclaration |
| **Builder** | Step complete OR escalation requested |
| **Skeptic** | After verdict delivered |
| **Judge** | After verification complete |
| **Scalpel** | Errors fixed OR exceeded scope |
| **Coordinator** | Task complete OR escalation to user |

### Lifetime Visualization

```
Task Received
    │
    ├─→ TaskProfiler ──────────────────────┐ (one-time assessment)
    │                                       │
    ├─→ Coordinator ────────────────────────┤ (entire task lifetime)
    │                                       │
    ├─→ [Architect] ──┐                     │ (conditional, High risk)
    │                 │                     │
    │   For each step:│                     │
    │   ├─→ Builder ──┼──┐                  │
    │   │             │  │                  │
    │   │   [Escalation?] ──────────────────┤
    │   │             │  │                  │
    │   ├─→ [Skeptic] ─┤  │ (conditional)   │
    │   │             │  │                  │
    │   ├─→ Judge ─────┤  │ (every step)    │
    │   │             │  │                  │
    │   └─→ [Scalpel] ─┘  │ (conditional)   │
    │                     │                  │
    └─→ Task Complete ←───┘
```

---

## Token Efficiency Analysis

| Scenario | Single Agent | Swarm | Savings |
|----------|--------------|-------|---------|
| Complex Feature | 15K tokens (multiple retries) | 12K tokens (Architect upfront) | 20% |
| Security Fix | 20K tokens (missed edge cases) | 18K tokens (Skeptic catches early) | 10% |
| Compile Error | 5K tokens (Builder redesign) | 2K tokens (Scalpel surgical) | 60% |
| Quick Fix | 2K tokens | 2K tokens | 0% |

**Key Insight**: Swarm saves tokens on complex tasks by catching issues early, but adds overhead for trivial tasks. The TaskProfiler routes to the right ensemble composition.

---

## Validation Checklist

To validate each use case works:

- [ ] TaskProfiler correctly assesses risk level
- [ ] Architect produces valid StructuralDeclaration (for High/Critical)
- [ ] Builder respects declaration boundaries
- [ ] Skeptic catches at least one issue per review
- [ ] Judge provides accurate compilation/test feedback
- [ ] Scalpel fixes compile errors without redesign
- [ ] Coordinator detects doom loop if Builder repeats
- [ ] Escalation path works (Builder → Architect)

---

## Next Steps

1. Create integration tests for each use case
2. Add metrics collection (token count, turns, success rate)
3. Build visualization for agent lifetime tracing
4. Implement agent activity logging for debugging
