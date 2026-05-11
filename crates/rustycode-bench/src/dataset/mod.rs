//! Dataset discovery and loading.
//!
//! Provides utilities for finding benchmark task datasets on the local
//! filesystem, including Harbor's cache directory.

pub mod composite;

use std::path::{Path, PathBuf};

/// Default Harbor cache directory.
const HARBOR_CACHE_DIR: &str = ".cache/harbor/tasks";

/// Registry for discovering benchmark datasets.
///
/// A dataset is a directory containing task subdirectories, each with
/// a `task.toml`, `instruction.md`, and `environment/Dockerfile`.
pub struct DatasetRegistry {
    /// Search paths for datasets.
    search_paths: Vec<PathBuf>,
}

impl DatasetRegistry {
    /// Create a new registry with default search paths.
    ///
    /// Searches in:
    /// - `~/.cache/harbor/tasks/` (Harbor cache)
    /// - Current directory
    pub fn new() -> Self {
        let mut search_paths = Vec::new();

        // Built-in tasks (crates/rustycode-bench/tasks/)
        let builtin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tasks");
        if builtin.exists() {
            search_paths.push(builtin);
        }

        // Harbor cache directory
        if let Ok(home) = std::env::var("HOME") {
            let harbor_cache = PathBuf::from(home).join(HARBOR_CACHE_DIR);
            if harbor_cache.exists() {
                search_paths.push(harbor_cache);
            }
        }

        Self { search_paths }
    }

    /// Create a registry with custom search paths.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_paths(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }

    /// List all available datasets across search paths.
    ///
    /// A dataset is a directory that contains task subdirectories
    /// (directories with `task.toml` files).
    pub fn list_datasets(&self) -> Vec<DatasetInfo> {
        let mut datasets = Vec::new();

        for search_path in &self.search_paths {
            if !search_path.exists() {
                continue;
            }

            let Ok(entries) = std::fs::read_dir(search_path) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                // Check if this contains task directories
                let task_count = count_tasks(&path);
                if task_count > 0 {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    datasets.push(DatasetInfo {
                        name,
                        path,
                        task_count,
                    });
                }
            }
        }

        datasets.sort_by(|a, b| a.name.cmp(&b.name));
        datasets
    }

    /// Find a dataset by name across all search paths.
    pub fn find_dataset(&self, name: &str) -> Option<PathBuf> {
        for search_path in &self.search_paths {
            let candidate = search_path.join(name);
            if candidate.exists() && count_tasks(&candidate) > 0 {
                return Some(candidate);
            }
        }
        None
    }

    /// Resolve a dataset reference to a path.
    ///
    /// Accepts:
    /// - A direct path (if it exists)
    /// - A dataset name (searched in registry paths)
    /// - `name@version` format (searched by name prefix)
    pub fn resolve(&self, reference: &str) -> anyhow::Result<PathBuf> {
        // Try as direct path first
        let direct = PathBuf::from(reference);
        if direct.exists() {
            return Ok(direct);
        }

        // Try as dataset name
        if let Some(path) = self.find_dataset(reference) {
            return Ok(path);
        }

        // Try name@version format — look for exact match in search paths
        let base_name = reference.split('@').next().unwrap_or(reference);
        if let Some(path) = self.find_dataset(base_name) {
            return Ok(path);
        }

        // Try exact match on the full reference (e.g. "terminal-bench@2.0")
        if let Some(path) = self.find_dataset(reference) {
            return Ok(path);
        }

        anyhow::bail!("Dataset not found: {reference}")
    }
}

impl Default for DatasetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a discovered dataset.
#[derive(Debug, Clone)]
pub struct DatasetInfo {
    /// Dataset name (directory name).
    pub name: String,
    /// Path to the dataset directory.
    pub path: PathBuf,
    /// Number of tasks in the dataset.
    pub task_count: usize,
}

/// Count the number of task directories in a given directory.
fn count_tasks(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    entries
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("task.toml").exists())
        .count()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn dataset_registry_default() {
        let registry = DatasetRegistry::default();
        // Default registry may or may not have search paths depending on HOME env
        let _ = registry.list_datasets();
    }

    #[test]
    fn dataset_registry_with_custom_paths() {
        let dir = tempfile::tempdir().unwrap();
        let registry = DatasetRegistry::with_paths(vec![dir.path().to_path_buf()]);
        assert!(registry.list_datasets().is_empty());
    }

    #[test]
    fn dataset_registry_list_datasets_counts_tasks() {
        let dir = tempfile::tempdir().unwrap();

        // Create a valid task inside a dataset directory
        let dataset = dir.path().join("my-dataset");
        let task_dir = dataset.join("task-alpha");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(task_dir.join("task.toml"), "").unwrap();

        let registry = DatasetRegistry::with_paths(vec![dir.path().to_path_buf()]);
        let datasets = registry.list_datasets();
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].name, "my-dataset");
        assert_eq!(datasets[0].task_count, 1);
    }

    #[test]
    fn dataset_registry_list_datasets_ignores_empty_dirs() {
        let dir = tempfile::tempdir().unwrap();

        // Empty subdirectory (no task.toml anywhere)
        let empty_dataset = dir.path().join("empty");
        std::fs::create_dir_all(&empty_dataset).unwrap();

        let registry = DatasetRegistry::with_paths(vec![dir.path().to_path_buf()]);
        assert!(registry.list_datasets().is_empty());
    }

    #[test]
    fn dataset_registry_find_dataset_found() {
        let dir = tempfile::tempdir().unwrap();
        let dataset = dir.path().join("target-dataset");
        let task_dir = dataset.join("task-one");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(task_dir.join("task.toml"), "").unwrap();

        let registry = DatasetRegistry::with_paths(vec![dir.path().to_path_buf()]);
        let found = registry.find_dataset("target-dataset");
        assert!(found.is_some());
        assert_eq!(found.unwrap().file_name().unwrap(), "target-dataset");
    }

    #[test]
    fn dataset_registry_find_dataset_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let registry = DatasetRegistry::with_paths(vec![dir.path().to_path_buf()]);
        assert!(registry.find_dataset("nonexistent").is_none());
    }

    #[test]
    fn dataset_registry_resolve_direct_path() {
        let dir = tempfile::tempdir().unwrap();
        let registry = DatasetRegistry::with_paths(vec![]);

        // resolve should work with a direct path that exists
        let result = registry.resolve(dir.path().to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn dataset_registry_resolve_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let dataset = dir.path().join("my-set");
        let task_dir = dataset.join("task-x");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(task_dir.join("task.toml"), "").unwrap();

        let registry = DatasetRegistry::with_paths(vec![dir.path().to_path_buf()]);
        let result = registry.resolve("my-set");
        assert!(result.is_ok());
        assert!(result.unwrap().join("task-x").exists());
    }

    #[test]
    fn count_tasks_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(count_tasks(dir.path()), 0);
    }

    #[test]
    fn count_tasks_with_tasks() {
        let dir = tempfile::tempdir().unwrap();
        for name in &["a", "b", "c"] {
            let task = dir.path().join(name);
            std::fs::create_dir_all(&task).unwrap();
            std::fs::write(task.join("task.toml"), "").unwrap();
        }
        assert_eq!(count_tasks(dir.path()), 3);
    }

    #[test]
    fn count_tasks_skips_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "hello").unwrap();
        assert_eq!(count_tasks(dir.path()), 0);
    }

    #[test]
    fn dataset_info_debug() {
        let info = DatasetInfo {
            name: "test".to_string(),
            path: PathBuf::from("/tmp/test"),
            task_count: 5,
        };
        let debug = format!("{info:?}");
        assert!(debug.contains("test"));
        assert!(debug.contains('5'));
    }
}
