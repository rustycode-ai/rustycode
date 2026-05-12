use serde::{Deserialize, Serialize};
use rustycode_protocol::{SessionId, EventMsg};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartedNotification {
    pub session_id: SessionId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStoppedNotification {
    pub session_id: SessionId,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Notification {
    SessionStarted(SessionStartedNotification),
    SessionStopped(SessionStoppedNotification),
    Event(EventMsg),
}
