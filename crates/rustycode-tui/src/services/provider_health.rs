//! Provider health check module

use reqwest::Client;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// Health status for a provider
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderHealth {
    /// Provider is online and responsive
    Online { latency_ms: u64 },
    /// Provider is offline or unreachable
    Offline,
    /// Provider configuration is invalid or missing
    Unconfigured,
    /// Health check is in progress
    Checking,
}

/// Health check result for a provider
#[derive(Debug, Clone)]
pub struct ProviderHealthResult {
    pub provider_type: String,
    pub provider_name: String,
    pub health: ProviderHealth,
    pub last_check: Option<Instant>,
}

impl ProviderHealthResult {
    pub fn new(provider_type: String, provider_name: String) -> Self {
        Self {
            provider_type,
            provider_name,
            health: ProviderHealth::Checking,
            last_check: None,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self.health, ProviderHealth::Online { .. })
    }

    /// Get latency in milliseconds if available
    pub fn latency_ms(&self) -> Option<u64> {
        match self.health {
            ProviderHealth::Online { latency_ms } => Some(latency_ms),
            _ => None,
        }
    }
}

/// Perform health check for a specific provider type
pub async fn check_provider_health(provider_type: &str, api_key: Option<String>) -> ProviderHealth {
    match provider_type {
        "anthropic" => check_anthropic_health(api_key).await,
        "openai" => check_openai_health(api_key).await,
        "openrouter" => check_openrouter_health(api_key).await,
        "ollama" => check_ollama_health().await,
        "gemini" => check_gemini_health(api_key).await,
        "copilot" => check_copilot_health(api_key).await,
        "custom" => check_custom_health(api_key).await,
        _ => ProviderHealth::Unconfigured,
    }
}

/// Check Anthropic API health
async fn check_anthropic_health(api_key: Option<String>) -> ProviderHealth {
    let api_key = match api_key {
        Some(key) => key,
        None => return ProviderHealth::Unconfigured,
    };

    let client = Client::new();
    let start = Instant::now();

    match timeout(
        Duration::from_secs(10),
        client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "ping"}]
            }))
            .send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let latency = start.elapsed().as_millis() as u64;
            if response.status().is_success() {
                ProviderHealth::Online {
                    latency_ms: latency,
                }
            } else {
                ProviderHealth::Offline
            }
        }
        Ok(Err(_)) => ProviderHealth::Offline,
        Err(_) => ProviderHealth::Offline, // Timeout
    }
}

/// Check OpenAI API health
async fn check_openai_health(api_key: Option<String>) -> ProviderHealth {
    let api_key = match api_key {
        Some(key) => key,
        None => return ProviderHealth::Unconfigured,
    };

    let client = Client::new();
    let start = Instant::now();

    match timeout(
        Duration::from_secs(10),
        client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": "gpt-3.5-turbo",
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1
            }))
            .send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let latency = start.elapsed().as_millis() as u64;
            if response.status().is_success() {
                ProviderHealth::Online {
                    latency_ms: latency,
                }
            } else {
                ProviderHealth::Offline
            }
        }
        Ok(Err(_)) => ProviderHealth::Offline,
        Err(_) => ProviderHealth::Offline,
    }
}

/// Check OpenRouter API health
async fn check_openrouter_health(api_key: Option<String>) -> ProviderHealth {
    let api_key = match api_key {
        Some(key) => key,
        None => return ProviderHealth::Unconfigured,
    };

    let client = Client::new();
    let start = Instant::now();

    match timeout(
        Duration::from_secs(10),
        client
            .get("https://openrouter.ai/api/v1/models")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("HTTP-Referer", "https://rustycode.dev")
            .header("X-Title", "RustyCode")
            .send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let latency = start.elapsed().as_millis() as u64;
            if response.status().is_success() {
                ProviderHealth::Online {
                    latency_ms: latency,
                }
            } else {
                ProviderHealth::Offline
            }
        }
        Ok(Err(_)) => ProviderHealth::Offline,
        Err(_) => ProviderHealth::Offline,
    }
}

/// Check Ollama local server health
async fn check_ollama_health() -> ProviderHealth {
    let client = Client::new();
    let start = Instant::now();

    match timeout(
        Duration::from_secs(5),
        client.get("http://localhost:11434/api/tags").send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let latency = start.elapsed().as_millis() as u64;
            if response.status().is_success() {
                ProviderHealth::Online {
                    latency_ms: latency,
                }
            } else {
                ProviderHealth::Offline
            }
        }
        Ok(Err(_)) => ProviderHealth::Offline,
        Err(_) => ProviderHealth::Offline,
    }
}

/// Check Google Gemini API health
async fn check_gemini_health(api_key: Option<String>) -> ProviderHealth {
    let api_key = match api_key {
        Some(key) => key,
        None => return ProviderHealth::Unconfigured,
    };

    let client = Client::new();
    let start = Instant::now();

    match timeout(
        Duration::from_secs(10),
        client
            .get(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent?key={}",
                api_key
            ))
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "contents": [{"parts": [{"text": "ping"}]}]
            }))
            .send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let latency = start.elapsed().as_millis() as u64;
            if response.status().is_success() {
                ProviderHealth::Online { latency_ms: latency }
            } else {
                ProviderHealth::Offline
            }
        }
        Ok(Err(_)) => ProviderHealth::Offline,
        Err(_) => ProviderHealth::Offline,
    }
}

/// Check GitHub Copilot API health
async fn check_copilot_health(api_key: Option<String>) -> ProviderHealth {
    let api_key = match api_key {
        Some(key) => key,
        None => return ProviderHealth::Unconfigured,
    };

    let client = Client::new();
    let start = Instant::now();

    match timeout(
        Duration::from_secs(10),
        client
            .get("https://api.github.com/copilot/rest/v1/telemetry/counters")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Accept", "application/json")
            .send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let latency = start.elapsed().as_millis() as u64;
            // GitHub API returns 200 for valid tokens, 401/403 for invalid
            if response.status().is_success()
                || response.status() == 401
                || response.status() == 403
            {
                ProviderHealth::Online {
                    latency_ms: latency,
                }
            } else {
                ProviderHealth::Offline
            }
        }
        Ok(Err(_)) => ProviderHealth::Offline,
        Err(_) => ProviderHealth::Offline,
    }
}

/// Check custom OpenAI-compatible API health
async fn check_custom_health(api_key: Option<String>) -> ProviderHealth {
    // For custom providers, we need the base URL from config
    // This is a simplified check - in practice, you'd need the full endpoint
    let _api_key = match api_key {
        Some(key) => key,
        None => return ProviderHealth::Unconfigured,
    };

    // Custom provider check would need configuration for base URL
    // For now, return Unconfigured as we don't have the endpoint
    ProviderHealth::Unconfigured
}

/// Batch check health for multiple providers
pub async fn check_multiple_providers(
    providers: Vec<(String, String, Option<String>)>,
) -> Vec<ProviderHealthResult> {
    let mut results = Vec::new();

    for (provider_type, provider_name, api_key) in providers {
        let health = check_provider_health(&provider_type, api_key).await;
        let mut result = ProviderHealthResult::new(provider_type, provider_name);
        result.health = health;
        result.last_check = Some(Instant::now());
        results.push(result);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_status_display() {
        let online = ProviderHealth::Online { latency_ms: 150 };
        let offline = ProviderHealth::Offline;
        let unconfigured = ProviderHealth::Unconfigured;

        assert!(matches!(online, ProviderHealth::Online { .. }));
        assert!(matches!(offline, ProviderHealth::Offline));
        assert!(matches!(unconfigured, ProviderHealth::Unconfigured));
    }

    #[test]
    fn test_provider_health_result() {
        let result = ProviderHealthResult::new("test".to_string(), "Test Provider".to_string());
        assert_eq!(result.provider_type, "test");
        assert_eq!(result.provider_name, "Test Provider");
        assert!(!result.is_available());
        assert_eq!(result.latency_ms(), None);
    }
}
