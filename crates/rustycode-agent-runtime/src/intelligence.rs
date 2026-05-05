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
    fn get_dependents(&self, path: &str) -> Vec<SymbolRef>;

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

// Noop implementation (testing / when no intelligence is available)

/// No-op intelligence — returns empty results for all queries.
/// Used when no code analysis infrastructure is available.
pub struct NoopIntelligence;

impl CodeIntelligence for NoopIntelligence {
    fn repo_map(&self, _budget_tokens: usize) -> String {
        String::new()
    }
    fn get_dependents(&self, _path: &str) -> Vec<SymbolRef> {
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

// Concrete implementation wrapping existing infrastructure

use rustycode_tools::indexing::{watcher::FileEvent, CodeIndex, FileSystemWatcher, RepoMap};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Concrete intelligence backed by `RepoMap` + `CodeIndex`.
pub struct LocalIntelligence {
    root: PathBuf,
    /// Cached repo map — built lazily on first query.
    repo_map_cache: Mutex<Option<RepoMap>>,
    /// Code Index for fast lookups and updates.
    index: Arc<Mutex<CodeIndex>>,
    /// Accumulated file changes from the watcher. Drained on each `changes()` call.
    changes: Arc<Mutex<Vec<FileChange>>>,
    /// Background watcher task handle.
    _watcher: FileSystemWatcher,
}

impl LocalIntelligence {
    pub fn new(root: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let root = root.into();
        let index = Arc::new(Mutex::new(CodeIndex::new(root.clone())));
        let changes: Arc<Mutex<Vec<FileChange>>> = Arc::new(Mutex::new(Vec::new()));

        let (tx, mut rx) = mpsc::channel(100);
        let watcher = FileSystemWatcher::new(root.clone(), tx).map_err(|e| {
            anyhow::anyhow!("failed to start file watcher for {}: {e}", root.display())
        })?;

        let index_clone = index.clone();
        let changes_clone = changes.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let change = match &event {
                    FileEvent::Created(path) | FileEvent::Modified(path) => {
                        let symbol_names = {
                            let mut idx = index_clone
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let _ = idx.update_file(path.clone());
                            idx.file_symbols(path)
                                .iter()
                                .map(|s| s.name.clone())
                                .collect::<Vec<_>>()
                        };
                        let change_type = match &event {
                            FileEvent::Created(_) => ChangeType::Created,
                            _ => ChangeType::Modified,
                        };
                        Some(FileChange {
                            path: path.clone(),
                            change_type,
                            symbols: symbol_names,
                        })
                    }
                    FileEvent::Deleted(path) => {
                        {
                            let mut idx = index_clone
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let _ = idx.remove_file(path.clone());
                        }
                        Some(FileChange {
                            path: path.clone(),
                            change_type: ChangeType::Deleted,
                            symbols: Vec::new(),
                        })
                    }
                };
                if let Some(change) = change {
                    changes_clone
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(change);
                }
            }
        });

        Ok(Self {
            root,
            repo_map_cache: Mutex::new(None),
            index,
            changes,
            _watcher: watcher,
        })
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

    fn get_dependents(&self, path: &str) -> Vec<SymbolRef> {
        let file_path = PathBuf::from(path);
        let idx = self
            .index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        idx.get_dependents(&file_path)
            .into_iter()
            .filter_map(|dep| {
                let symbols = idx.file_symbols(&dep);
                let sym = symbols.first()?;
                Some(SymbolRef {
                    name: sym.name.clone(),
                    file: dep,
                    line: sym.line,
                    kind: sym.kind.to_string(),
                })
            })
            .collect()
    }

    fn callers(&self, symbol: &str) -> Vec<SymbolRef> {
        let idx = self
            .index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        idx.find_symbols(symbol)
            .into_iter()
            .map(|s| SymbolRef {
                name: s.name.clone(),
                file: s.file_path.clone(),
                line: s.line,
                kind: s.kind.to_string(),
            })
            .collect()
    }

    fn changes(&self) -> Vec<FileChange> {
        std::mem::take(
            &mut *self
                .changes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    fn search(&self, query: &str, limit: usize) -> Vec<CodeLocation> {
        let idx = self
            .index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        idx.search(query)
            .into_iter()
            .take(limit)
            .map(|r| CodeLocation {
                file: r.file_path,
                line: r.line,
                symbol: String::new(),
                context: r.context,
            })
            .collect()
    }

    fn file_outline(&self, path: &Path) -> Option<String> {
        let outline = self
            .index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .file_outline(path);
        if outline.is_empty() {
            None
        } else {
            Some(outline)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_returns_empty() {
        let intel = NoopIntelligence;
        assert!(intel.repo_map(2000).is_empty());
        assert!(intel.get_dependents("foo.rs").is_empty());
        assert!(intel.callers("bar").is_empty());
        assert!(intel.changes().is_empty());
        assert!(intel.search("query", 10).is_empty());
        assert!(intel.file_outline(Path::new("foo.rs")).is_none());
    }
}
