/// Provider error types and error sanitization.
///
/// Sanitize an error message by redacting sensitive information.
///
/// Removes API keys, bearer tokens, and other credentials from error messages
/// before logging or displaying them to users.
pub fn sanitize_error_message(message: &str) -> String {
    static QUERY_SECRET_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static BEARER_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static API_KEY_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

    let query_secret_re = QUERY_SECRET_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)([?&](?:key|api[-_]?key|token|access_token)=)[^&\s]+")
            .expect("valid regex")
    });
    let bearer_re = BEARER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)(bearer\s+)[A-Za-z0-9._~-]+").expect("valid regex")
    });
    let api_key_re = API_KEY_RE
        .get_or_init(|| regex::Regex::new(r"(?i)(x-api-key[:=]\s*)[^\s,;]+").expect("valid regex"));

    let redacted = query_secret_re.replace_all(message, "$1[REDACTED]");
    let redacted = bearer_re.replace_all(&redacted, "$1[REDACTED]");
    api_key_re
        .replace_all(&redacted, "$1[REDACTED]")
        .into_owned()
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Rate limited. Please wait before retrying.")]
    RateLimited {
        retry_delay: Option<std::time::Duration>,
    },
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),
    #[error("Credits exhausted: {details}")]
    CreditsExhausted {
        details: String,
        top_up_url: Option<String>,
    },
    #[error("Invalid model: {0}")]
    InvalidModel(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Timeout error: {0}")]
    Timeout(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl ProviderError {
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Auth(msg.into())
    }

    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    pub fn api(msg: impl Into<String>) -> Self {
        Self::Api(msg.into())
    }

    pub fn with_model(self, model: &str) -> Self {
        let prefix = format!("[model: {model}] ");
        match self {
            Self::Auth(s) => Self::Auth(prefix + &s),
            Self::Network(s) => Self::Network(prefix + &s),
            Self::Api(s) => Self::Api(prefix + &s),
            Self::RateLimited { retry_delay } => Self::RateLimited { retry_delay },
            Self::ContextLengthExceeded(s) => Self::ContextLengthExceeded(prefix + &s),
            Self::CreditsExhausted {
                details,
                top_up_url,
            } => Self::CreditsExhausted {
                details: prefix + &details,
                top_up_url,
            },
            Self::InvalidModel(s) => Self::InvalidModel(prefix + &s),
            Self::Serialization(s) => Self::Serialization(prefix + &s),
            Self::Timeout(s) => Self::Timeout(prefix + &s),
            Self::Configuration(s) => Self::Configuration(prefix + &s),
            Self::Unknown(s) => Self::Unknown(prefix + &s),
        }
    }

    /// Check if this error indicates rate limiting
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
    }

    /// Check if this error indicates context length was exceeded
    pub fn is_context_exceeded(&self) -> bool {
        matches!(self, Self::ContextLengthExceeded(_))
    }

    /// Check if this error indicates credits are exhausted
    pub fn is_credits_exhausted(&self) -> bool {
        matches!(self, Self::CreditsExhausted { .. })
    }

    /// Get the retry delay if this is a rate limit error
    pub fn retry_delay(&self) -> Option<std::time::Duration> {
        match self {
            Self::RateLimited { retry_delay } => *retry_delay,
            _ => None,
        }
    }

    /// Get the top-up URL if credits are exhausted
    pub fn top_up_url(&self) -> Option<&str> {
        match self {
            Self::CreditsExhausted { top_up_url, .. } => top_up_url.as_deref(),
            _ => None,
        }
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Network(_) | Self::Timeout(_)
        )
    }
}
