use std::sync::Arc;

use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tracing::{error, info, warn};

use crate::bridge::EventBridge;
use crate::error::{ErrorCode, ErrorPayload};
use crate::protocol::{
    Capabilities, ClientMessage, Envelope, HeartbeatAckPayload, ServerMessage,
    SessionCreatedPayload, SessionResumedPayload,
};
use crate::session::SessionManager;

#[derive(Clone)]
pub struct AppState {
    pub session_manager: Arc<SessionManager>,
}

pub struct WsRouter;

impl WsRouter {
    pub fn build(session_manager: SessionManager) -> Router {
        let state = AppState {
            session_manager: Arc::new(session_manager),
        };
        Router::new()
            .route("/ws", get(ws_upgrade_handler))
            .with_state(state)
    }
}

async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    info!("websocket connection opened");

    let session_token;

    // Wait for hello message
    if let Some(hello) = wait_for_hello(&mut receiver).await {
        let (session_state, resumed) = state
            .session_manager
            .get_or_create(hello.session_token.as_deref())
            .await
            .unwrap_or_else(|e| {
                error!("session creation failed: {e}");
                panic!("session creation failed");
            });

        session_token = session_state.id.to_string();

        let msg = if resumed {
            ServerMessage::SessionResumed(SessionResumedPayload {
                session_token: session_token.clone(),
            })
        } else {
            ServerMessage::SessionCreated(SessionCreatedPayload {
                session_token: session_token.clone(),
                capabilities: Capabilities::default(),
            })
        };

        if let Err(e) = send_server_message(&mut sender, &msg, &correlation_id(0)).await {
            warn!("failed to send session response: {e}");
            return;
        }

        state
            .session_manager
            .client_connected(&session_token)
            .await
            .unwrap_or_else(|e| warn!("client tracking failed: {e}"));

        // Send initial state snapshot
        if let Ok(snapshot) = state.session_manager.snapshot(&session_token).await {
            let snap_msg = ServerMessage::StateSnapshot(snapshot);
            if let Err(e) = send_server_message(&mut sender, &snap_msg, &correlation_id(0)).await
            {
                warn!("failed to send state snapshot: {e}");
            }
        }
    } else {
        let err_msg = ServerMessage::Error(ErrorPayload {
            code: ErrorCode::Unauthorized,
            message: "expected hello message".to_string(),
        });
        let _ = send_server_message(&mut sender, &err_msg, "err").await;
        return;
    }

    // Main message loop
    let bridge = EventBridge::new(state.session_manager.clone(), session_token.clone());

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Err(e) =
                    handle_text_message(&text, &state, &session_token, &mut sender).await
                {
                    warn!("message handling error: {e}");
                    let err_msg = ServerMessage::Error(ErrorPayload {
                        code: ErrorCode::InternalError,
                        message: e.to_string(),
                    });
                    let _ = send_server_message(&mut sender, &err_msg, "err").await;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(data)) => {
                let _ = sender.send(Message::Pong(data)).await;
            }
            Err(e) => {
                warn!("websocket receive error: {e}");
                break;
            }
            _ => {}
        }
    }

    state
        .session_manager
        .client_disconnected(&session_token)
        .await;

    drop(bridge);
    info!(session_id = %session_token, "websocket connection closed");
}

async fn wait_for_hello(
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
) -> Option<crate::protocol::HelloPayload> {
    // First message must be hello, with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(envelope) = Envelope::decode(&text) {
                        if envelope.r#type == "hello" {
                            let hello: crate::protocol::HelloPayload =
                                serde_json::from_value(envelope.payload).ok()?;
                            return Some(hello);
                        }
                    }
                    return None;
                }
                Ok(Message::Close(_)) => return None,
                _ => {}
            }
        }
        None
    })
    .await;

    result.unwrap_or(None)
}

async fn handle_text_message(
    text: &str,
    state: &AppState,
    session_token: &str,
    sender: &mut futures_util::stream::SplitSink<
        WebSocket,
        Message,
    >,
) -> Result<(), crate::error::WsError> {
    let envelope =
        Envelope::decode(text).map_err(|e| crate::error::WsError::Protocol(e.to_string()))?;
    let client_msg = ClientMessage::from_envelope(&envelope)
        .map_err(crate::error::WsError::Protocol)?;
    let corr_id = envelope.id.clone();

    match client_msg {
        ClientMessage::Hello(_) => {
            let err_msg = ServerMessage::Error(ErrorPayload {
                code: ErrorCode::InvalidMessage,
                message: "hello already received".to_string(),
            });
            send_server_message(sender, &err_msg, &corr_id).await?;
        }
        ClientMessage::Input(payload) => {
            state
                .session_manager
                .submit_input(session_token, &payload.content)
                .await?;
        }
        ClientMessage::Abort => {
            info!(session_id = session_token, "abort requested");
        }
        ClientMessage::Heartbeat(payload) => {
            let ack = ServerMessage::HeartbeatAck(HeartbeatAckPayload {
                ts: payload.ts,
                server_ts: chrono::Utc::now().timestamp_millis(),
            });
            send_server_message(sender, &ack, &corr_id).await?;
        }
    }

    Ok(())
}

async fn send_server_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
    id: &str,
) -> Result<(), crate::error::WsError> {
    let envelope = msg
        .to_envelope(id)
        .map_err(crate::error::WsError::Serialization)?;
    let json = envelope
        .encode()
        .map_err(crate::error::WsError::Serialization)?;
    sender
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| crate::error::WsError::ConnectionClosed)
}

fn correlation_id(seq: u64) -> String {
    format!("corr-{seq}")
}
