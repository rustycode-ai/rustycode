//! Authentication error types

use thiserror::Error;

pub type AuthResult<T> = Result<T, AuthError>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AuthError {
    #[error("OAuth flow error: {0}")]
    OAuth(String),

    #[error("Token storage error: {0}")]
    TokenStorage(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Keyring error: {0}")]
    Keyring(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_error_display() {
        let err = AuthError::OAuth("invalid grant".to_string());
        assert!(format!("{err}").contains("OAuth flow error"));
        assert!(format!("{err}").contains("invalid grant"));
    }

    #[test]
    fn test_token_storage_error_display() {
        let err = AuthError::TokenStorage("disk full".to_string());
        assert!(format!("{err}").contains("Token storage error"));
    }

    #[test]
    fn test_invalid_token_error_display() {
        let err = AuthError::InvalidToken("expired".to_string());
        assert!(format!("{err}").contains("Invalid token"));
        assert!(format!("{err}").contains("expired"));
    }

    #[test]
    fn test_keyring_error_display() {
        let err = AuthError::Keyring("not found".to_string());
        assert!(format!("{err}").contains("Keyring error"));
    }

    #[test]
    fn test_auth_result_ok() {
        let result: AuthResult<String> = Ok("success".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_auth_result_err() {
        let result: AuthResult<String> = Err(AuthError::OAuth("fail".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(AuthError::OAuth("test".into()));
        assert!(err.to_string().contains("OAuth flow error"));
    }

    #[test]
    fn test_json_error_from_serde() {
        let bad_json = "not valid json {{{";
        let result: Result<serde_json::Value, _> = serde_json::from_str(bad_json);
        let serde_err = result.unwrap_err();
        let auth_err: AuthError = AuthError::Json(serde_err);
        assert!(format!("{auth_err}").contains("JSON error"));
    }

    #[test]
    fn test_all_error_variants_have_messages() {
        // Verify each variant produces a non-empty display message
        let cases: Vec<AuthError> = vec![
            AuthError::OAuth("oauth issue".into()),
            AuthError::TokenStorage("storage issue".into()),
            AuthError::InvalidToken("bad token".into()),
            AuthError::Keyring("keyring issue".into()),
        ];
        for err in &cases {
            let msg = format!("{err}");
            assert!(!msg.is_empty(), "Error {err:?} had empty display");
        }
    }

    #[test]
    fn test_oauth_error_source_chain() {
        let err = AuthError::OAuth("token expired at 12345".into());
        assert!(err.to_string().contains("12345"));
    }

    #[test]
    fn test_token_storage_error_specificity() {
        let err = AuthError::TokenStorage("keychain full, cannot write".into());
        let msg = format!("{err}");
        assert!(msg.contains("Token storage error"));
        assert!(msg.contains("keychain full"));
    }

    #[test]
    fn test_invalid_token_error_specificity() {
        let err = AuthError::InvalidToken("JWT signature mismatch".into());
        let msg = format!("{err}");
        assert!(msg.contains("Invalid token"));
        assert!(msg.contains("JWT signature mismatch"));
    }

    #[test]
    fn test_keyring_error_specificity() {
        let err = AuthError::Keyring("entry not found for provider: copilot".into());
        let msg = format!("{err}");
        assert!(msg.contains("Keyring error"));
        assert!(msg.contains("copilot"));
    }
}
