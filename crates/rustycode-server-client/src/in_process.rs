use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{mpsc, broadcast, oneshot};
use dashmap::DashMap;
use anyhow::{Result, Context};
use serde_json::Value;
use tracing::warn;
use rustycode_server_protocol::{
    RequestId, JsonRpcRequest, JsonRpcResponse, JsonRpcNotification,
    notifications::Notification, ClientMessage, ServerMessage,
};

pub struct InProcessClient {
    client_id: String,
    outbound_tx: mpsc::Sender<(String, ClientMessage)>,
    pending_requests: Arc<DashMap<RequestId, oneshot::Sender<JsonRpcResponse>>>,
    notification_tx: broadcast::Sender<Notification>,
    next_id: AtomicU64,
}

impl InProcessClient {
    pub fn new(
        client_id: String,
        outbound_tx: mpsc::Sender<(String, ClientMessage)>,
        mut inbound_rx: mpsc::Receiver<ServerMessage>,
    ) -> Self {
        let (notification_tx, _) = broadcast::channel(256);
        let pending_requests: Arc<DashMap<RequestId, oneshot::Sender<JsonRpcResponse>>> = Arc::new(DashMap::new());
        let pending_requests_clone = Arc::clone(&pending_requests);
        let notification_tx_clone = notification_tx.clone();

        // Spawn inbound message handler
        tokio::spawn(async move {
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
        
        self.outbound_tx.send((self.client_id.clone(), ClientMessage::Request(request))).await
            .context("Failed to send request to server")?;

        let response = rx.await.context("Server dropped request")?;
        
        if let Some(error) = response.error {
            anyhow::bail!("JSON-RPC Error {}: {}", error.code, error.message);
        }

        response.result.context("Response missing both result and error")
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let notification = JsonRpcNotification::new(method, params);
        self.outbound_tx.send((self.client_id.clone(), ClientMessage::Notification(notification))).await
            .context("Failed to send notification to server")?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Notification> {
        self.notification_tx.subscribe()
    }
}
