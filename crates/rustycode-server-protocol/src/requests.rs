use serde::{Deserialize, Serialize};
use rustycode_protocol::SessionMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateRequest {
    pub model: String,
    pub system_prompt: Option<String>,
    pub mode: Option<SessionMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListRequest {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSubmitRequest {
    pub session_id: String,
    pub message: String,
}
