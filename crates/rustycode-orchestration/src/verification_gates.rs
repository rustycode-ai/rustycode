//! Verification Gate (The Critic)

use crate::error::Result;
use crate::error_signal::SignalCategory;
use crate::execution_trace::TraceEntry;
use crate::judge::{JudgeConfig, JudgeVerdict};
use crate::schema::OutputSchema;
use crate::types::{OutputType, Step};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationOutcome {
    Valid,
    Invalid {
        reason: String,
        category: SignalCategory,
    },
    Uncertain {
        reason: String,
    },
}

pub trait VerificationStrategy: Send + Sync {
    fn verify(&self, step: &Step, result: &TraceEntry) -> VerificationOutcome;
}

pub struct JudgeVerificationStrategy {
    verdict: JudgeVerdict,
    config: JudgeConfig,
}

impl JudgeVerificationStrategy {
    pub const fn new(verdict: JudgeVerdict, config: JudgeConfig) -> Self {
        Self { verdict, config }
    }
}

impl VerificationStrategy for JudgeVerificationStrategy {
    fn verify(&self, _step: &Step, _result: &TraceEntry) -> VerificationOutcome {
        if self.verdict.passed_with(self.config.pass_threshold) {
            VerificationOutcome::Valid
        } else if self.config.required {
            VerificationOutcome::Invalid {
                reason: format!(
                    "Judge verdict failed: score {:.2} < threshold {:.2}. Feedback: {}",
                    self.verdict.weighted_score(),
                    self.config.pass_threshold,
                    self.verdict.feedback
                ),
                category: SignalCategory::LogicError,
            }
        } else {
            VerificationOutcome::Valid
        }
    }
}

pub struct SchemaVerificationStrategy {
    schema: OutputSchema,
}

impl SchemaVerificationStrategy {
    pub const fn new(schema: OutputSchema) -> Self {
        Self { schema }
    }
}

impl VerificationStrategy for SchemaVerificationStrategy {
    fn verify(&self, _step: &Step, result: &TraceEntry) -> VerificationOutcome {
        match serde_json::from_str::<serde_json::Value>(&result.output) {
            Ok(instance) => {
                let validation = self.schema.validate(&instance);
                if validation.is_valid() {
                    VerificationOutcome::Valid
                } else {
                    VerificationOutcome::Invalid {
                        reason: format!(
                            "Output schema validation failed: {}",
                            validation.error_message()
                        ),
                        category: SignalCategory::TypeError,
                    }
                }
            }
            Err(err) => VerificationOutcome::Invalid {
                reason: format!("Output is not valid JSON: {err}"),
                category: SignalCategory::SyntaxError,
            },
        }
    }
}

pub struct VerificationGateRegistry {
    strategies: HashMap<OutputType, Vec<Box<dyn VerificationStrategy>>>,
}

impl Default for VerificationGateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VerificationGateRegistry {
    pub fn new() -> Self {
        Self {
            strategies: HashMap::new(),
        }
    }

    pub fn register_strategy(
        &mut self,
        output_type: OutputType,
        strategy: Box<dyn VerificationStrategy>,
    ) {
        self.strategies
            .entry(output_type)
            .or_default()
            .push(strategy);
    }

    pub fn verify(&self, step: &Step, result: &TraceEntry) -> VerificationOutcome {
        if let Some(strategies) = self.strategies.get(&step.expected_output_type) {
            for strategy in strategies {
                let outcome = strategy.verify(step, result);
                if !matches!(outcome, VerificationOutcome::Valid) {
                    return outcome;
                }
            }
        }
        VerificationOutcome::Valid
    }
}

pub struct ExitCodeStrategy {
    pub valid_codes: Vec<i32>,
}

impl VerificationStrategy for ExitCodeStrategy {
    fn verify(&self, _step: &Step, result: &TraceEntry) -> VerificationOutcome {
        match result.exit_code {
            Some(code) if self.valid_codes.contains(&code) => VerificationOutcome::Valid,
            Some(code) => VerificationOutcome::Invalid {
                reason: format!("Exit code {code} not in valid codes {:?}", self.valid_codes),
                category: SignalCategory::LogicError,
            },
            None => VerificationOutcome::Uncertain {
                reason: "No exit code available for exit-code verification".into(),
            },
        }
    }
}

pub struct RegexStrategy {
    pub pattern: regex::Regex,
    pub fail_on_match: bool,
}

impl RegexStrategy {
    pub fn new(pattern: &str, fail_on_match: bool) -> std::result::Result<Self, regex::Error> {
        Ok(Self {
            pattern: regex::Regex::new(pattern)?,
            fail_on_match,
        })
    }
}

impl VerificationStrategy for RegexStrategy {
    fn verify(&self, _step: &Step, result: &TraceEntry) -> VerificationOutcome {
        let matched = self.pattern.is_match(&result.output);
        let invalid = if self.fail_on_match {
            matched
        } else {
            !matched
        };
        if invalid {
            VerificationOutcome::Invalid {
                reason: format!(
                    "Regex verification failed for pattern {}",
                    self.pattern.as_str()
                ),
                category: SignalCategory::LogicError,
            }
        } else {
            VerificationOutcome::Valid
        }
    }
}

pub struct FileExistsStrategy;

impl VerificationStrategy for FileExistsStrategy {
    fn verify(&self, _step: &Step, result: &TraceEntry) -> VerificationOutcome {
        let path = result.output.trim();
        if path.is_empty() || !std::path::Path::new(path).exists() {
            VerificationOutcome::Invalid {
                reason: format!("Expected file '{path}' does not exist"),
                category: SignalCategory::LogicError,
            }
        } else {
            VerificationOutcome::Valid
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub description: String,
    pub check: String,
    pub valid_codes: Option<Vec<i32>>,
    pub pattern: Option<String>,
    pub format: Option<String>,
    pub on_failure: String,
    pub on_match: Option<String>,
}

pub trait VerificationGate: Send + Sync {
    fn verify(&self, step: &Step, result: &TraceEntry) -> VerificationOutcome;
}

pub struct RuleFileVerificationGate {
    rules_dir: std::path::PathBuf,
    rules_by_task_type: HashMap<String, Vec<Rule>>,
}

impl RuleFileVerificationGate {
    pub fn new(rules_dir: &Path) -> Result<Self> {
        let mut rules_by_task_type = HashMap::new();

        if rules_dir.exists() {
            for entry in std::fs::read_dir(rules_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => match serde_yaml::from_str::<RuleFile>(&content) {
                            Ok(rule_file) => {
                                rules_by_task_type.insert(rule_file.task_type, rule_file.rules);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    path = %path.display(),
                                    error = %e,
                                    "skipping unparseable rule file"
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "skipping unreadable rule file"
                            );
                        }
                    }
                }
            }
        }

        Ok(Self {
            rules_dir: rules_dir.to_path_buf(),
            rules_by_task_type,
        })
    }

    pub fn rules_dir(&self) -> &Path {
        &self.rules_dir
    }

    fn get_rules_for_task_type(&self, task_type: &str) -> &[Rule] {
        self.rules_by_task_type.get(task_type).map_or_else(
            || {
                self.rules_by_task_type
                    .get("default")
                    .map_or(&[], Vec::as_slice)
            },
            Vec::as_slice,
        )
    }

    fn evaluate_rule(
        rule: &Rule,
        _step: &Step,
        result: &TraceEntry,
    ) -> Option<VerificationOutcome> {
        match rule.check.as_str() {
            "exit_code" => {
                if let Some(exit_code) = result.exit_code {
                    if let Some(valid_codes) = &rule.valid_codes {
                        if !valid_codes.contains(&exit_code) {
                            let category = Self::parse_error_category(&rule.on_failure);
                            return Some(VerificationOutcome::Invalid {
                                reason: format!(
                                    "Exit code {exit_code} not in valid codes {valid_codes:?}"
                                ),
                                category,
                            });
                        }
                    }
                }
            }
            "regex" => {
                if let Some(pattern) = &rule.pattern {
                    match regex::Regex::new(pattern) {
                        Ok(regex) => {
                            let is_match = regex.is_match(&result.output);
                            let fail_on_match = rule.on_match.as_deref() == Some("Invalid");
                            let should_fail = if fail_on_match { is_match } else { !is_match };

                            if should_fail {
                                let category = Self::parse_error_category(&rule.on_failure);
                                return Some(VerificationOutcome::Invalid {
                                    reason: format!(
                                        "Regex check '{}' failed for output",
                                        rule.description
                                    ),
                                    category,
                                });
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                pattern = %pattern,
                                error = %e,
                                "skipping verification rule with invalid regex"
                            );
                        }
                    }
                }
            }
            "file_exists" if !std::path::Path::new(&result.output.trim()).exists() => {
                let category = Self::parse_error_category(&rule.on_failure);
                return Some(VerificationOutcome::Invalid {
                    reason: format!("Expected file '{}' does not exist", result.output.trim()),
                    category,
                });
            }
            "format_validation" => {
                if let Some(format_type) = &rule.format {
                    if format_type == "json_or_csv" {
                        let is_json =
                            serde_json::from_str::<serde_json::Value>(&result.output).is_ok();
                        let is_csv =
                            result.output.contains(',') && result.output.lines().count() > 1;
                        if !is_json && !is_csv {
                            let category = Self::parse_error_category(&rule.on_failure);
                            return Some(VerificationOutcome::Invalid {
                                reason: "Output is neither valid JSON nor CSV".to_string(),
                                category,
                            });
                        }
                    }
                }
            }
            _ => {
                tracing::warn!(
                    rule_type = %rule.check,
                    rule_name = %rule.description,
                    "skipping unrecognized verification rule type"
                );
            }
        }
        None
    }

    fn parse_error_category(category_str: &str) -> SignalCategory {
        match category_str {
            "SyntaxError" => SignalCategory::SyntaxError,
            "CompileError" => SignalCategory::CompileError,
            "TypeError" => SignalCategory::TypeError,
            "LogicError" => SignalCategory::LogicError,
            "PermissionDenied" => SignalCategory::PermissionDenied,
            "DiskFull" => SignalCategory::DiskFull,
            "ToolTimeout" => SignalCategory::ToolTimeout,
            "ContextLengthExceeded" => SignalCategory::ContextLengthExceeded,
            "Internal" => SignalCategory::Internal,
            _ => SignalCategory::Custom(category_str.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuleFile {
    task_type: String,
    rules: Vec<Rule>,
}

impl VerificationGate for RuleFileVerificationGate {
    fn verify(&self, step: &Step, result: &TraceEntry) -> VerificationOutcome {
        let task_type = step.suggested_tool.as_deref().unwrap_or("unknown");
        let rules = self.get_rules_for_task_type(task_type);

        for rule in rules {
            if let Some(outcome) = Self::evaluate_rule(rule, step, result) {
                return outcome;
            }
        }

        VerificationOutcome::Valid
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_step(output_type: OutputType) -> Step {
        Step {
            id: "s1".into(),
            index: 0,
            description: "test".into(),
            expected_output_type: output_type,
            suggested_tool: None,
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::default(),
        }
    }

    fn make_entry(output: &str, exit_code: Option<i32>) -> TraceEntry {
        TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "test".into(),
            serde_json::json!({}),
            output.into(),
            exit_code,
            0.0,
        )
    }

    #[test]
    fn test_empty_registry_returns_valid() {
        let registry = VerificationGateRegistry::new();
        let step = make_step(OutputType::Code);
        let entry = make_entry("hello", Some(0));
        assert!(matches!(
            registry.verify(&step, &entry),
            VerificationOutcome::Valid
        ));
    }

    #[test]
    fn test_parse_error_category() {
        assert_eq!(
            RuleFileVerificationGate::parse_error_category("LogicError"),
            SignalCategory::LogicError
        );
        assert!(matches!(
            RuleFileVerificationGate::parse_error_category("Custom"),
            SignalCategory::Custom(_)
        ));
    }

    #[test]
    fn test_get_rules_for_task_type_returns_default() {
        let gate = RuleFileVerificationGate::new(std::path::Path::new("/nonexistent")).unwrap();
        let rules = gate.get_rules_for_task_type("nonexistent");
        assert!(rules.is_empty());
    }

    #[test]
    fn test_verify_with_no_rules_returns_valid() {
        let gate = RuleFileVerificationGate::new(std::path::Path::new("/nonexistent")).unwrap();
        let step = make_step(OutputType::Code);
        let entry = make_entry("hello", Some(0));
        let outcome = gate.verify(&step, &entry);
        assert!(matches!(outcome, VerificationOutcome::Valid));
    }

    #[test]
    fn test_evaluate_rule_exit_code_invalid() {
        let rule = Rule {
            description: "check exit code".into(),
            check: "exit_code".into(),
            valid_codes: Some(vec![0]),
            pattern: None,
            format: None,
            on_failure: "LogicError".into(),
            on_match: None,
        };
        let step = make_step(OutputType::Code);
        let entry = make_entry("output", Some(1));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(matches!(result, Some(VerificationOutcome::Invalid { .. })));
    }

    #[test]
    fn test_evaluate_rule_exit_code_valid() {
        let rule = Rule {
            description: "check exit code".into(),
            check: "exit_code".into(),
            valid_codes: Some(vec![0]),
            pattern: None,
            format: None,
            on_failure: "LogicError".into(),
            on_match: None,
        };
        let step = make_step(OutputType::Code);
        let entry = make_entry("output", Some(0));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(result.is_none());
    }

    #[test]
    fn test_registry_with_custom_strategy_valid() {
        struct AlwaysValid;
        impl VerificationStrategy for AlwaysValid {
            fn verify(&self, _step: &Step, _result: &TraceEntry) -> VerificationOutcome {
                VerificationOutcome::Valid
            }
        }

        let mut registry = VerificationGateRegistry::new();
        registry.register_strategy(OutputType::Code, Box::new(AlwaysValid));
        let step = make_step(OutputType::Code);
        let entry = make_entry("anything", Some(0));
        assert!(matches!(
            registry.verify(&step, &entry),
            VerificationOutcome::Valid
        ));
    }

    #[test]
    fn test_registry_with_custom_strategy_invalid() {
        struct AlwaysInvalid;
        impl VerificationStrategy for AlwaysInvalid {
            fn verify(&self, _step: &Step, _result: &TraceEntry) -> VerificationOutcome {
                VerificationOutcome::Invalid {
                    reason: "forced failure".into(),
                    category: SignalCategory::LogicError,
                }
            }
        }

        let mut registry = VerificationGateRegistry::new();
        registry.register_strategy(OutputType::Code, Box::new(AlwaysInvalid));
        let step = make_step(OutputType::Code);
        let entry = make_entry("anything", Some(0));
        assert!(matches!(
            registry.verify(&step, &entry),
            VerificationOutcome::Invalid { .. }
        ));
    }

    #[test]
    fn test_registry_strategy_not_matching_output_type() {
        struct AlwaysInvalid;
        impl VerificationStrategy for AlwaysInvalid {
            fn verify(&self, _step: &Step, _result: &TraceEntry) -> VerificationOutcome {
                VerificationOutcome::Invalid {
                    reason: "forced".into(),
                    category: SignalCategory::Internal,
                }
            }
        }

        let mut registry = VerificationGateRegistry::new();
        registry.register_strategy(OutputType::Code, Box::new(AlwaysInvalid));
        // Step expects Command, but strategy registered for Code — should return Valid
        let step = make_step(OutputType::Command);
        let entry = make_entry("anything", Some(0));
        assert!(matches!(
            registry.verify(&step, &entry),
            VerificationOutcome::Valid
        ));
    }

    #[test]
    fn test_judge_strategy_required_passes_above_threshold() {
        let verdict = crate::judge::JudgeVerdict::from_scores(0.9, 0.9, 0.9);
        let strategy =
            JudgeVerificationStrategy::new(verdict, crate::judge::JudgeConfig::default());
        let step = make_step(OutputType::Code);
        let entry = make_entry("anything", Some(0));
        assert!(matches!(
            strategy.verify(&step, &entry),
            VerificationOutcome::Valid
        ));
    }

    #[test]
    fn test_judge_strategy_required_fails_below_threshold() {
        let verdict = crate::judge::JudgeVerdict::from_scores(0.1, 0.2, 0.3);
        let config = crate::judge::JudgeConfig::new(0.8, true);
        let strategy = JudgeVerificationStrategy::new(verdict, config);
        let step = make_step(OutputType::Code);
        let entry = make_entry("anything", Some(0));
        assert!(matches!(
            strategy.verify(&step, &entry),
            VerificationOutcome::Invalid { .. }
        ));
    }

    #[test]
    fn test_schema_strategy_valid_json_passes() {
        let schema = crate::schema::OutputSchema::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "passed": { "type": "boolean" },
                "checks": { "type": "array" }
            },
            "required": ["passed", "checks"]
        }));
        let strategy = SchemaVerificationStrategy::new(schema);
        let step = make_step(OutputType::Verification);
        let entry = make_entry(
            r#"{"passed": true, "checks": [{"name": "compile", "passed": true}]}"#,
            Some(0),
        );
        assert!(matches!(
            strategy.verify(&step, &entry),
            VerificationOutcome::Valid
        ));
    }

    #[test]
    fn test_schema_strategy_invalid_json_fails() {
        let schema = crate::schema::OutputSchema::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "passed": { "type": "boolean" },
                "checks": { "type": "array" }
            },
            "required": ["passed", "checks"]
        }));
        let strategy = SchemaVerificationStrategy::new(schema);
        let step = make_step(OutputType::Verification);
        let entry = make_entry("not valid json", Some(0));
        assert!(matches!(
            strategy.verify(&step, &entry),
            VerificationOutcome::Invalid { .. }
        ));
    }

    #[test]
    fn test_evaluate_rule_regex_no_match_fails() {
        let rule = Rule {
            description: "must contain success".into(),
            check: "regex".into(),
            valid_codes: None,
            pattern: Some(r"success".into()),
            format: None,
            on_failure: "LogicError".into(),
            on_match: None,
        };
        let step = make_step(OutputType::Code);
        let entry = make_entry("error: something failed", Some(0));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(matches!(result, Some(VerificationOutcome::Invalid { .. })));
    }

    #[test]
    fn test_evaluate_rule_regex_match_passes() {
        let rule = Rule {
            description: "must contain success".into(),
            check: "regex".into(),
            valid_codes: None,
            pattern: Some(r"success".into()),
            format: None,
            on_failure: "LogicError".into(),
            on_match: None,
        };
        let step = make_step(OutputType::Code);
        let entry = make_entry("operation success", Some(0));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(result.is_none());
    }

    #[test]
    fn test_evaluate_rule_regex_fail_on_match() {
        let rule = Rule {
            description: "must not contain error".into(),
            check: "regex".into(),
            valid_codes: None,
            pattern: Some(r"error".into()),
            format: None,
            on_failure: "LogicError".into(),
            on_match: Some("Invalid".into()),
        };
        let step = make_step(OutputType::Code);
        let entry = make_entry("error: something bad", Some(0));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(matches!(result, Some(VerificationOutcome::Invalid { .. })));
    }

    #[test]
    fn test_evaluate_rule_format_validation_json() {
        let rule = Rule {
            description: "must be json or csv".into(),
            check: "format_validation".into(),
            valid_codes: None,
            pattern: None,
            format: Some("json_or_csv".into()),
            on_failure: "TypeError".into(),
            on_match: None,
        };
        let step = make_step(OutputType::Data);
        let entry = make_entry(r#"{"key": "value"}"#, Some(0));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(result.is_none()); // Valid JSON should pass
    }

    #[test]
    fn test_evaluate_rule_format_validation_invalid() {
        let rule = Rule {
            description: "must be json or csv".into(),
            check: "format_validation".into(),
            valid_codes: None,
            pattern: None,
            format: Some("json_or_csv".into()),
            on_failure: "TypeError".into(),
            on_match: None,
        };
        let step = make_step(OutputType::Data);
        let entry = make_entry("not json or csv at all", Some(0));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(matches!(result, Some(VerificationOutcome::Invalid { .. })));
    }

    #[test]
    fn test_evaluate_rule_unknown_check_returns_none() {
        let rule = Rule {
            description: "unknown check".into(),
            check: "nonexistent".into(),
            valid_codes: None,
            pattern: None,
            format: None,
            on_failure: "Internal".into(),
            on_match: None,
        };
        let step = make_step(OutputType::Code);
        let entry = make_entry("anything", Some(0));
        let result = RuleFileVerificationGate::evaluate_rule(&rule, &step, &entry);
        assert!(result.is_none());
    }

    #[test]
    fn test_yaml_rule_loading_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_content = r#"
task_type: Bash
rules:
  - description: "check exit code"
    check: "exit_code"
    valid_codes: [0]
    on_failure: "LogicError"
"#;
        std::fs::write(dir.path().join("bash_rules.yaml"), yaml_content).unwrap();

        let gate = RuleFileVerificationGate::new(dir.path()).unwrap();
        let step = Step {
            id: "s1".into(),
            index: 0,
            description: "test".into(),
            expected_output_type: OutputType::Command,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let entry = make_entry("ok", Some(1));
        let outcome = gate.verify(&step, &entry);
        assert!(matches!(outcome, VerificationOutcome::Invalid { .. }));
    }

    #[test]
    fn test_verification_gate_trait_impl() {
        let gate = RuleFileVerificationGate::new(std::path::Path::new("/nonexistent")).unwrap();
        let step = Step {
            id: "s1".into(),
            index: 0,
            description: "test".into(),
            expected_output_type: OutputType::Command,
            suggested_tool: Some("custom_tool".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let entry = make_entry("output", Some(0));
        let outcome = gate.verify(&step, &entry);
        assert!(matches!(outcome, VerificationOutcome::Valid));
    }

    #[test]
    fn test_rules_dir_accessor() {
        let dir = std::path::Path::new("/nonexistent");
        let gate = RuleFileVerificationGate::new(dir).unwrap();
        assert_eq!(gate.rules_dir(), dir);
    }
}
