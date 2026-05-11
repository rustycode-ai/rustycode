# LLM Provider Architecture Redesign

**Status:** Draft → Refined (incorporating cross-project insights from OpenCode)
**Date:** 2026-05-11 (updated after OpenCode survey)
**Scope:** `crates/rustycode-llm` (~77K LOC, 20 providers)

## Problem Statement

The current provider layer has significant duplication and conceptual confusion:

- **17 provider implementations** but only **5 distinct wire formats** (message shapes)
- `provider.rs` is 2,324 lines (god object) — contains `CompletionRequest`, `CompletionResponse`, `LLMProvider` trait, `ProviderConfig`, `ProviderError`, `ApiMode`, thinking types, output config types, SSE event types, helper functions
- `tools.rs` is 2,175 lines — 4 different normalization functions (`to_anthropic_tools`, `normalize_tools_for_openai`, `normalize_tools_for_responses`, `sanitize_tools_for_strict_providers`)
- Message conversion duplicated across ~700 LOC (every provider builds its own JSON body)
- Two different `ProviderConfig` types: one in `provider.rs` (runtime), one in file config
- No clear separation between *what the message looks like* (wire format) and *how it gets there* (transport)

The root cause: **providers and wire formats are conflated.** Changing a model endpoint shouldn't require a new 1,000-line provider file when the message shape is identical to an existing format.

---

## Current Architecture

### Key Types (provider.rs)

```
CompletionRequest                    CompletionResponse
├── model: String                    ├── content: String
├── messages: Vec<ChatMessage>       ├── model: String
├── max_tokens: Option<u32>          ├── usage: Option<Usage>
├── temperature: Option<f32>         ├── stop_reason: Option<String>
├── stream: bool                     ├── citations: Option<Vec<Citation>>
├── system_prompt: Option<String>    ├── thinking_blocks: Option<Vec<ThinkingBlock>>
├── tools: Option<Vec<serde_json::Value>>  ← raw json!()
├── thinking: Option<ThinkingConfig> └── structured_output: Option<Value>
├── output_config: Option<OutputConfig>
├── container: Option<Value>
├── tool_choice: Option<Value>
├── parallel_tool_calls: Option<bool>
├── session_id: Option<String>
└── api_mode: Option<ApiMode>
         ├── Auto
         ├── ChatCompletions
         ├── Responses
         └── ResponsesWs

LLMProvider trait
├── name() -> &'static str
├── supports_streaming() -> bool
├── is_available() -> async bool
├── list_models() -> async Vec<String>
├── complete(CompletionRequest) -> async CompletionResponse
├── complete_stream(CompletionRequest) -> async Stream<Item=StreamChunk>
└── config() -> Option<&ProviderConfig>

ProviderConfig (runtime)
├── api_key: Option<SecretString>
├── base_url: Option<String>
├── timeout_seconds: Option<u64>
├── extra_headers: Option<HashMap<String, String>>
└── retry_config: Option<RetryConfig>
```

### Current Providers and Their Sizes

| File | Lines | Wire Format | Auth | Extra Headers |
|------|------:|-------------|------|---------------|
| `provider.rs` | 2,324 | — (shared types) | — | — |
| `tools.rs` | 2,175 | — (normalization) | — | — |
| `provider_metadata.rs` | 1,409 | — (model registry) | — | — |
| `gemini.rs` | 1,361 | Gemini | `x-goog-api-key` | — |
| `openrouter.rs` | 1,215 | OpenAI Chat + Responses | Bearer | `HTTP-Referer`, `X-Title` |
| `azure.rs` | 1,195 | OpenAI Chat | Bearer / `api-key` | `api-version` |
| `anthropic/mod.rs` | 1,158 | Anthropic | `x-api-key` | `anthropic-version`, `anthropic-beta` |
| `cohere.rs` | 1,149 | Cohere v2 | Bearer | — |
| `ollama.rs` | 1,113 | OpenAI Chat (no tools) | None | — |
| `openai/mod.rs` | 1,097 | OpenAI Chat + Responses | Bearer | — |
| `bedrock.rs` | 1,020 | Bedrock Converse | AWS Sigv4 | — |
| `openai_compatible/responses.rs` | 847 | OpenAI Responses | Bearer | — |
| `zhipu.rs` | 818 | OpenAI Chat (GLM quirks) | Bearer | — |
| `conversation.rs` | 751 | — (history mgmt) | — | — |
| `copilot.rs` | 522 | OpenAI Chat | Bearer (GitHub) | `copilot-integration-id`, `editor-version` |
| `replay_provider.rs` | 522 | — (file I/O) | — | — |
| `huggingface.rs` | 531 | OpenAI Chat | Bearer (`hf_`) | — |
| `perplexity.rs` | 439 | OpenAI Chat | Bearer (`pplx-`) | — |
| `litert_lm.rs` | 395 | — (local inference) | — | — |
| `together.rs` | ~300 | OpenAI Chat | Bearer | — |
| `mistral.rs` | ~300 | OpenAI Chat | Bearer | — |
| **Total** | **~77K** | | | |

---

## Wire Format Catalog

Five distinct message shapes account for all 17 providers:

### 1. Anthropic Wire Format

**Who:** Anthropic direct, z.ai, Kimi, MiniMax, Alibaba Coding Plan (any `anthropic-compatible` endpoint)

**Request shape:**
```json
{
  "model": "claude-opus-4-7",
  "system": "You are...",          // ← separate from messages
  "messages": [
    {"role": "user", "content": [
      {"type": "text", "text": "..."},
      {"type": "image", "source": {"type": "base64", ...}}
    ]},
    {"role": "assistant", "content": [
      {"type": "text", "text": "..."},
      {"type": "tool_use", "id": "...", "name": "...", "input": {...}}
    ]},
    {"role": "user", "content": [
      {"type": "tool_result", "tool_use_id": "...", "content": "...", "is_error": false}
    ]}
  ],
  "tools": [
    {"name": "Edit", "description": "...", "input_schema": {...}}
  ],
  "tool_choice": {"type": "auto"},
  "max_tokens": 8192,
  "stream": true,
  "thinking": {"type": "enabled", "budget_tokens": 20000}
}
```

**Response shape (streaming events):**
- `message_start` → `content_block_start` → `content_block_delta` → `content_block_stop` → `message_delta` → `message_stop`
- Tool calls: `content_block_start` with `type: "tool_use"`
- Thinking: `content_block_start` with `type: "thinking"`

**Tool format:** `{name, description, input_schema, annotations?, defer_loading?}`

**Provider-specific quirks:**

| Provider | Auth | Extra Headers | Notes |
|----------|------|---------------|-------|
| Anthropic direct | `x-api-key` header | `anthropic-version: 2023-06-01`, `anthropic-beta: ...` | Prompt caching, extended thinking, deferred loading |
| z.ai | `Authorization: Bearer` | — | Anthropic-compatible, may not support all beta features |
| Kimi | `Authorization: Bearer` | — | Anthropic-compatible |
| MiniMax | `Authorization: Bearer` | — | Anthropic-compatible |
| Alibaba | `Authorization: Bearer` | — | Anthropic-compatible |

### 2. OpenAI Chat Completions Wire Format

**Who:** OpenAI, Azure, OpenRouter, Together, Mistral, Perplexity, HuggingFace, Copilot, Ollama, Zhipu, z.ai, Kimi, MiniMax, Alibaba

**Request shape:**
```json
{
  "model": "gpt-4.1",
  "messages": [
    {"role": "system", "content": "You are..."},
    {"role": "user", "content": "..."},
    {"role": "assistant", "content": "...", "tool_calls": [
      {"id": "call_123", "type": "function", "function": {"name": "Edit", "arguments": "{...}"}}
    ]},
    {"role": "tool", "tool_call_id": "call_123", "content": "result"}
  ],
  "tools": [
    {"type": "function", "function": {"name": "Edit", "description": "...", "parameters": {...}}}
  ],
  "tool_choice": "auto",
  "max_tokens": 8192,
  "temperature": 0.7,
  "stream": true
}
```

**Response shape (streaming):**
- Standard SSE: `data: {"choices": [{"delta": {"content": "..."}}]}`
- `data: [DONE]` terminates
- Tool calls: `delta.tool_calls[i].function.name/arguments`

**Tool format:** `{type: "function", function: {name, description, parameters}}`

**Provider-specific quirks:**

| Provider | Auth | Extra Headers | Notes |
|----------|------|---------------|-------|
| OpenAI | `Authorization: Bearer` | — | Reasoning models: `max_completion_tokens`, `reasoning_effort` instead of `temperature` |
| Azure | `Authorization: Bearer` or `api-key` | `api-version: 2024-02-15-preview` | Deployment-based URLs: `{base}/openai/deployments/{deployment}/chat/completions` |
| OpenRouter | `Authorization: Bearer` (sk-or-) | `HTTP-Referer`, `X-Title` | Max 128 tools, free tier models |
| Together | `Authorization: Bearer` | — | Minimal, uses openai_compatible |
| Mistral | `Authorization: Bearer` | — | Minimal, uses openai_compatible |
| Perplexity | `Authorization: Bearer` (pplx-) | — | Web search models, Sonar, `SseParseConfig::minimal()` |
| HuggingFace | `Authorization: Bearer` (hf_) | — | Model hub access |
| Copilot | `Authorization: Bearer` (ghp_) | `copilot-integration-id: vscode-chat`, `editor-version: vscode/1.0.0` | GitHub token, Copilot-specific model names |
| Ollama | None | — | No native tools (filtered to text), base64 images in messages, `keep_alive` |
| Zhipu/GLM | `Authorization: Bearer` | — | Auto-enables thinking for GLM models, `sanitize_tools_for_strict_providers()`, `reasoning_content` in deltas |

### 3. OpenAI Responses Wire Format

**Who:** OpenAI, OpenRouter (beta)

**Request shape:**
```json
{
  "model": "gpt-4.1",
  "input": [
    {"role": "system", "content": "You are..."},
    {"role": "user", "content": "..."}
  ],
  "tools": [
    {"type": "function", "name": "Edit", "description": "...", "parameters": {...}}
  ],
  "stream": true
}
```

**Key differences from Chat Completions:**
- `input` instead of `messages` (but accepts messages format too)
- Tools are flat: `{type, name, description, parameters}` — no nested `function` wrapper
- `strict: true` added to tool schemas
- Different streaming event types

**Transport variants:**
- HTTP+SSE (standard)
- WebSocket (OpenAI Realtime API)

### 4. Gemini Wire Format

**Who:** Google Gemini, Vertex AI

**Request shape:**
```json
{
  "contents": [
    {"role": "user", "parts": [{"text": "..."}]},
    {"role": "model", "parts": [{"text": "..."}]}
  ],
  "system_instruction": {"parts": [{"text": "You are..."}]},
  "tools": {"functionDeclarations": [
    {"name": "Edit", "description": "...", "parameters": {...}}
  ]},
  "tool_config": {"functionCallingConfig": {"mode": "AUTO"}},
  "generationConfig": {"temperature": 0.7, "maxOutputTokens": 8192}
}
```

**Key differences:**
- `contents` (not `messages`) with `parts` arrays (not `content`)
- System prompt in separate `system_instruction` field
- `functionDeclarations` (not `tools`)
- `generationConfig` (not top-level params)
- Schema sanitization required: removes `$schema`, `$defs`, `$ref`, flattens type arrays, removes `default: null`

**Auth:** `x-goog-api-key` header or Google OAuth

### 5. Bedrock Converse Wire Format

**Who:** AWS Bedrock

**Request shape:**
```json
{
  "modelId": "anthropic.claude-sonnet-4-20250514",
  "messages": [
    {"role": "user", "content": [{"text": "..."}]}
  ],
  "system": [{"text": "You are..."}],
  "toolConfig": {
    "tools": [{"toolSpec": {"name": "Edit", "description": "...", "inputSchema": {"json": {...}}}}],
    "toolChoice": {"auto": {}}
  },
  "inferenceConfig": {"temperature": 0.7, "maxTokens": 8192}
}
```

**Key differences:**
- `toolSpec` wrapper around tool definitions
- `inputSchema.json` (extra nesting)
- `inferenceConfig` (not top-level params)
- Tool results use `status: "success" | "error"`
- AWS Sigv4 authentication

### 6. Cohere Chat API (v2) Wire Format

**Who:** Cohere

**Request shape:**
```json
{
  "model": "command-r-plus",
  "messages": [
    {"role": "user", "content": "..."},
    {"role": "assistant", "content": "..."}
  ],
  "system": "You are...",
  "tools": [
    {"name": "Edit", "description": "...", "parameter_definitions": {...}}
  ],
  "tool_use": "auto",
  "max_tokens": 8192,
  "temperature": 0.7
}
```

**Key differences:**
- System prompt at top level (like Anthropic)
- `parameter_definitions` instead of `parameters`
- Tool invoke/result pattern similar to Anthropic
- `tool_use` enum: "auto", "off", "always"
- No streaming support for tools (yet)

**Auth:** `Authorization: Bearer` header

### 7. Local Inference Wire Formats

#### 7a. OpenAI Chat Compatible (Ollama, vLLM, Llama.cpp)

**Who:** Ollama, vLLM, Llama.cpp, LocalAI, text-generation-webui

**Request shape:** Identical to OpenAI Chat Completions (but not all features supported)

```json
{
  "model": "mistral:7b",
  "messages": [
    {"role": "user", "content": "..."}
  ],
  "temperature": 0.7,
  "max_tokens": 8192,
  "stream": true
}
```

**Key differences:**
- No native tool support (some implementations strip tools, some error)
- No image support (Ollama can embed base64 in messages, but not standardized)
- No streaming for tool calls
- `model` is a local model name, not a versioned identifier
- Optional `num_predict` / `num_ctx` for local context sizing
- Optional `keep_alive` for Ollama (keep model in memory)
- Optional `seed` for determinism

**Transport variants:**
- HTTP+SSE (most common)
- Custom streaming format (some implementations differ)

**Auth:** Usually None (local) or simple Bearer token

**Tool handling:** 
- Ollama 0.1.31+: Passes tool schema but doesn't invoke; tool calls come as text
- vLLM: Tool support via function calling, varies by model
- Llama.cpp: No native tool support; server can parse structured output

#### 7b. LiteRT Local Inference (On-Device)

**Who:** LiteRT (formerly TensorFlow Lite + ML.js)

**Transport:** In-process only (no HTTP)

**Request shape:** Direct in-memory Rust struct (no JSON serialization)

```rust
struct LocalInferenceRequest {
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: Option<f32>,
    seed: Option<u64>,
}
```

**Key differences:**
- No streaming (synchronous)
- No tools
- No authentication
- No remote calls
- Models bundled in binary or loaded from disk
- Latency: ~100ms–5s depending on model size and hardware

**Error handling:** OOM, model not found, inference failure (recovery unclear)

---

## Wire Format Coverage Matrix

| Provider | Wire Format | Status | Notes |
|----------|-------------|--------|-------|
| Anthropic | Anthropic | ✅ Direct | Full support + prompt caching |
| z.ai, Kimi, MiniMax, Alibaba | Anthropic | ✅ Compatible | May not support all beta features |
| OpenAI | OpenAI Chat + OpenAI Responses | ✅ Direct | Reasoning models use Responses |
| Azure OpenAI | OpenAI Chat | ✅ Compatible | Deployment URLs, `api-version` query |
| OpenRouter | OpenAI Chat (primary), OpenAI Responses (beta) | ✅ Compatible | Extra headers, max 128 tools |
| Together.ai | OpenAI Chat | ✅ Compatible | Minimal, standard OpenAI shape |
| Mistral | OpenAI Chat | ✅ Compatible | Standard OpenAI shape |
| Perplexity | OpenAI Chat | ✅ Compatible | Web search models (Sonar), minimal SSE |
| HuggingFace | OpenAI Chat | ✅ Compatible | Model hub access via Bearer |
| Copilot | OpenAI Chat | ✅ Compatible | GitHub token, Copilot-specific headers |
| Ollama | OpenAI Chat (local) | ⚠️ Partial | No native tools; models as `name:tag` |
| vLLM | OpenAI Chat (local) | ⚠️ Partial | Function calling varies by model; `num_predict` |
| Llama.cpp | OpenAI Chat (local) | ⚠️ Partial | No native tools; can parse structured output |
| Cohere | Cohere Chat v2 | ✅ Direct | Different tool schema, no streaming tools |
| Gemini | Gemini | ✅ Direct | Schema sanitization required |
| Bedrock | Bedrock Converse | ✅ Direct | AWS Sigv4, modelId-based |
| LiteRT | Local (Rust struct) | ⚠️ Special | In-process, no tools, no streaming |
| Replay | Records/replays | ⚠️ Special | Testing only; mirrors wrapped provider |

---

---

---

## Comparative Analysis: OpenCode's Protocol Abstraction

**Cross-Project Survey Finding:** OpenCode (TypeScript) has the best abstraction for handling 10+ providers with 6 wire formats. Its `Protocol<Body, Frame, Event, State>` generic separates concerns more cleanly than our approach in some ways.

### Comparison Table

| Aspect | RustyCode (Proposed) | OpenCode | Winner | Reason |
|--------|----------------------|----------|--------|--------|
| **Abstraction** | WireSerializer trait + Transport enum | Protocol<Body, Frame, Event, State> | Tie | Both work; OpenCode more type-safe, RustyCode more flexible |
| **Provider Duplication** | 5 serializers for 17 providers (71% reduction) | 6 protocols for 10+ providers | Tie | Similar ratio; both ~70–75% reduction |
| **Transport Abstraction** | Explicit enum (Http, HttpSse, WebSocket, Local, File) | Implicit in Protocol | RustyCode | Our separation makes new transports easier to add |
| **Multi-API Handling** | Endpoint selection with scoring + override | SDK layer hides it | OpenCode | Simpler for end-user, harder to debug |
| **Auth Abstraction** | Separate AuthMethod enum | Embedded in Protocol | RustyCode | More composable; bearer + api-key easier to combine |
| **Model-Specific Logic** | Hooks (on_request_start, on_response_deserialize) | Protocol methods | Tie | Both good; OpenCode more type-safe |
| **Testing** | VCR cassettes per provider; 90%+ wire coverage | Assumed (not visible in survey) | Unknown | Need to verify OpenCode's practice |

### Key Insights from OpenCode

1. **Generic over wire format body type** — OpenCode's `Body` is a type parameter, not always JSON
   - **RustyCode equivalent:** We use `serde_json::Value` everywhere. Adds flexibility, loses type safety.
   - **Recommendation:** Keep Value for now; if more typed formats emerge (protobuf, msgpack), refactor.

2. **Protocol methods are sealed, provider composition is simple**
   - OpenCode: DeepSeek reuses OpenAIChat.protocol + custom headers
   - **RustyCode equivalent:** We do this with EndpointConfig + hooks
   - **Recommendation:** Ours is more explicit. Both work.

3. **SDK abstraction hides provider selection**
   - OpenCode: User calls `sdk.complete(request)`, framework picks best protocol
   - **RustyCode equivalent:** User (or orchestration layer) calls `endpoint.execute(request)`
   - **Recommendation:** We expose more control. Useful for debugging, but less ergonomic.

### Verdict

**RustyCode's approach is slightly more decomposed (better) than OpenCode's, with one exception:**

- OpenCode's implicit provider selection is better UX (user doesn't know which API is used)
- RustyCode's explicit endpoint selection is better debuggability (user knows why Chat vs Responses was chosen)

**Action:** Recommend we add transparent logging to endpoint selection so users can understand routing decisions without sacrificing the abstraction.

---

## Design Questions & Detailed Answers

### Q1: How does model-specific behavior (thinking, reasoning, sanitization) flow through the system?

**Problem:** Different models have different capabilities:
- Anthropic Opus/Sonnet: `thinking: {type: "enabled", budget_tokens}`
- OpenAI o-series: `max_completion_tokens` + `reasoning_effort` (no `temperature`)
- Zhipu GLM: Auto-enables thinking when available
- Gemini: Requires schema sanitization

**Solution: Three-Layer Capability System**

```rust
// Layer 1: Model Metadata (static)
pub struct ModelCapability {
    pub name: String,                        // "claude-opus-4-7"
    pub supports_thinking: bool,
    pub supports_extended_thinking: bool,
    pub supports_tool_use: bool,
    pub supports_images: bool,
    pub reasoning_model: bool,               // o1, o3, GLM-5
    pub streaming_tool_calls: bool,
    pub streaming_thinking: bool,
    pub required_thinking_mode: Option<ThinkingMode>,  // Some models auto-think
    pub schema_sanitization: Option<SanitizationLevel>, // Gemini, Zhipu
    pub tool_choice_mode: Option<ToolChoiceStyle>,     // auto, required, disabled
    pub max_tokens: u32,
    pub context_window: u32,
    pub cost_per_million_input: Option<f64>,
    pub cost_per_million_output: Option<f64>,
}

// Layer 2: Endpoint-level hooks (runtime, provider-specific)
pub struct EndpointHooks {
    /// Called after user builds CompletionRequest, before serialization.
    /// Use to inject model-specific defaults (e.g., GLM auto-thinking).
    pub on_request_start: Option<fn(&mut CompletionRequest, &ModelCapability) -> Result<()>>,
    
    /// Called after wire serialization, before sending.
    /// Use to inject header quirks, modify JSON body (e.g., Azure deployment URL).
    pub on_request_serialize: Option<fn(&mut serde_json::Value, &ModelCapability) -> Result<()>>,
    
    /// Called on response parse, before returning to user.
    /// Use to extract thinking blocks, reason tokens (OpenAI o-series), etc.
    pub on_response_deserialize: Option<fn(&mut CompletionResponse, &ModelCapability) -> Result<()>>,
}

// Layer 3: Wire format awareness (serializer-level)
pub trait WireSerializer: Send + Sync {
    fn serialize_request(
        &self,
        request: &CompletionRequest,
        capability: &ModelCapability,  // ← NEW: wire serializer can inspect model
        tools: Option<&[ToolSchema]>,
    ) -> Result<serde_json::Value>;
}
```

**Flow Example: Anthropic Thinking**

```
User calls:
  request.thinking = Some(ThinkingConfig { budget_tokens: 10000 })

1. on_request_start(request, capability):
   // No transformation needed — Anthropic wire format handles it directly

2. wire::Anthropic::serialize_request():
   // Converts CompletionRequest to Anthropic body with thinking block

3. HTTP POST to Anthropic

4. Response comes back with thinking blocks in SSE events

5. on_response_deserialize(response, capability):
   // Extract thinking blocks from events, populate response.thinking_blocks
```

**Flow Example: OpenAI o-series Reasoning**

```
User calls:
  request.thinking = Some(ThinkingConfig { budget_tokens: 10000 })
  request.temperature = Some(0.7)

1. on_request_start(request, capability):
   // OpenAI o-series doesn't support temperature
   if capability.reasoning_model && request.temperature.is_some() {
       request.temperature = None;  // Strip it
   }

2. wire::OpenAIResponses::serialize_request():
   // Convert thinking to max_completion_tokens + reasoning_effort

3. HTTP POST to OpenAI

4. Response includes reasoning_tokens

5. on_response_deserialize(response, capability):
   // Map reasoning_tokens → thinking_blocks (for consistency)
```

**Flow Example: Zhipu Auto-Thinking**

```
User calls:
  request.model = "glm-5"
  request.thinking = None  // User didn't ask for thinking

1. on_request_start(request, capability):
   // GLM-5 always thinks
   if capability.required_thinking_mode == Some(ThinkingMode::Enabled) {
       request.thinking = Some(ThinkingConfig { budget_tokens: None })
   }

2. wire::OpenAIChat::serialize_request():
   // GLM-5 is OpenAI Chat compatible, thinking gets added

3. Response handling as above
```

**Key principle:** Capability metadata is the source of truth. Hooks and serializers consult it, not vice versa. The type system (ModelCapability) makes impossible states unrepresentable.

---

### Q2: How does endpoint selection work?

**Problem:** Some providers have multiple endpoints (OpenRouter: Chat + Responses, OpenAI: Chat + Responses), models may have platform-specific availability, and transports vary by use case (streaming, non-streaming, local).

**Solution: Ranked Endpoint Selection Algorithm**

```rust
pub struct EndpointSelector {
    // Caller's constraints
    pub model: String,
    pub prefer_streaming: bool,
    pub prefer_responses_api: bool,
    pub require_tools: bool,
    pub require_thinking: bool,
    
    // Provider's available endpoints
    pub endpoints: Vec<EndpointConfig>,
    pub model_capabilities: HashMap<String, ModelCapability>,
}

impl EndpointSelector {
    pub fn select(&self) -> Result<&EndpointConfig> {
        // Step 1: Filter endpoints that support this model
        let mut candidates: Vec<_> = self.endpoints.iter()
            .filter(|ep| {
                let cap = self.model_capabilities.get(self.model)?;
                ep.models.iter().any(|m| m.name == self.model)
            })
            .collect();
        
        if candidates.is_empty() {
            return Err(ProviderError::ModelNotSupported(self.model.clone()));
        }
        
        // Step 2: Filter by feature requirements
        if self.require_tools {
            candidates.retain(|ep| {
                self.model_capabilities.get(self.model)
                    .map(|c| c.supports_tool_use)
                    .unwrap_or(false)
            });
        }
        
        if self.require_thinking {
            candidates.retain(|ep| {
                self.model_capabilities.get(self.model)
                    .map(|c| c.supports_thinking)
                    .unwrap_or(false)
            });
        }
        
        // Step 3: Rank by user preferences
        let mut scored = candidates.into_iter()
            .map(|ep| {
                let mut score = 0;
                
                // Prefer requested transport
                if self.prefer_streaming && ep.transport == Transport::HttpSse {
                    score += 1000;
                }
                if self.prefer_responses_api && ep.wire_format == WireFormat::OpenAIResponses {
                    score += 500;
                }
                
                // Prefer non-error-prone transports
                match ep.transport {
                    Transport::Http => score += 100,
                    Transport::HttpSse => score += 200,
                    Transport::WebSocket => score += 150,
                    Transport::Local => score += 50,
                    Transport::File => score += 0,
                }
                
                (ep, score)
            })
            .collect::<Vec<_>>();
        
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Step 4: Return best match
        scored.first()
            .map(|(ep, _)| *ep)
            .ok_or_else(|| ProviderError::NoEndpointAvailable)
    }
}
```

**Example: OpenAI with o-series model**

```
User calls:
  provider.complete(CompletionRequest {
      model: "gpt-4-o3-mini",
      thinking: Some(...),
      stream: true,
  })

EndpointSelector:
  • Filter: gpt-4-o3-mini available on OpenAI Chat and OpenAI Responses
  • Filter: o3-mini is reasoning_model → must use Responses (Chat doesn't support it)
  • Score Responses: +500 (preferred), +200 (streaming), = 700
  • Result: OpenAI Responses endpoint

Transport fallback (if SSE parsing fails):
  • Retry with non-streaming (Http transport)
  • Log warning (tool calls may not stream)
```

**Example: OpenRouter with two endpoints**

```
Provider has:
  - Endpoint 1: Chat Completions + standard headers
  - Endpoint 2: Responses API + standard headers

User calls with stream=true:
  • Both support the model
  • Chat endpoint scores higher for streaming (more stable)
  • Select Chat endpoint

User calls with require_tools=true and some model doesn't support tools:
  • Filter filters it out
  • Return error: model not supported for tool use
```

---

### Q3: How is tool schema normalization done?

**Problem:** Each wire format has different requirements:
- Gemini: Removes `$schema`, `$defs`, `$ref`, flattens type unions, removes `default: null`
- Zhipu: Removes `minimum`, `maximum`, `enum`, `additionalProperties`
- OpenAI Responses: Adds `strict: true` to tool schemas
- Ollama/vLLM: No tool support — strip entirely or convert to text

**Solution: Format-Aware, Lossless Normalization**

```rust
/// Describes what features a format supports.
#[derive(Clone, Copy)]
pub struct SchemaNormalizationProfile {
    pub supports_ref: bool,                  // $ref
    pub supports_defs: bool,                 // $defs
    pub supports_schema_keyword: bool,       // $schema
    pub supports_default_values: bool,
    pub supports_enum: bool,
    pub supports_type_unions: bool,          // type: ["string", "null"]
    pub supports_additional_properties: bool,
    pub supports_min_max: bool,              // minimum, maximum
    pub supports_pattern: bool,              // pattern (regex)
    pub supports_format: bool,               // format: "email", "date-time"
    pub supports_examples: bool,
    pub requires_strict: bool,               // strict: true in OpenAI Responses
}

/// Normalized schema preserves original, tracks what was removed.
#[derive(Clone)]
pub struct NormalizedSchema {
    pub schema: serde_json::Value,
    pub warnings: Vec<String>,  // ["removed $ref to #/definitions/Error", ...]
    pub removed_features: Vec<&'static str>,
}

impl ToolSchema {
    /// Normalize for a specific wire format.
    pub fn normalize_for_format(
        &self,
        format: WireFormat,
    ) -> Result<NormalizedSchema> {
        let profile = profile_for_format(format);
        let mut normalized = self.input_schema.clone();
        let mut warnings = Vec::new();
        let mut removed = Vec::new();
        
        // 1. Handle $ref — expand or error
        if !profile.supports_ref {
            // This is lossy: we're losing semantic meaning if we remove $ref
            // Better: expand $ref inline (requires full schema context)
            removed.push("$ref");
            warnings.push("$ref not supported; consider expanding inline".into());
        }
        
        // 2. Handle type unions — flatten or error
        if !profile.supports_type_unions {
            normalize_type_unions(&mut normalized, &mut warnings, &mut removed);
        }
        
        // 3. Handle minimum/maximum — preserve as description or error
        if !profile.supports_min_max {
            move_constraints_to_description(&mut normalized, &mut warnings, &mut removed);
        }
        
        // 4. Handle enum — preserve or error
        if !profile.supports_enum {
            move_enum_to_description(&mut normalized, &mut warnings, &mut removed);
        }
        
        // 5. Add strict: true if required
        if profile.requires_strict {
            normalized["strict"] = serde_json::Value::Bool(true);
        }
        
        Ok(NormalizedSchema { schema: normalized, warnings, removed_features: removed })
    }
}
```

**Key principle:** Normalization is **lossless where possible**:
- Type unions: `["string", "null"]` → `"string"` with description noting nullable
- Constraints: `minimum: 1, maximum: 10` → move to description `"Value between 1 and 10"`
- Enum: Move to description if not supported
- **Only error if required features are missing** (e.g., object without properties)

**Validation:** Tools can opt-in to validation:
```rust
let normalized = schema.normalize_for_format(WireFormat::Gemini)?;
if !normalized.removed_features.is_empty() {
    log::warn!("Tool {} lost features: {:?}", schema.name, normalized.removed_features);
}
```

---

### Q4: How does streaming vs. non-streaming fallback work?

**Problem:** Users request `stream: true`, but:
- SSE parsing might fail
- Provider might not support streaming
- Tools are being invoked (some providers don't stream tool calls)
- Network goes down mid-stream

**Solution: Declarative Fallback Strategy**

```rust
pub struct TransportFallbackStrategy {
    pub primary: Transport,
    pub fallbacks: Vec<Transport>,  // Ordered by preference
    pub retry_max: u32,
    pub log_failures: bool,
}

impl TransportFallbackStrategy {
    pub fn default_for_provider(provider_name: &str) -> Self {
        match provider_name {
            "anthropic" => Self {
                primary: Transport::HttpSse,
                fallbacks: vec![Transport::Http],
                retry_max: 2,
                log_failures: true,
            },
            "openai" => Self {
                primary: Transport::HttpSse,
                fallbacks: vec![Transport::Http],
                retry_max: 2,
                log_failures: true,
            },
            "ollama" => Self {
                primary: Transport::Http,  // Ollama SSE is unreliable
                fallbacks: vec![],
                retry_max: 1,
                log_failures: false,
            },
            _ => Self::default(),
        }
    }
}

pub async fn execute_with_fallback(
    request: CompletionRequest,
    strategy: TransportFallbackStrategy,
) -> Result<CompletionResponse> {
    let mut last_error = None;
    
    for (attempt, transport) in
        std::iter::once(strategy.primary)
            .chain(strategy.fallbacks.iter().copied())
            .enumerate()
    {
        if attempt >= strategy.retry_max as usize {
            break;
        }
        
        let mut request_copy = request.clone();
        request_copy.stream = transport == Transport::HttpSse;
        
        match execute_with_transport(&request_copy, transport).await {
            Ok(response) => return Ok(response),
            Err(e) => {
                if strategy.log_failures {
                    log::warn!(
                        "Transport {:?} failed on attempt {}: {}. Trying fallback...",
                        transport, attempt, e
                    );
                }
                last_error = Some(e);
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| ProviderError::NoTransportAvailable))
}
```

**Key principle:** Fallback is **provider-aware**:
- Anthropic/OpenAI: Default to streaming, fall back to non-streaming
- Ollama/vLLM: Default to non-streaming (SSE parsing is fragile)
- Bedrock: Only HTTP (no SSE)
- LiteRT: No streaming at all

The transport selection is **implicit** — user just calls `complete(request)` and the framework picks transport + fallback.

---

### Q5: What about LiteRT and File providers?

**Problem:** These don't fit the HTTP × Auth × WireFormat model:
- **LiteRT:** In-process inference, no HTTP, no auth, Rust structs not JSON
- **File (Replay):** Records/replays whatever the wrapped provider does

**Solution: Special-Case Providers with Type-Safe Variants**

```rust
pub enum LLMProviderEnum {
    Http {
        endpoint: EndpointConfig,
        wire: Box<dyn WireSerializer>,
        transport: Transport,
        auth: AuthMethod,
    },
    Local {
        backend: LocalInferenceBackend,  // LiteRT, llama.cpp local, etc.
    },
    Replay {
        recorded: Vec<RecordedExchange>,  // For testing
    },
}

pub enum LocalInferenceBackend {
    LiteRT {
        model_path: PathBuf,
        device: Device,  // CPU, GPU
    },
    Ollama {
        base_url: String,  // http://localhost:11434
    },
    LlamaCpp {
        base_url: String,  // http://localhost:8000
    },
}
```

**LiteRT Implementation:**

```rust
pub struct LiteRTProvider {
    model: LiteRTModel,
    device: Device,
}

#[async_trait]
impl LLMProvider for LiteRTProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // No JSON serialization
        let result = self.model.infer(
            &request.messages,
            request.max_tokens.unwrap_or(1024),
            request.temperature.unwrap_or(0.7),
        )?;
        
        Ok(CompletionResponse {
            content: result.text,
            ..Default::default()
        })
    }
    
    async fn complete_stream(&self, request: CompletionRequest)
        -> Result<BoxStream<StreamChunk>>
    {
        Err(ProviderError::NotSupported("LiteRT does not support streaming"))
    }
}
```

**Key principle:** Type safety — the enum variant determines what's available. Callers can't accidentally request streaming from LiteRT.

---

### Q6: How does backward compatibility work during migration?

**Problem:** Existing code imports from `rustycode_llm::anthropic`, `rustycode_llm::openai`, etc. We can't break those immediately.

**Solution: Dual Exports + Deprecation Warnings**

```rust
// lib.rs
pub mod provider;
pub mod wire;
pub mod transport;

// Legacy re-exports (deprecated)
pub mod anthropic {
    //! **Deprecated:** Use `provider::anthropic` instead.
    pub use super::provider::anthropic::*;
}

pub mod openai {
    //! **Deprecated:** Use `provider::openai` and `wire::openai_chat` instead.
    pub use super::provider::openai::*;
}
```

**Concrete migration for a single provider:**

```rust
// OLD: crates/rustycode-llm/src/openai.rs (~1097 lines)
// This file contains:
//   - Request building
//   - SSE parsing
//   - Error handling
//   - All in one place

// NEW: Split into
//   - wire::openai_chat.rs (~300 lines) — message → JSON body
//   - transport::http_sse.rs (~200 lines) — SSE parsing logic
//   - auth::bearer.rs (~50 lines) — Bearer token injection
//   - provider::openai.rs (~100 lines) — endpoint config, model routing

// At cutover:
// 1. Implement wire::openai_chat::OpenAIChatSerializer
// 2. Test against wire serializer tests (no transport involved)
// 3. Wire up transport::http_sse for streaming
// 4. Implement provider::openai::OpenAIProvider using the new pieces
// 5. Add deprecation note to old openai.rs
// 6. Keep old impl working (calls new impl under the hood)
```

**Breaking change plan:**
- **v0.4:** Release with new architecture + deprecation warnings on old imports
- **v0.5:** Remove old code, convert deprecation warnings to errors

---

### Q7: How does cost tracking stay plugged in?

**Problem:** Cost tracking depends on provider info (pricing, token counts). When providers become thin wrappers, where does this logic live?

**Solution: Decoupled Cost Tracker**

```rust
pub struct CostTracker {
    /// Maps model name → per-token cost
    pub model_costs: HashMap<String, ModelCost>,
    pub input_token_counter: Box<dyn Fn(&str) -> usize + Send + Sync>,
}

pub struct ModelCost {
    pub cost_per_million_input: f64,
    pub cost_per_million_output: f64,
    pub reasoning_tokens_multiplier: Option<f64>,  // o-series
    pub cache_read_multiplier: Option<f64>,        // Anthropic prompt caching
}

impl CompletionResponse {
    pub fn estimate_cost(&self, model: &str, tracker: &CostTracker) -> Result<Cost> {
        let cost_info = tracker.model_costs.get(model)
            .ok_or(ProviderError::UnknownModel(model.to_string()))?;
        
        let input_tokens = self.usage.as_ref()
            .map(|u| u.input_tokens)
            .unwrap_or(0);
        
        let output_tokens = self.usage.as_ref()
            .map(|u| u.output_tokens)
            .unwrap_or(0);
        
        // Handle thinking tokens (Anthropic)
        let thinking_tokens = self.thinking_blocks.as_ref()
            .map(|blocks| blocks.iter().map(|b| b.token_count).sum::<u32>())
            .unwrap_or(0);
        
        // Handle reasoning tokens (OpenAI)
        let reasoning_tokens = self.usage.as_ref()
            .and_then(|u| u.reasoning_tokens)
            .unwrap_or(0);
        
        let input_cost = (input_tokens as f64 / 1_000_000.0) * cost_info.cost_per_million_input;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * cost_info.cost_per_million_output;
        
        let reasoning_cost = if let Some(mult) = cost_info.reasoning_tokens_multiplier {
            ((reasoning_tokens as f64) / 1_000_000.0) * cost_info.cost_per_million_input * mult
        } else {
            0.0
        };
        
        Ok(Cost {
            input_cost,
            output_cost,
            reasoning_cost,
            total: input_cost + output_cost + reasoning_cost,
        })
    }
}
```

**Key principle:** Cost tracking is **response-aware**, not provider-aware. It inspects the response and model metadata, not the provider implementation.

---

## Proposed Architecture

### Core Insight: Three Orthogonal Dimensions

```
Wire Format (message shape) × Transport (delivery) × Auth (identity)
```

| Dimension | Values | Purpose |
|-----------|--------|---------|
| **WireFormat** | Anthropic, OpenAIChat, OpenAIResponses, Gemini, Bedrock | How to serialize/deserialize messages |
| **Transport** | Http, HttpSse, WebSocket, Local, File | How to deliver and receive data |
| **AuthMethod** | Bearer, ApiKeyHeader, AwsSigv4, None | How to authenticate |

### Module Structure

```
crates/rustycode-llm/src/
├── types/                    # Shared types (extracted from provider.rs)
│   ├── mod.rs                # Re-exports
│   ├── request.rs            # CompletionRequest
│   ├── response.rs           # CompletionResponse, ThinkingBlock, Citation
│   ├── message.rs            # ChatMessage, MessageRole, MessageContent
│   ├── config.rs             # ProviderConfig, ThinkingConfig, OutputConfig, EffortLevel
│   ├── error.rs              # ProviderError
│   └── streaming.rs          # StreamChunk, StreamEvent, SSEEvent
│
├── schema/                   # Typed tool schema (replaces raw json!())
│   ├── mod.rs                # JsonSchema builder, ToolSchema
│   ├── normalizer.rs         # Per-format normalization (5 format-specific normalizers)
│   └── validator.rs          # Schema validation and feature detection
│
├── wire/                     # Wire format serializers (7 files, not 17)
│   ├── mod.rs                # WireFormat enum, WireSerializer trait
│   ├── anthropic.rs          # Anthropic message ↔ JSON body
│   ├── openai_chat.rs        # OpenAI Chat Completions format
│   ├── openai_responses.rs   # OpenAI Responses format
│   ├── cohere.rs             # Cohere Chat v2 format
│   ├── gemini.rs             # Gemini format
│   ├── bedrock.rs            # Bedrock Converse format
│   └── litert.rs             # LiteRT local inference (Rust structs)
│
├── transport/                # Delivery mechanisms
│   ├── mod.rs                # Transport trait + factory
│   ├── http.rs               # Non-streaming request/response
│   ├── http_sse.rs           # SSE streaming with format-aware parsing
│   ├── http_custom_sse.rs    # Custom streaming for vLLM/Ollama variants
│   ├── websocket.rs          # WS streaming (OpenAI Realtime)
│   ├── local.rs              # Local inference (LiteRT, in-process)
│   └── file.rs               # Replay provider (testing)
│
├── auth/                     # Auth adapters
│   ├── mod.rs                # AuthMethod enum + factory
│   ├── bearer.rs             # Authorization: Bearer header
│   ├── api_key_header.rs     # x-api-key, x-goog-api-key, api-key
│   ├── aws_sigv4.rs          # AWS Sigv4 signing
│   ├── github_token.rs       # GitHub token parsing (Copilot)
│   └── none.rs               # No auth (local)
│
├── provider/                 # Provider configs (thin wrappers)
│   ├── mod.rs                # LLMProvider impl, ProviderRegistry
│   ├── registry.rs           # ProviderRegistry with endpoint lookup
│   ├── endpoint.rs           # EndpointConfig definition + builder
│   ├── anthropic.rs          # Anthropic-specific config
│   ├── openai.rs             # OpenAI (dual API, reasoning model routing)
│   ├── gemini.rs             # Gemini (schema sanitization hooks)
│   ├── bedrock.rs            # Bedrock (Sigv4, model prefix)
│   ├── openrouter.rs         # OpenRouter (extra headers, tool limits)
│   ├── azure.rs              # Azure (deployment URLs, api-version)
│   ├── cohere.rs             # Cohere (chat v2)
│   ├── ollama.rs             # Ollama (local, tool filtering, keep_alive)
│   ├── vllm.rs               # vLLM (local, function calling detection)
│   ├── llama_cpp.rs          # Llama.cpp (local, structured output)
│   ├── litert_lm.rs          # LiteRT (in-process, no tools)
│   ├── zhipu.rs              # Zhipu GLM (thinking, sanitization)
│   ├── copilot.rs            # Copilot (GitHub headers, token parsing)
│   ├── hf_inference.rs       # HuggingFace Inference API
│   └── perplexity.rs         # Perplexity (web search)
│
├── model_capabilities.rs     # Model feature matrix (tools, thinking, streaming per model)
├── provider_metadata.rs      # Model registry + pricing
├── conversation.rs           # History management (unchanged)
├── retry.rs                  # Retry logic with exponential backoff
├── cost_tracking.rs          # Token counting + pricing calculator
├── timeout_handler.rs        # Timeout with provider-specific overrides
├── hooks.rs                  # Provider-specific pre/post processing
└── lib.rs                    # Public API
```

### Key Types

```rust
/// How messages are serialized — an endpoint property, not a provider property.
#[derive(Debug, Clone, Copy)]
pub enum WireFormat {
    Anthropic,
    OpenAIChat,
    OpenAIResponses,
    Gemini,
    Bedrock,
}

/// How data is delivered and received.
#[derive(Debug, Clone, Copy)]
pub enum Transport {
    Http,        // Non-streaming request/response
    HttpSse,     // Streaming via Server-Sent Events
    WebSocket,   // WebSocket streaming (OpenAI Realtime)
    Local,       // Local inference (LiteRT, etc.)
    File,        // Replay provider (testing)
}

/// How requests are authenticated.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    Bearer(SecretString),
    ApiKeyHeader { header: String, key: SecretString },  // x-api-key, x-goog-api-key, api-key
    AwsSigv4 { region: String, access_key: SecretString, secret_key: SecretString },
    None,
}

/// A single endpoint: wire format + transport + auth + URL + headers.
/// Providers may have multiple endpoints (e.g., OpenRouter has Chat + Responses).
pub struct EndpointConfig {
    pub url: String,
    pub wire_format: WireFormat,
    pub transport: Transport,
    pub auth: AuthMethod,
    pub extra_headers: Vec<(String, String)>,
    pub models: Vec<ModelConfig>,
}

/// Per-model configuration within an endpoint.
pub struct ModelConfig {
    pub name: String,                        // e.g., "claude-opus-4-7"
    pub max_tokens_override: Option<u32>,    // override default
    pub supports_thinking: bool,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub supports_images: bool,
    pub reasoning_model: bool,               // OpenAI o-series, GLM-5
    pub cost_per_million_input: Option<f64>,
    pub cost_per_million_output: Option<f64>,
}

/// Provider = collection of endpoints + provider-level headers/quirks.
pub struct ProviderDefinition {
    pub name: String,
    pub endpoints: Vec<EndpointConfig>,
    /// Provider-level headers applied to ALL endpoints (e.g., OpenRouter's Referer/Title)
    pub global_headers: Vec<(String, String)>,
    /// Per-request hooks for provider-specific transformations
    pub pre_serialize: Option<fn(&mut CompletionRequest)>,
    /// Per-response hooks for provider-specific normalization
    pub post_deserialize: Option<fn(&mut CompletionResponse)>,
}
```

### Wire Serializer Trait

```rust
/// Serialize/deserialize messages for a specific wire format.
/// Each wire format has ONE implementation shared by all providers using that format.
#[async_trait]
pub trait WireSerializer: Send + Sync {
    fn wire_format(&self) -> WireFormat;

    /// Build the HTTP request body from a CompletionRequest.
    fn serialize_request(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<serde_json::Value, ProviderError>;

    /// Parse a non-streaming response.
    fn deserialize_response(
        &self,
        body: &serde_json::Value,
    ) -> Result<CompletionResponse, ProviderError>;

    /// Parse a single SSE chunk into a StreamEvent.
    fn parse_sse_chunk(
        &self,
        data: &str,
    ) -> Result<Option<StreamEvent>, ProviderError>;

    /// Serialize tool definitions into this wire format's tool schema.
    fn serialize_tools(
        &self,
        tools: &[ToolSchema],
    ) -> Vec<serde_json::Value>;
}
```

### Endpoint Selection

```rust
impl ProviderDefinition {
    /// Select the best endpoint for a given request.
    pub fn select_endpoint(
        &self,
        model: &str,
        prefer_streaming: bool,
        prefer_responses_api: bool,
    ) -> Option<&EndpointConfig> {
        // 1. Find endpoints that support this model
        // 2. Prefer HttpSse over Http if streaming requested
        // 3. Prefer Responses over Chat if requested and available
        // 4. Return best match
    }
}
```

### How Duplication Collapses

**Before (17 files, ~20K LOC of duplicated logic):**
```
anthropic.rs     → builds Anthropic body, parses Anthropic SSE
openai.rs        → builds OpenAI body, parses OpenAI SSE
azure.rs         → builds OpenAI body, parses OpenAI SSE  ← DUPLICATE
openrouter.rs    → builds OpenAI body, parses OpenAI SSE  ← DUPLICATE
together.rs      → builds OpenAI body, parses OpenAI SSE  ← DUPLICATE
mistral.rs       → builds OpenAI body, parses OpenAI SSE  ← DUPLICATE
perplexity.rs    → builds OpenAI body, parses OpenAI SSE  ← DUPLICATE
huggingface.rs   → builds OpenAI body, parses OpenAI SSE  ← DUPLICATE
copilot.rs       → builds OpenAI body, parses OpenAI SSE  ← DUPLICATE
gemini.rs        → builds Gemini body, parses Gemini SSE
bedrock.rs       → builds Bedrock body, parses Bedrock events
ollama.rs        → builds OpenAI body, parses OpenAI SSE, no tools
zhipu.rs         → builds OpenAI body, parses OpenAI SSE, GLM quirks
cohere.rs        → builds Cohere body, parses Cohere SSE
```

**After (5 serializers + thin provider wrappers):**
```
wire/anthropic.rs        → ONE Anthropic serializer (shared by all Anthropic-format providers)
wire/openai_chat.rs      → ONE OpenAI Chat serializer (shared by 10+ providers)
wire/openai_responses.rs → ONE Responses serializer
wire/gemini.rs           → ONE Gemini serializer
wire/bedrock.rs          → ONE Bedrock serializer

provider/anthropic.rs    → ~50 lines (endpoint config + x-api-key + anthropic-version)
provider/openai.rs       → ~100 lines (dual API, reasoning models)
provider/openrouter.rs   → ~50 lines (Referer + Title headers)
provider/azure.rs        → ~40 lines (deployment URL, api-version)
provider/ollama.rs       → ~40 lines (no tools, image extraction)
provider/zhipu.rs        → ~50 lines (GLM thinking, sanitization)
provider/copilot.rs      → ~30 lines (GitHub headers)
```

**Estimated reduction:** ~20K LOC → ~5K LOC (75% reduction in duplicated logic).

---

## Typed Tool Schema

Replace raw `serde_json::Value` tool definitions with a typed builder:

```rust
/// Typed tool schema — replaces raw json!() macros.
#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

#[derive(Debug, Clone)]
pub struct JsonSchema {
    pub schema_type: Option<SchemaType>,
    pub description: Option<String>,
    pub properties: Option<BTreeMap<String, JsonSchema>>,
    pub required: Option<Vec<String>>,
    pub items: Option<Box<JsonSchema>>,
    pub enum_values: Option<Vec<String>>,
    pub any_of: Option<Vec<JsonSchema>>,
    pub additional_properties: Option<bool>,
}

impl JsonSchema {
    pub fn string(description: impl Into<String>) -> Self { ... }
    pub fn integer(description: impl Into<String>) -> Self { ... }
    pub fn boolean(description: impl Into<String>) -> Self { ... }
    pub fn object(properties: BTreeMap<String, Self>, required: Vec<String>) -> Self { ... }
    pub fn array(items: Self) -> Self { ... }
    pub fn enum_of(variants: Vec<&str>) -> Self { ... }
}

// Usage:
let edit_tool = ToolSchema::new(
    "Edit",
    "Replace exact string matches in files",
    JsonSchema::object(
        BTreeMap::from([
            ("file_path".into(), JsonSchema::string("Absolute file path")),
            ("old_string".into(), JsonSchema::string("Text to replace")),
            ("new_string".into(), JsonSchema::string("Replacement text")),
        ]),
        vec!["file_path".into(), "old_string".into(), "new_string".into()],
    ),
);
```

Each `WireSerializer` converts `ToolSchema` to its format:
- Anthropic: `{name, description, input_schema: {...}}`
- OpenAI Chat: `{type: "function", function: {name, description, parameters: {...}}}`
- OpenAI Responses: `{type: "function", name, description, parameters: {...}}`
- Gemini: `{functionDeclarations: [{name, description, parameters: {...}}]}`
- Bedrock: `{toolSpec: {name, description, inputSchema: {json: {...}}}}`

---

## Migration Path

### Phase 1: Extract types (non-breaking)

Split `provider.rs` (2,324 lines) into `types/` modules. Pure re-exports, no behavior change.

- `types/request.rs` — `CompletionRequest`
- `types/response.rs` — `CompletionResponse`, `ThinkingBlock`, `Citation`
- `types/message.rs` — `ChatMessage`, `MessageRole`
- `types/config.rs` — `ProviderConfig`, `ThinkingConfig`, `OutputConfig`, `EffortLevel`
- `types/error.rs` — `ProviderError`
- `types/streaming.rs` — `StreamChunk`, `StreamEvent`, `SSEEvent`

### Phase 2: Add typed schema (additive)

Create `schema/` module with `ToolSchema` and `JsonSchema`. Existing `tools.rs` remains, but new tools use the typed API. Both can coexist.

### Phase 3: Add wire serializers (additive)

Create `wire/` module with 5 serializers. Each implements `WireSerializer`. Existing providers continue using their current code. New providers or refactored providers use wire serializers.

### Phase 4: Add transport layer (additive)

Create `transport/` module. Extract `HttpSse` from existing SSE parsing code. Add `Http` for non-streaming. Add `WebSocket` stub.

### Phase 5: Migrate providers one at a time

For each provider, replace its inline serialization with the matching wire serializer + transport + auth combo. Providers become thin wrappers that just configure endpoints.

Priority order (by impact — most duplication first):

**Tier 1: OpenAI Chat Compatibility (10 providers, ~50% of duplication)**
1. `openrouter.rs` → `wire::OpenAIChat` + Bearer + extra headers (`HTTP-Referer`, `X-Title`)
2. `azure.rs` → `wire::OpenAIChat` + Bearer + deployment URL manipulation
3. `together.rs` → `wire::OpenAIChat` + Bearer
4. `mistral.rs` → `wire::OpenAIChat` + Bearer
5. `perplexity.rs` → `wire::OpenAIChat` + Bearer + SseParseConfig::minimal()
6. `huggingface.rs` → `wire::OpenAIChat` + Bearer (hf_)
7. `copilot.rs` → `wire::OpenAIChat` + GitHubToken + headers (`copilot-integration-id`, `editor-version`)
8. `zhipu.rs` → `wire::OpenAIChat` + Bearer + GLM hooks (auto-thinking, sanitization)

**Tier 2: Local Inference (4 providers, OpenAI Chat compatible)**
9. `ollama.rs` → `wire::OpenAIChat` + None (local) + tool filtering hook
10. `vllm.rs` (NEW) → `wire::OpenAIChat` + None (local) + function calling detection
11. `llama_cpp.rs` (NEW) → `wire::OpenAIChat` + None (local) + structured output
12. `litert_lm.rs` → `wire::LiteRT` (in-process) + Local transport + no tools/streaming

**Tier 3: Native Wire Formats (5 providers, distinct formats)**
13. `anthropic.rs` → `wire::Anthropic` + ApiKeyHeader + extra headers (`anthropic-version`, `anthropic-beta`)
14. `openai.rs` → `wire::OpenAIChat` + `wire::OpenAIResponses` (dual) + Bearer + model routing
15. `cohere.rs` → `wire::Cohere` + Bearer
16. `gemini.rs` → `wire::Gemini` + ApiKeyHeader + schema sanitization hook
17. `bedrock.rs` → `wire::Bedrock` + AwsSigv4

**Tier 4: Special Cases**
18. `conversation.rs` → No change (history management)
19. `replay_provider.rs` → Migrate to `transport::file.rs`
20. `litert_lm.rs` (alternative) → Check if truly in-process or HTTP-based

**Estimated LOC savings per tier:**
- Tier 1: ~8 providers × 850 LOC avg = 6,800 LOC → 1,200 LOC (82% reduction)
- Tier 2: ~4 providers × 400 LOC avg = 1,600 LOC → 300 LOC (81% reduction)
- Tier 3: ~5 providers × 1,100 LOC avg = 5,500 LOC → 2,500 LOC (55% reduction, more format-specific logic)
- **Total: ~13,900 LOC → ~4,000 LOC**

### Phase 6: Remove old code

After all providers migrated, remove `tools.rs` normalization functions and inline serialization code from individual provider files.

---

## Testing Strategy

### Unit Tests (Wire Serializers)

Test each wire format in isolation — no network, no auth, no HTTP.

```rust
// tests/wire/test_openai_chat.rs
#[test]
fn test_serialize_request_with_tools() {
    let request = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![...],
        tools: Some(vec![...]),
        ..Default::default()
    };
    
    let body = wire::OpenAIChat.serialize_request(&request, &capability, Some(&tools))?;
    
    // Assert: body has tools in the right shape
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "edit");
}

#[test]
fn test_parse_sse_chunk_with_tool_call() {
    let chunk = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"edit","arguments":"..."}}]}}]}"#;
    let event = wire::OpenAIChat.parse_sse_chunk(chunk)?;
    
    assert!(matches!(event, Some(StreamEvent::ToolCall(_))));
}
```

**Coverage goal:** 90%+ per serializer. Test:
- Message conversion (all roles)
- Tool serialization
- Thinking/reasoning config
- Error cases (missing required fields)
- SSE parsing (happy path + edge cases)

### Integration Tests (Provider + Transport)

Test providers against real endpoints (or VCR-recorded responses).

```rust
// tests/integration/test_anthropic_e2e.rs
#[tokio::test]
async fn test_anthropic_completion() {
    let provider = ProviderRegistry::new()
        .load_from_config("anthropic", env!("ANTHROPIC_API_KEY"))?
        .get("anthropic")?;
    
    let response = provider.complete(CompletionRequest {
        model: "claude-opus-4-7".into(),
        messages: vec![...],
        ..Default::default()
    }).await?;
    
    assert!(!response.content.is_empty());
}
```

**Coverage goal:** One happy path per provider (model-specific quirks tested in unit tests).

### VCR Recording

Record real API responses and replay in CI:

```rust
// .env.test
ANTHROPIC_API_KEY=sk-ant-test-...
RECORD_CASSETTES=false  // Use recorded responses in CI
CASSETTES_DIR=tests/cassettes/
```

```bash
# Record once (during development)
RECORD_CASSETTES=true cargo test integration::test_anthropic

# Replay in CI
RECORD_CASSETTES=false cargo test integration::test_anthropic
```

### Model Capability Tests

Verify metadata accuracy:

```rust
#[test]
fn test_model_capability_matrix() {
    let registry = ProviderRegistry::default();
    
    // Reasoning models should not support temperature
    let o3_mini = registry.capability("gpt-4-o3-mini")?;
    assert!(o3_mini.reasoning_model);
    
    // GLM-5 should auto-think
    let glm5 = registry.capability("glm-5")?;
    assert_eq!(glm5.required_thinking_mode, Some(ThinkingMode::Enabled));
}
```

### Transport Tests

Test SSE parsing, fallback logic, and error handling:

```rust
#[test]
fn test_http_sse_transport_handles_malformed_json() {
    let malformed = "data: {invalid json";
    let result = transport::HttpSse::parse_event(malformed);
    assert!(matches!(result, Err(TransportError::MalformedEvent)));
}

#[tokio::test]
async fn test_fallback_to_non_streaming() {
    // Simulate SSE parsing failure
    let response = provider.complete_with_fallback(request).await?;
    // Should return non-streamed response
    assert!(!response.content.is_empty());
}
```

---

## Integration Points with Broader System

### With `rustycode-tools`

The tool execution system depends on tool definitions + responses:

```rust
// Before: tools.rs was doing normalization
let normalized_tools = normalize_tools_for_openai(&my_tools)?;
let body = build_request_body(request, &normalized_tools)?;

// After: normalization is in the wire serializer
let mut my_tools: Vec<ToolSchema> = ...;
let body = wire::OpenAIChat::serialize_request(&request, &capability, Some(&my_tools))?;
// Normalization happens inside serialize_request
```

**Impact:** `rustycode-tools` no longer needs `normalize_tools_for_openai`. It passes raw `ToolSchema` and lets the provider handle format conversion.

### With `rustycode-core`

Core execution engine uses providers to call models:

```rust
// Before: Provider-specific knowledge scattered
let provider = get_provider("anthropic")?;
let response = provider.complete(request).await?;

// After: Same API, but provider is now a thin wrapper
let provider = registry.get("anthropic")?;
let response = provider.complete(request).await?;
// (Internally: endpoint selection, wire serialization, transport)
```

**Impact:** No breaking change. Core doesn't care about internals.

### With `rustycode-orchestration`

Milestone/strategy execution needs model routing:

```rust
// Before: Hard-coded per-milestone logic
if milestone.requires_reasoning {
    use_provider("openai-o3-mini")?
} else {
    use_provider("claude-opus")?
}

// After: Automatic endpoint selection
let request = CompletionRequest {
    model: "best-reasoning",  // or specific model
    require_thinking: milestone.requires_reasoning,
    require_tools: milestone.requires_tools,
    ..
};

let endpoint = registry.select_endpoint(&request)?;
let response = endpoint.execute(&request).await?;
```

**Impact:** Cleaner separation — orchestration declares requirements, provider/endpoint satisfies them.

### With Cost Tracking

Cost tracker queries model metadata:

```rust
// Before: Cost data in multiple places
let cost = calculate_cost(model, response, provider_name)?;

// After: Centralized metadata
let capability = registry.capability(model)?;
let cost = response.estimate_cost(model, &cost_tracker)?;
```

**Impact:** Cost tracking doesn't need provider-specific logic.

---

## Open Design Questions

### 1. Endpoint Priority for Dual-API Providers

**OpenRouter** supports both Chat and Responses APIs for the same model. How should we pick?

**Options:**
- A) Always prefer Chat (more stable, more providers support it)
- B) Let user specify in request (`prefer_responses_api: bool`)
- C) Introspect the model and prefer Responses for reasoning models
- D) Lazy evaluation: try Chat first, fall back to Responses on error

**Recommendation:** C + B
- Default: introspect model capability → if reasoning_model, use Responses; else Chat
- Override: `prefer_responses_api` lets user force Responses if needed
- Fallback: if primary fails, try the other (not ideal, but safe)

### 2. Schema Normalization Lossiness

**Gemini** can't handle `$ref`. Should we:

- A) Error loudly (user must flatten manually)
- B) Auto-expand `$ref` (complex, requires full schema context)
- C) Strip `$ref` and log a warning (lossy, but doesn't break)
- D) Convert to string description

**Recommendation:** B (auto-expand where possible)
- Requires passing full schema context to normalizer
- If we can't expand (circular ref, external ref), fall back to C
- Log warnings in all cases so user knows the schema changed

### 3. Local Inference Tooling

**vLLM** and **Llama.cpp** have different ways to handle tools. Should we:

- A) Support all three approaches (native, function calling tokens, structured output)
- B) Only support structured output (manual parsing)
- C) Only support no-tools (text-based tool invocation)

**Recommendation:** A
- Detect model capability (query server/metadata)
- Route to appropriate handler
- Fall back to structured output if native not available

### 4. Thinking Block Extraction

Different models expose thinking differently:
- Anthropic: `content_block_start` type="thinking"
- OpenAI o-series: Not exposed (only `reasoning_tokens` count)
- Zhipu: `reasoning_content` in deltas

Should we:

- A) Expose raw thinking as-is (format-specific)
- B) Normalize to `ThinkingBlock` array (lossy but consistent)
- C) Both: raw in `response.raw_thinking`, normalized in `response.thinking_blocks`

**Recommendation:** C
- User can access raw if they want format-specific details
- Normalized blocks for standard access
- Thinking block counts in token usage for cost tracking

### 5. Error Recovery & Retry

When should we retry vs. fail?

- Transient: network timeout, rate limit → retry with backoff
- Permanent: model not found, auth failure → fail immediately
- Ambiguous: SSE parsing error → try non-streaming fallback once, then fail

Should we:

- A) Let caller decide (expose retry config)
- B) Automatic smart retry (built-in)
- C) Combination (default smart retry, customizable)

**Recommendation:** C
- Default: exponential backoff for transient errors
- User can disable/customize via `RetryPolicy` in `EndpointConfig`
- SSE parse failure is special: try HTTP fallback once, then conventional retry

---

## Success Criteria

### Architectural Quality

- [ ] No provider file exceeds 200 lines
- [ ] No wire serializer exceeds 500 lines
- [ ] Wire serializers are testable in isolation (no HTTP)
- [ ] Transport layer is independent of wire format
- [ ] Model capability metadata covers 95%+ of deployed models

### Code Reduction

- [ ] 75%+ reduction in duplicated serialization/parsing code
- [ ] `provider.rs` reduced from 2,324 lines to <500 lines (types only)
- [ ] `tools.rs` reduced from 2,175 lines to <300 lines (or removed entirely)

### Developer Experience

- [ ] Adding a new OpenAI-compatible endpoint requires <50 lines of code
- [ ] Adding a new wire format requires <500 lines
- [ ] Debugging provider-specific issues can be traced to: wire format, transport, or auth
- [ ] Common mistakes (e.g., forgetting to normalize) are caught at type level

### Performance

- [ ] No regression in latency (serialization is synchronous)
- [ ] Streaming response time unchanged
- [ ] Model metadata loaded once, cached (O(1) lookups)

### Testing

- [ ] 90%+ coverage on all wire serializers
- [ ] 80%+ coverage on providers (mostly config)
- [ ] VCR cassettes for all major providers
- [ ] CI tests 5+ representative models (anthropic, openai, ollama, etc.)

---

## Risk Analysis & Mitigation

### Risk 1: Wire Format Serializer Bugs Breaking Everything

**Impact:** High (all requests flow through serializers)
**Likelihood:** High (complex JSON transformation logic)

**Mitigation:**
- Write serializers incrementally, test thoroughly before migration
- Each serializer has 90%+ unit test coverage
- Dual-run during migration: old code + new code, compare outputs
- VCR cassettes record real API responses; replay ensures compatibility
- Soft launch: enable new serializers as opt-in for 2 weeks, collect telemetry

### Risk 2: Model Capability Metadata Goes Stale

**Impact:** Medium (user sees "not supported" for features that work)
**Likelihood:** High (LLM vendors update models monthly)

**Mitigation:**
- Model capability is loaded from a separate, versioned JSON file
- File lives in a `models/` crate, updated independently of provider code
- Capability lookup has a `loaded_at` timestamp; warn if >30 days old
- Implement `/models --update` command to refresh from vendor APIs
- Fallback: if model unknown, try the request anyway (user can report)

### Risk 3: Endpoint Selection Algorithm Picks Wrong Endpoint

**Impact:** Medium (user request succeeds but takes longer path or uses suboptimal format)
**Likelihood:** Medium (algorithm is heuristic)

**Mitigation:**
- Scoring algorithm is transparent: debug log shows scores + reasoning
- User can override via `prefer_responses_api`, `prefer_streaming` in request
- Endpoint selection tests cover edge cases (dual-API, no-tools model, etc.)
- Fallback chain ensures request succeeds even if primary endpoint picked wrong
- Metrics: track which endpoints are selected and if they fail

### Risk 4: Transport Layer Introduces New Failure Modes

**Impact:** Medium (streaming could break in new ways)
**Likelihood:** High (new abstraction adds complexity)

**Mitigation:**
- Transport is extracted from existing SSE code (not new logic)
- Existing SSE tests continue to pass
- Each transport has separate unit tests (no HTTP)
- Common failure modes are tested: timeout, incomplete response, malformed JSON
- Fallback strategy: non-streaming endpoint available for all providers

### Risk 5: Tool Schema Normalization Is Too Lossy

**Impact:** High (tools silently stop working)
**Likelihood:** Medium (easy to miss edge cases)

**Mitigation:**
- Normalization preserves original schema in `NormalizedSchema`
- Warnings logged for all removed features
- Validation tests ensure normalized schema is valid for the format
- Tool execution layer validates response matches expected schema
- Gradual rollout: log warnings but don't error for 1 release

### Risk 6: Cost Tracking Breaks Due to Missing Model Metadata

**Impact:** Medium (cost estimates wrong)
**Likelihood:** Medium (new models added constantly)

**Mitigation:**
- Cost data is separate from capability data (different JSON files)
- Missing cost data is non-fatal: log warning, return zero cost
- Unit tests for cost estimation with mock models
- Integration test: verify cost for known models matches vendor pricing (within 1%)

### Risk 7: Local Inference Providers Have Inconsistent Behavior

**Impact:** Low (local inference is secondary)
**Likelihood:** High (Ollama, vLLM, Llama.cpp each differ)

**Mitigation:**
- Local providers are tested separately from cloud providers
- Feature matrix is explicit: what each local provider does/doesn't support
- Graceful degradation: if tools not supported, error is clear
- Integration test against local instance (if available) in CI

### Risk 8: Migration Takes Longer Than Expected

**Impact:** High (resources tied up, delays other work)
**Likelihood:** Medium (6 phases, 20+ providers)

**Mitigation:**
- Plan in quarters, not months (reframe as ongoing refactoring)
- Start with Tier 1 (OpenAI Chat, highest impact)
- Parallelize: multiple people can work on different providers
- Phase 1 (type extraction) is non-blocking; can ship without provider migration
- Phase 6 (removal of old code) can wait; coexist indefinitely if needed

---

## Decision Summary

### Core Architecture

| Decision | Rationale | Trade-off |
|----------|-----------|-----------|
| Separate wire format from provider | Eliminates 75% duplication | Adds indirection (worth it) |
| Three orthogonal dimensions | Composable, extensible | More types, more complexity |
| Capability metadata as first-class | Makes impossible states unrepresentable | Metadata can go stale |
| Hook-based model-specific logic | Keeps serializers generic | Hooks are harder to debug |
| Endpoint selection with scoring | Flexible, learnable | Heuristic can be wrong |
| Tool schema normalization | Format-agnostic tools | Lossy (mitigated by warnings) |
| Dual exports during migration | No breaking changes | Maintains legacy code longer |

### Scope

| Item | Included | Excluded |
|------|----------|----------|
| Provider consolidation | ✅ 20 providers into 7 wire formats | ❌ Changes to request/response types |
| Message format normalization | ✅ Tool schema handling | ❌ User-facing API changes |
| Local inference | ✅ Ollama, vLLM, Llama.cpp, LiteRT | ❌ Custom on-device models |
| Testing | ✅ Unit tests for serializers, VCR | ❌ Load testing, soak tests |
| Documentation | ✅ Architecture, migration path, quirks | ❌ User guide (added in later version) |

### Timeline (Recommended)

- **Month 1:** Phase 1–2 (types + schema) — non-breaking
- **Month 2:** Phase 3–4 (wire + transport) — infrastructure
- **Months 3–4:** Phase 5 (provider migration) — bulk of work, can parallelize
- **Month 5:** Phase 6 (cleanup) + testing + docs
- **Month 6:** Soft launch, gather feedback, fix bugs

**Parallel activities:**
- Cost tracking integration (Month 2–3)
- Model capability data (Month 1–2, ongoing)
- VCR cassette recording (Month 3, ongoing)

---

## Appendix: Example Provider Implementations

### Example 1: New Local Provider (vLLM)

```rust
// provider/vllm.rs (~40 lines)
use super::*;

pub fn vllm(base_url: &str) -> ProviderDefinition {
    ProviderDefinition {
        name: "vllm".into(),
        endpoints: vec![
            EndpointConfig {
                url: format!("{}/v1/chat/completions", base_url),
                wire_format: WireFormat::OpenAIChat,
                transport: Transport::Http,
                auth: AuthMethod::None,
                extra_headers: vec![],
                models: vec![
                    // Populated from /models endpoint
                ],
            },
        ],
        global_headers: vec![],
        pre_serialize: Some(|request| {
            // vLLM may need custom param names
            Ok(())
        }),
        post_deserialize: None,
    }
}
```

### Example 2: New Wire Format (Future: Claude API w/ Custom Streaming)

```rust
// wire/claude_custom.rs (~400 lines)
pub struct ClaudeCustomSerializer;

#[async_trait]
impl WireSerializer for ClaudeCustomSerializer {
    fn wire_format(&self) -> WireFormat {
        WireFormat::ClaudeCustom
    }
    
    fn serialize_request(
        &self,
        request: &CompletionRequest,
        capability: &ModelCapability,
        tools: Option<&[ToolSchema]>,
    ) -> Result<serde_json::Value> {
        // Implementation
        Ok(serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(serialize_message).collect::<Vec<_>>(),
            // ...
        }))
    }
    
    fn parse_sse_chunk(&self, data: &str) -> Result<Option<StreamEvent>> {
        // Custom parsing logic
        Ok(Some(StreamEvent::Delta("...".into())))
    }
    
    fn serialize_tools(&self, tools: &[ToolSchema]) -> Vec<serde_json::Value> {
        tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        }).collect()
    }
}
```

---

## Implementation Checklist

- [ ] **Research Phase**
  - [ ] Audit all 20 current provider implementations
  - [ ] Confirm 5 wire formats account for 95%+ of providers
  - [ ] Document all model-specific quirks
  - [ ] Survey downstream dependencies (tools, orchestration, core)

- [ ] **Phase 1: Type Extraction**
  - [ ] Create `types/` module structure
  - [ ] Extract `CompletionRequest`, `CompletionResponse` from `provider.rs`
  - [ ] Move to `types/request.rs`, `types/response.rs`
  - [ ] Add re-exports to `lib.rs` (no breaking change)
  - [ ] Verify existing code still compiles

- [ ] **Phase 2: Typed Schema**
  - [ ] Implement `JsonSchema` builder
  - [ ] Implement `ToolSchema` wrapper
  - [ ] Create `schema/normalizer.rs` with format-specific handlers
  - [ ] Add validation functions
  - [ ] Test against 10+ tool definitions from existing code

- [ ] **Phase 3: Wire Serializers**
  - [ ] Extract Anthropic serializer from `anthropic.rs`
  - [ ] Extract OpenAI Chat serializer (consolidate from 8 providers)
  - [ ] Extract OpenAI Responses serializer
  - [ ] Extract Gemini serializer
  - [ ] Extract Bedrock serializer
  - [ ] Add `WireSerializer` trait
  - [ ] Write 90%+ unit test coverage

- [ ] **Phase 4: Transport Layer**
  - [ ] Extract `Http` transport (non-streaming)
  - [ ] Extract `HttpSse` transport (from existing SSE code)
  - [ ] Add `WebSocket` stub
  - [ ] Add `Local` transport for in-process inference
  - [ ] Add `File` transport for replay
  - [ ] Write transport selection logic

- [ ] **Phase 5: Provider Migration (per-provider)**
  - [ ] Implement `provider/openrouter.rs` using new architecture
  - [ ] Run tests, verify compatibility with cassettes
  - [ ] Repeat for Tier 1 providers (8 total)
  - [ ] Repeat for Tier 2 (4 local providers)
  - [ ] Repeat for Tier 3 (5 native formats)

- [ ] **Phase 6: Cleanup**
  - [ ] Remove old provider implementations
  - [ ] Remove `tools.rs` normalization functions
  - [ ] Update documentation
  - [ ] Remove deprecation warnings

- [ ] **Integration & Testing**
  - [ ] Record VCR cassettes for all 5+ representative models
  - [ ] Run full test suite against live APIs (if possible)
  - [ ] Performance benchmarking (latency regression test)
  - [ ] Metrics/telemetry for endpoint selection accuracy

- [ ] **Rollout**
  - [ ] Feature flag: new architecture behind flag
  - [ ] Soft launch: opt-in for 2 weeks, collect feedback
  - [ ] GA: enable by default
  - [ ] Document migration for downstream users

---

## Provider-Specific Quirks Reference

### Extra Headers

| Provider | Header | Value | Reason |
|----------|--------|-------|--------|
| Anthropic | `x-api-key` | API key | Auth (not Bearer) |
| Anthropic | `anthropic-version` | `2023-06-01` | Required API version |
| Anthropic | `anthropic-beta` | `code-execution-2025-08-25,skills-2025-10-02` | Skills API |
| Anthropic | `anthropic-beta` | `prompt-caching-2024-07-31` | Deferred loading |
| Gemini | `x-goog-api-key` | API key | Auth (not Bearer) |
| Azure | `api-version` | `2024-02-15-preview` | API version in query string |
| OpenRouter | `HTTP-Referer` | `https://rustycode.ai` | Required for attribution |
| OpenRouter | `X-Title` | `RustyCode` | Required for attribution |
| Copilot | `copilot-integration-id` | `vscode-chat` | Required for Copilot API |
| Copilot | `editor-version` | `vscode/1.0.0` | Required for Copilot API |

### Tool Schema Differences

| Wire Format | Tool Shape | Notes |
|-------------|-----------|-------|
| Anthropic | `{name, description, input_schema}` | `annotations`, `defer_loading` extensions |
| OpenAI Chat | `{type: "function", function: {name, description, parameters}}` | Nested `function` wrapper |
| OpenAI Responses | `{type, name, description, parameters}` | Flat, no nesting |
| Gemini | `{functionDeclarations: [{name, description, parameters}]}` | Wrapped in `functionDeclarations` |
| Bedrock | `{toolSpec: {name, description, inputSchema: {json: {...}}}}` | Double-nested |

### Schema Sanitization

Some providers can't handle full JSON Schema. Required transformations:

| Provider | Removes | Flattens |
|----------|---------|----------|
| Gemini | `$schema`, `$defs`, `$ref`, `default: null` | `type: ["string", "null"]` → `type: "string"` |
| Zhipu | `minimum`, `maximum`, `enum`, `additionalProperties` | — |
| OpenAI Responses | — | Adds `strict: true` |

### Reasoning/Thinking Models

| Provider | Models | Mechanism |
|----------|--------|-----------|
| Anthropic | Opus 4.5+, Sonnet 4.5+ | `thinking: {type, budget_tokens}` |
| OpenAI | o-series, GPT-5.x | `max_completion_tokens` + `reasoning_effort` (no `temperature`) |
| Zhipu | GLM-5, GLM-4.5, GLM-4.6, GLM-4.7 | Auto-adds `thinking: {type: "enabled"}` |

### Non-Streaming Support

All HTTP-based providers support `stream: false` for non-streaming responses. The architecture should:
- Default to `HttpSse` transport for streaming
- Fall back to `Http` transport when `request.stream == false`
- Fall back to `Http` when SSE parsing fails
- `Local` and `File` transports ignore streaming entirely

### Tool Support Matrix

| Provider | Native Tools | Stream Tools | Notes |
|----------|:---:|:---:|-------|
| Anthropic | Yes | Yes | Full support + annotations + deferred loading |
| OpenAI | Yes | Yes | Full support |
| Gemini | Yes | Yes | Full support + `functionCallingConfig` |
| Bedrock | Yes | Yes | Full support via Converse API |
| Cohere | Yes | No | v2 Chat API, non-streaming |
| OpenRouter | Yes | Yes | Max 128 tools |
| Azure | Yes | Yes | Full support |
| Zhipu | Yes | Yes | With schema sanitization |
| Copilot | Yes | Yes | Full support |
| Together | Yes | Yes | Full support |
| Mistral | Yes | Yes | Full support |
| Perplexity | Yes | Yes | Full support |
| HuggingFace | Yes | Yes | Full support |
| Ollama | Partial | No | Tool schema passes through but model ignores; tool calls come as text |
| vLLM | Partial | No | Function calling via special tokens; varies by model and config |
| Llama.cpp | No | No | No native tools; can parse structured output with constraints |
| LiteRT | **No** | **No** | Local inference, no tool support |
| Replay | N/A | N/A | Records/replays whatever the wrapped provider does |

### Local Inference Specifics

**Ollama (0.1.31+)**
- Tools: Schema is accepted but model typically doesn't use it; response comes as text
- Streaming: Standard SSE, but tool calls and thinking blocks come as text tokens
- Images: Base64 in `content` (non-standard)
- Special params: `keep_alive` (how long to keep model in memory)
- Model format: `name:tag` (e.g., `mistral:7b`, `neural-chat:latest`)
- Context: Model-specific; Ollama respects system `num_ctx` if provided

**vLLM**
- Tools: Function calling via special tokens (e.g., `<|im_function_calls|>`) or structured output
- Streaming: Mostly standard OpenAI SSE, but some models return tool calls as text tokens
- Special params: `num_predict` (max tokens), `num_ctx` (context length)
- Model format: `path/to/model` or HuggingFace model ID
- Feature detection: Must check server's `/models` endpoint to determine function calling capability
- Streaming tool calls: Not supported; entire tool call comes in a single delta

**Llama.cpp**
- Tools: No native support; can use structured output (JSON mode) and parse manually
- Streaming: Standard OpenAI SSE
- Special params: `n_predict` (max tokens), `n_ctx` (context length)
- Model format: Local file path (`.gguf`)
- Grammar: Can use GBNF grammar to constrain output to tool call format
- Example structured output request:
  ```json
  {
    "messages": [...],
    "stream": true,
    "tools": [...],
    "response_format": {"type": "json_schema", "json_schema": {...}}
  }
  ```

**LiteRT**
- No HTTP; in-process inference
- Models: `.tflite` files or quantized format
- Device support: CPU, GPU (platform-dependent)
- Token counting: Via vocab size, not API call
- No streaming, no tools, no thinking
- Latency: 100ms–5s depending on model and device

---

## Cross-Project Survey

Surveyed 5 external AI coding tools to validate the proposed architecture and identify patterns worth adopting.

### OpenCode (TypeScript, 10+ providers, 6 wire formats)

**Architecture:** `Protocol<Body, Frame, Event, State>` generic — the cleanest abstraction found.

```typescript
// OpenCode's core abstraction
interface Protocol<Body, Frame, Event, State> {
  serialize(body: Body): Promise<RequestInit>;
  events(r: Response): AsyncGenerator<Frame>;
  map(frame: Frame): Event[];
  reduce(state: State, event: Event): State;
}
```

**Key patterns:**
- **Generic over body type** — Body is not always JSON; Anthropic uses JSON, Gemini uses JSON, future formats could use protobuf
- **Composable auth** — Each protocol specifies auth independently; DeepSeek reuses OpenAIChat.protocol + custom headers
- **Multi-API per provider** — OpenAI has separate Chat and Responses protocols; provider selects at runtime
- **Streaming state machine** — `reduce()` accumulates partial SSE events into a typed state object

**What we adopt:** Protocol concept → `WireSerializer` trait. Their generic type parameters translate to our `serialize_request`, `parse_sse_chunk`, `deserialize_response`.

**What we do differently:** Explicit `Transport` enum (they embed it implicitly in Protocol). More composable auth (they embed auth in Protocol).

### Codex (TypeScript, single wire format)

**Architecture:** Single OpenAI Chat wire format with dual transport (HTTP + WebSocket).

```typescript
// Codex uses a single format for everything
interface ChatCompletionRequest { model, messages, tools, stream, ... }
```

**Key patterns:**
- **One format, all providers** — Even non-OpenAI providers use the Chat Completions shape
- **Transport abstraction** — `Http` and `WebSocket` transports share the same body shape
- **Provider = config** — Providers are thin config objects (base_url, auth, headers)

**What we adopt:** Provider-as-config pattern. Our `ProviderDefinition` is similarly thin.

**What we do differently:** We support multiple wire formats (Codex forces everything into OpenAI Chat). This matters for Gemini and Bedrock whose native formats have features OpenAI Chat can't express.

### Gemini CLI (Go, provider-locked)

**Architecture:** Hardcoded to Gemini API only.

```go
// Gemini CLI is locked to a single provider
type GeminiClient struct {
    apiKey string
    model  string
}
```

**Key patterns:**
- **Deep integration** — Full access to Gemini-specific features (grounding, code execution, safety settings)
- **No abstraction needed** — Single provider means no abstraction overhead

**What we learn:** When a provider has unique features (Gemini grounding, Anthropic prompt caching), those features need first-class support — not just "extra headers." Our `ProviderOptions` typed extensions handle this.

### Claude Code (TypeScript, direct SDK)

**Architecture:** Direct Anthropic SDK usage, no abstraction layer.

```typescript
// Claude Code uses the Anthropic SDK directly
import Anthropic from '@anthropic-ai/sdk';
const client = new Anthropic({ apiKey });
```

**Key patterns:**
- **SDK-native types** — Uses SDK types for messages, tools, thinking blocks
- **Provider-specific features** — Extended thinking, prompt caching, tool annotations all first-class

**What we learn:** SDK-native types are more type-safe than generic abstractions. Our typed `ToolSchema` and `ThinkingBlock` mirror this approach — we just layer them behind a generic trait.

### Kilocode (TypeScript, AI SDK layer)

**Architecture:** Vercel AI SDK as universal abstraction layer.

```typescript
// Kilocode wraps everything in AI SDK
import { generateText, streamText } from 'ai';
const result = await generateText({ model: provider(modelId), prompt });
```

**Key patterns:**
- **SDK handles everything** — Wire format, transport, auth all hidden behind `generateText()`
- **Provider adapters** — Each provider maps to an AI SDK provider module
- **Streaming abstraction** — `streamText()` returns a standard async iterable regardless of provider

**What we adopt:** Single entry point pattern. Our `ProviderDefinition::execute()` is similar — caller doesn't need to know which wire format or transport is used.

**What we do differently:** We expose the layers for debugging. Kilocode's abstraction makes it hard to diagnose why a specific provider fails. Our explicit `WireFormat × Transport × Auth` dimensions help with debugging.

### Survey Summary

| Pattern | OpenCode | Codex | Gemini CLI | Claude Code | Kilocode | RustyCode (Proposed) |
|---------|----------|-------|------------|-------------|----------|---------------------|
| Multiple wire formats | Yes (6) | No (1) | No (1) | No (1) | Yes (via SDK) | Yes (5-7) |
| Transport abstraction | Implicit | Explicit | No | No | SDK handles | Explicit enum |
| Auth composition | In protocol | Config | Config | SDK handles | SDK handles | Separate enum |
| Provider-as-config | Partial | Full | N/A | N/A | Full | Full |
| Extension support | No | No | Deep (Gemini) | Deep (Anthropic) | No | Typed per-provider |
| Debuggability | Medium | High | High | High | Low | High (explicit layers) |

---

## Extension & Model API Support

Different providers expose unique capabilities beyond basic chat completion. These need typed, discoverable support — not raw JSON escape hatches.

### Design: Typed ProviderOptions

Each provider gets its own options struct. Unknown providers use `HttpOverrides` as an escape hatch.

```rust
/// Per-provider extension options.
/// Attached to `EndpointConfig` and consulted during serialization.
#[derive(Debug, Clone)]
pub enum ProviderOptions {
    Anthropic(AnthropicOptions),
    OpenAI(OpenAIOptions),
    Gemini(GeminiOptions),
    Bedrock(BedrockOptions),
    Cohere(CohereOptions),
    Ollama(OllamaOptions),
    OpenRouter(OpenRouterOptions),
    Azure(AzureOptions),
    Copilot(CopilotOptions),
    /// Escape hatch for unknown providers — raw header/query/body overrides
    HttpOverrides(HttpOverrides),
}

/// Anthropic-specific extensions.
#[derive(Debug, Clone)]
pub struct AnthropicOptions {
    /// Enable prompt caching for system/messages.
    pub cache_control: bool,
    /// Beta features to opt into.
    pub beta_features: Vec<String>,
    /// Enable extended thinking with budget.
    pub thinking_budget: Option<u32>,
    /// Tool annotations (name → annotations).
    pub tool_annotations: HashMap<String, ToolAnnotations>,
    /// Deferred tool loading — only send full schema when tool is called.
    pub defer_tool_loading: bool,
}

/// Gemini-specific extensions.
#[derive(Debug, Clone)]
pub struct GeminiOptions {
    /// Enable Google Search grounding.
    pub grounding: bool,
    /// Safety settings (category → threshold).
    pub safety_settings: HashMap<String, String>,
    /// Response MIME type override.
    pub response_mime_type: Option<String>,
    /// Enable code execution tool.
    pub code_execution: bool,
    /// Thinking config (budget tokens).
    pub thinking_budget: Option<u32>,
}

/// OpenAI-specific extensions.
#[derive(Debug, Clone)]
pub struct OpenAIOptions {
    /// API preference: ChatCompletions vs Responses.
    pub api_preference: ApiPreference,
    /// Reasoning effort for o-series models.
    pub reasoning_effort: Option<EffortLevel>,
    /// Enable structured output with JSON schema.
    pub structured_output: Option<JsonSchema>,
    /// Enable web search tool (Responses API).
    pub web_search: bool,
    /// File search tool (Responses API).
    pub file_search: Option<FileSearchConfig>,
}

/// Ollama-specific extensions.
#[derive(Debug, Clone)]
pub struct OllamaOptions {
    /// Keep model in memory for N seconds after request.
    pub keep_alive: Option<u64>,
    /// Number of context tokens.
    pub num_ctx: Option<u32>,
    /// Number of predict tokens.
    pub num_predict: Option<u32>,
    /// Seed for deterministic generation.
    pub seed: Option<u64>,
    /// Whether to strip tools (local models can't use them).
    pub strip_tools: bool,
}

/// Bedrock-specific extensions.
#[derive(Debug, Clone)]
pub struct BedrockOptions {
    /// AWS region override.
    pub region: Option<String>,
    /// Model ID prefix (e.g., "anthropic.", "meta.").
    pub model_prefix: Option<String>,
    /// Cross-region inference profile.
    pub inference_profile: Option<String>,
    /// Guardrail configuration.
    pub guardrail: Option<GuardrailConfig>,
}

/// Escape hatch for unknown or minimally-supported providers.
#[derive(Debug, Clone)]
pub struct HttpOverrides {
    /// Extra HTTP headers to add.
    pub extra_headers: Vec<(String, String)>,
    /// Query parameters to append.
    pub query_params: Vec<(String, String)>,
    /// JSON patches to apply to the serialized body.
    pub body_patches: Vec<JsonPatch>,
}

/// A JSON patch operation (RFC 6902 subset).
#[derive(Debug, Clone)]
pub struct JsonPatch {
    pub op: PatchOp,
    pub path: String,
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum PatchOp {
    Add,
    Remove,
    Replace,
}
```

### Usage Examples

**Anthropic prompt caching:**
```rust
let endpoint = EndpointConfig {
    url: "https://api.anthropic.com/v1/messages".into(),
    wire_format: WireFormat::Anthropic,
    transport: Transport::HttpSse,
    auth: AuthMethod::ApiKeyHeader {
        header: "x-api-key".into(),
        key: api_key,
    },
    options: ProviderOptions::Anthropic(AnthropicOptions {
        cache_control: true,
        beta_features: vec!["prompt-caching-2024-07-31".into()],
        ..Default::default()
    }),
    ..Default::default()
};

// Wire serializer checks options during serialization:
// → Adds cache_control breakpoints to system prompt and large messages
```

**Gemini grounding:**
```rust
let options = ProviderOptions::Gemini(GeminiOptions {
    grounding: true,
    safety_settings: HashMap::from([
        ("HARM_CATEGORY_HARASSMENT".into(), "BLOCK_NONE".into()),
    ]),
    ..Default::default()
});

// Wire serializer adds Google Search tool when grounding=true:
// → tools: [{ "google_search": {} }]
```

**Ollama keep-alive:**
```rust
let options = ProviderOptions::Ollama(OllamaOptions {
    keep_alive: Some(300), // 5 minutes
    strip_tools: true,
    ..Default::default()
});

// Wire serializer adds keep_alive to body:
// → { "model": "...", "keep_alive": 300, ... }
// → Strips tools from request (strip_tools=true)
```

### How ProviderOptions Flow Through the System

```
1. User creates EndpointConfig with ProviderOptions::Anthropic(...)
2. WireSerializer::serialize_request() receives &ProviderOptions
3. Anthropic serializer checks:
   - cache_control? → add cache_control breakpoints to system/messages
   - thinking_budget? → add thinking block to request
   - tool_annotations? → add annotations to tool definitions
4. Transport sends the serialized body
5. Response comes back
6. WireSerializer::deserialize_response() checks options for:
   - thinking_budget? → extract thinking blocks
   - cache_control? → extract cache metrics from usage
```

The key insight: **ProviderOptions are consulted by the wire serializer, not the transport.** Transport doesn't know about caching or grounding — it just delivers bytes.

---

## Refined Protocol Layer

Incorporating OpenCode's Protocol pattern into the Rust architecture with type-safe generics.

### Protocol Trait (Adapted from OpenCode)

```rust
/// Core serialization protocol for a wire format.
///
/// Adapted from OpenCode's Protocol<Body, Frame, Event, State> generic.
/// In Rust, we use concrete types (serde_json::Value) for flexibility
/// but keep the same separation of concerns.
pub trait Protocol: Send + Sync {
    /// The wire format this protocol handles.
    fn format(&self) -> WireFormat;

    /// Convert a CompletionRequest into a JSON body for this format.
    fn serialize_body(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
        options: Option<&ProviderOptions>,
    ) -> Result<serde_json::Value>;

    /// Parse a non-streaming response body.
    fn parse_response(
        &self,
        body: &serde_json::Value,
        options: Option<&ProviderOptions>,
    ) -> Result<CompletionResponse>;

    /// Parse a single SSE data line into a stream event.
    /// Returns None if the line is a keep-alive or should be skipped.
    fn parse_sse_event(
        &self,
        data: &str,
        options: Option<&ProviderOptions>,
    ) -> Result<Option<StreamEvent>>;

    /// Convert tool definitions into this format's tool schema.
    fn serialize_tools(
        &self,
        tools: &[ToolSchema],
        options: Option<&ProviderOptions>,
    ) -> Vec<serde_json::Value>;
}
```

### Route: Protocol + Transport + Auth + Endpoint

```rust
/// A complete request pipeline: wire format + delivery + auth + URL.
///
/// This is the unit of configuration. Providers compose one or more Routes.
pub struct Route {
    /// Where to send requests.
    pub endpoint: String,
    /// How to serialize/deserialize messages.
    pub protocol: Box<dyn Protocol>,
    /// How to deliver requests (HTTP, SSE, WebSocket, etc.).
    pub transport: Transport,
    /// How to authenticate requests.
    pub auth: AuthMethod,
    /// Provider-specific options consulted during serialization.
    pub options: Option<ProviderOptions>,
    /// Extra HTTP headers applied to all requests.
    pub extra_headers: Vec<(String, String)>,
    /// Models available on this route.
    pub models: Vec<ModelConfig>,
    /// Fallback strategy if primary transport fails.
    pub fallback: TransportFallbackStrategy,
}

impl Route {
    /// Execute a completion request through this route.
    pub async fn execute(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<CompletionResponse> {
        // 1. Serialize body using protocol
        let body = self.protocol.serialize_body(
            request,
            tools,
            self.options.as_ref(),
        )?;

        // 2. Build HTTP request with auth + headers
        let http_request = self.build_http_request(&body)?;

        // 3. Execute via transport
        match self.transport {
            Transport::Http => {
                let response = self.transport_http(http_request).await?;
                self.protocol.parse_response(&response, self.options.as_ref())
            }
            Transport::HttpSse => {
                // Try SSE first, fall back to HTTP if parsing fails
                self.transport_sse_with_fallback(http_request, request, tools).await
            }
            Transport::WebSocket => {
                self.transport_websocket(http_request, request, tools).await
            }
            Transport::Local => {
                Err(ProviderError::NotSupported("local transport via Route"))
            }
            Transport::File => {
                Err(ProviderError::NotSupported("file transport via Route"))
            }
        }
    }

    /// Execute with streaming via SSE.
    pub async fn execute_stream(
        &self,
        request: &CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<BoxStream<'static, StreamChunk>> {
        let body = self.protocol.serialize_body(
            request,
            tools,
            self.options.as_ref(),
        )?;

        let http_request = self.build_http_request(&body)?;
        self.transport_sse_stream(http_request).await
    }
}
```

### Provider: Named Collection of Routes

```rust
/// A named provider with one or more routes (endpoints).
///
/// Example: OpenAI has routes for Chat Completions and Responses API.
/// Example: OpenRouter has a single route for Chat Completions.
/// Example: Ollama has a single route for local inference.
pub struct Provider {
    /// Human-readable provider name (e.g., "anthropic", "openai").
    pub name: String,
    /// Available routes. Multiple routes allow multi-API support.
    pub routes: Vec<Route>,
    /// Provider-level headers applied to ALL routes.
    pub global_headers: Vec<(String, String)>,
    /// Model capability metadata.
    pub model_capabilities: HashMap<String, ModelCapability>,
}

impl Provider {
    /// Select the best route for a given request.
    pub fn select_route(
        &self,
        request: &CompletionRequest,
    ) -> Result<&Route> {
        // Filter routes that support the requested model
        let candidates: Vec<_> = self.routes.iter()
            .filter(|r| r.models.iter().any(|m| m.name == request.model))
            .collect();

        if candidates.is_empty() {
            return Err(ProviderError::ModelNotSupported(request.model.clone()));
        }

        // Score based on preferences
        let prefer_streaming = request.stream;
        let prefer_responses = matches!(
            request.api_mode,
            Some(ApiMode::Responses) | Some(ApiMode::ResponsesWs)
        );

        let best = candidates.into_iter().max_by_key(|route| {
            let mut score = 0i32;
            if prefer_streaming && route.transport == Transport::HttpSse {
                score += 1000;
            }
            if prefer_responses && route.protocol.format() == WireFormat::OpenAIResponses {
                score += 500;
            }
            score
        });

        best.ok_or(ProviderError::NoEndpointAvailable)
    }

    /// Execute a completion request via the best route.
    pub async fn complete(
        &self,
        request: CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<CompletionResponse> {
        let route = self.select_route(&request)?;
        route.execute(&request, tools).await
    }

    /// Execute a streaming completion request.
    pub async fn complete_stream(
        &self,
        request: CompletionRequest,
        tools: Option<&[ToolSchema]>,
    ) -> Result<BoxStream<'static, StreamChunk>> {
        let route = self.select_route(&request)?;
        route.execute_stream(&request, tools).await
    }
}
```

### Declarative Provider Definitions

Providers are defined declaratively — no imperative code needed for standard cases:

```rust
/// Build the Anthropic provider definition.
pub fn anthropic_provider(api_key: SecretString) -> Provider {
    Provider {
        name: "anthropic".into(),
        routes: vec![
            Route {
                endpoint: "https://api.anthropic.com/v1/messages".into(),
                protocol: Box::new(wire::AnthropicProtocol),
                transport: Transport::HttpSse,
                auth: AuthMethod::ApiKeyHeader {
                    header: "x-api-key".into(),
                    key: api_key,
                },
                options: Some(ProviderOptions::Anthropic(AnthropicOptions {
                    cache_control: true,
                    beta_features: vec!["prompt-caching-2024-07-31".into()],
                    ..Default::default()
                })),
                extra_headers: vec![
                    ("anthropic-version".into(), "2023-06-01".into()),
                ],
                models: vec![
                    // Populated from model registry
                ],
                fallback: TransportFallbackStrategy::sse_with_http_fallback(),
            },
        ],
        global_headers: vec![],
        model_capabilities: anthropic_model_capabilities(),
    }
}

/// Build the OpenRouter provider definition.
pub fn openrouter_provider(api_key: SecretString) -> Provider {
    Provider {
        name: "openrouter".into(),
        routes: vec![
            Route {
                endpoint: "https://openrouter.ai/api/v1/chat/completions".into(),
                protocol: Box::new(wire::OpenAIChatProtocol),
                transport: Transport::HttpSse,
                auth: AuthMethod::Bearer(api_key),
                options: None,
                extra_headers: vec![
                    ("HTTP-Referer".into(), "https://rustycode.ai".into()),
                    ("X-Title".into(), "RustyCode".into()),
                ],
                models: vec![],
                fallback: TransportFallbackStrategy::sse_with_http_fallback(),
            },
        ],
        global_headers: vec![],
        model_capabilities: Default::default(), // Inherits from OpenAI + adds custom
    }
}

/// Build the Ollama provider definition.
pub fn ollama_provider(base_url: &str) -> Provider {
    Provider {
        name: "ollama".into(),
        routes: vec![
            Route {
                endpoint: format!("{}/v1/chat/completions", base_url),
                protocol: Box::new(wire::OpenAIChatProtocol),
                transport: Transport::Http,
                auth: AuthMethod::None,
                options: Some(ProviderOptions::Ollama(OllamaOptions {
                    strip_tools: true,
                    keep_alive: Some(300),
                    ..Default::default()
                })),
                extra_headers: vec![],
                models: vec![],
                fallback: TransportFallbackStrategy::no_fallback(),
            },
        ],
        global_headers: vec![],
        model_capabilities: Default::default(),
    }
}
```

### How This Collapses the Current Code

**Current:** 17 provider files, each 300–1,361 lines, with ~20K LOC of duplicated serialization.

**Proposed:**
- **7 protocol implementations** (~2,500 LOC total): Anthropic, OpenAIChat, OpenAIResponses, Gemini, Bedrock, Cohere, LiteRT
- **20 provider definitions** (~800 LOC total): Each is a declarative builder function, 30–50 lines
- **5 transport implementations** (~1,000 LOC total): Http, HttpSse, WebSocket, Local, File
- **5 auth implementations** (~400 LOC total): Bearer, ApiKeyHeader, AwsSigv4, GitHubToken, None
- **Total: ~4,700 LOC** (77% reduction from ~20K duplicated LOC)

The key architectural win: **adding a new OpenAI-compatible provider is 30 lines of declarative config** — no serialization code, no SSE parsing, no auth logic.

---

## Auth Layer: What Already Exists & Gaps

### Existing Components

| Component | Status | Location |
|-----------|--------|----------|
| OAuth 2.0 + PKCE | Done | `rustycode-auth/src/oauth.rs` |
| API key via env vars | Done | `rustycode-config/src/parser.rs` |
| OS keychain storage | Done | `rustycode-auth/src/token_store.rs` (keyring 4.0) |
| GitHub Copilot device flow | Done | `rustycode-auth/src/github_copilot.rs` |
| `secrecy::SecretString` | Done | All tokens/redacted in debug |
| Provider configs | Done | `ProvidersConfig` with per-provider settings |
| Headless detection | Done | Falls back to manual URL input |

### Auth Wiring Gaps

Current credential resolution chain: `env var → config file → ???`

The OS keyring (`TokenStore`) exists but is **never checked** in the resolution chain. OAuth tokens can be stored but are never retrieved during LLM calls.

| Gap | Impact |
|-----|--------|
| No `rustycode login` CLI command | Users must set env vars manually |
| Keyring not in resolve chain | OAuth tokens stored but never used |
| No token refresh lifecycle | OAuth tokens expire silently |
| Config API keys are plain text | Keys visible on disk |
| No auth status command | Can't verify which providers are authenticated |
| No generic OAuth login | Only Copilot has a login flow |

### Redesign Implications

The `auth/` module in the new architecture must:

1. **Close the keyring gap** — Add `TokenStore` lookup to credential resolution:
   `env var → config file → keyring → prompt login`

2. **Wire auth into `Route`** — Each `Route` carries an `AuthMethod`, but credential *resolution* should follow the full chain, not just the static value in `AuthMethod`.

3. **Token refresh middleware** — Before each LLM call via `Route::execute()`, check `expires_at` and auto-refresh if needed. This belongs in the transport layer, not the protocol layer.

4. **Multi-provider sessions** — The `ProviderRegistry` should track auth state per provider, enabling simultaneous Anthropic + OpenAI authentication.

These gaps are addressed in the implementation plan as part of the auth module tasks.

## Multi-Account Support (Horizontal Scaling)

### Problem

Users with multiple API subscriptions on the same provider (e.g., 3 Anthropic keys) want to distribute requests across them to increase throughput. This is NOT a separate pool abstraction — it's a natural extension of the Route model.

### Design

Each API key = a separate `Route` with the same Protocol/Endpoint but different `AuthMethod`. The `Provider`'s `select_route()` method applies a configurable selection strategy across equivalent routes.

```
Provider "anthropic"
├── Route 1: AnthropicProtocol + HttpTransport + Bearer(key_1) + us-east-1
├── Route 2: AnthropicProtocol + HttpTransport + Bearer(key_2) + us-east-1
└── Route 3: AnthropicProtocol + HttpTransport + Bearer(key_3) + us-west-2
```

### RouteSelection Strategy

```rust
#[derive(Debug, Clone, Default)]
pub enum RouteSelection {
    /// First available route (default, current behavior)
    #[default]
    First,
    /// Round-robin across routes with matching model capability
    RoundRobin,
    /// Random selection
    Random,
    /// Fewest concurrent in-flight requests (requires runtime state)
    LeastLoaded,
}
```

The Provider holds an `AtomicUsize` counter for round-robin state — no mutex, no async runtime dependency.

### Config Surface (config.json)

```json
{
  "providers": {
    "anthropic": {
      "strategy": "round_robin",
      "accounts": [
        { "api_key_env": "ANTHROPIC_API_KEY_1" },
        { "api_key_env": "ANTHROPIC_API_KEY_2" },
        { "api_key_env": "ANTHROPIC_API_KEY_3" }
      ]
    },
    "openai": {
      "strategy": "round_robin",
      "accounts": [
        { "api_key_env": "OPENAI_API_KEY_1" },
        { "api_key_env": "OPENAI_API_KEY_2" }
      ]
    }
  }
}
```

Each account entry can carry provider-specific overrides:

```json
{
  "providers": {
    "anthropic": {
      "strategy": "round_robin",
      "accounts": [
        {
          "api_key_env": "ANTHROPIC_API_KEY_1",
          "priority": 1
        },
        {
          "api_key_env": "ANTHROPIC_API_KEY_2",
          "priority": 2,
          "headers": { "x-custom": "value" }
        }
      ]
    }
  }
}
```

### Construction

At provider construction time, each `accounts[]` entry expands into a `Route`:

1. Resolve auth via credential chain: `env var → config file → keyring → prompt`
2. Build `AuthMethod` from resolved credential
3. Share the same `Protocol` and `Endpoint` as sibling routes
4. Apply optional per-account `ProviderOptions` overrides

The `Provider::select_route()` method:

```rust
fn select_route(&self, model: &str) -> Option<&Route> {
    let candidates: Vec<&Route> = self.routes.iter()
        .filter(|r| r.supports_model(model))
        .collect();

    match self.strategy {
        RouteSelection::First => candidates.first().copied(),
        RouteSelection::RoundRobin => {
            let idx = self.counter.fetch_add(1, Ordering::Relaxed) % candidates.len();
            Some(candidates[idx])
        }
        RouteSelection::Random => candidates.choose(&mut rand::thread_rng()).copied(),
        RouteSelection::LeastLoaded => candidates.iter()
            .min_by_key(|r| r.in_flight.load(Ordering::Relaxed))
            .copied(),
    }
}
```

### Key Design Decisions

1. **Not a separate pool abstraction** — Routes already form a pool. Adding a `ProviderPool` would be over-engineering.
2. **Strategy is per-provider, not per-model** — All models under a provider share the same route pool. Per-model routing can be added later via model filters on routes.
3. **`LeastLoaded` requires per-Route atomic counter** — Incremented on `Route::execute()`, decremented on drop. Lightweight, no async.
4. **Fallback is automatic** — If a route fails (429 rate limit, auth error), `select_route()` can be called again to pick the next candidate. The retry logic lives in the transport layer.
5. **Single-account is the degenerate case** — One account = one route = `RouteSelection::First` (default). Zero config change for existing users.
