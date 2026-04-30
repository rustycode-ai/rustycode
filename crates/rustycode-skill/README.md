# rustycode-skill

Skill discovery and loading system for RustyCode.

## Purpose

Manages discovery, loading, caching, and lifecycle of "skills" — reusable workflows and tool libraries defined in YAML frontmatter within markdown files. Skills encapsulate workflows (TDD, debugging, code review), tool bundles, and best practices that Claude agents can activate and follow.

## Key Types

- `Skill` — Discovered skill with name, path, and description
- `SkillDefinition` — Complete skill data from YAML frontmatter
- `SkillManager` — Central manager for discovery, caching, and activation
- `SkillRegistry` — Registry of all available skills with metadata
- `SkillWorkflow` — Structured, enforceable workflow (e.g., TDD: RED→GREEN→REFACTOR)
- `SkillActivation` — Request to activate a skill with parameters

## Public API

```rust
use rustycode_skill::{SkillManager, SkillRegistry};

// Create and initialize manager
let manager = SkillManager::new("/path/to/skills")?;

// Discover all skills from SKILL.md files
let registry = manager.discover().await?;

// List available skills
for skill in registry.list_skills() {
    println!("{}: {}", skill.name, skill.description.unwrap_or_default());
}

// Load specific skill (metadata-only, cached)
if let Some(skill) = manager.get_skill("tdd-guide").await? {
    println!("Skill: {}", skill.name);
}

// Activate skill with full content for execution
let content = manager.load_full_skill("tdd-guide").await?;
```

## Skill Format

Skills are defined via YAML frontmatter in markdown files:

```markdown
---
name: tdd-guide
description: Test-driven development workflow
type: workflow
keywords: [testing, tdd, development]
version: 1.0.0
---

# TDD Workflow

This skill enforces the TDD cycle...

## Steps

1. RED - Write failing test
2. GREEN - Implement to pass
3. REFACTOR - Clean up code
```

## Features

- **Metadata-Only Loading** — First load reads YAML frontmatter only (fast)
- **On-Demand Content** — Full markdown loaded when needed
- **TTL-Based Caching** — Automatic cache invalidation
- **Relevance Scoring** — Select most relevant skills for task
- **Progressive Discovery** — Lazy-load skill directories
- **Workflow Enforcement** — Structured steps that must be followed

## Skill Types

- **workflow** — Enforceable workflow (TDD, debugging, code review)
- **tool-bundle** — Collection of related tools
- **pattern** — Best practice or design pattern
- **guide** — Guidance on a specific topic
- **custom** — User-defined skill type

## Dependencies

- `tokio` — Async file I/O
- `serde` — Serialization
- `regex` — YAML frontmatter parsing
- `tracing` — Logging
- `anyhow` — Error handling

## Architecture Notes

The skill system uses progressive loading: metadata discovery is fast (YAML parsing), full content loading only happens when a skill is activated. Caching reduces repeated reads. A file watcher detects changes and invalidates cache entries.

Skills are discovered from all SKILL.md files in registered directories. Frontmatter is parsed once and cached. Relevance scoring uses keyword matching and context to suggest the most applicable skills.

## Testing

Tests verify YAML parsing, discovery from multiple directories, caching behavior, and relevance scoring. Mock file systems test edge cases without disk I/O.

## See Also

- `rustycode-plugins` — Plugin system (similar but different use case)
- `rustycode-tools-registry` — Tool discovery (related pattern)
- `rustycode-tui` — TUI integration for skill activation
