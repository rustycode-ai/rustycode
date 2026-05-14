# Dependencies: External Services, Integrations, Libraries

<!-- Generated: 2026-05-14 | Files scanned: 1601 | Token estimate: ~500 -->

## Runtime Dependencies

### Async Runtime
- **tokio** — async runtime (all crates)
- **futures** — stream/future combinators

### HTTP
- **reqwest** 0.12 — HTTP client (LLM providers, web_fetch)
- **axum** — HTTP server (WS server, tool server)
- **tower-http** — middleware (CORS, tracing)
- **tokio-tungstenite** — WebSocket client/server

### Database
- **rusqlite** — SQLite (storage, session, orchestration)

### Serialization
- **serde** + **serde_json** — JSON everywhere
- **serde_yaml** — YAML config/skills
- **toml** — TOML config
- **bincode** — binary serialization (session snapshots)
- **schemars** — JSON Schema generation (tool definitions)

### LLM Provider SDKs
- No external SDKs — all providers use reqwest + custom wire protocols
- **tokio-stream** — SSE streaming

### Security & Auth
- **secrecy** — SecretString for API keys
- **sha2** — key hashing
- **keyring** — OS keychain integration (auth)
- **base64** — encoding

### Code Analysis
- **tree-sitter** + grammars (rust, python, js, ts, go, java, cpp, scala) — AST parsing
- **similar** — diff generation
- **handlebars** — prompt templates
- **jsonschema** — schema validation

### Terminal UI
- **ratatui** — TUI framework
- **crossterm** — terminal control
- **syntect** — syntax highlighting (ui-core)
- **pulldown-cmark** — Markdown rendering
- **arboard** — clipboard access
- **image** — image processing

### Browser Automation
- **chromiumoxide** — Chrome DevTools Protocol (MCP/browser tools)

### Container/Bench
- **bollard** — Docker API (bench)
- **git2** — libgit2 bindings (bench)

### Observability
- **tracing** + **tracing-subscriber** — structured logging
- **parking_lot** — efficient sync primitives
- **dashmap** — concurrent hashmap

### Platform
- **landlock** — Linux sandboxing
- **windows-sys** — Windows syscalls (tools-security)
- **libc** — Unix syscalls
- **nix** — Unix API wrappers (TUI signals)

### Embeddings
- **fastembed** — local embeddings (vector-memory)

## Build Dependencies (dev only)
- **criterion** — benchmarking
- **tempfile** — test fixtures
- **once_cell** — lazy statics

## External Service Integrations

| Service | Crate | Protocol |
|---------|-------|----------|
| Anthropic API | rustycode-llm | REST + SSE |
| OpenAI API | rustycode-llm | REST + SSE |
| Azure OpenAI | rustycode-llm | REST + SSE |
| AWS Bedrock | rustycode-llm | AWS SDK SigV4 |
| Google Gemini | rustycode-llm | REST + SSE |
| Ollama | rustycode-llm | REST + SSE |
| HuggingFace | rustycode-llm | REST |
| Docker | rustycode-bench | Docker API (bollard) |
| Git | rustycode-git, rustycode-bench | libgit2 |
| LSP Servers | rustycode-lsp | stdin/stdout JSON-RPC |
| Chrome/Chromium | rustycode-mcp | CDP (chromiumoxide) |
| OS Keychain | rustycode-auth | keyring-rs |
