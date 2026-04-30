# ADR 001: Provider v2 API Design

## Status
**Accepted** - Implemented in rustycode-llm v0.1.0

## Context
The original provider API (`Provider` trait in `provider.rs`) had several limitations:
- Tight coupling with legacy configuration format
- Inconsistent error handling across providers
- No support for modern async patterns
- Difficulty adding new providers
- Mixed concerns (configuration, execution, serialization)

## Decision
Create a new `provider` module with:
1. **Clean separation of concerns**
   - `ProviderConfig` - Configuration only
   - `LLMProvider` trait - Core provider interface
   - `ProviderError` - Comprehensive error types
   - `CompletionRequest/Response` - Request/response types

2. **Modern async patterns**
   - Use `async_trait` for trait methods
   - Support streaming with `Stream<Item = StreamChunk>`
   - Pin-based streaming for flexibility

3. **Unified error handling**
   - `ProviderError` enum with specific variants
   - Consistent error propagation
   - Clear error recovery strategies

4. **Secret-first design**
   - Use `SecretString` from secrecy crate
   - Zeroization on drop for security
   - No accidental secret leakage in logs

## Consequences

### Positive
- **Type safety**: Compile-time guarantees on configuration
- **Security**: Secrets properly isolated and zeroized
- **Extensibility**: Easy to add new providers
- **Testability**: Clear interfaces for mocking
- **Error handling**: Predictable error patterns

### Negative
- **Migration cost**: Existing code needs updates
- **Learning curve**: New concepts (`SecretString`)
- **Dependency**: Added `secrecy` crate dependency

### Mitigation
- Provide migration guide
- Support legacy functions during transition
- Comprehensive examples and documentation

## Implementation
```rust
// Before (v1)
let provider = OpenAiProvider::from_legacy_config(config)?;

// After (v2)
let config = ProviderConfig {
    api_key: Some(SecretString::new(api_key)),
    base_url: Some("https://api.openai.com".to_string()),
    timeout_seconds: Some(120),
    extra_headers: None,
};
let provider = OpenAiProvider::new(config);
```

## Alternatives Considered
1. **Keep v1 API**: Rejected due to technical debt
2. **Generic builder pattern**: Rejected as too complex
3. **Dynamic configuration**: Rejected for lack of type safety

## References
- [SecretString ADR](./002-secretstring-integration.md)

## Date
2024-03-14

## Author
rustycode-llm ensemble
