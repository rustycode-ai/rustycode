//! Error mapping for OpenAI-compatible providers

use crate::provider::ProviderError;
use crate::retry::extract_retry_after_ms;
use reqwest::StatusCode;
use std::time::Duration;

/// Map HTTP error responses to ProviderError.
///
/// This is the standard error mapping used across all OpenAI-compatible
/// providers. Only the provider name and environment variable name vary.
///
pub fn map_http_error(
    status: StatusCode,
    text: String,
    headers: &reqwest::header::HeaderMap,
    provider_name: &str,
    env_var_name: &str,
) -> ProviderError {
    match status.as_u16() {
        401 | 403 => ProviderError::auth(format!(
            "Authentication failed for {}. Check your API key (set via {} environment variable). Response: {}",
            provider_name, env_var_name, text
        )),
        404 => ProviderError::InvalidModel(format!(
            "Model or deployment not found for {}. Response: {}",
            provider_name, text
        )),
        429 => ProviderError::RateLimited {
            retry_delay: extract_retry_after_ms(headers).map(Duration::from_millis),
        },
        502..=504 => ProviderError::Network(format!(
            "{} service temporarily unavailable (HTTP {}). Response: {}",
            provider_name, status, text
        )),
        _ => ProviderError::api(format!(
            "{} API error (HTTP {}): {}",
            provider_name, status, text
        )),
    }
}

/// Build an HTTP request with standard OpenAI-compatible headers.
///
pub fn build_request_with_auth(
    builder: reqwest::RequestBuilder,
    api_key: &str,
    extra_headers: Option<&std::collections::HashMap<String, String>>,
) -> reqwest::RequestBuilder {
    let mut builder = builder
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json");

    if let Some(headers) = extra_headers {
        for (key, value) in headers {
            builder = builder.header(key, value);
        }
    }

    builder
}
