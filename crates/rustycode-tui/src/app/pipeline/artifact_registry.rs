use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use tokio::sync::RwLock;

use super::types::{Artifact, ArtifactQuery};

/// Pluggable storage backend for artifact persistence.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    async fn store(&self, artifact: &Artifact) -> Result<()>;
    async fn retrieve(&self, artifact_id: &str) -> Result<Option<Artifact>>;
    async fn list(&self, type_tag: &str) -> Result<Vec<Artifact>>;
    async fn delete(&self, artifact_id: &str) -> Result<()>;
}

/// Stub backend — all operations are no-ops. Suitable as a default when no
/// external persistence is needed.
pub struct InMemoryStore;

#[async_trait]
impl ArtifactStore for InMemoryStore {
    async fn store(&self, _artifact: &Artifact) -> Result<()> {
        Ok(())
    }

    async fn retrieve(&self, _artifact_id: &str) -> Result<Option<Artifact>> {
        Ok(None)
    }

    async fn list(&self, _type_tag: &str) -> Result<Vec<Artifact>> {
        Ok(vec![])
    }

    async fn delete(&self, _artifact_id: &str) -> Result<()> {
        Ok(())
    }
}

/// In-memory artifact registry backed by a pluggable [`ArtifactStore`].
///
/// Maintains a `HashMap<String, Artifact>` for O(1) ID lookups and a
/// `type_tag → [id]` secondary index for type-scoped queries.
pub struct ArtifactRegistry {
    memory: Arc<RwLock<HashMap<String, Artifact>>>,
    index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    storage: Arc<dyn ArtifactStore>,
}

impl ArtifactRegistry {
    pub fn new() -> Self {
        Self {
            memory: Arc::new(RwLock::new(HashMap::new())),
            index: Arc::new(RwLock::new(HashMap::new())),
            storage: Arc::new(InMemoryStore),
        }
    }

    pub fn with_storage(storage: Arc<dyn ArtifactStore>) -> Self {
        Self {
            memory: Arc::new(RwLock::new(HashMap::new())),
            index: Arc::new(RwLock::new(HashMap::new())),
            storage,
        }
    }

    pub async fn register(&self, artifact: Artifact) -> Result<()> {
        self.storage.store(&artifact).await?;

        let artifact_id = artifact.id.clone();
        let type_tag = artifact.type_tag.clone();

        self.memory
            .write()
            .await
            .insert(artifact_id.clone(), artifact);

        self.index
            .write()
            .await
            .entry(type_tag)
            .or_default()
            .push(artifact_id);

        Ok(())
    }

    pub async fn get(&self, artifact_id: &str) -> Result<Artifact> {
        self.memory
            .read()
            .await
            .get(artifact_id)
            .cloned()
            .ok_or_else(|| anyhow!("artifact not found: {artifact_id}"))
    }

    /// Filter by `type_tag` (required), then optionally by `after_phase`,
    /// `after_time`, and arbitrary metadata key-value pairs.
    pub async fn query(&self, q: &ArtifactQuery) -> Result<Vec<Artifact>> {
        let memory = self.memory.read().await;
        let index = self.index.read().await;

        let candidate_ids = index.get(&q.type_tag).cloned().unwrap_or_default();

        let results: Vec<Artifact> = candidate_ids
            .iter()
            .filter_map(|id| memory.get(id).cloned())
            .filter(|artifact| {
                if let Some(ref phase) = q.after_phase {
                    if artifact.source_phase != *phase {
                        return false;
                    }
                }

                if let Some(after_time) = q.after_time {
                    if artifact.created_at <= after_time {
                        return false;
                    }
                }

                for (key, value) in &q.filters {
                    if artifact.metadata.get(key) != Some(value) {
                        return false;
                    }
                }

                true
            })
            .collect();

        Ok(results)
    }

    /// Evict artifacts past their `retention_days`. Returns count removed.
    pub async fn cleanup(&self) -> Result<usize> {
        let now = Utc::now();

        let expired_ids: Vec<String> = {
            let memory = self.memory.read().await;
            memory
                .iter()
                .filter(|(_, artifact)| {
                    let retention = Duration::days(i64::from(artifact.retention_days));
                    artifact.created_at + retention < now
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        let count = expired_ids.len();

        {
            let mut memory = self.memory.write().await;
            let mut index = self.index.write().await;

            for id in &expired_ids {
                if let Some(artifact) = memory.remove(id) {
                    if let Some(ids) = index.get_mut(&artifact.type_tag) {
                        ids.retain(|entry_id| entry_id != id);
                    }
                }
            }
        }

        for id in &expired_ids {
            let _ = self.storage.delete(id).await;
        }

        Ok(count)
    }

    pub async fn count_by_type(&self, type_tag: &str) -> usize {
        self.index.read().await.get(type_tag).map_or(0, Vec::len)
    }
}

impl Default for ArtifactRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_artifact(id: &str, type_tag: &str, phase: &str) -> Artifact {
        Artifact {
            id: id.to_string(),
            type_tag: type_tag.to_string(),
            source_phase: phase.to_string(),
            created_at: Utc::now(),
            payload: super::super::types::ArtifactPayload::Json(serde_json::json!({})),
            metadata: HashMap::new(),
            retention_days: 30,
        }
    }

    #[tokio::test]
    async fn test_register_and_retrieve() {
        let registry = ArtifactRegistry::new();
        let artifact = make_artifact("a1", "report", "phase1");

        registry
            .register(artifact)
            .await
            .expect("register should succeed");

        let retrieved = registry.get("a1").await.expect("get should succeed");
        assert_eq!(retrieved.id, "a1");
        assert_eq!(retrieved.type_tag, "report");
    }

    #[tokio::test]
    async fn test_query_by_type() {
        let registry = ArtifactRegistry::new();

        registry
            .register(make_artifact("a1", "metric", "phase1"))
            .await
            .expect("register a1");
        registry
            .register(make_artifact("a2", "metric", "phase2"))
            .await
            .expect("register a2");
        registry
            .register(make_artifact("a3", "report", "phase1"))
            .await
            .expect("register a3");

        let results = registry
            .query(&ArtifactQuery::new("metric"))
            .await
            .expect("query should succeed");

        assert_eq!(results.len(), 2);
        let ids: Vec<&str> = results.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"a1"));
        assert!(ids.contains(&"a2"));
    }

    #[tokio::test]
    async fn test_query_filter_by_phase() {
        let registry = ArtifactRegistry::new();

        registry
            .register(make_artifact("a1", "metric", "phase1"))
            .await
            .expect("register a1");
        registry
            .register(make_artifact("a2", "metric", "phase2"))
            .await
            .expect("register a2");

        let results = registry
            .query(&ArtifactQuery::new("metric").after_phase("phase1"))
            .await
            .expect("query should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a1");
    }

    #[tokio::test]
    async fn test_query_filter_by_metadata() {
        let registry = ArtifactRegistry::new();

        let mut a1 = make_artifact("a1", "metric", "phase1");
        a1.metadata.insert("env".to_string(), "prod".to_string());

        let mut a2 = make_artifact("a2", "metric", "phase1");
        a2.metadata.insert("env".to_string(), "staging".to_string());

        registry.register(a1).await.expect("register a1");
        registry.register(a2).await.expect("register a2");

        let results = registry
            .query(&ArtifactQuery::new("metric").filter("env", "prod"))
            .await
            .expect("query should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a1");
    }

    #[tokio::test]
    async fn test_count_by_type() {
        let registry = ArtifactRegistry::new();

        registry
            .register(make_artifact("a1", "metric", "phase1"))
            .await
            .expect("register a1");
        registry
            .register(make_artifact("a2", "metric", "phase2"))
            .await
            .expect("register a2");
        registry
            .register(make_artifact("a3", "report", "phase1"))
            .await
            .expect("register a3");

        assert_eq!(registry.count_by_type("metric").await, 2);
        assert_eq!(registry.count_by_type("report").await, 1);
        assert_eq!(registry.count_by_type("nonexistent").await, 0);
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_error() {
        let registry = ArtifactRegistry::new();
        let result = registry.get("does_not_exist").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("artifact not found"));
    }
}
