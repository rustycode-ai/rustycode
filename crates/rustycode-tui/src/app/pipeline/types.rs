use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_secs: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
#[non_exhaustive]
pub enum FailureStrategy {
    #[serde(rename = "hard_block")]
    HardBlock { retry: RetryPolicy },
    #[serde(rename = "soft_degrade")]
    SoftDegrade {
        retry: RetryPolicy,
        fallback_artifact: Option<String>,
    },
    #[serde(rename = "checkpoint_veto")]
    CheckpointVeto { retry: RetryPolicy },
    #[serde(rename = "skip_on_fail")]
    SkipOnFail { retry: RetryPolicy },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ArtifactSchema {
    pub type_tag: String,
    pub format: String,
    pub description: String,
    pub retention_days: u32,
    pub metadata_schema: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Artifact {
    pub id: String,
    pub type_tag: String,
    pub source_phase: String,
    pub created_at: DateTime<Utc>,
    pub payload: ArtifactPayload,
    pub metadata: HashMap<String, String>,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format")]
#[non_exhaustive]
pub enum ArtifactPayload {
    #[serde(rename = "json")]
    Json(serde_json::Value),
    #[serde(rename = "csv")]
    Csv(String),
    #[serde(rename = "html")]
    Html(String),
    #[serde(rename = "parquet")]
    Parquet(Vec<u8>),
    #[serde(rename = "raw")]
    Raw(Vec<u8>),
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ArtifactQuery {
    pub type_tag: String,
    pub after_phase: Option<String>,
    pub after_time: Option<DateTime<Utc>>,
    pub filters: HashMap<String, String>,
}

impl ArtifactQuery {
    pub fn new(type_tag: impl Into<String>) -> Self {
        Self {
            type_tag: type_tag.into(),
            after_phase: None,
            after_time: None,
            filters: HashMap::new(),
        }
    }

    pub fn after_phase(mut self, phase: impl Into<String>) -> Self {
        self.after_phase = Some(phase.into());
        self
    }

    pub fn filter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.filters.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PhaseResult {
    Success,
    Degraded { reason: String },
    VetoPending { reason: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PhaseStatus {
    NotStarted,
    Running,
    Completed,
    CompletedDegraded,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.backoff_secs, 60);
    }

    #[test]
    fn test_retry_policy_custom() {
        let policy = RetryPolicy {
            max_attempts: 5,
            backoff_secs: 30,
        };
        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.backoff_secs, 30);
    }

    #[test]
    fn test_artifact_schema_creation() {
        let schema = ArtifactSchema {
            type_tag: "test_type".to_string(),
            format: "json".to_string(),
            description: "Test artifact".to_string(),
            retention_days: 30,
            metadata_schema: None,
        };
        assert_eq!(schema.type_tag, "test_type");
        assert_eq!(schema.format, "json");
        assert_eq!(schema.retention_days, 30);
    }

    #[test]
    fn test_artifact_creation() {
        let artifact = Artifact {
            id: "artifact1".to_string(),
            type_tag: "test_type".to_string(),
            source_phase: "phase1".to_string(),
            created_at: Utc::now(),
            payload: ArtifactPayload::Json(serde_json::json!({"key": "value"})),
            metadata: HashMap::new(),
            retention_days: 30,
        };
        assert_eq!(artifact.id, "artifact1");
        assert_eq!(artifact.type_tag, "test_type");
    }

    #[test]
    fn test_artifact_payload_json() {
        let json_val = serde_json::json!({"key": "value"});
        let payload = ArtifactPayload::Json(json_val.clone());
        match payload {
            ArtifactPayload::Json(v) => assert_eq!(v, json_val),
            _ => panic!("Expected Json variant"),
        }
    }

    #[test]
    fn test_artifact_payload_csv() {
        let csv = "col1,col2\nval1,val2".to_string();
        let payload = ArtifactPayload::Csv(csv.clone());
        match payload {
            ArtifactPayload::Csv(c) => assert_eq!(c, csv),
            _ => panic!("Expected Csv variant"),
        }
    }

    #[test]
    fn test_artifact_payload_html() {
        let html = "<html><body>test</body></html>".to_string();
        let payload = ArtifactPayload::Html(html.clone());
        match payload {
            ArtifactPayload::Html(h) => assert_eq!(h, html),
            _ => panic!("Expected Html variant"),
        }
    }

    #[test]
    fn test_artifact_payload_raw() {
        let raw = vec![1, 2, 3, 4];
        let payload = ArtifactPayload::Raw(raw.clone());
        match payload {
            ArtifactPayload::Raw(r) => assert_eq!(r, raw),
            _ => panic!("Expected Raw variant"),
        }
    }

    #[test]
    fn test_artifact_payload_parquet() {
        let parquet = vec![80, 65, 82, 49]; // "PAR1" header
        let payload = ArtifactPayload::Parquet(parquet.clone());
        match payload {
            ArtifactPayload::Parquet(p) => assert_eq!(p, parquet),
            _ => panic!("Expected Parquet variant"),
        }
    }

    #[test]
    fn test_artifact_query_new() {
        let query = ArtifactQuery::new("test_type");
        assert_eq!(query.type_tag, "test_type");
        assert_eq!(query.after_phase, None);
        assert_eq!(query.after_time, None);
        assert!(query.filters.is_empty());
    }

    #[test]
    fn test_artifact_query_builder() {
        let query = ArtifactQuery::new("test_type")
            .after_phase("phase1")
            .filter("status", "active");
        assert_eq!(query.type_tag, "test_type");
        assert_eq!(query.after_phase, Some("phase1".to_string()));
        assert_eq!(query.filters.get("status"), Some(&"active".to_string()));
    }

    #[test]
    fn test_phase_result_success() {
        let result = PhaseResult::Success;
        match result {
            PhaseResult::Success => (),
            _ => panic!("Expected Success variant"),
        }
    }

    #[test]
    fn test_phase_result_degraded() {
        let result = PhaseResult::Degraded {
            reason: "partial failure".to_string(),
        };
        match result {
            PhaseResult::Degraded { reason } => assert_eq!(reason, "partial failure"),
            _ => panic!("Expected Degraded variant"),
        }
    }

    #[test]
    fn test_phase_result_veto_pending() {
        let result = PhaseResult::VetoPending {
            reason: "awaiting approval".to_string(),
        };
        match result {
            PhaseResult::VetoPending { reason } => assert_eq!(reason, "awaiting approval"),
            _ => panic!("Expected VetoPending variant"),
        }
    }

    #[test]
    fn test_phase_result_skipped() {
        let result = PhaseResult::Skipped {
            reason: "dependency failed".to_string(),
        };
        match result {
            PhaseResult::Skipped { reason } => assert_eq!(reason, "dependency failed"),
            _ => panic!("Expected Skipped variant"),
        }
    }

    #[test]
    fn test_phase_status_values() {
        assert_eq!(PhaseStatus::NotStarted, PhaseStatus::NotStarted);
        assert_ne!(PhaseStatus::NotStarted, PhaseStatus::Running);
        assert_eq!(PhaseStatus::Completed, PhaseStatus::Completed);
        assert_ne!(PhaseStatus::Completed, PhaseStatus::CompletedDegraded);
        assert_ne!(PhaseStatus::Completed, PhaseStatus::Failed);
    }

    #[test]
    fn test_failure_strategy_hard_block() {
        let strategy = FailureStrategy::HardBlock {
            retry: RetryPolicy::default(),
        };
        match strategy {
            FailureStrategy::HardBlock { retry } => assert_eq!(retry.max_attempts, 3),
            _ => panic!("Expected HardBlock variant"),
        }
    }

    #[test]
    fn test_failure_strategy_soft_degrade() {
        let strategy = FailureStrategy::SoftDegrade {
            retry: RetryPolicy::default(),
            fallback_artifact: Some("fallback_id".to_string()),
        };
        match strategy {
            FailureStrategy::SoftDegrade {
                retry,
                fallback_artifact,
            } => {
                assert_eq!(retry.max_attempts, 3);
                assert_eq!(fallback_artifact, Some("fallback_id".to_string()));
            }
            _ => panic!("Expected SoftDegrade variant"),
        }
    }

    #[test]
    fn test_failure_strategy_checkpoint_veto() {
        let strategy = FailureStrategy::CheckpointVeto {
            retry: RetryPolicy::default(),
        };
        match strategy {
            FailureStrategy::CheckpointVeto { retry } => assert_eq!(retry.max_attempts, 3),
            _ => panic!("Expected CheckpointVeto variant"),
        }
    }

    #[test]
    fn test_failure_strategy_skip_on_fail() {
        let strategy = FailureStrategy::SkipOnFail {
            retry: RetryPolicy::default(),
        };
        match strategy {
            FailureStrategy::SkipOnFail { retry } => assert_eq!(retry.max_attempts, 3),
            _ => panic!("Expected SkipOnFail variant"),
        }
    }
}
