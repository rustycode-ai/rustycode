//! LLM-as-Judge: rubric-based quality evaluation for structured outputs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeConfig {
    pub pass_threshold: f64,
    pub required: bool,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            pass_threshold: 0.6,
            required: false,
        }
    }
}

impl JudgeConfig {
    pub const fn new(pass_threshold: f64, required: bool) -> Self {
        Self {
            pass_threshold: pass_threshold.clamp(0.0, 1.0),
            required,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeRubric {
    pub name: String,
    pub criteria: Vec<String>,
}

impl JudgeRubric {
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(name: String, criteria: Vec<String>) -> Self {
        Self { name, criteria }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeVerdict {
    pub correctness: f64,
    pub completeness: f64,
    pub quality: f64,
    #[serde(default)]
    pub feedback: String,
    #[serde(default)]
    pub issues: Vec<String>,
    pub rubric_name: String,
}

impl JudgeVerdict {
    pub const fn from_scores(correctness: f64, completeness: f64, quality: f64) -> Self {
        Self {
            correctness: correctness.clamp(0.0, 1.0),
            completeness: completeness.clamp(0.0, 1.0),
            quality: quality.clamp(0.0, 1.0),
            feedback: String::new(),
            issues: Vec::new(),
            rubric_name: String::new(),
        }
    }

    pub fn passed_with(&self, threshold: f64) -> bool {
        self.weighted_score() >= threshold
    }

    pub fn passed(&self) -> bool {
        self.passed_with(0.6)
    }

    pub fn weighted_score(&self) -> f64 {
        self.quality
            .mul_add(0.3, self.correctness.mul_add(0.4, self.completeness * 0.3))
    }

    pub fn grade(&self) -> JudgeGrade {
        JudgeGrade::from_score(self.weighted_score())
    }

    pub fn parse_from_llm_response(json_str: &str) -> Result<Self, JudgeParseError> {
        let parsed: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| JudgeParseError {
                message: format!("invalid JSON: {e}"),
            })?;

        let correctness = parsed
            .get("correctness")
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| JudgeParseError {
                message: "missing or invalid 'correctness' field".to_string(),
            })?;
        let completeness = parsed
            .get("completeness")
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| JudgeParseError {
                message: "missing or invalid 'completeness' field".to_string(),
            })?;
        let quality = parsed
            .get("quality")
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| JudgeParseError {
                message: "missing or invalid 'quality' field".to_string(),
            })?;

        let feedback = parsed
            .get("feedback")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let issues = parsed
            .get("issues")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            correctness: correctness.clamp(0.0, 1.0),
            completeness: completeness.clamp(0.0, 1.0),
            quality: quality.clamp(0.0, 1.0),
            feedback,
            issues,
            rubric_name: String::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("judge parse error: {message}")]
pub struct JudgeParseError {
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JudgeGrade {
    Excellent,
    Good,
    Fair,
    Poor,
    Critical,
}

impl JudgeGrade {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::Excellent
        } else if score >= 0.65 {
            Self::Good
        } else if score >= 0.5 {
            Self::Fair
        } else if score >= 0.3 {
            Self::Poor
        } else {
            Self::Critical
        }
    }
}

pub fn build_judge_prompt(task_description: &str, output: &str, rubric: &JudgeRubric) -> String {
    let criteria_list = rubric
        .criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "You are a quality judge evaluating AI-generated output.\n\n## Task\n{task_description}\n\n## Output to Evaluate\n{output}\n\n## Evaluation Rubric: {rubric_name}\n{criteria_list}\n\n## Instructions\nScore the output on three axes, each from 0.0 to 1.0:\n- correctness: Does the output correctly accomplish the task?\n- completeness: Does the output cover all requirements?\n- quality: Is the output well-structured, clean, and maintainable?\n\nRespond with ONLY a JSON object:\n{{\"correctness\": <0.0-1.0>, \"completeness\": <0.0-1.0>, \"quality\": <0.0-1.0>, \"feedback\": \"<brief feedback>\", \"issues\": [\"<issue1>\", ...]}}",
        rubric_name = rubric.name,
    )
}

pub struct BuiltInRubrics;

impl BuiltInRubrics {
    pub fn code_quality() -> JudgeRubric {
        JudgeRubric::new(
            "Code Quality".to_string(),
            vec![
                "Does the code handle all edge cases mentioned in the task?".to_string(),
                "Is error handling comprehensive with proper error types?".to_string(),
                "Are there magic numbers or hardcoded values that should be constants?".to_string(),
                "Is the code readable with clear naming and structure?".to_string(),
                "Does the code follow the project's coding standards?".to_string(),
                "Are there appropriate tests for the new functionality?".to_string(),
            ],
        )
    }

    pub fn plan_quality() -> JudgeRubric {
        JudgeRubric::new(
            "Plan Quality".to_string(),
            vec![
                "Does the plan address all requirements from the task?".to_string(),
                "Are the steps ordered logically with clear dependencies?".to_string(),
                "Are potential risks identified with mitigation strategies?".to_string(),
                "Is the estimated complexity reasonable?".to_string(),
                "Are the files to modify correctly identified?".to_string(),
            ],
        )
    }

    pub fn verification() -> JudgeRubric {
        JudgeRubric::new(
            "Verification Quality".to_string(),
            vec![
                "Are all verification checks actually run?".to_string(),
                "Do the check results accurately reflect the state?".to_string(),
                "Are failure messages specific and actionable?".to_string(),
                "Is the overall pass/fail assessment correct?".to_string(),
            ],
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rubric_creation() {
        let rubric = JudgeRubric::new(
            "Code Quality".to_string(),
            vec![
                "Does the code handle edge cases?".to_string(),
                "Is error handling comprehensive?".to_string(),
                "Are there magic numbers?".to_string(),
            ],
        );
        assert_eq!(rubric.name, "Code Quality");
        assert_eq!(rubric.criteria.len(), 3);
    }

    #[test]
    fn verdict_from_scores_all_passing() {
        let verdict = JudgeVerdict::from_scores(0.9, 0.85, 0.95);
        assert!(verdict.passed());
        assert_eq!(verdict.grade(), JudgeGrade::Excellent);
    }

    #[test]
    fn verdict_from_scores_failing() {
        let verdict = JudgeVerdict::from_scores(0.3, 0.4, 0.5);
        assert!(!verdict.passed());
        assert_eq!(verdict.grade(), JudgeGrade::Poor);
    }

    #[test]
    fn verdict_from_scores_borderline() {
        let verdict = JudgeVerdict::from_scores(0.6, 0.65, 0.7);
        assert!(verdict.passed());
        assert_eq!(verdict.grade(), JudgeGrade::Fair);
    }

    #[test]
    fn judge_grade_from_score() {
        assert_eq!(JudgeGrade::from_score(0.95), JudgeGrade::Excellent);
        assert_eq!(JudgeGrade::from_score(0.8), JudgeGrade::Excellent);
        assert_eq!(JudgeGrade::from_score(0.7), JudgeGrade::Good);
        assert_eq!(JudgeGrade::from_score(0.5), JudgeGrade::Fair);
        assert_eq!(JudgeGrade::from_score(0.3), JudgeGrade::Poor);
        assert_eq!(JudgeGrade::from_score(0.1), JudgeGrade::Critical);
    }

    #[test]
    fn judge_config_default_threshold() {
        let config = JudgeConfig::default();
        assert!((config.pass_threshold - 0.6).abs() < f64::EPSILON);
        assert!(!config.required);
    }

    #[test]
    fn judge_config_custom_threshold() {
        let config = JudgeConfig::new(0.8, true);
        assert!((config.pass_threshold - 0.8).abs() < f64::EPSILON);
        assert!(config.required);
    }

    #[test]
    fn build_judge_prompt_contains_rubric_and_task() {
        let rubric = JudgeRubric::new(
            "Code Quality".to_string(),
            vec![
                "Handles edge cases".to_string(),
                "Error handling".to_string(),
            ],
        );
        let prompt = build_judge_prompt(
            "implement a binary search",
            "fn binary_search(arr: &[i32], target: i32) -> Option<usize> { ... }",
            &rubric,
        );
        assert!(prompt.contains("binary search"));
        assert!(prompt.contains("Handles edge cases"));
        assert!(prompt.contains("0.0 to 1.0"));
    }

    #[test]
    fn parse_verdict_from_json() {
        let response = serde_json::json!({
            "correctness": 0.9,
            "completeness": 0.8,
            "quality": 0.85,
            "feedback": "Good implementation with proper edge case handling.",
            "issues": ["Missing documentation for public function"]
        });
        let verdict = JudgeVerdict::parse_from_llm_response(&response.to_string()).unwrap();
        assert!(verdict.passed());
        assert!(!verdict.feedback.is_empty());
        assert_eq!(verdict.issues.len(), 1);
    }

    #[test]
    fn parse_verdict_missing_field() {
        let response = serde_json::json!({
            "correctness": 0.9,
        });
        let result = JudgeVerdict::parse_from_llm_response(&response.to_string());
        assert!(result.is_err());
    }

    #[test]
    fn parse_verdict_invalid_json() {
        let result = JudgeVerdict::parse_from_llm_response("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn built_in_rubric_code_quality() {
        let rubric = BuiltInRubrics::code_quality();
        assert!(!rubric.criteria.is_empty());
        assert!(rubric.name.contains("Code"));
    }

    #[test]
    fn built_in_rubric_plan_quality() {
        let rubric = BuiltInRubrics::plan_quality();
        assert!(!rubric.criteria.is_empty());
    }

    #[test]
    fn built_in_rubric_verification() {
        let rubric = BuiltInRubrics::verification();
        assert!(!rubric.criteria.is_empty());
    }

    #[test]
    fn verdict_serialization_roundtrip() {
        let verdict = JudgeVerdict::from_scores(0.8, 0.7, 0.9);
        let json = serde_json::to_string(&verdict).unwrap();
        let back: JudgeVerdict = serde_json::from_str(&json).unwrap();
        assert!((back.correctness - 0.8).abs() < f64::EPSILON);
        assert!((back.completeness - 0.7).abs() < f64::EPSILON);
        assert!((back.quality - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn judge_rubric_serialization_roundtrip() {
        let rubric = JudgeRubric::new("Test".to_string(), vec!["Criterion 1".to_string()]);
        let json = serde_json::to_string(&rubric).unwrap();
        let back: JudgeRubric = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Test");
        assert_eq!(back.criteria.len(), 1);
    }
}
