# LLM Provider Migration Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate 4 Tier 2 providers (gemini, cohere, ollama, bedrock) from inline serialization to the Route + Protocol abstraction, removing ~3,600 LOC of duplicated serialization logic.

**Architecture:** Each provider currently defines its own request/response types and does HTTP calls inline. The migration replaces this with `Route::new(endpoint, Protocol, Transport, Auth)` and delegates `complete()`/`complete_stream()` to `route.execute()`/`route.execute_stream()`. The wire/ modules from Plan 1 already contain the Protocol trait implementations.

**Tech Stack:** Rust, tokio, async-trait, serde_json, reqwest

---

## Current State (Pre-Migration)

**Already migrated (Tier 1 — use Route + OpenAIChatProtocol):**
- openrouter.rs (262 lines), azure.rs (247), together.rs (210), mistral.rs (218)
- perplexity.rs (243), huggingface.rs (232), copilot.rs (283), zhipu.rs (245)

**Target for migration (Tier 2 — still inline):**
- `gemini.rs` — 1,361 lines → target ~300 lines
- `cohere.rs` — 1,149 lines → target ~250 lines
- `ollama.rs` — 1,113 lines → target ~250 lines
- `bedrock.rs` — 1,020 lines → target ~250 lines

**Wire protocols already exist from Plan 1:**
- `wire/gemini.rs` (360 lines) — GeminiProtocol
- `wire/cohere.rs` (295 lines) — CohereProtocol
- `wire/openai_chat.rs` (483 lines) — OpenAIChatProtocol (for Ollama)
- `wire/bedrock.rs` (275 lines) — BedrockProtocol

---

## Chunk 1: Cohere + Ollama Migration

These two are simpler — Cohere uses its own wire/cohere.rs protocol, Ollama is OpenAI-compatible.

### Task 1: Migrate Cohere Provider

**Files:**
- Modify: `crates/rustycode-llm/src/cohere.rs` (1,149 → ~250 lines)
- Reference: `crates/rustycode-llm/src/wire/cohere.rs` (295 lines)

**Current pattern (inline):**
```rust
// cohere.rs defines its own types and does HTTP directly
struct CohereV2Request { ... }
struct CohereV2Message { ... }
// serialize → reqwest::Client → parse response
```

**Target pattern (Route):**
```rust
// Same as together.rs, azure.rs, etc.
let route = Route::new(
    endpoint,
    Box::new(CohereProtocol),
    Box::new(HttpTransport::new(timeout)),
    auth,
).with_name("cohere-chat");
```

- [ ] **Step 1: Write failing test for Route-based CohereProvider**

Add a test that verifies `CohereProvider::new()` creates a provider that delegates to `Route`:

```rust
#[test]
fn cohere_provider_creates_route() {
    let config = make_config(Some("test-key"));
    let provider = CohereProvider::new(config).unwrap();
    // Verify route wire format is Cohere
    assert_eq!(provider.route.wire_format(), WireFormat::Cohere);
}
```

Run: `cargo test -p rustycode-llm cohere_provider_creates_route`
Expected: FAIL — `route` field doesn't exist yet on CohereProvider

- [ ] **Step 2: Rewrite CohereProvider struct to hold Route**

Replace the inline fields with a single `route: Route` field. Remove all inline request/response types (`CohereV2Request`, `CohereV2Message`, `CohereV2Response`, `CohereV2StreamEvent`, etc.).

The new constructor:
```rust
fn new_internal(config: ProviderConfig) -> Result<Self, ProviderError> {
    let endpoint = config.base_url.clone()
        .unwrap_or_else(|| COHERE_API_ENDPOINT.to_string());

    let auth: Box<dyn AuthMethod> = Box::new(crate::auth::BearerAuth::new(
        config.api_key.clone()
            .ok_or_else(|| ProviderError::auth("Missing API key"))?,
    ));

    let route = Route::new(
        endpoint,
        Box::new(CohereProtocol),
        Box::new(HttpTransport::new(config.timeout_seconds.unwrap_or(120))
            .map_err(|e| ProviderError::Configuration(e.to_string()))?),
        auth,
    ).with_name("cohere-chat");

    Ok(Self { config, route })
}
```

- [ ] **Step 3: Rewrite complete() and complete_stream() to use Route**

```rust
async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
    self.route.execute(&request, None).await
        .map_err(|e| ProviderError::api(e.to_string()))
}

async fn complete_stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
    let stream = self.route.execute_stream(&request, None).await
        .map_err(|e| ProviderError::api(e.to_string()))?;
    let chunk_stream = stream.map(|res| res.map_err(|e| ProviderError::api(e.to_string())));
    Ok(Box::pin(chunk_stream))
}
```

- [ ] **Step 4: Run existing tests to verify migration**

Run: `cargo test -p rustycode-llm -- cohere`
Expected: ALL PASS — existing tests cover provider creation, missing API key, config

- [ ] **Step 5: Verify wire/cohere.rs handles all Cohere-specific logic**

Check that `wire/cohere.rs` handles:
- Preamble (system message as `preamble` field, not `system` message)
- Tool calling format (Cohere v2 uses `{ type: "function", function: { name, description, parameters } }`)
- Streaming events (Cohere uses different event types)
- Error responses

If any logic is missing from `wire/cohere.rs`, add it there.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-llm/src/cohere.rs
git commit -m "refactor(llm): migrate Cohere provider from inline serialization to Route+CohereProtocol"
```

### Task 2: Migrate Ollama Provider

**Files:**
- Modify: `crates/rustycode-llm/src/ollama.rs` (1,113 → ~250 lines)
- Reference: `crates/rustycode-llm/src/wire/openai_chat.rs` (483 lines)

**Notes:**
- Ollama's `/api/chat` endpoint is mostly OpenAI-compatible but uses `/api/chat` not `/v1/chat/completions`
- Ollama has extra options (temperature, num_predict, top_p, etc.) — these need to be passed via `ProviderOptions::Ollama`
- Ollama supports vision (images field) — check if wire/openai_chat.rs handles this
- Ollama has local-only features (keep_alive, model listing) — keep these in the provider

- [ ] **Step 1: Verify OpenAIChatProtocol works for Ollama**

Ollama uses `/api/chat` endpoint format which is close to but NOT identical to OpenAI's `/v1/chat/completions`. Check:
1. Does Ollama accept the same request body format? (Yes for basic fields, but options vs. params differ)
2. Does Ollama return the same response format? (Mostly yes)
3. Are tool calls formatted identically? (Close but may differ)

If differences are significant, create `wire/ollama.rs` with an OllamaProtocol that extends OpenAIChatProtocol with Ollama-specific behavior.

- [ ] **Step 2: Write the failing test**

```rust
#[test]
fn ollama_provider_creates_route() {
    let config = make_config(None); // Ollama doesn't need API key
    let provider = OllamaProvider::new(config).unwrap();
    assert!(provider.route.wire_format() == WireFormat::OpenAIChat
         || provider.route.wire_format() == WireFormat::Ollama);
}
```

- [ ] **Step 3: Rewrite OllamaProvider to use Route**

Remove inline types (`OllamaRequest`, `OllamaMessage`, `OllamaOptions`, `OllamaResponse`, etc.).
Keep local-only features (model listing via `/api/tags`, keep_alive).

```rust
fn new_internal(config: ProviderConfig) -> Result<Self, ProviderError> {
    let endpoint = config.base_url.clone()
        .unwrap_or_else(|| "http://localhost:11434".to_string());

    let route = Route::new(
        format!("{}/api/chat", endpoint),
        Box::new(OpenAIChatProtocol), // or OllamaProtocol if needed
        Box::new(HttpTransport::new(config.timeout_seconds.unwrap_or(300))
            .map_err(|e| ProviderError::Configuration(e.to_string()))?),
        Box::new(crate::auth::NoAuth), // Ollama needs no auth
    ).with_name("ollama-chat");

    Ok(Self { config, route, base_url: endpoint })
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rustycode-llm -- ollama`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-llm/src/ollama.rs
git commit -m "refactor(llm): migrate Ollama provider from inline serialization to Route+OpenAIChatProtocol"
```

---

## Chunk 2: Gemini Migration

The most complex Tier 2 migration — Gemini has a unique wire format with `contents`/`candidates` structure, grounding, thinking, and a special streaming endpoint.

### Task 3: Migrate Gemini Provider

**Files:**
- Modify: `crates/rustycode-llm/src/gemini.rs` (1,361 → ~300 lines)
- Reference: `crates/rustycode-llm/src/wire/gemini.rs` (360 lines)

**Gemini-specific concerns:**
1. Two endpoints: `generateContent` (non-streaming) and `streamGenerateContent` (streaming)
2. API key passed as query parameter `?key=API_KEY`, not as header
3. System message as `system_instruction` field, not as a message
4. `contents` array uses `parts` with `text` objects
5. Tool declarations use `functionDeclarations` format
6. Response has `candidates[].content.parts[]` structure
7. Usage metadata in `usageMetadata` field

- [ ] **Step 1: Verify wire/gemini.rs handles all Gemini-specific logic**

Check that `GeminiProtocol` in `wire/gemini.rs` covers:
- System message → `system_instruction` conversion
- Message roles → Gemini's `user`/`model` roles (no `assistant`)
- Tool schema conversion (`functionDeclarations`)
- Response parsing (`candidates[].content.parts[]`)
- Streaming SSE parsing (Gemini uses `"text": true` in response array)
- Usage metadata extraction

If any logic is missing, add it to `wire/gemini.rs` first.

- [ ] **Step 2: Handle Gemini's query-param auth**

Gemini passes API key as `?key=API_KEY` query parameter, not as an `Authorization` header. Options:
a. Create a `GeminiQueryAuth` AuthMethod that appends `?key=` to the URL
b. Add query params support to Route (already in `HttpOverrides`)
c. Use `ApiKeyHeader` with `x-goog-api-key` header (Gemini supports both)

Preferred: option (b) — add query params to Route's endpoint construction.

- [ ] **Step 3: Handle Gemini's dual endpoints**

Gemini uses different URLs for streaming vs non-streaming:
- `generateContent` for `complete()`
- `streamGenerateContent?alt=sse` for `complete_stream()`

Options:
a. Two routes in GeminiProvider (one per endpoint)
b. Add endpoint override to streaming in Route
c. Use `with_extra_headers` / query params approach

Preferred: option (a) — two routes, following the OpenRouter pattern.

- [ ] **Step 4: Write the failing test**

```rust
#[test]
fn gemini_provider_creates_routes() {
    let config = make_config(Some("test-key"));
    let provider = GeminiProvider::new(config).unwrap();
    assert_eq!(provider.chat_route.wire_format(), WireFormat::Gemini);
    assert_eq!(provider.stream_route.wire_format(), WireFormat::Gemini);
}
```

- [ ] **Step 5: Rewrite GeminiProvider to use Route(s)**

Remove all inline types (`GeminiRequest`, `GeminiContent`, `GeminiGenerationConfig`, etc.).
Keep metadata() and model listing.

```rust
fn new_internal(config: ProviderConfig) -> Result<Self, ProviderError> {
    let base = config.base_url.clone()
        .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
    let api_key = config.api_key.clone()
        .ok_or_else(|| ProviderError::auth("Missing API key"))?;

    let auth: Box<dyn AuthMethod> = Box::new(crate::auth::BearerAuth::new(api_key.clone()));

    let model = config.model.clone().unwrap_or_else(|| "gemini-2.5-pro".to_string());

    let chat_route = Route::new(
        format!("{}/v1beta/models/{}:generateContent", base, model),
        Box::new(GeminiProtocol),
        Box::new(HttpTransport::new(config.timeout_seconds.unwrap_or(180))
            .map_err(|e| ProviderError::Configuration(e.to_string()))?),
        auth.clone_box(),
    ).with_name("gemini-chat");

    let stream_route = Route::new(
        format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse", base, model),
        Box::new(GeminiProtocol),
        Box::new(HttpTransport::new(config.timeout_seconds.unwrap_or(180))
            .map_err(|e| ProviderError::Configuration(e.to_string()))?),
        auth,
    ).with_name("gemini-stream");

    Ok(Self { config, chat_route, stream_route })
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p rustycode-llm -- gemini`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-llm/src/gemini.rs
git commit -m "refactor(llm): migrate Gemini provider from inline serialization to Route+GeminiProtocol"
```

---

## Chunk 3: Bedrock Migration

Bedrock uses AWS Sigv4 signing for authentication and has a unique Converse API format.

### Task 4: Migrate Bedrock Provider

**Files:**
- Modify: `crates/rustycode-llm/src/bedrock.rs` (1,020 → ~250 lines)
- Reference: `crates/rustycode-llm/src/wire/bedrock.rs` (275 lines)
- Reference: `crates/rustycode-llm/src/auth/aws_sigv4.rs`

**Bedrock-specific concerns:**
1. AWS Sigv4 authentication (already implemented in `auth/aws_sigv4.rs`)
2. Endpoint varies by region: `bedrock-runtime.{region}.amazonaws.com`
3. Model ID in URL path: `/model/{model-id}/converse`
4. Converse API format (different from both OpenAI and Anthropic)
5. Supports streaming via `/converse-stream`
6. Can use direct AWS credentials OR API key

- [ ] **Step 1: Verify wire/bedrock.rs handles Converse API format**

Check that `BedrockProtocol` covers:
- `messages` → `BedrockConverseMessage` conversion
- `system` → `BedrockSystemContent`
- `inferenceConfig` mapping (maxTokens, temperature, topP)
- `toolConfig` → `BedrockToolConfig`
- Response parsing from Converse output
- Streaming via ConverseStream

- [ ] **Step 2: Verify aws_sigv4.rs works with Route's auth flow**

The `AwsSigv4Auth` needs to:
1. Read AWS credentials from env (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
2. Sign the request with Sigv4
3. Apply signed headers via `apply(&mut headers)`

Check that this integrates correctly with `Route::execute_internal()`.

- [ ] **Step 3: Write the failing test**

```rust
#[test]
fn bedrock_provider_creates_route() {
    // Set required env vars
    std::env::set_var("AWS_ACCESS_KEY_ID", "test");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "test");
    std::env::set_var("AWS_REGION", "us-east-1");

    let config = make_config(Some("test-key"));
    let provider = BedrockProvider::new(config).unwrap();
    assert_eq!(provider.route.wire_format(), WireFormat::Bedrock);

    // Clean up
    std::env::remove_var("AWS_ACCESS_KEY_ID");
    std::env::remove_var("AWS_SECRET_ACCESS_KEY");
    std::env::remove_var("AWS_REGION");
}
```

- [ ] **Step 4: Rewrite BedrockProvider to use Route**

Remove inline types (`BedrockRequest`, `BedrockConverseMessage`, etc.).
Keep metadata(), credential resolution logic, and model prefix handling.

```rust
fn new_internal(config: ProviderConfig) -> Result<Self, ProviderError> {
    let region = std::env::var("AWS_REGION")
        .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string());

    let endpoint = config.base_url.clone()
        .unwrap_or_else(|| format!("https://bedrock-runtime.{}.amazonaws.com", region));

    let auth: Box<dyn AuthMethod> = if config.api_key.is_some() {
        Box::new(crate::auth::BearerAuth::new(config.api_key.clone().unwrap()))
    } else {
        Box::new(crate::auth::AwsSigv4Auth::from_env()?)
    };

    let route = Route::new(
        endpoint,
        Box::new(BedrockProtocol),
        Box::new(HttpTransport::new(config.timeout_seconds.unwrap_or(300))
            .map_err(|e| ProviderError::Configuration(e.to_string()))?),
        auth,
    ).with_name("bedrock-converse");

    Ok(Self { config, route, region })
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p rustycode-llm -- bedrock`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-llm/src/bedrock.rs
git commit -m "refactor(llm): migrate Bedrock provider from inline serialization to Route+BedrockProtocol"
```

---

## Chunk 4: Verification + Cleanup

### Task 5: Full Verification

- [ ] **Step 1: Run full test suite for rustycode-llm**

Run: `cargo test -p rustycode-llm 2>&1 | tail -30`
Expected: ALL tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p rustycode-llm -- -D warnings 2>&1 | tail -20`
Expected: Zero warnings

- [ ] **Step 3: Verify downstream crates compile**

Run: `cargo check -p rustycode-core -p rustycode-orchestration -p rustycode-tui -p rustycode-cli 2>&1 | tail -10`
Expected: All compile successfully

- [ ] **Step 4: Count LOC reduction**

Run:
```bash
wc -l crates/rustycode-llm/src/gemini.rs crates/rustycode-llm/src/cohere.rs crates/rustycode-llm/src/ollama.rs crates/rustycode-llm/src/bedrock.rs
```

Expected: Total ~1,000-1,200 lines (down from 4,643)

### Task 6: Dead Code Cleanup

- [ ] **Step 1: Identify dead code after migration**

After all 4 providers are migrated, check for:
- Inline request/response types that are no longer used
- Helper functions only used by the old inline serialization
- Unused imports in provider files
- Dead code in `provider.rs` that was only used by inline providers

Run: `cargo clippy -p rustycode-llm -- -W dead_code 2>&1 | grep "dead_code"`

- [ ] **Step 2: Remove dead code**

Remove all flagged dead code. Run tests after each removal to verify nothing breaks.

- [ ] **Step 3: Final commit**

```bash
git add -A crates/rustycode-llm/src/
git commit -m "refactor(llm): remove dead code after Tier 2 provider migration"
```

---

## Post-Migration State

**Expected file sizes:**
- `gemini.rs`: ~300 lines (was 1,361)
- `cohere.rs`: ~250 lines (was 1,149)
- `ollama.rs`: ~250 lines (was 1,113)
- `bedrock.rs`: ~250 lines (was 1,020)

**Total reduction:** ~4,643 → ~1,050 lines (-77%)

**All providers using Route + Protocol abstraction:**
- 8 Tier 1: OpenAIChatProtocol ✅ (already done)
- 4 Tier 2: CohereProtocol, OpenAIChatProtocol, GeminiProtocol, BedrockProtocol ✅ (this plan)
- Core providers (anthropic, openai): Complex migration deferred to Plan 3

**Deferred to Plan 3:**
- Migrate `anthropic/` to use `wire/anthropic.rs` + Route
- Migrate `openai/` to use `wire/openai_chat.rs` + Route
- Consolidate `openai_compatible/` shared code
- Remove `build_request()`, `shared_client()`, and other legacy helpers
