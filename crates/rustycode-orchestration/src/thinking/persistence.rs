//! Session persistence layer for saving and loading reasoning graphs

use crate::thinking::core::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Serializable representation of a reasoning graph for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedGraph {
    /// All thoughts in the graph
    pub thoughts: Vec<SerializedThought>,
    /// All edges between thoughts
    pub edges: Vec<SerializedEdge>,
    /// Root thought IDs
    pub root_ids: Vec<String>,
    /// Metadata about the session
    pub metadata: GraphMetadata,
}

/// Serialized thought with all necessary data for reconstruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedThought {
    /// Unique ID (as string for JSON compatibility)
    pub id: String,
    /// Kind of thought
    pub kind: String,
    /// Actual content
    pub content: String,
    /// Confidence score
    pub confidence: f64,
    /// Evidence/reasoning
    pub evidence: Vec<String>,
    /// Creation timestamp
    pub created_at: i64,
}

/// Serialized edge between thoughts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEdge {
    /// Source thought ID
    pub from: String,
    /// Target thought ID
    pub to: String,
    /// Edge type/kind
    pub kind: String,
}

/// Metadata about a reasoning session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetadata {
    /// Session title/problem description
    pub problem: String,
    /// When the session was created
    pub created_at: i64,
    /// When the session was last modified
    pub modified_at: i64,
    /// Session ID
    pub session_id: String,
    /// Strategy used
    pub strategy: Option<String>,
    /// Number of iterations completed
    pub iterations: usize,
    /// Custom metadata key-value pairs
    pub custom: std::collections::HashMap<String, String>,
}

/// Manager for persisting and loading reasoning sessions
pub struct SessionManager {
    base_path: PathBuf,
}

impl SessionManager {
    /// Create a new session manager with a base directory
    ///
    pub fn new(base_path: impl AsRef<Path>) -> Result<Self> {
        let path = base_path.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;
        Ok(Self { base_path: path })
    }

    /// Sanitize a session ID to prevent path traversal attacks.
    /// Rejects IDs containing path separators or parent directory references.
    fn sanitize_session_id(session_id: &str) -> Result<String> {
        if session_id.is_empty()
            || session_id.contains('/')
            || session_id.contains('\\')
            || session_id == "."
            || session_id == ".."
            || session_id.starts_with('.')
        {
            return Err(crate::thinking::core::error::Error::InvalidOperation(
                format!("Invalid session ID: {session_id:?}"),
            ));
        }
        Ok(session_id.to_string())
    }

    /// Save a graph to a JSON file
    ///
    pub fn save_json(&self, graph: &SerializedGraph, session_id: &str) -> Result<PathBuf> {
        let safe_id = Self::sanitize_session_id(session_id)?;
        let file_path = self.base_path.join(format!("{safe_id}.json"));
        let json = serde_json::to_string_pretty(graph)?;
        fs::write(&file_path, &json).map_err(|e| {
            crate::thinking::core::error::Error::SerializationError(format!(
                "Failed to write graph to {}: {e}",
                file_path.display()
            ))
        })?;
        tracing::info!("Saved graph to {}", file_path.display());
        Ok(file_path)
    }

    /// Load a graph from a JSON file
    ///
    pub fn load_json(&self, session_id: &str) -> Result<SerializedGraph> {
        let safe_id = Self::sanitize_session_id(session_id)?;
        let file_path = self.base_path.join(format!("{safe_id}.json"));
        let json = fs::read_to_string(&file_path).map_err(|e| {
            crate::thinking::core::error::Error::SerializationError(format!(
                "Failed to read graph from {}: {e}",
                file_path.display()
            ))
        })?;
        let graph = serde_json::from_str(&json)?;
        tracing::info!("Loaded graph from {}", file_path.display());
        Ok(graph)
    }

    /// Save a graph to a binary file (bincode format)
    ///
    pub fn save_bincode(&self, graph: &SerializedGraph, session_id: &str) -> Result<PathBuf> {
        let safe_id = Self::sanitize_session_id(session_id)?;
        let file_path = self.base_path.join(format!("{safe_id}.bin"));
        let bytes = bincode::serialize(graph)?;
        fs::write(&file_path, &bytes).map_err(|e| {
            crate::thinking::core::error::Error::SerializationError(format!(
                "Failed to write bincode graph to {}: {e}",
                file_path.display()
            ))
        })?;
        tracing::info!("Saved graph (bincode) to {}", file_path.display());
        Ok(file_path)
    }

    /// Load a graph from a binary file
    ///
    pub fn load_bincode(&self, session_id: &str) -> Result<SerializedGraph> {
        let safe_id = Self::sanitize_session_id(session_id)?;
        let file_path = self.base_path.join(format!("{safe_id}.bin"));
        let bytes = fs::read(&file_path)?;
        let graph: SerializedGraph = bincode::deserialize(&bytes)?;
        tracing::info!("Loaded graph (bincode) from {}", file_path.display());
        Ok(graph)
    }

    /// List all saved sessions
    ///
    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(stem) = path.file_stem() {
                    if let Some(name) = stem.to_str() {
                        sessions.push(name.to_string());
                    }
                }
            }
        }
        sessions.sort();
        Ok(sessions)
    }

    /// Delete a saved session
    ///
    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        let safe_id = Self::sanitize_session_id(session_id)?;
        // Try deleting both JSON and binary formats
        let json_path = self.base_path.join(format!("{safe_id}.json"));
        let bin_path = self.base_path.join(format!("{safe_id}.bin"));

        if json_path.exists() {
            fs::remove_file(&json_path)?;
        }
        if bin_path.exists() {
            fs::remove_file(&bin_path)?;
        }

        tracing::info!("Deleted session: {}", session_id);
        Ok(())
    }

    /// Check if a session exists
    #[must_use]
    pub fn session_exists(&self, session_id: &str) -> bool {
        let json_path = self.base_path.join(format!("{session_id}.json"));
        let bin_path = self.base_path.join(format!("{session_id}.bin"));
        json_path.exists() || bin_path.exists()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_session_manager_creation() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        assert_eq!(manager.list_sessions().unwrap().len(), 0);
    }

    #[test]
    fn test_save_load_json() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        let graph = SerializedGraph {
            thoughts: vec![SerializedThought {
                id: "test-1".to_string(),
                kind: "Initial".to_string(),
                content: "Test thought".to_string(),
                confidence: 0.8,
                evidence: vec!["because".to_string()],
                created_at: 1000,
            }],
            edges: vec![],
            root_ids: vec!["test-1".to_string()],
            metadata: GraphMetadata {
                problem: "Test problem".to_string(),
                created_at: 1000,
                modified_at: 1000,
                session_id: "test-session".to_string(),
                strategy: Some("Sequential".to_string()),
                iterations: 1,
                custom: HashMap::new(),
            },
        };

        let path = manager.save_json(&graph, "test-session").unwrap();
        assert!(path.exists());

        let loaded = manager.load_json("test-session").unwrap();
        assert_eq!(loaded.thoughts.len(), 1);
        assert_eq!(loaded.thoughts[0].content, "Test thought");
        assert_eq!(loaded.metadata.problem, "Test problem");
    }

    #[test]
    fn test_save_load_bincode() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        let graph = SerializedGraph {
            thoughts: vec![SerializedThought {
                id: "test-1".to_string(),
                kind: "Analysis".to_string(),
                content: "Bincode test".to_string(),
                confidence: 0.7,
                evidence: vec![],
                created_at: 2000,
            }],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "Bincode test".to_string(),
                created_at: 2000,
                modified_at: 2000,
                session_id: "bincode-test".to_string(),
                strategy: None,
                iterations: 0,
                custom: HashMap::new(),
            },
        };

        let path = manager.save_bincode(&graph, "bincode-test").unwrap();
        assert!(path.exists());

        let loaded = manager.load_bincode("bincode-test").unwrap();
        assert_eq!(loaded.thoughts.len(), 1);
        assert_eq!(loaded.thoughts[0].kind, "Analysis");
    }

    #[test]
    fn test_list_sessions() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        let graph = SerializedGraph {
            thoughts: vec![],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "Test".to_string(),
                created_at: 3000,
                modified_at: 3000,
                session_id: "session-1".to_string(),
                strategy: None,
                iterations: 0,
                custom: HashMap::new(),
            },
        };

        manager.save_json(&graph, "session-1").unwrap();
        manager.save_json(&graph, "session-2").unwrap();

        let sessions = manager.list_sessions().unwrap();
        assert!(sessions.iter().any(|s| s.contains("session-1")));
        assert!(sessions.iter().any(|s| s.contains("session-2")));
    }

    #[test]
    fn test_delete_session() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        let graph = SerializedGraph {
            thoughts: vec![],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "Delete test".to_string(),
                created_at: 4000,
                modified_at: 4000,
                session_id: "to-delete".to_string(),
                strategy: None,
                iterations: 0,
                custom: HashMap::new(),
            },
        };

        manager.save_json(&graph, "to-delete").unwrap();
        assert!(manager.session_exists("to-delete"));

        manager.delete_session("to-delete").unwrap();
        assert!(!manager.session_exists("to-delete"));
    }

    #[test]
    fn test_session_exists() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        assert!(!manager.session_exists("nonexistent"));

        let graph = SerializedGraph {
            thoughts: vec![],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "Exists test".to_string(),
                created_at: 5000,
                modified_at: 5000,
                session_id: "exists".to_string(),
                strategy: None,
                iterations: 0,
                custom: HashMap::new(),
            },
        };

        manager.save_json(&graph, "exists").unwrap();
        assert!(manager.session_exists("exists"));
    }

    #[test]
    fn test_custom_metadata() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        let mut custom = HashMap::new();
        custom.insert("user".to_string(), "alice".to_string());
        custom.insert("branch".to_string(), "main".to_string());

        let graph = SerializedGraph {
            thoughts: vec![],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "Custom metadata test".to_string(),
                created_at: 6000,
                modified_at: 6000,
                session_id: "custom-meta".to_string(),
                strategy: Some("Dialectic".to_string()),
                iterations: 3,
                custom,
            },
        };

        manager.save_json(&graph, "custom-meta").unwrap();
        let loaded = manager.load_json("custom-meta").unwrap();

        assert_eq!(
            loaded.metadata.custom.get("user").map(String::as_str),
            Some("alice")
        );
        assert_eq!(loaded.metadata.iterations, 3);
        assert_eq!(loaded.metadata.strategy, Some("Dialectic".to_string()));
    }

    #[test]
    fn test_serialized_graph_with_edges() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        let graph = SerializedGraph {
            thoughts: vec![
                SerializedThought {
                    id: "t1".into(),
                    kind: "Initial".into(),
                    content: "Start".into(),
                    confidence: 0.9,
                    evidence: vec![],
                    created_at: 100,
                },
                SerializedThought {
                    id: "t2".into(),
                    kind: "Analysis".into(),
                    content: "Analyzed".into(),
                    confidence: 0.8,
                    evidence: vec!["evidence1".into()],
                    created_at: 200,
                },
            ],
            edges: vec![SerializedEdge {
                from: "t1".into(),
                to: "t2".into(),
                kind: "DerivesFrom".into(),
            }],
            root_ids: vec!["t1".into()],
            metadata: GraphMetadata {
                problem: "Edge test".into(),
                created_at: 100,
                modified_at: 200,
                session_id: "edge-test".into(),
                strategy: None,
                iterations: 1,
                custom: HashMap::new(),
            },
        };

        manager.save_json(&graph, "edge-test").unwrap();
        let loaded = manager.load_json("edge-test").unwrap();
        assert_eq!(loaded.edges.len(), 1);
        assert_eq!(loaded.edges[0].kind, "DerivesFrom");
        assert_eq!(loaded.thoughts[1].evidence.len(), 1);
    }

    #[test]
    fn test_load_nonexistent_session_errors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        assert!(manager.load_json("nonexistent").is_err());
        assert!(manager.load_bincode("nonexistent").is_err());
    }

    #[test]
    fn test_delete_nonexistent_session_ok() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        // Deleting a nonexistent session should not error
        assert!(manager.delete_session("nonexistent").is_ok());
    }

    #[test]
    fn test_session_exists_bincode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();

        let graph = SerializedGraph {
            thoughts: vec![],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "Bincode exists".into(),
                created_at: 7000,
                modified_at: 7000,
                session_id: "bin-exists".into(),
                strategy: None,
                iterations: 0,
                custom: HashMap::new(),
            },
        };

        manager.save_bincode(&graph, "bin-exists").unwrap();
        assert!(manager.session_exists("bin-exists"));

        manager.delete_session("bin-exists").unwrap();
        assert!(!manager.session_exists("bin-exists"));
    }

    #[test]
    fn test_serialized_thought_debug() {
        let thought = SerializedThought {
            id: "t1".into(),
            kind: "Analysis".into(),
            content: "Test".into(),
            confidence: 0.5,
            evidence: vec![],
            created_at: 0,
        };
        let debug = format!("{thought:?}");
        assert!(debug.contains("t1"));
    }

    #[test]
    fn test_graph_metadata_serialization() {
        let meta = GraphMetadata {
            problem: "test".into(),
            created_at: 1000,
            modified_at: 2000,
            session_id: "s1".into(),
            strategy: Some("Parallel".into()),
            iterations: 5,
            custom: HashMap::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: GraphMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.problem, "test");
        assert_eq!(back.iterations, 5);
    }

    // ─── Path Traversal Prevention ────────────────────────────────────────

    fn make_empty_graph(session_id: &str) -> SerializedGraph {
        SerializedGraph {
            thoughts: vec![],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "traversal test".into(),
                created_at: 0,
                modified_at: 0,
                session_id: session_id.into(),
                strategy: None,
                iterations: 0,
                custom: HashMap::new(),
            },
        }
    }

    #[test]
    fn test_sanitize_rejects_traversal_dotdot() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        let graph = make_empty_graph("evil");
        assert!(manager.save_json(&graph, "../etc/passwd").is_err());
    }

    #[test]
    fn test_sanitize_rejects_slash() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        assert!(manager.load_json("foo/bar").is_err());
    }

    #[test]
    fn test_sanitize_rejects_backslash() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        let graph = make_empty_graph("bs");
        assert!(manager.save_bincode(&graph, "foo\\bar").is_err());
    }

    #[test]
    fn test_sanitize_rejects_dot() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        assert!(manager.delete_session(".").is_err());
    }

    #[test]
    fn test_sanitize_rejects_dotdot() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        assert!(manager.load_bincode("..").is_err());
    }

    #[test]
    fn test_sanitize_rejects_dot_prefix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        assert!(manager.load_json(".hidden").is_err());
    }

    #[test]
    fn test_sanitize_rejects_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        assert!(manager.delete_session("").is_err());
    }

    #[test]
    fn test_sanitize_allows_hyphenated() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        let graph = make_empty_graph("valid-session-1");
        assert!(manager.save_json(&graph, "valid-session-1").is_ok());
    }

    #[test]
    fn test_sanitize_allows_underscore() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path()).unwrap();
        let graph = make_empty_graph("valid_session_2");
        assert!(manager.save_json(&graph, "valid_session_2").is_ok());
    }
}
