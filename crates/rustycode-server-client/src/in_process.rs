use anyhow::{Context, Result};
use dashmap::DashMap;
use rustycode_server_protocol::{
    notifications::Notification, ClientMessage, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, RequestId, ServerMessage,
};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::warn;

pub struct InProcessClient {
    client_id: String,
    outbound_tx: mpsc::Sender<(String, ClientMessage)>,
    pending_requests: Arc<DashMap<RequestId, oneshot::Sender<JsonRpcResponse>>>,
    notification_tx: broadcast::Sender<Notification>,
    next_id: AtomicU64,
}
impl Clone for InProcessClient {
    fn clone(&self) -> Self {
        Self {
            client_id: self.client_id.clone(),
            outbound_tx: self.outbound_tx.clone(),
            pending_requests: self.pending_requests.clone(),
            notification_tx: self.notification_tx.clone(),
            next_id: AtomicU64::new(self.next_id.load(Ordering::SeqCst)),
        }
    }
}

impl InProcessClient {
    pub fn new(
        client_id: String,
        outbound_tx: mpsc::Sender<(String, ClientMessage)>,
        mut inbound_rx: mpsc::Receiver<ServerMessage>,
    ) -> Self {
        let (notification_tx, _) = broadcast::channel(256);
        let pending_requests: Arc<DashMap<RequestId, oneshot::Sender<JsonRpcResponse>>> =
            Arc::new(DashMap::new());
        let pending_requests_clone = Arc::clone(&pending_requests);
        let notification_tx_clone = notification_tx.clone();

        // Spawn inbound message handler on its own thread with a dedicated
        // Tokio runtime. This avoids panicking when no runtime exists (e.g. TUI).
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    warn!("Failed to create runtime for inbound handler: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                while let Some(msg) = inbound_rx.recv().await {
                    match msg {
                        ServerMessage::Response(resp) => {
                            if let Some((_, tx)) = pending_requests_clone.remove(&resp.id) {
                                let _ = tx.send(resp);
                            } else {
                                warn!("Received response for unknown request ID: {:?}", resp.id);
                            }
                        }
                        ServerMessage::Notification(notif) => {
                            let _ = notification_tx_clone.send(notif);
                        }
                    }
                }
            });
        });

        Self {
            client_id,
            outbound_tx,
            pending_requests,
            notification_tx,
            next_id: AtomicU64::new(1),
        }
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = RequestId::Number(self.next_id.fetch_add(1, Ordering::SeqCst));
        let (tx, rx) = oneshot::channel();

        self.pending_requests.insert(id.clone(), tx);

        let request = JsonRpcRequest::new(id.clone(), method, params);

        self.outbound_tx
            .send((self.client_id.clone(), ClientMessage::Request(request)))
            .await
            .context("Failed to send request to server")?;

        let response = rx.await.context("Server dropped request")?;

        if let Some(error) = response.error {
            anyhow::bail!("JSON-RPC Error {}: {}", error.code, error.message);
        }

        response
            .result
            .context("Response missing both result and error")
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let notification = JsonRpcNotification::new(method, params);
        self.outbound_tx
            .send((
                self.client_id.clone(),
                ClientMessage::Notification(notification),
            ))
            .await
            .context("Failed to send notification to server")?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Notification> {
        self.notification_tx.subscribe()
    }
}
