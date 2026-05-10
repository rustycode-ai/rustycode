//! Shared async HTTP client for web tools.
//!
//! Uses async `reqwest::Client` so HTTP calls cooperate with the tokio runtime
//! instead of blocking worker threads. Tools bridge to the synchronous trait
//! interface via `block_on_shared()`.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::Duration;

const USER_AGENT: &str = concat!("RustyCode/", env!("CARGO_PKG_VERSION"));

pub fn build_client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
        .context("failed to build HTTP client")
}

/// Fetch a URL asynchronously, returning status, headers, and body text.
pub async fn web_fetch_async(
    url: &str,
    timeout_secs: u64,
) -> Result<(u16, HashMap<String, String>, String)> {
    let client = build_client(Duration::from_secs(timeout_secs))?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("HTTP request failed for {url}"))?;

    let status_code = response.status().as_u16();

    let headers_map = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect();

    let body = response
        .text()
        .await
        .context("failed to read response body")?;

    Ok((status_code, headers_map, body))
}

/// GET request returning parsed JSON.
pub async fn http_get_json<T: serde::de::DeserializeOwned>(
    url: &str,
    timeout_secs: u64,
) -> Result<(u16, T)> {
    let client = build_client(Duration::from_secs(timeout_secs))?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("HTTP request failed for {url}"))?;
    let status = response.status().as_u16();
    let body: T = response
        .json()
        .await
        .with_context(|| format!("failed to parse JSON from {url}"))?;
    Ok((status, body))
}

/// POST request with JSON body returning parsed JSON.
pub async fn http_post_json<B: serde::Serialize + Sync, T: serde::de::DeserializeOwned>(
    url: &str,
    body: &B,
    timeout_secs: u64,
) -> Result<(u16, T)> {
    let client = build_client(Duration::from_secs(timeout_secs))?;
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("HTTP POST failed for {url}"))?;
    let status = response.status().as_u16();
    let result: T = response
        .json()
        .await
        .with_context(|| format!("failed to parse JSON response from {url}"))?;
    Ok((status, result))
}
