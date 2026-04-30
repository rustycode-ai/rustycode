use anyhow::Result;

/// Handle /copilot-login slash command
///
/// Initiates GitHub Copilot OAuth device flow login.
/// Displays the user code and verification URL, then polls
/// for authorization and exchanges the token.
///
/// # Returns
/// Result with success message or error
pub async fn handle_copilot_login_command() -> Result<String> {
    use rustycode_auth::GitHubCopilotAuth;

    let auth = GitHubCopilotAuth::new();

    // Step 1: Request device code
    let device = auth
        .request_device_code()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to request device code: {}", e))?;

    let mut msg = format!(
        "\n  GitHub Copilot Device Flow\n  ─────────────────────────\n\n  1. Open: {}\n  2. Enter code: {}\n\n  Waiting for authorization ({}s timeout)...\n",
        device.verification_uri,
        device.user_code,
        device.expires_in,
    );

    // Step 2: Poll for token
    let github_token = auth
        .poll_for_token(&device.device_code, device.interval, device.expires_in)
        .await
        .map_err(|e| anyhow::anyhow!("Authorization failed: {}", e))?;

    msg.push_str("\n  ✓ GitHub authorization received\n");

    // Step 3: Exchange for Copilot token
    let result = auth
        .exchange_for_copilot_token(&github_token)
        .await
        .map_err(|e| anyhow::anyhow!("Token exchange failed: {}", e))?;

    msg.push_str(&format!(
        "\n  ✓ Copilot token obtained (expires at Unix {})\n",
        result.expires_at
    ));

    // Store the token in the keyring
    let token = rustycode_auth::AuthToken {
        access_token: result.copilot_token.clone().into(),
        refresh_token: Some(result.github_token.into()),
        expires_at: Some(result.expires_at as i64),
        token_type: "bearer".to_string(),
    };

    let store = rustycode_auth::TokenStore::new();
    match store.store_token("copilot", &token) {
        Ok(()) => {
            msg.push_str("  ✓ Token stored in system keyring\n");
        }
        Err(e) => {
            msg.push_str(&format!(
                "  ⚠ Could not store in keyring ({}). Token is active for this session.\n",
                e
            ));
        }
    }

    Ok(msg)
}

#[cfg(test)]
mod tests {
    // Device flow tests require user interaction (browser login),
    // so we only test that the module compiles.
}
