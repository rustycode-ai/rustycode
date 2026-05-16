//! Shared code-index cache for tools that need a `CodeIndex`.
//!
//! Previously, the same `OnceLock<Mutex<HashMap<PathBuf, Arc<CodeIndex>>>>` was
//! duplicated across four provider files. This module owns the single canonical
//! cache and exposes a `build_code_index` helper.

use crate::indexing::CodeIndex;
use crate::ToolContext;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

static CODE_INDEX_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<CodeIndex>>>> = OnceLock::new();

/// Return the shared cache map (initialised on first call).
fn code_indexes() -> &'static Mutex<HashMap<PathBuf, Arc<CodeIndex>>> {
    CODE_INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Canonicalise the workspace root from the tool context.
fn workspace_root(ctx: &ToolContext) -> PathBuf {
    std::fs::canonicalize(&ctx.cwd).unwrap_or_else(|_| ctx.cwd.clone())
}

/// Build (or retrieve from cache) a `CodeIndex` for the workspace root in `ctx`.
///
/// The index is built with `CodeIndex::new(root)` and cached per canonical path.
pub fn build_code_index(ctx: &ToolContext) -> Result<Arc<CodeIndex>> {
    let root = workspace_root(ctx);
    let mut guard = code_indexes().lock().unwrap_or_else(|e| e.into_inner());

    if let Some(index) = guard.get(&root) {
        return Ok(Arc::clone(index));
    }

    let mut index = CodeIndex::new(root.clone());
    index
        .build()
        .with_context(|| format!("failed to build code index for {}", root.display()))?;

    let index = Arc::new(index);
    guard.insert(root, Arc::clone(&index));
    Ok(index)
}
