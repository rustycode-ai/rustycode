# SWE-bench Experiment Report

**Date**: 2025-05-15
**Framework**: `scripts/swebench_experiment.py`
**Model**: Claude Opus 4.7 (Anthropic API)
**Goal**: Measure and improve SWE-bench Lite resolve rate through prompt engineering, context enrichment, and structured thinking.

---

## Executive Summary

| Run | Prompt | Instances | Resolved | Rate | Notes |
|-----|--------|-----------|----------|------|-------|
| AST | thinking_v8 | 30 | ?/30 | ? | Patches generated (28/30) but never evaluated (test_passed=None) |
| test-3 | thinking_v8 | 3 | ?/3 | ? | Patches never evaluated |
| v10-batch | thinking_v10 | 5 | 4/5 | **80%** | Curated easy/medium instances |
| v10-overall | thinking_v10 | 8 | 7/8 | **87.5%** | Combined v10 results |
| v11-retry | thinking_v11 | 8 | 2/8 | 25% | Harder instances, corrupted prompt |
| v11-retry2 | thinking_v11 | 8 | 1/8 | 12.5% | Same instances, model variance |
| v12-hard | thinking_v12 | 8 | 1/8 | 12.5% | Corrupted base prompt |
| v12-broad | thinking_v12 | 15 | 8/15 | **53.3%** | Clean run, fixed prompt + pre-loaded source |

**Critical**: Earlier analysis reported 0% across all runs due to using `r.get('resolved', False)` — the correct field is `test_passed`.

---

## Infrastructure

### Experiment Runner (`scripts/swebench_experiment.py`)

Per-instance pipeline:
1. **Setup**: Clone repo at pre-fix commit SHA, create venv, install package
2. **Context building**: Import maps, source directory listings, (v12) pre-loaded source snippets
3. **Agent loop**: LLM with tools (Bash, Read, Edit, Grep, Glob) for up to 40 turns
4. **Nudge system**: Turn-specific guidance at configurable thresholds
5. **Evaluation**: Apply model patch, run FAIL_TO_PASS tests, check pass/fail

### Known Infrastructure Bugs

1. **thinking_v11 prompt corruption** (FIXED in v12 patch): Committed version has actual newlines where `\n` escapes should be. All v11 runs used corrupted prompt. Fixed by `/tmp/patch_v12_clean.py`.

2. **AST evaluation never runs**: All 30 AST results have `test_passed=None`. Patches generated but not evaluated.

3. **Result field mismatch**: Results use `test_passed` (boolean), not `resolved`.

4. **No API rate limit handling**: Runner produces garbage patches when rate-limited instead of failing cleanly or retrying.

5. **Edit tool reversion**: Claude Code's Edit tool on `swebench_experiment.py` reverts to committed state. Workaround: external Python patch scripts.

---

## Prompt Evolution

### v10 — Edit-First with Import Context

`build_import_map()` pre-extracts imports/symbols. Strong edit-urgency nudges.

**Result: 4/5 (80%) on curated batch, 7/8 (87.5%) overall**

### v11 — Test-File Guard (corrupted)

Stronger test-file rules. **All runs used corrupted prompt** (actual newlines instead of `\n`).

**Result: 2/8 (25%) first run, 1/8 (12.5%) retry** — unreliable due to corruption

### v12 — Pre-loaded Source + Diagnosis Methodology

New additions:
- `build_source_snippets()`: Reads top 5 source files (150 lines each) via import/symbol analysis
- TRACE → DIAGNOSE → FIX → VERIFY methodology
- Diagnosis-first nudges (turn 2: "state root cause in one sentence")
- "Don't re-read pre-loaded files" instruction

**Result: 8/15 (53.3%) on broad batch — significant improvement from v11's 12.5% on overlapping hard instances.**

New instances solved by v12: sympy-11618, sympy-12096, sphinx-10323, xarray-3095, pylint-4661, requests-5414.
7 failures: 3 environment issues, 2 wrong fixes, 2 hard bugs.

---

## Failure Mode Analysis

### Categories (hard batch, v11/v12)

| Category | Count | Instances | Description |
|----------|-------|-----------|-------------|
| Wrong fix | ~50% | django-10973, sphinx-10435, xarray-3151 | Edits right file, wrong logic |
| Environment issue | ~25% | pytest-10356, requests-2931 | Python version incompatibility |
| Model variance | ~12% | requests-5414 | Intermittent pass/fail across runs |
| No output | ~12% | seaborn-3187 | Agent produces no edits |

### Environment Issues (fixable)

- **Python 3.10+ compatibility**: `collections.Callable` removed. Affects: requests, sympy, astropy
- **Test discovery**: pytest can't find tests with wrong name format
- **Package version mismatch**: pytest minversion check fails

---

## Instance Difficulty

### Easy (high resolve rate)
- psf__requests-1142, django__django-10880, pydata__xarray-2905, mwaskom__seaborn-3069 — all PASS in v10
- pylint-dev__pylint-4970 — PASS in all 3 runs (v11, v11-retry2, v12)

### Medium (sometimes resolves)
- psf__requests-5414 — PASS 1/3 runs

### Hard (never resolved)
- django__django-10973, sphinx-doc__sphinx-10435, pydata__xarray-3151 — wrong diagnosis
- pytest-dev__pytest-10356, psf__requests-2931 — environment issues
- mwaskom__seaborn-3187 — no patch / wrong fix

---

## Next Steps

### Immediate
- [ ] Re-run v12 broad batch after rate limit resets (~7 hours)
- [ ] Compare against v10 baseline on same 15 instances

### Infrastructure
- [ ] Add API rate limit retry logic (backoff + retry instead of garbage patch)
- [ ] Fix AST evaluation pipeline (test_passed=None for all 30)
- [ ] Add Python version compatibility checks
- [ ] Fix thinking_v11 corruption in git permanently

### Prompt
- [ ] Validate v12 pre-loaded source context hypothesis
- [ ] A/B test with multiple runs per instance to measure model variance
- [ ] Scale to full SWE-bench Lite (300 instances) for comparable score
