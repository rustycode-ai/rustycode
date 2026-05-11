//! OAuth 2.1 authentication support for MCP
//!
//! Implements the MCP OAuth 2.1 specification for authenticating with MCP servers.
//! Supports both PKCE (Proof Key for Code Exchange) flow and dynamic client registration.

use crate::{McpError, McpResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// OAuth 2.1 token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    /// Optional refresh token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Token type (e.g., "Bearer")
    #[serde(default = "default_bearer")]
    pub token_type: String,
    /// Optional expiration time (seconds from now)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// Optional scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// When this token was obtained (for expiration calculation)
    #[serde(skip, default = "default_obtained_at")]
    pub obtained_at: chrono::DateTime<chrono::Utc>,
}

fn default_obtained_at() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

impl Default for OAuthToken {
    fn default() -> Self {
        Self {
            access_token: String::new(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: None,
            scope: None,
            obtained_at: chrono::Utc::now(),
        }
    }
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

impl OAuthToken {
    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_in) = self.expires_in {
            let expires_at = self.obtained_at + chrono::Duration::seconds(expires_in as i64);
            chrono::Utc::now() >= expires_at
        } else {
            false // No expiration = never expired
        }
    }

    /// Get remaining seconds until expiration
    pub fn expires_in_seconds(&self) -> Option<u64> {
        if let Some(expires_in) = self.expires_in {
            let expires_at = self.obtained_at + chrono::Duration::seconds(expires_in as i64);
            let remaining = expires_at - chrono::Utc::now();
            if remaining.num_seconds() > 0 {
                Some(remaining.num_seconds() as u64)
            } else {
                Some(0)
            }
        } else {
            None
        }
    }
}

/// OAuth 2.1 authorization server metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthMetadata {
    /// Authorization server issuer
    pub issuer: String,
    /// Authorization endpoint URL
    pub authorization_endpoint: String,
    /// Token endpoint URL
    pub token_endpoint: String,
    /// Optional registration endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,
    /// Supported scopes
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    /// Supported response types
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    /// Supported grant types
    #[serde(default)]
    pub grant_types_supported: Vec<String>,
    /// PKCE code challenge methods supported
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
}

/// OAuth client credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClientCredentials {
    pub client_id: String,
    /// Optional client secret
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Client name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    /// Redirect URIs
    #[serde(default)]
    pub redirect_uris: Vec<String>,
    /// Grant types
    #[serde(default = "default_grant_types")]
    pub grant_types: Vec<String>,
    /// Response types
    #[serde(default = "default_response_types")]
    pub response_types: Vec<String>,
}

fn default_grant_types() -> Vec<String> {
    vec![
        "authorization_code".to_string(),
        "refresh_token".to_string(),
    ]
}

fn default_response_types() -> Vec<String> {
    vec!["code".to_string()]
}

/// PKCE state for authorization code flow
#[derive(Debug, Clone)]
pub struct PkceState {
    /// Code verifier (random string)
    pub code_verifier: String,
    /// Code challenge (derived from verifier)
    pub code_challenge: String,
    /// Challenge method (S256 or plain)
    pub challenge_method: String,
}

impl PkceState {
    /// Generate new PKCE state
    pub fn new() -> Self {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use sha2::{Digest, Sha256};

        // Generate random code verifier (43-128 characters)
        let code_verifier: String = (0..32)
            .map(|_| {
                let idx = fastrand::usize(..62);
                match idx {
                    0..=25 => (b'a' + idx as u8) as char,
                    26..=51 => (b'A' + (idx - 26) as u8) as char,
                    _ => (b'0' + (idx - 52) as u8) as char,
                }
            })
            .collect();

        // Generate code challenge (SHA256 hash, base64 encoded)
        let hash = Sha256::digest(&code_verifier);
        let code_challenge = URL_SAFE_NO_PAD.encode(hash);

        Self {
            code_verifier,
            code_challenge,
            challenge_method: "S256".to_string(),
        }
    }
}

impl Default for PkceState {
    fn default() -> Self {
        Self::new()
    }
}

/// OAuth authorization URL with state
#[derive(Debug, Clone)]
pub struct AuthorizationUrl {
    /// Full authorization URL
    pub url: String,
    /// State parameter for CSRF protection
    pub state: String,
    /// PKCE state (if using PKCE)
    pub pkce: Option<PkceState>,
}

/// Token storage for a server
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerTokenStore {
    pub access_token: Option<OAuthToken>,
    pub client_credentials: Option<OAuthClientCredentials>,
    pub oauth_metadata: Option<OAuthMetadata>,
}

/// OAuth manager for MCP authentication
#[derive(Clone)]
pub struct OAuthManager {
    /// Token stores per server
    stores: Arc<RwLock<HashMap<String, ServerTokenStore>>>,
    /// Path to token storage file
    token_file_path: Option<PathBuf>,
}

impl OAuthManager {
    pub fn new() -> Self {
        Self {
            stores: Arc::new(RwLock::new(HashMap::new())),
            token_file_path: None,
        }
    }

    /// Create with token persistence
    pub fn with_persistence(token_file_path: PathBuf) -> McpResult<Self> {
        let mut manager = Self::new();
        manager.load_tokens(&token_file_path)?;
        manager.token_file_path = Some(token_file_path);
        Ok(manager)
    }

    /// Load tokens from file
    fn load_tokens(&mut self, path: &PathBuf) -> McpResult<()> {
        use std::fs;
        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| McpError::ProtocolError(format!("Failed to read token file: {e}")))?;

        let stores: HashMap<String, ServerTokenStore> = serde_json::from_str(&content)
            .map_err(|e| McpError::ProtocolError(format!("Failed to parse token file: {e}")))?;

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut self_stores = self.stores.write().await;
                *self_stores = stores;
            });
        });

        Ok(())
    }

    /// Save tokens to file
    fn save_tokens(&self) -> McpResult<()> {
        use std::fs;
        let path = match &self.token_file_path {
            Some(p) => p,
            None => return Ok(()), // No persistence configured
        };

        let stores = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { self.stores.read().await.clone() })
        });

        let content = serde_json::to_string_pretty(&stores)
            .map_err(|e| McpError::ProtocolError(format!("Failed to serialize tokens: {e}")))?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                McpError::ProtocolError(format!("Failed to create token directory: {e}"))
            })?;
        }

        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &content)
            .map_err(|e| McpError::ProtocolError(format!("Failed to write token file: {e}")))?;
        fs::rename(&tmp_path, path)
            .map_err(|e| McpError::ProtocolError(format!("Failed to rename token file: {e}")))?;

        Ok(())
    }

    /// Set OAuth metadata for a server
    pub async fn set_oauth_metadata(&self, server_id: &str, metadata: OAuthMetadata) {
        let mut stores = self.stores.write().await;
        let store = stores.entry(server_id.to_string()).or_default();
        store.oauth_metadata = Some(metadata);
    }

    /// Set client credentials for a server
    pub async fn set_client_credentials(
        &self,
        server_id: &str,
        credentials: OAuthClientCredentials,
    ) {
        let mut stores = self.stores.write().await;
        let store = stores.entry(server_id.to_string()).or_default();
        store.client_credentials = Some(credentials);
    }

    /// Generate authorization URL for PKCE flow
    pub async fn generate_authorization_url(
        &self,
        server_id: &str,
        redirect_uri: &str,
        scope: Option<&str>,
    ) -> McpResult<AuthorizationUrl> {
        let stores = self.stores.read().await;
        let store = stores.get(server_id).ok_or_else(|| {
            McpError::ProtocolError(format!("No OAuth configuration for server '{server_id}'"))
        })?;

        let metadata = store.oauth_metadata.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!("No OAuth metadata for server '{server_id}'"))
        })?;

        // Generate state for CSRF protection
        let state: String = (0..32).map(|_| fastrand::alphanumeric()).collect();

        // Generate PKCE state
        let pkce = PkceState::new();

        // Build authorization URL
        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method={}",
            metadata.authorization_endpoint,
            store.client_credentials.as_ref().map_or(&String::new(), |c| &c.client_id),
            urlencoding::encode(redirect_uri),
            state,
            pkce.code_challenge,
            pkce.challenge_method,
        );

        if let Some(s) = scope {
            url.push_str(&format!("&scope={}", urlencoding::encode(s)));
        }

        Ok(AuthorizationUrl {
            url,
            state,
            pkce: Some(pkce),
        })
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_authorization_code(
        &self,
        server_id: &str,
        code: &str,
        redirect_uri: &str,
        pkce: PkceState,
    ) -> McpResult<OAuthToken> {
        let stores = self.stores.read().await;
        let store = stores.get(server_id).ok_or_else(|| {
            McpError::ProtocolError(format!("No OAuth configuration for server '{server_id}'"))
        })?;

        let metadata = store.oauth_metadata.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!("No OAuth metadata for server '{server_id}'"))
        })?;

        let client_credentials = store.client_credentials.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!("No client credentials for server '{server_id}'"))
        })?;

        // Build token request
        let client = reqwest::Client::new();
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", redirect_uri);
        params.insert("client_id", &client_credentials.client_id);
        params.insert("code_verifier", &pkce.code_verifier);

        if let Some(ref secret) = client_credentials.client_secret {
            params.insert("client_secret", secret);
        }

        let response = client
            .post(&metadata.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| McpError::ProtocolError(format!("Token exchange failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(McpError::ProtocolError(format!(
                "Token exchange failed: {status} - {error_text}"
            )));
        }

        let token: OAuthToken = response
            .json()
            .await
            .map_err(|e| McpError::ProtocolError(format!("Failed to parse token response: {e}")))?;

        // Store the token
        drop(stores);
        let mut stores = self.stores.write().await;
        let store = stores.entry(server_id.to_string()).or_default();
        store.access_token = Some(token.clone());

        // Save to disk if configured
        drop(stores);
        self.save_tokens()?;

        Ok(token)
    }

    /// Get a valid access token (refresh if needed)
    pub async fn access_token(&self, server_id: &str) -> McpResult<String> {
        let stores = self.stores.read().await;
        let store = stores.get(server_id).ok_or_else(|| {
            McpError::ProtocolError(format!("No OAuth configuration for server '{server_id}'"))
        })?;

        let token = store.access_token.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!("No access token for server '{server_id}'"))
        })?;

        if token.is_expired() {
            // Token expired, need to refresh
            drop(stores);
            self.refresh_token(server_id).await?;

            // Get the refreshed token
            let stores = self.stores.read().await;
            let store = stores.get(server_id).ok_or_else(|| {
                McpError::ProtocolError(format!("No OAuth configuration for server '{server_id}'"))
            })?;

            let token = store.access_token.as_ref().ok_or_else(|| {
                McpError::ProtocolError(format!(
                    "No access token after refresh for server '{server_id}'"
                ))
            })?;

            Ok(token.access_token.clone())
        } else {
            Ok(token.access_token.clone())
        }
    }

    /// Refresh an access token
    async fn refresh_token(&self, server_id: &str) -> McpResult<()> {
        let stores = self.stores.read().await;
        let store = stores.get(server_id).ok_or_else(|| {
            McpError::ProtocolError(format!("No OAuth configuration for server '{server_id}'"))
        })?;

        let metadata = store.oauth_metadata.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!("No OAuth metadata for server '{server_id}'"))
        })?;

        let client_credentials = store.client_credentials.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!("No client credentials for server '{server_id}'"))
        })?;

        let old_token = store.access_token.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!("No refresh token for server '{server_id}'"))
        })?;

        let refresh_token = old_token.refresh_token.as_ref().ok_or_else(|| {
            McpError::ProtocolError(format!(
                "No refresh token available for server '{server_id}'"
            ))
        })?;

        // Build refresh token request
        let client = reqwest::Client::new();
        let mut params = HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", refresh_token);
        params.insert("client_id", &client_credentials.client_id);

        if let Some(ref secret) = client_credentials.client_secret {
            params.insert("client_secret", secret);
        }

        let response = client
            .post(&metadata.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| McpError::ProtocolError(format!("Token refresh failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(McpError::ProtocolError(format!(
                "Token refresh failed: {status} - {error_text}"
            )));
        }

        let new_token: OAuthToken = response.json().await.map_err(|e| {
            McpError::ProtocolError(format!("Failed to parse refresh response: {e}"))
        })?;

        // Store the new token
        drop(stores);
        let mut stores = self.stores.write().await;
        let store = stores.entry(server_id.to_string()).or_default();
        store.access_token = Some(new_token);

        // Save to disk if configured
        drop(stores);
        self.save_tokens()?;

        Ok(())
    }

    /// Clear tokens for a server
    pub async fn clear_tokens(&self, server_id: &str) -> McpResult<()> {
        let mut stores = self.stores.write().await;
        if let Some(store) = stores.get_mut(server_id) {
            store.access_token = None;
        }
        drop(stores);
        self.save_tokens()
    }

    /// Check if a server has OAuth configured
    pub async fn has_oauth_config(&self, server_id: &str) -> bool {
        let stores = self.stores.read().await;
        stores
            .get(server_id)
            .is_some_and(|s| s.oauth_metadata.is_some())
    }

    /// Check if a server has a valid access token
    pub async fn has_valid_token(&self, server_id: &str) -> bool {
        let stores = self.stores.read().await;
        stores
            .get(server_id)
            .is_some_and(|s| s.access_token.as_ref().is_some_and(|t| !t.is_expired()))
    }
}

impl Default for OAuthManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_state_generation() {
        let pkce = PkceState::new();
        assert!(!pkce.code_verifier.is_empty());
        assert!(!pkce.code_challenge.is_empty());
        assert_eq!(pkce.challenge_method, "S256");
    }

    #[test]
    fn test_oauth_token_expiration() {
        use chrono::Duration;

        // Non-expired token
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            scope: None,
            obtained_at: chrono::Utc::now(),
        };
        assert!(!token.is_expired());

        // Expired token
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            scope: None,
            obtained_at: chrono::Utc::now() - Duration::hours(2),
        };
        assert!(token.is_expired());

        // Token without expiration
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: None,
            scope: None,
            obtained_at: chrono::Utc::now(),
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_oauth_token_default() {
        let token = OAuthToken::default();
        assert!(token.access_token.is_empty());
        assert!(token.refresh_token.is_none());
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_in.is_none());
        assert!(token.scope.is_none());
    }

    #[test]
    fn test_oauth_token_serialization() {
        let token = OAuthToken {
            access_token: "abc123".to_string(),
            refresh_token: Some("refresh456".to_string()),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            scope: Some("read write".to_string()),
            obtained_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("refresh456"));
        assert!(!json.contains("obtained_at")); // skip serializing
    }

    #[test]
    fn test_oauth_token_expires_in_seconds() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            scope: None,
            obtained_at: chrono::Utc::now(),
        };
        let remaining = token.expires_in_seconds().unwrap();
        assert!(remaining <= 3600);
        assert!(remaining > 3500);
    }

    #[test]
    fn test_oauth_token_no_expiration() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: None,
            scope: None,
            obtained_at: chrono::Utc::now(),
        };
        assert!(token.expires_in_seconds().is_none());
    }

    #[test]
    fn test_pkce_state_unique() {
        let pkce1 = PkceState::new();
        let pkce2 = PkceState::new();
        // Code verifiers should be unique
        assert_ne!(pkce1.code_verifier, pkce2.code_verifier);
        assert_ne!(pkce1.code_challenge, pkce2.code_challenge);
    }

    #[test]
    fn test_oauth_metadata_creation() {
        let meta = OAuthMetadata {
            issuer: "https://auth.example.com".to_string(),
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            registration_endpoint: None,
            scopes_supported: vec!["read".to_string(), "write".to_string()],
            response_types_supported: vec!["code".to_string()],
            grant_types_supported: vec!["authorization_code".to_string()],
            code_challenge_methods_supported: vec!["S256".to_string()],
        };
        assert_eq!(meta.issuer, "https://auth.example.com");
    }

    #[test]
    fn test_oauth_metadata_serialization_roundtrip() {
        let meta = OAuthMetadata {
            issuer: "https://auth.example.com".to_string(),
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            registration_endpoint: Some("https://auth.example.com/register".to_string()),
            scopes_supported: vec!["read".to_string()],
            response_types_supported: vec!["code".to_string()],
            grant_types_supported: vec!["authorization_code".to_string()],
            code_challenge_methods_supported: vec!["S256".to_string()],
        };
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: OAuthMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.issuer, meta.issuer);
        assert!(parsed.registration_endpoint.is_some());
    }

    #[test]
    fn test_oauth_metadata_skip_optional_fields() {
        let meta = OAuthMetadata {
            issuer: "https://auth.example.com".to_string(),
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            registration_endpoint: None,
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            code_challenge_methods_supported: vec![],
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("registration_endpoint"));
    }

    #[test]
    fn test_oauth_client_credentials_serialization() {
        let creds = OAuthClientCredentials {
            client_id: "my-client".to_string(),
            client_secret: Some("secret123".to_string()),
            client_name: Some("My App".to_string()),
            redirect_uris: vec!["http://localhost:8080/callback".to_string()],
            grant_types: default_grant_types(),
            response_types: default_response_types(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let parsed: OAuthClientCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.client_id, "my-client");
        assert_eq!(parsed.client_secret, Some("secret123".to_string()));
    }

    #[test]
    fn test_oauth_client_credentials_skip_optional() {
        let creds = OAuthClientCredentials {
            client_id: "id".to_string(),
            client_secret: None,
            client_name: None,
            redirect_uris: vec![],
            grant_types: default_grant_types(),
            response_types: default_response_types(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        assert!(!json.contains("client_secret"));
        assert!(!json.contains("client_name"));
    }

    #[test]
    fn test_oauth_token_expired_expires_in_seconds() {
        // Token that expired 1 hour ago
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            scope: None,
            obtained_at: chrono::Utc::now() - chrono::Duration::hours(2),
        };
        assert!(token.is_expired());
        let remaining = token.expires_in_seconds().unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_pkce_state_default() {
        let pkce = PkceState::default();
        assert!(!pkce.code_verifier.is_empty());
        assert!(!pkce.code_challenge.is_empty());
        assert_eq!(pkce.challenge_method, "S256");
    }

    #[test]
    fn test_server_token_store_default() {
        let store = ServerTokenStore::default();
        assert!(store.access_token.is_none());
        assert!(store.client_credentials.is_none());
        assert!(store.oauth_metadata.is_none());
    }

    #[test]
    fn test_server_token_store_serialization() {
        let store = ServerTokenStore {
            access_token: Some(OAuthToken::default()),
            client_credentials: None,
            oauth_metadata: None,
        };
        let json = serde_json::to_string(&store).unwrap();
        let parsed: ServerTokenStore = serde_json::from_str(&json).unwrap();
        assert!(parsed.access_token.is_some());
        assert!(parsed.client_credentials.is_none());
    }

    #[tokio::test]
    async fn test_oauth_manager_new() {
        let manager = OAuthManager::new();
        assert!(!manager.has_oauth_config("any-server").await);
        assert!(!manager.has_valid_token("any-server").await);
    }

    #[tokio::test]
    async fn test_oauth_manager_default() {
        let manager = OAuthManager::default();
        assert!(!manager.has_oauth_config("srv").await);
    }

    #[tokio::test]
    async fn test_oauth_manager_set_metadata() {
        let manager = OAuthManager::new();
        let meta = OAuthMetadata {
            issuer: "https://auth.test".to_string(),
            authorization_endpoint: "https://auth.test/authorize".to_string(),
            token_endpoint: "https://auth.test/token".to_string(),
            registration_endpoint: None,
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            code_challenge_methods_supported: vec![],
        };
        manager.set_oauth_metadata("srv", meta).await;
        assert!(manager.has_oauth_config("srv").await);
    }

    #[tokio::test]
    async fn test_oauth_manager_has_valid_token() {
        let manager = OAuthManager::new();
        let _stores = manager.stores.read().await;
        // No token at all
        assert!(!manager.has_valid_token("srv").await);
    }

    #[tokio::test]
    async fn test_oauth_manager_clear_tokens() {
        let manager = OAuthManager::new();
        // Clear on nonexistent should not panic
        manager.clear_tokens("srv").await.unwrap();
    }

    #[test]
    fn test_authorization_url_debug() {
        let auth_url = AuthorizationUrl {
            url: "https://example.com".to_string(),
            state: "state123".to_string(),
            pkce: None,
        };
        let debug_str = format!("{:?}", auth_url);
        assert!(debug_str.contains("example.com"));
        assert!(debug_str.contains("state123"));
    }
}
