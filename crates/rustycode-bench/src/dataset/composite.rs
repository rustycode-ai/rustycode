//! Composite dataset — merge tasks from multiple sources.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::dataset::DatasetRegistry;
use crate::registry::RegistryDownloader;
use crate::task::ResolvedTask;

/// A dataset source — either local or remote.
#[derive(Debug, Clone)]
pub enum DatasetSource {
    Local { path: PathBuf },
    Remote { reference: String },
}

/// A composite dataset that merges tasks from multiple sources.
pub struct CompositeDataset {
    pub sources: Vec<DatasetSource>,
}

impl CompositeDataset {
    /// Create from a list of references (paths or registry refs).
    pub fn from_references(refs: &[String]) -> Self {
        let sources = refs
            .iter()
            .map(|r| {
                let path = PathBuf::from(r);
                if path.exists() {
                    DatasetSource::Local { path }
                } else {
                    DatasetSource::Remote {
                        reference: r.clone(),
                    }
                }
            })
            .collect();
        Self { sources }
    }

    /// Resolve all sources and return deduplicated tasks.
    pub async fn resolve(&self) -> Result<Vec<ResolvedTask>> {
        let mut all_tasks = Vec::new();
        let mut seen_names = HashSet::new();

        for source in &self.sources {
            let path = resolve_source(source).await?;
            let tasks = ResolvedTask::discover(&path)
                .with_context(|| format!("Failed to discover tasks in {}", path.display()))?;

            for task in tasks {
                if seen_names.insert(task.name.clone()) {
                    all_tasks.push(task);
                }
            }
        }

        all_tasks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all_tasks)
    }
}

async fn resolve_source(source: &DatasetSource) -> Result<PathBuf> {
    match source {
        DatasetSource::Local { path } => Ok(path.clone()),
        DatasetSource::Remote { reference } => {
            let registry = DatasetRegistry::new();
            if let Ok(p) = registry.resolve(reference) {
                return Ok(p);
            }

            let downloader = RegistryDownloader::new();
            downloader
                .resolve(reference)
                .await
                .with_context(|| format!("Failed to resolve dataset '{reference}'"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_references_local_path() {
        let refs = vec![".".to_string()];
        let ds = CompositeDataset::from_references(&refs);
        assert_eq!(ds.sources.len(), 1);
        assert!(matches!(ds.sources[0], DatasetSource::Local { .. }));
    }

    #[test]
    fn from_references_remote() {
        let refs = vec!["terminal-bench@2.0".to_string()];
        let ds = CompositeDataset::from_references(&refs);
        assert_eq!(ds.sources.len(), 1);
        assert!(matches!(ds.sources[0], DatasetSource::Remote { .. }));
    }

    #[test]
    fn from_references_mixed() {
        let refs = vec![".".to_string(), "terminal-bench@2.0".to_string()];
        let ds = CompositeDataset::from_references(&refs);
        assert_eq!(ds.sources.len(), 2);
    }
}
