use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio_stream::Stream;
use tracing;

use crate::installer;
use crate::process::ProcessPool;

/// Manages LiteRT-LM binary lifecycle and per-model process pools.
///
/// Replaces the upstream `litert_lm::LitManager` with our own installer-based
/// binary discovery and [`ProcessPool`]-backed inference. Each unique model
/// name maps to its own pool so that models are loaded independently.
#[derive(Debug, Clone)]
pub struct LitManager {
    binary_path: Arc<RwLock<Option<PathBuf>>>,
    pools: Arc<Mutex<HashMap<String, Arc<ProcessPool>>>>,
    pool_size: usize,
}

impl LitManager {
    /// Create a new [`LitManager`] by downloading / verifying the LiteRT-LM
    /// binary via [`installer::ensure_litert_lm_binary`].
    ///
    /// A default pool size of 2 is used.
    pub async fn new() -> Result<Self> {
        let config = installer::LiteRtLmInstallConfig::default();
        let binary_path = installer::ensure_litert_lm_binary(&config).await?;
        tracing::info!(path = %binary_path.display(), "LiteRT-LM binary ready");
        Ok(Self {
            binary_path: Arc::new(RwLock::new(Some(binary_path))),
            pools: Arc::new(Mutex::new(HashMap::new())),
            pool_size: 2,
        })
    }

    /// Create a [`LitManager`] with a pre-known binary path.
    ///
    /// Intended for tests that want to skip the download step entirely.
    pub fn with_binary_path(binary_path: PathBuf, pool_size: usize) -> Self {
        Self {
            binary_path: Arc::new(RwLock::new(Some(binary_path))),
            pools: Arc::new(Mutex::new(HashMap::new())),
            pool_size,
        }
    }

    /// Return the path to the LiteRT-LM binary, downloading it if necessary.
    ///
    /// Uses a double-check locking pattern: first a read lock is taken and, if
    /// the path is already present, it is returned immediately. Otherwise the
    /// read lock is released, a write lock is acquired, and — after a second
    /// check (another task may have resolved the path in the meantime) — the
    /// binary is ensured via the installer.
    async fn ensure_binary(&self) -> Result<PathBuf> {
        // Fast path: read lock
        let read_lock = self.binary_path.read().await;
        if let Some(path) = read_lock.as_ref() {
            tracing::trace!(path = %path.display(), "Binary path already cached");
            return Ok(path.clone());
        }
        drop(read_lock);

        // Slow path: write lock
        tracing::debug!("Binary path not cached, acquiring write lock");
        let mut write_lock = self.binary_path.write().await;

        // Double-check after acquiring write lock
        if let Some(path) = write_lock.as_ref() {
            tracing::trace!(path = %path.display(), "Binary path set by another task");
            return Ok(path.clone());
        }

        tracing::info!("Ensuring LiteRT-LM binary is available");
        let config = installer::LiteRtLmInstallConfig::default();
        let path = installer::ensure_litert_lm_binary(&config)
            .await
            .context("failed to ensure LiteRT-LM binary")?;
        tracing::info!(path = %path.display(), "LiteRT-LM binary obtained");
        *write_lock = Some(path.clone());
        drop(write_lock);
        Ok(path)
    }

    /// Return (or lazily create) the [`ProcessPool`] for the given model.
    ///
    /// If a pool already exists for *model* its `Arc` clone is returned.
    /// Otherwise a new pool is created, initialized, and stored for future
    /// reuse.
    async fn get_pool(&self, model: &str) -> Result<Arc<ProcessPool>> {
        let pools = self.pools.lock().await;

        if let Some(pool) = pools.get(model) {
            tracing::debug!(model = %model, "Reusing existing process pool");
            return Ok(pool.clone());
        }

        drop(pools);

        tracing::info!(model = %model, pool_size = self.pool_size, "Creating new process pool");

        let binary_path = self.ensure_binary().await?;
        let mut pool = ProcessPool::new(binary_path, model.to_string(), self.pool_size);
        pool.initialize()
            .context("failed to initialize process pool")?;

        let pool_arc = Arc::new(pool);
        self.pools
            .lock()
            .await
            .insert(model.to_string(), pool_arc.clone());
        tracing::info!(model = %model, "Process pool created and initialized");
        Ok(pool_arc)
    }

    /// Run a single completion request and return the full response.
    pub async fn run_completion(&self, model: &str, prompt: &str) -> Result<String> {
        tracing::debug!(model = %model, prompt_length = prompt.len(), "Running completion");
        let pool = self.get_pool(model).await?;
        let response = pool.send_prompt(prompt).await?;
        tracing::debug!(model = %model, response_length = response.len(), "Completion finished");
        Ok(response)
    }

    /// Run a streaming completion request.
    ///
    /// Returns a [`Stream`] that yields response chunks as `Result<String>`.
    pub async fn run_completion_stream(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<impl Stream<Item = Result<String>>> {
        tracing::debug!(model = %model, prompt_length = prompt.len(), "Starting streaming completion");
        let pool = self.get_pool(model).await?;
        let process = pool.process()?;
        let stream = process.send_prompt_stream(prompt).await?;
        Ok(stream)
    }

    /// Ensure a model is available locally.
    ///
    /// This is a placeholder — model downloading is not yet implemented.
    /// Models must either be pre-installed or the model name must resolve to
    /// a file that already exists on disk.
    pub const fn ensure_model(&self, _model_source: &str, _alias: Option<&str>) -> Result<()> {
        // TODO: Implement model download/management
        // For now, models must be pre-installed or the model name resolves to a file on disk
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_binary_path_creates_manager() {
        let mgr = LitManager::with_binary_path(PathBuf::from("/usr/local/bin/litert_lm_main"), 3);
        assert_eq!(mgr.pool_size, 3);
    }

    #[test]
    fn manager_debug_format() {
        let mgr = LitManager::with_binary_path(PathBuf::from("/usr/local/bin/litert"), 2);
        let debug = format!("{mgr:?}");
        assert!(debug.contains("LitManager"));
    }

    #[test]
    fn ensure_model_is_noop() {
        let mgr = LitManager::with_binary_path(PathBuf::from("/fake"), 1);
        let result = mgr.ensure_model("gemma", Some("default"));
        assert!(result.is_ok());
    }

    #[test]
    fn manager_clone_preserves_state() {
        let mgr = LitManager::with_binary_path(PathBuf::from("/fake/binary"), 4);
        let cloned = mgr.clone();
        assert_eq!(cloned.pool_size, mgr.pool_size);
    }
}
