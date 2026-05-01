use std::sync::Arc;

use axum::{
    extract::{Path, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
    routing::{delete, get},
    Json,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::bridge::EventBridge;
use crate::error::{ErrorCode, ErrorPayload};
use crate::protocol::{
    Capabilities, ClientMessage, Envelope, HeartbeatAckPayload, ServerMessage,
    SessionCreatedPayload, SessionResumedPayload,
};
use crate::session::{SessionManager, SessionInfo, ProviderInfo, SessionState};

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
            .route("/api/providers", get(get_providers))
            .route("/api/sessions", get(list_sessions))
            .route("/api/sessions/{id}", delete(delete_session))
            .route("/api/sessions/new", get(create_session))
            .with_state(state)
    }
}

async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Message sent from various tasks to the WS forwarding loop.
enum Outbound {
    Server(ServerMessage),
    Close,
}

#[allow(clippy::too_many_lines)]
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    info!("websocket connection opened");

    let session_token;

    // Wait for hello message
    if let Some(hello) = wait_for_hello(&mut ws_receiver).await {
        let (session_state, resumed) = match state
            .session_manager
            .get_or_create(hello.session_token.as_deref())
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!("session creation failed: {e}");
                let err_msg = ServerMessage::Error(ErrorPayload {
                    code: ErrorCode::InternalError,
                    message: format!("session creation failed: {e}"),
                });
                let _ = send_server_message(&mut ws_sender, &err_msg, "err").await;
                return;
            }
        };

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

        if let Err(e) = send_server_message(&mut ws_sender, &msg, &correlation_id(0)).await {
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
            if let Err(e) = send_server_message(&mut ws_sender, &snap_msg, &correlation_id(0)).await
            {
                warn!("failed to send state snapshot: {e}");
            }
        }
    } else {
        let err_msg = ServerMessage::Error(ErrorPayload {
            code: ErrorCode::Unauthorized,
            message: "expected hello message".to_string(),
        });
        let _ = send_server_message(&mut ws_sender, &err_msg, "err").await;
        return;
    }

    // Channel for outbound messages — both the main loop and EventBridge write to it.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Outbound>();

    // Spawn forwarding task: reads from out_rx and sends to WebSocket.
    let forward_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        while let Some(outbound) = out_rx.recv().await {
            match outbound {
                Outbound::Server(msg) => {
                    if let Err(e) = send_server_message(&mut ws_sender, &msg, "auto").await {
                        warn!("ws send failed: {e}");
                        break;
                    }
                }
                Outbound::Close => break,
            }
        }
    });

    // Main message loop — reads from WebSocket, dispatches.
    let mut bridge_handle: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Err(e) = handle_text_message(
                    &text,
                    &state,
                    &session_token,
                    &out_tx,
                    &mut bridge_handle,
                )
                .await
                {
                    warn!("message handling error: {e}");
                    let _ = out_tx.send(Outbound::Server(ServerMessage::Error(ErrorPayload {
                        code: ErrorCode::InternalError,
                        message: e.to_string(),
                    })));
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_data)) => {
                // Axum handles pings automatically, but just in case
                let _ = out_tx.send(Outbound::Server(
                    ServerMessage::HeartbeatAck(HeartbeatAckPayload {
                        ts: 0,
                        server_ts: chrono::Utc::now().timestamp_millis(),
                    }),
                ));
            }
            Err(e) => {
                warn!("websocket receive error: {e}");
                break;
            }
            _ => {}
        }
    }

    // Clean up
    if let Some(handle) = bridge_handle {
        handle.abort();
    }
    let _ = out_tx.send(Outbound::Close);
    let _ = forward_handle.await;

    state
        .session_manager
        .client_disconnected(&session_token)
        .await;

    info!(session_id = %session_token, "websocket connection closed");
}

async fn wait_for_hello(
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
) -> Option<crate::protocol::HelloPayload> {
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

#[allow(clippy::too_many_lines, clippy::items_after_statements)]
async fn handle_text_message(
    text: &str,
    state: &AppState,
    session_token: &str,
    out_tx: &mpsc::UnboundedSender<Outbound>,
    bridge_handle: &mut Option<tokio::task::JoinHandle<()>>,
) -> Result<(), crate::error::WsError> {
    let envelope =
        Envelope::decode(text).map_err(|e| crate::error::WsError::Protocol(e.to_string()))?;
    let client_msg = ClientMessage::from_envelope(&envelope)
        .map_err(crate::error::WsError::Protocol)?;

    match client_msg {
        ClientMessage::Hello(_) => {
            let _ = out_tx.send(Outbound::Server(ServerMessage::Error(ErrorPayload {
                code: ErrorCode::InvalidMessage,
                message: "hello already received".to_string(),
            })));
        }
        ClientMessage::Input(payload) => {
            let task_id = state
                .session_manager
                .submit_input(session_token, &payload.content)
                .await?;

            // Abort previous bridge
            if let Some(handle) = bridge_handle.take() {
                handle.abort();
            }

            // Spawn event bridge that sends to our outbound channel
            let bridge = EventBridge::new(
                state.session_manager.clone(),
                session_token.to_string(),
                task_id,
            );
            let out_tx_clone = out_tx.clone();
            let handle = tokio::spawn(async move {
                // The bridge reads from the orchestration bus and sends
                // Outbound::Server messages through out_tx_clone.
                // We use a wrapper that adapts the bridge to use our channel.
                let bus = bridge.session_manager.pipeline().bus_handle();
                let mut rx = bus.subscribe();

                use rustycode_orchestration::bus::OrchestrationEvent;
                use rustycode_protocol::StreamEvent;

                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let Some(stream_event) = (match &event {
                                OrchestrationEvent::TextDelta { task_id: tid, content }
                                    if tid == &bridge.task_id =>
                                {
                                    Some(StreamEvent::TextDelta { content: content.clone() })
                                }
                                OrchestrationEvent::ThinkingDelta { task_id: tid, content }
                                    if tid == &bridge.task_id =>
                                {
                                    Some(StreamEvent::ThinkingDelta { content: content.clone() })
                                }
                                OrchestrationEvent::ToolCallStarted {
                                    task_id: tid, tool_id, tool_name, ..
                                } if tid == &bridge.task_id => Some(StreamEvent::ToolCallStarted {
                                    id: tool_id.clone(),
                                    name: tool_name.clone(),
                                }),
                                OrchestrationEvent::ToolCallCompleted {
                                    task_id: tid, tool_id, tool_name, success, output_preview, ..
                                } if tid == &bridge.task_id => Some(StreamEvent::ToolExecCompleted {
                                    id: tool_id.clone(),
                                    name: tool_name.clone(),
                                    output: output_preview.clone(),
                                    is_error: !success,
                                }),
                                OrchestrationEvent::TaskCompleted { task_id: tid, .. }
                                    if tid == &bridge.task_id =>
                                {
                                    Some(StreamEvent::Done)
                                }
                                _ => None,
                            }) else {
                                continue;
                            };

                            let seq = bridge
                                .session_manager
                                .update_session(&bridge.session_token, SessionState::next_seq)
                                .await
                                .unwrap_or(0);

                            let payload = crate::protocol::EventPayload {
                                seq,
                                event_id: format!("evt-{seq}"),
                                event: stream_event,
                            };

                            if out_tx_clone
                                .send(Outbound::Server(ServerMessage::Event(payload)))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("bus lagged by {n} events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            *bridge_handle = Some(handle);
        }
        ClientMessage::Abort => {
            if let Some(handle) = bridge_handle.take() {
                handle.abort();
            }
            state.session_manager.abort(session_token).await?;
            info!(session_id = session_token, "abort requested");
        }
        ClientMessage::Heartbeat(payload) => {
            let _ = out_tx.send(Outbound::Server(ServerMessage::HeartbeatAck(
                HeartbeatAckPayload {
                    ts: payload.ts,
                    server_ts: chrono::Utc::now().timestamp_millis(),
                },
            )));
        }
    }

    Ok(())
}

// ── REST API Handlers ──────────────────────────────────────

async fn get_providers(
    State(state): State<AppState>,
) -> Json<ProviderInfo> {
    Json(state.session_manager.provider_info())
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Json<Vec<SessionInfo>> {
    Json(state.session_manager.list_sessions().await)
}

async fn create_session(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let session_state = state.session_manager.create_session().await;
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "session_token": session_state.id.to_string() })),
    )
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Json<serde_json::Value>> {
    state
        .session_manager
        .delete_session(&id)
        .await
        .map_err(|e| {
            Json(serde_json::json!({ "error": e.to_string() }))
        })?;
    Ok(axum::http::StatusCode::NO_CONTENT)
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
