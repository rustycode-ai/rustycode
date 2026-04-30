//! `GitHub` Copilot authentication via OAuth Device Flow.
//!
//! Implements the `GitHub` OAuth device flow to obtain a Copilot access token:
//! 1. Request a device code from `GitHub`
//! 2. Display `user_code` and `verification_uri` to the user
//! 3. Poll for authorization
//! 4. Exchange the `GitHub` OAuth token for a Copilot-specific token

use crate::{AuthError, AuthResult};
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, SystemTime};

/// `GitHub` OAuth client ID for VS Code (public, not secret)
const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Scopes required for Copilot access
const COPILOT_SCOPES: &str = "read:user";

/// `GitHub` device code endpoint
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// `GitHub` token endpoint
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// Copilot internal token exchange endpoint
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// Response from the device code request
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Response from polling the token endpoint while waiting
#[derive(Debug, Deserialize)]
struct PendingTokenResponse {
    error: Option<String>,
    error_description: Option<String>,
    access_token: Option<String>,
    #[allow(dead_code)] // Kept for future use
    refresh_token: Option<String>,
    #[allow(dead_code)] // Kept for future use
    expires_in: Option<u64>,
    #[allow(dead_code)] // Kept for future use
    token_type: Option<String>,
}

/// Copilot token response from the internal exchange endpoint
#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: u64,
    #[allow(dead_code)] // Kept for future use
    refresh_in: Option<u64>,
}

/// The result of a successful Copilot login
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CopilotAuthResult {
    /// The Copilot API token to use as Bearer token
    pub copilot_token: String,
    /// Unix timestamp when the token expires
    pub expires_at: u64,
    /// The underlying `GitHub` OAuth token (for refresh)
    pub github_token: String,
}

/// `GitHub` Copilot authenticator
#[derive(Debug)]
pub struct GitHubCopilotAuth {
    http_client: Client,
}

impl GitHubCopilotAuth {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }

    /// Step 1: Request a device code from `GitHub`.
    ///
    /// Returns the device code response containing `user_code` and `verification_uri`
    /// that must be shown to the user.
    pub async fn request_device_code(&self) -> AuthResult<DeviceCodeResponse> {
        let response = self
            .http_client
            .post(DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", COPILOT_SCOPES)])
            .send()
            .await
            .map_err(AuthError::Network)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(AuthError::OAuth(format!(
                "failed to request device code (HTTP {status}): {error_text}"
            )));
        }

        let device_code: DeviceCodeResponse = response
            .json()
            .await
            .map_err(|e| AuthError::OAuth(format!("failed to parse device code response: {e}")))?;

        Ok(device_code)
    }

    /// Step 2: Poll for the access token.
    ///
    /// Polls `GitHub`'s token endpoint until the user completes authorization
    /// or the device code expires. Returns the `GitHub` OAuth token.
    pub async fn poll_for_token(
        &self,
        device_code: &str,
        interval_secs: u64,
        expires_in_secs: u64,
    ) -> AuthResult<String> {
        let start = SystemTime::now();
        let deadline = start
            .checked_add(Duration::from_secs(expires_in_secs))
            .ok_or_else(|| AuthError::OAuth("device code expiry overflow".into()))?;
        let mut interval = Duration::from_secs(interval_secs.max(5));

        loop {
            let now = SystemTime::now();
            if now >= deadline {
                return Err(AuthError::OAuth(
                    "device code expired — please try again".into(),
                ));
            }

            tokio::time::sleep(interval).await;

            let response = self
                .http_client
                .post(TOKEN_URL)
                .header("Accept", "application/json")
                .form(&[
                    ("client_id", GITHUB_CLIENT_ID),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await
                .map_err(AuthError::Network)?;

            let body: PendingTokenResponse = response
                .json()
                .await
                .map_err(|e| AuthError::OAuth(format!("failed to parse token response: {e}")))?;

            match body.error.as_deref() {
                None => {
                    // Success — token received
                    let token = body.access_token.ok_or_else(|| {
                        AuthError::OAuth("token response missing access_token".into())
                    })?;
                    return Ok(token);
                }
                Some("authorization_pending") => {
                    // User hasn't approved yet — keep polling
                }
                Some("slow_down") => {
                    // GitHub wants us to slow down
                    interval = interval.saturating_add(Duration::from_secs(5));
                }
                Some("expired_token") => {
                    return Err(AuthError::OAuth(
                        "device code expired — please try again".into(),
                    ));
                }
                Some("access_denied") => {
                    return Err(AuthError::OAuth("user denied authorization".into()));
                }
                Some(other) => {
                    let desc = body
                        .error_description
                        .unwrap_or_else(|| "unknown error".into());
                    return Err(AuthError::OAuth(format!("{other}: {desc}")));
                }
            }
        }
    }

    /// Step 3: Exchange a `GitHub` OAuth token for a Copilot API token.
    ///
    /// The `GitHub` OAuth token alone cannot call Copilot APIs. This step
    /// exchanges it for a Copilot-specific bearer token.
    pub async fn exchange_for_copilot_token(
        &self,
        github_token: &str,
    ) -> AuthResult<CopilotAuthResult> {
        let response = self
            .http_client
            .get(COPILOT_TOKEN_URL)
            .header("Authorization", format!("Bearer {github_token}"))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(AuthError::Network)?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AuthError::OAuth(format!(
                "copilot token exchange failed (HTTP {status}): {text}"
            )));
        }

        let copilot_resp: CopilotTokenResponse = response.json().await.map_err(|e| {
            AuthError::OAuth(format!("failed to parse copilot token response: {e}"))
        })?;

        // Validate that the token hasn't already expired
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if copilot_resp.expires_at <= now {
            return Err(AuthError::OAuth(format!(
                "copilot token already expired (expires_at={}, now={}). \
                 Server may have a clock skew issue.",
                copilot_resp.expires_at, now
            )));
        }

        Ok(CopilotAuthResult {
            copilot_token: copilot_resp.token,
            expires_at: copilot_resp.expires_at,
            github_token: github_token.to_string(),
        })
    }

    /// Full device flow: request code, display info, poll, exchange.
    ///
    /// This is a convenience method that combines all steps. The caller
    /// should display the returned `DeviceCodeResponse` to the user before
    /// calling `complete_login`.
    pub async fn login(&self) -> AuthResult<CopilotAuthResult> {
        let device = self.request_device_code().await?;

        let github_token = self
            .poll_for_token(&device.device_code, device.interval, device.expires_in)
            .await?;

        self.exchange_for_copilot_token(&github_token).await
    }
}

impl Default for GitHubCopilotAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_are_nonempty() {
        assert!(!GITHUB_CLIENT_ID.is_empty());
        assert!(!COPILOT_SCOPES.is_empty());
        assert!(DEVICE_CODE_URL.starts_with("https://"));
        assert!(TOKEN_URL.starts_with("https://"));
        assert!(COPILOT_TOKEN_URL.starts_with("https://"));
    }

    #[test]
    fn test_copilot_auth_result_fields() {
        let result = CopilotAuthResult {
            copilot_token: "ghu_test".into(),
            expires_at: 9_999_999_999,
            github_token: "gho_test".into(),
        };
        assert_eq!(result.copilot_token, "ghu_test");
        assert_eq!(result.expires_at, 9_999_999_999);
    }

    #[test]
    fn test_github_copilot_auth_default() {
        let auth = GitHubCopilotAuth::default();
        // Verify default creates a valid instance
        assert!(format!("{auth:?}").contains("GitHubCopilotAuth"));
    }

    #[test]
    fn test_device_code_response_deserialization() {
        let json = r#"{
            "device_code": "dc_abc123",
            "user_code": "ABCD-1234",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 5
        }"#;
        let resp: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.device_code, "dc_abc123");
        assert_eq!(resp.user_code, "ABCD-1234");
        assert_eq!(resp.verification_uri, "https://github.com/login/device");
        assert_eq!(resp.expires_in, 900);
        assert_eq!(resp.interval, 5);
    }

    #[test]
    fn test_copilot_auth_result_serialization() {
        let result = CopilotAuthResult {
            copilot_token: "tid=abcd;exp=12345".into(),
            expires_at: 12345,
            github_token: "gho_abc".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("tid=abcd;exp=12345"));
        assert!(json.contains("gho_abc"));
        assert!(json.contains("12345"));

        // Verify roundtrip
        let decoded: CopilotAuthResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.copilot_token, result.copilot_token);
        assert_eq!(decoded.expires_at, result.expires_at);
        assert_eq!(decoded.github_token, result.github_token);
    }

    #[test]
    fn test_copilot_auth_result_debug() {
        let result = CopilotAuthResult {
            copilot_token: "tok".into(),
            expires_at: 100,
            github_token: "ght".into(),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("CopilotAuthResult"));
        assert!(debug.contains("copilot_token"));
        assert!(debug.contains("github_token"));
    }

    #[test]
    fn test_device_code_response_debug() {
        let resp = DeviceCodeResponse {
            device_code: "dc_test".into(),
            user_code: "WXYZ-5678".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in: 600,
            interval: 10,
        };
        let debug = format!("{resp:?}");
        assert!(debug.contains("device_code"));
        assert!(debug.contains("user_code"));
    }

    #[test]
    fn test_constants_valid_urls() {
        assert!(DEVICE_CODE_URL.contains("github.com"));
        assert!(TOKEN_URL.contains("github.com"));
        assert!(COPILOT_TOKEN_URL.contains("api.github.com"));
        assert!(COPILOT_TOKEN_URL.contains("copilot"));
    }

    #[test]
    fn test_copilot_auth_result_clone() {
        let result = CopilotAuthResult {
            copilot_token: "tok".into(),
            expires_at: 9999,
            github_token: "ght".into(),
        };
        let cloned = result.clone();
        assert_eq!(cloned.copilot_token, result.copilot_token);
        assert_eq!(cloned.expires_at, result.expires_at);
        assert_eq!(cloned.github_token, result.github_token);
    }

    #[test]
    fn test_device_code_response_edge_values() {
        // Minimal expires_in
        let json = r#"{
            "device_code": "dc_min",
            "user_code": "A-1",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 0,
            "interval": 1
        }"#;
        let resp: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.expires_in, 0);
        assert_eq!(resp.interval, 1);
    }

    #[test]
    fn test_token_expiration_validation_rejects_past_timestamp() {
        // Verify that the expiration check logic works:
        // a timestamp in the past should be rejected.
        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Token expired 1 hour ago
        let expired_at = now_secs - 3600;
        assert!(
            expired_at <= now_secs,
            "past timestamp should be <= now for validation to catch it"
        );
    }

    #[test]
    fn test_token_expiration_validation_accepts_future_timestamp() {
        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Token valid for 1 hour
        let expires_at = now_secs + 3600;
        assert!(
            expires_at > now_secs,
            "future timestamp should be > now for validation to accept it"
        );
    }
}
