use super::types::{ArtifactSchema, FailureStrategy};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ManifestMetadata {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StepDefinition {
    pub id: String,
    pub implementation: String,
    #[serde(default)]
    pub params: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PhaseDefinition {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    pub failure_strategy: FailureStrategy,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub parallel: Option<bool>,
    #[serde(default)]
    pub hard_deps: Option<Vec<String>>,
    #[serde(default)]
    pub soft_deps: Option<Vec<String>>,
    #[serde(default)]
    pub steps: Option<Vec<StepDefinition>>,
    #[serde(default)]
    pub artifacts_produced: Option<Vec<ArtifactSchema>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Manifest {
    pub version: String,
    pub metadata: ManifestMetadata,
    pub phases: Vec<PhaseDefinition>,
}

impl Manifest {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).context("Failed to parse manifest from YAML")
    }

    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to parse manifest from JSON")
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != "1.0" {
            anyhow::bail!(
                "Unsupported manifest version: expected '1.0', got '{}'",
                self.version
            );
        }
        if self.phases.is_empty() {
            anyhow::bail!("Manifest must contain at least one phase");
        }
        for phase in &self.phases {
            if phase.id.is_empty() {
                anyhow::bail!("Phase id must not be empty");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_manifest_yaml() {
        let yaml = r#"
version: "1.0"
metadata:
  name: "test_pipeline"
phases:
  - id: "phase_1"
    schedule: "30 5 * * *"
    failure_strategy:
      mode: "hard_block"
      retry:
        max_attempts: 3
        backoff_secs: 60
"#;
        let manifest = Manifest::from_yaml(yaml).expect("YAML parse should succeed");
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.metadata.name, "test_pipeline");
        assert_eq!(manifest.phases.len(), 1);
        assert_eq!(manifest.phases[0].id, "phase_1");
        assert_eq!(manifest.phases[0].schedule.as_deref(), Some("30 5 * * *"));
    }

    #[test]
    fn test_parse_manifest_with_deps() {
        let yaml = r#"
version: "1.0"
metadata:
  name: "dep_pipeline"
  description: "Pipeline with deps"
phases:
  - id: "phase_a"
    failure_strategy:
      mode: "hard_block"
      retry:
        max_attempts: 3
        backoff_secs: 60
    hard_deps:
      - "phase_0"
    soft_deps:
      - "phase_optional"
"#;
        let manifest = Manifest::from_yaml(yaml).expect("YAML parse should succeed");
        let phase = &manifest.phases[0];
        assert_eq!(
            phase.hard_deps.as_deref(),
            Some(["phase_0".to_string()].as_slice())
        );
        assert_eq!(
            phase.soft_deps.as_deref(),
            Some(["phase_optional".to_string()].as_slice())
        );
    }

    #[test]
    fn test_parse_manifest_json() {
        let json = r#"{
  "version": "1.0",
  "metadata": {
    "name": "json_pipeline",
    "description": "A JSON manifest"
  },
  "phases": [
    {
      "id": "phase_1",
      "failure_strategy": {
        "mode": "hard_block",
        "retry": {
          "max_attempts": 3,
          "backoff_secs": 60
        }
      }
    }
  ]
}"#;
        let manifest = Manifest::from_json(json).expect("JSON parse should succeed");
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.metadata.name, "json_pipeline");
        assert_eq!(manifest.phases.len(), 1);
        assert_eq!(manifest.phases[0].id, "phase_1");
    }

    #[test]
    fn test_validate_valid_manifest() {
        let json = r#"{
  "version": "1.0",
  "metadata": { "name": "valid_pipeline" },
  "phases": [
    {
      "id": "phase_1",
      "failure_strategy": {
        "mode": "hard_block",
        "retry": { "max_attempts": 3, "backoff_secs": 60 }
      }
    }
  ]
}"#;
        let manifest = Manifest::from_yaml(json).expect("Parse should succeed");
        manifest.validate().expect("Validation should succeed");
    }

    #[test]
    fn test_validate_wrong_version() {
        let json = r#"{
  "version": "2.0",
  "metadata": { "name": "wrong_version" },
  "phases": [
    {
      "id": "phase_1",
      "failure_strategy": {
        "mode": "hard_block",
        "retry": { "max_attempts": 3, "backoff_secs": 60 }
      }
    }
  ]
}"#;
        let manifest = Manifest::from_yaml(json).expect("Parse should succeed");
        let err = manifest.validate().unwrap_err();
        assert!(
            err.to_string().contains("Unsupported manifest version"),
            "Expected version error, got: {err}"
        );
    }

    #[test]
    fn test_validate_empty_phases() {
        let json = r#"{
  "version": "1.0",
  "metadata": { "name": "no_phases" },
  "phases": []
}"#;
        let manifest = Manifest::from_yaml(json).expect("Parse should succeed");
        let err = manifest.validate().unwrap_err();
        assert!(
            err.to_string().contains("at least one phase"),
            "Expected empty phases error, got: {err}"
        );
    }

    #[test]
    fn test_validate_empty_phase_id() {
        let json = r#"{
  "version": "1.0",
  "metadata": { "name": "empty_id" },
  "phases": [
    {
      "id": "",
      "failure_strategy": {
        "mode": "hard_block",
        "retry": { "max_attempts": 3, "backoff_secs": 60 }
      }
    }
  ]
}"#;
        let manifest = Manifest::from_yaml(json).expect("Parse should succeed");
        let err = manifest.validate().unwrap_err();
        assert!(
            err.to_string().contains("Phase id must not be empty"),
            "Expected empty phase id error, got: {err}"
        );
    }

    #[test]
    fn test_parse_all_failure_strategies() {
        let strategies: &[(&str, &str)] = &[
            ("hard_block", "hard_block"),
            ("soft_degrade", "soft_degrade"),
            ("checkpoint_veto", "checkpoint_veto"),
            ("skip_on_fail", "skip_on_fail"),
        ];

        for (idx, (name, mode)) in strategies.iter().enumerate() {
            let json = format!(
                r#"{{
  "version": "1.0",
  "metadata": {{ "name": "fs_test_{name}" }},
  "phases": [
    {{
      "id": "phase_{idx}",
      "failure_strategy": {{
        "mode": "{mode}",
        "retry": {{ "max_attempts": 3, "backoff_secs": 60 }}
      }}
    }}
  ]
}}"#
            );
            let manifest = Manifest::from_yaml(&json)
                .unwrap_or_else(|e| panic!("Failed to parse manifest for strategy '{name}': {e}"));
            assert_eq!(
                manifest.phases[0].id,
                format!("phase_{idx}"),
                "Phase id mismatch for strategy '{name}'"
            );
        }
    }

    #[test]
    fn test_parse_manifest_with_steps() {
        let json = r#"{
  "version": "1.0",
  "metadata": { "name": "steps_pipeline" },
  "phases": [
    {
      "id": "phase_1",
      "failure_strategy": {
        "mode": "hard_block",
        "retry": { "max_attempts": 3, "backoff_secs": 60 }
      },
      "steps": [
        { "id": "step_1", "implementation": "run_analysis" },
        {
          "id": "step_2",
          "implementation": "generate_report",
          "params": {
            "format": "pdf",
            "verbose": true
          }
        }
      ]
    }
  ]
}"#;
        let manifest = Manifest::from_yaml(json).expect("Parse should succeed");
        let steps = manifest.phases[0]
            .steps
            .as_ref()
            .expect("Steps should be present");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id, "step_1");
        assert_eq!(steps[0].implementation, "run_analysis");
        assert_eq!(steps[1].id, "step_2");
        assert_eq!(steps[1].implementation, "generate_report");
        assert!(steps[1].params.is_some());
    }

    #[test]
    fn test_parse_manifest_with_artifacts() {
        let json = r#"{
  "version": "1.0",
  "metadata": { "name": "artifacts_pipeline" },
  "phases": [
    {
      "id": "phase_1",
      "failure_strategy": {
        "mode": "hard_block",
        "retry": { "max_attempts": 3, "backoff_secs": 60 }
      },
      "artifacts_produced": [
        {
          "type_tag": "report",
          "format": "json",
          "description": "Analysis report artifact",
          "retention_days": 30
        },
        {
          "type_tag": "metrics",
          "format": "csv",
          "description": "Performance metrics",
          "retention_days": 90,
          "metadata_schema": {
            "source": "string",
            "version": "string"
          }
        }
      ]
    }
  ]
}"#;
        let manifest = Manifest::from_yaml(json).expect("Parse should succeed");
        let artifacts = manifest.phases[0]
            .artifacts_produced
            .as_ref()
            .expect("Artifacts should be present");
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].type_tag, "report");
        assert_eq!(artifacts[0].format, "json");
        assert_eq!(artifacts[0].retention_days, 30);
        assert_eq!(artifacts[1].type_tag, "metrics");
        assert_eq!(artifacts[1].format, "csv");
        assert_eq!(artifacts[1].retention_days, 90);
        let meta = artifacts[1]
            .metadata_schema
            .as_ref()
            .expect("metadata_schema should be present");
        assert_eq!(meta.get("source").map(String::as_str), Some("string"));
    }
}
