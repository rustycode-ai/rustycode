//! Reward file parsing for TB2 verifier output.

use std::collections::HashMap;

/// Parsed reward from verifier output.
#[derive(Debug, Clone)]
pub struct RewardResult {
    /// Named rewards (e.g., {"accuracy": 1.0} or {"default": 0.5}).
    pub rewards: HashMap<String, f64>,
    /// Structured CTRF test report, if available.
    pub ctrf: Option<CtrfReport>,
}

/// CTRF (Common Test Report Format) test report.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CtrfReport {
    pub results: CtrfResults,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CtrfResults {
    pub summary: CtrfSummary,
    #[serde(default)]
    pub tests: Vec<CtrfTest>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CtrfSummary {
    pub tests: usize,
    pub passed: usize,
    pub failed: usize,
    #[serde(default)]
    pub skipped: usize,
    #[serde(default)]
    pub pending: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CtrfTest {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Parse reward.txt content (single float value).
pub fn parse_reward_txt(content: &str) -> Option<f64> {
    content.trim().parse::<f64>().ok()
}

/// Parse reward.json content (named rewards dict).
pub fn parse_reward_json(content: &str) -> Option<HashMap<String, f64>> {
    serde_json::from_str(content).ok()
}

/// Parse CTRF JSON report.
pub fn parse_ctrf_json(content: &str) -> Option<CtrfReport> {
    serde_json::from_str(content).ok()
}

/// Parse all reward files from verifier output directory.
pub fn parse_verifier_output(
    reward_txt: Option<&str>,
    reward_json: Option<&str>,
    ctrf_json: Option<&str>,
) -> RewardResult {
    let rewards = if let Some(txt) = reward_txt {
        if let Some(val) = parse_reward_txt(txt) {
            HashMap::from([("default".to_string(), val)])
        } else {
            HashMap::new()
        }
    } else if let Some(json) = reward_json {
        parse_reward_json(json).unwrap_or_default()
    } else {
        HashMap::new()
    };

    let ctrf = ctrf_json.and_then(parse_ctrf_json);

    RewardResult { rewards, ctrf }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_reward_txt_pass() {
        assert_eq!(parse_reward_txt("1"), Some(1.0));
        assert_eq!(parse_reward_txt("1\n"), Some(1.0));
        assert_eq!(parse_reward_txt("1.0"), Some(1.0));
    }

    #[test]
    fn parse_reward_txt_fail() {
        assert_eq!(parse_reward_txt("0"), Some(0.0));
        assert_eq!(parse_reward_txt("0.0"), Some(0.0));
    }

    #[test]
    fn parse_reward_txt_invalid() {
        assert_eq!(parse_reward_txt("error"), None);
        assert_eq!(parse_reward_txt(""), None);
    }

    #[test]
    fn parse_reward_txt_partial() {
        assert_eq!(parse_reward_txt("0.75"), Some(0.75));
    }

    #[test]
    fn parse_reward_json_single_key() {
        let result = parse_reward_json(r#"{"accuracy": 0.95}"#);
        assert_eq!(
            result,
            Some(HashMap::from([("accuracy".to_string(), 0.95)]))
        );
    }

    #[test]
    fn parse_reward_json_multiple_keys() {
        let result = parse_reward_json(r#"{"accuracy": 0.9, "completeness": 0.8}"#);
        let map = result.unwrap();
        assert_eq!(map.get("accuracy"), Some(&0.9));
        assert_eq!(map.get("completeness"), Some(&0.8));
    }

    #[test]
    fn parse_reward_json_invalid() {
        assert_eq!(parse_reward_json("not json"), None);
    }

    #[test]
    fn parse_ctrf_json_basic() {
        let json = r#"{"results": {"summary": {"tests": 3, "passed": 2, "failed": 1, "skipped": 0, "pending": 0}, "tests": [{"name": "test_a", "status": "passed"}, {"name": "test_b", "status": "failed", "message": "wrong value"}]}}"#;
        let report = parse_ctrf_json(json).unwrap();
        assert_eq!(report.results.summary.tests, 3);
        assert_eq!(report.results.summary.passed, 2);
        assert_eq!(report.results.tests.len(), 2);
        assert_eq!(report.results.tests[1].status, "failed");
    }

    #[test]
    fn parse_ctrf_json_invalid() {
        assert_eq!(parse_ctrf_json("bad"), None);
    }

    #[test]
    fn parse_verifier_output_txt_only() {
        let result = parse_verifier_output(Some("1"), None, None);
        assert_eq!(result.rewards.get("default"), Some(&1.0));
        assert!(result.ctrf.is_none());
    }

    #[test]
    fn parse_verifier_output_all_files() {
        let ctrf = r#"{"results": {"summary": {"tests": 1, "passed": 1, "failed": 0, "skipped": 0, "pending": 0}, "tests": []}}"#;
        let result = parse_verifier_output(Some("1"), None, Some(ctrf));
        assert_eq!(result.rewards.get("default"), Some(&1.0));
        assert!(result.ctrf.is_some());
        assert_eq!(result.ctrf.unwrap().results.summary.passed, 1);
    }

    #[test]
    fn parse_verifier_output_nothing() {
        let result = parse_verifier_output(None, None, None);
        assert!(result.rewards.is_empty());
        assert!(result.ctrf.is_none());
    }
}
