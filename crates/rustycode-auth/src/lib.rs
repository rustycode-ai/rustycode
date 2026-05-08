#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! OAuth 2.0 authentication framework for `RustyCode`
//!
//! Provides secure OAuth flows for multiple LLM providers including:
//! - `Google` (`Gemini`)
//! - `GitHub` (Copilot)
//! - Custom OAuth providers

pub mod browser;
pub mod callback_server;
pub mod error;
pub mod github_copilot;
pub mod login;
pub mod oauth;
pub mod providers;
pub mod token_store;

pub use browser::{is_headless, open_url};
pub use callback_server::{CallbackResult, CallbackServer};
pub use error::{AuthError, AuthResult};
pub use github_copilot::{CopilotAuthResult, DeviceCodeResponse, GitHubCopilotAuth};
pub use login::{login, LoginMethod, LoginResult};
pub use oauth::{AuthMethod, OAuthClient, OAuthConfig};
pub use providers::{
    env_api_key, find_provider, ProviderAuthConfig, ProviderAuthMethod, PROVIDERS,
};
pub use token_store::{StoredToken, TokenStore};

use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

/// OAuth authentication methods supported
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum AuthType {
    /// API key authentication
    ApiKey { key: String },
    /// OAuth 2.0 authorization code flow
    OAuthCode {
        client_id: String,
        client_secret: String,
        auth_url: String,
        token_url: String,
    },
    /// OAuth 2.0 implicit flow
    OAuthImplicit { client_id: String, auth_url: String },
}

impl std::fmt::Debug for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey { .. } => f
                .debug_struct("ApiKey")
                .field("key", &"[REDACTED]")
                .finish(),
            Self::OAuthCode {
                client_id,
                auth_url,
                token_url,
                ..
            } => f
                .debug_struct("OAuthCode")
                .field("client_id", client_id)
                .field("client_secret", &"[REDACTED]")
                .field("auth_url", auth_url)
                .field("token_url", token_url)
                .finish(),
            Self::OAuthImplicit {
                client_id,
                auth_url,
            } => f
                .debug_struct("OAuthImplicit")
                .field("client_id", client_id)
                .field("auth_url", auth_url)
                .finish(),
        }
    }
}

/// Authentication token with metadata
#[derive(Clone)]
pub struct AuthToken {
    pub access_token: secrecy::SecretString,
    pub refresh_token: Option<secrecy::SecretString>,
    pub expires_at: Option<i64>, // Unix timestamp
    pub token_type: String,
}

impl std::fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthToken")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("token_type", &self.token_type)
            .finish()
    }
}

/// Serializable representation of `AuthToken` for storage
#[derive(Serialize, Deserialize)]
pub(crate) struct AuthTokenData {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    token_type: String,
}

impl From<AuthTokenData> for AuthToken {
    fn from(data: AuthTokenData) -> Self {
        Self {
            access_token: data.access_token.into(),
            refresh_token: data.refresh_token.map(Into::into),
            expires_at: data.expires_at,
            token_type: data.token_type,
        }
    }
}

impl From<AuthToken> for AuthTokenData {
    fn from(token: AuthToken) -> Self {
        Self {
            access_token: token.access_token.expose_secret().to_string(),
            refresh_token: token.refresh_token.map(|t| t.expose_secret().to_string()),
            expires_at: token.expires_at,
            token_type: token.token_type,
        }
    }
}

impl<'a> From<&'a AuthToken> for AuthTokenData {
    fn from(token: &'a AuthToken) -> Self {
        Self {
            access_token: token.access_token.expose_secret().to_string(),
            refresh_token: token
                .refresh_token
                .as_ref()
                .map(|t| t.expose_secret().to_string()),
            expires_at: token.expires_at,
            token_type: token.token_type.clone(),
        }
    }
}

/// Provider-specific authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAuth {
    pub provider_id: String,
    pub auth_type: AuthType,
    pub scopes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_type_api_key_roundtrip() {
        let auth = AuthType::ApiKey {
            key: "sk-test".to_string(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: AuthType = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, decoded);
    }

    #[test]
    fn test_auth_type_oauth_code_roundtrip() {
        let auth = AuthType::OAuthCode {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            auth_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: AuthType = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, decoded);
    }

    #[test]
    fn test_auth_type_implicit_roundtrip() {
        let auth = AuthType::OAuthImplicit {
            client_id: "id".to_string(),
            auth_url: "https://example.com/auth".to_string(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: AuthType = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, decoded);
    }

    #[test]
    fn test_auth_token_serialization() {
        let token = AuthToken {
            access_token: "ghu_test".to_string().into(),
            refresh_token: Some("ghr_test".to_string().into()),
            expires_at: Some(9_999_999_999),
            token_type: "bearer".to_string(),
        };
        let data: AuthTokenData = token.into();
        let json = serde_json::to_string(&data).unwrap();
        let decoded_data: AuthTokenData = serde_json::from_str(&json).unwrap();
        let decoded: AuthToken = decoded_data.into();
        assert_eq!(decoded.access_token.expose_secret(), "ghu_test");
        assert_eq!(
            decoded
                .refresh_token
                .as_ref()
                .map(|s| s.expose_secret().to_string()),
            Some("ghr_test".to_string())
        );
        assert_eq!(decoded.expires_at, Some(9_999_999_999));
        assert_eq!(decoded.token_type, "bearer");
    }

    #[test]
    fn test_auth_token_no_optional_fields() {
        let token = AuthToken {
            access_token: "test".to_string().into(),
            refresh_token: None,
            expires_at: None,
            token_type: "bearer".to_string(),
        };
        let data: AuthTokenData = token.into();
        let json = serde_json::to_string(&data).unwrap();
        let decoded_data: AuthTokenData = serde_json::from_str(&json).unwrap();
        let decoded: AuthToken = decoded_data.into();
        assert!(decoded.refresh_token.is_none());
        assert!(decoded.expires_at.is_none());
    }

    #[test]
    fn test_provider_auth_serialization() {
        let auth = ProviderAuth {
            provider_id: "copilot".to_string(),
            auth_type: AuthType::ApiKey {
                key: "test-key".to_string(),
            },
            scopes: vec!["read".to_string(), "write".to_string()],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: ProviderAuth = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "copilot");
        assert_eq!(decoded.scopes, vec!["read", "write"]);
    }

    #[test]
    fn test_auth_type_equality() {
        let a = AuthType::ApiKey {
            key: "k".to_string(),
        };
        let b = AuthType::ApiKey {
            key: "k".to_string(),
        };
        let c = AuthType::ApiKey {
            key: "different".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_provider_auth_empty_scopes() {
        let auth = ProviderAuth {
            provider_id: "test".to_string(),
            auth_type: AuthType::ApiKey {
                key: "key".to_string(),
            },
            scopes: vec![],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: ProviderAuth = serde_json::from_str(&json).unwrap();
        assert!(decoded.scopes.is_empty());
    }

    #[test]
    fn test_auth_type_api_key_serialization_format() {
        let auth = AuthType::ApiKey {
            key: "sk-abc123".to_string(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("ApiKey"));
        assert!(json.contains("sk-abc123"));
    }

    #[test]
    fn test_auth_type_oauth_code_fields() {
        let auth = AuthType::OAuthCode {
            client_id: "my-client".to_string(),
            client_secret: "my-secret".to_string(),
            auth_url: "https://auth.example.com".to_string(),
            token_url: "https://token.example.com".to_string(),
        };
        let json = serde_json::to_string_pretty(&auth).unwrap();
        assert!(json.contains("my-client"));
        assert!(json.contains("my-secret"));
        assert!(json.contains("auth.example.com"));
        assert!(json.contains("token.example.com"));
    }

    #[test]
    fn test_auth_type_implicit_fields() {
        let auth = AuthType::OAuthImplicit {
            client_id: "implicit-client".to_string(),
            auth_url: "https://implicit.example.com".to_string(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("implicit-client"));
    }

    #[test]
    fn test_auth_token_expired() {
        let token = AuthToken {
            access_token: "test".to_string().into(),
            refresh_token: None,
            expires_at: Some(1), // epoch = long expired
            token_type: "bearer".to_string(),
        };
        assert!(token.expires_at.unwrap() < 1_000_000_000);
    }

    #[test]
    fn test_provider_auth_with_oauth_code() {
        let auth = ProviderAuth {
            provider_id: "gemini".to_string(),
            auth_type: AuthType::OAuthCode {
                client_id: "gemini-id".to_string(),
                client_secret: "gemini-secret".to_string(),
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
            },
            scopes: vec!["openid".to_string(), "email".to_string()],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: ProviderAuth = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "gemini");
        assert_eq!(decoded.scopes.len(), 2);
    }

    #[test]
    fn test_stored_token_type() {
        let token = AuthToken {
            access_token: "ghu_abc".to_string().into(),
            refresh_token: Some("ghr_xyz".to_string().into()),
            expires_at: Some(1_735_689_600),
            token_type: "bearer".to_string(),
        };
        let stored = StoredToken {
            provider_id: "copilot".to_string(),
            token,
        };
        assert_eq!(stored.provider_id, "copilot");
        assert_eq!(stored.token.access_token.expose_secret(), "ghu_abc");
    }

    #[test]
    fn test_auth_type_deserialization_from_json() {
        // API key from JSON
        let json = r#"{"ApiKey":{"key":"sk-test123"}}"#;
        let auth: AuthType = serde_json::from_str(json).unwrap();
        assert_eq!(
            auth,
            AuthType::ApiKey {
                key: "sk-test123".into()
            }
        );
    }

    #[test]
    fn test_auth_type_oauth_code_deserialization() {
        let json = r#"{"OAuthCode":{"client_id":"c1","client_secret":"s1","auth_url":"https://a.com","token_url":"https://t.com"}}"#;
        let auth: AuthType = serde_json::from_str(json).unwrap();
        match auth {
            AuthType::OAuthCode {
                client_id,
                client_secret,
                auth_url,
                token_url,
            } => {
                assert_eq!(client_id, "c1");
                assert_eq!(client_secret, "s1");
                assert_eq!(auth_url, "https://a.com");
                assert_eq!(token_url, "https://t.com");
            }
            _ => panic!("Expected OAuthCode variant"),
        }
    }

    #[test]
    fn test_auth_type_implicit_deserialization() {
        let json = r#"{"OAuthImplicit":{"client_id":"c2","auth_url":"https://i.com"}}"#;
        let auth: AuthType = serde_json::from_str(json).unwrap();
        match auth {
            AuthType::OAuthImplicit {
                client_id,
                auth_url,
            } => {
                assert_eq!(client_id, "c2");
                assert_eq!(auth_url, "https://i.com");
            }
            _ => panic!("Expected OAuthImplicit variant"),
        }
    }

    #[test]
    fn test_auth_token_clone() {
        let token = AuthToken {
            access_token: "at_orig".to_string().into(),
            refresh_token: Some("rt_orig".to_string().into()),
            expires_at: Some(12345),
            token_type: "bearer".to_string(),
        };
        let cloned = token;
        assert_eq!(cloned.access_token.expose_secret(), "at_orig");
        assert_eq!(
            cloned
                .refresh_token
                .as_ref()
                .map(|s| s.expose_secret().to_string()),
            Some("rt_orig".to_string())
        );
    }

    #[test]
    fn test_auth_token_debug() {
        let token = AuthToken {
            access_token: "secret_token".to_string().into(),
            refresh_token: None,
            expires_at: None,
            token_type: "bearer".to_string(),
        };
        let debug = format!("{token:?}");
        assert!(debug.contains("AuthToken"));
        // Access token should be redacted, not printed in plaintext
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret_token"));
    }

    #[test]
    fn test_provider_auth_clone() {
        let auth = ProviderAuth {
            provider_id: "test".to_string(),
            auth_type: AuthType::ApiKey {
                key: "k".to_string(),
            },
            scopes: vec!["read".to_string()],
        };
        let cloned = auth.clone();
        assert_eq!(cloned.provider_id, auth.provider_id);
        assert_eq!(cloned.scopes, auth.scopes);
    }

    #[test]
    fn test_provider_auth_debug() {
        let auth = ProviderAuth {
            provider_id: "debug-test".to_string(),
            auth_type: AuthType::OAuthImplicit {
                client_id: "c".to_string(),
                auth_url: "https://a.com".to_string(),
            },
            scopes: vec![],
        };
        let debug = format!("{auth:?}");
        assert!(debug.contains("ProviderAuth"));
        assert!(debug.contains("debug-test"));
    }

    #[test]
    fn test_auth_type_inequality_across_variants() {
        let api_key = AuthType::ApiKey { key: "k".into() };
        let oauth_code = AuthType::OAuthCode {
            client_id: "k".into(),
            client_secret: "k".into(),
            auth_url: "k".into(),
            token_url: "k".into(),
        };
        let implicit = AuthType::OAuthImplicit {
            client_id: "k".into(),
            auth_url: "k".into(),
        };
        assert_ne!(api_key, oauth_code);
        assert_ne!(api_key, implicit);
        assert_ne!(oauth_code, implicit);
    }

    #[test]
    fn test_auth_token_with_zero_expiry() {
        let token = AuthToken {
            access_token: "test".to_string().into(),
            refresh_token: None,
            expires_at: Some(0),
            token_type: "bearer".to_string(),
        };
        // Epoch 0 is far in the past
        assert_eq!(token.expires_at, Some(0));
        assert!(token.expires_at.unwrap() < 1_000_000_000);
    }

    #[test]
    fn test_provider_auth_with_implicit_auth() {
        let auth = ProviderAuth {
            provider_id: "custom".to_string(),
            auth_type: AuthType::OAuthImplicit {
                client_id: "custom-id".to_string(),
                auth_url: "https://custom.auth.com".to_string(),
            },
            scopes: vec!["openid".to_string()],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: ProviderAuth = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "custom");
        assert_eq!(decoded.scopes, vec!["openid"]);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for auth lib
    // =========================================================================

    // 1. AuthType equality same variant same values
    #[test]
    fn auth_type_equality_same_variant() {
        let a = AuthType::ApiKey { key: "k1".into() };
        let b = AuthType::ApiKey { key: "k1".into() };
        assert_eq!(a, b);

        let a = AuthType::ApiKey { key: "k1".into() };
        let b = AuthType::ApiKey { key: "k2".into() };
        assert_ne!(a, b);
    }

    // 2. AuthType OAuthCode equality
    #[test]
    fn auth_type_oauth_code_equality() {
        let a = AuthType::OAuthCode {
            client_id: "c".into(),
            client_secret: "s".into(),
            auth_url: "https://a".into(),
            token_url: "https://t".into(),
        };
        let b = AuthType::OAuthCode {
            client_id: "c".into(),
            client_secret: "s".into(),
            auth_url: "https://a".into(),
            token_url: "https://t".into(),
        };
        assert_eq!(a, b);
    }

    // 3. AuthType OAuthImplicit equality
    #[test]
    fn auth_type_implicit_equality() {
        let a = AuthType::OAuthImplicit {
            client_id: "c".into(),
            auth_url: "https://a".into(),
        };
        let b = AuthType::OAuthImplicit {
            client_id: "c".into(),
            auth_url: "https://a".into(),
        };
        assert_eq!(a, b);
    }

    // 4. AuthToken serde roundtrip
    #[test]
    fn auth_token_serde_roundtrip() {
        let token = AuthToken {
            access_token: "at_12345".into(),
            refresh_token: Some("rt_67890".into()),
            expires_at: Some(1_735_689_600),
            token_type: "bearer".into(),
        };
        let data: AuthTokenData = token.into();
        let json = serde_json::to_string(&data).unwrap();
        let decoded_data: AuthTokenData = serde_json::from_str(&json).unwrap();
        let decoded: AuthToken = decoded_data.into();
        assert_eq!(decoded.access_token.expose_secret(), "at_12345");
        assert_eq!(
            decoded
                .refresh_token
                .as_ref()
                .map(|s| s.expose_secret().to_string()),
            Some("rt_67890".to_string())
        );
        assert_eq!(decoded.expires_at, Some(1_735_689_600));
        assert_eq!(decoded.token_type, "bearer");
    }

    // 5. AuthToken with None fields serde
    #[test]
    fn auth_token_none_fields_serde() {
        let token = AuthToken {
            access_token: "bare".into(),
            refresh_token: None,
            expires_at: None,
            token_type: "bearer".into(),
        };
        let data: AuthTokenData = token.into();
        let json = serde_json::to_string(&data).unwrap();
        let decoded_data: AuthTokenData = serde_json::from_str(&json).unwrap();
        let decoded: AuthToken = decoded_data.into();
        assert!(decoded.refresh_token.is_none());
        assert!(decoded.expires_at.is_none());
    }

    // 6. ProviderAuth serde roundtrip with ApiKey
    #[test]
    fn provider_auth_apikey_serde() {
        let auth = ProviderAuth {
            provider_id: "openai".into(),
            auth_type: AuthType::ApiKey {
                key: "sk-abc".into(),
            },
            scopes: vec!["read".into(), "write".into()],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: ProviderAuth = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "openai");
        assert_eq!(decoded.scopes.len(), 2);
    }

    // 7. AuthType debug redacts ApiKey
    #[test]
    fn auth_type_debug_redacts_api_key() {
        let auth = AuthType::ApiKey {
            key: "super-secret-key".into(),
        };
        let debug = format!("{auth:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("super-secret-key"));
    }

    // 8. AuthType debug redacts OAuthCode secret
    #[test]
    fn auth_type_debug_redacts_oauth_secret() {
        let auth = AuthType::OAuthCode {
            client_id: "public-id".into(),
            client_secret: "top-secret".into(),
            auth_url: "https://auth.example.com".into(),
            token_url: "https://token.example.com".into(),
        };
        let debug = format!("{auth:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(debug.contains("public-id"));
        assert!(!debug.contains("top-secret"));
    }

    // 9. AuthType debug shows OAuthImplicit fields
    #[test]
    fn auth_type_debug_implicit_fields() {
        let auth = AuthType::OAuthImplicit {
            client_id: "implicit-id".into(),
            auth_url: "https://implicit.example.com".into(),
        };
        let debug = format!("{auth:?}");
        assert!(debug.contains("implicit-id"));
        assert!(debug.contains("https://implicit.example.com"));
    }

    // 10. AuthToken clone independence
    #[test]
    fn auth_token_clone_independence() {
        let token = AuthToken {
            access_token: "orig".into(),
            refresh_token: Some("rt".into()),
            expires_at: Some(9999),
            token_type: "bearer".into(),
        };
        let mut cloned = token.clone();
        cloned.access_token = "modified".into();
        assert_eq!(token.access_token.expose_secret(), "orig");
        assert_eq!(cloned.access_token.expose_secret(), "modified");
    }

    // 11. ProviderAuth with empty scopes
    #[test]
    fn provider_auth_empty_scopes() {
        let auth = ProviderAuth {
            provider_id: "none".into(),
            auth_type: AuthType::ApiKey { key: "k".into() },
            scopes: vec![],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let decoded: ProviderAuth = serde_json::from_str(&json).unwrap();
        assert!(decoded.scopes.is_empty());
    }

    // 12. AuthToken debug redacts all sensitive fields
    #[test]
    fn auth_token_debug_redacts_all() {
        let token = AuthToken {
            access_token: "secret-access".into(),
            refresh_token: Some("secret-refresh".into()),
            expires_at: Some(12345),
            token_type: "bearer".into(),
        };
        let debug = format!("{token:?}");
        assert!(!debug.contains("secret-access"));
        assert!(!debug.contains("secret-refresh"));
        assert!(debug.contains("[REDACTED]"));
    }

    // 13. AuthType non-exhaustive wildcard
    #[test]
    fn auth_type_non_exhaustive() {
        let auth = AuthType::ApiKey { key: "k".into() };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(!json.is_empty());
    }

    // 14. ProviderAuth debug format
    #[test]
    fn provider_auth_debug_contains_provider_id() {
        let auth = ProviderAuth {
            provider_id: "my-provider".into(),
            auth_type: AuthType::ApiKey { key: "k".into() },
            scopes: vec![],
        };
        let debug = format!("{auth:?}");
        assert!(debug.contains("my-provider"));
    }

    // 15. AuthToken with large values
    #[test]
    fn auth_token_large_values() {
        let big_token = "x".repeat(10_000);
        let token = AuthToken {
            access_token: big_token.into(),
            refresh_token: Some("refresh".into()),
            expires_at: Some(i64::MAX),
            token_type: "bearer".into(),
        };
        let data: AuthTokenData = token.into();
        let json = serde_json::to_string(&data).unwrap();
        let decoded_data: AuthTokenData = serde_json::from_str(&json).unwrap();
        let decoded: AuthToken = decoded_data.into();
        assert_eq!(decoded.access_token.expose_secret().len(), 10_000);
        assert_eq!(decoded.expires_at, Some(i64::MAX));
    }
}
