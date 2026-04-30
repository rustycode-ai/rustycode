use crate::error_recovery::{classify_error, ErrorKind};
use crate::provider::{sanitize_error_message, ProviderType};

#[derive(Debug, Clone, Copy)]
pub struct RetryPlan {
    pub max_attempts: u32,
}

fn is_overloaded(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("529") || m.contains("overloaded")
}

pub fn retry_plan_for_error(provider: ProviderType, err: &anyhow::Error) -> RetryPlan {
    let msg = err.to_string();
    if is_overloaded(&msg) {
        return RetryPlan { max_attempts: 2 };
    }

    let kind = classify_error(err);
    match provider {
        ProviderType::Anthropic => match kind {
            ErrorKind::RateLimit => RetryPlan { max_attempts: 3 },
            ErrorKind::NetworkError => RetryPlan { max_attempts: 3 },
            _ => RetryPlan { max_attempts: 1 },
        },
        ProviderType::OpenAI | ProviderType::Azure => match kind {
            ErrorKind::RateLimit => RetryPlan { max_attempts: 4 },
            ErrorKind::NetworkError => RetryPlan { max_attempts: 3 },
            _ => RetryPlan { max_attempts: 1 },
        },
        ProviderType::Bedrock => match kind {
            ErrorKind::RateLimit => RetryPlan { max_attempts: 5 }, // AWS rate limits are more aggressive
            ErrorKind::NetworkError => RetryPlan { max_attempts: 3 },
            _ => RetryPlan { max_attempts: 1 },
        },
        ProviderType::Cohere => match kind {
            ErrorKind::RateLimit => RetryPlan { max_attempts: 3 },
            ErrorKind::NetworkError => RetryPlan { max_attempts: 3 },
            _ => RetryPlan { max_attempts: 1 },
        },
        ProviderType::Gemini => match kind {
            ErrorKind::RateLimit => RetryPlan { max_attempts: 3 },
            ErrorKind::NetworkError => RetryPlan { max_attempts: 2 },
            _ => RetryPlan { max_attempts: 1 },
        },
        _ => match kind {
            ErrorKind::RateLimit => RetryPlan { max_attempts: 3 },
            ErrorKind::NetworkError => RetryPlan { max_attempts: 2 },
            _ => RetryPlan { max_attempts: 1 },
        },
    }
}

pub fn user_facing_error_for(provider: ProviderType, err: &anyhow::Error) -> String {
    let raw = err.to_string();
    let raw_lc = raw.to_lowercase();

    if is_overloaded(&raw) {
        return "API is overloaded — try again shortly".to_string();
    }

    match classify_error(err) {
        ErrorKind::RateLimit => match provider {
            ProviderType::Bedrock => {
                "AWS Bedrock rate limit exceeded — please wait and retry".to_string()
            }
            ProviderType::Azure => {
                "Azure OpenAI rate limit exceeded — please wait and retry".to_string()
            }
            ProviderType::Cohere => {
                "Cohere API rate limit exceeded — please wait and retry".to_string()
            }
            ProviderType::Gemini => {
                "Google Gemini rate limit exceeded — please wait and retry".to_string()
            }
            _ => "Rate limited by API — please wait a moment and try again".to_string(),
        },
        ErrorKind::AuthError => match provider {
            ProviderType::Anthropic => {
                "Authentication failed — check your API key in ~/.codex/rustycode/config.toml"
                    .to_string()
            }
            ProviderType::Azure => {
                "Azure OpenAI authentication failed — verify API key and endpoint".to_string()
            }
            ProviderType::Bedrock => {
                "AWS Bedrock authentication failed — check AWS credentials".to_string()
            }
            ProviderType::Cohere => {
                "Cohere authentication failed — verify your API key".to_string()
            }
            ProviderType::Gemini => {
                "Google Gemini authentication failed — verify your API key".to_string()
            }
            _ => {
                "Authentication failed — verify provider credentials and configuration".to_string()
            }
        },
        ErrorKind::NetworkError => "Network error — check your internet connection".to_string(),
        ErrorKind::ContextTooLong => {
            "Conversation too long — use /clear to start fresh".to_string()
        }
        ErrorKind::InvalidRequest => {
            "Provider rejected the request as invalid — verify model and request parameters"
                .to_string()
        }
        _ => {
            if raw_lc.contains("retry-after") {
                "Provider requested backoff — retry shortly".to_string()
            } else {
                let sanitized = sanitize_error_message(&raw);
                if let Some(http_status) = extract_http_status(&sanitized) {
                    format!(
                        "AI error: upstream request failed with HTTP {}",
                        http_status
                    )
                } else {
                    "AI error: provider request failed".to_string()
                }
            }
        }
    }
}

fn extract_http_status(message: &str) -> Option<u16> {
    message
        .split(|ch: char| !ch.is_ascii_digit())
        .find_map(|part| match part.parse::<u16>() {
            Ok(status) if (100..=599).contains(&status) => Some(status),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_and_openai_rate_limits_differ() {
        let err = anyhow::anyhow!("HTTP 429: too many requests");
        assert_eq!(
            retry_plan_for_error(ProviderType::Anthropic, &err).max_attempts,
            3
        );
        assert_eq!(
            retry_plan_for_error(ProviderType::OpenAI, &err).max_attempts,
            4
        );
    }

    #[test]
    fn overload_uses_two_attempts() {
        let err = anyhow::anyhow!("Anthropic API error 529: overloaded");
        assert_eq!(
            retry_plan_for_error(ProviderType::Anthropic, &err).max_attempts,
            2
        );
    }

    #[test]
    fn anthropic_auth_message_is_specific() {
        let err = anyhow::anyhow!("HTTP 401 unauthorized");
        let msg = user_facing_error_for(ProviderType::Anthropic, &err);
        assert!(msg.contains("~/.codex/rustycode/config.toml"));
    }

    #[test]
    fn unknown_errors_do_not_echo_secret_material() {
        let err = anyhow::anyhow!("boom bearer sk-secret https://api.example.com?key=abc123");
        let msg = user_facing_error_for(ProviderType::OpenAI, &err);
        assert!(!msg.contains("sk-secret"));
        assert!(!msg.contains("abc123"));
        assert_eq!(msg, "AI error: provider request failed");
    }

    #[test]
    fn invalid_request_errors_are_sanitized() {
        let err = anyhow::anyhow!("HTTP 400 bad request with provider body");
        let msg = user_facing_error_for(ProviderType::Gemini, &err);
        assert!(msg.contains("Provider rejected the request as invalid"));
    }
}
