use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::{McpError, McpResult};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::transport::{IncomingMessage, Transport};

/// SSE-based transport for MCP (GET to establish, POST for client messages)
#[allow(dead_code)]
pub struct SseTransport {
    client: Client,
    url: String,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    connected: bool,
    // For simplicity in tests, we reuse a basic inbox channel
    inbox: mpsc::Receiver<IncomingMessage>,
    inbox_sender: Option<mpsc::Sender<IncomingMessage>>,

    sse_endpoint: String,
}

impl SseTransport {
    pub fn new(url: &str, headers: HashMap<String, String>) -> McpResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| McpError::TransportError(format!("HTTP client error: {e}")))?;

        let (tx, rx) = mpsc::channel(100);

        Ok(Self {
            client,
            url: url.to_string(),
            headers,
            session_id: None,
            connected: true,
            inbox: rx,
            inbox_sender: Some(tx),
            sse_endpoint: url.to_string(),
        })
    }

    #[cfg(test)]
    pub fn test_set_sse_endpoint(&mut self, endpoint: String) {
        self.sse_endpoint = endpoint;
    }
}

#[async_trait]
impl Transport for SseTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        // Serialize the request
        let json = request
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {e}")))?;

        let mut req = self
            .client
            .post(&self.sse_endpoint)
            .header("Content-Type", "application/json");

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }
        req = req.body(json);

        let resp = req
            .send()
            .await
            .map_err(|e| McpError::TransportError(format!("SSE POST failed: {e}")))?;

        if let Some(val) = resp.headers().get("Mcp-Session-Id") {
            if let Ok(s) = val.to_str() {
                self.session_id = Some(s.to_string());
            }
        }

        if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(ct_str) = ct.to_str() {
                if ct_str.contains("application/json") {
                    let text = resp
                        .text()
                        .await
                        .map_err(|e| McpError::TransportError(format!("SSE read error: {e}")))?;
                    let json_resp = JsonRpcResponse::from_json(&text)
                        .map_err(|e| McpError::ProtocolError(format!("Invalid JSON: {e}")))?;
                    return Ok(json_resp);
                }
            }
        }
        Err(McpError::TransportError(
            "SSE endpoint did not return JSON".to_string(),
        ))
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        let json = notification
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {e}")))?;

        let mut req = self
            .client
            .post(&self.sse_endpoint)
            .header("Content-Type", "application/json");
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }
        req = req.body(json);

        let _resp = req
            .send()
            .await
            .map_err(|e| McpError::TransportError(format!("SSE POST failed: {e}")))?;
        Ok(())
    }

    async fn receive(&mut self) -> McpResult<IncomingMessage> {
        match self.inbox.recv().await {
            Some(m) => Ok(m),
            None => Err(McpError::ConnectionClosed),
        }
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn close(&mut self) -> McpResult<()> {
        self.connected = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_sse_transport_new_builds_client() {
        let headers: HashMap<String, String> = HashMap::new();
        let t = SseTransport::new("http://example.invalid/mcp", headers).unwrap();
        assert!(t.is_connected());
    }
}
