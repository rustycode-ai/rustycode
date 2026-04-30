# rustycode-providers

LLM provider registry with metadata, pricing, and auto-discovery.

## Purpose

Central registry for managing LLM providers (Anthropic, OpenAI, Google, Ollama, etc.) with metadata, pricing information, and cost tracking. Auto-discovers available providers from environment variables. Tracks token usage and cost per provider.

## Key Types

- `ModelRegistry` — Central registry for providers and models
- `ProviderMetadata` — Provider capabilities, endpoints, auth methods
- `ModelInfo` — Model-specific information (context window, pricing, etc.)
- `ProviderCapabilities` — Feature flags (streaming, function calling, vision, etc.)
- `PricingInfo` — Input/output token costs
- `CostTracker` — Tracks cumulative costs per provider
- `CostSummary` — Aggregated cost statistics

## Public API

```rust
use rustycode_providers::bootstrap_from_env;

let registry = bootstrap_from_env().await;
for provider_id in registry.list_providers().await {
    if let Some(provider) = registry.get_provider(&provider_id).await {
        println!("Provider: {}", provider.name);
    }
}

let costs = registry.get_cost_summary().await;
println!("Total spent: ${:.2}", costs.total_cost);
```

## Auto-Discovery Environment Variables

- `ANTHROPIC_API_KEY` → Anthropic/Claude
- `OPENAI_API_KEY` → OpenAI/GPT
- `OPENROUTER_API_KEY` → OpenRouter
- `GEMINI_API_KEY` → Google Gemini
- `KIMI_CN_API_KEY` → Kimi (China)
- `KIMI_GLOBAL_API_KEY` → Kimi (Global)
- `ALIBABA_CN_API_KEY` → Alibaba/Qwen (China)
- `ALIBABA_GLOBAL_API_KEY` → Alibaba/Qwen (Global)
- `VERTEX_ACCESS_TOKEN` / `VERTEX_SERVICE_ACCOUNT_KEY` → Google Vertex AI
- `OLLAMA_BASE_URL` → Ollama (local, defaults to http://localhost:11434)

## Built-in Providers

### Cloud Providers

- **Anthropic** — Claude models (claude-3.5-sonnet, etc.)
- **OpenAI** — GPT models (gpt-4o, etc.)
- **Google Gemini** — Multimodal LLM with 1M context
- **OpenRouter** — Multi-provider aggregator
- **Google Vertex AI** — Managed Gemini endpoint

### International Providers

- **Kimi (Moonshot AI)** — High-quality reasoning (China & Global)
- **Alibaba/DashScope (Qwen)** — Cost-effective models (China & Global)

### Local

- **Ollama** — Self-hosted, free inference

## Features

- Accurate pricing data (kept up-to-date)
- Cost tracking across providers
- Streaming and function calling support matrix
- Vision model detection
- Multi-language support (China, Global, etc.)

## Dependencies

- `async-trait` — Async trait support
- `serde`/`serde_json` — Serialization
- `tokio` — Async runtime
- `tracing` — Logging

## Architecture Notes

- Extensible for adding new providers
- Lazy initialization via `bootstrap_from_env()`
- Cost tracking useful for monitoring spending
- Pricing data updateable at runtime

## Testing

- Provider metadata tests
- Pricing calculation tests
- Cost tracking tests
- Environment variable discovery tests

## See Also

- `rustycode-llm` — LLM provider trait implementations
- `rustycode-litert` — Local inference provider
