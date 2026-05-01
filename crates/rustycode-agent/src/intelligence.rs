//! `CodeIntelligence` — the agent's brain.
//!
//! Structural code understanding the agent queries on demand.
//! Replaces heuristic nudges with real analysis:
//!
//! | Heuristic (deleted)            | What this does instead                |
//! |--------------------------------|---------------------------------------|
//! | "URGENT: N reads no writes!"   | dependents() shows what needs editing |
//! | "You forgot to verify!"        | diagnostics() shows if anything broke |
//! | "Same file edited 15 times!"   | diagnostics() shows if approach works  |
//! | 370-line stop-prevention chain | changes() shows what's done/isn't      |

use std::path::{Path, PathBuf};

/// Structural code understanding the agent queries on demand.
pub trait CodeIntelligence: Send + Sync {
    /// Structural summary of the codebase for system prompt injection.
    /// Token-budgeted — the output fits within the given budget.
    fn repo_map(&self, budget_tokens: usize) -> String;

    /// Files/functions that depend on the given path.
    fn dependents(&self, path: &str) -> Vec<SymbolRef>;

    /// Functions/symbols that call the given symbol.
    fn callers(&self, symbol: &str) -> Vec<SymbolRef>;

    /// Files that changed since the last query.
    fn changes(&self) -> Vec<FileChange>;

    /// Semantic search for code related to a query.
    fn search(&self, query: &str, limit: usize) -> Vec<CodeLocation>;

    /// Outline of a single file (functions, structs, traits).
    fn file_outline(&self, path: &Path) -> Option<String>;
}

/// A reference to a symbol in the codebase.
#[derive(Debug, Clone)]
pub struct SymbolRef {
    pub name: String,
    pub file: PathBuf,
    pub line: usize,
    pub kind: String,
}

/// A file change detected since the last query.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    /// Symbols defined in this file (for impact analysis).
    pub symbols: Vec<String>,
}

/// Type of file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

/// A location in the codebase found via search.
#[derive(Debug, Clone)]
pub struct CodeLocation {
    pub file: PathBuf,
    pub line: usize,
    pub symbol: String,
    pub context: String,
}

// ---------------------------------------------------------------------------
// Noop implementation (testing / when no intelligence is available)
// ---------------------------------------------------------------------------

/// No-op intelligence — returns empty results for all queries.
/// Used when no code analysis infrastructure is available.
pub struct NoopIntelligence;

impl CodeIntelligence for NoopIntelligence {
    fn repo_map(&self, _budget_tokens: usize) -> String {
        String::new()
    }
    fn dependents(&self, _path: &str) -> Vec<SymbolRef> {
        Vec::new()
    }
    fn callers(&self, _symbol: &str) -> Vec<SymbolRef> {
        Vec::new()
    }
    fn changes(&self) -> Vec<FileChange> {
        Vec::new()
    }
    fn search(&self, _query: &str, _limit: usize) -> Vec<CodeLocation> {
        Vec::new()
    }
    fn file_outline(&self, _path: &Path) -> Option<String> {
        None
    }
}

// ---------------------------------------------------------------------------
// Concrete implementation wrapping existing infrastructure
// ---------------------------------------------------------------------------

use rustycode_tools::indexing::{watcher::FileEvent, CodeIndex, FileSystemWatcher, RepoMap};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Concrete intelligence backed by `RepoMap` + `CodeIndex`.
pub struct LocalIntelligence {
    root: PathBuf,
    /// Cached repo map — built lazily on first query.
    repo_map_cache: Mutex<Option<RepoMap>>,
    /// Code Index for fast lookups and updates.
    #[allow(dead_code)]
    index: Arc<Mutex<CodeIndex>>,
    /// Background watcher task handle.
    _watcher: FileSystemWatcher,
}

impl LocalIntelligence {
    pub fn new(root: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let root = root.into();
        let index = Arc::new(Mutex::new(CodeIndex::new(root.clone())));

        let (tx, mut rx) = mpsc::channel(100);
        let watcher = FileSystemWatcher::new(root.clone(), tx)
            .map_err(|e| anyhow::anyhow!("failed to start file watcher for {}: {e}", root.display()))?;

        let index_clone = index.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let mut idx = index_clone
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match event {
                    FileEvent::Created(path) | FileEvent::Modified(path) => {
                        let _ = idx.update_file(path);
                    }
                    FileEvent::Deleted(path) => {
                        let _ = idx.remove_file(path);
                    }
                }
            }
        });

        Ok(Self {
            root,
            repo_map_cache: Mutex::new(None),
            index,
            _watcher: watcher,
        })
    }

    #[allow(dead_code)]
    const fn snapshot_state() -> Vec<(PathBuf, std::time::SystemTime)> {
        Vec::new()
    }
}

impl CodeIntelligence for LocalIntelligence {
    fn repo_map(&self, budget_tokens: usize) -> String {
        let mut cache = self
            .repo_map_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.is_none() {
            match RepoMap::build(&self.root, budget_tokens) {
                Ok(map) => {
                    let output = map.to_map_string().to_string();
                    *cache = Some(map);
                    return output;
                }
                Err(e) => {
                    tracing::warn!("Failed to build repo map: {e}");
                    return String::new();
                }
            }
        }
        // Return cached — already within budget
        cache
            .as_ref()
            .map_or_else(String::new, |c| c.to_map_string().to_string())
    }

    fn dependents(&self, _path: &str) -> Vec<SymbolRef> {
        // Will be implemented with CodeIndex once wired
        Vec::new()
    }

    fn callers(&self, _symbol: &str) -> Vec<SymbolRef> {
        // Will be implemented with CodeIndex once wired
        Vec::new()
    }

    fn changes(&self) -> Vec<FileChange> {
        Vec::new()
    }

    fn search(&self, _query: &str, _limit: usize) -> Vec<CodeLocation> {
        // Will be implemented with CodeIndex once wired
        Vec::new()
    }

    fn file_outline(&self, path: &Path) -> Option<String> {
        let cache = self
            .repo_map_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.as_ref().and_then(|map| {
            map.for_file(path).map(|summary| {
                let mut lines = Vec::new();
                for sym in &summary.symbols {
                    lines.push(format!(
                        "L{}: {kind} {name}",
                        sym.line,
                        kind = sym.kind,
                        name = sym.name
                    ));
                }
                lines.join("\n")
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_returns_empty() {
        let intel = NoopIntelligence;
        assert!(intel.repo_map(2000).is_empty());
        assert!(intel.dependents("foo.rs").is_empty());
        assert!(intel.callers("bar").is_empty());
        assert!(intel.changes().is_empty());
        assert!(intel.search("query", 10).is_empty());
        assert!(intel.file_outline(Path::new("foo.rs")).is_none());
    }
}
