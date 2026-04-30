# rustycode-lsp

Language Server Protocol (LSP) client integration for RustyCode.

## Purpose

Provides LSP client support for querying language servers. Enables RustyCode to request code intelligence features like hover information, goto definition, completions, diagnostics, and symbol search from running language servers (rust-analyzer, pyright, typescript-language-server, etc.).

## Key Types

- `LspClient` — Main LSP client for communicating with language servers
- `LspClientConfig` — Configuration for LSP connection and behavior
- `LspClientState` — Client lifecycle state (Initializing, Ready, Shutdown)
- `ProjectDetector` — Auto-detect project type and language
- `ProjectToolDetection` — Detected build system and language toolchain
- `LanguageId` — Language identifier (rust, python, typescript, etc.)
- `LspServerConfig` — Per-language server configuration

## Public API

```rust
use rustycode_lsp::{LspClient, LspClientConfig};
use lsp_types::Uri as Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = LspClientConfig::default();
    let mut client = LspClient::new(config);

    // Start the client
    client.start().await?;

    // Open a document
    let uri = Url::from_file_path("/path/to/file.rs")?;
    client.open_document(uri.clone(), "rust", 1, "fn main() {}").await?;

    // Request code intelligence
    let hover = client.hover(uri.clone(), lsp_types::Position::new(0, 0)).await?;
    let completions = client.completion(uri.clone(), lsp_types::Position::new(0, 0)).await?;
    let definition = client.goto_definition(uri.clone(), lsp_types::Position::new(0, 0)).await?;

    // Shutdown gracefully
    client.shutdown().await?;
    client.exit().await?;
    Ok(())
}
```

## Supported Operations

- **Document Management** — open_document, close_document, change_document
- **Hover** — Get type information and documentation at position
- **Goto Definition** — Navigate to symbol definition
- **Completions** — Auto-complete suggestions
- **Diagnostics** — Code issues and warnings
- **Symbol Search** — Find symbols by name
- **References** — Find all references to a symbol
- **Rename** — Refactor symbol names

## Project Detection

Auto-detects project type and selects appropriate language server:
- **Rust** → rust-analyzer
- **Python** → pyright, pylance, or pyls
- **TypeScript/JavaScript** → typescript-language-server
- **Go** → gopls
- **C/C++** → clangd

## Dependencies

- `lsp-types` — LSP protocol types
- `tokio` — Async runtime
- `serde_json` — JSON serialization
- `anyhow` — Error handling
- `tracing` — Logging

## Architecture Notes

The LSP client manages a subprocess running the language server. Communication uses JSON-RPC over stdin/stdout. The client maintains request state to match responses with pending requests. Project detection automatically selects the right language server for the codebase.

Document changes are batched and synchronized with the language server incrementally.

## Testing

Tests use mock language servers to verify request/response handling. Integration tests verify end-to-end communication with actual language servers when available.

## See Also

- `rustycode-tools` — Tools that may use LSP for code generation
- `rustycode-core` — Session context (LSP client is session-scoped)
- `rustycode-observability` — Tracing of LSP requests/responses
