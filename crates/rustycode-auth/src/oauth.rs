//! OAuth 2.0 client implementation

use crate::{AuthResult, AuthToken};
use async_trait::async_trait;
use rand::RngExt;
use reqwest::Client;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// OAuth authentication methods
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AuthMethod {
    /// User visits URL, gets code, pastes it back
    Code {
        url: String,
        verifier: String,
        /// CSRF protection state — must be validated against the callback's `state` parameter
        state: String,
    },
    /// Automatic redirect
    Auto { url: String },
}

/// OAuth configuration
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_url: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    token_type: String,
}

/// OAuth client trait for extensible provider support
#[async_trait]
pub trait OAuthClient: Send + Sync {
    async fn authorize(&self, config: &OAuthConfig) -> AuthResult<AuthMethod>;
    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        _verifier: &str,
    ) -> AuthResult<AuthToken>;
    async fn refresh_token(
        &self,
        config: &OAuthConfig,
        refresh_token: &str,
    ) -> AuthResult<AuthToken>;
}

/// Compute absolute expiry timestamp from a relative `expires_in` seconds value.
fn compute_expires_at(expires_in: Option<u64>) -> Option<i64> {
    let secs = expires_in?;
    #[allow(clippy::cast_possible_wrap)]
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs() as i64;
    #[allow(clippy::cast_possible_wrap)]
    let offset = secs as i64;
    Some(now_secs.saturating_add(offset))
}

/// Default OAuth 2.0 implementation
pub struct DefaultOAuthClient {
    http_client: Client,
}

impl DefaultOAuthClient {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }
}

#[async_trait]
impl OAuthClient for DefaultOAuthClient {
    async fn authorize(&self, config: &OAuthConfig) -> AuthResult<AuthMethod> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use sha2::{Digest, Sha256};

        // Generate PKCE verifier and challenge
        let mut rng = rand::rng();
        let verifier: String = (0..128)
            .map(|_| {
                const CHARSET: &[u8] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
                CHARSET[rng.random_range(0..CHARSET.len())] as char
            })
            .collect();

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

        // Build authorization URL with state parameter for CSRF protection
        let scope = config.scopes.join(" ");
        let mut rng = rand::rng();
        let state: String = (0..32)
            .map(|_| {
                let charset = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                charset[rng.random_range(0..charset.len())] as char
            })
            .collect();
        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
            config.auth_url,
            urlencoding::encode(&config.client_id),
            urlencoding::encode(&config.redirect_url),
            urlencoding::encode(&scope),
            challenge,
            urlencoding::encode(&state)
        );

        Ok(AuthMethod::Code {
            url,
            verifier,
            state,
        })
    }

    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        verifier: &str,
    ) -> AuthResult<AuthToken> {
        let client = &self.http_client;

        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &config.redirect_url),
            ("client_id", &config.client_id),
            ("code_verifier", verifier),
        ];

        if let Some(secret) = &config.client_secret {
            params.push(("client_secret", secret));
        }

        let response = client.post(&config.token_url).form(&params).send().await?;

        let token_resp: TokenResponse = response.json().await?;

        Ok(AuthToken {
            access_token: token_resp.access_token.into(),
            refresh_token: token_resp.refresh_token.map(Into::into),
            expires_at: compute_expires_at(token_resp.expires_in),
            token_type: token_resp.token_type,
        })
    }

    async fn refresh_token(
        &self,
        config: &OAuthConfig,
        refresh_token: &str,
    ) -> AuthResult<AuthToken> {
        let client = &self.http_client;

        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &config.client_id),
        ];

        if let Some(secret) = &config.client_secret {
            params.push(("client_secret", secret));
        }

        let response = client.post(&config.token_url).form(&params).send().await?;

        let token_resp: TokenResponse = response.json().await?;

        Ok(AuthToken {
            access_token: token_resp.access_token.into(),
            refresh_token: token_resp.refresh_token.map(Into::into),
            expires_at: compute_expires_at(token_resp.expires_in),
            token_type: token_resp.token_type,
        })
    }
}

impl Default for DefaultOAuthClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_config_construction() {
        let config = OAuthConfig {
            client_id: "test-id".to_string(),
            client_secret: Some("test-secret".to_string()),
            auth_url: "https://auth.example.com".to_string(),
            token_url: "https://token.example.com".to_string(),
            redirect_url: "http://localhost:8080/callback".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
        };
        assert_eq!(config.client_id, "test-id");
        assert!(config.client_secret.is_some());
        assert_eq!(config.scopes.len(), 2);
    }

    #[test]
    fn test_oauth_config_no_secret() {
        let config = OAuthConfig {
            client_id: "public-id".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com".to_string(),
            token_url: "https://token.example.com".to_string(),
            redirect_url: "http://localhost:8080/callback".to_string(),
            scopes: vec![],
        };
        assert!(config.client_secret.is_none());
        assert!(config.scopes.is_empty());
    }

    #[test]
    fn test_default_oauth_client_new() {
        let _client = DefaultOAuthClient::new();
    }

    #[test]
    fn test_default_oauth_client_default() {
        let _client = DefaultOAuthClient::default();
    }

    #[test]
    fn test_auth_method_code_debug() {
        let method = AuthMethod::Code {
            url: "https://example.com".to_string(),
            verifier: "abc123".to_string(),
            state: "state123".to_string(),
        };
        let debug_str = format!("{method:?}");
        assert!(debug_str.contains("Code"));
    }

    #[test]
    fn test_auth_method_auto_debug() {
        let method = AuthMethod::Auto {
            url: "https://example.com".to_string(),
        };
        let debug_str = format!("{method:?}");
        assert!(debug_str.contains("Auto"));
    }

    #[test]
    fn test_oauth_config_all_fields() {
        let config = OAuthConfig {
            client_id: "my-app".to_string(),
            client_secret: Some("shh".to_string()),
            auth_url: "https://provider.com/authorize".to_string(),
            token_url: "https://provider.com/token".to_string(),
            redirect_url: "https://localhost:4321/cb".to_string(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        };
        assert_eq!(config.client_id, "my-app");
        assert_eq!(config.client_secret.as_deref(), Some("shh"));
        assert!(config.auth_url.starts_with("https://"));
        assert!(config.token_url.starts_with("https://"));
        assert_eq!(config.scopes.len(), 3);
    }

    #[test]
    fn test_oauth_config_debug_format() {
        let config = OAuthConfig {
            client_id: "debug-test".to_string(),
            client_secret: None,
            auth_url: "https://a.com".to_string(),
            token_url: "https://t.com".to_string(),
            redirect_url: "https://r.com".to_string(),
            scopes: vec![],
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("OAuthConfig"));
        assert!(debug.contains("debug-test"));
    }

    #[test]
    fn test_oauth_config_clone() {
        let config = OAuthConfig {
            client_id: "original".to_string(),
            client_secret: Some("secret".to_string()),
            auth_url: "https://a.com".to_string(),
            token_url: "https://t.com".to_string(),
            redirect_url: "https://r.com".to_string(),
            scopes: vec!["read".to_string()],
        };
        let cloned = config.clone();
        assert_eq!(cloned.client_id, "original");
        assert_eq!(cloned.scopes, config.scopes);
    }

    #[test]
    fn test_auth_method_code_fields() {
        let method = AuthMethod::Code {
            url: "https://auth.example.com?code=abc".to_string(),
            verifier: "my-pkce-verifier-123".to_string(),
            state: "state-value-xyz".to_string(),
        };
        if let AuthMethod::Code {
            url,
            verifier,
            state,
        } = &method
        {
            assert_eq!(url, "https://auth.example.com?code=abc");
            assert_eq!(verifier, "my-pkce-verifier-123");
            assert!(!state.is_empty());
        } else {
            panic!("Expected Code variant");
        }
    }

    #[test]
    fn test_auth_method_auto_fields() {
        let method = AuthMethod::Auto {
            url: "https://auto.example.com".to_string(),
        };
        if let AuthMethod::Auto { url } = &method {
            assert_eq!(url, "https://auto.example.com");
        } else {
            panic!("Expected Auto variant");
        }
    }

    #[test]
    fn test_default_oauth_client_authorize_generates_pkce_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = DefaultOAuthClient::new();
        let config = OAuthConfig {
            client_id: "test-client".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            redirect_url: "http://localhost:8080/callback".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
        };

        let result = rt.block_on(client.authorize(&config)).unwrap();

        match result {
            AuthMethod::Code {
                url,
                verifier,
                state,
            } => {
                // URL should contain PKCE params
                assert!(url.contains("code_challenge="));
                assert!(url.contains("code_challenge_method=S256"));
                assert!(url.contains("client_id=test-client"));
                assert!(url.contains("response_type=code"));
                assert!(url.contains("scope="));
                // URL should contain the state parameter for CSRF protection
                assert!(url.contains("state="));
                assert!(!state.is_empty(), "state must not be empty");
                // Verifier should be a reasonable length
                assert!(
                    verifier.len() >= 43,
                    "PKCE verifier too short: {}",
                    verifier.len()
                );
                assert!(verifier.len() <= 128);
            }
            AuthMethod::Auto { .. } => {
                panic!("Expected Code variant from authorize");
            }
        }
    }

    #[test]
    fn test_default_oauth_client_authorize_empty_scopes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = DefaultOAuthClient::new();
        let config = OAuthConfig {
            client_id: "empty-scope".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            redirect_url: "http://localhost:8080/callback".to_string(),
            scopes: vec![],
        };

        let result = rt.block_on(client.authorize(&config)).unwrap();

        if let AuthMethod::Code { url, .. } = result {
            // Should handle empty scopes gracefully
            assert!(url.contains("scope="));
        } else {
            panic!("Expected Code variant");
        }
    }

    #[test]
    fn test_default_oauth_client_authorize_url_encodes_params() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = DefaultOAuthClient::new();
        let config = OAuthConfig {
            client_id: "client with spaces".to_string(),
            client_secret: None,
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            redirect_url: "http://localhost:8080/cb?extra=param".to_string(),
            scopes: vec!["read write".to_string()],
        };

        let result = rt.block_on(client.authorize(&config)).unwrap();

        if let AuthMethod::Code { url, .. } = result {
            // Spaces should be encoded
            assert!(!url.contains("client with spaces"));
            assert!(url.contains("client%20with%20spaces") || url.contains('+'));
        } else {
            panic!("Expected Code variant");
        }
    }
}
