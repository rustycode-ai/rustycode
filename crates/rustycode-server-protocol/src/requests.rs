use rustycode_protocol::SessionMode;
use serde::{Deserialize, Serialize};

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
pub struct SymbolOutlineRequest {
    pub file_path: String,
}
