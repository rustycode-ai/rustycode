#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::items_after_statements
)]

//! Integration tests for OAuth device flow against real GitHub API.
//!
//! These tests hit the live GitHub device code endpoint.
//! They are ignored by default to avoid network dependency in CI.
//! Run with: cargo test -p rustycode-auth --test oauth_device_flow -- --ignored

use rustycode_auth::oauth::DefaultOAuthClient;
use rustycode_auth::{
    AuthMethod, AuthToken, GitHubCopilotAuth, OAuthClient, OAuthConfig, TokenStore,
};
use secrecy::ExposeSecret;

/// Test Step 1: Device code request against live GitHub API.
#[tokio::test]
#[ignore = "requires live GitHub API access"]
async fn device_code_request_hits_github() {
    let auth = GitHubCopilotAuth::new();
    let device = auth
        .request_device_code()
        .await
        .expect("device code request should succeed");

    assert!(
        !device.device_code.is_empty(),
        "device_code must be non-empty"
    );
    assert!(!device.user_code.is_empty(), "user_code must be non-empty");
    assert!(
        device.verification_uri.contains("github.com"),
        "verification_uri should point to github.com: got {}",
        device.verification_uri
    );
    assert!(device.expires_in > 0, "expires_in should be positive");
    assert!(device.interval >= 5, "interval should be at least 5s");
}

/// Test that the full login flow starts correctly (device code only, no polling).
#[tokio::test]
#[ignore = "requires live GitHub API access"]
async fn copilot_auth_new_creates_valid_client() {
    let auth = GitHubCopilotAuth::new();
    let device = auth.request_device_code().await;
    assert!(device.is_ok(), "should get a device code from GitHub");
}

/// Test the generic OAuth client PKCE flow (authorize URL generation).
#[tokio::test]
async fn generic_oauth_authorize_generates_valid_pkce_url() {
    let client = DefaultOAuthClient::new();
    let config = OAuthConfig {
        client_id: "test-integration-client".to_string(),
        client_secret: None,
        auth_url: "https://github.com/login/oauth/authorize".to_string(),
        token_url: "https://github.com/login/oauth/access_token".to_string(),
        redirect_url: "http://localhost:9090/callback".to_string(),
        scopes: vec!["read:user".to_string()],
    };

    let method = client
        .authorize(&config)
        .await
        .expect("authorize should succeed");

    match method {
        AuthMethod::Code {
            url,
            verifier,
            state,
        } => {
            // URL must contain all required OAuth params
            assert!(
                url.contains("client_id=test-integration-client"),
                "URL missing client_id"
            );
            assert!(
                url.contains("response_type=code"),
                "URL missing response_type"
            );
            assert!(
                url.contains("code_challenge="),
                "URL missing PKCE challenge"
            );
            assert!(
                url.contains("code_challenge_method=S256"),
                "URL missing challenge method"
            );
            assert!(url.contains("state="), "URL missing CSRF state");
            assert!(url.contains("scope="), "URL missing scope");

            // PKCE verifier must be RFC 7636 compliant (43-128 chars)
            assert!(
                verifier.len() >= 43,
                "PKCE verifier too short: {} chars",
                verifier.len()
            );
            assert!(
                verifier.len() <= 128,
                "PKCE verifier too long: {} chars",
                verifier.len()
            );

            // State must be non-empty for CSRF protection
            assert!(!state.is_empty(), "CSRF state must not be empty");
        }
        AuthMethod::Auto { .. } => panic!("Expected AuthMethod::Code, got Auto"),
        _ => panic!("Expected AuthMethod::Code, got unknown variant"),
    }
}

/// Test that two consecutive authorize calls produce different PKCE verifiers and states.
#[tokio::test]
async fn oauth_authorize_produces_unique_verifiers() {
    let client = DefaultOAuthClient::new();
    let config = OAuthConfig {
        client_id: "uniqueness-test".to_string(),
        client_secret: None,
        auth_url: "https://auth.example.com/authorize".to_string(),
        token_url: "https://auth.example.com/token".to_string(),
        redirect_url: "http://localhost:9999/cb".to_string(),
        scopes: vec!["read".to_string()],
    };

    let first = client.authorize(&config).await.expect("first authorize");
    let second = client.authorize(&config).await.expect("second authorize");

    // Extract verifiers and states
    let (v1, s1) = match &first {
        rustycode_auth::AuthMethod::Code {
            verifier, state, ..
        } => (verifier.clone(), state.clone()),
        _ => panic!("Expected Code variant"),
    };
    let (v2, s2) = match &second {
        rustycode_auth::AuthMethod::Code {
            verifier, state, ..
        } => (verifier.clone(), state.clone()),
        _ => panic!("Expected Code variant"),
    };

    assert_ne!(v1, v2, "each authorize must produce a unique PKCE verifier");
    assert_ne!(s1, s2, "each authorize must produce a unique CSRF state");
}

/// Check whether an OS keyring backend is available.
/// Returns false in headless/SSH sessions where no keyring daemon is running.
fn keyring_available() -> bool {
    use keyring_core::Entry;
    Entry::new("rustycode-auth-test-probe", "probe")
        .and_then(|e| e.get_password())
        .is_ok()
        || Entry::new("rustycode-auth-test-probe", "probe").is_ok()
}

/// Test compute_expires_at via token store — verify valid tokens pass is_token_valid.
/// Requires an OS keyring backend (skipped in headless environments).
#[test]
fn token_store_validates_expiry_timestamps() {
    if !keyring_available() {
        eprintln!("  (skipped: no OS keyring backend available)");
        return;
    }
    let store = TokenStore::new();
    let provider_id = "test-validation-provider";

    // Token that expires far in the future
    let future_token = AuthToken {
        access_token: "future-token".to_string().into(),
        refresh_token: None,
        expires_at: Some(9_999_999_999),
        token_type: "bearer".to_string(),
    };

    // Store it
    store
        .store_token(provider_id, &future_token)
        .expect("store should succeed");

    // Validate
    let is_valid = store
        .is_token_valid(provider_id)
        .expect("validity check should succeed");
    assert!(is_valid, "future token should be valid");

    // Clean up
    store
        .delete_token(provider_id)
        .expect("delete should succeed");
}

/// Test token store roundtrip: store → retrieve → verify content matches.
/// Requires an OS keyring backend (skipped in headless environments).
#[test]
fn token_store_roundtrip_preserves_token_data() {
    if !keyring_available() {
        eprintln!("  (skipped: no OS keyring backend available)");
        return;
    }
    let store = TokenStore::new();
    let provider_id = "test-roundtrip-provider";

    let original = AuthToken {
        access_token: "at_roundtrip_test".to_string().into(),
        refresh_token: Some("rt_roundtrip_test".to_string().into()),
        expires_at: Some(9_999_999_999),
        token_type: "bearer".to_string(),
    };

    store
        .store_token(provider_id, &original)
        .expect("store should succeed");

    let retrieved = store.token(provider_id).expect("retrieve should succeed");

    assert_eq!(retrieved.access_token.expose_secret(), "at_roundtrip_test");
    assert_eq!(
        retrieved
            .refresh_token
            .as_ref()
            .map(|s| s.expose_secret().to_string()),
        Some("rt_roundtrip_test".to_string())
    );
    assert_eq!(retrieved.expires_at, Some(9_999_999_999));
    assert_eq!(retrieved.token_type, "bearer");

    // Clean up
    store
        .delete_token(provider_id)
        .expect("delete should succeed");
}

/// Test that storing a token overwrites the previous one.
/// Requires an OS keyring backend (skipped in headless environments).
#[test]
fn token_store_overwrite_updates_token() {
    if !keyring_available() {
        eprintln!("  (skipped: no OS keyring backend available)");
        return;
    }
    let store = TokenStore::new();
    let provider_id = "test-overwrite-provider";

    let first = AuthToken {
        access_token: "first_token".to_string().into(),
        refresh_token: None,
        expires_at: None,
        token_type: "bearer".to_string(),
    };
    let second = AuthToken {
        access_token: "second_token".to_string().into(),
        refresh_token: Some("new_refresh".to_string().into()),
        expires_at: Some(9_999_999_999),
        token_type: "bearer".to_string(),
    };

    store
        .store_token(provider_id, &first)
        .expect("first store should succeed");
    store
        .store_token(provider_id, &second)
        .expect("overwrite should succeed");

    let retrieved = store.token(provider_id).expect("retrieve should succeed");
    assert_eq!(
        retrieved.access_token.expose_secret(),
        "second_token",
        "should get the overwritten token, not the first"
    );
    assert!(retrieved.refresh_token.is_some());

    store
        .delete_token(provider_id)
        .expect("delete should succeed");
}

/// Test that delete followed by retrieve returns error.
/// Requires an OS keyring backend (skipped in headless environments).
#[test]
fn token_store_delete_then_get_fails() {
    if !keyring_available() {
        eprintln!("  (skipped: no OS keyring backend available)");
        return;
    }
    let store = TokenStore::new();
    let provider_id = "test-delete-provider";

    let token = AuthToken {
        access_token: "to_delete".to_string().into(),
        refresh_token: None,
        expires_at: None,
        token_type: "bearer".to_string(),
    };

    store
        .store_token(provider_id, &token)
        .expect("store should succeed");
    store
        .delete_token(provider_id)
        .expect("delete should succeed");

    let result = store.token(provider_id);
    assert!(result.is_err(), "getting deleted token should fail");
}
