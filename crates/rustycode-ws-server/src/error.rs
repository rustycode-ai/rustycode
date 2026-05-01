use thiserror::Error;

#[derive(Debug, Error)]
pub enum WsError {
    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("channel closed")]
    ChannelClosed,

    #[error("connection closed")]
    ConnectionClosed,

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidMessage,
    SessionNotFound,
    SessionExpired,
    RateLimited,
    InternalError,
    Unauthorized,
}

#[derive(Debug, serde::Serialize)]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub message: String,
}
