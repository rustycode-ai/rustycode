use std::sync::Arc;
use rustycode_server_protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcError};
use rustycode_runtime::AsyncRuntime;
use rustycode_tools::indexing::symbols::extract_file;
use std::path::PathBuf;

pub struct RequestRouter {
    _runtime: Arc<AsyncRuntime>,
}

impl RequestRouter {
    pub fn new(runtime: Arc<AsyncRuntime>) -> Self {
        Self { _runtime: runtime }
    }

    pub async fn dispatch(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        match req.method.as_str() {
            "symbol/outline" => self.handle_symbol_outline(req).await,
            _ => JsonRpcResponse::error(req.id, JsonRpcError::new(
                JsonRpcError::METHOD_NOT_FOUND,
                format!("Method not found: {}", req.method)
            )),
        }
    }

    async fn handle_symbol_outline(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let params: rustycode_server_protocol::requests::SymbolOutlineRequest = match serde_json::from_value(req.params) {
            Ok(p) => p,
            Err(_) => return JsonRpcResponse::error(req.id, JsonRpcError::new(JsonRpcError::INVALID_PARAMS, "Invalid params")),
        };

        let path = PathBuf::from(&params.file_path);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return JsonRpcResponse::error(req.id, JsonRpcError::new(JsonRpcError::INTERNAL_ERROR, format!("Failed to read file: {}", e))),
        };

        let outline = extract_file(&path, &content);
        let response = rustycode_server_protocol::responses::SymbolOutlineResponse { outline };
        
        JsonRpcResponse::success(req.id, serde_json::to_value(response).unwrap())
    }
}
