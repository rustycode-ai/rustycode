//! Authentication status diagnostics.
//!
//! Reports which credential sources are available for each provider
//! (env var, OS keyring, config file) and whether stored tokens are valid.

use std::fmt::Write;

use crate::providers::{self, PROVIDERS};
use crate::token_store::TokenStore;

/// Authentication status for a single provider.
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub provider_id: String,
    pub display_name: String,
    /// API key found in environment variable.
    pub has_env_key: bool,
    /// Token found in OS keyring.
    pub has_keyring_token: bool,
    /// Token in keyring is valid (not expired). `None` if no token stored.
    pub token_valid: Option<bool>,
    /// Best available credential source.
    pub source: Option<CredentialSource>,
}

/// Where the credential comes from (in priority order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    EnvVar,
    Keyring,
    ConfigFile,
}

impl std::fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnvVar => write!(f, "env var"),
            Self::Keyring => write!(f, "keyring"),
            Self::ConfigFile => write!(f, "config"),
        }
    }
}

/// Check auth status for a single provider.
pub fn provider_status(provider_id: &str) -> Option<ProviderStatus> {
    let provider = providers::find_provider(provider_id)?;

    let has_env_key = std::env::var(provider.env_key).is_ok_and(|v| !v.trim().is_empty());

    let store = TokenStore::new();
    let has_keyring_token = store.token(provider_id).is_ok();
    let token_valid = store.is_token_valid(provider_id).ok();

    let source = if has_env_key {
        Some(CredentialSource::EnvVar)
    } else if token_valid == Some(true) {
        Some(CredentialSource::Keyring)
    } else {
        None
    };

    Some(ProviderStatus {
        provider_id: provider_id.to_string(),
        display_name: provider.display_name.to_string(),
        has_env_key,
        has_keyring_token,
        token_valid,
        source,
    })
}

/// Auth status for all known providers.
pub fn all_providers_status() -> Vec<ProviderStatus> {
    PROVIDERS
        .iter()
        .filter_map(|p| provider_status(p.id))
        .collect()
}

/// Format a status table for display.
pub fn format_status_table(statuses: &[ProviderStatus]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<20} {:<12} {:<10} {:<10}",
        "PROVIDER", "SOURCE", "KEYRING", "VALID"
    );
    out.push_str(&"-".repeat(52));
    out.push('\n');

    for s in statuses {
        let source = s
            .source
            .as_ref()
            .map_or_else(|| "none".to_string(), std::string::ToString::to_string);
        let keyring = if s.has_keyring_token { "yes" } else { "no" };
        let valid = match s.token_valid {
            Some(true) => "yes",
            Some(false) => "expired",
            None => "n/a",
        };
        let _ = writeln!(
            out,
            "{:<20} {:<12} {:<10} {:<10}",
            s.display_name, source, keyring, valid
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_status_returns_known() {
        // Anthropic should always return a status (may or may not have credentials)
        let status = provider_status("anthropic");
        assert!(status.is_some());
        let s = status.unwrap();
        assert_eq!(s.provider_id, "anthropic");
    }

    #[test]
    fn provider_status_returns_none_for_unknown() {
        assert!(provider_status("nonexistent_provider").is_none());
    }

    #[test]
    fn all_providers_status_covers_all() {
        let statuses = all_providers_status();
        assert_eq!(statuses.len(), PROVIDERS.len());
    }

    #[test]
    fn format_table_produces_output() {
        let statuses = all_providers_status();
        let table = format_status_table(&statuses);
        assert!(table.contains("PROVIDER"));
        assert!(table.contains("SOURCE"));
        assert!(table.contains("KEYRING"));
        assert!(table.contains("VALID"));
    }

    #[test]
    fn credential_source_display() {
        assert_eq!(CredentialSource::EnvVar.to_string(), "env var");
        assert_eq!(CredentialSource::Keyring.to_string(), "keyring");
        assert_eq!(CredentialSource::ConfigFile.to_string(), "config");
    }
}
