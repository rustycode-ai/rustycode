# rustycode-config

Configuration loading and validation for RustyCode.

## Purpose

Loads configuration from multiple sources (files, environment variables, CLI flags) with validation and defaults. Provides unified configuration interface for all RustyCode components.

## Key Types

- `Config` — Root configuration object
- `ConfigBuilder` — Builder for constructing configs
- `ConfigSource` — Source of configuration (file, env, CLI)
- `ValidationError` — Configuration validation errors
- `LLMConfig` — LLM provider configuration
- `StorageConfig` — Database configuration
- `TUIConfig` — Terminal UI configuration

## Configuration Sources (Priority Order)

1. CLI flags (highest)
2. Environment variables
3. Config files (~/.rustycode/config.toml, ./rustycode.toml)
4. Defaults (lowest)

## Public API

```rust
use rustycode_config::Config;

// Load config from sources
let config = Config::load()?;

// Access settings
println!("Model: {}", config.llm.default_model);
println!("Database: {}", config.storage.database_path);

// Or build custom config
let config = Config::builder()
    .with_model("claude-opus-4-7")
    .with_max_tokens(2048)
    .build()?;
```

## Configuration File Format

```toml
[llm]
default_model = "claude-opus-4-7"
max_tokens = 2048
timeout_seconds = 300

[storage]
database_path = "~/.rustycode/sessions.db"
retention_days = 90

[tui]
theme = "dark"
enable_syntax_highlighting = true

[auth]
# API keys loaded from environment variables
# ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.
```

## Environment Variables

- `RUSTYCODE_MODEL` — Default LLM model
- `RUSTYCODE_MAX_TOKENS` — Token limit
- `RUSTYCODE_STORAGE` — Database path
- Provider keys: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, etc.

## Dependencies

- `toml` or `serde_yaml` — Config file parsing
- `serde` — Serialization
- `validator` — Schema validation
- `anyhow` — Error handling

## Architecture Notes

Configuration is loaded once at startup and made available globally (LazyLock or Arc). Changes to config files don't take effect until restart.

Validation ensures all required fields are present and have valid values. Validation errors are clear and actionable.

Secrets (API keys) are never logged or displayed. SecretString is used for sensitive config values.

## Testing

Tests verify loading from each source, priority order, validation, defaults, and error handling. Mock files test edge cases.

## See Also

- `rustycode-auth` — Auth configuration
- `rustycode-storage` — Storage configuration
- `rustycode-observability` — Logging configuration
