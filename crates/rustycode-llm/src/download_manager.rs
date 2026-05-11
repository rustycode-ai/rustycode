//! Download Manager with Progress Tracking
//!
//! Robust async download manager for large files (models, datasets, assets)
//! with progress tracking, speed/ETA calculation, cancellation, and automatic
//! cleanup of partial downloads.
//!
//! Inspired by goose's `download_manager.rs`.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_llm::download_manager::{DownloadManager, get_download_manager};
//!
//! let manager = get_download_manager();
//! manager.download_model(
//!     "llama-3.1-8b".to_string(),
//!     "https://example.com/model.bin".to_string(),
//!     PathBuf::from("/models/llama-3.1-8b.bin"),
//!     None,
//! ).await?;
//!
//! // Check progress
//! if let Some(progress) = manager.progress("llama-3.1-8b") {
//!     println!("{}% ({}/{} bytes)", progress.progress_percent(), progress.bytes_downloaded, progress.total_bytes);
//! }
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;

// ── Types ───────────────────────────────────────────────────────────────────

/// Status of a download operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DownloadStatus {
    /// Currently downloading
    Downloading,
    /// Download completed successfully
    Completed,
    /// Download failed with an error
    Failed,
    /// Download was cancelled by user
    Cancelled,
}

impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Downloading => write!(f, "downloading"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            #[allow(unreachable_patterns)]
            _ => write!(f, "unknown"),
        }
    }
}

/// Progress information for an active or completed download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Identifier for the download (typically model name or ID)
    pub id: String,
    /// Current download status
    pub status: DownloadStatus,
    /// Bytes downloaded so far
    pub bytes_downloaded: u64,
    /// Total bytes to download (0 if unknown)
    pub total_bytes: u64,
    /// Error message if the download failed
    pub error: Option<String>,
    /// Whether the background download task has exited
    #[serde(skip)]
    task_exited: bool,
}

impl DownloadProgress {
    /// Progress as a percentage (0.0 - 100.0).
    ///
    /// Returns 0.0 if total size is unknown.
    pub fn progress_percent(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.bytes_downloaded as f64 / self.total_bytes as f64 * 100.0) as f32
    }

    /// Download speed in bytes per second.
    pub fn speed_bps(&self, elapsed: std::time::Duration) -> Option<u64> {
        let secs = elapsed.as_secs_f64();
        if secs > 0.0 {
            Some((self.bytes_downloaded as f64 / secs) as u64)
        } else {
            None
        }
    }

    /// Estimated time remaining in seconds.
    pub fn eta_seconds(&self, speed_bps: u64) -> Option<u64> {
        if speed_bps > 0 && self.total_bytes > self.bytes_downloaded {
            Some((self.total_bytes - self.bytes_downloaded) / speed_bps)
        } else {
            None
        }
    }

    /// Human-readable download size.
    pub fn human_downloaded(&self) -> String {
        format_bytes(self.bytes_downloaded)
    }

    /// Human-readable total size.
    pub fn human_total(&self) -> String {
        format_bytes(self.total_bytes)
    }
}

// ── Download Manager ────────────────────────────────────────────────────────

/// Manages multiple concurrent downloads with progress tracking.
///
/// Thread-safe via `Arc<Mutex<>>` — safe to share across threads.
pub struct DownloadManager {
    downloads: Arc<Mutex<HashMap<String, DownloadProgress>>>,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            downloads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the progress for a specific download.
    pub fn progress(&self, id: &str) -> Option<DownloadProgress> {
        let guard = match self.downloads.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("Download lock poisoned: {}", e);
                return None;
            }
        };
        guard.get(id).cloned()
    }

    /// Get all active downloads.
    pub fn active_downloads(&self) -> Vec<DownloadProgress> {
        self.downloads
            .lock()
            .map(|d| {
                d.values()
                    .filter(|p| p.status == DownloadStatus::Downloading)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Cancel an active download.
    pub fn cancel_download(&self, id: &str) -> Result<()> {
        let mut downloads = self
            .downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire download lock"))?;

        if let Some(progress) = downloads.get_mut(id) {
            progress.status = DownloadStatus::Cancelled;
            Ok(())
        } else {
            anyhow::bail!("Download '{}' not found", id)
        }
    }

    /// Clear a completed/failed/cancelled download from tracking.
    pub fn clear(&self, id: &str) {
        if let Ok(mut downloads) = self.downloads.lock() {
            if let Some(progress) = downloads.get(id) {
                let is_terminal = progress.status != DownloadStatus::Downloading;
                if is_terminal && progress.task_exited {
                    downloads.remove(id);
                }
            }
        }
    }

    /// Start downloading a file in the background.
    ///
    /// The download runs in a spawned tokio task. Progress can be tracked
    /// via `get_progress()`.
    ///
    pub async fn download(
        &self,
        id: String,
        url: String,
        destination: PathBuf,
        on_complete: Option<Box<dyn FnOnce() + Send + 'static>>,
    ) -> Result<()> {
        tracing::info!(id = %id, url = %url, "Starting download");

        // Check for duplicate active downloads
        {
            let mut downloads = self
                .downloads
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to acquire download lock"))?;

            if let Some(existing) = downloads.get(&id) {
                if existing.status == DownloadStatus::Downloading {
                    anyhow::bail!("Download '{}' already in progress", id);
                }
                if existing.status == DownloadStatus::Cancelled && !existing.task_exited {
                    anyhow::bail!(
                        "Download '{}' is being cancelled; wait for it to finish",
                        id
                    );
                }
            }

            downloads.insert(
                id.clone(),
                DownloadProgress {
                    id: id.clone(),
                    status: DownloadStatus::Downloading,
                    bytes_downloaded: 0,
                    total_bytes: 0,
                    error: None,
                    task_exited: false,
                },
            );
        }

        // Create parent directory
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create directory: {}", e))?;
        }

        let downloads = self.downloads.clone();
        let id_clone = id.clone();
        let destination_for_cleanup = destination.clone();

        tokio::spawn(async move {
            match Self::download_file(&url, &destination, &downloads, &id_clone).await {
                Ok(()) => {
                    tracing::info!(id = %id_clone, "Download completed");
                    if let Ok(mut dl) = downloads.lock() {
                        if let Some(progress) = dl.get_mut(&id_clone) {
                            progress.status = DownloadStatus::Completed;
                            progress.task_exited = true;
                        }
                    }
                    if let Some(callback) = on_complete {
                        callback();
                    }
                }
                Err(e) => {
                    // Clean up partial file
                    let partial = partial_path(&destination_for_cleanup);
                    if let Err(cleanup_err) = tokio::fs::remove_file(&partial).await {
                        tracing::warn!(
                            path = %partial.display(),
                            error = %cleanup_err,
                            "Failed to clean up partial download after failure"
                        );
                    }

                    if let Ok(mut dl) = downloads.lock() {
                        if let Some(progress) = dl.get_mut(&id_clone) {
                            if progress.status != DownloadStatus::Cancelled {
                                progress.status = DownloadStatus::Failed;
                            }
                            progress.error = Some(e.to_string());
                            progress.task_exited = true;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Internal: perform the actual file download.
    async fn download_file(
        url: &str,
        destination: &Path,
        downloads: &Arc<Mutex<HashMap<String, DownloadProgress>>>,
        id: &str,
    ) -> Result<(), anyhow::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_mins(10))
            .build()?;

        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            anyhow::bail!("HTTP {} downloading {}: {}", status, url, error_text);
        }

        let total_bytes = response.content_length().unwrap_or(0);

        // Update total size
        if let Ok(mut dl) = downloads.lock() {
            if let Some(progress) = dl.get_mut(id) {
                progress.total_bytes = total_bytes;
            }
        }

        let partial = partial_path(destination);
        let mut file = tokio::fs::File::create(&partial).await?;
        let mut bytes_downloaded = 0u64;

        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            // Check cancellation
            let should_cancel = {
                if let Ok(dl) = downloads.lock() {
                    dl.get(id)
                        .map(|p| p.status == DownloadStatus::Cancelled)
                        .unwrap_or(false)
                } else {
                    false
                }
            };

            if should_cancel {
                if let Err(cleanup_err) = tokio::fs::remove_file(&partial).await {
                    tracing::warn!(
                        path = %partial.display(),
                        error = %cleanup_err,
                        "Failed to clean up partial download on cancellation"
                    );
                }
                anyhow::bail!("Download cancelled");
            }

            file.write_all(&chunk).await?;
            bytes_downloaded += chunk.len() as u64;

            // Update progress
            if let Ok(mut dl) = downloads.lock() {
                if let Some(progress) = dl.get_mut(id) {
                    progress.bytes_downloaded = bytes_downloaded;
                }
            }
        }

        file.flush().await?;
        drop(file);

        // Atomic rename from partial to final
        tokio::fs::rename(&partial, destination).await?;
        Ok(())
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Get the partial download path (appends `.part` to the extension).
fn partial_path(destination: &Path) -> PathBuf {
    destination.with_extension(
        destination
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    )
}

/// Remove leftover `.part` files in a directory.
pub fn cleanup_partial_downloads(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "part") {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to clean up partial download file"
                    );
                }
            }
        }
    }
}

/// Format bytes as human-readable string (e.g., "1.5 GiB").
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GiB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MiB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KiB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ── Global Singleton ────────────────────────────────────────────────────────

static DOWNLOAD_MANAGER: once_cell::sync::Lazy<DownloadManager> =
    once_cell::sync::Lazy::new(DownloadManager::new);

/// Get the global download manager singleton.
pub fn download_manager() -> &'static DownloadManager {
    &DOWNLOAD_MANAGER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_progress_percent() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Downloading,
            bytes_downloaded: 50,
            total_bytes: 100,
            error: None,
            task_exited: false,
        };
        assert!((p.progress_percent() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_download_progress_percent_unknown_total() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Downloading,
            bytes_downloaded: 50,
            total_bytes: 0,
            error: None,
            task_exited: false,
        };
        assert!((p.progress_percent() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_download_progress_speed() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Downloading,
            bytes_downloaded: 1000,
            total_bytes: 10000,
            error: None,
            task_exited: false,
        };
        let speed = p.speed_bps(std::time::Duration::from_secs(1));
        assert_eq!(speed, Some(1000));
    }

    #[test]
    fn test_download_progress_eta() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Downloading,
            bytes_downloaded: 5000,
            total_bytes: 10000,
            error: None,
            task_exited: false,
        };
        let eta = p.eta_seconds(1000);
        assert_eq!(eta, Some(5));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GiB");
        assert_eq!(format_bytes(1536 * 1024 * 1024), "1.5 GiB");
    }

    #[test]
    fn test_cancel_nonexistent_download() {
        let manager = DownloadManager::new();
        assert!(manager.cancel_download("nonexistent").is_err());
    }

    #[test]
    fn test_get_progress_nonexistent() {
        let manager = DownloadManager::new();
        assert!(manager.progress("nonexistent").is_none());
    }

    #[test]
    fn test_active_downloads_empty() {
        let manager = DownloadManager::new();
        assert!(manager.active_downloads().is_empty());
    }

    #[test]
    fn test_clear_downloading_not_cleared() {
        let manager = DownloadManager::new();
        manager.downloads.lock().unwrap().insert(
            "test".to_string(),
            DownloadProgress {
                id: "test".to_string(),
                status: DownloadStatus::Downloading,
                bytes_downloaded: 0,
                total_bytes: 0,
                error: None,
                task_exited: false,
            },
        );
        manager.clear("test");
        // Should NOT be cleared because it's still downloading
        assert!(manager.progress("test").is_some());
    }

    #[test]
    fn test_clear_completed_is_cleared() {
        let manager = DownloadManager::new();
        manager.downloads.lock().unwrap().insert(
            "test".to_string(),
            DownloadProgress {
                id: "test".to_string(),
                status: DownloadStatus::Completed,
                bytes_downloaded: 100,
                total_bytes: 100,
                error: None,
                task_exited: true,
            },
        );
        manager.clear("test");
        assert!(manager.progress("test").is_none());
    }

    #[test]
    fn test_cancel_sets_status() {
        let manager = DownloadManager::new();
        manager.downloads.lock().unwrap().insert(
            "test".to_string(),
            DownloadProgress {
                id: "test".to_string(),
                status: DownloadStatus::Downloading,
                bytes_downloaded: 0,
                total_bytes: 100,
                error: None,
                task_exited: false,
            },
        );
        manager.cancel_download("test").unwrap();
        let progress = manager.progress("test").unwrap();
        assert_eq!(progress.status, DownloadStatus::Cancelled);
    }

    #[test]
    fn test_partial_path() {
        assert_eq!(
            partial_path(Path::new("/models/llama.bin")),
            PathBuf::from("/models/llama.bin.part")
        );
        assert_eq!(
            partial_path(Path::new("/models/llama")),
            PathBuf::from("/models/llama.part")
        );
        assert_eq!(
            partial_path(Path::new("/models/archive.tar.gz")),
            PathBuf::from("/models/archive.tar.gz.part")
        );
    }

    #[test]
    fn test_download_status_display() {
        assert_eq!(format!("{}", DownloadStatus::Downloading), "downloading");
        assert_eq!(format!("{}", DownloadStatus::Completed), "completed");
        assert_eq!(format!("{}", DownloadStatus::Failed), "failed");
        assert_eq!(format!("{}", DownloadStatus::Cancelled), "cancelled");
    }

    #[test]
    fn test_global_singleton() {
        let manager = download_manager();
        assert!(manager.active_downloads().is_empty());
    }

    #[test]
    fn test_duplicate_download_rejected() {
        let manager = DownloadManager::new();
        manager.downloads.lock().unwrap().insert(
            "test".to_string(),
            DownloadProgress {
                id: "test".to_string(),
                status: DownloadStatus::Downloading,
                bytes_downloaded: 0,
                total_bytes: 100,
                error: None,
                task_exited: false,
            },
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(manager.download(
            "test".to_string(),
            "http://example.com/file".to_string(),
            PathBuf::from("/tmp/test"),
            None,
        ));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already in progress"));
    }

    #[test]
    fn test_download_status_serde_roundtrip() {
        for status in [
            DownloadStatus::Downloading,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
            DownloadStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: DownloadStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn test_download_progress_speed_zero_duration() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Downloading,
            bytes_downloaded: 1000,
            total_bytes: 10000,
            error: None,
            task_exited: false,
        };
        let speed = p.speed_bps(std::time::Duration::from_secs(0));
        assert_eq!(speed, None);
    }

    #[test]
    fn test_download_progress_eta_zero_speed() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Downloading,
            bytes_downloaded: 5000,
            total_bytes: 10000,
            error: None,
            task_exited: false,
        };
        let eta = p.eta_seconds(0);
        assert_eq!(eta, None);
    }

    #[test]
    fn test_download_progress_eta_complete() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Downloading,
            bytes_downloaded: 10000,
            total_bytes: 10000,
            error: None,
            task_exited: false,
        };
        let eta = p.eta_seconds(1000);
        assert_eq!(eta, None); // Already complete
    }

    #[test]
    fn test_human_readable_sizes() {
        let p = DownloadProgress {
            id: "test".to_string(),
            status: DownloadStatus::Completed,
            bytes_downloaded: 1536 * 1024,
            total_bytes: 2 * 1024 * 1024,
            error: None,
            task_exited: true,
        };
        assert_eq!(p.human_downloaded(), "1.5 MiB");
        assert_eq!(p.human_total(), "2.0 MiB");
    }
}
