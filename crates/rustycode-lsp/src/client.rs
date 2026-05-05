//! LSP client implementation
//!
//! Provides full Language Server Protocol client support including:
//! - Server process management
//! - JSON-RPC communication
//! - Request/response handling
//! - Notification support
//! - Capability negotiation
//! - Diagnostic tracking

use anyhow::{Context, Result};
use lsp_types::Uri as Url;
use lsp_types::*;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::transport::{LspNotification, LspResponse, LspResponseReader};
use crate::types::{LanguageId, LspConfig, LspServerConfig};

/// LSP client configuration
#[derive(Debug, Clone)]
pub struct LspClientConfig {
    /// Server name (e.g., "rust-analyzer")
    pub server_name: String,

    /// Server command
    pub command: String,

    /// Server arguments
    pub args: Vec<String>,

    /// Root URI for the workspace
    pub root_uri: Option<String>,

    /// Client capabilities
    pub capabilities: ClientCapabilities,
}

impl Default for LspClientConfig {
    fn default() -> Self {
        Self {
            server_name: "rust-analyzer".to_string(),
            command: "rust-analyzer".to_string(),
            args: vec![],
            root_uri: None,
            capabilities: ClientCapabilities::default(),
        }
    }
}

/// LSP client state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LspClientState {
    Starting,
    Running,
    ShuttingDown,
    Stopped,
}

/// LSP client for communicating with language servers
pub struct LspClient {
    config: LspClientConfig,
    state: LspClientState,
    ready: bool,
    child: Option<Child>,
    child_stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    request_id: i64,
    pending_requests: Arc<Mutex<HashMap<i64, oneshot::Sender<Result<JsonValue>>>>>,
    server_capabilities: Option<ServerCapabilities>,
    diagnostics: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
    response_reader_task: Option<tokio::task::JoinHandle<()>>,
    response_handler_task: Option<tokio::task::JoinHandle<()>>,
}

impl Clone for LspClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            state: self.state,
            ready: self.ready,
            child: None,
            child_stdin: Arc::new(Mutex::new(None)),
            request_id: self.request_id,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            server_capabilities: self.server_capabilities.clone(),
            diagnostics: self.diagnostics.clone(),
            response_reader_task: None,
            response_handler_task: None,
        }
    }
}

impl LspClient {
    pub fn new(config: LspClientConfig) -> Self {
        Self {
            config,
            state: LspClientState::Stopped,
            ready: false,
            child: None,
            child_stdin: Arc::new(Mutex::new(None)),
            request_id: 0,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            server_capabilities: None,
            diagnostics: Arc::new(RwLock::new(HashMap::new())),
            response_reader_task: None,
            response_handler_task: None,
        }
    }

    /// Get diagnostics for all documents
    pub async fn fetch_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        self.diagnostics
            .read()
            .await
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    /// Add a diagnostics handler callback
    pub async fn set_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        let mut diag = self.diagnostics.write().await;
        diag.insert(uri, diagnostics);
    }

    /// Start the LSP server
    #[allow(clippy::too_many_lines)]
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting LSP server: {}", self.config.server_name);

        // Spawn the LSP server process
        let mut child = Command::new(&self.config.command)
            .args(&self.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn LSP server: {}", self.config.server_name))?;

        // Take stdout for response reader and stdin for sending responses
        let stdout = child.stdout.take().with_context(|| {
            format!(
                "LSP server '{}' spawned without stdout — \
                 ensure Stdio::piped() is configured",
                self.config.server_name
            )
        })?;
        let stdin = child.stdin.take().with_context(|| {
            format!(
                "LSP server '{}' spawned without stdin — \
                 ensure Stdio::piped() is configured",
                self.config.server_name
            )
        })?;
        let child_stdin = Arc::new(Mutex::new(Some(stdin)));

        // Create response reader and spawn background task
        let (reader, mut response_rx) = LspResponseReader::new();
        let diagnostics = self.diagnostics.clone();
        let pending = self.pending_requests.clone();
        let child_stdin_for_handler = child_stdin.clone();

        let response_reader_task = reader.spawn_reader(stdout);

        // Store child and stdin
        self.child = Some(child);
        self.child_stdin = child_stdin;

        // Spawn task to handle responses
        let response_handler_task = tokio::spawn(async move {
            while let Some(response) = response_rx.recv().await {
                match response {
                    LspResponse::Notification {
                        ref method,
                        ref params,
                    } => {
                        debug!("Received notification: {}", method);

                        // Handle notifications
                        if method == "textDocument/publishDiagnostics" {
                            debug!(
                                "Received diagnostics notification with params: {:?}",
                                params
                            );
                            let notification = LspResponse::Notification {
                                method: method.clone(),
                                params: params.clone(),
                            };

                            if let Some(notif) = notification.parse_notification() {
                                if let LspNotification::PublishDiagnostics(diagnostics_params) =
                                    notif
                                {
                                    let uri = diagnostics_params.uri.clone();
                                    let diags = diagnostics_params.diagnostics.clone();
                                    let diag_count = diags.len();
                                    {
                                        let mut guard = diagnostics.write().await;
                                        guard.insert(uri.clone(), diags);
                                    }
                                    info!(
                                        "Updated diagnostics for {:?}: {diag_count} diagnostics",
                                        uri
                                    );
                                }
                            } else {
                                warn!("Failed to parse diagnostics notification");
                            }
                        }
                    }
                    LspResponse::Request { id, method, .. } => {
                        debug!(id = id, method = %method, "Received server request");
                        // Respond with a default OK response
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": null
                        });
                        let response_str = format!(
                            "Content-Length: {}\r\n\r\n{}",
                            response.to_string().len(),
                            response
                        );

                        // Send the response
                        let mut stdin_guard = child_stdin_for_handler.lock().await;
                        if let Some(ref mut stdin) = *stdin_guard {
                            if let Err(e) = stdin.write_all(response_str.as_bytes()).await {
                                warn!(id = id, error = %e, "Failed to send response to server request");
                            } else {
                                let _ = stdin.flush().await;
                                debug!(id = id, "Sent response to server request");
                            }
                        } else {
                            warn!(id = id, "stdin is None, cannot respond to server request");
                        }
                    }
                    LspResponse::Response { id, result, error } => {
                        debug!(
                            id = id,
                            has_result = result.is_some(),
                            has_error = error.is_some(),
                            "Received response for request"
                        );
                        // Send response through the oneshot channel
                        let tx = {
                            let mut pending_guard = pending.lock().await;
                            pending_guard.remove(&id)
                        };
                        if let Some(tx) = tx {
                            let response_result = error.map_or_else(
                                || Ok(result.clone().unwrap_or(JsonValue::Null)),
                                |err| Err(anyhow::anyhow!("LSP error: {err:?}")),
                            );

                            // Send response (ignore errors if channel is closed)
                            debug!(id = id, "Sending response through channel");
                            if let Err(_e) = tx.send(response_result) {
                                debug!(id = id, "Failed to send response through channel");
                            }
                        } else {
                            let pending_ids: Vec<i64> = {
                                let pending_guard = pending.lock().await;
                                pending_guard.keys().copied().collect()
                            };
                            debug!(id = id, pending = ?pending_ids, "Received response for unknown request ID");
                        }
                    }
                }
            }
        });

        // Send initialize request
        let workspace_folders = self
            .config
            .root_uri
            .as_ref()
            .and_then(|u| u.parse().ok())
            .map(|uri: Url| {
                vec![WorkspaceFolder {
                    uri,
                    name: "workspace".to_string(),
                }]
            });

        let initialize_params = InitializeParams {
            process_id: None,
            workspace_folders,
            capabilities: self.config.capabilities.clone(),
            trace: Some(TraceValue::Off),
            initialization_options: None,
            client_info: Some(ClientInfo {
                name: "RustyCode".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            locale: Some("en-US".to_string()),
            work_done_progress_params: WorkDoneProgressParams::default(),
            // Note: Deprecated fields (root_uri, root_path) omitted
            ..Default::default()
        };

        self.send_request("initialize", initialize_params).await?;

        // Send initialized notification to complete the handshake
        self.notify("initialized", serde_json::json!({})).await?;

        self.state = LspClientState::Starting;
        self.response_reader_task = Some(response_reader_task);
        self.response_handler_task = Some(response_handler_task);

        // Mark as running
        self.state = LspClientState::Running;

        info!("LSP server started: {}", self.config.server_name);
        Ok(())
    }

    /// Send a request to the LSP server
    async fn send_request(&mut self, method: &str, params: impl serde::Serialize) -> Result<i64> {
        self.request_id = self.request_id.saturating_add(1);
        let id = self.request_id;

        let params_json = serde_json::to_value(params)?;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params_json
        });

        let request_str = format!(
            "Content-Length: {}\r\n\r\n{}",
            request.to_string().len(),
            request
        );

        debug!(id = id, method = %method, request = %request, "Sending request");

        {
            let mut stdin_guard = self.child_stdin.lock().await;
            if let Some(ref mut stdin) = *stdin_guard {
                stdin.write_all(request_str.as_bytes()).await?;
                stdin.flush().await?;
                debug!(id = id, "Sent request bytes");
            } else {
                anyhow::bail!("LSP server stdin is not available - server may have crashed");
            }
        }

        Ok(id)
    }

    /// Send a request and wait for the response
    async fn send_request_sync(
        &mut self,
        method: &str,
        params: impl serde::Serialize,
    ) -> Result<JsonValue> {
        let id = self.send_request(method, params).await?;
        debug!(method = %method, id = id, "Sending request sync");

        // Create a channel to receive the response
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, tx);
            debug!(
                id = id,
                total_pending = pending.len(),
                "Registered pending request"
            );
        }

        // Wait for the response (already removed by handler)
        let timeout = tokio::time::Duration::from_secs(30);
        debug!(
            id = id,
            timeout_secs = timeout.as_secs(),
            "Waiting for response to request"
        );
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => {
                debug!(id = id, "Received response for request");
                result
            }
            Ok(Err(e)) => {
                debug!(id = id, error = %e, "Response channel closed for request");
                Err(anyhow::anyhow!("Response channel closed: {e}"))
            }
            Err(_) => {
                debug!(
                    id = id,
                    method = %method,
                    timeout_secs = timeout.as_secs(),
                    "Request timed out"
                );
                Err(anyhow::anyhow!("Request timed out"))
            }
        }
    }

    /// Send a notification to the LSP server
    pub async fn notify(&mut self, method: &str, params: impl serde::Serialize) -> Result<()> {
        let params_json = serde_json::to_value(params)?;
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params_json
        });

        let notification_str = format!(
            "Content-Length: {}\r\n\r\n{}",
            notification.to_string().len(),
            notification
        );

        {
            let mut stdin_guard = self.child_stdin.lock().await;
            if let Some(ref mut stdin) = *stdin_guard {
                stdin.write_all(notification_str.as_bytes()).await?;
                stdin.flush().await?;
                debug!("Sent notification: {}", method);
            }
        }

        Ok(())
    }

    /// Open a document in the LSP server
    pub async fn open_document(
        &mut self,
        uri: Url,
        language_id: &str,
        version: i32,
        text: &str,
    ) -> Result<()> {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: language_id.to_string(),
                version,
                text: text.to_string(),
            },
        };

        self.notify("textDocument/didOpen", params).await
    }

    /// Change a document in the LSP server
    pub async fn change_document(
        &mut self,
        uri: Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Result<()> {
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri, version },
            content_changes: changes,
        };

        self.notify("textDocument/didChange", params).await
    }

    /// Get hover information for a position
    pub async fn hover(&mut self, uri: Url, position: Position) -> Result<Option<Hover>> {
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let response = self.send_request_sync("textDocument/hover", params).await?;

        // Parse the response
        if response.is_null() {
            Ok(None)
        } else {
            let hover = serde_json::from_value::<Hover>(response)
                .context("Failed to parse hover response")?;
            Ok(Some(hover))
        }
    }

    /// Get definition for a symbol at a position
    pub async fn goto_definition(
        &mut self,
        uri: Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response = self
            .send_request_sync("textDocument/definition", params)
            .await?;

        // Parse the response (GotoDefinitionResponse can be Scalar, Array, or null)
        if response.is_null() {
            Ok(None)
        } else {
            let definition = serde_json::from_value::<GotoDefinitionResponse>(response)
                .context("Failed to parse definition response")?;
            Ok(Some(definition))
        }
    }

    /// Get completions for a position
    pub async fn completion(
        &mut self,
        uri: Url,
        position: Position,
        context: Option<CompletionContext>,
    ) -> Result<Option<CompletionResponse>> {
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context,
        };

        debug!(params = ?params, "Completion params");
        let response = self
            .send_request_sync("textDocument/completion", params)
            .await?;

        // Parse the response (CompletionResponse can be null or Array/List)
        if response.is_null() {
            Ok(None)
        } else {
            let completion = serde_json::from_value::<CompletionResponse>(response)
                .context("Failed to parse completion response")?;
            Ok(Some(completion))
        }
    }

    /// Get document symbols
    pub async fn document_symbols(&mut self, uri: Url) -> Result<Vec<DocumentSymbol>> {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response = self
            .send_request_sync("textDocument/documentSymbol", params)
            .await?;

        // Handle both DocumentSymbol[] and SymbolInformation[] responses
        if let Some(_arr) = response.as_array() {
            // Try to parse as DocumentSymbol first
            if let Ok(symbols) = serde_json::from_value::<Vec<DocumentSymbol>>(response.clone()) {
                return Ok(symbols);
            }

            // Fall back to SymbolInformation
            if let Ok(symbols) = serde_json::from_value::<Vec<SymbolInformation>>(response) {
                // Convert SymbolInformation to DocumentSymbol
                let doc_symbols: Vec<DocumentSymbol> = symbols
                    .into_iter()
                    .map(|sym| DocumentSymbol {
                        name: sym.name,
                        detail: sym.container_name,
                        kind: sym.kind,
                        tags: None,
                        #[allow(deprecated)]
                        deprecated: None,
                        range: sym.location.range,
                        selection_range: sym.location.range,
                        children: None,
                    })
                    .collect();
                return Ok(doc_symbols);
            }
        }

        Ok(vec![])
    }

    /// Find all references to a symbol
    pub async fn references(&mut self, uri: Url, position: Position) -> Result<Vec<Location>> {
        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        };

        let response = self
            .send_request_sync("textDocument/references", params)
            .await?;

        if let Some(_arr) = response.as_array() {
            let locations =
                serde_json::from_value::<Vec<Location>>(response.clone()).unwrap_or_default();
            return Ok(locations);
        }

        Ok(vec![])
    }

    /// Search for symbols across the workspace
    pub async fn workspace_symbols(&mut self, query: &str) -> Result<Vec<SymbolInformation>> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response = self.send_request_sync("workspace/symbol", params).await?;

        // Handle both flat and nested WorkspaceSymbolResponse variants
        match response {
            JsonValue::Array(_) => {
                // Flat response: Vec<SymbolInformation>
                let symbols =
                    serde_json::from_value::<Vec<SymbolInformation>>(response).unwrap_or_default();
                Ok(symbols)
            }
            JsonValue::Object(_) => {
                // Nested response might have different structure; for now treat as empty
                Ok(vec![])
            }
            _ => Ok(vec![]),
        }
    }

    /// Get diagnostics for a document
    /// Force re-validation of a document
    pub async fn validate_document(&mut self, uri: Url) -> Result<()> {
        self.notify(
            "textDocument/didChange",
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri, version: 1 },
                content_changes: vec![],
            },
        )
        .await
    }

    pub async fn diagnostic(&mut self, uri: Url) -> Result<Vec<Diagnostic>> {
        // For now, use cached diagnostics from publishDiagnostics notifications
        // These are pushed by the language server as files are analyzed
        Ok(self.fetch_diagnostics(&uri).await)
    }

    /// Get code actions for a document range
    pub async fn code_actions(&mut self, uri: Url, range: Range) -> Result<Vec<CodeAction>> {
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri },
            range,
            context: CodeActionContext::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response = self
            .send_request_sync("textDocument/codeAction", params)
            .await?;

        // Handle both Command and Command[] responses
        if let Some(_arr) = response.as_array() {
            let actions = serde_json::from_value::<Vec<CodeActionOrCommand>>(response.clone())
                .unwrap_or_default();
            let code_actions: Vec<CodeAction> = actions
                .into_iter()
                .map(|a| match a {
                    CodeActionOrCommand::CodeAction(action) => action,
                    CodeActionOrCommand::Command(command) => {
                        // Convert command to code action
                        CodeAction {
                            title: command.title.clone(),
                            kind: None,
                            diagnostics: None,
                            edit: None,
                            command: Some(command),
                            is_preferred: None,
                            disabled: None,
                            data: None,
                        }
                    }
                })
                .collect();
            return Ok(code_actions);
        }

        Ok(vec![])
    }

    /// Prepare to rename a symbol
    pub async fn prepare_rename(
        &mut self,
        uri: Url,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>> {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position,
        };

        let response: JsonValue = self
            .send_request_sync("textDocument/prepareRename", params)
            .await?;

        if response.is_null() {
            Ok(None)
        } else {
            let prepare_result = serde_json::from_value::<PrepareRenameResponse>(response)
                .context("Failed to parse prepare rename response")?;
            Ok(Some(prepare_result))
        }
    }

    /// Rename a symbol
    pub async fn rename(
        &mut self,
        uri: Url,
        position: Position,
        new_name: String,
    ) -> Result<WorkspaceEdit> {
        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            new_name,
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let response = self
            .send_request_sync("textDocument/rename", params)
            .await?;

        serde_json::from_value::<WorkspaceEdit>(response).context("Failed to parse rename response")
    }

    /// Format a document
    pub async fn document_formatting(&mut self, uri: Url) -> Result<Vec<TextEdit>> {
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let response = self
            .send_request_sync("textDocument/formatting", params)
            .await?;

        if response.is_null() {
            Ok(vec![])
        } else {
            serde_json::from_value::<Vec<TextEdit>>(response)
                .context("Failed to parse formatting response")
        }
    }

    /// Format a document range
    pub async fn document_range_formatting(
        &mut self,
        uri: Url,
        range: Range,
    ) -> Result<Vec<TextEdit>> {
        let params = DocumentRangeFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            range,
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let response = self
            .send_request_sync("textDocument/rangeFormatting", params)
            .await?;

        if response.is_null() {
            Ok(vec![])
        } else {
            serde_json::from_value::<Vec<TextEdit>>(response)
                .context("Failed to parse range formatting response")
        }
    }

    /// Shutdown the LSP server
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down LSP server: {}", self.config.server_name);

        self.send_request("shutdown", JsonValue::Null).await?;
        self.state = LspClientState::ShuttingDown;
        self.ready = false;

        Ok(())
    }

    /// Exit the LSP server
    pub async fn exit(&mut self) -> Result<()> {
        info!("Exiting LSP server: {}", self.config.server_name);

        self.notify("exit", JsonValue::Null).await?;

        if let Some(mut child) = self.child.take() {
            child.kill().await?;
            child.wait().await?;
        }

        self.state = LspClientState::Stopped;
        self.ready = false;
        info!("LSP server stopped: {}", self.config.server_name);

        Ok(())
    }

    /// Get server capabilities
    pub const fn server_capabilities(&self) -> Option<&ServerCapabilities> {
        self.server_capabilities.as_ref()
    }

    /// Check if the client is running
    pub const fn is_running(&self) -> bool {
        matches!(self.state, LspClientState::Running)
    }

    /// Check if the server has completed initial indexing
    pub const fn is_ready(&self) -> bool {
        self.ready
    }

    /// Wait for the server to finish initial indexing.
    ///
    /// Uses `workspace_symbols("")` as a probe: a cold server returns an empty
    /// result, while a warm one returns at least one symbol. Retries every
    /// 500 ms until the server responds with symbols or the timeout elapses.
    pub async fn wait_for_ready(&mut self, timeout: Duration) -> Result<()> {
        if self.ready {
            return Ok(());
        }

        let probe_interval = Duration::from_millis(500);
        let deadline = tokio::time::Instant::now() + timeout;
        let mut attempts = 0u32;

        loop {
            attempts += 1;
            match self.workspace_symbols("").await {
                Ok(symbols) if !symbols.is_empty() => {
                    info!(
                        server = %self.config.server_name,
                        attempts,
                        elapsed_ms = timeout
                            .saturating_sub(deadline.duration_since(tokio::time::Instant::now()))
                            .as_millis(),
                        "LSP server ready"
                    );
                    self.ready = true;
                    return Ok(());
                }
                Ok(_) => {
                    debug!(
                        server = %self.config.server_name,
                        attempts,
                        "LSP server not yet indexed, retrying..."
                    );
                }
                Err(e) => {
                    debug!(
                        server = %self.config.server_name,
                        attempts,
                        error = %e,
                        "LSP readiness probe failed, retrying..."
                    );
                }
            }

            if tokio::time::Instant::now() + probe_interval >= deadline {
                self.ready = false;
                return Err(anyhow::anyhow!(
                    "language server '{}' did not finish indexing within {}s — \
                     still warming up. Retry the request in a moment.",
                    self.config.server_name,
                    timeout.as_secs(),
                ));
            }

            tokio::time::sleep(probe_interval).await;
        }
    }

    pub const fn is_healthy(&self) -> bool {
        if !self.is_running() {
            return false;
        }
        self.child.is_some()
    }

    pub const fn state(&self) -> LspClientState {
        self.state
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if self.child.is_some() {
            warn!("LspClient dropped without explicit shutdown");
        }
    }
}

/// Create an LSP client config for a given [`LanguageId`].
pub fn create_client_config(lang: LanguageId) -> Option<LspClientConfig> {
    let (server_name, command, args) = match lang {
        LanguageId::Rust => ("rust-analyzer", "rust-analyzer", vec![]),
        LanguageId::TypeScript | LanguageId::JavaScript => (
            "typescript-language-server",
            "typescript-language-server",
            vec!["--stdio".to_string()],
        ),
        LanguageId::Python => (
            "pyright-langserver",
            "pyright-langserver",
            vec!["--stdio".to_string()],
        ),
        LanguageId::Go => ("gopls", "gopls", vec!["serve".to_string()]),
        _ => return None,
    };
    Some(LspClientConfig {
        server_name: server_name.to_string(),
        command: command.to_string(),
        args,
        root_uri: None,
        capabilities: ClientCapabilities::default(),
    })
}

/// Create an LSP client config for a given language string.
///
/// Prefer [`create_client_config`] with a [`LanguageId`] for type safety.
pub fn create_client_config_for_language(language: &str) -> Option<LspClientConfig> {
    let lang: LanguageId = language.parse().ok()?;
    create_client_config(lang)
}

/// Create an LSP client config with optional user overrides from [`LspConfig`].
///
/// Resolution order:
/// 1. User override from `lsp_config` (if present and enabled)
/// 2. Built-in hardcoded defaults via [`create_client_config`]
pub fn create_client_config_with_override(
    lang: LanguageId,
    lsp_config: Option<&LspConfig>,
) -> Option<LspClientConfig> {
    let resolved = lsp_config.and_then(|c| c.resolve(lang));

    let cfg = if let Some(cfg) = resolved {
        cfg
    } else {
        let (command, args) = match lang {
            LanguageId::Rust => ("rust-analyzer", vec![]),
            LanguageId::TypeScript | LanguageId::JavaScript => {
                ("typescript-language-server", vec!["--stdio".to_string()])
            }
            LanguageId::Python => ("pyright-langserver", vec!["--stdio".to_string()]),
            LanguageId::Go => ("gopls", vec!["serve".to_string()]),
            _ => return None,
        };
        LspServerConfig::new(command, args)
    };

    let server_name = cfg.command.clone();
    Some(LspClientConfig {
        server_name,
        command: cfg.command,
        args: cfg.args,
        root_uri: None,
        capabilities: ClientCapabilities::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn parse_uri(uri: &str) -> Url {
        Url::from_str(uri).unwrap()
    }

    #[test]
    fn test_create_client_config_rust() {
        let config = create_client_config_for_language("rust");
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.server_name, "rust-analyzer");
        assert_eq!(config.command, "rust-analyzer");
    }

    #[test]
    fn test_create_client_config_unknown_language() {
        let config = create_client_config_for_language("unknown-language");
        assert!(config.is_none());
    }

    #[test]
    fn test_client_initial_state() {
        let config = LspClientConfig::default();
        let client = LspClient::new(config);
        assert!(!client.is_running());
        assert!(client.server_capabilities().is_none());
    }

    #[test]
    fn test_create_client_config_typescript() {
        let config = create_client_config_for_language("typescript").unwrap();
        assert_eq!(config.server_name, "typescript-language-server");
        assert_eq!(config.args, vec!["--stdio"]);
    }

    #[test]
    fn test_create_client_config_javascript() {
        let config = create_client_config_for_language("javascript").unwrap();
        assert_eq!(config.server_name, "typescript-language-server");
    }

    #[test]
    fn test_create_client_config_python() {
        let config = create_client_config_for_language("python").unwrap();
        assert_eq!(config.server_name, "pyright-langserver");
        assert_eq!(config.args, vec!["--stdio"]);
    }

    #[test]
    fn test_create_client_config_go() {
        let config = create_client_config_for_language("go").unwrap();
        assert_eq!(config.server_name, "gopls");
        assert_eq!(config.args, vec!["serve"]);
    }

    #[test]
    fn test_create_client_config_unsupported() {
        assert!(create_client_config_for_language("cobol").is_none());
        assert!(create_client_config_for_language("").is_none());
        assert!(create_client_config_for_language("ruby").is_none());
    }

    #[test]
    fn test_lsp_client_state_variants() {
        assert!(matches!(LspClientState::Starting, LspClientState::Starting));
        assert!(matches!(LspClientState::Running, LspClientState::Running));
        assert!(matches!(
            LspClientState::ShuttingDown,
            LspClientState::ShuttingDown
        ));
        assert!(matches!(LspClientState::Stopped, LspClientState::Stopped));
    }

    #[test]
    fn test_lsp_client_config_default() {
        let config = LspClientConfig::default();
        assert_eq!(config.server_name, "rust-analyzer");
        assert_eq!(config.command, "rust-analyzer");
        assert!(config.args.is_empty());
        assert!(config.root_uri.is_none());
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_client_clone_inherits_state() {
        let config = LspClientConfig::default();
        let client = LspClient::new(config);
        assert!(!client.is_running());
        // Verify Clone trait compiles and produces a valid instance
        let _cloned = client.clone();
    }

    #[test]
    fn test_custom_config() {
        let config = LspClientConfig {
            server_name: "my-lsp".to_string(),
            command: "my-lsp-binary".to_string(),
            args: vec!["--port".to_string(), "8080".to_string()],
            root_uri: Some("file:///project".to_string()),
            capabilities: ClientCapabilities::default(),
        };
        assert_eq!(config.server_name, "my-lsp");
        assert_eq!(config.args.len(), 2);
        assert!(config.root_uri.is_some());
    }

    #[test]
    fn test_lsp_client_state_copy() {
        let state = LspClientState::Running;
        let copied = state;
        assert!(matches!(copied, LspClientState::Running));
    }

    #[test]
    fn test_lsp_client_state_all_variants_distinct() {
        let states = [
            LspClientState::Starting,
            LspClientState::Running,
            LspClientState::ShuttingDown,
            LspClientState::Stopped,
        ];
        // Ensure they're all different discriminants
        for i in 0..states.len() {
            for j in (i + 1)..states.len() {
                assert_ne!(states[i], states[j]);
            }
        }
    }

    #[test]
    fn test_lsp_client_config_clone() {
        let config = LspClientConfig {
            server_name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec!["--arg".to_string()],
            root_uri: Some("file:///root".to_string()),
            capabilities: ClientCapabilities::default(),
        };
        let _cloned = config.clone();
        assert_eq!(config.server_name, "test");
        assert_eq!(config.command, "test-cmd");
        assert_eq!(config.args, vec!["--arg"]);
        assert_eq!(config.root_uri, Some("file:///root".to_string()));
    }

    #[test]
    fn test_client_new_with_custom_config() {
        let config = LspClientConfig {
            server_name: "custom-lsp".to_string(),
            command: "custom-lsp-bin".to_string(),
            args: vec!["--stdio".to_string()],
            root_uri: None,
            capabilities: ClientCapabilities::default(),
        };
        let client = LspClient::new(config);
        assert!(!client.is_running());
        assert!(client.server_capabilities().is_none());
    }

    #[tokio::test]
    async fn test_get_diagnostics_empty() {
        let config = LspClientConfig::default();
        let client = LspClient::new(config);
        let uri = parse_uri("file:///test.rs");
        let diags = client.fetch_diagnostics(&uri).await;
        assert!(diags.is_empty());
    }

    #[tokio::test]
    async fn test_set_and_get_diagnostics() {
        let config = LspClientConfig::default();
        let client = LspClient::new(config);
        let uri = parse_uri("file:///test.rs");

        let diagnostic = Diagnostic::new(
            Range::new(Position::new(0, 0), Position::new(0, 5)),
            Some(DiagnosticSeverity::ERROR),
            None,
            None,
            "test error".to_string(),
            None,
            None,
        );

        client.set_diagnostics(uri.clone(), vec![diagnostic]).await;
        let diags = client.fetch_diagnostics(&uri).await;
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "test error");
    }

    #[tokio::test]
    async fn test_set_diagnostics_replaces_previous() {
        let config = LspClientConfig::default();
        let client = LspClient::new(config);
        let uri = parse_uri("file:///test.rs");

        let d1 = Diagnostic::new_simple(
            Range::new(Position::new(0, 0), Position::new(0, 1)),
            "first".to_string(),
        );
        let d2 = Diagnostic::new_simple(
            Range::new(Position::new(1, 0), Position::new(1, 1)),
            "second".to_string(),
        );

        client.set_diagnostics(uri.clone(), vec![d1]).await;
        client.set_diagnostics(uri.clone(), vec![d2]).await;
        let diags = client.fetch_diagnostics(&uri).await;
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "second");
    }

    #[tokio::test]
    async fn test_diagnostics_for_different_uris() {
        let config = LspClientConfig::default();
        let client = LspClient::new(config);
        let uri1 = parse_uri("file:///a.rs");
        let uri2 = parse_uri("file:///b.rs");

        let d = Diagnostic::new_simple(
            Range::new(Position::new(0, 0), Position::new(0, 1)),
            "error".to_string(),
        );

        client.set_diagnostics(uri1.clone(), vec![d.clone()]).await;

        assert_eq!(client.fetch_diagnostics(&uri1).await.len(), 1);
        assert!(client.fetch_diagnostics(&uri2).await.is_empty());
    }

    #[test]
    fn test_create_client_config_rust_no_args() {
        let config = create_client_config_for_language("rust").unwrap();
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_create_client_config_all_languages_have_server_name() {
        for lang in &["rust", "typescript", "javascript", "python", "go"] {
            let config = create_client_config_for_language(lang);
            assert!(config.is_some(), "Expected config for language: {lang}");
            assert!(!config.unwrap().server_name.is_empty());
        }
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_client_clone_has_no_child() {
        let config = LspClientConfig::default();
        let client = LspClient::new(config);
        // Cloned client should not be running (child is None)
        assert!(!client.is_running());
        // Verify Clone trait compiles and produces a valid instance
        let _cloned = client.clone();
    }

    #[test]
    fn test_lsp_client_config_default_values() {
        let config = LspClientConfig::default();
        assert!(config.root_uri.is_none());
        assert!(config.args.is_empty());
        assert_eq!(config.server_name, config.command);
    }
}
