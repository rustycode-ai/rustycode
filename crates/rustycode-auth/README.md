# rustycode-auth

OAuth 2.0 and API key authentication framework for RustyCode.

## Purpose

Manages authentication for LLM providers and services using OAuth 2.0, API keys, and specialized flows like GitHub Copilot device code authentication. Handles secure token storage, refresh, and rotation. All secrets are wrapped with `secrecy::SecretString` to prevent accidental logging.

## Key Types

- `AuthType` — Authentication method (ApiKey, OAuthCode, OAuthImplicit)
- `OAuthClient` — OAuth 2.0 client for authorization code flow
- `OAuthConfig` — OAuth configuration with URLs and credentials
- `GitHubCopilotAuth` — GitHub Copilot device code authentication
- `TokenStore` — Persistent secure token storage
- `StoredToken` — Token with expiry and metadata
- `AuthError`, `AuthResult` — Error handling

## Public API

```rust
use rustycode_auth::{OAuthConfig, OAuthClient, AuthType};

// API key authentication (simplest)
let auth = AuthType::ApiKey {
    key: "sk-...".to_string(),
};

// OAuth 2.0 authentication code flow
let oauth_config = OAuthConfig {
    client_id: "your-client-id".to_string(),
    client_secret: "your-client-secret".to_string(),
    auth_url: "https://accounts.google.com/o/oauth2/auth".to_string(),
    token_url: "https://oauth2.googleapis.com/token".to_string(),
};

let oauth_client = OAuthClient::new(oauth_config);

// Start authorization flow
let auth_url = oauth_client.authorization_url("state123")?;
println!("Visit: {}", auth_url);

// After user approves, exchange code for token
let token = oauth_client.exchange_code("auth-code-from-callback", "state123").await?;

// Store token securely
let token_store = TokenStore::new("/path/to/tokens")?;
token_store.save("google", token)?;
```

## Authentication Methods

- **API Key** — Direct key authentication (Anthropic, OpenAI, etc.)
- **OAuth Code Flow** — Server-side authorization (Google, Anthropic OAuth)
- **OAuth Implicit** — Browser-based authentication (legacy)
- **GitHub Copilot Device Code** — Special device code flow for Copilot

## Token Storage

Tokens are stored with encryption on disk:
- Encrypted at rest using OS keychain or file-based encryption
- Metadata includes expiry, refresh token, scopes
- Automatic refresh before expiry
- Per-provider token isolation

## Security Features

- All secrets wrapped with `secrecy::SecretString` (zero on drop)
- Never logs or displays raw secrets
- PKCE support for OAuth (if available)
- Secure random state generation for CSRF protection
- Token rotation on refresh

## Dependencies

- `secrecy` — Secret type wrappers
- `serde` — Serialization
- `tokio` — Async runtime
- `reqwest` — HTTP client for OAuth flows
- `anyhow` — Error handling

## Architecture Notes

Token storage supports multiple backends:
- OS keychain (macOS: Keychain, Linux: Secret Service, Windows: Credential Manager)
- Encrypted file storage (fallback)

OAuth flows use PKCE when available for additional security. State tokens are cryptographically random and time-limited.

## Testing

Tests use mock HTTP servers to verify OAuth flows without hitting real endpoints. Token storage tests verify encryption/decryption and expiry handling.

## See Also

- `rustycode-llm` — LLM provider implementations that use auth
- `rustycode-providers` — Provider registry with auth metadata
- `rustycode-core` — Session initialization (auth happens at startup)
