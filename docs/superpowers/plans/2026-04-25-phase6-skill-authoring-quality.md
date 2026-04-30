# Phase 6: Skill Authoring + Quality -- TDD Implementation Plan

**Date**: 2026-04-25
**Pattern Source**: "Skill Authoring Patterns from Anthropic's Best Practices" (Generative Programmer, Patterns 14, 21, 22, 24) + "Evaluation and Quality Gates" (Pattern E)
**Status**: Implemented
**See Also**: [Generative Programmer analysis](2026-04-25-generative-programmer-real-analysis.md#phase-status-map)
**Depends On**: Phase 5 (Progressive Tooling + Hooks) -- skill tool scoping needs tool tiers

---

## Overview

Complete the skill authoring system with production-quality patterns. Five work items that transform skills from static instruction files into a self-improving quality pipeline:

1. **Exclusion clauses** -- `excludes` field in skill YAML prevents wrong-skill activation
2. **Gotchas section** -- `gotchas` field auto-surfaces failure-mode warnings on activation
3. **Execution checklists** -- Auto-generated from workflow steps, visible in agent output
4. **LLM-as-Judge** -- Optional second model evaluates output quality with rubric-based scoring
5. **Output schema enforcement** -- JSON schemas for tier outputs, validated before handoff

**Success metrics**:
- Skills with exclusion clauses activate more accurately
- Gotchas prevent at least 3 common failure modes per project
- LLM-as-Judge catches semantic errors missed by rule-based gates
- Output format compliance > 95%

## Implementation Status

Completed in this pass:

- `crates/rustycode-skill/src/exclusions.rs` and `crates/rustycode-skill/src/gotchas.rs` now exist.
- `SkillDefinition` now includes `excludes` and `gotchas`.
- `SkillRegistry` frontmatter parsing and `ActivationManager` scoring now honor the new exclusion metadata.
- `crates/rustycode-skill/src/checklist.rs` now generates execution checklists from pipelines and workflows.
- `crates/rustycode-orchestration/src/schema.rs` now validates structured outputs against JSON schemas.
- `crates/rustycode-orchestration/src/judge.rs` now provides rubric-based judging helpers and built-in rubrics.
- `crates/rustycode-orchestration/src/verification_gates.rs` now includes judge and schema verification strategies.

---

## Existing Codebase Leveraged

| Component | Crate | What It Provides |
|-----------|-------|-----------------|
| `SkillDefinition` | `rustycode-skill/src/types.rs` | Skill metadata struct with 20+ fields |
| `ActivationSpec` | `rustycode-skill/src/types.rs` | Activation mode, paths, trigger_tools |
| `ActivationManager` | `rustycode-skill/src/activation.rs` | Skill activation scoring (`score_skill()`), budget management |
| `SkillRegistry` | `rustycode-skill/src/registry.rs` | Skill storage and lookup |
| `ParsedFrontmatter` | `rustycode-skill/src/metadata.rs` | Frontmatter field extraction |
| `parse_frontmatter_map()` | `rustycode-protocol/src/frontmatter.rs` | YAML frontmatter parser |
| `Workflow` / `WorkflowPhase` | `rustycode-skill/src/workflows.rs` | Enforceable workflow definitions with phases |
| `Pipeline` / `PipelineStage` | `rustycode-skill/src/types.rs` | Procedure pipeline stages |
| `VerificationGateRegistry` | `rustycode-orchestration/src/verification_gates.rs` | Strategy-based verification with `VerificationStrategy` trait |
| `VerificationOutcome` | `rustycode-orchestration/src/verification_gates.rs` | Valid / Invalid / Uncertain results |
| `TraceEntry` | `rustycode-orchestration/src/execution_trace.rs` | Step execution results (output, exit_code, cost) |
| `Step` | `rustycode-orchestration/src/types.rs` | Step definition with expected_output_type |
| `LLMProvider` trait | `rustycode-llm/src/provider.rs` | `complete()` method for LLM calls |
| `OrchestrationError` | `rustycode-orchestration/src/error.rs` | Error types with `is_recoverable()`, `category()` |
| `TaskResult` | `rustycode-orchestration/src/pipeline.rs` | Success/Failed task outcomes |

---

## File Structure

| # | File | Action | Purpose |
|---|------|--------|---------|
| 1 | `crates/rustycode-skill/src/exclusions.rs` | **Create** | Exclusion clause parsing and activation scoring integration |
| 2 | `crates/rustycode-skill/src/gotchas.rs` | **Create** | Gotchas section parsing, matching, and auto-surfacing |
| 3 | `crates/rustycode-skill/src/checklist.rs` | **Create** | Execution checklist generation from workflow/pipeline steps |
| 4 | `crates/rustycode-orchestration/src/judge.rs` | **Edit** | LLM-as-Judge with rubric-based scoring |
| 5 | `crates/rustycode-orchestration/src/schema.rs` | **Edit** | Output schema enforcement (JSON Schema validation) |
| 6 | `crates/rustycode-skill/src/types.rs` | **Edit** | Add `excludes` and `gotchas` fields to `SkillDefinition` |
| 7 | `crates/rustycode-skill/src/metadata.rs` | **Edit** | Parse `excludes` and `gotchas` from frontmatter |
| 8 | `crates/rustycode-skill/src/activation.rs` | **Edit** | Integrate exclusion scoring into `score_skill()` |
| 9 | `crates/rustycode-skill/src/lib.rs` | **Edit** | Add module declarations for new files |
| 10 | `crates/rustycode-orchestration/src/lib.rs` | **Edit** | Add module declarations for new files |
| 11 | `crates/rustycode-orchestration/src/error.rs` | **Edit** | Add `JudgeError` and `SchemaError` variants |
| 12 | `crates/rustycode-orchestration/src/verification_gates.rs` | **Edit** | Integrate JudgeStrategy and SchemaStrategy |
| 13 | `crates/rustycode-skill/Cargo.toml` | **Edit** | Add `thiserror` dependency |
| 14 | `crates/rustycode-orchestration/Cargo.toml` | **Edit** | Add `jsonschema` dependency |

---

## TDD Steps

---

### Chunk 1: Exclusion Clauses (15 tests)

**Files**: `crates/rustycode-skill/src/exclusions.rs` (new), `crates/rustycode-skill/src/types.rs` (edit), `crates/rustycode-skill/src/metadata.rs` (edit), `crates/rustycode-skill/src/lib.rs` (edit)

#### Task 1.1: Create ExclusionClause type and parser

**File**: `crates/rustycode-skill/src/exclusions.rs`

**Write failing test first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_exclusions() {
        let clauses = ExclusionClauseSet::from_list(&[]);
        assert!(clauses.is_empty());
        assert!(!clauses.matches_any("deploy to production"));
    }

    #[test]
    fn parse_string_exclusions() {
        let clauses = ExclusionClauseSet::from_list(&[
            "blog articles".to_string(),
            "documentation generation".to_string(),
            "newsletter".to_string(),
        ]);
        assert_eq!(clauses.len(), 3);
        assert!(clauses.matches_any("write a blog article about Rust"));
        assert!(clauses.matches_any("generate documentation for this API"));
        assert!(clauses.matches_any("send a newsletter"));
        assert!(!clauses.matches_any("implement a sorting algorithm"));
    }

    #[test]
    fn exclusion_matching_is_case_insensitive() {
        let clauses = ExclusionClauseSet::from_list(&[
            "BLOG ARTICLES".to_string(),
        ]);
        assert!(clauses.matches_any("write a blog article"));
    }

    #[test]
    fn exclusion_matches_partial_words() {
        let clauses = ExclusionClauseSet::from_list(&[
            "newsletter".to_string(),
        ]);
        // "newsletters" should also match "newsletter"
        assert!(clauses.matches_any("send newsletters"));
    }

    #[test]
    fn from_list_with_whitespace() {
        let clauses = ExclusionClauseSet::from_list(&[
            "  blog articles  ".to_string(),
            "".to_string(),
            "documentation  ".to_string(),
        ]);
        assert_eq!(clauses.len(), 2); // empty string excluded
    }
}
```

**Verify fail**:
```bash
cargo test -p rustycode-skill -- exclusions::tests 2>&1 | grep "test result"
# Expected: 0 passed, 5 failed (compilation error -- module does not exist)
```

**Write minimal implementation**:

```rust
//! Exclusion clauses for skill activation accuracy.
//!
//! Skills declare what they should NOT be used for via `excludes` in YAML
//! frontmatter. These clauses reduce false-positive activations when multiple
//! skills have overlapping trigger keywords.

/// A set of exclusion clauses that prevent a skill from activating.
///
/// Parsed from the `excludes` field in skill YAML frontmatter. Each clause
/// is a short phrase describing a task type this skill should NOT handle.
/// Matching is case-insensitive substring matching against the user's context.
#[derive(Debug, Clone, Default)]
pub struct ExclusionClauseSet {
    /// Normalized (lowercased, trimmed) exclusion phrases.
    clauses: Vec<String>,
}

impl ExclusionClauseSet {
    /// Create an exclusion set from a list of raw strings.
    /// Empty strings are ignored. All strings are trimmed and lowercased.
    pub fn from_list(raw: &[String]) -> Self {
        let clauses = raw
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        Self { clauses }
    }

    /// Returns true if any exclusion clause matches the given context.
    /// Case-insensitive substring matching.
    pub fn matches_any(&self, context: &str) -> bool {
        let context_lower = context.to_lowercase();
        self.clauses.iter().any(|clause| context_lower.contains(clause))
    }

    /// Number of active exclusion clauses.
    pub fn len(&self) -> usize {
        self.clauses.len()
    }

    /// Whether there are no exclusion clauses.
    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    /// Get the raw clauses (lowercased).
    pub fn clauses(&self) -> &[String] {
        &self.clauses
    }
}
```

**Verify pass**:
```bash
cargo test -p rustycode-skill -- exclusions::tests 2>&1 | tail -3
# Expected: 5 passed, 0 failed
```

**Commit**: `feat(skill): add ExclusionClauseSet for skill activation exclusion`

---

#### Task 1.2: Add `excludes` field to SkillDefinition and frontmatter parsing

**Files**: `crates/rustycode-skill/src/types.rs` (edit), `crates/rustycode-skill/src/metadata.rs` (edit), `crates/rustycode-skill/src/lib.rs` (edit)

**Write failing test first** (in `types.rs` tests):

```rust
#[test]
fn skill_definition_has_excludes() {
    let def = SkillDefinition {
        id: "test".to_string(),
        name: "Test".to_string(),
        description: "A test".to_string(),
        when_to_use: String::new(),
        source: SkillSource::Bundled,
        version: String::new(),
        activation: ActivationSpec::always(),
        effort: SkillEffortLevel::default(),
        context: ExecutionContext::default(),
        procedure: None,
        allowed_tools: vec![],
        user_invocable: true,
        model_invocable: true,
        agent: None,
        model_override: None,
        argument_hint: None,
        categories: vec![],
        quality: SkillQuality::default(),
        lifecycle_state: LifecycleState::default(),
        content_path: PathBuf::new(),
        content: None,
        excludes: vec!["blog articles".to_string(), "newsletter".to_string()],
        gotchas: vec![],
    };
    assert_eq!(def.excludes.len(), 2);
    assert_eq!(def.excludes[0], "blog articles");
}
```

**Verify fail**:
```bash
cargo test -p rustycode-skill -- types::tests::skill_definition_has_excludes 2>&1 | grep "error\[E"
# Expected: missing field `excludes` or `gotchas`
```

**Write minimal implementation**:

In `types.rs`, add two fields to `SkillDefinition`:

```rust
// Add to SkillDefinition struct, after categories:
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gotchas: Vec<String>,
```

In `metadata.rs`, add to `ParsedFrontmatter`:

```rust
    pub excludes: Vec<String>,
    pub gotchas: Vec<String>,
```

And in `parse_frontmatter_fields()`, add:

```rust
        excludes: extract_string_array(fm, "excludes"),
        gotchas: extract_string_array(fm, "gotchas"),
```

In `lib.rs`, add:

```rust
pub mod exclusions;
pub mod gotchas;
pub mod checklist;
```

Update all existing `SkillDefinition` construction sites (in tests across the crate) to include the new fields:

```rust
excludes: vec![],
gotchas: vec![],
```

**Verify pass**:
```bash
cargo test -p rustycode-skill -- types::tests::skill_definition_has_excludes 2>&1 | tail -3
# Expected: 1 passed
cargo clippy -p rustycode-skill -- -D warnings 2>&1 | tail -3
# Expected: 0 warnings
```

**Commit**: `feat(skill): add excludes and gotchas fields to SkillDefinition`

---

#### Task 1.3: Integrate exclusion scoring into activation

**File**: `crates/rustycode-skill/src/activation.rs` (edit)

**Write failing test first**:

```rust
#[test]
fn exclusion_clause_reduces_score() {
    let mut reg = SkillRegistry::new();
    let mut skill = make_skill("code-review", "Reviews code for quality", "Use when reviewing");
    skill.excludes = vec!["blog articles".to_string(), "documentation".to_string()];
    reg.register_bundled(skill);
    let mgr = ActivationManager::new(10_000);

    // Should score high for code review context
    let recs_code = mgr.evaluate_for_context(&reg, "please review my code changes");
    assert!(!recs_code.is_empty());

    // Should score lower (or filtered) for documentation context
    let recs_docs = mgr.evaluate_for_context(&reg, "write documentation for this API");
    // The skill may still appear but its score should be penalized
    if let Some(doc_rec) = recs_docs.iter().find(|r| r.skill_id == "code-review") {
        let code_rec = recs_code.iter().find(|r| r.skill_id == "code-review").unwrap();
        assert!(doc_rec.score < code_rec.score);
    }
}

#[test]
fn exclusion_clause_can_filter_skill_entirely() {
    let mut reg = SkillRegistry::new();
    let mut skill = make_skill("code-review", "Reviews code for quality", "Use when reviewing");
    skill.excludes = vec!["blog".to_string()];
    reg.register_bundled(skill);
    let mgr = ActivationManager::new(10_000);

    // "blog" exclusion should heavily penalize the score
    let recs = mgr.evaluate_for_context(&reg, "write a blog article");
    // Skill should not appear or have very low score
    if let Some(rec) = recs.iter().find(|r| r.skill_id == "code-review") {
        // If it appears, score should be below threshold
        assert!(rec.score < 0.3);
    }
}
```

**Verify fail**:
```bash
cargo test -p rustycode-skill -- activation::tests::exclusion_clause_reduces_score 2>&1 | tail -3
# Expected: assertion failed (score not penalized)
```

**Write minimal implementation**:

In `activation.rs`, modify `score_skill()` to use exclusion clauses:

```rust
fn score_skill(&self, skill: &SkillDefinition, context_lower: &str) -> f64 {
    let mut score = 0.0;

    // Exclusion penalty: if any exclusion clause matches, apply heavy penalty
    if !skill.excludes.is_empty() {
        let exclusion_set = crate::exclusions::ExclusionClauseSet::from_list(&skill.excludes);
        if exclusion_set.matches_any(context_lower) {
            score -= 5.0; // Heavy penalty for matching exclusion
        }
    }

    // ... existing scoring logic unchanged ...
```

**Verify pass**:
```bash
cargo test -p rustycode-skill -- activation::tests::exclusion 2>&1 | tail -3
# Expected: 2 passed
cargo test -p rustycode-skill 2>&1 | tail -3
# Expected: all existing tests still pass
```

**Commit**: `feat(skill): integrate exclusion clauses into activation scoring`

---

### Chunk 2: Gotchas Section (14 tests)

**File**: `crates/rustycode-skill/src/gotchas.rs` (new)

#### Task 2.1: Create Gotcha type with parsing and matching

**Write failing test first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gotcha_from_string() {
        let gotcha = Gotcha::new(
            "Scanned PDFs return empty silently. Check page type first.".to_string(),
        );
        assert!(!gotcha.description.is_empty());
        assert!(gotcha.keywords.is_empty()); // no explicit keywords
    }

    #[test]
    fn parse_gotcha_with_keywords() {
        let gotcha = Gotcha::with_keywords(
            "Scanned PDFs return empty silently".to_string(),
            vec!["pdf".to_string(), "scan".to_string()],
        );
        assert_eq!(gotcha.keywords.len(), 2);
    }

    #[test]
    fn gotcha_matches_context_by_keyword() {
        let gotcha = Gotcha::with_keywords(
            "Scanned PDFs return empty".to_string(),
            vec!["pdf".to_string()],
        );
        assert!(gotcha.matches("read the pdf document"));
        assert!(!gotcha.matches("read the csv file"));
    }

    #[test]
    fn gotcha_matches_context_by_description_words() {
        let gotcha = Gotcha::new("Scanned PDFs return empty silently".to_string());
        // Should match when context contains significant words from description
        assert!(gotcha.matches("extract text from scanned pdfs"));
    }

    #[test]
    fn gotcha_set_from_strings() {
        let gotchas = GotchaSet::from_descriptions(&[
            "Scanned PDFs return empty silently".to_string(),
            "UTF-8 encoding issues with BOM markers".to_string(),
        ]);
        assert_eq!(gotchas.len(), 2);
    }

    #[test]
    fn gotcha_set_find_relevant() {
        let gotchas = GotchaSet::from_descriptions(&[
            "Scanned PDFs return empty silently. Check page type first.".to_string(),
            "UTF-8 BOM markers cause parse failures in CSV readers.".to_string(),
            "Async tasks may deadlock if the runtime is dropped prematurely.".to_string(),
        ]);
        let relevant = gotchas.find_relevant("parse a pdf document for text extraction");
        assert_eq!(relevant.len(), 1);
        assert!(relevant[0].description.contains("PDF"));
    }

    #[test]
    fn gotcha_set_find_relevant_returns_all_if_no_keywords() {
        let gotchas = GotchaSet::from_descriptions(&[
            "Watch out for X".to_string(),
        ]);
        let relevant = gotchas.find_relevant("unrelated context");
        // With no explicit keyword matching, returns all as general warnings
        assert_eq!(relevant.len(), 1);
    }

    #[test]
    fn gotcha_format_for_display() {
        let gotcha = Gotcha::new("Scanned PDFs return empty silently".to_string());
        let formatted = gotcha.format_warning();
        assert!(formatted.contains("WARNING"));
        assert!(formatted.contains("Scanned PDFs"));
    }

    #[test]
    fn gotcha_set_empty() {
        let gotchas = GotchaSet::default();
        assert!(gotchas.is_empty());
        assert!(gotchas.find_relevant("anything").is_empty());
    }

    #[test]
    fn gotcha_matching_is_case_insensitive() {
        let gotcha = Gotcha::with_keywords(
            "PDF issue".to_string(),
            vec!["pdf".to_string()],
        );
        assert!(gotcha.matches("PARSE THE PDF FILE"));
    }

    #[test]
    fn gotcha_serialization_roundtrip() {
        let gotcha = Gotcha::with_keywords(
            "Test gotcha".to_string(),
            vec!["keyword1".to_string()],
        );
        let json = serde_json::to_string(&gotcha).unwrap();
        let back: Gotcha = serde_json::from_str(&json).unwrap();
        assert_eq!(back.description, "Test gotcha");
        assert_eq!(back.keywords.len(), 1);
    }

    #[test]
    fn gotcha_set_from_mixed_strings() {
        let gotchas = GotchaSet::from_descriptions(&[
            "".to_string(),
            "Valid gotcha".to_string(),
            "   ".to_string(),
        ]);
        assert_eq!(gotchas.len(), 1);
    }

    #[test]
    fn gotcha_relevance_scoring() {
        let gotcha = Gotcha::with_keywords(
            "PDF issue".to_string(),
            vec!["pdf".to_string(), "scan".to_string()],
        );
        // Both keywords match -> higher score
        let score_both = gotcha.relevance_score("scan the pdf file");
        // Only one keyword matches -> lower score
        let score_one = gotcha.relevance_score("read the pdf file");
        assert!(score_both > score_one);
    }
}
```

**Verify fail**:
```bash
cargo test -p rustycode-skill -- gotchas::tests 2>&1 | grep "error\[E"
# Expected: module not found
```

**Write minimal implementation**:

```rust
//! Gotchas section for auto-surfacing failure-mode warnings.
//!
//! Skills declare known pitfalls via `gotchas` in YAML frontmatter. When a
//! skill is activated, relevant gotchas are surfaced as warnings to the agent,
//! preventing common failure modes before they occur.

use serde::{Deserialize, Serialize};

/// A single gotcha (known pitfall) associated with a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gotcha {
    /// Human-readable description of the pitfall.
    pub description: String,
    /// Optional keywords that trigger this warning. If empty, it is always
    /// surfaced as a general warning when the skill is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

impl Gotcha {
    /// Create a gotcha from a description alone (no explicit keywords).
    /// Word-matching from the description will be used for relevance.
    pub fn new(description: String) -> Self {
        let description = description.trim().to_string();
        Self {
            description,
            keywords: Vec::new(),
        }
    }

    /// Create a gotcha with explicit keywords for matching.
    pub fn with_keywords(description: String, keywords: Vec<String>) -> Self {
        Self {
            description: description.trim().to_string(),
            keywords,
        }
    }

    /// Whether this gotcha is relevant to the given context.
    /// If keywords are set, matches by keyword. Otherwise, matches by
    /// significant words from the description (length > 3).
    pub fn matches(&self, context: &str) -> bool {
        self.relevance_score(context) > 0.0
    }

    /// Score how relevant this gotcha is to the given context (0.0 = not relevant).
    pub fn relevance_score(&self, context: &str) -> f64 {
        let context_lower = context.to_lowercase();
        let mut score = 0.0;

        if !self.keywords.is_empty() {
            for keyword in &self.keywords {
                if context_lower.contains(&keyword.to_lowercase()) {
                    score += 1.0;
                }
            }
        } else {
            // Fall back to matching significant words from description
            for word in self.description.to_lowercase().split_whitespace() {
                if word.len() > 3 && context_lower.contains(word) {
                    score += 0.5;
                }
            }
        }

        score
    }

    /// Format this gotcha as a warning string for agent output.
    pub fn format_warning(&self) -> String {
        format!("WARNING: {}", self.description)
    }
}

/// A collection of gotchas for a skill.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GotchaSet {
    gotchas: Vec<Gotcha>,
}

impl GotchaSet {
    /// Create a gotcha set from raw description strings.
    /// Empty and whitespace-only strings are filtered out.
    pub fn from_descriptions(descriptions: &[String]) -> Self {
        let gotchas = descriptions
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| Gotcha::new(s.to_string()))
            .collect();
        Self { gotchas }
    }

    /// Create a gotcha set from pre-built Gotcha instances.
    pub fn from_gotchas(gotchas: Vec<Gotcha>) -> Self {
        Self { gotchas }
    }

    /// Find gotchas relevant to the given context.
    /// Returns all gotchas that match the context, sorted by relevance (highest first).
    /// If no gotchas match by keyword, returns all gotchas as general warnings.
    pub fn find_relevant(&self, context: &str) -> Vec<&Gotcha> {
        let mut scored: Vec<(&Gotcha, f64)> = self
            .gotchas
            .iter()
            .map(|g| (g, g.relevance_score(context)))
            .collect();

        let any_match = scored.iter().any(|(_, s)| *s > 0.0);

        if any_match {
            scored.retain(|(_, s)| *s > 0.0);
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.into_iter().map(|(g, _)| g).collect()
        } else {
            // No keyword match: return all as general warnings
            self.gotchas.iter().collect()
        }
    }

    /// Number of gotchas.
    pub fn len(&self) -> usize {
        self.gotchas.len()
    }

    /// Whether there are no gotchas.
    pub fn is_empty(&self) -> bool {
        self.gotchas.is_empty()
    }

    /// Get all gotchas.
    pub fn all(&self) -> &[Gotcha] {
        &self.gotchas
    }
}
```

**Verify pass**:
```bash
cargo test -p rustycode-skill -- gotchas::tests 2>&1 | tail -3
# Expected: 13 passed, 0 failed
```

**Commit**: `feat(skill): add Gotcha/GotchaSet for auto-surfacing failure-mode warnings`

---

#### Task 2.2: Add gotchas field parsing to frontmatter

This was already done in Task 1.2 (the `gotchas` field was added to `SkillDefinition` and `ParsedFrontmatter`). The remaining integration is in the `gotchas` field being populated from YAML. Verify with:

**Write test** (in `metadata.rs` or a new integration test):

```rust
#[test]
fn parse_frontmatter_with_gotchas() {
    use rustycode_protocol::frontmatter::{parse_frontmatter_map, split_frontmatter};

    let yaml = "name: test\ngotchas:\n  - Scanned PDFs return empty\n  - UTF-8 BOM issues";
    let map = parse_frontmatter_map(yaml);
    let parsed = crate::metadata::parse_frontmatter_fields(&map, "fallback");
    assert_eq!(parsed.gotchas.len(), 2);
    assert_eq!(parsed.gotchas[0], "Scanned PDFs return empty");
}
```

**Verify pass**:
```bash
cargo test -p rustycode-skill -- parse_frontmatter_with_gotchas 2>&1 | tail -3
# Expected: 1 passed
```

**Commit**: `feat(skill): wire gotchas field from frontmatter to SkillDefinition`

---

### Chunk 3: Execution Checklists (12 tests)

**File**: `crates/rustycode-skill/src/checklist.rs` (new)

#### Task 3.1: Create ChecklistItem and Checklist types

**Write failing test first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Pipeline, PipelineStage};

    #[test]
    fn checklist_item_creation() {
        let item = ChecklistItem::new("Write failing test".to_string());
        assert_eq!(item.description, "Write failing test");
        assert!(!item.checked);
    }

    #[test]
    fn checklist_item_check_off() {
        let mut item = ChecklistItem::new("Write failing test".to_string());
        assert!(!item.is_checked());
        item.check();
        assert!(item.is_checked());
    }

    #[test]
    fn from_pipeline_stages() {
        let pipeline = Pipeline {
            stages: vec![
                PipelineStage {
                    name: "Write Tests".to_string(),
                    description: "Write unit tests first".to_string(),
                    required_tools: vec![],
                    parallel: false,
                },
                PipelineStage {
                    name: "Implement".to_string(),
                    description: "Write minimal code to pass tests".to_string(),
                    required_tools: vec![],
                    parallel: false,
                },
                PipelineStage {
                    name: "Refactor".to_string(),
                    description: "Clean up while keeping tests green".to_string(),
                    required_tools: vec![],
                    parallel: false,
                },
            ],
        };
        let checklist = Checklist::from_pipeline(&pipeline);
        assert_eq!(checklist.items.len(), 3);
        assert_eq!(checklist.items[0].description, "Write Tests: Write unit tests first");
        assert!(!checklist.is_complete());
    }

    #[test]
    fn checklist_progress() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
            ChecklistItem::new("Step 3".to_string()),
        ]);
        assert_eq!(checklist.progress(), (0, 3));

        checklist.items[0].check();
        assert_eq!(checklist.progress(), (1, 3));

        checklist.items[1].check();
        checklist.items[2].check();
        assert_eq!(checklist.progress(), (3, 3));
        assert!(checklist.is_complete());
    }

    #[test]
    fn checklist_format_markdown() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
        ]);
        checklist.items[0].check();
        let md = checklist.format_markdown();
        assert!(md.contains("[x] Step 1"));
        assert!(md.contains("[ ] Step 2"));
    }

    #[test]
    fn from_workflow_phases() {
        use crate::workflows::{Workflow, WorkflowPhase, VerificationRule, FailureHandling};
        use rustycode_protocol::team::TeamRole;

        let workflow = Workflow {
            id: "test-wf".to_string(),
            name: "Test Workflow".to_string(),
            description: "A test".to_string(),
            phases: vec![
                WorkflowPhase {
                    name: "RED".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Write failing test".to_string(),
                    verification: Some(VerificationRule {
                        check: "Test fails".to_string(),
                        retry_max: 2,
                        escalate_on_failure: false,
                    }),
                    on_failure: FailureHandling::Retry,
                },
                WorkflowPhase {
                    name: "GREEN".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Make test pass".to_string(),
                    verification: None,
                    on_failure: FailureHandling::Retry,
                },
            ],
            triggers: vec![],
            enabled: true,
        };
        let checklist = Checklist::from_workflow(&workflow);
        assert_eq!(checklist.items.len(), 2);
        assert_eq!(checklist.items[0].description, "RED: Write failing test");
    }

    #[test]
    fn empty_checklist_is_complete() {
        let checklist = Checklist::new(vec![]);
        assert!(checklist.is_complete());
        assert_eq!(checklist.progress(), (0, 0));
    }

    #[test]
    fn checklist_check_by_index() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
        ]);
        assert!(checklist.check(0).is_ok());
        assert!(checklist.check(5).is_err());
        assert!(checklist.items[0].checked);
        assert!(!checklist.items[1].checked);
    }

    #[test]
    fn checklist_current_step() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
        ]);
        assert_eq!(checklist.current_step(), Some(0));
        checklist.items[0].check();
        assert_eq!(checklist.current_step(), Some(1));
        checklist.items[1].check();
        assert_eq!(checklist.current_step(), None);
    }

    #[test]
    fn checklist_serialization_roundtrip() {
        let mut checklist = Checklist::new(vec![
            ChecklistItem::new("Step 1".to_string()),
            ChecklistItem::new("Step 2".to_string()),
        ]);
        checklist.items[0].check();
        let json = serde_json::to_string(&checklist).unwrap();
        let back: Checklist = serde_json::from_str(&json).unwrap();
        assert!(back.items[0].checked);
        assert!(!back.items[1].checked);
    }

    #[test]
    fn from_pipeline_with_description_merge() {
        let pipeline = Pipeline {
            stages: vec![
                PipelineStage {
                    name: "Setup".to_string(),
                    description: "Configure environment".to_string(),
                    required_tools: vec!["bash".to_string()],
                    parallel: false,
                },
            ],
        };
        let checklist = Checklist::from_pipeline(&pipeline);
        assert_eq!(checklist.items[0].description, "Setup: Configure environment");
    }

    #[test]
    fn from_pipeline_stage_no_description() {
        let pipeline = Pipeline {
            stages: vec![
                PipelineStage {
                    name: "Setup".to_string(),
                    description: String::new(),
                    required_tools: vec![],
                    parallel: false,
                },
            ],
        };
        let checklist = Checklist::from_pipeline(&pipeline);
        assert_eq!(checklist.items[0].description, "Setup");
    }
}
```

**Verify fail**:
```bash
cargo test -p rustycode-skill -- checklist::tests 2>&1 | grep "error\[E"
# Expected: module not found
```

**Write minimal implementation**:

```rust
//! Execution checklists auto-generated from skill workflow/pipeline steps.
//!
//! When a skill is activated, its workflow steps are converted into a visible
//! checklist. The agent ticks off steps as it completes them, providing
//! progress visibility to the user.

use crate::types::{Pipeline, ProcedureKind};
use crate::workflows::Workflow;
use serde::{Deserialize, Serialize};

/// A single item in an execution checklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    /// Description of the step to complete.
    pub description: String,
    /// Whether this step has been completed.
    pub checked: bool,
}

impl ChecklistItem {
    /// Create a new unchecked item.
    pub fn new(description: String) -> Self {
        Self {
            description,
            checked: false,
        }
    }

    /// Mark this item as checked.
    pub fn check(&mut self) {
        self.checked = true;
    }

    /// Whether this item is checked.
    pub fn is_checked(&self) -> bool {
        self.checked
    }
}

/// An execution checklist generated from a skill's workflow or pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checklist {
    /// Ordered checklist items.
    pub items: Vec<ChecklistItem>,
}

impl Checklist {
    /// Create a checklist from pre-built items.
    pub fn new(items: Vec<ChecklistItem>) -> Self {
        Self { items }
    }

    /// Generate a checklist from a pipeline's stages.
    pub fn from_pipeline(pipeline: &Pipeline) -> Self {
        let items = pipeline
            .stages
            .iter()
            .map(|stage| {
                let desc = if stage.description.is_empty() {
                    stage.name.clone()
                } else {
                    format!("{}: {}", stage.name, stage.description)
                };
                ChecklistItem::new(desc)
            })
            .collect();
        Self { items }
    }

    /// Generate a checklist from a workflow's phases.
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let items = workflow
            .phases
            .iter()
            .map(|phase| {
                let desc = if phase.instructions.is_empty() {
                    phase.name.clone()
                } else {
                    format!("{}: {}", phase.name, phase.instructions)
                };
                ChecklistItem::new(desc)
            })
            .collect();
        Self { items }
    }

    /// Generate a checklist from a skill's procedure (pipeline or instruction).
    /// Returns None if the skill has no procedure or uses a plain instruction
    /// (no structured steps).
    pub fn from_procedure(procedure: &ProcedureKind) -> Option<Self> {
        match procedure {
            ProcedureKind::Pipeline(pipeline) => Some(Self::from_pipeline(pipeline)),
            ProcedureKind::Instruction(_) => None,
        }
    }

    /// Check off an item by index. Returns error if index out of bounds.
    pub fn check(&mut self, index: usize) -> Result<(), ChecklistError> {
        self.items
            .get_mut(index)
            .map(|item| item.check())
            .ok_or(ChecklistError::IndexOutOfRange {
                index,
                len: self.items.len(),
            })
    }

    /// Get the index of the current (first unchecked) step.
    /// Returns None if all steps are complete.
    pub fn current_step(&self) -> Option<usize> {
        self.items.iter().position(|item| !item.checked)
    }

    /// How many items are checked vs total.
    pub fn progress(&self) -> (usize, usize) {
        let checked = self.items.iter().filter(|i| i.checked).count();
        (checked, self.items.len())
    }

    /// Whether all items are checked.
    pub fn is_complete(&self) -> bool {
        self.items.iter().all(|i| i.checked)
    }

    /// Format as markdown checkboxes for agent output.
    pub fn format_markdown(&self) -> String {
        self.items
            .iter()
            .map(|item| {
                let marker = if item.checked { "[x]" } else { "[ ]" };
                format!("- {marker} {}", item.description)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Error from checklist operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ChecklistError {
    #[error("checklist index {index} out of range (len={len})")]
    IndexOutOfRange { index: usize, len: usize },
}
```

Note: Add `thiserror` to `crates/rustycode-skill/Cargo.toml`:

```toml
thiserror.workspace = true
```

**Verify pass**:
```bash
cargo test -p rustycode-skill -- checklist::tests 2>&1 | tail -3
# Expected: 12 passed, 0 failed
```

**Commit**: `feat(skill): add Checklist auto-generation from pipeline/workflow steps`

---

### Chunk 4: Output Schema Enforcement (16 tests)

**Files**: `crates/rustycode-orchestration/src/schema.rs` (new), `crates/rustycode-orchestration/src/error.rs` (edit), `crates/rustycode-orchestration/src/lib.rs` (edit), `crates/rustycode-orchestration/Cargo.toml` (edit)

#### Task 4.1: Create OutputSchema type and validation

**Add dependency** to `crates/rustycode-orchestration/Cargo.toml`:

```toml
jsonschema = "0.26"
```

**Write failing test first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_json_against_simple_schema() {
        let schema = OutputSchema::from_json(json!({
            "type": "object",
            "properties": {
                "output": { "type": "string" },
                "success": { "type": "boolean" }
            },
            "required": ["output", "success"]
        }));
        let result = schema.validate(json!({
            "output": "hello",
            "success": true
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn invalid_json_against_simple_schema() {
        let schema = OutputSchema::from_json(json!({
            "type": "object",
            "properties": {
                "output": { "type": "string" }
            },
            "required": ["output"]
        }));
        let result = schema.validate(json!({
            "missing_output": true
        }));
        assert!(!result.is_valid());
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn schema_from_raw_json_string() {
        let schema_json = r#"{"type": "object", "required": ["status"]}"#;
        let schema = OutputSchema::parse(schema_json).unwrap();
        let result = schema.validate(json!({"status": "ok"}));
        assert!(result.is_valid());
    }

    #[test]
    fn schema_parse_invalid_json() {
        let result = OutputSchema::parse("not valid json {{{");
        assert!(result.is_err());
    }

    #[test]
    fn validation_result_error_messages() {
        let schema = OutputSchema::from_json(json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            },
            "required": ["count"]
        }));
        let result = schema.validate(json!({"count": "not a number"}));
        assert!(!result.is_valid());
        let error_msg = result.error_message();
        assert!(!error_msg.is_empty());
    }

    #[test]
    fn tier_output_schema_plan() {
        let schema = TierSchema::plan();
        let result = schema.validate(json!({
            "steps": [
                {"description": "Implement feature X", "files": ["src/main.rs"]}
            ],
            "estimated_complexity": "medium",
            "risks": []
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn tier_output_schema_plan_missing_steps() {
        let schema = TierSchema::plan();
        let result = schema.validate(json!({
            "estimated_complexity": "medium",
            "risks": []
        }));
        assert!(!result.is_valid());
    }

    #[test]
    fn tier_output_schema_code_change() {
        let schema = TierSchema::code_change();
        let result = schema.validate(json!({
            "files_modified": ["src/lib.rs"],
            "diff": "+added line\n-removed line",
            "tests_passed": true
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn tier_output_schema_verification() {
        let schema = TierSchema::verification();
        let result = schema.validate(json!({
            "passed": true,
            "checks": [
                {"name": "compilation", "passed": true},
                {"name": "tests", "passed": true}
            ]
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn tier_output_schema_verification_failed() {
        let schema = TierSchema::verification();
        let result = schema.validate(json!({
            "passed": false,
            "checks": [
                {"name": "compilation", "passed": true},
                {"name": "tests", "passed": false, "message": "2 failures"}
            ]
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn custom_schema_registration() {
        let mut registry = SchemaRegistry::new();
        let schema = OutputSchema::from_json(json!({
            "type": "object",
            "required": ["custom_field"]
        }));
        registry.register("my_output_type", schema);
        assert!(registry.get("my_output_type").is_some());
    }

    #[test]
    fn registry_validate_with_registered_schema() {
        let mut registry = SchemaRegistry::new();
        registry.register("my_type", OutputSchema::from_json(json!({
            "type": "object",
            "required": ["value"]
        })));
        let result = registry.validate("my_type", json!({"value": 42}));
        assert!(result.is_valid());
    }

    #[test]
    fn registry_validate_unknown_type_is_uncertain() {
        let registry = SchemaRegistry::new();
        let result = registry.validate("unknown", json!({"anything": true}));
        assert!(result.is_uncertain());
    }

    #[test]
    fn validation_result_is_uncertain() {
        let result = ValidationResult::uncertain("no schema registered".to_string());
        assert!(result.is_uncertain());
        assert!(!result.is_valid());
        assert!(!result.is_invalid());
    }

    #[test]
    fn schema_for_output_type() {
        let schema = TierSchema::for_output_type(OutputType::Code);
        assert!(schema.is_some());

        let schema = TierSchema::for_output_type(OutputType::Data);
        assert!(schema.is_some());

        let schema = TierSchema::for_output_type(OutputType::Verification);
        assert!(schema.is_some());
    }
}
```

**Verify fail**:
```bash
cargo test -p rustycode-orchestration -- schema::tests 2>&1 | grep "error\[E"
# Expected: module not found
```

**Write minimal implementation**:

```rust
//! Output schema enforcement for orchestration tier outputs.
//!
//! Each tier (Composer/Editor/Musician) produces structured output. This module
//! validates that output against JSON Schema before handing off to the next tier.
//! This prevents cascading errors from malformed intermediate results.

use crate::types::OutputType;
use serde_json::Value;

/// A JSON Schema compiled for validation.
#[derive(Debug, Clone)]
pub struct OutputSchema {
    schema_json: Value,
}

impl OutputSchema {
    /// Create a schema from a pre-parsed JSON Value.
    pub fn from_json(schema: Value) -> Self {
        Self {
            schema_json: schema,
        }
    }

    /// Parse a schema from a raw JSON string.
    pub fn parse(json_str: &str) -> Result<Self, SchemaParseError> {
        let schema: Value = serde_json::from_str(json_str)
            .map_err(|e| SchemaParseError {
                message: e.to_string(),
            })?;
        Ok(Self { schema_json: schema })
    }

    /// Validate a JSON value against this schema.
    pub fn validate(&self, instance: Value) -> ValidationResult {
        // Use jsonschema crate for actual validation.
        // For the initial implementation, we perform structural validation
        // using a lightweight approach that does not require the full jsonschema
        // crate at compile time for simple schemas.
        match jsonschema::validate(&self.schema_json, &instance) {
            Ok(_) => ValidationResult::valid(),
            Err(errors) => {
                let errs: Vec<SchemaValidationError> = errors
                    .map(|e| SchemaValidationError {
                        path: e.instance_path().to_string(),
                        message: e.to_string(),
                    })
                    .collect();
                ValidationResult::invalid(errs)
            }
        }
    }

    /// Get the raw schema JSON.
    pub fn as_json(&self) -> &Value {
        &self.schema_json
    }
}

/// A single validation error.
#[derive(Debug, Clone)]
pub struct SchemaValidationError {
    /// JSON path where the error occurred.
    pub path: String,
    /// Human-readable error description.
    pub message: String,
}

/// Result of validating a JSON instance against a schema.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed.
    valid: bool,
    /// Whether the result is uncertain (no schema available).
    uncertain: bool,
    /// Validation errors (empty if valid).
    pub errors: Vec<SchemaValidationError>,
}

impl ValidationResult {
    /// Create a valid result.
    pub fn valid() -> Self {
        Self {
            valid: true,
            uncertain: false,
            errors: Vec::new(),
        }
    }

    /// Create an invalid result with errors.
    pub fn invalid(errors: Vec<SchemaValidationError>) -> Self {
        Self {
            valid: false,
            uncertain: false,
            errors,
        }
    }

    /// Create an uncertain result (no schema to validate against).
    pub fn uncertain(reason: String) -> Self {
        Self {
            valid: false,
            uncertain: true,
            errors: vec![SchemaValidationError {
                path: String::new(),
                message: reason,
            }],
        }
    }

    /// Whether validation passed.
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Whether validation definitively failed.
    pub fn is_invalid(&self) -> bool {
        !self.valid && !self.uncertain
    }

    /// Whether no schema was available for validation.
    pub fn is_uncertain(&self) -> bool {
        self.uncertain
    }

    /// Concatenated error message.
    pub fn error_message(&self) -> String {
        self.errors
            .iter()
            .map(|e| {
                if e.path.is_empty() {
                    e.message.clone()
                } else {
                    format!("{}: {}", e.path, e.message)
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Error from parsing a schema.
#[derive(Debug, thiserror::Error)]
#[error("schema parse error: {message}")]
pub struct SchemaParseError {
    pub message: String,
}

/// Registry mapping output types to their schemas.
#[derive(Debug, Clone)]
pub struct SchemaRegistry {
    schemas: std::collections::HashMap<String, OutputSchema>,
}

impl SchemaRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            schemas: std::collections::HashMap::new(),
        }
    }

    /// Register a schema for an output type name.
    pub fn register(&mut self, type_name: &str, schema: OutputSchema) {
        self.schemas.insert(type_name.to_string(), schema);
    }

    /// Get a schema by output type name.
    pub fn get(&self, type_name: &str) -> Option<&OutputSchema> {
        self.schemas.get(type_name)
    }

    /// Validate a JSON value against the schema for the given output type.
    /// Returns uncertain if no schema is registered for the type.
    pub fn validate(&self, type_name: &str, instance: Value) -> ValidationResult {
        match self.schemas.get(type_name) {
            Some(schema) => schema.validate(instance),
            None => ValidationResult::uncertain(format!(
                "no schema registered for output type '{type_name}'"
            )),
        }
    }

    /// Number of registered schemas.
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in schemas for standard tier outputs.
pub struct TierSchema;

impl TierSchema {
    /// Schema for Composer (Tier 4) plan output.
    /// Requires: steps (array of objects with description), estimated_complexity, risks.
    pub fn plan() -> OutputSchema {
        OutputSchema::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "files": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["description"]
                    },
                    "minItems": 1
                },
                "estimated_complexity": {
                    "type": "string",
                    "enum": ["easy", "medium", "hard"]
                },
                "risks": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["steps"]
        }))
    }

    /// Schema for Editor (Tier 3) code change output.
    /// Requires: files_modified (array), diff (string).
    pub fn code_change() -> OutputSchema {
        OutputSchema::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "files_modified": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "diff": { "type": "string" },
                "tests_passed": { "type": "boolean" }
            },
            "required": ["files_modified", "diff"]
        }))
    }

    /// Schema for verification output.
    /// Requires: passed (boolean), checks (array).
    pub fn verification() -> OutputSchema {
        OutputSchema::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "passed": { "type": "boolean" },
                "checks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "passed": { "type": "boolean" },
                            "message": { "type": "string" }
                        },
                        "required": ["name", "passed"]
                    }
                }
            },
            "required": ["passed", "checks"]
        }))
    }

    /// Get the default schema for an OutputType.
    pub fn for_output_type(output_type: OutputType) -> Option<OutputSchema> {
        match output_type {
            OutputType::Code => Some(Self::code_change()),
            OutputType::Verification => Some(Self::verification()),
            OutputType::Data => Some(Self::plan()),
            OutputType::Command | OutputType::File | OutputType::Query => None,
        }
    }
}
```

Add to `error.rs`:

```rust
    // Add to OrchestrationError enum:
    #[error("Schema validation error: {message}")]
    SchemaValidation { message: String },

    #[error("Judge evaluation error: {message}")]
    JudgeError { message: String },
```

And in `category()` match:

```rust
            Self::SchemaValidation { .. } | Self::JudgeError { .. } => {
                OrchestrationErrorCategory::Verification
            }
```

Add `pub mod schema;` and `pub mod judge;` to `lib.rs`.

**Verify pass**:
```bash
cargo test -p rustycode-orchestration -- schema::tests 2>&1 | tail -3
# Expected: 16 passed, 0 failed
```

**Commit**: `feat(orchestration): add output schema enforcement with JSON Schema validation`

---

### Chunk 5: LLM-as-Judge (18 tests)

**Files**: `crates/rustycode-orchestration/src/judge.rs` (new), `crates/rustycode-orchestration/src/verification_gates.rs` (edit)

#### Task 5.1: Create JudgeRubric and JudgeVerdict types

**Write failing test first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rubric_creation() {
        let rubric = JudgeRubric::new("Code Quality".to_string(), vec![
            "Does the code handle edge cases?".to_string(),
            "Is error handling comprehensive?".to_string(),
            "Are there magic numbers?".to_string(),
        ]);
        assert_eq!(rubric.name, "Code Quality");
        assert_eq!(rubric.criteria.len(), 3);
    }

    #[test]
    fn verdict_from_scores_all_passing() {
        let verdict = JudgeVerdict::from_scores(0.9, 0.85, 0.95);
        assert!(verdict.passed());
        assert_eq!(verdict.grade(), JudgeGrade::Excellent);
    }

    #[test]
    fn verdict_from_scores_failing() {
        let verdict = JudgeVerdict::from_scores(0.3, 0.4, 0.5);
        assert!(!verdict.passed());
        assert_eq!(verdict.grade(), JudgeGrade::Poor);
    }

    #[test]
    fn verdict_from_scores_borderline() {
        let verdict = JudgeVerdict::from_scores(0.6, 0.65, 0.7);
        assert!(verdict.passed()); // 0.65 >= 0.6 threshold
        assert_eq!(verdict.grade(), JudgeGrade::Good);
    }

    #[test]
    fn judge_grade_from_score() {
        assert_eq!(JudgeGrade::from_score(0.95), JudgeGrade::Excellent);
        assert_eq!(JudgeGrade::from_score(0.8), JudgeGrade::Excellent);
        assert_eq!(JudgeGrade::from_score(0.7), JudgeGrade::Good);
        assert_eq!(JudgeGrade::from_score(0.5), JudgeGrade::Fair);
        assert_eq!(JudgeGrade::from_score(0.3), JudgeGrade::Poor);
        assert_eq!(JudgeGrade::from_score(0.1), JudgeGrade::Critical);
    }

    #[test]
    fn judge_config_default_threshold() {
        let config = JudgeConfig::default();
        assert!((config.pass_threshold - 0.6).abs() < f64::EPSILON);
        assert!(!config.required); // opt-in by default
    }

    #[test]
    fn judge_config_custom_threshold() {
        let config = JudgeConfig::new(0.8, true);
        assert!((config.pass_threshold - 0.8).abs() < f64::EPSILON);
        assert!(config.required);
    }

    #[test]
    fn build_judge_prompt() {
        let rubric = JudgeRubric::new("Code Quality".to_string(), vec![
            "Handles edge cases".to_string(),
            "Error handling".to_string(),
        ]);
        let prompt = build_judge_prompt(
            "implement a binary search",
            "fn binary_search(arr: &[i32], target: i32) -> Option<usize> { ... }",
            &rubric,
        );
        assert!(prompt.contains("binary search"));
        assert!(prompt.contains("Handles edge cases"));
        assert!(prompt.contains("0.0 to 1.0"));
    }

    #[test]
    fn parse_verdict_from_json() {
        let response = serde_json::json!({
            "correctness": 0.9,
            "completeness": 0.8,
            "quality": 0.85,
            "feedback": "Good implementation with proper edge case handling.",
            "issues": ["Missing documentation for public function"]
        });
        let verdict = JudgeVerdict::parse_from_llm_response(&response.to_string()).unwrap();
        assert!(verdict.passed());
        assert!(!verdict.feedback.is_empty());
        assert_eq!(verdict.issues.len(), 1);
    }

    #[test]
    fn parse_verdict_missing_field() {
        let response = serde_json::json!({
            "correctness": 0.9,
            // missing completeness and quality
        });
        let result = JudgeVerdict::parse_from_llm_response(&response.to_string());
        assert!(result.is_err());
    }

    #[test]
    fn parse_verdict_invalid_json() {
        let result = JudgeVerdict::parse_from_llm_response("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn built_in_rubric_code_quality() {
        let rubric = BuiltInRubrics::code_quality();
        assert!(!rubric.criteria.is_empty());
        assert!(rubric.name.contains("Code"));
    }

    #[test]
    fn built_in_rubric_plan_quality() {
        let rubric = BuiltInRubrics::plan_quality();
        assert!(!rubric.criteria.is_empty());
    }

    #[test]
    fn built_in_rubric_verification() {
        let rubric = BuiltInRubrics::verification();
        assert!(!rubric.criteria.is_empty());
    }

    #[test]
    fn verdict_serialization_roundtrip() {
        let verdict = JudgeVerdict::from_scores(0.8, 0.7, 0.9);
        let json = serde_json::to_string(&verdict).unwrap();
        let back: JudgeVerdict = serde_json::from_str(&json).unwrap();
        assert!((back.correctness - 0.8).abs() < f64::EPSILON);
        assert!((back.completeness - 0.7).abs() < f64::EPSILON);
        assert!((back.quality - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn judge_rubric_serialization_roundtrip() {
        let rubric = JudgeRubric::new("Test".to_string(), vec!["Criterion 1".to_string()]);
        let json = serde_json::to_string(&rubric).unwrap();
        let back: JudgeRubric = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Test");
        assert_eq!(back.criteria.len(), 1);
    }
}
```

**Verify fail**:
```bash
cargo test -p rustycode-orchestration -- judge::tests 2>&1 | grep "error\[E"
# Expected: module not found
```

**Write minimal implementation**:

```rust
//! LLM-as-Judge: optional second model evaluates output quality.
//!
//! A judge uses a rubric with explicit criteria to score the output of a tier
//! on three axes: correctness, completeness, and quality. The verdict determines
//! whether the output passes or needs rework.
//!
//! This is an **optional** evaluation tier. It is opt-in by default (not enabled
//! unless configured). When enabled, it adds a second LLM call after each tier
//! execution to catch semantic errors that rule-based verification gates miss.

use serde::{Deserialize, Serialize};

/// Configuration for the LLM-as-Judge evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeConfig {
    /// Score threshold (0.0-1.0) for a passing verdict. Default: 0.6.
    pub pass_threshold: f64,
    /// Whether the judge is required (fails the step on negative verdict)
    /// or advisory (logs the verdict but does not block).
    pub required: bool,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            pass_threshold: 0.6,
            required: false,
        }
    }
}

impl JudgeConfig {
    /// Create a new config with a custom threshold.
    pub fn new(pass_threshold: f64, required: bool) -> Self {
        Self {
            pass_threshold: pass_threshold.clamp(0.0, 1.0),
            required,
        }
    }
}

/// A rubric defines the evaluation criteria for judging output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeRubric {
    /// Name of the rubric (e.g., "Code Quality", "Plan Quality").
    pub name: String,
    /// Evaluation criteria. Each is a question or statement the judge scores.
    pub criteria: Vec<String>,
}

impl JudgeRubric {
    /// Create a new rubric.
    pub fn new(name: String, criteria: Vec<String>) -> Self {
        Self { name, criteria }
    }
}

/// The verdict from judging an output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeVerdict {
    /// Correctness score (0.0-1.0): does the output do what was asked?
    pub correctness: f64,
    /// Completeness score (0.0-1.0): does the output cover all requirements?
    pub completeness: f64,
    /// Quality score (0.0-1.0): is the output well-structured and maintainable?
    pub quality: f64,
    /// Human-readable feedback from the judge.
    #[serde(default)]
    pub feedback: String,
    /// Specific issues found by the judge.
    #[serde(default)]
    pub issues: Vec<String>,
    /// The rubric used for this evaluation.
    pub rubric_name: String,
}

impl JudgeVerdict {
    /// Create a verdict from three scores.
    pub fn from_scores(correctness: f64, completeness: f64, quality: f64) -> Self {
        Self {
            correctness: correctness.clamp(0.0, 1.0),
            completeness: completeness.clamp(0.0, 1.0),
            quality: quality.clamp(0.0, 1.0),
            feedback: String::new(),
            issues: Vec::new(),
            rubric_name: String::new(),
        }
    }

    /// Whether the verdict passes the given threshold.
    pub fn passed_with(&self, threshold: f64) -> bool {
        self.weighted_score() >= threshold
    }

    /// Whether the verdict passes the default threshold (0.6).
    pub fn passed(&self) -> bool {
        self.passed_with(0.6)
    }

    /// Weighted overall score: correctness 40%, completeness 30%, quality 30%.
    pub fn weighted_score(&self) -> f64 {
        self.correctness * 0.4 + self.completeness * 0.3 + self.quality * 0.3
    }

    /// Letter grade based on weighted score.
    pub fn grade(&self) -> JudgeGrade {
        JudgeGrade::from_score(self.weighted_score())
    }

    /// Parse a verdict from an LLM response JSON string.
    /// Expected format: {"correctness": 0.0-1.0, "completeness": 0.0-1.0,
    ///                    "quality": 0.0-1.0, "feedback": "...", "issues": [...]}
    pub fn parse_from_llm_response(json_str: &str) -> Result<Self, JudgeParseError> {
        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| JudgeParseError {
                message: format!("invalid JSON: {e}"),
            })?;

        let correctness = parsed.get("correctness")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| JudgeParseError {
                message: "missing or invalid 'correctness' field".to_string(),
            })?;

        let completeness = parsed.get("completeness")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| JudgeParseError {
                message: "missing or invalid 'completeness' field".to_string(),
            })?;

        let quality = parsed.get("quality")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| JudgeParseError {
                message: "missing or invalid 'quality' field".to_string(),
            })?;

        let feedback = parsed.get("feedback")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let issues = parsed.get("issues")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            correctness: correctness.clamp(0.0, 1.0),
            completeness: completeness.clamp(0.0, 1.0),
            quality: quality.clamp(0.0, 1.0),
            feedback,
            issues,
            rubric_name: String::new(),
        })
    }
}

/// Error from parsing a judge verdict.
#[derive(Debug, thiserror::Error)]
#[error("judge parse error: {message}")]
pub struct JudgeParseError {
    pub message: String,
}

/// Letter grade for a judge verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JudgeGrade {
    Excellent,
    Good,
    Fair,
    Poor,
    Critical,
}

impl JudgeGrade {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::Excellent
        } else if score >= 0.65 {
            Self::Good
        } else if score >= 0.5 {
            Self::Fair
        } else if score >= 0.3 {
            Self::Poor
        } else {
            Self::Critical
        }
    }
}

/// Build the prompt sent to the judge LLM.
pub fn build_judge_prompt(
    task_description: &str,
    output: &str,
    rubric: &JudgeRubric,
) -> String {
    let criteria_list = rubric.criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"You are a quality judge evaluating AI-generated output.

## Task
{task_description}

## Output to Evaluate
{output}

## Evaluation Rubric: {rubric_name}
{criteria_list}

## Instructions
Score the output on three axes, each from 0.0 to 1.0:
- **correctness**: Does the output correctly accomplish the task?
- **completeness**: Does the output cover all requirements?
- **quality**: Is the output well-structured, clean, and maintainable?

Respond with ONLY a JSON object:
{{"correctness": <0.0-1.0>, "completeness": <0.0-1.0>, "quality": <0.0-1.0>, "feedback": "<brief feedback>", "issues": ["<issue1>", ...]}}"#,
        rubric_name = rubric.name,
    )
}

/// Built-in rubrics for common evaluation scenarios.
pub struct BuiltInRubrics;

impl BuiltInRubrics {
    /// Rubric for evaluating code output quality.
    pub fn code_quality() -> JudgeRubric {
        JudgeRubric::new("Code Quality".to_string(), vec![
            "Does the code handle all edge cases mentioned in the task?".to_string(),
            "Is error handling comprehensive with proper error types?".to_string(),
            "Are there magic numbers or hardcoded values that should be constants?".to_string(),
            "Is the code readable with clear naming and structure?".to_string(),
            "Does the code follow the project's coding standards?".to_string(),
            "Are there appropriate tests for the new functionality?".to_string(),
        ])
    }

    /// Rubric for evaluating plan output quality.
    pub fn plan_quality() -> JudgeRubric {
        JudgeRubric::new("Plan Quality".to_string(), vec![
            "Does the plan address all requirements from the task?".to_string(),
            "Are the steps ordered logically with clear dependencies?".to_string(),
            "Are potential risks identified with mitigation strategies?".to_string(),
            "Is the estimated complexity reasonable?".to_string(),
            "Are the files to modify correctly identified?".to_string(),
        ])
    }

    /// Rubric for evaluating verification output quality.
    pub fn verification() -> JudgeRubric {
        JudgeRubric::new("Verification Quality".to_string(), vec![
            "Are all verification checks actually run?".to_string(),
            "Do the check results accurately reflect the state?".to_string(),
            "Are failure messages specific and actionable?".to_string(),
            "Is the overall pass/fail assessment correct?".to_string(),
        ])
    }
}
```

**Verify pass**:
```bash
cargo test -p rustycode-orchestration -- judge::tests 2>&1 | tail -3
# Expected: 18 passed, 0 failed
```

**Commit**: `feat(orchestration): add LLM-as-Judge with rubric-based scoring`

---

#### Task 5.2: Integrate JudgeStrategy into verification gates

**File**: `crates/rustycode-orchestration/src/verification_gates.rs` (edit)

**Write failing test first** (in `verification_gates.rs` tests):

```rust
#[test]
fn judge_strategy_returns_valid_on_pass() {
    use crate::judge::{JudgeVerdict, JudgeConfig};
    let verdict = JudgeVerdict::from_scores(0.9, 0.85, 0.8);
    let strategy = JudgeVerificationStrategy::new(verdict, JudgeConfig::default());
    let step = make_step(OutputType::Code);
    let entry = make_entry("good output", Some(0));
    let outcome = strategy.verify(&step, &entry);
    assert!(matches!(outcome, VerificationOutcome::Valid));
}

#[test]
fn judge_strategy_returns_invalid_on_fail() {
    use crate::judge::{JudgeVerdict, JudgeConfig};
    let verdict = JudgeVerdict::from_scores(0.3, 0.2, 0.3);
    let mut config = JudgeConfig::default();
    config.required = true;
    let strategy = JudgeVerificationStrategy::new(verdict, config);
    let step = make_step(OutputType::Code);
    let entry = make_entry("poor output", Some(0));
    let outcome = strategy.verify(&step, &entry);
    assert!(matches!(outcome, VerificationOutcome::Invalid { .. }));
}

#[test]
fn schema_strategy_returns_valid_on_match() {
    let schema = crate::schema::OutputSchema::from_json(serde_json::json!({
        "type": "object",
        "required": ["status"]
    }));
    let strategy = SchemaVerificationStrategy::new(schema);
    let step = make_step(OutputType::Data);
    let entry = make_entry(r#"{"status": "ok"}"#, Some(0));
    let outcome = strategy.verify(&step, &entry);
    assert!(matches!(outcome, VerificationOutcome::Valid));
}

#[test]
fn schema_strategy_returns_invalid_on_mismatch() {
    let schema = crate::schema::OutputSchema::from_json(serde_json::json!({
        "type": "object",
        "required": ["status"]
    }));
    let strategy = SchemaVerificationStrategy::new(schema);
    let step = make_step(OutputType::Data);
    let entry = make_entry(r#"{"missing": true}"#, Some(0));
    let outcome = strategy.verify(&step, &entry);
    assert!(matches!(outcome, VerificationOutcome::Invalid { .. }));
}
```

**Verify fail**:
```bash
cargo test -p rustycode-orchestration -- verification_gates::tests::judge_strategy 2>&1 | tail -5
# Expected: cannot find type JudgeVerificationStrategy
```

**Write minimal implementation** (add to `verification_gates.rs`):

```rust
use crate::judge::{JudgeConfig, JudgeVerdict};
use crate::schema::OutputSchema;

/// Verification strategy that uses a pre-computed LLM-as-Judge verdict.
pub struct JudgeVerificationStrategy {
    verdict: JudgeVerdict,
    config: JudgeConfig,
}

impl JudgeVerificationStrategy {
    pub fn new(verdict: JudgeVerdict, config: JudgeConfig) -> Self {
        Self { verdict, config }
    }
}

impl VerificationStrategy for JudgeVerificationStrategy {
    fn verify(&self, _step: &Step, _result: &TraceEntry) -> VerificationOutcome {
        if self.verdict.passed_with(self.config.pass_threshold) {
            VerificationOutcome::Valid
        } else if self.config.required {
            VerificationOutcome::Invalid {
                reason: format!(
                    "Judge verdict failed: score {:.2} < threshold {:.2}. Feedback: {}",
                    self.verdict.weighted_score(),
                    self.config.pass_threshold,
                    self.verdict.feedback,
                ),
                category: SignalCategory::LogicError,
            }
        } else {
            // Advisory: log but pass
            VerificationOutcome::Valid
        }
    }
}

/// Verification strategy that validates output against a JSON Schema.
pub struct SchemaVerificationStrategy {
    schema: OutputSchema,
}

impl SchemaVerificationStrategy {
    pub fn new(schema: OutputSchema) -> Self {
        Self { schema }
    }
}

impl VerificationStrategy for SchemaVerificationStrategy {
    fn verify(&self, _step: &Step, result: &TraceEntry) -> VerificationOutcome {
        let parsed: serde_json::Result<serde_json::Value> =
            serde_json::from_str(&result.output);
        match parsed {
            Ok(instance) => {
                let validation = self.schema.validate(instance);
                if validation.is_valid() {
                    VerificationOutcome::Valid
                } else {
                    VerificationOutcome::Invalid {
                        reason: format!(
                            "Output schema validation failed: {}",
                            validation.error_message()
                        ),
                        category: SignalCategory::TypeError,
                    }
                }
            }
            Err(e) => VerificationOutcome::Invalid {
                reason: format!("Output is not valid JSON: {e}"),
                category: SignalCategory::SyntaxError,
            },
        }
    }
}
```

**Verify pass**:
```bash
cargo test -p rustycode-orchestration -- verification_gates::tests::judge_strategy 2>&1 | tail -3
# Expected: 2 passed
cargo test -p rustycode-orchestration -- verification_gates::tests::schema_strategy 2>&1 | tail -3
# Expected: 2 passed
```

**Commit**: `feat(orchestration): integrate JudgeStrategy and SchemaStrategy into verification gates`

---

### Chunk 6: Cross-Module Integration Tests (8 tests)

**File**: `crates/rustycode-orchestration/tests/phase6_integration.rs` (new)

#### Task 6.1: End-to-end integration tests

**Write failing test first**:

```rust
//! Phase 6 integration tests: skill authoring + quality pipeline.

use rustycode_orchestration::verification_gates::{
    VerificationGateRegistry, VerificationOutcome, VerificationStrategy,
};
use rustycode_orchestration::execution_trace::TraceEntry;
use rustycode_orchestration::types::{OutputType, Step};

fn make_step(output_type: OutputType) -> Step {
    Step {
        id: "s1".into(),
        index: 0,
        description: "test".into(),
        expected_output_type: output_type,
        suggested_tool: None,
        retry_on_failure: false,
        required_resources: rustycode_orchestration::guard::RequiredResources::default(),
    }
}

fn make_entry(output: &str, exit_code: Option<i32>) -> TraceEntry {
    TraceEntry::new_success(
        "s1".into(), 0, 2, "test".into(),
        serde_json::json!({}), output.into(), exit_code, 0.0,
    )
}

#[test]
fn schema_validation_catches_malformed_plan_output() {
    let schema = rustycode_orchestration::schema::TierSchema::plan();
    let result = schema.validate(serde_json::json!({
        // missing "steps" field
        "estimated_complexity": "medium"
    }));
    assert!(!result.is_valid());
}

#[test]
fn schema_validation_passes_well_formed_plan_output() {
    let schema = rustycode_orchestration::schema::TierSchema::plan();
    let result = schema.validate(serde_json::json!({
        "steps": [{"description": "Do the thing"}],
        "estimated_complexity": "easy",
        "risks": ["None identified"]
    }));
    assert!(result.is_valid());
}

#[test]
fn schema_validation_catches_malformed_code_output() {
    let schema = rustycode_orchestration::schema::TierSchema::code_change();
    let result = schema.validate(serde_json::json!({
        // missing "files_modified"
        "diff": "some diff"
    }));
    assert!(!result.is_valid());
}

#[test]
fn judge_verdict_weighted_scoring() {
    use rustycode_orchestration::judge::{JudgeVerdict, JudgeGrade};
    let verdict = JudgeVerdict::from_scores(0.8, 0.7, 0.9);
    // weighted: 0.8*0.4 + 0.7*0.3 + 0.9*0.3 = 0.32 + 0.21 + 0.27 = 0.80
    let weighted = verdict.weighted_score();
    assert!((weighted - 0.80).abs() < 0.01);
    assert_eq!(verdict.grade(), JudgeGrade::Excellent);
}

#[test]
fn judge_verdict_parse_roundtrip() {
    use rustycode_orchestration::judge::JudgeVerdict;
    let json = r#"{"correctness": 0.85, "completeness": 0.7, "quality": 0.9, "feedback": "Good work", "issues": ["minor style issue"]}"#;
    let verdict = JudgeVerdict::parse_from_llm_response(json).unwrap();
    assert!((verdict.correctness - 0.85).abs() < f64::EPSILON);
    assert!(verdict.passed());
    assert_eq!(verdict.issues.len(), 1);
}

#[test]
fn schema_registry_with_built_in_tier_schemas() {
    use rustycode_orchestration::schema::SchemaRegistry;
    use rustycode_orchestration::schema::TierSchema;

    let mut registry = SchemaRegistry::new();
    registry.register("plan", TierSchema::plan());
    registry.register("code_change", TierSchema::code_change());
    registry.register("verification", TierSchema::verification());

    assert_eq!(registry.len(), 3);

    let plan_result = registry.validate("plan", serde_json::json!({
        "steps": [{"description": "Do X"}]
    }));
    assert!(plan_result.is_valid());

    let unknown_result = registry.validate("unknown", serde_json::json!({}));
    assert!(unknown_result.is_uncertain());
}

#[test]
fn full_verification_pipeline_with_schema() {
    let mut registry = VerificationGateRegistry::new();

    let schema = rustycode_orchestration::schema::TierSchema::verification();
    registry.register_strategy(
        OutputType::Verification,
        Box::new(rustycode_orchestration::verification_gates::SchemaVerificationStrategy::new(schema)),
    );

    let valid_step = make_step(OutputType::Verification);
    let valid_entry = make_entry(
        r#"{"passed": true, "checks": [{"name": "compile", "passed": true}]}"#,
        Some(0),
    );
    assert!(matches!(
        registry.verify(&valid_step, &valid_entry),
        VerificationOutcome::Valid
    ));

    let invalid_entry = make_entry(r#"{"not": "valid"}"#, Some(0));
    assert!(matches!(
        registry.verify(&valid_step, &invalid_entry),
        VerificationOutcome::Invalid { .. }
    ));
}

#[test]
fn judge_and_schema_strategies_combined() {
    use rustycode_orchestration::judge::{JudgeConfig, JudgeVerdict};
    use rustycode_orchestration::verification_gates::JudgeVerificationStrategy;

    let mut registry = VerificationGateRegistry::new();

    // Add schema strategy
    let schema = rustycode_orchestration::schema::TierSchema::code_change();
    registry.register_strategy(
        OutputType::Code,
        Box::new(rustycode_orchestration::verification_gates::SchemaVerificationStrategy::new(schema)),
    );

    // Add judge strategy (passing verdict)
    let verdict = JudgeVerdict::from_scores(0.9, 0.85, 0.9);
    registry.register_strategy(
        OutputType::Code,
        Box::new(JudgeVerificationStrategy::new(verdict, JudgeConfig::default())),
    );

    let step = make_step(OutputType::Code);
    let entry = make_entry(
        r#"{"files_modified": ["src/lib.rs"], "diff": "+added"}"#,
        Some(0),
    );
    let outcome = registry.verify(&step, &entry);
    assert!(matches!(outcome, VerificationOutcome::Valid));
}
```

**Verify pass**:
```bash
cargo test -p rustycode-orchestration --test phase6_integration 2>&1 | tail -3
# Expected: 8 passed, 0 failed
```

**Commit**: `test(orchestration): add Phase 6 integration tests`

---

## Test Count Summary

| Chunk | Module | Tests | Category |
|-------|--------|-------|----------|
| 1 | `exclusions.rs` | 5 | ExclusionClauseSet parsing + matching |
| 1 | `types.rs` edit | 1 | SkillDefinition field addition |
| 1 | `activation.rs` edit | 2 | Exclusion scoring integration |
| 2 | `gotchas.rs` | 13 | Gotcha/GotchaSet parsing + matching |
| 2 | `metadata.rs` edit | 1 | Frontmatter gotchas parsing |
| 3 | `checklist.rs` | 12 | Checklist generation + formatting |
| 4 | `schema.rs` | 16 | OutputSchema + TierSchema + SchemaRegistry |
| 5 | `judge.rs` | 18 | JudgeRubric + JudgeVerdict + BuiltInRubrics |
| 5 | `verification_gates.rs` edit | 4 | JudgeStrategy + SchemaStrategy |
| 6 | Integration tests | 8 | End-to-end pipeline |
| **Total** | | **80** | |

---

## Build Verification Commands

After all chunks are complete, verify the full workspace:

```bash
# Format check
cargo fmt --check

# Clippy (strict: warnings as errors)
cargo clippy -p rustycode-skill -p rustycode-orchestration --all-targets -- -D warnings

# Unit tests (skill crate)
cargo test -p rustycode-skill

# Unit tests (orchestration crate)
cargo test -p rustycode-orchestration

# Integration tests
cargo test -p rustycode-orchestration --test phase6_integration

# Full workspace (verify no regressions)
cargo test --workspace
```

Expected: 0 clippy warnings, 0 test failures, all 80 new tests passing.

---

## Dependency Changes

### `crates/rustycode-skill/Cargo.toml`

Add:
```toml
thiserror.workspace = true
```

### `crates/rustycode-orchestration/Cargo.toml`

Add:
```toml
jsonschema = "0.26"
```

---

## Implementation Notes

### Exclusion Clause Scoring

The exclusion penalty (-5.0) is deliberately heavier than any positive scoring contribution. This ensures that if a skill explicitly says "do not use for X", it takes a very high-quality match on all other axes to overcome the penalty. The threshold of 0.3 in `evaluate_for_context` means the score must exceed 0.3 after the penalty, which effectively filters most excluded skills.

### Gotcha Matching Strategy

Gotchas use a two-tier matching system:
1. **Keyword-based**: If the skill author provided explicit keywords, match only those (precise).
2. **Description-word-based**: If no keywords, match significant words (length > 3) from the gotcha description (fuzzy but reasonable).

When no gotchas match the context, all gotchas are surfaced as general warnings. This prevents the agent from walking into known traps just because the context didn't mention specific keywords.

### Checklist Generation Pipeline

```
ProcedureKind::Pipeline { stages }
    |
    v
PipelineStage { name, description }
    |
    v
ChecklistItem { description: "Name: Description" }
    |
    v
Checklist { items, format_markdown() }
    |
    v
Agent output: "- [ ] Write Tests: Write unit tests first\n- [ ] Implement: ..."
```

### LLM-as-Judge Flow

```
Tier output
    |
    v
JudgeVerificationStrategy::verify()
    |
    v
pre-computed JudgeVerdict { correctness, completeness, quality }
    |
    v
If config.required && verdict.weighted_score() < config.pass_threshold:
    return Invalid (blocks pipeline)
Else:
    return Valid (advisory logging for non-required)
```

The judge verdict is pre-computed (the LLM call happens before the verification gate). This separation means the verification gate is synchronous and deterministic, while the LLM evaluation is async and optional.

### Schema Validation Strategy

```
TraceEntry.output (string)
    |
    v
serde_json::from_str -> JSON Value
    |
    v
jsonschema::validate(schema, instance)
    |
    v
Valid / Invalid { reason, category: TypeError }
```

Output schemas are registered per `OutputType`. The `TierSchema` built-in schemas provide sensible defaults for Code, Data, and Verification output types. Custom schemas can be registered via `SchemaRegistry::register()`.

---

## Future Enhancements (Post-Phase 6)

1. **Judge prompt caching**: Cache the judge prompt template to avoid rebuilding it every call.
2. **Rubric inference**: Auto-generate rubrics from the task description when no explicit rubric is available.
3. **Gotcha telemetry**: Track which gotchas are triggered and whether they prevented failures. Use this to improve gotcha quality.
4. **Checklist persistence**: Persist checklist state across sessions so interrupted workflows can resume.
5. **Schema evolution**: Version schemas so that old outputs can be migrated to new formats.
6. **Multi-model judging**: Use a different (cheaper) model for judging than for execution to reduce cost.
