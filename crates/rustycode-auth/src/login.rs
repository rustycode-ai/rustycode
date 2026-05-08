//! Login orchestrator — coordinates the full OAuth flow end-to-end.
//!
//! Handles both authorization code flow (browser callback) and device flow,
//! with headless fallback for SSH/containers.

use crate::browser;
use crate::callback_server::CallbackServer;
use crate::oauth::{AuthMethod, OAuthClient};
use crate::providers::{self, ProviderAuthConfig, ProviderAuthMethod};
use crate::token_store::TokenStore;
use crate::{AuthError, AuthResult, AuthToken};

/// Result of a successful login.
#[derive(Debug)]
pub struct LoginResult {
    pub provider_id: String,
    pub token: AuthToken,
    pub method: LoginMethod,
}

/// How the user authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginMethod {
    /// Browser-based OAuth authorization code flow with PKCE.
    OAuthCallback,
    /// Device flow (poll for token).
    DeviceFlow,
    /// API key from environment variable or user input.
    ApiKey,
}

/// Perform login for a provider.
///
/// Dispatches to the appropriate flow based on the provider's supported methods:
/// 1. If API key is available in environment, use it directly
/// 2. If provider supports device flow, use that (works everywhere)
/// 3. If provider supports authorization code flow, use browser callback
/// 4. Fall back to prompting for API key in headless environments
pub async fn login(provider_id: &str, oauth_client: &dyn OAuthClient) -> AuthResult<LoginResult> {
    let provider = providers::find_provider(provider_id)
        .ok_or_else(|| AuthError::OAuth(format!("unknown provider: {provider_id}")))?;

    // 1. Check for API key in environment
    if let Some(key) = providers::env_api_key(provider_id) {
        let token = AuthToken {
            access_token: key.into(),
            refresh_token: None,
            expires_at: None,
            token_type: "bearer".to_string(),
        };
        return Ok(LoginResult {
            provider_id: provider_id.to_string(),
            token,
            method: LoginMethod::ApiKey,
        });
    }

    // 2. Determine preferred auth method
    let method = select_auth_method(provider)?;

    match method {
        LoginMethod::DeviceFlow => login_device_flow(provider_id, provider),
        LoginMethod::OAuthCallback => {
            login_authorization_code(provider_id, provider, oauth_client).await
        }
        LoginMethod::ApiKey => login_api_key_prompt(provider_id),
    }
}

/// Select the best auth method for the current environment.
fn select_auth_method(provider: &ProviderAuthConfig) -> AuthResult<LoginMethod> {
    let headless = browser::is_headless();

    for method in provider.auth_methods {
        match method {
            ProviderAuthMethod::DeviceFlow => return Ok(LoginMethod::DeviceFlow),
            ProviderAuthMethod::AuthorizationCode if !headless => {
                return Ok(LoginMethod::OAuthCallback);
            }
            ProviderAuthMethod::ApiKey => return Ok(LoginMethod::ApiKey),
            _ => {}
        }
    }

    Err(AuthError::OAuth(format!(
        "no compatible auth method for provider '{}' in {} environment",
        provider.id,
        if headless { "headless" } else { "graphical" }
    )))
}

/// Authorization code flow: start callback server, open browser, exchange code.
async fn login_authorization_code(
    provider_id: &str,
    provider: &ProviderAuthConfig,
    oauth_client: &dyn OAuthClient,
) -> AuthResult<LoginResult> {
    let config_fn = provider
        .oauth
        .ok_or_else(|| AuthError::OAuth(format!("provider {provider_id} has no OAuth config")))?;

    // Start callback server
    let server = CallbackServer::bind().await?;
    let redirect_url = server.redirect_url();

    // Build OAuth config with our callback URL
    let oauth_config = config_fn(&redirect_url);

    // Generate authorize URL + PKCE verifier + state
    let auth_method = oauth_client.authorize(&oauth_config).await?;
    let AuthMethod::Code {
        url,
        verifier,
        state: expected_state,
    } = auth_method
    else {
        return Err(AuthError::OAuth(
            "provider returned unexpected auth method".into(),
        ));
    };

    eprintln!("  Opening browser for {provider_id} login...");
    browser::open_url(&url)?;

    // Wait for callback
    eprintln!("  Waiting for browser callback (timeout: 120s)...");
    let callback = server.wait_for_callback(&expected_state, None).await?;

    // Exchange code for token
    eprintln!("  Exchanging authorization code for token...");
    let token = oauth_client
        .exchange_code(&oauth_config, &callback.code, &verifier)
        .await?;

    // Store token
    let store = TokenStore::new();
    store.store_token(provider_id, &token)?;

    eprintln!("  Successfully authenticated with {provider_id}!");

    Ok(LoginResult {
        provider_id: provider_id.to_string(),
        token,
        method: LoginMethod::OAuthCallback,
    })
}

/// Device flow: request device code, show user code, poll for token.
fn login_device_flow(provider_id: &str, _provider: &ProviderAuthConfig) -> AuthResult<LoginResult> {
    // Device flow is handled by provider-specific implementations.
    if provider_id == "copilot" {
        return Err(AuthError::OAuth(
            "use GitHubCopilotAuth::login() for Copilot device flow".into(),
        ));
    }
    Err(AuthError::OAuth(format!(
        "device flow not implemented for provider '{provider_id}'"
    )))
}

/// API key prompt: ask user to provide their API key.
fn login_api_key_prompt(provider_id: &str) -> AuthResult<LoginResult> {
    let provider = providers::find_provider(provider_id)
        .ok_or_else(|| AuthError::OAuth(format!("unknown provider: {provider_id}")))?;

    eprintln!(
        "  Enter your {} API key (or set {}): ",
        provider.display_name, provider.env_key
    );

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| AuthError::OAuth(format!("failed to read input: {e}")))?;

    let key = input.trim().to_string();
    if key.is_empty() {
        return Err(AuthError::OAuth("no API key provided".into()));
    }

    let token = AuthToken {
        access_token: key.into(),
        refresh_token: None,
        expires_at: None,
        token_type: "bearer".to_string(),
    };

    // Store the key
    let store = TokenStore::new();
    store.store_token(provider_id, &token)?;

    eprintln!("  API key stored for {provider_id}.");

    Ok(LoginResult {
        provider_id: provider_id.to_string(),
        token,
        method: LoginMethod::ApiKey,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_auth_method_prefers_device_flow() {
        let provider = providers::find_provider("copilot").unwrap();
        let method = select_auth_method(provider).unwrap();
        assert_eq!(method, LoginMethod::DeviceFlow);
    }

    #[test]
    fn select_auth_method_uses_api_key_for_anthropic() {
        let provider = providers::find_provider("anthropic").unwrap();
        let method = select_auth_method(provider).unwrap();
        assert_eq!(method, LoginMethod::ApiKey);
    }

    #[test]
    fn select_auth_method_unknown_provider_fails() {
        let provider = ProviderAuthConfig {
            id: "test",
            display_name: "Test",
            auth_methods: &[],
            oauth: None,
            env_key: "TEST_KEY",
        };
        let result = select_auth_method(&provider);
        assert!(result.is_err());
    }

    #[test]
    fn login_method_equality() {
        assert_eq!(LoginMethod::ApiKey, LoginMethod::ApiKey);
        assert_ne!(LoginMethod::ApiKey, LoginMethod::DeviceFlow);
        assert_ne!(LoginMethod::OAuthCallback, LoginMethod::DeviceFlow);
    }

    #[tokio::test]
    async fn login_unknown_provider_returns_error() {
        let client = crate::oauth::DefaultOAuthClient::new();
        let result = login("nonexistent", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown provider"));
    }
}
