//! File system watcher service
//!
//! Watches for file changes and triggers incremental indexing updates.

use anyhow::Result;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

/// Event type for file changes
#[derive(Debug, Clone)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
}

/// Service that watches for file system changes
#[allow(dead_code)]
pub struct FileSystemWatcher {
    watcher: RecommendedWatcher,
    event_tx: mpsc::Sender<FileEvent>,
}

impl FileSystemWatcher {
    /// Create a new watcher service
    pub fn new(root: impl AsRef<Path>, event_tx: mpsc::Sender<FileEvent>) -> Result<Self> {
        let tx = event_tx.clone();

        // Define watcher callback
        let watcher_handler = move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let file_event = match event.kind {
                        notify::EventKind::Create(_) => FileEvent::Created(path),
                        notify::EventKind::Modify(_) => FileEvent::Modified(path),
                        notify::EventKind::Remove(_) => FileEvent::Deleted(path),
                        _ => continue,
                    };
                    let _ = tx.blocking_send(file_event);
                }
            }
        };

        let mut watcher = RecommendedWatcher::new(watcher_handler, Config::default())?;

        watcher.watch(root.as_ref(), RecursiveMode::Recursive)?;

        Ok(Self { watcher, event_tx })
    }
}
