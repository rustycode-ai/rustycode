//! Error mapping for OpenAI-compatible providers.
//!
//! Handles structured error JSON from the OpenAI API spec and maps HTTP status
//! codes to typed [`ProviderError`] variants. Also provides a streaming error
//! mapper for SSE error frames.

use crate::provider::ProviderError;
use crate::retry::extract_retry_after_ms;
use reqwest::StatusCode;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Structured error body (OpenAI spec)
// ---------------------------------------------------------------------------

/// Top-level envelope returned by OpenAI-compatible APIs on error.
#[derive(serde::Deserialize, Debug)]
struct OpenAiErrorBody {
    error: OpenAiErrorDetail,
}

/// Inner error detail within the OpenAI error envelope.
#[derive(serde::Deserialize, Debug)]
struct OpenAiErrorDetail {
    #[serde(default)]
    message: String,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    param: Option<String>,
}

/// Result of attempting to parse the structured JSON error body.
struct ParsedError {
    message: String,
    error_type: Option<String>,
    error_code: Option<String>,
    param: Option<String>,
}

impl ParsedError {
    /// Try to parse structured JSON; fall back to raw text on failure.
    fn from_body(text: &str) -> Self {
        match serde_json::from_str::<OpenAiErrorBody>(text) {
            Ok(body) => Self {
                message: body.error.message,
                error_type: if body.error.r#type.is_empty() {
                    None
                } else {
                    Some(body.error.r#type)
                },
                error_code: body.error.code.filter(|c| !c.is_empty()),
                param: body.error.param.filter(|p| !p.is_empty()),
            },
            Err(_) => Self {
                message: text.to_owned(),
                error_type: None,
                error_code: None,
                param: None,
            },
        }
    }

    /// Format a human-readable message including provider name and structured
    /// type/code when available.
    fn format_message(&self, provider_name: &str, status: StatusCode) -> String {
        match (&self.error_type, &self.error_code) {
            (Some(t), Some(c)) => {
                format!("{} error ({}/{}): {}", provider_name, t, c, self.message)
            }
            (Some(t), None) => {
                format!("{} error ({}): {}", provider_name, t, self.message)
            }
            (None, Some(c)) => {
                format!("{} error ({}): {}", provider_name, c, self.message)
            }
            (None, None) => {
                format!(
                    "{} API error (HTTP {}): {}",
                    provider_name,
                    status.as_u16(),
                    self.message
                )
            }
        }
    }

    /// Append param info to a message if the server reported a specific
    /// parameter that caused the error.
    fn with_param(&self, base: String) -> String {
        match &self.param {
            Some(p) => format!("{} (param: {})", base, p),
            None => base,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers shared between HTTP and stream error mapping
// ---------------------------------------------------------------------------

/// Check for code-based overrides that apply regardless of HTTP status.
fn apply_code_overrides(
    parsed: &ParsedError,
    provider_name: &str,
    base: ProviderError,
) -> ProviderError {
    if let Some(code) = &parsed.error_code {
        match code.as_str() {
            "context_length_exceeded" => {
                return ProviderError::ContextLengthExceeded(
                    parsed.format_message(provider_name, StatusCode::BAD_REQUEST),
                );
            }
            "insufficient_quota" => {
                return ProviderError::CreditsExhausted {
                    details: parsed.format_message(provider_name, StatusCode::PAYMENT_REQUIRED),
                    top_up_url: Some("https://platform.openai.com/account/billing".to_owned()),
                };
            }
            _ => {}
        }
    }
    base
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Map HTTP error responses to [`ProviderError`].
///
/// This is the standard error mapping used across all OpenAI-compatible
/// providers. Only the provider name and environment variable name vary.
///
/// Parses OpenAI's structured error JSON when available and falls back to raw
/// text otherwise.
pub fn map_http_error(
    status: StatusCode,
    text: String,
    headers: &reqwest::header::HeaderMap,
    provider_name: &str,
    _env_var_name: &str,
) -> ProviderError {
    let parsed = ParsedError::from_body(&text);
    let msg = parsed.format_message(provider_name, status);

    let base_error = match status.as_u16() {
        // 400 — Bad request / invalid_request_error
        400 => {
            let detail = parsed.with_param(msg);
            ProviderError::Api(detail)
        }

        // 401 — Authentication error
        401 => ProviderError::Auth(msg),

        // 402 — Payment required / insufficient_quota
        402 => ProviderError::CreditsExhausted {
            details: msg,
            top_up_url: Some("https://platform.openai.com/account/billing".to_owned()),
        },

        // 403 — Permission error
        403 => ProviderError::Auth(msg),

        // 404 — Not found / model_not_found
        404 => ProviderError::InvalidModel(msg),

        // 409 — Conflict
        409 => ProviderError::Api(msg),

        // 422 — Unprocessable entity
        422 => ProviderError::Api(msg),

        // 429 — Rate limited
        429 => ProviderError::RateLimited {
            retry_delay: extract_retry_after_ms(headers).map(Duration::from_millis),
        },

        // 500 — Internal server error (retryable)
        500 => ProviderError::Network(msg),

        // 502-504 — Gateway errors (retryable)
        502..=504 => ProviderError::Network(msg),

        // Everything else → generic API error
        _ => ProviderError::Api(msg),
    };

    apply_code_overrides(&parsed, provider_name, base_error)
}

/// Map a streaming error JSON frame to [`ProviderError`].
///
/// OpenAI-compatible providers send errors during streaming as JSON frames
/// matching the same `{ "error": { ... } }` structure as regular responses.
pub fn map_stream_error(error_json: &serde_json::Value, provider_name: &str) -> ProviderError {
    // Extract the inner "error" object if present; otherwise use the value
    // directly.
    let inner = error_json.get("error").unwrap_or(error_json);

    let message = inner
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown streaming error")
        .to_owned();

    let error_code = inner
        .get("code")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());

    let error_type = inner
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());

    let parsed = ParsedError {
        message,
        error_type,
        error_code,
        param: None,
    };

    // Default: Network error (streaming errors are generally transient).
    let base_error = ProviderError::Network(
        parsed.format_message(provider_name, StatusCode::INTERNAL_SERVER_ERROR),
    );

    apply_code_overrides(&parsed, provider_name, base_error)
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create an empty `HeaderMap`.
    fn empty_headers() -> reqwest::header::HeaderMap {
        reqwest::header::HeaderMap::new()
    }

    /// Helper to create headers with a `retry-after` value (in seconds).
    fn headers_with_retry_after(secs: u64) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from(secs),
        );
        h
    }

    // ----- HTTP status mapping tests -----

    #[test]
    fn status_400_structured_invalid_request() {
        let body = r#"{"error":{"type":"invalid_request_error","message":"bad req","param":"temperature","code":null}}"#;
        let err = map_http_error(
            StatusCode::BAD_REQUEST,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::Api(msg) => {
                assert!(msg.contains("invalid_request_error"));
                assert!(msg.contains("bad req"));
                assert!(msg.contains("param: temperature"));
            }
            other => panic!("expected Api, got {:?}", other),
        }
    }

    #[test]
    fn status_401_authentication_error() {
        let body =
            r#"{"error":{"type":"authentication_error","message":"Invalid API key","code":null}}"#;
        let err = map_http_error(
            StatusCode::UNAUTHORIZED,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::Auth(msg) => {
                assert!(msg.contains("authentication_error"));
                assert!(msg.contains("Invalid API key"));
            }
            other => panic!("expected Auth, got {:?}", other),
        }
    }

    #[test]
    fn status_402_insufficient_quota() {
        let body = r#"{"error":{"type":"insufficient_quota","message":"You exceeded your quota","code":"insufficient_quota"}}"#;
        let err = map_http_error(
            StatusCode::PAYMENT_REQUIRED,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match &err {
            ProviderError::CreditsExhausted {
                details,
                top_up_url,
            } => {
                assert!(details.contains("insufficient_quota"));
                assert!(details.contains("You exceeded your quota"));
                assert_eq!(
                    top_up_url,
                    &Some("https://platform.openai.com/account/billing".to_owned())
                );
            }
            other => panic!("expected CreditsExhausted, got {:?}", other),
        }
        assert!(err.is_credits_exhausted());
    }

    #[test]
    fn status_403_permission_error() {
        let body = r#"{"error":{"type":"permission_error","message":"Access denied","code":null}}"#;
        let err = map_http_error(
            StatusCode::FORBIDDEN,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::Auth(msg) => {
                assert!(msg.contains("permission_error"));
            }
            other => panic!("expected Auth, got {:?}", other),
        }
    }

    #[test]
    fn status_404_model_not_found() {
        let body = r#"{"error":{"type":"not_found_error","message":"Model not found","code":"model_not_found"}}"#;
        let err = map_http_error(
            StatusCode::NOT_FOUND,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::InvalidModel(msg) => {
                assert!(msg.contains("model_not_found"));
                assert!(msg.contains("Model not found"));
            }
            other => panic!("expected InvalidModel, got {:?}", other),
        }
    }

    #[test]
    fn status_429_rate_limited_with_retry_after() {
        let body = r#"{"error":{"type":"rate_limit_error","message":"Slow down","code":"rate_limit_exceeded"}}"#;
        let err = map_http_error(
            StatusCode::TOO_MANY_REQUESTS,
            body.to_owned(),
            &headers_with_retry_after(5),
            "TestProvider",
            "TEST_API_KEY",
        );
        match &err {
            ProviderError::RateLimited { retry_delay } => {
                assert_eq!(retry_delay, &Some(Duration::from_secs(5)));
            }
            other => panic!("expected RateLimited, got {:?}", other),
        }
        assert!(err.is_rate_limited());
    }

    #[test]
    fn status_429_rate_limited_without_header() {
        let body = r#"{"error":{"type":"rate_limit_error","message":"Slow down","code":null}}"#;
        let err = map_http_error(
            StatusCode::TOO_MANY_REQUESTS,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::RateLimited { retry_delay } => {
                assert!(retry_delay.is_none());
            }
            other => panic!("expected RateLimited, got {:?}", other),
        }
    }

    #[test]
    fn status_500_server_error_is_retryable() {
        let body = r#"{"error":{"type":"server_error","message":"Internal failure","code":"server_error"}}"#;
        let err = map_http_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match &err {
            ProviderError::Network(msg) => {
                assert!(msg.contains("server_error"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
        assert!(err.is_retryable());
    }

    #[test]
    fn status_502_bad_gateway_is_retryable() {
        let err = map_http_error(
            StatusCode::BAD_GATEWAY,
            "gateway timeout".to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match &err {
            ProviderError::Network(msg) => {
                assert!(msg.contains("gateway timeout"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
        assert!(err.is_retryable());
    }

    #[test]
    fn status_503_engine_overloaded_is_retryable() {
        let body = r#"{"error":{"type":"server_error","message":"Engine overloaded","code":"engine_overloaded"}}"#;
        let err = map_http_error(
            StatusCode::SERVICE_UNAVAILABLE,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match &err {
            ProviderError::Network(msg) => {
                assert!(msg.contains("Engine overloaded"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
        assert!(err.is_retryable());
    }

    // ----- Code-based override tests -----

    #[test]
    fn context_length_exceeded_overrides_any_status() {
        // Send on a 400 — should map to ContextLengthExceeded
        let body = r#"{"error":{"type":"invalid_request_error","message":"too many tokens","code":"context_length_exceeded"}}"#;
        let err = map_http_error(
            StatusCode::BAD_REQUEST,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match &err {
            ProviderError::ContextLengthExceeded(msg) => {
                assert!(msg.contains("context_length_exceeded"));
                assert!(msg.contains("too many tokens"));
            }
            other => panic!("expected ContextLengthExceeded, got {:?}", other),
        }
        assert!(err.is_context_exceeded());
    }

    #[test]
    fn context_length_exceeded_on_413() {
        // Even on an unexpected status code, the code override wins
        let body = r#"{"error":{"type":"invalid_request_error","message":"Token limit","code":"context_length_exceeded"}}"#;
        let err = map_http_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        assert!(err.is_context_exceeded());
    }

    #[test]
    fn insufficient_quota_code_overrides_any_status() {
        // 403 with insufficient_quota code → CreditsExhausted override
        let body = r#"{"error":{"type":"permission_error","message":"Billing limit","code":"insufficient_quota"}}"#;
        let err = map_http_error(
            StatusCode::FORBIDDEN,
            body.to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::CreditsExhausted { top_up_url, .. } => {
                assert_eq!(
                    top_up_url,
                    Some("https://platform.openai.com/account/billing".to_owned())
                );
            }
            other => panic!("expected CreditsExhausted, got {:?}", other),
        }
    }

    // ----- Fallback / edge-case tests -----

    #[test]
    fn malformed_json_falls_back_to_raw_text() {
        let body = "this is not json at all".to_owned();
        let err = map_http_error(
            StatusCode::BAD_REQUEST,
            body,
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::Api(msg) => {
                assert!(msg.contains("this is not json at all"));
                assert!(msg.contains("HTTP 400"));
            }
            other => panic!("expected Api, got {:?}", other),
        }
    }

    #[test]
    fn empty_body_produces_valid_error() {
        let err = map_http_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            String::new(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::Network(msg) => {
                assert!(msg.contains("TestProvider"));
                assert!(msg.contains("HTTP 500"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
    }

    #[test]
    fn unknown_status_falls_back_to_api_error() {
        let err = map_http_error(
            StatusCode::from_u16(418).unwrap(), // I'm a teapot
            "no coffee".to_owned(),
            &empty_headers(),
            "TestProvider",
            "TEST_API_KEY",
        );
        match err {
            ProviderError::Api(msg) => {
                assert!(msg.contains("no coffee"));
                assert!(msg.contains("HTTP 418"));
            }
            other => panic!("expected Api, got {:?}", other),
        }
    }

    // ----- Streaming error tests -----

    #[test]
    fn stream_error_parses_structured_json() {
        let json = serde_json::json!({
            "error": {
                "code": "context_length_exceeded",
                "message": "Too many tokens in stream"
            }
        });
        let err = map_stream_error(&json, "TestProvider");
        match err {
            ProviderError::ContextLengthExceeded(msg) => {
                assert!(msg.contains("Too many tokens in stream"));
            }
            other => panic!("expected ContextLengthExceeded, got {:?}", other),
        }
    }

    #[test]
    fn stream_error_insufficient_quota() {
        let json = serde_json::json!({
            "error": {
                "code": "insufficient_quota",
                "message": "Billing hard limit reached"
            }
        });
        let err = map_stream_error(&json, "TestProvider");
        match err {
            ProviderError::CreditsExhausted {
                details,
                top_up_url,
            } => {
                assert!(details.contains("Billing hard limit reached"));
                assert!(top_up_url.is_some());
            }
            other => panic!("expected CreditsExhausted, got {:?}", other),
        }
    }

    #[test]
    fn stream_error_defaults_to_network() {
        let json = serde_json::json!({
            "error": {
                "message": "Something went wrong"
            }
        });
        let err = map_stream_error(&json, "TestProvider");
        match &err {
            ProviderError::Network(msg) => {
                assert!(msg.contains("Something went wrong"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
        assert!(err.is_retryable());
    }

    #[test]
    fn stream_error_without_error_wrapper() {
        // Some providers may send the error fields at the top level
        let json = serde_json::json!({
            "code": "rate_limit_exceeded",
            "message": "Slow down"
        });
        let err = map_stream_error(&json, "TestProvider");
        // No code override for rate_limit_exceeded, so defaults to Network
        match &err {
            ProviderError::Network(msg) => {
                assert!(msg.contains("Slow down"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
    }
}
