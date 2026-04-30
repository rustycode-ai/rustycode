# rustycode-llm

LLM provider abstraction and implementations for RustyCode.

## Purpose

Provides unified interface for interacting with multiple LLM providers (Anthropic, OpenAI, Google Gemini, OpenRouter, Ollama, and custom providers). Handles API communication, token counting, cost tracking, and streaming responses.

## Key Types

- `LLMProvider` — Trait for provider implementations
- `Provider` — V2 trait with streaming and async support
- `CompletionRequest` — Request to LLM with context and messages
- `CompletionResponse` — Response with choices and token usage
- `StreamingChunk` — Token chunk for streaming responses
- `ToolUse` — Tool invocation from LLM output
- `ModelConfig` — Model-specific configuration

## Providers

### Implemented

- **Anthropic** — Claude models (opus, sonnet, haiku) with vision
- **OpenAI** — GPT-4, GPT-3.5-turbo, vision models
- **Google Gemini** — Gemini models with vision
- **OpenRouter** — Meta-router for multiple providers
- **Ollama** — Local model inference
- **Kimi** — Chinese LLM provider
- **Alibaba Qwen** — Qwen models
- **Vertex AI** — Google Cloud LLM

## Public API

```rust
use rustycode_llm::{LLMProvider, CompletionRequest, Message};

// Create provider from environment (auto-detects API key)
let provider = LLMProvider::from_env("claude-opus-4-7")?;

// Build request
let request = CompletionRequest {
    model: "claude-opus-4-7".to_string(),
    messages: vec![
        Message { role: "user".to_string(), content: "Write a function".to_string() },
    ],
    max_tokens: 1024,
    system_prompt: Some("You are a coding assistant".to_string()),
    ..Default::default()
};

// Get response
let response = provider.complete(request).await?;
println!("Response: {}", response.content);
println!("Tokens: {} in, {} out", response.input_tokens, response.output_tokens);

// Stream responses
let mut stream = provider.stream(request).await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk.content);
}
```

## Features

- **Streaming** — Real-time token streaming from providers
- **Token Counting** — Accurate token counts per provider
- **Tool Use** — Parse and execute tool calls from LLM
- **Vision** — Support for image inputs (Claude, GPT-4V, Gemini)
- **Cost Tracking** — Automatic cost calculation per API call
- **Fallback** — Chain multiple providers with fallback
- **Caching** — Prompt caching (Anthropic, Gemini)

## Dependencies

- `reqwest` — HTTP client
- `tokio` — Async runtime
- `serde_json` — JSON handling
- `rustycode-protocol` — Core types
- `rustycode-providers` — Provider registry and metadata
- `anyhow` — Error handling

## Architecture Notes

Each provider is a separate module implementing the `Provider` trait. All HTTP communication is async/await based. Token counting uses provider-specific tokenizers when available.

Streaming is implemented via futures `Stream` trait for composability. Responses include usage metrics (input/output tokens) for cost tracking.

Tool use responses are parsed and converted to `ToolUse` structs for execution.

## Testing

Tests use mock HTTP servers to verify requests/responses without hitting real APIs. Provider-specific test suites verify token counting accuracy.

## See Also

- `rustycode-protocol` — Request/response types
- `rustycode-providers` — Provider registry and pricing
- `rustycode-auth` — Authentication and token management
- `rustycode-observability` — Cost and token tracking
- `rustycode-prompt` — Prompt templating
