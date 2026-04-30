# Coding Standards: Anti-Pattern Prevention

## Overview

This document outlines coding standards to prevent common anti-patterns in the rustycode codebase, based on lessons learned during the anti-pattern elimination initiative (2026-03-14).

## Core Principles

1. **No panics in production code** - All errors must be handled explicitly
2. **Type safety over string typing** - Use enums and strong types
3. **Immutable patterns** - Never mutate shared state without synchronization
4. **Explicit over implicit** - Make error handling and validation visible

## Anti-Patterns to Avoid

### 1. unwrap() and expect() in Production Code

**❌ WRONG:**
```rust
let client = reqwest::Client::builder()
    .build()
    .expect("failed to build HTTP client");
```

**✅ RIGHT:**
```rust
let client = reqwest::Client::builder()
    .build()
    .map_err(|e| ProviderError::Configuration(format!("failed to build HTTP client: {}", e)))?;
```

**When to use unwrap():**
- ✅ Test code (tests are allowed to panic)
- ✅ Static strings that can never fail at runtime
  ```rust
  "application/json".parse().unwrap()  // OK - compile-time constant
  "2023-06-01".parse().unwrap()       // OK - compile-time constant
  ```
- ❌ User input, external data, network operations

**When to use expect():**
- ✅ Test assertions
- ❌ Production code (prefer Result with context)

### 2. String-Typed Enums

**❌ WRONG:**
```rust
pub struct ChatMessage {
    pub role: String,  // Could be "user", "User", "usr", typo, etc.
    pub content: String,
}
```

**✅ RIGHT:**
```rust
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool(String),
}

pub struct ChatMessage {
    pub role: MessageRole,  // Type-safe, compiler-checked
    pub content: String,
}
```

### 3. Missing Validation

**❌ WRONG:**
```rust
pub fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
    // No validation - sends invalid requests to API
    self.send_request(request).await
}
```

**✅ RIGHT:**
```rust
pub fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
    request.validate()?;  // Fail fast with clear error
    self.send_request(request).await
}

impl CompletionRequest {
    pub fn validate(&self) -> Result<(), RequestValidationError> {
        if self.model.is_empty() {
            return Err(RequestValidationError::EmptyModel);
        }
        if self.messages.is_empty() {
            return Err(RequestValidationError::NoMessages);
        }
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(RequestValidationError::InvalidTemperature(temp));
            }
        }
        Ok(())
    }
}
```

### 4. Secret Leakage in Error Messages

**❌ WRONG:**
```rust
api_key
    .parse()
    .map_err(|e| ProviderError::Configuration(format!("invalid API key format: {}", e)))?
```

**✅ RIGHT:**
```rust
api_key
    .parse()
    .map_err(|_| ProviderError::Configuration("invalid API key format".to_string()))?
```

**Rule:** Never include secret values in error messages, logs, or debug output.

### 5. Header Injection

**❌ WRONG:**
```rust
for (key, value) in extra_headers {
    if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
        headers.insert(header_name, value.parse().unwrap());  // User-controlled!
    }
}
```

**✅ RIGHT:**
```rust
const ALLOWED_HEADERS: &[&str] = &["X-Custom-Header", "X-Request-ID"];

for (key, value) in extra_headers {
    // Whitelist approach
    if !ALLOWED_HEADERS.contains(&key.as_str()) {
        continue;
    }

    // Block override of security-critical headers
    match key.as_str() {
        "authorization" | "host" | "content-type" => continue,
        _ => {}
    }

    // Validate for CRLF injection
    if value.contains('\r') || value.contains('\n') {
        return Err(ProviderError::Configuration(format!(
            "header value for '{}' contains invalid characters", key
        )));
    }

    if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
        let header_value = value.parse().map_err(|e| {
            ProviderError::Configuration(format!("invalid header value '{}': {}", value, e))
        })?;
        headers.insert(header_name, header_value);
    }
}
```

### 6. Inconsistent Constructor Signatures

**❌ WRONG:**
```rust
// Different providers have different signatures!
OpenAiProvider::new(config)           // Returns Self
AnthropicProvider::new(config)?       // Returns Result
OllamaProvider::new(config, model)    // Different parameters
```

**✅ RIGHT:**
```rust
// All providers follow the same pattern
pub fn new(config: ProviderConfig, model: String) -> Result<Self, ProviderError> {
    // ...
}
```

**Standard Pattern:**
- Always return `Result<Self, ProviderError>`
- Accept `ProviderConfig` as first parameter
- Accept model as second parameter (even if optional internally)
- Provide `from_env()` convenience constructor

## Error Handling Patterns

### Constructor Pattern

```rust
impl MyProvider {
    pub fn new(config: ProviderConfig, model: String) -> Result<Self, ProviderError> {
        // Build HTTP client
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.timeout_seconds.unwrap_or(120)
            ))
            .build()
            .map_err(|e| ProviderError::Configuration(format!(
                "failed to build HTTP client: {}", e
            )))?;

        // Validate API key
        if config.api_key.as_ref().map(|k| k.expose_secret().is_empty()).unwrap_or(true) {
            return Err(ProviderError::Configuration(
                "API key required. Set api_key in config or env var".to_string()
            ));
        }

        Ok(Self { config, client, model })
    }
}
```

### Header Parsing Pattern

```rust
// Static strings - OK to unwrap
headers.insert(
    reqwest::header::CONTENT_TYPE,
    "application/json".parse().unwrap(),  // Safe - compile-time constant
);

// User data - must handle errors
for (key, value) in extra_headers {
    let header_value = value.parse().map_err(|e| {
        ProviderError::Configuration(format!("invalid header value '{}': {}", value, e))
    })?;
    headers.insert(header_name, header_value);
}
```

### SecretString Handling Pattern

```rust
use secrecy::{SecretString, ExposeSecret};

// Creating
let api_key = SecretString::new(api_key_string.into());

// Using (brief exposure)
let key_str = config.api_key
    .as_ref()
    .map(|k| k.expose_secret().to_string());

// NOT this
let key_str = config.api_key.as_ref().map(|k| k.expose_secret().clone());  // Wrong type!
```

## Performance Considerations

### String Allocations

**❌ WRONG:**
```rust
pub struct ChatMessage {
    pub role: String,  // Allocates for every message
    pub content: String,
}
```

**✅ BETTER:**
```rust
pub enum MessageRole {
    User,       // Zero-size
    Assistant,  // Zero-size
    System,     // Zero-size
}

pub struct ChatMessage {
    pub role: MessageRole,  // No allocation
    pub content: String,    // Necessary allocation
}
```

### Mutex Lock Duration

**❌ WRONG:**
```rust
let mut guard = self.inner.lock().unwrap();
// ... 100 lines of processing ...
drop(guard);
```

**✅ RIGHT:**
```rust
// Extract only what you need
let key = self.inner.lock().unwrap().key.clone();
drop(guard);  // Release lock immediately

// ... process data ...

// Re-acquire for write
let mut guard = self.inner.lock().unwrap();
guard.value = new_value;
drop(guard);
```

## Code Review Checklist

Before marking code as complete, verify:

- [ ] No `.unwrap()` in production code (except static strings)
- [ ] No `.expect()` in production code
- [ ] All constructors return `Result<Self, Error>`
- [ ] All user input is validated
- [ ] No secrets in error messages or logs
- [ ] Header injection protection in place
- [ ] Consistent with other provider implementations
- [ ] Error messages are actionable
- [ ] Secrets use `SecretString` type
- [ ] Mutex locks are brief and necessary

## Testing Guidelines

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construction_with_valid_config() {
        let config = ProviderConfig {
            api_key: Some(SecretString::new("test-key".to_string())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
        };

        let provider = MyProvider::new(config, "model".to_string()).unwrap();
        assert_eq!(provider.name(), "my-provider");
    }

    #[test]
    fn test_construction_fails_without_api_key() {
        let config = ProviderConfig {
            api_key: None,  // Missing API key
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
        };

        let result = MyProvider::new(config, "model".to_string());
        assert!(result.is_err());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_complete_with_invalid_request() {
    let provider = create_test_provider();

    let request = CompletionRequest {
        model: "".to_string(),  // Invalid
        messages: vec![],       // Invalid
        max_tokens: None,
        temperature: Some(3.0),  // Invalid range
        stream: false,
    };

    let result = provider.complete(request).await;
    assert!(result.is_err());
}
```

## Documentation Standards

### Module Documentation

```rust
//! # MyProvider
//!
//! This provider implements the MyProvider API for LLM completions.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from MY_API_KEY environment variable)
//! - Model name (e.g., "my-model-v1")
//!
//! ## Example
//!
//! ```rust
//! use rustycode_llm::{MyProvider, ProviderConfig};
//! use secrecy::SecretString;
//!
//! let config = ProviderConfig {
//!     api_key: std::env::var("MY_API_KEY")
//!         .ok()
//!         .map(|k| SecretString::new(k.into())),
//!     base_url: Some("https://api.example.com".to_string()),
//!     timeout_seconds: Some(120),
//!     extra_headers: None,
//! };
//!
//! let provider = MyProvider::new(config, "my-model-v1".to_string())?;
//! # Ok::<(), rustycode_llm::ProviderError>(())
//! ```
//!
//! ## Rate Limits
//!
//! - 100 requests per minute
//! - 10,000 tokens per minute
//!
//! ## Errors
//!
//! - `ProviderError::Auth`: Invalid API key
//! - `ProviderError::RateLimited`: Rate limit exceeded
//! - `ProviderError::Network`: Connection failure
```

## Resources

- [SPARV Journal](/.sparv/journal.md) - Anti-pattern elimination progress
- [ADR: Provider v2 API](/docs/adr/001-provider-v2-api.md) - Design decisions
- [Provider Examples](/examples/) - Working code examples

## Version History

- **2026-03-14**: Initial version based on anti-pattern elimination findings
- Fixed 5 critical anti-patterns in production code
- Established coding standards for future development

---

**Remember**: Code reviews should check for these anti-patterns. When in doubt, prefer explicit error handling over convenience.
