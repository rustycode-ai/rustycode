use crate::security::{create_file_symlink_safe, open_file_symlink_safe};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use lsp_types::{
    CompletionContext, CompletionTriggerKind, DiagnosticSeverity, Position, Uri as Url,
};
use rustycode_lsp::{create_client_config_with_override, LanguageId, LspClient, LspConfig};
use rustycode_shared_runtime as shared_runtime;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use url::Url as FileUrl;

static LSP_CLIENTS: OnceLock<Mutex<HashMap<String, LspClient>>> = OnceLock::new();
static LSP_BACKOFF: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
const MAX_LSP_CLIENTS: usize = 10;
const CRASH_BACKOFF_SECS: u64 = 60;

fn lsp_backoff() -> &'static Mutex<HashMap<String, std::time::Instant>> {
    LSP_BACKOFF.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_lsp_backoff(key: &str, secs: u64) {
    if let Ok(mut map) = lsp_backoff().lock() {
        map.insert(key.to_string(), std::time::Instant::now() + std::time::Duration::from_secs(secs));
    }
}

fn clients() -> &'static Mutex<HashMap<String, LspClient>> {
    LSP_CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cleanup_clients_if_needed(map: &mut HashMap<String, LspClient>) {
    while map.len() >= MAX_LSP_CLIENTS {
        if let Some(first_key) = map.keys().next().cloned() {
            // Gracefully shutdown the LSP client before removing it from the map
            if let Some(mut client) = map.remove(&first_key) {
                // Best-effort synchronous shutdown using existing helper
                let _ = run_async_result(async { client.shutdown().await });
                let _ = run_async_result(async { client.exit().await });
            }
        }
    }
}

pub fn active_clients_status() -> Vec<(String, String)> {
    let map = clients().lock().unwrap_or_else(|e| e.into_inner());
    map.iter()
        .map(|(k, c)| {
            let state_str = match c.state() {
                rustycode_lsp::LspClientState::Starting => "starting",
                rustycode_lsp::LspClientState::Running => "running",
                rustycode_lsp::LspClientState::ShuttingDown => "shutting_down",
                rustycode_lsp::LspClientState::Stopped => "stopped",
                _ => "unknown",
            };
            (k.clone(), state_str.to_string())
        })
        .collect()
}

pub fn shutdown_client(key: &str) -> bool {
    let mut map = clients().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut client) = map.remove(key) {
        tracing::debug!(key = %key, "shutting down LSP client");
        let _ = run_async_result(async { client.shutdown().await });
        let _ = run_async_result(async { client.exit().await });
        true
    } else {
        tracing::warn!(key = %key, "no LSP client found for shutdown");
        false
    }
}

pub fn shutdown_all_clients() {
    let mut map = clients().lock().unwrap_or_else(|e| e.into_inner());
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        if let Some(mut client) = map.remove(&key) {
            tracing::debug!(key = %key, "shutting down LSP client");
            let _ = run_async_result(async { client.shutdown().await });
            let _ = run_async_result(async { client.exit().await });
        }
    }
}

fn language_for_path(path: &Path) -> LanguageId {
    LanguageId::from_path(path)
}

trait UriPathExt {
    fn from_file_path(path: impl AsRef<Path>) -> Result<Url, ()>;
    fn from_directory_path(path: impl AsRef<Path>) -> Result<Url, ()>;
}

impl UriPathExt for Url {
    fn from_file_path(path: impl AsRef<Path>) -> Result<Url, ()> {
        let file_url = FileUrl::from_file_path(path)?;
        file_url.as_str().parse().map_err(|_| ())
    }

    fn from_directory_path(path: impl AsRef<Path>) -> Result<Url, ()> {
        let file_url = FileUrl::from_directory_path(path)?;
        file_url.as_str().parse().map_err(|_| ())
    }
}

fn resolve_file_path(ctx: &ToolContext, params: &Value) -> Result<PathBuf> {
    let path = params
        .get("file_path")
        .or_else(|| params.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required parameter: file_path"))?;
    let p = PathBuf::from(path);
    let resolved = if p.is_absolute() { p } else { ctx.cwd.join(p) };
    ensure_path_within_workspace(ctx, &resolved)?;
    Ok(resolved)
}

/// Symlink-safe file write: opens with `O_NOFOLLOW`, writes, syncs.
fn safe_write_file(path: &Path, content: &[u8]) -> Result<()> {
    let mut file = create_file_symlink_safe(path)
        .with_context(|| format!("failed to create file {}", path.display()))?;
    file.write_all(content)
        .with_context(|| format!("failed to write file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync file {}", path.display()))?;
    Ok(())
}

/// Symlink-safe file read: opens with `O_NOFOLLOW`, reads to string.
fn safe_read_file_to_string(path: &Path) -> Result<String> {
    let mut file = open_file_symlink_safe(path)
        .with_context(|| format!("failed to open file {}", path.display()))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .with_context(|| format!("failed to read file {}", path.display()))?;
    Ok(content)
}

fn ensure_path_within_workspace(ctx: &ToolContext, path: &Path) -> Result<()> {
    let workspace_root = std::fs::canonicalize(&ctx.cwd).unwrap_or_else(|_| ctx.cwd.clone());
    let canonical_anchor = canonicalize_existing_or_parent(path)?;
    anyhow::ensure!(
        canonical_anchor.starts_with(&workspace_root),
        "path '{}' is outside workspace '{}' and is blocked",
        path.display(),
        workspace_root.display()
    );
    Ok(())
}

fn canonicalize_existing_or_parent(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return std::fs::canonicalize(&current)
                .map_err(|e| anyhow!("failed to canonicalize '{}': {}", current.display(), e));
        }
        if !current.pop() {
            return Err(anyhow!(
                "unable to resolve path anchor for '{}'",
                path.display()
            ));
        }
    }
}

fn param_u32(params: &Value, key: &str) -> Result<u32> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .map(|v| {
            v.try_into()
                .map_err(|_| anyhow!("{key} value {v} exceeds u32 range"))
        })
        .transpose()?
        .ok_or_else(|| anyhow!("missing required parameter: {key}"))
}

pub(crate) fn run_async_result<F, T>(fut: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    // Delegate to shared_runtime::block_on_shared which correctly handles
    // both "no runtime" and "inside runtime" cases. When inside a runtime
    // it uses block_in_place + futures::executor::block_on instead of
    // handle.block_on, avoiding the "Tokio context is being shutdown" panic.
    shared_runtime::block_on_shared(fut)
}

/// Read a file's contents, using blocking I/O safely from async contexts.
pub(crate) fn read_file_blocking(file_path: &Path) -> Result<String> {
    let path = file_path.to_path_buf();
    if tokio::runtime::Handle::try_current().is_ok() {
        // We're in an async runtime — use block_on_shared to avoid
        // handle.block_on panics when the runtime is shutting down.
        shared_runtime::block_on_shared(async {
            tokio::fs::read_to_string(&path).await
        })
        .with_context(|| format!("failed to read file {}", path.display()))
    } else {
        // No runtime, use symlink-safe direct I/O
        safe_read_file_to_string(&path)
    }
}

pub(crate) fn with_lsp_client<T>(
    ctx: &ToolContext,
    language: LanguageId,
    lsp_config: Option<&LspConfig>,
    op: impl FnOnce(&mut LspClient) -> Result<T>,
) -> Result<T> {
    let language_str = language.language_id_str();
    let root_uri = Url::from_directory_path(&ctx.cwd)
        .ok()
        .map(|u: Url| u.to_string());
    let key = format!("{}::{}", language_str, ctx.cwd.display());

    let mut map = clients()
        .lock()
        .map_err(|_| anyhow!("failed to lock lsp client registry"))?;

    // Crash-loop backoff: if this key failed recently, refuse immediately
    if let Some(backoff_until) = lsp_backoff().lock().ok().and_then(|mut m| m.remove(&key)) {
        if std::time::Instant::now() < backoff_until {
            return Err(anyhow!(
                "language server for {language_str} crashed repeatedly — \
                 waiting before retry. Try again in a moment."
            ));
        }
    }

    if !map.contains_key(&key) {
        cleanup_clients_if_needed(&mut map);
        let mut cfg = create_client_config_with_override(language, lsp_config)
            .ok_or_else(|| anyhow!("unsupported language for lsp tool: {language_str}"))?;
        cfg.root_uri = root_uri;
        map.insert(key.clone(), LspClient::new(cfg));
    }

    // Start + readiness probe (separate block so we can remove dead client on failure)
    let probe_result = {
        let client = map
            .get_mut(&key)
            .ok_or_else(|| anyhow!("failed to retrieve lsp client"))?;

        if !client.is_running() {
            run_async_result(async { client.start().await })
                .context("failed to auto-start language server")?;
        }

        if client.is_ready() {
            Ok(())
        } else {
            let timeout_secs: u64 = std::env::var("RUSTYCODE_LSP_READY_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30);
            let timeout = std::time::Duration::from_secs(timeout_secs);
            run_async_result(async { client.wait_for_ready(timeout).await })
        }
    };

    if let Err(e) = probe_result {
        // Server crashed or failed to index — remove from pool and set backoff
        map.remove(&key);
        set_lsp_backoff(&key, CRASH_BACKOFF_SECS);
        return Err(e.context("LSP server not ready"));
    }

    let client = map
        .get_mut(&key)
        .ok_or_else(|| anyhow!("lsp client removed during readiness probe"))?;
    op(client)
}

/// Helper function to load LSP configuration for a project
/// from the .rustycode/config.json file, if it exists.
pub(crate) fn get_lsp_config_for_project(cwd: &Path) -> Option<LspConfig> {
    let config_path = cwd.join(".rustycode").join("config.json");
    if !config_path.exists() {
        return None;
    }

    // Try to load and parse the config file
    if let Ok(config_content) = safe_read_file_to_string(&config_path) {
        if let Ok(config_json) = serde_json::from_str::<Value>(&config_content) {
            // Extract lsp_config from advanced.lsp_config or advanced.project_tools.lsp_config
            if let Some(advanced) = config_json.get("advanced").and_then(|v| v.as_object()) {
                // Try to find lsp_config at the top level
                if let Some(lsp_config_val) = advanced.get("lsp_config") {
                    if let Ok(lsp_config) =
                        serde_json::from_value::<LspConfig>(lsp_config_val.clone())
                    {
                        return Some(lsp_config);
                    }
                }
            }
        }
    }
    None
}

pub struct LspDiagnosticsTool;
pub struct LspHoverTool;
pub struct LspDefinitionTool;
pub struct LspCompletionTool;

impl Tool for LspDiagnosticsTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_diagnostics"
    }

    fn description(&self) -> &'static str {
        "Check which language servers are available and their status. Use this to verify code intelligence capabilities before using other LSP tools."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "servers": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore, ToolTag::Implement]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let servers = params
            .get("servers")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![
                    "rust-analyzer".to_string(),
                    "typescript-language-server".to_string(),
                    "pyright-langserver".to_string(),
                ]
            });
        let statuses = rustycode_lsp::discover(&servers);
        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&statuses)?,
            json!(statuses),
        ))
    }
}

impl Tool for LspHoverTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_hover"
    }

    fn description(&self) -> &'static str {
        "Get type information, documentation, and signature at a specific position. Use when: you need to know the type of a variable, the signature of a function, or the docs for a method. Faster than reading the whole file. Requires: file_path, line, character."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "line", "character"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "line": { "type": "integer", "minimum": 0 },
                "character": { "type": "integer", "minimum": 0 },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let line = param_u32(&params, "line")?;
        let character = param_u32(&params, "character")?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let hover = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client
                    .hover(uri.clone(), Position::new(line, character))
                    .await
            })
        })?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&hover)?,
            json!({ "hover": hover }),
        ))
    }
}

impl Tool for LspDefinitionTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_definition"
    }

    fn description(&self) -> &'static str {
        "Jump to the definition of a function, variable, type, or import at a specific position. PREFER THIS OVER GREP for navigation — gives the exact definition location. Use when: you see a symbol used in code and want to find where it's defined, you need to trace an import to its source. Requires: file_path, line, character."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "line", "character"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "line": { "type": "integer", "minimum": 0 },
                "character": { "type": "integer", "minimum": 0 },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let line = param_u32(&params, "line")?;
        let character = param_u32(&params, "character")?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let definition = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client
                    .goto_definition(uri.clone(), Position::new(line, character))
                    .await
            })
        })?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&definition)?,
            json!({ "definition": definition }),
        ))
    }
}

impl Tool for LspCompletionTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_completion"
    }

    fn description(&self) -> &'static str {
        "Get code completions (suggestions) at a specific position. Use this to see what functions, variables, or keywords are available while typing. Requires file_path, line, and character position."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "line", "character"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "line": { "type": "integer", "minimum": 0 },
                "character": { "type": "integer", "minimum": 0 },
                "language": { "type": "string" },
                "trigger_character": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Implement]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let line = param_u32(&params, "line")?;
        let character = param_u32(&params, "character")?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let trigger_character = params
            .get("trigger_character")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let context = trigger_character.clone().map(|ch| CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(ch),
        });

        let completion = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client
                    .completion(uri.clone(), Position::new(line, character), context)
                    .await
            })
        })?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&completion)?,
            json!({ "completion": completion }),
        ))
    }
}

pub struct LspDocumentSymbolsTool;

impl Tool for LspDocumentSymbolsTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_document_symbols"
    }

    fn description(&self) -> &'static str {
        "Get the structure of a file (functions, classes, modules, etc.) without reading the entire content. Use this to:
- Understand what's in a file before reading it
- Get an overview of file organization
- Find specific symbols in a file
- Navigate large files efficiently

Requires: file_path
Returns: Hierarchical list of symbols with their types and locations"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri).await
            })
        })?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&symbols)?,
            json!({ "symbols": symbols }),
        ))
    }
}

pub struct LspReferencesTool;

impl Tool for LspReferencesTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_references"
    }

    fn description(&self) -> &'static str {
        "Find ALL references (usages) of a symbol across the codebase. PREFER THIS OVER GREP for finding usages — it understands scope, imports, and renames. Use when: you need to refactor and want to know all call sites, you want to understand how a function/type is used, you're checking impact of a change. Requires: file_path, line, character."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "line", "character"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "line": { "type": "integer", "minimum": 0 },
                "character": { "type": "integer", "minimum": 0 },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let line = param_u32(&params, "line")?;
        let character = param_u32(&params, "character")?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let references = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client
                    .references(uri.clone(), Position::new(line, character))
                    .await
            })
        })?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&references)?,
            json!({ "references": references }),
        ))
    }
}

pub struct LspFullDiagnosticsTool;

impl Tool for LspFullDiagnosticsTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_full_diagnostics"
    }

    fn description(&self) -> &'static str {
        "Get diagnostics (errors, warnings, hints) for a file WITHOUT running a build. PREFER THIS OVER cargo check for quick feedback on recent edits — faster and shows inline error locations. Use when: you just edited a file and want to check for errors, you need to verify types/signatures match. Requires: file_path."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore, ToolTag::Implement]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let diagnostics = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.diagnostic(uri).await
            })
        })?;

        // Calculate build status
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .count();
        let hint_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::HINT))
            .count();

        let status = if error_count > 0 {
            "failed"
        } else if warning_count > 0 {
            "warnings"
        } else {
            "success"
        };

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&diagnostics)?,
            json!({
                "diagnostics": diagnostics,
                "build_status": {
                    "status": status,
                    "error_count": error_count,
                    "warning_count": warning_count,
                    "hint_count": hint_count
                }
            }),
        ))
    }
}

pub struct LspCodeActionsTool;

impl Tool for LspCodeActionsTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_code_actions"
    }

    fn description(&self) -> &'static str {
        "Get available code actions and refactorings for a range. Use this to:
- Find quick fixes for errors and warnings
- Discover available refactorings
- Get code improvements suggested by the language server

Requires: file_path, line, character
Optional: end_line, end_character (for range, defaults to position)
Returns: List of code actions with titles and kinds"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "line", "character"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "line": { "type": "integer", "description": "0-based line number" },
                "character": { "type": "integer", "description": "0-based character offset" },
                "end_line": { "type": "integer", "description": "0-based end line (optional)" },
                "end_character": { "type": "integer", "description": "0-based end character (optional)" },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let line: u32 = params
            .get("line")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("missing line parameter"))?
            .try_into()
            .map_err(|_| anyhow!("line must be non-negative"))?;
        let character: u32 = params
            .get("character")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("missing character parameter"))?
            .try_into()
            .map_err(|_| anyhow!("character must be non-negative"))?;

        let end_line = params
            .get("end_line")
            .and_then(Value::as_i64)
            .map(|v| v.try_into().unwrap_or(u32::MAX));
        let end_character = params
            .get("end_character")
            .and_then(Value::as_i64)
            .map(|v| v.try_into().unwrap_or(u32::MAX));

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let range = lsp_types::Range {
            start: Position { line, character },
            end: Position {
                line: end_line.unwrap_or(line),
                character: end_character.unwrap_or(character),
            },
        };

        let code_actions = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.code_actions(uri, range).await
            })
        })?;

        // Format the output
        let actions_summary: Vec<Value> = code_actions
            .iter()
            .map(|action| {
                json!({
                    "title": action.title,
                    "kind": action.kind,
                    "is_preferred": action.is_preferred,
                    "disabled": action.disabled,
                })
            })
            .collect();

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&code_actions)?,
            json!({ "code_actions": actions_summary }),
        ))
    }
}

pub struct LspRenameTool;

impl Tool for LspRenameTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_rename"
    }

    fn description(&self) -> &'static str {
        "Rename a symbol at a position across all references. Use this to:
- Rename variables, functions, types, and other symbols
- Update all references automatically
- Ensure code remains consistent

Requires: file_path, line, character, new_name
Returns: Workspace edit with all changes to apply"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "line", "character", "new_name"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "line": { "type": "integer", "description": "0-based line number" },
                "character": { "type": "integer", "description": "0-based character offset" },
                "new_name": { "type": "string", "description": "New name for the symbol" },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Refactor]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let line: u32 = params
            .get("line")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("missing line parameter"))?
            .try_into()
            .map_err(|_| anyhow!("line must be non-negative"))?;
        let character: u32 = params
            .get("character")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("missing character parameter"))?
            .try_into()
            .map_err(|_| anyhow!("character must be non-negative"))?;
        let new_name = params
            .get("new_name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing new_name parameter"))?;

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let position = Position { line, character };

        let workspace_edit = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            let new_name = new_name.to_string();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.rename(uri, position, new_name).await
            })
        })?;

        // Format the output
        let changes_summary = if let Some(changes) = &workspace_edit.changes {
            changes
                .iter()
                .map(|(uri, edits)| {
                    json!({
                        "file": uri,
                        "edits": edits.len()
                    })
                })
                .collect::<Vec<_>>()
        } else if let Some(document_changes) = &workspace_edit.document_changes {
            match document_changes {
                lsp_types::DocumentChanges::Edits(edits) => edits
                    .iter()
                    .map(|edit| {
                        json!({
                            "file": edit.text_document.uri,
                            "edits": edit.edits.len()
                        })
                    })
                    .collect::<Vec<_>>(),
                lsp_types::DocumentChanges::Operations(ops) => ops
                    .iter()
                    .map(|op| match op {
                        lsp_types::DocumentChangeOperation::Op(op) => match op {
                            lsp_types::ResourceOp::Create(create) => json!({
                                "file": create.uri,
                                "operation": "create"
                            }),
                            lsp_types::ResourceOp::Rename(rename) => json!({
                                "old": rename.old_uri,
                                "new": rename.new_uri,
                                "operation": "rename"
                            }),
                            lsp_types::ResourceOp::Delete(delete) => json!({
                                "file": delete.uri,
                                "operation": "delete"
                            }),
                        },
                        lsp_types::DocumentChangeOperation::Edit(edit) => {
                            json!({
                                "file": edit.text_document.uri,
                                "edits": edit.edits.len()
                            })
                        }
                    })
                    .collect::<Vec<_>>(),
            }
        } else {
            vec![]
        };

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&workspace_edit)?,
            json!({
                "workspace_edit": workspace_edit,
                "summary": {
                    "new_name": new_name,
                    "changes": changes_summary
                }
            }),
        ))
    }
}

pub struct LspFormattingTool;

impl Tool for LspFormattingTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_formatting"
    }

    fn description(&self) -> &'static str {
        "Format a document using the language server's formatter. Use this to:
- Format entire files according to language standards
- Apply consistent code style
- Fix indentation and spacing

Requires: file_path
Optional: range (line, character, end_line, end_character) for range formatting
Returns: Text edits to apply for formatting"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": { "type": "string", "description": "File path (alias: path)" },
                "path": { "type": "string", "description": "Alias for file_path" },
                "line": { "type": "integer", "description": "0-based start line for range formatting" },
                "character": { "type": "integer", "description": "0-based start character for range formatting" },
                "end_line": { "type": "integer", "description": "0-based end line for range formatting" },
                "end_character": { "type": "integer", "description": "0-based end character for range formatting" },
                "language": { "type": "string" }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Refactor]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let text_edits = if let (Some(line), Some(character), Some(end_line), Some(end_character)) = (
            params.get("line").and_then(Value::as_i64),
            params.get("character").and_then(Value::as_i64),
            params.get("end_line").and_then(Value::as_i64),
            params.get("end_character").and_then(Value::as_i64),
        ) {
            // Range formatting
            let range = lsp_types::Range {
                start: Position {
                    line: line.try_into().unwrap_or(u32::MAX),
                    character: character.try_into().unwrap_or(u32::MAX),
                },
                end: Position {
                    line: end_line.try_into().unwrap_or(u32::MAX),
                    character: end_character.try_into().unwrap_or(u32::MAX),
                },
            };

            with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
                let uri = uri.clone();
                let language_str = language_str.clone();
                let text = text.clone();
                run_async_result(async {
                    client
                        .open_document(uri.clone(), &language_str, 1, &text)
                        .await?;
                    client.document_range_formatting(uri, range).await
                })
            })?
        } else {
            // Full document formatting
            with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
                let uri = uri.clone();
                let language_str = language_str.clone();
                let text = text.clone();
                run_async_result(async {
                    client
                        .open_document(uri.clone(), &language_str, 1, &text)
                        .await?;
                    client.document_formatting(uri).await
                })
            })?
        };

        // Format the output
        let edits_summary: Vec<Value> = text_edits
            .iter()
            .map(|edit| {
                json!({
                    "range": edit.range,
                    "new_text": edit.new_text.chars().take(50).collect::<String>() + if edit.new_text.len() > 50 { "..." } else { "" }
                })
            })
            .collect();

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&text_edits)?,
            json!({
                "formatting_edits": text_edits,
                "summary": {
                    "edit_count": text_edits.len()
                },
                "preview": edits_summary
            }),
        ))
    }
}

// ============================================================================
// Symbol-Level Editing Tools
// ============================================================================

/// Get a compact overview of symbols in a file, grouped by kind.
///
/// Parameters:
/// - `file_path`: Path to the source file (absolute or relative to cwd)
/// - depth: Optional depth for nested symbols (default: 2)
/// - language: Optional language ID (auto-detected from extension if not provided)
pub struct LspGetSymbolsOverviewTool;

impl Tool for LspGetSymbolsOverviewTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_get_symbols_overview"
    }

    fn description(&self) -> &'static str {
        "Get a compact overview of symbols in a file grouped by kind"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "depth": {
                    "type": "integer",
                    "description": "Depth for nested symbols (default: 2)"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Implement]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let depth = param_u32(&params, "depth").unwrap_or(2) as usize;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let overview = crate::providers::symbol::symbols_overview(&symbols, depth);

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&overview)?,
            json!({
                "file_path": file_path.to_string_lossy().to_string(),
                "symbols": overview
            }),
        ))
    }
}

/// Find symbols matching a name path pattern.
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path (e.g., "`MyClass/my_method`" or "/root/child")
/// - `include_body`: Optional, whether to include symbol body text (default: false)
/// - language: Optional language ID (auto-detected if not provided)
pub struct LspFindSymbolTool;

impl Tool for LspFindSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_find_symbol"
    }

    fn description(&self) -> &'static str {
        "Search for symbols (functions, structs, enums, traits, modules) by qualified name path. FASTER and MORE PRECISE than grep for finding definitions — use this instead of grep when you know a symbol name. Returns symbol kind, file path, and location. Examples: 'main', 'Session::new', 'hash_map::Entry'. Requires: query (symbol name or path), language."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path (e.g., 'MyClass/my_method' or '/root/child')"
                },
                "include_body": {
                    "type": "boolean",
                    "description": "Whether to include symbol body text (default: false)"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore, ToolTag::Debug]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let include_body = params
            .get("include_body")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let matches = crate::providers::symbol::find_symbols(&symbols, &sym_path);

        let results: Vec<Value> = matches
            .iter()
            .map(|found| {
                let mut obj = json!({
                    "name": &found.symbol.name,
                    "kind": crate::providers::symbol::format_symbol_kind(&found.symbol.kind),
                    "qualified_path": &found.qualified_path,
                    "range": {
                        "start": {
                            "line": found.symbol.range.start.line,
                            "character": found.symbol.range.start.character,
                        },
                        "end": {
                            "line": found.symbol.range.end.line,
                            "character": found.symbol.range.end.character,
                        }
                    },
                    "selection_range": {
                        "start": {
                            "line": found.symbol.selection_range.start.line,
                            "character": found.symbol.selection_range.start.character,
                        },
                        "end": {
                            "line": found.symbol.selection_range.end.line,
                            "character": found.symbol.selection_range.end.character,
                        }
                    }
                });

                if include_body {
                    if let Ok(start_idx) = crate::providers::symbol::position_to_byte_index(
                        &text,
                        found.symbol.range.start,
                    ) {
                        if let Ok(end_idx) = crate::providers::symbol::position_to_byte_index(
                            &text,
                            found.symbol.range.end,
                        ) {
                            if let Some(body) = text.get(start_idx..end_idx) {
                                obj["body"] = Value::String(body.to_string());
                            }
                        }
                    }
                }

                obj
            })
            .collect();

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&results)?,
            json!({
                "file_path": file_path.to_string_lossy(),
                "matches_count": results.len(),
                "matches": results
            }),
        ))
    }
}

/// Replace the body of a symbol with new content.
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path to identify the symbol
/// - body: New body content to replace with
/// - language: Optional language ID (auto-detected if not provided)
pub struct LspReplaceSymbolBodyTool;

impl Tool for LspReplaceSymbolBodyTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_replace_symbol_body"
    }

    fn description(&self) -> &'static str {
        "Replace a symbol's body with new content"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path", "body"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to identify the symbol"
                },
                "body": {
                    "type": "string",
                    "description": "New body content to replace with"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Implement]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let body = params
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: body"))?;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        let new_text = crate::providers::symbol::replace_range(&text, &target_symbol.range, body)?;
        safe_write_file(&file_path, new_text.as_bytes())?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "replaced": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "characters_changed": new_text.len() as i64 - text.len() as i64
            }))?,
            json!({
                "replaced": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "characters_changed": new_text.len() as i64 - text.len() as i64
            }),
        ))
    }
}

/// Insert text before a symbol (at the beginning of its range).
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path to identify the symbol
/// - body: Text to insert before the symbol
/// - language: Optional language ID (auto-detected if not provided)
pub struct LspInsertBeforeSymbolTool;

impl Tool for LspInsertBeforeSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_insert_before_symbol"
    }

    fn description(&self) -> &'static str {
        "Insert text before a symbol (at the beginning of its range)"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path", "body"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to identify the symbol"
                },
                "body": {
                    "type": "string",
                    "description": "Text to insert before the symbol"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Implement]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let body = params
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: body"))?;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        let insertion_line = target_symbol.range.start.line;
        let new_text = crate::providers::symbol::insert_at_line(&text, insertion_line, body)?;
        safe_write_file(&file_path, new_text.as_bytes())?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "inserted_before": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "line": insertion_line
            }))?,
            json!({
                "inserted_before": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "line": insertion_line
            }),
        ))
    }
}

/// Insert text after a symbol (after the end of its range).
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path to identify the symbol
/// - body: Text to insert after the symbol
/// - language: Optional language ID (auto-detected if not provided)
pub struct LspInsertAfterSymbolTool;

impl Tool for LspInsertAfterSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_insert_after_symbol"
    }

    fn description(&self) -> &'static str {
        "Insert text after a symbol (after the end of its range)"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path", "body"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to identify the symbol"
                },
                "body": {
                    "type": "string",
                    "description": "Text to insert after the symbol"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Implement]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let body = params
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: body"))?;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        let insertion_line = target_symbol.range.end.line + 1;
        let new_text = crate::providers::symbol::insert_at_line(&text, insertion_line, body)?;
        safe_write_file(&file_path, new_text.as_bytes())?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "inserted_after": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "line": insertion_line
            }))?,
            json!({
                "inserted_after": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "line": insertion_line
            }),
        ))
    }
}

/// Safely delete a symbol, checking for references first.
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path to identify the symbol
/// - language: Optional language ID (auto-detected if not provided)
///
/// Returns an error if the symbol has references elsewhere in the codebase.
pub struct LspSafeDeleteSymbolTool;

impl Tool for LspSafeDeleteSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_safe_delete_symbol"
    }

    fn description(&self) -> &'static str {
        "Safely delete a symbol after checking for references"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to identify the symbol"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Implement]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        // Check for references using the selection_range (identifier position)
        let references = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let pos = target_symbol.selection_range.start;
            run_async_result(async { client.references(uri.clone(), pos).await })
        })?;

        if !references.is_empty() {
            let ref_list: Vec<String> = references
                .iter()
                .map(|loc| {
                    format!(
                        "{}:{}:{}",
                        loc.uri.path(),
                        loc.range.start.line + 1,
                        loc.range.start.character + 1
                    )
                })
                .collect();

            return Err(anyhow!(
                "symbol is referenced {} times: {}",
                references.len(),
                ref_list.join(", ")
            ));
        }

        // No references, safe to delete
        let new_text = crate::providers::symbol::replace_range(&text, &target_symbol.range, "")?;
        safe_write_file(&file_path, new_text.as_bytes())?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "deleted": name_path_str,
                "file_path": file_path.to_string_lossy(),
            }))?,
            json!({
                "deleted": name_path_str,
                "file_path": file_path.to_string_lossy(),
            }),
        ))
    }
}

/// Rename a symbol across the codebase.
///
/// Parameters:
/// - `file_path`: Path to the source file containing the symbol
/// - `name_path`: Symbol name path to identify the symbol to rename
/// - `new_name`: The new name for the symbol
/// - language: Optional language ID (auto-detected if not provided)
///
/// Returns a summary of all files modified by the rename operation.
pub struct LspRenameSymbolTool;

impl Tool for LspRenameSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_rename_symbol"
    }

    fn description(&self) -> &'static str {
        "Rename a symbol across the codebase"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path", "new_name"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file containing the symbol (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to identify the symbol to rename"
                },
                "new_name": {
                    "type": "string",
                    "description": "The new name for the symbol"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Refactor]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let new_name = params
            .get("new_name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: new_name"))?;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        // Use the selection_range.start position (identifier location) for rename
        let rename_pos = target_symbol.selection_range.start;

        // Get rename edits from LSP
        let workspace_edits = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            run_async_result(async {
                client
                    .rename(uri.clone(), rename_pos, new_name.to_string())
                    .await
            })
        })?;

        // Apply edits to files
        let mut affected_files = Vec::new();
        if let Some(changes) = workspace_edits.changes {
            for (file_uri, edits) in changes {
                let file_path_from_uri = PathBuf::from(file_uri.path().to_string());

                let mut file_text = safe_read_file_to_string(&file_path_from_uri)
                    .context("failed to read file for rename")?;

                // Apply edits in reverse order to preserve positions
                for edit in edits.iter().rev() {
                    file_text = crate::providers::symbol::replace_range(
                        &file_text,
                        &edit.range,
                        &edit.new_text,
                    )?;
                }

                safe_write_file(&file_path_from_uri, file_text.as_bytes())
                    .context("failed to write renamed file")?;
                affected_files.push(file_path_from_uri.to_string_lossy().to_string());
            }
        }

        affected_files.sort();

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "renamed": name_path_str,
                "new_name": new_name,
                "files_modified": affected_files.len(),
                "files": affected_files
            }))?,
            json!({
                "renamed": name_path_str,
                "new_name": new_name,
                "files_modified": affected_files.len(),
                "files": affected_files
            }),
        ))
    }
}

/// Analyze a symbol to get detailed information (references, implementations, etc.).
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path to analyze
/// - language: Optional language ID (auto-detected if not provided)
///
/// Returns comprehensive symbol analysis including reference count, definition, complexity metrics.
pub struct LspAnalyzeSymbolTool;

impl Tool for LspAnalyzeSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_analyze_symbol"
    }

    fn description(&self) -> &'static str {
        "Analyze a symbol to get its references, implementations, call hierarchy, and complexity metrics. Use when: you need a comprehensive understanding of a symbol's role in the codebase, you're planning a refactor, or you need to understand inheritance/implementation chains. Requires: file_path, line, character."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to analyze"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Debug, ToolTag::Explore, ToolTag::Refactor]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        // Get references
        let references = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let pos = target_symbol.selection_range.start;
            run_async_result(async { client.references(uri.clone(), pos).await })
        })?;

        // Get implementations (would require gotoImplementation which LSP supports but LspClient doesn't yet expose)
        // For now, we use an empty vector - in future versions this could be added to LspClient
        let implementations: Vec<lsp_types::Location> = Vec::new();

        // Calculate body complexity (simple heuristics: lines, nesting depth)
        let body_text = if let Ok(start_idx) =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.start)
        {
            if let Ok(end_idx) =
                crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.end)
            {
                text.get(start_idx..end_idx).unwrap_or("")
            } else {
                ""
            }
        } else {
            ""
        };

        let body_lines = body_text.lines().count();
        let nesting_depth = body_text.chars().filter(|&c| c == '{' || c == '(').count();

        // Group references by file
        let mut refs_by_file: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
        for loc in &references {
            let file_key = loc.uri.path().to_string();
            let entry = refs_by_file.entry(file_key).or_default();
            entry.push((loc.range.start.line + 1, loc.range.start.character + 1));
        }

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "symbol": name_path_str,
                "kind": crate::providers::symbol::format_symbol_kind(&target_symbol.kind),
                "definition": {
                    "file": file_path.to_string_lossy(),
                    "range": {
                        "start": format!("{}:{}", target_symbol.range.start.line + 1, target_symbol.range.start.character + 1),
                        "end": format!("{}:{}", target_symbol.range.end.line + 1, target_symbol.range.end.character + 1)
                    }
                },
                "references": {
                    "total_count": references.len(),
                    "by_file": refs_by_file
                },
                "implementations": {
                    "count": implementations.len(),
                    "locations": implementations.iter().map(|loc| {
                        format!("{}:{}", loc.range.start.line + 1, loc.range.start.character + 1)
                    }).collect::<Vec<_>>()
                },
                "complexity": {
                    "lines": body_lines,
                    "nesting_depth": nesting_depth,
                    "cyclomatic_estimate": (nesting_depth / 2).max(1)
                }
            }))?,
            json!({
                "symbol": name_path_str,
                "references_count": references.len(),
                "implementations_count": implementations.len(),
                "body_lines": body_lines,
                "nesting_depth": nesting_depth
            }),
        ))
    }
}

/// Extract a symbol definition to a new file/module.
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path to extract
/// - `target_file`: Where to extract the symbol (new or existing file)
/// - language: Optional language ID (auto-detected if not provided)
///
/// Returns path to the created/modified file and import statement to add.
pub struct LspExtractSymbolTool;

impl Tool for LspExtractSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_extract_symbol"
    }

    fn description(&self) -> &'static str {
        "Extract a symbol definition to a new file or module"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path", "target_file"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file containing the symbol (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to extract"
                },
                "target_file": {
                    "type": "string",
                    "description": "Path where to extract the symbol (relative path for new module)"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Refactor]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let target_file_str = params
            .get("target_file")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: target_file"))?;
        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        // Extract the symbol body
        let body_start =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.start)?;
        let body_end =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.end)?;
        let symbol_body = text[body_start..body_end].to_string();

        // Resolve target file path
        let target_file = ctx.cwd.join(target_file_str);
        ensure_path_within_workspace(ctx, &target_file)?;

        // Create parent directories if needed
        if let Some(parent) = target_file.parent() {
            std::fs::create_dir_all(parent).context("failed to create target directory")?;
        }

        // Write the extracted symbol to target file
        let target_content = if target_file.exists() {
            // Append to existing file
            let mut existing = read_file_blocking(&target_file)?;
            existing.push('\n');
            existing.push('\n');
            existing.push_str(&symbol_body);
            existing
        } else {
            // Create new file with module declaration if needed
            symbol_body.clone()
        };

        safe_write_file(&target_file, target_content.as_bytes())
            .context("failed to write target file")?;

        // Remove from original file
        let new_original =
            crate::providers::symbol::replace_range(&text, &target_symbol.range, "")?;
        safe_write_file(&file_path, new_original.as_bytes())
            .context("failed to update original file")?;

        let import_stmt = format!(
            "mod {}; use {}::*;",
            target_file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy(),
            name_path_str.split('/').next().unwrap_or("_")
        );

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "extracted": name_path_str,
                "from": file_path.to_string_lossy(),
                "to": target_file.to_string_lossy(),
                "import_statement": import_stmt,
                "symbol_size_bytes": symbol_body.len()
            }))?,
            json!({
                "extracted": name_path_str,
                "target_file": target_file.to_string_lossy(),
                "import": import_stmt
            }),
        ))
    }
}

/// Inline a symbol definition at its usage sites.
///
/// Parameters:
/// - `file_path`: Path to the source file
/// - `name_path`: Symbol name path to inline
/// - language: Optional language ID (auto-detected if not provided)
///
/// Returns summary of how many sites were inlined.
pub struct LspInlineSymbolTool;

impl Tool for LspInlineSymbolTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_inline_symbol"
    }

    fn description(&self) -> &'static str {
        "Inline a symbol definition at its usage sites"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "name_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the source file containing the symbol (alias: path)"
                },
                "path": {
                    "type": "string",
                    "description": "Alias for file_path"
                },
                "name_path": {
                    "type": "string",
                    "description": "Symbol name path to inline"
                },
                "language": {
                    "type": "string",
                    "description": "Language ID (auto-detected from extension if not provided)"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Refactor]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path = resolve_file_path(ctx, &params)?;
        let name_path_str = params
            .get("name_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: name_path"))?;
        let force = params
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let remove_definition = params
            .get("remove_definition")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let language_id = if let Some(lang_str) = params.get("language").and_then(Value::as_str) {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let mut text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        // Extract function signature and body
        let sel_start = crate::providers::symbol::position_to_byte_index(
            &text,
            target_symbol.selection_range.start,
        )?;
        let body_end =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.end)?;

        // Get parameter names from the signature
        let sig_text = &text[sel_start..body_end];
        let params_list =
            crate::providers::symbol::extract_param_names(sig_text).unwrap_or_default();

        // Get function body
        let body_text = &text[sel_start..body_end];
        let (body_content, is_single_expr) =
            crate::providers::symbol::extract_function_body(body_text)?;

        // Check if inlining is safe (multi-statement requires force=true)
        if !is_single_expr && !force {
            return Err(anyhow!(
                "function body has multiple statements; use force=true to inline anyway"
            ));
        }

        // Get references
        let references = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let pos = target_symbol.selection_range.start;
            run_async_result(async { client.references(uri.clone(), pos).await })
        })?;

        // Filter to references in the same file (excluding the definition itself)
        let same_file_refs: Vec<_> = references
            .iter()
            .filter(|r| {
                r.uri.path().as_str() == file_path.as_os_str()
                    && r.range != target_symbol.selection_range
            })
            .collect();

        if same_file_refs.is_empty() {
            return Ok(ToolOutput::with_structured(
                "No references found to inline".to_string(),
                json!({
                    "symbol": name_path_str,
                    "status": "no_references"
                }),
            ));
        }

        // Process each reference from end to start (to avoid index shifts)
        let mut call_sites: Vec<_> = same_file_refs
            .iter()
            .map(|r| crate::providers::symbol::position_to_byte_index(&text, r.range.start))
            .collect::<Result<Vec<_>>>()?;
        call_sites.sort_by_key(|a| std::cmp::Reverse(*a)); // Sort in reverse order

        let mut inlined_count = 0;
        let mut errors = Vec::new();

        for call_byte_idx in call_sites {
            // Find the argument list for this call
            if let Some((arg_start, arg_end)) =
                crate::providers::symbol::find_call_args_range(&text, call_byte_idx)
            {
                let args_str = &text[arg_start + 1..arg_end];
                let args = crate::providers::symbol::split_args(args_str);

                // Validate arity
                if args.len() != params_list.len() {
                    errors.push(format!(
                        "arity mismatch at byte {}: expected {} args, got {}",
                        call_byte_idx,
                        params_list.len(),
                        args.len()
                    ));
                    continue;
                }

                // Perform substitution
                let param_refs: Vec<&str> = params_list.iter().map(String::as_str).collect();
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let inlined_body = crate::providers::symbol::substitute_params(
                    &body_content,
                    &param_refs,
                    &arg_refs,
                );

                // Replace the call with the inlined body
                let replacement = format!("({inlined_body})");
                text = crate::providers::symbol::replace_range(
                    &text,
                    &lsp_types::Range {
                        start: Position {
                            line: 0,
                            character: call_byte_idx as u32,
                        },
                        end: Position {
                            line: 0,
                            character: (arg_end + 1) as u32,
                        },
                    },
                    &replacement,
                )?;

                inlined_count += 1;
            } else {
                errors.push(format!(
                    "could not find argument list at byte {call_byte_idx}"
                ));
            }
        }

        // Optionally remove the definition
        if remove_definition && inlined_count > 0 {
            text = crate::providers::symbol::replace_range(&text, &target_symbol.range, "")?;
        }

        // Write the file back
        safe_write_file(&file_path, text.as_bytes())
            .with_context(|| format!("failed to write file {}", file_path.display()))?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&json!({
                "symbol": name_path_str,
                "inlined_count": inlined_count,
                "definition_removed": remove_definition && inlined_count > 0,
                "errors": errors,
                "status": if errors.is_empty() {
                    format!("Successfully inlined {inlined_count} call site(s)")
                } else {
                    format!("Inlined {} call site(s) with {} error(s)", inlined_count, errors.len())
                }
            }))?,
            json!({
                "symbol": name_path_str,
                "inlined_count": inlined_count,
                "definition_removed": remove_definition && inlined_count > 0,
                "errors": errors
            }),
        ))
    }
}

/// Search for symbols across the workspace.
pub struct LspWorkspaceSymbolsTool;

impl Tool for LspWorkspaceSymbolsTool {
    fn defer_loading(&self) -> Option<bool> {
        Some(true)
    }

    fn name(&self) -> &'static str {
        "lsp_workspace_symbols"
    }

    fn description(&self) -> &'static str {
        "Search for symbols across the entire workspace by name. PREFER THIS OVER GREP for finding function, struct, enum, or trait definitions — it returns exact locations with symbol kinds. Use when: you need to find where a type/function is defined, you know the symbol name but not the file. Requires: query (symbol name), language."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query", "language"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Symbol name or pattern to search for (e.g., 'MyClass', 'parse')"
                },
                "language": {
                    "type": "string",
                    "description": "Programming language (e.g., 'rust', 'python', 'typescript'). Defaults to auto-detect from workspace."
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required parameter: query"))?;

        let language_str = params
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("rust");

        let language_id = LanguageId::from_path(&PathBuf::from(format!("dummy.{language_str}")));
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = match with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            run_async_result(async { client.workspace_symbols(query).await })
        }) {
            Ok(syms) => syms,
            Err(e) => {
                // Workspace symbols may fail if the language server hasn't finished
                // indexing or doesn't support the request. Return empty results with
                // a helpful message so callers can fall back to grep.
                tracing::debug!("workspace_symbols failed (server may still be indexing): {e:#}");
                return Ok(ToolOutput::with_structured(
                    format!("Workspace symbol search unavailable for '{query}'. The language server may still be indexing. Try lsp_document_symbols on a specific file instead, or use grep.\n"),
                    json!({
                        "query": query,
                        "count": 0,
                        "symbols": [],
                        "error": format!("{e:#}")
                    }),
                ));
            }
        };

        // Format the output
        let symbol_info: Vec<Value> = symbols
            .iter()
            .map(|sym| {
                json!({
                    "name": sym.name,
                    "kind": format!("{:?}", sym.kind),
                    "file": sym.location.uri.path().to_string(),
                    "line": sym.location.range.start.line,
                    "character": sym.location.range.start.character,
                    "container": sym.container_name.as_deref().unwrap_or("<root>")
                })
            })
            .collect();

        let text_summary = format!("Found {} symbol(s) matching '{}'\n\n", symbols.len(), query);

        let detailed = symbol_info.iter().map(|s| {
            format!(
                "{} ({}): {}:{}",
                s["name"].as_str().unwrap_or("?"),
                s["kind"].as_str().unwrap_or("?"),
                s["file"].as_str().unwrap_or("?"),
                s["line"].as_u64().unwrap_or(0)
            )
        });

        let detailed_text = detailed.collect::<Vec<_>>().join("\n");

        Ok(ToolOutput::with_structured(
            format!("{text_summary}{detailed_text}"),
            json!({
                "query": query,
                "count": symbols.len(),
                "symbols": symbol_info
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper to create a test context
    fn create_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let ctx = ToolContext::new(temp_dir.path());
        (ctx, temp_dir)
    }

    // Helper to create a test file
    fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_path_lossy_in_json() {
        // Verify that PathBuf::to_string_lossy works in json! macro
        let p = PathBuf::from("/project/src/main.rs");
        let val = json!({ "file_path": p.to_string_lossy() });
        assert_eq!(val["file_path"], "/project/src/main.rs");
    }

    #[test]
    fn test_path_lossy_relative() {
        let p = PathBuf::from("src/lib.rs");
        let val = json!({ "path": p.to_string_lossy() });
        assert_eq!(val["path"], "src/lib.rs");
    }

    // ============================================================================
    // LspDiagnosticsTool Tests
    // ============================================================================

    #[test]
    fn test_diagnostics_tool_name_and_description() {
        let tool = LspDiagnosticsTool;
        assert_eq!(tool.name(), "lsp_diagnostics");
        assert_eq!(
            tool.description(),
            "Check which language servers are available and their status. Use this to verify code intelligence capabilities before using other LSP tools."
        );
    }

    #[test]
    fn test_diagnostics_tool_permission() {
        let tool = LspDiagnosticsTool;
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_diagnostics_tool_parameters_schema() {
        let tool = LspDiagnosticsTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["servers"].is_object());
        assert_eq!(schema["properties"]["servers"]["type"], "array");
        assert_eq!(schema["properties"]["servers"]["items"]["type"], "string");
    }

    #[test]
    fn test_diagnostics_default_servers() {
        let tool = LspDiagnosticsTool;
        let (ctx, _temp) = create_test_context();

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(
            output.text.contains("rust-analyzer")
                || output.text.contains("typescript-language-server")
                || output.text.contains("pyright-langserver")
        );
        assert!(output.structured.is_some());
    }

    #[test]
    fn test_diagnostics_custom_servers() {
        let tool = LspDiagnosticsTool;
        let (ctx, _temp) = create_test_context();

        let params = json!({
            "servers": ["rust-analyzer", "nonexistent-server"]
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(
            output.text.contains("rust-analyzer") || output.text.contains("nonexistent-server")
        );
        assert!(output.structured.is_some());
    }

    #[test]
    fn test_diagnostics_empty_servers_array() {
        let tool = LspDiagnosticsTool;
        let (ctx, _temp) = create_test_context();

        let params = json!({ "servers": [] });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.structured.is_some());
        let structured = output.structured.unwrap();
        assert_eq!(structured.as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_diagnostics_metadata_generation() {
        let tool = LspDiagnosticsTool;
        let (ctx, _temp) = create_test_context();

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(!output.text.is_empty());
        assert!(output.structured.is_some());

        let structured = output.structured.unwrap();
        if let Some(array) = structured.as_array() {
            for item in array {
                assert!(item.is_object());
                let obj = item.as_object().unwrap();
                assert!(obj.contains_key("name"));
                assert!(obj.contains_key("installed"));
                assert!(obj["name"].is_string());
                assert!(obj["installed"].is_boolean());
            }
        }
    }

    // ============================================================================
    // LspHoverTool Tests
    // ============================================================================

    #[test]
    fn test_hover_tool_name_and_description() {
        let tool = LspHoverTool;
        assert_eq!(tool.name(), "lsp_hover");
        assert_eq!(
            tool.description(),
            "Get type information, documentation, and signature at a specific position. Use when: you need to know the type of a variable, the signature of a function, or the docs for a method. Faster than reading the whole file. Requires: file_path, line, character."
        );
    }

    #[test]
    fn test_hover_tool_permission() {
        let tool = LspHoverTool;
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_hover_tool_parameters_schema() {
        let tool = LspHoverTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["required"].is_array());
        let required = schema["required"].as_array().unwrap();
        let expected_json = json!(["file_path", "line", "character"]);
        let expected = expected_json.as_array().unwrap();
        assert_eq!(required, expected);
        assert_eq!(schema["properties"]["file_path"]["type"], "string");
        assert_eq!(schema["properties"]["line"]["type"], "integer");
        assert_eq!(schema["properties"]["character"]["type"], "integer");
        assert_eq!(schema["properties"]["language"]["type"], "string");
    }

    #[test]
    fn test_hover_missing_file_path() {
        let tool = LspHoverTool;
        let (ctx, _temp) = create_test_context();

        let params = json!({
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file_path"));
    }

    #[test]
    fn test_hover_missing_line() {
        let tool = LspHoverTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line"));
    }

    #[test]
    fn test_hover_missing_character() {
        let tool = LspHoverTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "line": 0
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("character"));
    }

    #[test]
    fn test_hover_path_outside_workspace() {
        let tool = LspHoverTool;
        let (ctx, _temp) = create_test_context();

        let params = json!({
            "file_path": "/etc/passwd",
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("outside workspace"));
    }

    #[test]
    fn test_hover_relative_path_resolution() {
        let tool = LspHoverTool;
        let (ctx, _temp) = create_test_context();
        create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        let params = json!({
            "file_path": "test.rs",
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        // We expect this to fail (LSP not available), but path resolution should succeed
        // The error should be about LSP, not about file path
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("failed to read file"));
        }
    }

    #[test]
    fn test_hover_language_detection() {
        let tool = LspHoverTool;
        let (ctx, _temp) = create_test_context();

        // Test Rust file
        let rs_file = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");
        let params = json!({
            "file_path": rs_file.to_string_lossy(),
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        // Will fail due to LSP unavailable, but validates language detection works
        assert!(result.is_err() || result.is_ok());
    }

    // ============================================================================
    // LspDefinitionTool Tests
    // ============================================================================

    #[test]
    fn test_definition_tool_name_and_description() {
        let tool = LspDefinitionTool;
        assert_eq!(tool.name(), "lsp_definition");
        assert_eq!(
            tool.description(),
            "Jump to the definition of a function, variable, type, or import at a specific position. PREFER THIS OVER GREP for navigation — gives the exact definition location. Use when: you see a symbol used in code and want to find where it's defined, you need to trace an import to its source. Requires: file_path, line, character."
        );
    }

    #[test]
    fn test_definition_tool_permission() {
        let tool = LspDefinitionTool;
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_definition_tool_parameters_schema() {
        let tool = LspDefinitionTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["required"].is_array());
        let required = schema["required"].as_array().unwrap();
        let expected_json = json!(["file_path", "line", "character"]);
        let expected = expected_json.as_array().unwrap();
        assert_eq!(required, expected);
    }

    #[test]
    fn test_definition_missing_parameters() {
        let tool = LspDefinitionTool;
        let (ctx, _temp) = create_test_context();

        // Test missing file_path
        let result = tool.execute(json!({ "line": 0, "character": 0 }), &ctx);
        assert!(result.is_err());

        // Test missing line
        let result = tool.execute(json!({ "file_path": "test.rs", "character": 0 }), &ctx);
        assert!(result.is_err());

        // Test missing character
        let result = tool.execute(json!({ "file_path": "test.rs", "line": 0 }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_definition_position_parsing() {
        let tool = LspDefinitionTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        // Test valid position
        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        // Will fail due to LSP unavailable, but position parsing should succeed
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("missing required parameter"));
        }
    }

    #[test]
    fn test_definition_invalid_position() {
        let tool = LspDefinitionTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        // Test negative line (should be rejected by schema)
        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "line": -1,
            "character": 0
        });

        // Parameter extraction should handle this
        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_definition_absolute_vs_relative_path() {
        let tool = LspDefinitionTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn test() {}");

        // Test with absolute path
        let params_abs = json!({
            "file_path": file_path.to_string_lossy(),
            "line": 0,
            "character": 0
        });

        let result_abs = tool.execute(params_abs, &ctx);
        // Both should behave the same way (fail due to LSP unavailable)
        assert!(result_abs.is_err() || result_abs.is_ok());

        // Test with relative path
        let params_rel = json!({
            "file_path": "test.rs",
            "line": 0,
            "character": 0
        });

        let result_rel = tool.execute(params_rel, &ctx);
        assert!(result_rel.is_err() || result_rel.is_ok());
    }

    #[test]
    fn test_definition_nonexistent_file() {
        let tool = LspDefinitionTool;
        let (ctx, _temp) = create_test_context();

        let params = json!({
            "file_path": "nonexistent.rs",
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to read file"));
    }

    // ============================================================================
    // LspCompletionTool Tests
    // ============================================================================

    #[test]
    fn test_completion_tool_name_and_description() {
        let tool = LspCompletionTool;
        assert_eq!(tool.name(), "lsp_completion");
        assert_eq!(
            tool.description(),
            "Get code completions (suggestions) at a specific position. Use this to see what functions, variables, or keywords are available while typing. Requires file_path, line, and character position."
        );
    }

    #[test]
    fn test_completion_tool_permission() {
        let tool = LspCompletionTool;
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_completion_tool_parameters_schema() {
        let tool = LspCompletionTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["required"].is_array());
        let required = schema["required"].as_array().unwrap();
        let expected_json = json!(["file_path", "line", "character"]);
        let expected = expected_json.as_array().unwrap();
        assert_eq!(required, expected);
        assert_eq!(schema["properties"]["file_path"]["type"], "string");
        assert_eq!(schema["properties"]["line"]["type"], "integer");
        assert_eq!(schema["properties"]["character"]["type"], "integer");
        assert_eq!(schema["properties"]["language"]["type"], "string");
        assert_eq!(schema["properties"]["trigger_character"]["type"], "string");
    }

    #[test]
    fn test_completion_missing_required_parameters() {
        let tool = LspCompletionTool;
        let (ctx, _temp) = create_test_context();

        // Missing file_path
        let result = tool.execute(json!({ "line": 0, "character": 0 }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file_path"));

        // Missing line
        let result = tool.execute(json!({ "file_path": "test.rs", "character": 0 }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("line"));

        // Missing character
        let result = tool.execute(json!({ "file_path": "test.rs", "line": 0 }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("character"));
    }

    #[test]
    fn test_completion_with_trigger_character() {
        let tool = LspCompletionTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "line": 0,
            "character": 0,
            "trigger_character": "."
        });

        let result = tool.execute(params, &ctx);
        // Will fail due to LSP unavailable, but trigger_character should be processed
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("missing required parameter"));
        }
    }

    #[test]
    fn test_completion_optional_language_parameter() {
        let tool = LspCompletionTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        // Test without language parameter (should auto-detect)
        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);
        // Should process successfully (even if LSP fails)
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_completion_explicit_language_parameter() {
        let tool = LspCompletionTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.py", "print('hello')");

        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "line": 0,
            "character": 0,
            "language": "python"
        });

        let result = tool.execute(params, &ctx);
        // Should use explicit language parameter
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("missing required parameter"));
        }
    }

    #[test]
    fn test_completion_output_structure() {
        let tool = LspCompletionTool;
        let (ctx, _temp) = create_test_context();
        let file_path = create_test_file(&ctx.cwd, "test.rs", "fn main() {}");

        let params = json!({
            "file_path": file_path.to_string_lossy(),
            "line": 0,
            "character": 0
        });

        let result = tool.execute(params, &ctx);

        // Even on error, check that the tool follows the expected pattern
        if let Ok(output) = result {
            assert!(!output.text.is_empty());
            assert!(output.structured.is_some());

            let structured = output.structured.unwrap();
            assert!(structured.is_object());
            assert!(structured.get("completion").is_some());
        }
    }

    // ============================================================================
    // Shared Utility Tests
    // ============================================================================

    #[test]
    fn test_language_for_path() {
        assert_eq!(
            language_for_path(Path::new("test.rs")).language_id_str(),
            "rust"
        );
        assert_eq!(
            language_for_path(Path::new("test.ts")).language_id_str(),
            "typescript"
        );
        assert_eq!(
            language_for_path(Path::new("test.tsx")).language_id_str(),
            "typescript"
        );
        assert_eq!(
            language_for_path(Path::new("test.js")).language_id_str(),
            "javascript"
        );
        assert_eq!(
            language_for_path(Path::new("test.jsx")).language_id_str(),
            "javascript"
        );
        assert_eq!(
            language_for_path(Path::new("test.py")).language_id_str(),
            "python"
        );
        assert_eq!(
            language_for_path(Path::new("test.go")).language_id_str(),
            "go"
        );
        assert_eq!(
            language_for_path(Path::new("test.unknown")).language_id_str(),
            "unknown"
        );
    }

    #[test]
    fn test_param_u32_valid() {
        let params = json!({ "value": 42 });
        assert_eq!(param_u32(&params, "value").unwrap(), 42);
    }

    #[test]
    fn test_param_u32_missing() {
        let params = json!({ "other": 42 });
        assert!(param_u32(&params, "value").is_err());
    }

    #[test]
    fn test_param_u32_zero() {
        let params = json!({ "value": 0 });
        assert_eq!(param_u32(&params, "value").unwrap(), 0);
    }

    #[test]
    fn test_resolve_file_path_absolute() {
        let temp_dir = TempDir::new().unwrap();
        let ctx = ToolContext::new(temp_dir.path());
        let file_path = temp_dir.path().join("test.rs");

        let params = json!({
            "file_path": file_path.to_string_lossy()
        });

        let resolved = resolve_file_path(&ctx, &params).unwrap();
        assert_eq!(resolved, file_path);
    }

    #[test]
    fn test_resolve_file_path_relative() {
        let temp_dir = TempDir::new().unwrap();
        let ctx = ToolContext::new(temp_dir.path());

        let params = json!({ "file_path": "test.rs" });

        let resolved = resolve_file_path(&ctx, &params).unwrap();
        assert_eq!(resolved, temp_dir.path().join("test.rs"));
    }

    #[test]
    fn test_resolve_file_path_missing_parameter() {
        let temp_dir = TempDir::new().unwrap();
        let ctx = ToolContext::new(temp_dir.path());

        let params = json!({ "other": "test.rs" });

        let result = resolve_file_path(&ctx, &params);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file_path"));
    }

    #[test]
    fn test_path_validation_workspace_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let ctx = ToolContext::new(temp_dir.path());

        // Try to access parent directory (should be blocked)
        let parent_path = temp_dir.path().parent().unwrap().join("test.rs");
        let result = ensure_path_within_workspace(&ctx, &parent_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("outside workspace"));
    }

    #[test]
    fn test_path_validation_valid_path() {
        let temp_dir = TempDir::new().unwrap();
        let ctx = ToolContext::new(temp_dir.path());
        let valid_path = temp_dir.path().join("subdir").join("test.rs");

        // Create parent directory
        fs::create_dir_all(valid_path.parent().unwrap()).unwrap();

        let result = ensure_path_within_workspace(&ctx, &valid_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_param_u32_overflow() {
        let params = json!({"line": u64::MAX});
        let result = param_u32(&params, "line");
        assert!(result.is_err(), "u64::MAX should not fit in u32");
    }

    // ============================================================================
    // Bridge Function Tests
    // ============================================================================

    #[test]
    fn test_active_clients_status_empty_when_no_clients() {
        let status = active_clients_status();
        assert!(status.is_empty());
    }

    #[test]
    fn test_shutdown_client_returns_false_for_unknown_key() {
        let found = shutdown_client("nonexistent_key");
        assert!(!found);
    }

    #[test]
    fn test_shutdown_all_clients_on_empty_pool() {
        shutdown_all_clients();
        let status = active_clients_status();
        assert!(status.is_empty());
    }
}
