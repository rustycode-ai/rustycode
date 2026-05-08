//! SWE-bench instance types — represents a single real GitHub issue to fix.

use serde::{Deserialize, Serialize};

/// A single SWE-bench instance describing a real GitHub issue and its fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweBenchInstance {
    /// Unique identifier (e.g. "django__django-12345").
    pub instance_id: String,
    /// Repository in "owner/repo" format.
    pub repo: String,
    /// Base commit hash to checkout before applying the patch.
    pub base_commit: String,
    /// The problem statement (issue body).
    pub problem_statement: String,
    /// Optional hints text.
    #[serde(default)]
    pub hints_text: String,
    /// Version string.
    #[serde(default)]
    pub version: String,
    /// JSON array of test names that should transition from FAIL to PASS.
    #[serde(default)]
    pub fail_to_pass: String,
    /// JSON array of test names that should remain PASS.
    #[serde(default)]
    pub pass_to_pass: String,
    /// The test patch to apply during evaluation.
    #[serde(default)]
    pub test_patch: String,
    /// The gold patch (ground truth — used for evaluation, not by the agent).
    #[serde(default)]
    pub patch: String,
}

/// Load SWE-bench instances from a JSON or JSONL file.
pub fn load_instances(path: &std::path::Path) -> anyhow::Result<Vec<SweBenchInstance>> {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_instance_json() {
        let json = r#"[{
            "instance_id": "test__repo-1",
            "repo": "test/repo",
            "base_commit": "abc123",
            "problem_statement": "Fix the bug"
        }]"#;
        let instances: Vec<SweBenchInstance> = serde_json::from_str(json).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].instance_id, "test__repo-1");
    }

    #[test]
    fn parse_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instances.jsonl");
        let content = r#"{"instance_id":"a","repo":"x/y","base_commit":"c1","problem_statement":"p1"}
{"instance_id":"b","repo":"x/z","base_commit":"c2","problem_statement":"p2"}
"#;
        std::fs::write(&path, content).unwrap();
        let instances = load_instances(&path).unwrap();
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].instance_id, "a");
        assert_eq!(instances[1].instance_id, "b");
    }
}
