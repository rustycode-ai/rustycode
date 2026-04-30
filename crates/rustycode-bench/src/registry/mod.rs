//! Remote dataset registry — downloads tasks from GitHub.
//!
//! Fetches the Harbor `registry.json` to discover datasets, then
//! clones/downloads task directories to the local cache.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

/// Default Harbor cache directory.
const HARBOR_CACHE_DIR: &str = ".cache/harbor";

/// URL for the Harbor registry.
const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/harbor-framework/harbor/main/registry.json";

/// Default TB2 dataset git URL.
const TB2_GIT_URL: &str = "https://github.com/harbor-framework/terminal-bench-2";

/// A dataset entry from the Harbor registry.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryDataset {
    pub name: String,
    pub version: String,
    #[allow(dead_code)]
    pub description: String,
    pub tasks: Vec<RegistryTask>,
}

/// A task reference within a dataset.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryTask {
    pub name: String,
    pub git_url: String,
    pub git_commit_id: String,
    pub path: String,
}

/// Manages downloading and caching remote datasets.
pub struct RegistryDownloader {
    cache_dir: PathBuf,
    client: reqwest::Client,
}

impl RegistryDownloader {
    /// Create a new downloader using the default cache directory.
    pub fn new() -> Self {
        let cache_dir = Self::default_cache_dir();
        Self {
            cache_dir,
            client: reqwest::Client::new(),
        }
    }

    /// Create with a custom cache directory.
    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            client: reqwest::Client::new(),
        }
    }

    /// Default cache directory: `~/.cache/harbor/`.
    pub fn default_cache_dir() -> PathBuf {
        dirs_home().join(HARBOR_CACHE_DIR)
    }

    /// Resolve a dataset reference (e.g. "terminal-bench@2.0") to a local path,
    /// downloading if necessary.
    pub async fn resolve(&self, reference: &str) -> anyhow::Result<PathBuf> {
        // Try as a direct path first
        let direct = PathBuf::from(reference);
        if direct.exists() {
            return Ok(direct);
        }

        // Parse name@version
        let (name, version) = match reference.split_once('@') {
            Some((n, v)) => (n, Some(v)),
            None => (reference, None),
        };

        // Check local cache
        if let Some(cached) = self.check_cache(name, version) {
            tracing::info!("Using cached dataset: {}", cached.display());
            return Ok(cached);
        }

        // Download from registry
        self.download(name, version).await
    }

    /// Check if a dataset exists in the local cache.
    fn check_cache(&self, name: &str, version: Option<&str>) -> Option<PathBuf> {
        let cache_path = if let Some(v) = version {
            self.cache_dir.join("tasks").join(format!("{name}@{v}"))
        } else {
            // Search for any version
            let tasks_dir = self.cache_dir.join("tasks");
            let entries = std::fs::read_dir(&tasks_dir).ok()?;
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_str = file_name.to_string_lossy();
                if file_str.starts_with(&format!("{name}@")) || file_str == name {
                    return Some(entry.path());
                }
            }
            return None;
        };

        if cache_path.exists() && has_tasks(&cache_path) {
            Some(cache_path)
        } else {
            None
        }
    }

    /// Download a dataset from the registry.
    async fn download(&self, name: &str, version: Option<&str>) -> anyhow::Result<PathBuf> {
        // Special case: terminal-bench downloads from the TB2 repo directly
        if name == "terminal-bench" || name == "terminal-bench-2" {
            return self.download_tb2(version);
        }

        // Fetch registry
        let datasets = self.fetch_registry().await?;
        let dataset = datasets
            .iter()
            .find(|d| d.name == name && version.is_none_or(|v| d.version == v))
            .ok_or_else(|| {
                let available: Vec<String> = datasets.iter().map(|d| d.name.clone()).collect();
                anyhow::anyhow!(
                    "Dataset '{name}' not found in registry. Available: {}",
                    available.join(", ")
                )
            })?;

        let dest_dir = self
            .cache_dir
            .join("tasks")
            .join(format!("{}@{}", dataset.name, dataset.version));
        std::fs::create_dir_all(&dest_dir)?;

        // Download tasks via git archive (lightweight — no full clone)
        let pb = indicatif::ProgressBar::new(dataset.tasks.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::with_template("{msg} {pos}/{len} [{bar}] {eta}")?
                .progress_chars("=>-"),
        );
        pb.set_message("Downloading tasks");

        for task in &dataset.tasks {
            self.download_task(task, &dest_dir)?;
            pb.inc(1);
        }
        pb.finish_with_message("Done");

        Ok(dest_dir)
    }

    /// Download Terminal Bench 2.0 tasks directly from GitHub.
    fn download_tb2(&self, version: Option<&str>) -> anyhow::Result<PathBuf> {
        let version_str = version.unwrap_or("2.0");
        let dest_dir = self
            .cache_dir
            .join("tasks")
            .join(format!("terminal-bench@{version_str}"));

        if dest_dir.exists() && has_tasks(&dest_dir) {
            tracing::info!("TB2 already cached at {}", dest_dir.display());
            return Ok(dest_dir);
        }

        std::fs::create_dir_all(&dest_dir)?;

        tracing::info!("Downloading Terminal Bench 2.0 tasks...");

        // Clone the repo (shallow) to a temp dir, then copy task directories
        let tmp_dir = dest_dir.join(".tmp-clone");
        if tmp_dir.exists() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
        }

        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.depth(1);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_opts);

        match builder.clone(TB2_GIT_URL, &tmp_dir) {
            Ok(_) => {
                // Copy task directories from the clone
                let entries = std::fs::read_dir(&tmp_dir)?;
                let mut count = 0;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && path.join("task.toml").exists() {
                        let task_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let dest = dest_dir.join(task_name.as_ref());
                        if !dest.exists() {
                            copy_dir_recursive(&path, &dest)?;
                            count += 1;
                        }
                    }
                }

                // Clean up clone
                let _ = std::fs::remove_dir_all(&tmp_dir);

                tracing::info!("Downloaded {count} TB2 tasks to {}", dest_dir.display());
                Ok(dest_dir)
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&tmp_dir);
                bail!("Failed to clone TB2 repository: {e}")
            }
        }
    }

    /// Download a single task from a git repo.
    fn download_task(&self, task: &RegistryTask, dest_dir: &Path) -> anyhow::Result<()> {
        let task_dir = dest_dir.join(&task.name);
        if task_dir.exists() {
            return Ok(());
        }

        // For tasks from a shared git repo, we clone shallowly and extract
        let tmp_dir = dest_dir.join(format!(".tmp-{}", task.name));
        std::fs::create_dir_all(&tmp_dir)?;

        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.depth(1);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_opts);

        // Try to checkout the specific commit
        match builder.clone(&task.git_url, &tmp_dir) {
            Ok(repo) => {
                // Resolve the commit OID — fall back to HEAD if the stored ID is invalid
                let oid = git2::Oid::from_str(&task.git_commit_id).unwrap_or_else(|_| {
                    repo.revparse_single("HEAD")
                        .map(|obj| obj.id())
                        .unwrap_or_else(|_| git2::Oid::zero())
                });

                if !oid.is_zero() {
                    if let Ok(commit) = repo.find_commit(oid) {
                        let _ = repo.checkout_tree(commit.as_object(), None);
                    }
                }

                // Copy the specific path
                let src = tmp_dir.join(&task.path);
                if src.exists() {
                    if let Err(e) = copy_dir_recursive(&src, &task_dir) {
                        tracing::warn!("Failed to copy task {}: {e}, skipping", task.name);
                    }
                }
            }
            Err(e) => {
                // Fallback: just try to copy what we can
                tracing::warn!("Git clone failed for {}: {e}, skipping", task.name);
            }
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
        Ok(())
    }

    /// Fetch the registry index from GitHub.
    async fn fetch_registry(&self) -> anyhow::Result<Vec<RegistryDataset>> {
        let response = self
            .client
            .get(REGISTRY_URL)
            .send()
            .await
            .context("Failed to fetch registry")?;

        if !response.status().is_success() {
            bail!("Registry fetch failed: HTTP {}", response.status());
        }

        let datasets: Vec<RegistryDataset> =
            response.json().await.context("Failed to parse registry")?;

        Ok(datasets)
    }

    /// List available datasets from the registry.
    pub async fn list_remote(&self) -> anyhow::Result<Vec<RegistryDataset>> {
        self.fetch_registry().await
    }

    /// List locally cached datasets.
    pub fn list_cached(&self) -> Vec<CachedDataset> {
        let tasks_dir = self.cache_dir.join("tasks");
        let mut datasets = Vec::new();

        let Ok(entries) = std::fs::read_dir(&tasks_dir) else {
            return datasets;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let task_count = count_tasks_in_dir(&path);
            if task_count > 0 {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                datasets.push(CachedDataset {
                    name,
                    path,
                    task_count,
                });
            }
        }

        datasets
    }
}

impl Default for RegistryDownloader {
    fn default() -> Self {
        Self::new()
    }
}

/// A locally cached dataset.
#[derive(Debug, Clone)]
pub struct CachedDataset {
    pub name: String,
    pub path: PathBuf,
    pub task_count: usize,
}

/// Get the user's home directory.
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// Check if a directory contains tasks (directories with task.toml).
fn has_tasks(dir: &Path) -> bool {
    count_tasks_in_dir(dir) > 0
}

/// Count task directories.
fn count_tasks_in_dir(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("task.toml").exists())
        .count()
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dest: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    let entries = std::fs::read_dir(src)?;
    for entry in entries.flatten() {
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn dirs_home_returns_something() {
        let home = dirs_home();
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn has_tasks_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_tasks(dir.path()));
    }

    #[test]
    fn has_tasks_with_task() {
        let dir = tempfile::tempdir().unwrap();
        let task = dir.path().join("my-task");
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("task.toml"), "").unwrap();
        assert!(has_tasks(dir.path()));
    }

    #[test]
    fn count_tasks_in_dir_counts_correctly() {
        let dir = tempfile::tempdir().unwrap();
        for name in &["a", "b", "c"] {
            let task = dir.path().join(name);
            std::fs::create_dir_all(&task).unwrap();
            std::fs::write(task.join("task.toml"), "").unwrap();
        }
        assert_eq!(count_tasks_in_dir(dir.path()), 3);
    }

    #[test]
    fn copy_dir_recursive_works() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();

        std::fs::write(src.path().join("file.txt"), "hello").unwrap();
        let sub = src.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("nested.txt"), "world").unwrap();

        let dest_path = dest.path().join("copy");
        copy_dir_recursive(src.path(), &dest_path).unwrap();

        assert!(dest_path.join("file.txt").exists());
        assert!(dest_path.join("sub/nested.txt").exists());
    }

    #[test]
    fn check_cache_finds_cached_dataset() {
        let dir = tempfile::tempdir().unwrap();
        let downloader = RegistryDownloader::with_cache_dir(dir.path().to_path_buf());

        // Create a cached dataset
        let tasks_dir = dir.path().join("tasks").join("my-dataset@1.0");
        let task = tasks_dir.join("task-one");
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("task.toml"), "").unwrap();

        let found = downloader.check_cache("my-dataset", Some("1.0"));
        assert!(found.is_some());
    }

    #[test]
    fn check_cache_misses_unknown_dataset() {
        let dir = tempfile::tempdir().unwrap();
        let downloader = RegistryDownloader::with_cache_dir(dir.path().to_path_buf());

        let found = downloader.check_cache("nonexistent", None);
        assert!(found.is_none());
    }

    #[test]
    fn list_cached_empty() {
        let dir = tempfile::tempdir().unwrap();
        let downloader = RegistryDownloader::with_cache_dir(dir.path().to_path_buf());
        assert!(downloader.list_cached().is_empty());
    }

    #[test]
    fn list_cached_finds_datasets() {
        let dir = tempfile::tempdir().unwrap();
        let downloader = RegistryDownloader::with_cache_dir(dir.path().to_path_buf());

        let tasks_dir = dir.path().join("tasks").join("ds@1.0");
        let task = tasks_dir.join("t1");
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("task.toml"), "").unwrap();

        let cached = downloader.list_cached();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].task_count, 1);
    }

    #[test]
    fn registry_dataset_parse() {
        let json = r#"{
            "name": "test-ds",
            "version": "1.0",
            "description": "A test dataset",
            "tasks": []
        }"#;
        let ds: RegistryDataset = serde_json::from_str(json).unwrap();
        assert_eq!(ds.name, "test-ds");
        assert_eq!(ds.version, "1.0");
        assert!(ds.tasks.is_empty());
    }

    #[test]
    fn registry_task_parse() {
        let json = r#"{
            "name": "task-1",
            "git_url": "https://github.com/example/repo.git",
            "git_commit_id": "abc123",
            "path": "datasets/task-1"
        }"#;
        let task: RegistryTask = serde_json::from_str(json).unwrap();
        assert_eq!(task.name, "task-1");
        assert_eq!(task.git_commit_id, "abc123");
    }

    #[tokio::test]
    async fn resolve_direct_path() {
        let dir = tempfile::tempdir().unwrap();
        let downloader =
            RegistryDownloader::with_cache_dir(tempfile::tempdir().unwrap().path().to_path_buf());

        let result = downloader.resolve(dir.path().to_str().unwrap()).await;
        assert!(result.is_ok());
    }
}
