//! SWE-bench prediction result and output formatting.

use serde::{Deserialize, Serialize};

/// A single prediction: the agent's patch for an instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweBenchPrediction {
    pub instance_id: String,
    pub model_patch: String,
    /// Number of agent attempts (1 = no retry).
    #[serde(default)]
    pub attempts: u32,
}

/// Load predictions from a JSON or JSONL file.
pub fn load_predictions(path: &std::path::Path) -> anyhow::Result<Vec<SweBenchPrediction>> {
    let content = std::fs::read_to_string(path)?;
    if content.trim_start().starts_with('[') {
        Ok(serde_json::from_str(&content)?)
    } else {
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|e| anyhow::anyhow!(e)))
            .collect()
    }
}

/// Save predictions in the standard SWE-bench format (JSON array or JSONL).
pub fn save_predictions(
    predictions: &[SweBenchPrediction],
    output_path: &std::path::Path,
    format: &str,
    model_name: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let output: Vec<serde_json::Value> = predictions
        .iter()
        .map(|p| {
            serde_json::json!({
                "instance_id": p.instance_id,
                "model_patch": p.model_patch,
                "model_name_or_path": model_name,
                "attempts": p.attempts,
            })
        })
        .collect();

    let content = match format {
        "jsonl" => output
            .iter()
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => serde_json::to_string_pretty(&output)?,
    };

    std::fs::write(output_path, content)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn save_json_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pred.json");
        let preds = vec![SweBenchPrediction {
            instance_id: "test-1".to_string(),
            model_patch: "diff --git a/file.txt".to_string(),
            attempts: 1,
        }];
        save_predictions(&preds, &path, "json", "test-model").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"instance_id\""));
        assert!(content.contains("test-model"));
    }

    #[test]
    fn save_jsonl_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pred.jsonl");
        let preds = vec![
            SweBenchPrediction {
                instance_id: "a".to_string(),
                model_patch: "p1".to_string(),
                attempts: 1,
            },
            SweBenchPrediction {
                instance_id: "b".to_string(),
                model_patch: "p2".to_string(),
                attempts: 2,
            },
        ];
        save_predictions(&preds, &path, "jsonl", "model").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"a\""));
        assert!(lines[1].contains("\"b\""));
    }
}
