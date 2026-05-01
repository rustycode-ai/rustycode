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
