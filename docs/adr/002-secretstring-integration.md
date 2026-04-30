# ADR 002: SecretString Integration

## Status
**Accepted** - Implemented in rustycode-llm v0.1.0

## Context
LLM providers require API keys for authentication. Storing these as plain
`String` types creates security risks:
- Keys can be leaked in logs/debug output
- Keys remain in memory after use
- Accidental exposure in error messages
- No zeroization guarantees

## Decision
Integrate `secrecy` crate's `SecretString` type for all API key handling.

### Requirements
1. All API keys use `SecretString`
2. Keys are zeroized on drop
3. No accidental exposure in `Debug` output
4. Safe exposure via `expose_secret()` method
5. Compatible with HTTP client headers

### Implementation Pattern
```rust
use secrecy::{SecretString, ExposeSecret};

pub struct ProviderConfig {
    pub api_key: Option<SecretString>,
    // ...
}

impl Provider {
    fn get_api_key(&self) -> Result<String, ProviderError> {
        self.config.api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string())
            .or_else(|| std::env::var("API_KEY").ok())
            .ok_or_else(|| ProviderError::Configuration(...))
    }
}
```

### Key Design Decisions

1. **`Option<SecretString>` for optional keys**
   - Allows config without API keys
   - Environment variable fallback
   - Clean `None` handling

2. **`.expose_secret().to_string()` for conversion**
   - `expose_secret()` returns `&str`
   - `.to_string()` creates owned `String`
   - Explicit conversion shows intent
   - **Not** `.clone()` which returns `&str` (type error)

3. **`.into()` for `SecretString::new()`**
   - `SecretString::new()` expects `Box<str>`
   - `.into()` converts `String` → `Box<str>`
   - Cleaner than manual boxing

## Consequences

### Positive
- **Security**: Keys zeroized on drop
- **Debug safety**: No accidental key exposure
- **Type safety**: Compile-time guarantees
- **Standard**: Uses well-audited crate

### Negative
- **Conversion overhead**: String → SecretString → String
- **Learning curve**: Developers must understand `SecretString`
- **Dependency**: Added `secrecy` crate
- **Type complexity**: More complex than plain `String`

### Migration Impact
```rust
// Before
api_key: Option<String>

// After
api_key: Option<SecretString>

// Conversion
String → SecretString:  SecretString::new(s.into())
SecretString → String:  k.expose_secret().to_string()
```

## Security Benefits

1. **Zeroization**: Keys securely wiped from memory
2. **Debug protection**: `Debug` impl shows `[REDACTED]`
3. **No accidental logs**: Must explicitly `expose_secret()`
4. **Clear intent**: Secret handling is explicit

## Alternatives Considered

1. **Plain `String`**: Rejected (security risk)
2. **Custom `Secret<T>`**: Rejected (reinventing wheel)
3. **`zeroize` crate directly**: Rejected (less ergonomic)
4. **Compile-time encryption**: Rejected (over-engineering)

## References
- [secrecy crate](https://docs.rs/secrecy/)
- [Provider v2 ADR](./001-provider-v2-api.md)

## Date
2024-03-14

## Author
rustycode-llm ensemble
