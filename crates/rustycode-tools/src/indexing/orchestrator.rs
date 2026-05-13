use crate::indexing::code_index::CodeIndex;
use crate::indexing::repo_map::RepoMap;
use crate::indexing::symbols::{extract_file, compute_structural_hash};
use crate::indexing::watcher::{FileSystemWatcher, FileEvent};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Coordinates incremental updates for all code indices.
pub struct IndexOrchestrator {
    pub root: PathBuf,
    pub code_index: Arc<Mutex<CodeIndex>>,
    pub repo_map: Arc<Mutex<RepoMap>>,
    /// file_path -> structural_hash
    hashes: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl IndexOrchestrator {
    pub fn new(root: PathBuf) -> Result<Self> {
        Ok(Self {
            code_index: Arc::new(Mutex::new(CodeIndex::new(root.clone()))),
            repo_map: Arc::new(Mutex::new(RepoMap::build(&root, 4000)?)),
            hashes: Arc::new(Mutex::new(HashMap::new())),
            root,
        })
    }

    /// Start the background watcher and listen for events.
    pub async fn start(self: Arc<Self>) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(100);
        let _watcher = FileSystemWatcher::new(&self.root, tx)?;
        
        tracing::info!("IndexOrchestrator started for {}", self.root.display());

        while let Some(event) = rx.recv().await {
            match event {
                FileEvent::Created(path) | FileEvent::Modified(path) => {
                    if let Err(e) = self.handle_update(&path).await {
                        tracing::error!("Failed to update index for {}: {}", path.display(), e);
                    }
                }
                FileEvent::Deleted(path) => {
                    if let Err(e) = self.handle_delete(&path).await {
                        tracing::error!("Failed to remove index for {}: {}", path.display(), e);
                    }
                }
            }
        }
        
        Ok(())
    }

    async fn handle_update(&self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let outline = extract_file(path, &content);
        let new_hash = compute_structural_hash(&outline);
        
        let mut hashes = self.hashes.lock().await;
        let old_hash = hashes.get(path).cloned();
        
        let mut code_index = self.code_index.lock().await;
        code_index.update_file(path.to_path_buf())?;
        
        if Some(new_hash.clone()) != old_hash {
            tracing::debug!("Structural change detected in {}", path.display());
            hashes.insert(path.to_path_buf(), new_hash);
            
            // Trigger RepoMap refresh (lazy or immediate)
            // For now, just mark it as dirty in our mental model.
            // In a real impl, RepoMap might have a 'mark_dirty' method.
        }
        
        Ok(())
    }

    async fn handle_delete(&self, path: &Path) -> Result<()> {
        let mut hashes = self.hashes.lock().await;
        hashes.remove(path);
        
        let mut code_index = self.code_index.lock().await;
        code_index.remove_file(path.to_path_buf())?;
        
        Ok(())
    }
}
