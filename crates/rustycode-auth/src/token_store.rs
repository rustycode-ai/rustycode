//! Secure token storage using keyring

use crate::{AuthError, AuthResult, AuthToken};
use keyring::Entry;
use serde_json;

const SERVICE_NAME: &str = "rustycode-auth";

/// Stored token with provider metadata
#[derive(Debug, Clone)]
pub struct StoredToken {
    pub provider_id: String,
    pub token: AuthToken,
}

/// Secure token storage backed by OS keyring
#[derive(Debug, Clone)]
pub struct TokenStore {
    service: String,
}

impl TokenStore {
    pub fn new() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
        }
    }

    pub fn store_token(&self, provider_id: &str, token: &AuthToken) -> AuthResult<()> {
        let entry = Entry::new(&self.service, provider_id)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;

        let data: crate::AuthTokenData = token.into();
        let serialized = serde_json::to_string(&data)?;
        entry
            .set_password(&serialized)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;

        Ok(())
    }

    pub fn get_token(&self, provider_id: &str) -> AuthResult<AuthToken> {
        let entry = Entry::new(&self.service, provider_id)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;

        let password = entry
            .get_password()
            .map_err(|e| AuthError::Keyring(e.to_string()))?;

        let data: crate::AuthTokenData = serde_json::from_str(&password)?;
        Ok(data.into())
    }

    pub fn delete_token(&self, provider_id: &str) -> AuthResult<()> {
        let entry = Entry::new(&self.service, provider_id)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;

        entry
            .delete_credential()
            .map_err(|e| AuthError::Keyring(e.to_string()))?;

        Ok(())
    }

    pub fn is_token_valid(&self, provider_id: &str) -> AuthResult<bool> {
        let token = self.get_token(provider_id)?;

        Ok(token.expires_at.is_none_or(|expires_at| {
            #[allow(clippy::cast_possible_wrap)]
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_secs() as i64;
            expires_at > now
        }))
    }
}

impl Default for TokenStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthToken;
    use secrecy::ExposeSecret;

    #[test]
    fn test_token_store_new() {
        let store = TokenStore::new();
        assert_eq!(store.service, SERVICE_NAME);
    }

    #[test]
    fn test_token_store_default() {
        let store = TokenStore::default();
        assert_eq!(store.service, SERVICE_NAME);
    }

    #[test]
    fn test_token_store_clone() {
        let store = TokenStore::new();
        let cloned = store.clone();
        assert_eq!(cloned.service, store.service);
    }

    #[test]
    fn test_token_store_debug() {
        let store = TokenStore::new();
        let debug = format!("{store:?}");
        assert!(debug.contains("TokenStore"));
    }

    #[test]
    fn test_token_store_custom_service() {
        // TokenStore always uses SERVICE_NAME currently, but test the field
        let store = TokenStore::new();
        assert_eq!(store.service, "rustycode-auth");
    }

    #[test]
    fn test_stored_token_construction() {
        let token = AuthToken {
            access_token: "at_123".to_string().into(),
            refresh_token: Some("rt_456".to_string().into()),
            expires_at: Some(9_999_999_999),
            token_type: "bearer".to_string(),
        };
        let stored = StoredToken {
            provider_id: "test-provider".to_string(),
            token,
        };
        assert_eq!(stored.provider_id, "test-provider");
        assert_eq!(stored.token.access_token.expose_secret(), "at_123");
        assert_eq!(
            stored
                .token
                .refresh_token
                .as_ref()
                .map(|s| s.expose_secret().to_string()),
            Some("rt_456".to_string())
        );
    }

    #[test]
    fn test_stored_token_clone() {
        let token = AuthToken {
            access_token: "at".to_string().into(),
            refresh_token: None,
            expires_at: None,
            token_type: "bearer".to_string(),
        };
        let stored = StoredToken {
            provider_id: "p".to_string(),
            token,
        };
        let cloned = stored.clone();
        assert_eq!(cloned.provider_id, stored.provider_id);
        assert_eq!(
            cloned.token.access_token.expose_secret(),
            stored.token.access_token.expose_secret()
        );
    }

    #[test]
    fn test_stored_token_debug() {
        let token = AuthToken {
            access_token: "at".to_string().into(),
            refresh_token: None,
            expires_at: None,
            token_type: "bearer".to_string(),
        };
        let stored = StoredToken {
            provider_id: "provider".to_string(),
            token,
        };
        let debug = format!("{stored:?}");
        assert!(debug.contains("StoredToken"));
        assert!(debug.contains("provider"));
    }

    #[test]
    fn test_service_name_constant() {
        assert_eq!(SERVICE_NAME, "rustycode-auth");
        assert!(!SERVICE_NAME.is_empty());
    }

    #[test]
    fn test_get_token_missing_provider_returns_error() {
        let store = TokenStore::new();
        let result = store.get_token("nonexistent-provider-xyz");
        assert!(result.is_err());
        match result {
            Err(AuthError::Keyring(_)) => {} // Expected
            other => panic!("Expected Keyring error, got: {other:?}"),
        }
    }

    #[test]
    fn test_delete_token_missing_provider_returns_error() {
        let store = TokenStore::new();
        let result = store.delete_token("nonexistent-provider-xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_token_valid_missing_provider_returns_error() {
        let store = TokenStore::new();
        let result = store.is_token_valid("nonexistent-provider-xyz");
        assert!(result.is_err());
    }
}
