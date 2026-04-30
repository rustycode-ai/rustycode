# ADR 003: Macro-Based Provider Boilerplate Reduction

## Status
**Proposed** - Under consideration

## Context
The codebase has 13 LLM providers with significant code duplication:
- **Average**: 270 lines per provider
- **Common patterns**: 87% duplicate code
- **Total duplication**: ~3,500 lines of repetitive code

### Duplication Examples

#### 1. API Key Retrieval (13 duplicates)
```rust
fn get_api_key(&self) -> Result<String, ProviderError> {
    let config_key = self.config.api_key.as_ref()
        .map(|k| k.expose_secret().to_string());
    let env_key = std::env::var("PROVIDER_API_KEY").ok();
    config_key.or(env_key).ok_or_else(|| ...)
}
```

#### 2. Trait Boilerplate (13 duplicates)
```rust
fn name(&self) -> &'static str { "provider" }
async fn is_available(&self) -> bool { ... }
async fn list_models(&self) -> Result<Vec<String>> { ... }
fn config(&self) -> Option<&ProviderConfig> { ... }
```

#### 3. SSE Streaming Parsing (10 duplicates)
25+ lines of OpenAI-compatible SSE parsing

## Decision
Create declarative macros to eliminate boilerplate while maintaining:
- Type safety
- Code clarity
- Flexibility for provider-specific needs

### Macro Suite

#### 1. `get_api_key!` Macro
**Reduces**: 13 lines → 1 line (92% reduction)

```rust
// Before
fn get_api_key(&self) -> Result<String, ProviderError> {
    let config_key = self.config.api_key.as_ref()
        .map(|k| k.expose_secret().to_string());
    let env_key = std::env::var("PROVIDER_API_KEY").ok();
    config_key.or(env_key).ok_or_else(||
        ProviderError::Configuration(...)
    )
}

// After
fn get_api_key(&self) -> Result<String, ProviderError> {
    get_api_key!(self, "PROVIDER_API_KEY")
}
```

#### 2. `provider_common!` Macro
**Reduces**: 30 lines → 1 line (97% reduction)

```rust
// Before
fn name(&self) -> &'static str { "provider" }
async fn is_available(&self) -> bool { ... }
async fn list_models(&self) -> Result<Vec<String>> { ... }
fn config(&self) -> Option<&ProviderConfig> { ... }

// After
provider_common!("provider", vec!["model1".to_string()]);
```

#### 3. `parse_openai_sse!` Macro
**Reduces**: 25 lines → 1 line (96% reduction)

```rust
// Before
for line in text.lines() {
    if line.starts_with("data: ") {
        let json_str = line.trim_start_matches("data: ").trim();
        if json_str == "[DONE]" { continue; }
        if let Ok(data) = serde_json::from_str::<Value>(json_str) {
            // ... 20 more lines ...
        }
    }
}

// After
parse_openai_sse!(text, chunks);
```

## Consequences

### Positive
- **Code reduction**: ~1,850 lines saved (52% per provider)
- **Consistency**: All providers use same patterns
- **Maintenance**: Single source of truth
- **Bug fixes**: Fix once, apply everywhere
- **Onboarding**: Easier to add new providers

### Negative
- **Macro magic**: Less explicit code
- **Debugging**: Stack traces in macros
- **Flexibility**: Macros limit customization
- **Learning curve**: Must learn macro system

### Mitigation
- Keep macros simple and declarative
- Document macro expansion
- Provide examples for each macro
- Keep provider-specific methods outside macros

## Implementation

### Phase 1: Core Macros (COMPLETED)
- [x] `get_api_key!` - API key retrieval
- [x] `provider_common!` - Trait methods
- [x] `parse_openai_sse!` - SSE parsing
- [x] `convert_messages!` - Message conversion
- [x] `build_http_client!` - HTTP client builder

### Phase 2: Provider Migration (PENDING)
- [ ] OpenAI provider
- [ ] Anthropic provider
- [ ] Azure provider
- [ ] 10 remaining providers

### Phase 3: Validation (PENDING)
- [ ] All providers compile
- [ ] No functionality regressions
- [ ] Performance benchmarks
- [ ] Documentation updated

## Example: Before vs After

### Before (270 lines)
```rust
impl Provider {
    fn get_api_key(&self) -> Result<String, ProviderError> {
        let config_key = self.config.api_key.as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("PROVIDER_API_KEY").ok();
        config_key.or(env_key).ok_or_else(||
            ProviderError::Configuration(...)
        )
    }
}

#[async_trait]
impl LLMProvider for Provider {
    fn name(&self) -> &'static str { "provider" }
    async fn is_available(&self) -> bool {
        self.config.api_key.as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty())
    }
    async fn list_models(&self) -> Result<Vec<String>> {
        Ok(vec!["model1".to_string()])
    }
    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
    // ... plus SSE parsing, message conversion, etc.
}
```

### After (120 lines)
```rust
impl Provider {
    fn get_api_key(&self) -> Result<String, ProviderError> {
        get_api_key!(self, "PROVIDER_API_KEY")
    }
}

#[async_trait]
impl LLMProvider for Provider {
    provider_common!("provider", vec!["model1".to_string()]);

    async fn complete(&self, request: CompletionRequest) -> ... {
        // Provider-specific implementation
    }

    async fn complete_stream(&self, request: CompletionRequest) -> ... {
        let sse_stream = bytes_stream.map(|chunk| {
            let text = String::from_utf8_lossy(&chunk);
            let mut chunks = Vec::new();
            parse_openai_sse!(text, chunks);
            Ok(chunks.join(""))
        });
        Ok(Box::pin(sse_stream))
    }
}
```

## Alternatives Considered

1. **No macros (status quo)**: Rejected (maintenance burden)
2. **Code generation**: Rejected (complex build process)
3. **Trait-based defaults**: Rejected (less flexible)
4. **External macro crate**: Rejected (custom needs)

## References
- [Provider V2 Module](../../crates/rustycode-llm/src/provider.rs)

## Date
2024-03-14

## Author
rustycode-llm ensemble
