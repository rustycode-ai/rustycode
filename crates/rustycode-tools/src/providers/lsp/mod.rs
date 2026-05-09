use crate::security::{create_file_symlink_safe, open_file_symlink_safe};
use crate::ToolContext;
use anyhow::{anyhow, Context, Result};
use lsp_types::Uri as Url;
use rustycode_lsp::{create_client_config_with_override, LanguageId, LspClient, LspConfig};
use rustycode_shared_runtime as shared_runtime;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use url::Url as FileUrl;

// Re-export all tools
pub use analyze_symbol::*;
pub use code_actions::*;
pub use completion::*;
pub use definition::*;
pub use diagnostics::*;
pub use document_symbols::*;
pub use extract_symbol::*;
pub use find_symbol::*;
pub use formatting::*;
pub use full_diagnostics::*;
pub use hover::*;
pub use inline_symbol::*;
pub use insert_after_symbol::*;
pub use insert_before_symbol::*;
pub use references::*;
pub use rename::*;
pub use rename_symbol::*;
pub use replace_symbol_body::*;
pub use safe_delete_symbol::*;
pub use symbols_overview::*;
pub use workspace_symbols::*;

pub mod analyze_symbol;
pub mod code_actions;
pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod document_symbols;
pub mod extract_symbol;
pub mod find_symbol;
pub mod formatting;
pub mod full_diagnostics;
pub mod hover;
pub mod inline_symbol;
pub mod insert_after_symbol;
pub mod insert_before_symbol;
pub mod references;
pub mod rename;
pub mod rename_symbol;
pub mod replace_symbol_body;
pub mod safe_delete_symbol;
pub mod symbols_overview;
pub mod workspace_symbols;

static LSP_CLIENTS: OnceLock<Mutex<HashMap<String, LspClient>>> = OnceLock::new();
static LSP_BACKOFF: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
const MAX_LSP_CLIENTS: usize = 10;
const CRASH_BACKOFF_SECS: u64 = 60;

fn lsp_backoff() -> &'static Mutex<HashMap<String, std::time::Instant>> {
    LSP_BACKOFF.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_lsp_backoff(key: &str, secs: u64) {
    if let Ok(mut map) = lsp_backoff().lock() {
        map.insert(
            key.to_string(),
            std::time::Instant::now() + std::time::Duration::from_secs(secs),
        );
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

pub(crate) fn language_for_path(path: &Path) -> LanguageId {
    LanguageId::from_path(path)
}

pub(crate) trait UriPathExt {
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

pub(crate) fn resolve_file_path_from_str(ctx: &ToolContext, path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    let resolved = if p.is_absolute() { p } else { ctx.cwd.join(p) };
    ensure_path_within_workspace(ctx, &resolved)?;
    Ok(resolved)
}

/// Symlink-safe file write: opens with `O_NOFOLLOW`, writes, syncs.
pub(crate) fn safe_write_file(path: &Path, content: &[u8]) -> Result<()> {
    let mut file = create_file_symlink_safe(path)
        .with_context(|| format!("failed to create file {}", path.display()))?;
    file.write_all(content)
        .with_context(|| format!("failed to write file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync file {}", path.display()))?;
    Ok(())
}

/// Symlink-safe file read: opens with `O_NOFOLLOW`, reads to string.
pub(crate) fn safe_read_file_to_string(path: &Path) -> Result<String> {
    let mut file = open_file_symlink_safe(path)
        .with_context(|| format!("failed to open file {}", path.display()))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .with_context(|| format!("failed to read file {}", path.display()))?;
    Ok(content)
}

pub(crate) fn ensure_path_within_workspace(ctx: &ToolContext, path: &Path) -> Result<()> {
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
        shared_runtime::block_on_shared(async { tokio::fs::read_to_string(&path).await })
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

#[cfg(test)]
pub(crate) mod tests_common {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper to create a test context
    pub fn create_test_context() -> (ToolContext, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let ctx = ToolContext::new(temp_dir.path());
        (ctx, temp_dir)
    }

    // Helper to create a test file
    #[allow(dead_code)]
    pub fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

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
        std::fs::create_dir_all(valid_path.parent().unwrap()).unwrap();

        let result = ensure_path_within_workspace(&ctx, &valid_path);
        assert!(result.is_ok());
    }

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
