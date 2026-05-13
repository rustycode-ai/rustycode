use serde::{Deserialize, Serialize};
use rustycode_protocol::{SessionId, SessionSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateResponse {
    pub session_id: SessionId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionSnapshot>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolOutlineResponse {
    pub outline: rustycode_protocol::code_symbol::FileOutline,
}
