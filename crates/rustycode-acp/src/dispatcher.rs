//! JSON-RPC Dispatcher for ACP
//!
//! Handles parsing, routing, and dispatching of ACP JSON-RPC requests.

use crate::server::ACPServer;
use crate::types::{JsonRpcError, JsonRpcResponse, RequestId};
use anyhow::Result;
use serde_json::Value;
use tracing::{debug, warn};

pub struct ACPDispatcher {
    server: ACPServer,
}

impl ACPDispatcher {
    pub fn new(server: ACPServer) -> Self {
        Self { server }
    }

    pub fn dispatch(&self, request_str: &str) -> Result<JsonRpcResponse<Value>> {
        let raw: Value = serde_json::from_str(request_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;

        let id = raw
            .get("id")
            .and_then(|v| {
                if let Some(n) = v.as_u64() {
                    Some(RequestId::Num(n))
                } else {
                    v.as_str().map(|s| RequestId::Str(s.to_string()))
                }
            })
            .unwrap_or(RequestId::Num(0));

        let method = raw
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'method' field"))?;

        debug!("Dispatching method: {}", method);

        match method {
            "initialize" => self.server.handle_initialize(raw, id),
            "session/new" => self.server.handle_session_new(raw, id),
            "session/load" => self.server.handle_session_load(raw, id),
            "session/prompt" => self.server.handle_session_prompt(raw, id),
            "shutdown" => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(serde_json::json!({})),
                error: None,
            }),
            _ => {
                warn!("Unknown method: {}", method);
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", method),
                        data: None,
                    }),
                })
            }
        }
    }
}
