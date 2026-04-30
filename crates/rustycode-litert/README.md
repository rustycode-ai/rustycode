# rustycode-litert

LiteRT LM runtime management and process pooling.

## Purpose

Manages the LiteRT (TensorFlow Lite Runtime) local inference engine, including binary installation, model loading, and process lifecycle. Enables running quantized LLMs locally without cloud APIs. Provides a pool-based interface for concurrent inference requests.

## Key Types

- `LitManager` — Orchestrates LiteRT lifecycle and inference
- `LitProcess` — Individual LiteRT process wrapper
- `ProcessPool` — Manages concurrent LiteRT processes
- Installation helpers: `ensure_litert_lm_binary()`, `ensure_gemma_e4b_model()`
- `LiteRtLmInstallConfig` — Configuration for LiteRT installation
- `LiteRtLmInstallResult` — Result of installation process

## Public API

```rust
use rustycode_litert::{LitManager, ProcessPool};

let manager = LitManager::new().await?;
let pool = ProcessPool::new(4); // 4 concurrent processes

let output = pool.infer("What is Rust?").await?;
println!("{}", output);
```

## Configuration

- `default_litert_lm_binary_url()` — Location to download LiteRT binary
- `default_gemma_e4b_model_url()` — Location to download Gemma model
- `default_litert_lm_install_dir()` — Installation directory (defaults to ~/.cache/rustycode)

## Dependencies

- `tokio` — Async runtime
- `anyhow` — Error handling
- `serde` — Configuration serialization

## Architecture Notes

- Provides alternative to cloud-based LLM providers
- Used as a fallback provider when cloud APIs unavailable
- Process pooling prevents overhead of starting inference processes
- Works with quantized models (Gemma 2B, 7B variants)
- Useful for latency-sensitive or privacy-critical applications

## Testing

- Unit tests for installation and process spawning
- Pool concurrency tests
- Mock inference tests

## See Also

- `rustycode-providers` — Provider registry (includes LiteRT entry)
- `rustycode-llm` — LLM provider abstractions
