use anyhow::{Context, Result};
use dashmap::DashMap;
use rustycode_runtime::AsyncRuntime;
use rustycode_server_protocol::{notifications::Notification, ClientMessage, ServerMessage};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

pub type ClientId = String;

pub struct AppServer {
    /// Core runtime handle
    runtime: Arc<AsyncRuntime>,
    /// Connected clients (clientId -> sender)
    clients: Arc<DashMap<ClientId, mpsc::Sender<ServerMessage>>>,
    /// Inbound from clients
    inbound_rx: mpsc::Receiver<(ClientId, ClientMessage)>,
    /// Outbound broadcaster for notifications
    notify_tx: broadcast::Sender<Notification>,
}

impl AppServer {
    pub fn new(
        runtime: Arc<AsyncRuntime>,
        inbound_rx: mpsc::Receiver<(ClientId, ClientMessage)>,
    ) -> Self {
        let (notify_tx, _) = broadcast::channel(256);
        Self {
            runtime,
            clients: Arc::new(DashMap::new()),
            inbound_rx,
            notify_tx,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("AppServer running");

        // Subscribe to runtime events
        let (_id, mut event_rx) = self
            .runtime
            .subscribe_events("*")
            .await
            .context("Failed to subscribe to runtime events")?;

        loop {
            tokio::select! {
                msg = self.inbound_rx.recv() => {
                    match msg {
                        Some((client_id, msg)) => {
                            if let Err(e) = self.handle_client_message(client_id, msg).await {
                                error!("Error handling client message: {:?}", e);
                            }
                        }
                        None => {
                            info!("Inbound channel closed, shutting down AppServer");
                            break;
                        }
                    }
                }
                event = event_rx.recv() => {
                    match event {
                        Ok(event) => {
                            debug!("Received runtime event: {:?}", event.event_type());
                            // TODO: Map event to Notification and broadcast
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!("Event bus lagged, skipped {} events", skipped);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            error!("Event bus closed");
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn add_client(&self, client_id: ClientId, tx: mpsc::Sender<ServerMessage>) {
        self.clients.insert(client_id, tx);
    }

    pub fn remove_client(&self, client_id: &ClientId) {
        self.clients.remove(client_id);
    }

    async fn handle_client_message(&self, client_id: ClientId, msg: ClientMessage) -> Result<()> {
        match msg {
            ClientMessage::Request(req) => {
                debug!("Received request from {}: {}", client_id, req.method);
                // TODO: Dispatch to router
            }
            ClientMessage::Notification(notif) => {
                debug!("Received notification from {}: {}", client_id, notif.method);
            }
        }
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Notification> {
        self.notify_tx.subscribe()
    }
}
