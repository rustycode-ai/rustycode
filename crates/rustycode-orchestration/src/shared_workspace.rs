use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub written_by: String,
    pub step_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct SharedWorkspace {
    entries: Arc<Mutex<HashMap<String, WorkspaceEntry>>>,
}

impl SharedWorkspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn write(
        &self,
        key: String,
        value: serde_json::Value,
        written_by: String,
        step_id: Option<String>,
    ) {
        let entry = WorkspaceEntry {
            key: key.clone(),
            value,
            written_by,
            step_id,
            timestamp: chrono::Utc::now(),
        };
        self.entries.lock().await.insert(key, entry);
    }

    pub async fn read(&self, key: &str) -> Option<WorkspaceEntry> {
        self.entries.lock().await.get(key).cloned()
    }

    pub async fn read_value(&self, key: &str) -> Option<serde_json::Value> {
        self.entries.lock().await.get(key).map(|e| e.value.clone())
    }

    pub async fn contains(&self, key: &str) -> bool {
        self.entries.lock().await.contains_key(key)
    }

    pub async fn remove(&self, key: &str) -> Option<WorkspaceEntry> {
        self.entries.lock().await.remove(key)
    }

    pub async fn keys(&self) -> Vec<String> {
        self.entries.lock().await.keys().cloned().collect()
    }

    pub async fn len(&self) -> usize {
        self.entries.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.entries.lock().await.is_empty()
    }

    pub async fn clear(&self) {
        self.entries.lock().await.clear();
    }

    pub async fn snapshot(&self) -> HashMap<String, WorkspaceEntry> {
        self.entries.lock().await.clone()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_and_read() {
        let ws = SharedWorkspace::new();
        ws.write(
            "result".into(),
            serde_json::json!({"status": "ok"}),
            "musician".into(),
            Some("step-1".into()),
        )
        .await;

        let entry = ws.read("result").await.unwrap();
        assert_eq!(entry.written_by, "musician");
        assert_eq!(entry.value["status"], "ok");
    }

    #[tokio::test]
    async fn test_read_value() {
        let ws = SharedWorkspace::new();
        ws.write("data".into(), serde_json::json!(42), "editor".into(), None)
            .await;

        let val = ws.read_value("data").await.unwrap();
        assert_eq!(val, serde_json::json!(42));
    }

    #[tokio::test]
    async fn test_missing_key() {
        let ws = SharedWorkspace::new();
        assert!(ws.read("nonexistent").await.is_none());
        assert!(ws.read_value("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_contains() {
        let ws = SharedWorkspace::new();
        assert!(!ws.contains("key").await);
        ws.write("key".into(), serde_json::json!(true), "test".into(), None)
            .await;
        assert!(ws.contains("key").await);
    }

    #[tokio::test]
    async fn test_remove() {
        let ws = SharedWorkspace::new();
        ws.write("key".into(), serde_json::json!(1), "test".into(), None)
            .await;
        let removed = ws.remove("key").await.unwrap();
        assert_eq!(removed.value, serde_json::json!(1));
        assert!(ws.read("key").await.is_none());
    }

    #[tokio::test]
    async fn test_keys() {
        let ws = SharedWorkspace::new();
        ws.write("a".into(), serde_json::json!(1), "t".into(), None)
            .await;
        ws.write("b".into(), serde_json::json!(2), "t".into(), None)
            .await;
        let mut keys = ws.keys().await;
        keys.sort();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn test_len_and_clear() {
        let ws = SharedWorkspace::new();
        assert!(ws.is_empty().await);
        ws.write("k".into(), serde_json::json!(0), "t".into(), None)
            .await;
        assert_eq!(ws.len().await, 1);
        ws.clear().await;
        assert!(ws.is_empty().await);
    }

    #[tokio::test]
    async fn test_overwrite() {
        let ws = SharedWorkspace::new();
        ws.write("k".into(), serde_json::json!(1), "a".into(), None)
            .await;
        ws.write("k".into(), serde_json::json!(2), "b".into(), None)
            .await;
        let val = ws.read_value("k").await.unwrap();
        assert_eq!(val, serde_json::json!(2));
        let entry = ws.read("k").await.unwrap();
        assert_eq!(entry.written_by, "b");
    }

    #[tokio::test]
    async fn test_snapshot() {
        let ws = SharedWorkspace::new();
        ws.write("x".into(), serde_json::json!(10), "t".into(), None)
            .await;
        let snap = ws.snapshot().await;
        assert_eq!(snap.len(), 1);
        assert_eq!(snap["x"].value, serde_json::json!(10));
    }
}
