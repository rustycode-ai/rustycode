# AGENTS — How to work with the RustyCode repo (for automated coding agents)

Checklist (what this file contains)
- Quick architectural map: major crates, boundaries, and why things are split
- Concrete developer workflows: build, test, lint, run, debug commands used by humans/CI
- Project-specific conventions and canonical code patterns (with file references)
- Integration & cross-component communication points you must know
- Short examples you can follow when modifying the codebase

Important high-level facts
- Workspace-style Rust monorepo. Key crates you will use often:
  - `crates/rustycode-cli` — CLI & TUI entrypoint (run with `cargo run -p rustycode-cli -- ...`). Includes `rustycode update` self-update command.
  - `crates/rustycode-orchestration` — Autonomous execution core (reasoning loops, AST pipeline). This is the orchestration boundary and the single canonical place for autonomous algorithms. See `crates/rustycode-orchestration/README.md` for module map.
  - `crates/rustycode-core` — Session management, headless runtime.
  - `crates/rustycode-llm` — LLM provider abstraction; implementors (Anthropic/OpenAI) live here. Use `LLMProvider` trait.
  - `crates/rustycode-tools` / `rustycode-tools-api` — Tools, tool executors, and security checks.
  - `crates/rustycode-protocol` — Shared cross-crate types and messages (use this for typed integration).
  - `crates/rustycode-bus` — Event bus (pub/sub) used for cross-crate eventing.
- Two repos: `luengnat/rustycode` (dev, private) and `rustycode-ai/rustycode` (release, public). Release builds pull source from the dev repo via `PRIVATE_REPO_PAT`.
- Release workflow: `.github/workflows/build-release.yml` on `rustycode-ai/rustycode` — supports `stable` and `nightly` channels, builds 5 platforms (Linux x64/ARM64, macOS ARM64/x64, Windows x64).

Why the split matters (agent guidance)
- Orchestration must not depend on CLI/TUI. If a change needs terminal/session awareness, modify `rustycode-cli` or `rustycode-tui` instead of orchestration.
- If you need a shared abstraction (types, traits) across crates, add it to `rustycode-protocol` rather than creating a new circular dependency.

Essential developer commands (copy/paste)
- Build workspace (fast):
  cargo build -p rustycode-cli
- Build all crates (CI-like):
  cargo build --workspace --all-targets
- Run CLI/TUI:
  cargo run -p rustycode-cli -- --help
  cargo run -p rustycode-cli -- tui
- Run tests:
  cargo test --workspace
  cargo test -p rustycode-llm -- <test_name>
- Lint & CI checks (required by CI):
  cargo clippy --workspace --all-targets -- -D warnings
  cargo fmt -- --check
- Run clippy+fmt locally before pushing to avoid CI failures.

Project-specific coding conventions (must-follow)
- Error handling:
  - Application code: use `anyhow::Result` and add contextual messages with `.with_context(...)`. Example pattern in `CLAUDE.md`.
  - Library crate error types: use `thiserror::Error`.
- Secrets: always use `secrecy::SecretString` for keys/tokens. Do NOT log raw secrets. See `crates/rustycode-tools/src/security.rs` for `sanitize_for_log()` and path validation rules.
- Async: use `tokio` for async runtime. Prefer `tokio::fs` for file IO and `tokio::sync` primitives (e.g., `tokio::sync::Mutex`, `RwLock`) over std types.
- Concurrency: share state with `Arc<Mutex<T>>` or `Arc<RwLock<T>>` when needed.
- Error messages: always provide human-readable context (example in CLAUDE.md snippet using `Context` in `anyhow`).

Security & permissions (agent constraints)
- `crates/rustycode-tools/src/security.rs` enforces file/command validation. If your change touches tool execution or shell invocation, update/consult that file.
- Pre-commit hooks (repository enforces secret scanning): gitleaks + formatting + clippy. Avoid committing secrets.

Cross-component integration patterns (how components talk)
- Typed messages: use `rustycode-protocol` types for any cross-crate message payloads.
- Eventing: publish/subscribe using `rustycode-bus::EventBus` for decoupled signaling.
- LLM providers: follow the `rustycode-llm::LLMProvider` trait shape when adding a new provider. Register new providers in `crates/rustycode-llm/src/lib.rs` and provider config in `provider.rs`.
- Tools: implement tool interfaces in `rustycode-tools` and expose minimal public APIs in `rustycode-tools-api` to avoid circular deps.

Patterns to copy when adding new code
- New provider/tool: (1) add a new module in the impl crate, (2) register in the impl crate's `lib.rs`, (3) add configuration structures in provider/config file, (4) add tests under the same crate using the project's test patterns.
- New crate: create `crates/<name>/` with `Cargo.toml` and `src/lib.rs`, add to workspace `members` in root `Cargo.toml` (one per line), and add README.md. Keep lints consistent with workspace.

Files & locations you will consult frequently
- `CLAUDE.md` (project development guide) — root
- `crates/rustycode-orchestration/README.md` — orchestration architecture
- `crates/rustycode-tools/src/security.rs` — permission and sanitization rules
- `crates/rustycode-llm/` — provider trait & implementations
- `crates/rustycode-protocol/` — shared types
- `crates/rustycode-bus/` — event bus patterns
- `Cargo.toml` (workspace root) — workspace members and shared deps
- `.github/workflows/` — CI expectations (lint/test steps)

Quick examples (copyable)
- Run the CLI in dev mode:
  cargo run -p rustycode-cli -- some-command --flag
- Add context-rich error:
  let content = tokio::fs::read_to_string(path)
    .with_context(|| format!("read config {}", path.display()))?;

Notes & gotchas
- Avoid introducing circular dependencies. If two crates need shared behavior, move the abstraction into `rustycode-protocol`.
- The repository enforces strict lints in CI (clippy warnings treated as errors). Run clippy locally with the same flags.
- Don't add runtime dependencies lightly — the repo is intentionally conservative about new dependencies.

If you are an automated agent changing code:
- Always run the same commands humans run (build + clippy + fmt + tests) before making a PR.
- When touching tool execution, consult `crates/rustycode-tools/src/security.rs` and add tests that cover the permission checks.

References
- `CLAUDE.md` (root)
- `crates/rustycode-orchestration/README.md`
- `crates/rustycode-tools/src/security.rs`
- `crates/rustycode-llm/`
- `crates/rustycode-protocol/`
- `.github/workflows/` (CI definitions)

---
Generated: a concise, actionable AGENTS guide tailored to this workspace. Use it as your first-stop reference when writing or modifying code in RustyCode.

