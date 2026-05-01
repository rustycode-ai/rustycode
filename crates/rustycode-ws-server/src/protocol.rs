use rustycode_protocol::StreamEvent;
use rustycode_ui_model::FrontendSession;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub v: u8,
    pub r#type: String,
    pub id: String,
    pub payload: serde_json::Value,
}

impl Envelope {
    pub fn new(msg_type: impl Into<String>, id: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            r#type: msg_type.into(),
            id: id.into(),
            payload,
        }
    }

    pub fn encode(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn decode(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloPayload {
    pub session_token: Option<String>,
    pub client_info: Option<ClientInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputPayload {
    pub content: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalRequestPayload {
    pub request_id: String,
    pub tool_name: String,
    pub input_preview: String,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalResponsePayload {
    pub request_id: String,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanApprovalPayload {
    pub plan_id: String,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreatedPayload {
    pub session_token: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResumedPayload {
    pub session_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub seq: u64,
    pub event_id: String,
    #[serde(flatten)]
    pub event: StreamEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatAckPayload {
    pub ts: i64,
    pub server_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub max_frame_size: usize,
    pub heartbeat_interval_secs: u64,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            max_frame_size: 256 * 1024,
            heartbeat_interval_secs: 30,
        }
    }
}

pub mod client_types {
    pub const HELLO: &str = "hello";
    pub const INPUT: &str = "input";
    pub const ABORT: &str = "abort";
    pub const HEARTBEAT: &str = "heartbeat";
    pub const TOOL_APPROVAL: &str = "tool_approval";
    pub const PLAN_APPROVAL: &str = "plan_approval";
}

pub mod server_types {
    pub const SESSION_CREATED: &str = "session_created";
    pub const SESSION_RESUMED: &str = "session_resumed";
    pub const EVENT: &str = "event";
    pub const STATE_SNAPSHOT: &str = "state_snapshot";
    pub const HEARTBEAT_ACK: &str = "heartbeat_ack";
    pub const ERROR: &str = "error";
    pub const TOOL_APPROVAL_REQUESTED: &str = "tool_approval_requested";
}

pub enum ClientMessage {
    Hello(HelloPayload),
    Input(InputPayload),
    Abort,
    Heartbeat(HeartbeatPayload),
    ToolApproval(ToolApprovalResponsePayload),
    PlanApproval(PlanApprovalPayload),
}

impl ClientMessage {
    pub fn from_envelope(envelope: &Envelope) -> Result<Self, String> {
        match envelope.r#type.as_str() {
            client_types::HELLO => {
                let payload: HelloPayload = serde_json::from_value(envelope.payload.clone())
                    .map_err(|e| format!("invalid hello payload: {e}"))?;
                Ok(Self::Hello(payload))
            }
            client_types::INPUT => {
                let payload: InputPayload = serde_json::from_value(envelope.payload.clone())
                    .map_err(|e| format!("invalid input payload: {e}"))?;
                Ok(Self::Input(payload))
            }
            client_types::ABORT => Ok(Self::Abort),
            client_types::HEARTBEAT => {
                let payload: HeartbeatPayload = serde_json::from_value(envelope.payload.clone())
                    .map_err(|e| format!("invalid heartbeat payload: {e}"))?;
                Ok(Self::Heartbeat(payload))
            }
            client_types::TOOL_APPROVAL => {
                let payload: ToolApprovalResponsePayload = serde_json::from_value(envelope.payload.clone())
                    .map_err(|e| format!("invalid tool_approval payload: {e}"))?;
                Ok(Self::ToolApproval(payload))
            }
            client_types::PLAN_APPROVAL => {
                let payload: PlanApprovalPayload = serde_json::from_value(envelope.payload.clone())
                    .map_err(|e| format!("invalid plan_approval payload: {e}"))?;
                Ok(Self::PlanApproval(payload))
            }
            other => Err(format!("unknown message type: {other}")),
        }
    }
}

pub enum ServerMessage {
    SessionCreated(SessionCreatedPayload),
    SessionResumed(SessionResumedPayload),
    Event(EventPayload),
    StateSnapshot(FrontendSession),
    HeartbeatAck(HeartbeatAckPayload),
    ToolApprovalRequested(ToolApprovalRequestPayload),
    Error(crate::error::ErrorPayload),
}

impl ServerMessage {
    pub fn to_envelope(&self, id: &str) -> Result<Envelope, serde_json::Error> {
        match self {
            Self::SessionCreated(payload) => Ok(Envelope::new(
                server_types::SESSION_CREATED,
                id,
                serde_json::to_value(payload)?,
            )),
            Self::SessionResumed(payload) => Ok(Envelope::new(
                server_types::SESSION_RESUMED,
                id,
                serde_json::to_value(payload)?,
            )),
            Self::Event(payload) => Ok(Envelope::new(
                server_types::EVENT,
                id,
                serde_json::to_value(payload)?,
            )),
            Self::StateSnapshot(session) => Ok(Envelope::new(
                server_types::STATE_SNAPSHOT,
                id,
                serde_json::to_value(session)?,
            )),
            Self::HeartbeatAck(payload) => Ok(Envelope::new(
                server_types::HEARTBEAT_ACK,
                id,
                serde_json::to_value(payload)?,
            )),
            Self::ToolApprovalRequested(payload) => Ok(Envelope::new(
                server_types::TOOL_APPROVAL_REQUESTED,
                id,
                serde_json::to_value(payload)?,
            )),
            Self::Error(payload) => Ok(Envelope::new(
                server_types::ERROR,
                id,
                serde_json::to_value(payload)?,
            )),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip() {
        let envelope = Envelope::new(
            "hello",
            "test-123",
            serde_json::json!({"session_token": null}),
        );
        let json = envelope.encode().unwrap();
        let decoded = Envelope::decode(&json).unwrap();
        assert_eq!(decoded.v, PROTOCOL_VERSION);
        assert_eq!(decoded.r#type, "hello");
        assert_eq!(decoded.id, "test-123");
    }

    #[test]
    fn client_message_hello() {
        let envelope = Envelope::new(
            "hello",
            "1",
            serde_json::json!({"session_token": "abc", "client_info": null}),
        );
        let msg = ClientMessage::from_envelope(&envelope).unwrap();
        assert!(matches!(msg, ClientMessage::Hello(p) if p.session_token.as_deref() == Some("abc")));
    }

    #[test]
    fn client_message_input() {
        let envelope = Envelope::new(
            "input",
            "2",
            serde_json::json!({"content": "hello world"}),
        );
        let msg = ClientMessage::from_envelope(&envelope).unwrap();
        assert!(matches!(msg, ClientMessage::Input(p) if p.content == "hello world"));
    }

    #[test]
    fn client_message_abort() {
        let envelope = Envelope::new("abort", "3", serde_json::json!({}));
        let msg = ClientMessage::from_envelope(&envelope).unwrap();
        assert!(matches!(msg, ClientMessage::Abort));
    }

    #[test]
    fn client_message_heartbeat() {
        let envelope = Envelope::new("heartbeat", "4", serde_json::json!({"ts": 12345}));
        let msg = ClientMessage::from_envelope(&envelope).unwrap();
        assert!(matches!(msg, ClientMessage::Heartbeat(p) if p.ts == 12345));
    }

    #[test]
    fn client_message_unknown_type() {
        let envelope = Envelope::new("unknown", "5", serde_json::json!({}));
        let result = ClientMessage::from_envelope(&envelope);
        assert!(result.is_err());
    }

    #[test]
    fn client_message_tool_approval() {
        let envelope = Envelope::new(
            "tool_approval",
            "6",
            serde_json::json!({"request_id": "req-1", "approved": true}),
        );
        let msg = ClientMessage::from_envelope(&envelope).unwrap();
        assert!(matches!(msg, ClientMessage::ToolApproval(p) if p.approved && p.request_id == "req-1"));
    }

    #[test]
    fn client_message_plan_approval() {
        let envelope = Envelope::new(
            "plan_approval",
            "7",
            serde_json::json!({"plan_id": "plan-42", "approved": false}),
        );
        let msg = ClientMessage::from_envelope(&envelope).unwrap();
        assert!(matches!(msg, ClientMessage::PlanApproval(p) if !p.approved && p.plan_id == "plan-42"));
    }

    #[test]
    fn server_message_session_created() {
        let msg = ServerMessage::SessionCreated(SessionCreatedPayload {
            session_token: "tok-123".to_string(),
            capabilities: Capabilities::default(),
        });
        let envelope = msg.to_envelope("corr-1").unwrap();
        assert_eq!(envelope.r#type, "session_created");
        assert_eq!(envelope.v, PROTOCOL_VERSION);
    }

    #[test]
    fn server_message_event() {
        let msg = ServerMessage::Event(EventPayload {
            seq: 42,
            event_id: "evt-abc".to_string(),
            event: StreamEvent::TextDelta {
                content: "hello".to_string(),
            },
        });
        let envelope = msg.to_envelope("corr-2").unwrap();
        assert_eq!(envelope.r#type, "event");

        let payload: EventPayload = serde_json::from_value(envelope.payload).unwrap();
        assert_eq!(payload.seq, 42);
    }

    #[test]
    fn server_message_state_snapshot() {
        let msg = ServerMessage::StateSnapshot(FrontendSession::default());
        let envelope = msg.to_envelope("corr-3").unwrap();
        assert_eq!(envelope.r#type, "state_snapshot");

        let session: FrontendSession = serde_json::from_value(envelope.payload).unwrap();
        assert!(session.messages.is_empty());
    }

    #[test]
    fn server_message_error() {
        let msg = ServerMessage::Error(crate::error::ErrorPayload {
            code: crate::error::ErrorCode::InternalError,
            message: "something broke".to_string(),
        });
        let envelope = msg.to_envelope("corr-4").unwrap();
        assert_eq!(envelope.r#type, "error");
    }

    #[test]
    fn server_message_heartbeat_ack() {
        let msg = ServerMessage::HeartbeatAck(HeartbeatAckPayload {
            ts: 1000,
            server_ts: 1005,
        });
        let envelope = msg.to_envelope("corr-5").unwrap();
        assert_eq!(envelope.r#type, "heartbeat_ack");
    }

    #[test]
    fn event_payload_tool_call_started() {
        let payload = make_event(StreamEvent::ToolCallStarted {
            id: "tool-1".to_string(),
            name: "bash".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "tool_call_started");
        assert_eq!(json["data"]["id"], "tool-1");
        assert_eq!(json["data"]["name"], "bash");
    }

    #[test]
    fn event_payload_text_delta() {
        let payload = make_event(StreamEvent::TextDelta {
            content: "hello world".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "text_delta");
        assert_eq!(json["data"]["content"], "hello world");
    }

    #[test]
    fn event_payload_thinking_delta() {
        let payload = make_event(StreamEvent::ThinkingDelta {
            content: "reasoning...".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "thinking_delta");
        assert_eq!(json["data"]["content"], "reasoning...");
    }

    #[test]
    fn event_payload_tool_input_delta() {
        let payload = make_event(StreamEvent::ToolInputDelta {
            id: "tool-2".to_string(),
            chunk: r#"{"command":"ls"}"#.to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "tool_input_delta");
        assert_eq!(json["data"]["id"], "tool-2");
        assert_eq!(json["data"]["chunk"], r#"{"command":"ls"}"#);
    }

    #[test]
    fn event_payload_tool_exec_started() {
        let payload = make_event(StreamEvent::ToolExecStarted {
            id: "tool-3".to_string(),
            name: "read_file".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "tool_exec_started");
        assert_eq!(json["data"]["id"], "tool-3");
        assert_eq!(json["data"]["name"], "read_file");
    }

    #[test]
    fn event_payload_tool_exec_completed() {
        let payload = make_event(StreamEvent::ToolExecCompleted {
            id: "tool-4".to_string(),
            name: "bash".to_string(),
            output: "file.txt\nmain.rs".to_string(),
            is_error: false,
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "tool_exec_completed");
        assert_eq!(json["data"]["id"], "tool-4");
        assert_eq!(json["data"]["name"], "bash");
        assert_eq!(json["data"]["is_error"], false);
    }

    #[test]
    fn event_payload_tool_exec_completed_error() {
        let payload = make_event(StreamEvent::ToolExecCompleted {
            id: "tool-5".to_string(),
            name: "bash".to_string(),
            output: "permission denied".to_string(),
            is_error: true,
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "tool_exec_completed");
        assert_eq!(json["data"]["is_error"], true);
    }

    #[test]
    fn event_payload_turn_started() {
        let payload = make_event(StreamEvent::TurnStarted { turn: 3 });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "turn_started");
        assert_eq!(json["data"]["turn"], 3);
    }

    #[test]
    fn event_payload_token_usage() {
        let payload = make_event(StreamEvent::TokenUsage {
            input_tokens: 1500,
            output_tokens: 800,
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "token_usage");
        assert_eq!(json["data"]["input_tokens"], 1500);
        assert_eq!(json["data"]["output_tokens"], 800);
    }

    #[test]
    fn event_payload_turn_completed() {
        let payload = make_event(StreamEvent::TurnCompleted {
            stop_reason: "end_turn".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "turn_completed");
        assert_eq!(json["data"]["stop_reason"], "end_turn");
    }

    #[test]
    fn event_payload_cache_usage() {
        let payload = make_event(StreamEvent::CacheUsage {
            cache_read_tokens: 5000,
            cache_creation_tokens: 1200,
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "cache_usage");
        assert_eq!(json["data"]["cache_read_tokens"], 5000);
        assert_eq!(json["data"]["cache_creation_tokens"], 1200);
    }

    #[test]
    fn event_payload_done() {
        let payload = make_event(StreamEvent::Done);
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "done");
    }

    #[test]
    fn event_payload_plan_created() {
        use rustycode_protocol::StreamPlanStep;
        let payload = make_event(StreamEvent::PlanCreated {
            id: "plan-1".to_string(),
            title: "Refactor auth".to_string(),
            steps: vec![
                StreamPlanStep { name: "Add types".into(), description: "Define newtypes".into() },
                StreamPlanStep { name: "Migrate".into(), description: "Update callers".into() },
            ],
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "plan_created");
        assert_eq!(json["data"]["id"], "plan-1");
        assert_eq!(json["data"]["title"], "Refactor auth");
        assert_eq!(json["data"]["steps"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn event_payload_plan_step_started() {
        let payload = make_event(StreamEvent::PlanStepStarted {
            plan_id: "plan-1".to_string(),
            step_index: 0,
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "plan_step_started");
        assert_eq!(json["data"]["step_index"], 0);
    }

    #[test]
    fn event_payload_plan_step_completed() {
        let payload = make_event(StreamEvent::PlanStepCompleted {
            plan_id: "plan-1".to_string(),
            step_index: 0,
            success: true,
            message: "Done".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "plan_step_completed");
        assert_eq!(json["data"]["success"], true);
    }

    #[test]
    fn event_payload_plan_completed() {
        let payload = make_event(StreamEvent::PlanCompleted {
            plan_id: "plan-1".to_string(),
            success: true,
            summary: "All steps completed".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "plan_completed");
        assert_eq!(json["data"]["plan_id"], "plan-1");
        assert_eq!(json["data"]["success"], true);
    }

    #[test]
    fn event_payload_plan_approval_requested() {
        use rustycode_protocol::StreamPlanStep;
        let payload = make_event(StreamEvent::PlanApprovalRequested {
            plan_id: "plan-1".to_string(),
            title: "Refactor auth".to_string(),
            steps: vec![
                StreamPlanStep { name: "Step 1".into(), description: "Do the thing".into() },
            ],
        });
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["type"], "plan_approval_requested");
        assert_eq!(json["data"]["title"], "Refactor auth");
    }

    #[test]
    fn event_roundtrip_text_delta() {
        let payload = make_event(StreamEvent::TextDelta {
            content: "roundtrip test".to_string(),
        });
        let json_str = serde_json::to_string(&payload).unwrap();
        let roundtrip: EventPayload = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtrip.seq, 1);
        assert_eq!(roundtrip.event_id, "evt-test");
    }

    fn make_event(event: StreamEvent) -> EventPayload {
        EventPayload {
            seq: 1,
            event_id: "evt-test".to_string(),
            event,
        }
    }

    #[test]
    fn capabilities_defaults() {
        let caps = Capabilities::default();
        assert_eq!(caps.max_frame_size, 256 * 1024);
        assert_eq!(caps.heartbeat_interval_secs, 30);
    }
}
