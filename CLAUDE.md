# CLAUDE.md — RustyCode Development Guide

This file provides guidance to anyone (human or AI) working with the RustyCode codebase.

## Project Overview

RustyCode is an AI-powered autonomous development framework built in Rust. It provides an interactive TUI, a CLI, and an autonomous development mode (Autonomous Mode) with multi-provider LLM support.

**Repository (dev)**: https://github.com/luengnat/rustycode
**Releases**: https://github.com/rustycode-ai/rustycode
**Install page**: https://rustycode-ai.github.io/
**License**: MIT
**Rust Edition**: 2021
**Current Release**: v0.4.0
**Minimum Rust Version**: See `Cargo.toml` (MSRV not formally specified; use latest stable)

## Architecture Status

See `/docs/architecture/ARCHITECTURE-REVIEW-2026-04-20.md` for the full analysis.

**Resolved P0 issues:** Unified `LLMProvider` trait, `CheckpointRecovery` complete, orchestration crate consolidation (deep-thinker/orchestra merged into `rustycode-orchestration`), module splits (bus/events, llm/openai, llm/anthropic, git/*, storage/*).

**Resolved P0:** Circular dependency `rustycode-llm` ↔ `rustycode-tools` — broken by `rustycode-tool-integration` shim crate. Both crates now depend on the shim (which provides `ToolExecutorApi`, `ToolInfo`, `TokenCounter`, `CostTracker`) and build/test independently.

**P2 (pending):** God objects in `rustycode-tui`, `rustycode-core`, `rustycode-tools` — keep changes localized, don't add dependencies, prefer extracting to new crates.

**Known build issue:** None — workspace members are clean.

## Orchestration — Responsibility Boundary

`rustycode-orchestration` is the single canonical crate for autonomous execution algorithms. It owns execution strategies, reasoning loops, quality gates, the AST pipeline, tiered model execution, and prompt context management.

**Dependency direction:** CLI/TUI depend on orchestration. Orchestration never depends on CLI/TUI.

**When adding new code, ask:**
- Does it know about terminals, disk paths, or user sessions? → `rustycode-cli` or `rustycode-tui`
- Does it know about tasks, reasoning, or model tiers? → `rustycode-orchestration`
- Does it know about both? → it needs to be split

See `crates/rustycode-orchestration/README.md` for full module map.

## Repository Structure

### Key Crates

| Crate | Purpose |
|-------|---------|
| `rustycode-cli` | CLI binary (default workspace member) |
| `rustycode-tui` | Terminal UI (ratatui-based) |
| `rustycode-core` | Session management, headless runtime |
| `rustycode-orchestration` | Autonomous execution: strategies, reasoning, quality gates, AST pipeline, milestone sequencing |
| `rustycode-llm` | LLM provider abstractions (Anthropic, OpenAI, etc.) |
| `rustycode-tools` | Tool execution framework + permissions |
| `rustycode-protocol` | Cross-crate shared types (Milestone, Plan, PlanDependency, Message, ToolCall) |
| `rustycode-bus` | Event bus (pub/sub) |
| `rustycode-agent-runtime` | Agent definitions (headless) |
| `rustycode-bench` | rtk-bench: native/Docker benchmark runner |
| `rustycode-team` | Team-based agent orchestration, task profiling, coordination |
| `rustycode-sandbox` | OS-level sandboxing (macOS Seatbelt, Linux landlock) |

Other top-level dirs: `docs/`, `scripts/`, `tests/`, `benches/`, `examples/`.

Excluded from workspace: `crates/rustycode-web/` (separate web/WASM build).

## Build & Run

```bash
# Build (CLI + TUI binary)
cargo build --release

# Build all workspace members
cargo build --workspace --all-targets

# Run CLI
cargo run -p rustycode-cli -- [args]

# Run TUI
cargo run -p rustycode-cli -- tui

# Run all tests
cargo test --workspace

# Run a single test
cargo test -p rustycode-llm test_name
cargo test -p rustycode-core --test integration_test_name

# Run tests for one crate
cargo test -p rustycode-orchestration

# Run clippy (CI enforces this)
cargo clippy --workspace --all-targets -- -D warnings

# Format check
cargo fmt --check
```

## Coding Standards

### Lint Configuration

The workspace enforces strict lints via `Cargo.toml`:

- **Clippy pedantic + nursery**: Enabled as warnings
- **`unwrap_used` / `expect_used`**: Warn (use `?` or `.context()`)
- **`unsafe_code`**: Forbidden (must opt-in per crate with `#![allow(unsafe_code)]`)
- **`dead_code`**: Warn (CI will flag unused items)

Allowed lints (with documented rationale in `Cargo.toml`):
- `type_complexity`, `too_many_arguments`, `module_inception`
- `upper_case_acronyms`, `wildcard_imports`, `must_use_candidate`
- `cast_possible_truncation`, `cast_sign_loss`
- `missing_errors_doc`, `missing_panics_doc`

### Error Handling

**Use `anyhow` for application code, `thiserror` for library error types.**

```rust
use anyhow::{Context, Result};

// Always provide context for errors
fn read_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config from {}", path.display()))?;
    Ok(toml::from_str(&content)?)
}
```

For crate-level error types:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BusError {
    #[error("channel closed")]
    ChannelClosed,
    #[error("handler not found: {0}")]
    HandlerNotFound(String),
}
```

### Secrets & API Keys

**Always use `secrecy::SecretString` for API keys and tokens.** Never log or display raw secrets.

```rust
use secrecy::SecretString;

pub struct ProviderConfig {
    pub api_key: Option<SecretString>,
}
```

The `sanitize_for_log()` function in `rustycode-tools/src/security.rs` strips API key patterns from log output.

**Never commit real API keys.** The `.gitignore` blocks `.env`, `credentials.json`, `config.json`. The `.gitleaks.toml` config provides pre-commit secret scanning.

### Async Patterns

- Use `tokio` for all async operations
- Use `tokio::fs` over `std::fs` in async contexts
- Use `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared state
- Prefer `tokio::sync` primitives over `std::sync` in async code

### Module Organization

```rust
// lib.rs — re-export public API
pub mod config;
pub mod error;
pub mod types;

pub use config::Config;
pub use error::{Error, Result};
```

### Testing

- Inline `#[cfg(test)] mod tests` for unit tests within the source file
- Separate `tests/` directory for integration tests
- Use `#[tokio::test]` for async tests
- Benchmark with Criterion (`benches/`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parsing() {
        let config = Config::parse("key = \"value\"").unwrap();
        assert_eq!(config.key, "value");
    }
}
```

### Adding Dependencies

1. Add to the crate's `Cargo.toml`
2. If the dependency is shared across multiple crates, add it to the workspace `[workspace.dependencies]` section and reference it as `dep.workspace = true`
3. Prefer async-compatible crates (tokio-based)

## Architecture

### Layer Diagram

```
┌─────────────────────────────────────────────────────────────┐
│  rustycode-cli (CLI + TUI binary) / rustycode-guard (binary) │
├─────────────────────────────────────────────────────────────┤
│  rustycode-core (session, headless runtime)                 │
│  rustycode-orchestration (autonomous development, milestones)│
├─────────────────────────────────────────────────────────────┤
│  rustycode-llm (providers)  │  rustycode-tools             │
│  rustycode-bus (events)      │  rustycode-guard (security)  │
│  rustycode-team (agents)     │  rustycode-sandbox (isolation)│
├─────────────────────────────────────────────────────────────┤
│  rustycode-protocol  │  rustycode-config  │  rustycode-skill │
│  rustycode-storage   │  rustycode-auth     │  rustycode-session│
└─────────────────────────────────────────────────────────────┘
```

### Inter-Crate Communication

- **Types**: Use types from `rustycode-protocol` for cross-crate messages
- **Events**: Use `rustycode-bus::EventBus` for pub/sub
- **Tools**: Use `rustycode-tools-api` trait definitions
- **LLM**: Use `rustycode-llm::LLMProvider` trait
- **Config**: Use `rustycode-config` for loading configuration
- **Skills**: Use `rustycode-skill` for skill discovery and YAML frontmatter

### Key Traits

| Trait | Crate | Purpose |
|-------|-------|---------|
| `LLMProvider` | `rustycode-llm` | Unified LLM provider abstraction |
| `ToolExecutor` | `rustycode-tools-api` | Tool execution interface |
| `EventHandler` | `rustycode-bus` | Event subscription |

## Security

- **Permission system**: `rustycode-tools/src/security.rs` validates all file/command operations
- **Path validation**: Blocks `.env`, `credentials.json`, and sensitive file access
- **Command validation**: `rustycode-tools/src/bash.rs` validates shell commands before execution
- **Secret sanitization**: API keys are stripped from logs and debug output
- **Pre-commit hooks**: `.pre-commit-config.yaml` runs gitleaks for secret detection

## Common Tasks

### Adding a new LLM provider

1. Create a new file in `crates/rustycode-llm/src/` (e.g., `my_provider.rs`)
2. Implement the `LLMProvider` or `Provider` trait
3. Register in `crates/rustycode-llm/src/lib.rs`
4. Add provider config in `crates/rustycode-llm/src/provider.rs`
5. Add tests following existing provider test patterns

### Adding a new tool

1. Define the tool in `crates/rustycode-tools/src/`
2. Implement the tool trait from `rustycode-tools-api`
3. Register in `crates/rustycode-tools/src/lib.rs`
4. Add security validation if the tool touches files or runs commands
5. Add tests

### Adding a new crate

1. Create `crates/rustycode-newcrate/` with `Cargo.toml` and `src/lib.rs`
2. Add to workspace `members` in root `Cargo.toml` (one per line)
3. Add `lints.workspace = true` to the crate's `Cargo.toml`
4. Use workspace dependencies where possible (`dep.workspace = true`)
5. Add README.md explaining purpose, API, integration point, and examples
6. Document what crates depend on it and what it depends on

## Architecture Guidance

### Known Issues & Workarounds

**Circular Dependency: `rustycode-llm` ↔ `rustycode-tools`** — **Resolved.**
- Broken by `rustycode-tool-integration` shim crate providing shared traits (`ToolExecutorApi`, `ToolInfo`, `TokenCounter`, `CostTracker`)
- Both crates build and test independently
- **When modifying:** no special coordination needed between these two crates

**Provider trait:** Use `rustycode-llm::LLMProvider` (consolidated). Do not create new provider traits.

### Dependency Navigation

**When uncertain which crate to use:**

| Question | Crate(s) | Avoid |
|----------|----------|-------|
| How do I define a tool? | `rustycode-tools-api` | `rustycode-tools` (impl crate) |
| How do I execute a tool? | `rustycode-tools::executor` | Direct tool calls |
| How do I call an LLM? | `rustycode-llm::LLMProvider` | Creating custom provider traits |
| How do I store session state? | `rustycode-storage` | Direct SQLite |
| How do I publish events? | `rustycode-bus::EventBus` | Direct channels |
| How do I manage skill lifecycle? | `rustycode-skill` | Manual YAML parsing |
| How do I execute a plan? | `rustycode-core::execution` | Direct step iteration |
| How do I group plans into milestones? | `rustycode-protocol::{Milestone, PlanDependency}` | Ad-hoc plan grouping |
| How do I run milestone-aware autonomous mode? | `rustycode-orchestration::execute_milestone` | Manual plan sequencing |
| How do I sandbox command execution? | `rustycode-sandbox` | Raw `std::process::Command` |
| How do I coordinate multiple agents? | `rustycode-team` | Direct agent dispatch |

**Architecture Constraint:** No circular dependencies. If you need shared abstractions, add them to `rustycode-protocol`.

### Breaking Code (God Objects)

These crates are too large and will be refactored:

- `rustycode-tui` (22 dependencies, 5K+ LOC) → Will split into thin UI layer
- `rustycode-core` (40K+ LOC, 18 modules) → Will separate agents/execution/recovery
- `rustycode-tools` (50+ modules) → Will split into api/executor/registry/security

**When modifying these crates:**
- Keep changes localized to modules
- Don't add new dependencies (they're already over-connected)
- Prefer extracting functionality to new crates

## CI

CI runs:
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo fmt --check
```

Pre-commit hooks (`.pre-commit-config.yaml`):
- gitleaks — secret detection
- cargo fmt
- cargo clippy

## Release Process

### Repository Architecture

| Repo | Purpose | URL |
|------|---------|-----|
| `origin` (dev) | Source of truth for all code | `github.com/luengnat/rustycode` |
| `release` | Public-facing: CI/CD, releases, install page | `github.com/rustycode-ai/rustycode` |

The release repo contains **only** CI/CD workflows (`.github/workflows/ci.yml`, `release.yml`) and release assets. It checks out source code from the dev repo using a PAT (`secrets.PRIVATE_REPO_PAT`). **Never push code or tags directly to the release repo.**

### How to Cut a Release

1. **Commit all changes** to `main` on the dev repo
2. **Bump version** in root `Cargo.toml`
3. **Generate changelog** with git-cliff:
   ```bash
   git-cliff --tag v0.X.X -o CHANGELOG.md
   ```
4. **Commit and tag**:
   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "chore: bump version to 0.X.X"
   git tag v0.X.X
   ```
5. **Push to dev repo**:
   ```bash
   git push origin main --tags
   ```
6. **Trigger the release workflow** on the release repo:
   ```bash
   gh workflow run release.yml --repo rustycode-ai/rustycode \
     --field ref=v0.X.X --field release_type=stable
   ```
7. **Monitor** the build:
   ```bash
   gh run list --repo rustycode-ai/rustycode --limit 1
   ```

### Nightly Builds

Same process but use `release_type=nightly`:
```bash
gh workflow run release.yml --repo rustycode-ai/rustycode \
  --field ref=main --field release_type=nightly
```

### Do NOT

- Push tags or branches to the `release` remote — it checks out from the dev repo via PAT
- Run `git push release main` — the histories are intentionally separate
- Create GitHub releases manually — the workflow handles this
