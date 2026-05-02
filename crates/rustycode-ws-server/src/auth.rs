use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuthConfig {
    pub api_key: Option<String>,
}

impl AuthConfig {
    pub fn is_enabled(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }
}

#[derive(Clone)]
pub struct AuthState {
    pub config: AuthConfig,
}

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

pub fn extract_api_key(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(std::string::ToString::to_string)
        .or_else(|| query_token.map(std::string::ToString::to_string))
}

pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    request: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    if !auth_state.config.is_enabled() {
        return Ok(next.run(request).await);
    }

    let provided = extract_api_key(request.headers(), None);
    let expected = auth_state.config.api_key.as_deref().unwrap_or("");

    if provided.as_deref() == Some(expected) {
        Ok(next.run(request).await)
    } else {
        Err((StatusCode::UNAUTHORIZED, "unauthorized"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn auth_config_disabled_when_no_key() {
        let config = AuthConfig::default();
        assert!(!config.is_enabled());
    }

    #[test]
    fn auth_config_disabled_when_empty_key() {
        let config = AuthConfig {
            api_key: Some(String::new()),
        };
        assert!(!config.is_enabled());
    }

    #[test]
    fn auth_config_enabled_when_key_set() {
        let config = AuthConfig {
            api_key: Some("secret".to_string()),
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn extract_api_key_from_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer my-token".parse().unwrap());
        let result = extract_api_key(&headers, None);
        assert_eq!(result, Some("my-token".to_string()));
    }

    #[test]
    fn extract_api_key_from_query_token() {
        let headers = HeaderMap::new();
        let result = extract_api_key(&headers, Some("query-token"));
        assert_eq!(result, Some("query-token".to_string()));
    }

    #[test]
    fn extract_api_key_prefers_header_over_query() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer header-token".parse().unwrap());
        let result = extract_api_key(&headers, Some("query-token"));
        assert_eq!(result, Some("header-token".to_string()));
    }

    #[test]
    fn extract_api_key_returns_none_when_absent() {
        let headers = HeaderMap::new();
        let result = extract_api_key(&headers, None);
        assert!(result.is_none());
    }

    #[test]
    fn extract_api_key_ignores_malformed_header() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Basic abc123".parse().unwrap());
        let result = extract_api_key(&headers, None);
        assert!(result.is_none());
    }

    #[test]
    fn extract_api_key_handles_empty_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer ".parse().unwrap());
        let result = extract_api_key(&headers, None);
        assert_eq!(result, Some(String::new()));
    }
}
