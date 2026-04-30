use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type SkillId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    Bundled,
    Managed,
    User,
    Project,
    Mcp,
    Plugin,
    Dynamic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    Always,
    Manual,
    Conditional,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationSpec {
    pub mode: ActivationMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trigger_tools: Vec<String>,
}

impl ActivationSpec {
    #[allow(clippy::missing_const_for_fn)]
    pub fn always() -> Self {
        Self {
            mode: ActivationMode::Always,
            paths: Vec::new(),
            trigger_tools: Vec::new(),
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn manual() -> Self {
        Self {
            mode: ActivationMode::Manual,
            paths: Vec::new(),
            trigger_tools: Vec::new(),
        }
    }

    pub const fn conditional(paths: Vec<String>) -> Self {
        Self {
            mode: ActivationMode::Conditional,
            paths,
            trigger_tools: Vec::new(),
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn auto() -> Self {
        Self {
            mode: ActivationMode::Auto,
            paths: Vec::new(),
            trigger_tools: Vec::new(),
        }
    }

    pub fn matches_path(&self, file_path: &str) -> bool {
        if self.paths.is_empty() {
            return false;
        }
        self.paths
            .iter()
            .any(|pattern| glob::Pattern::new(pattern).is_ok_and(|p| p.matches(file_path)))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillEffortLevel {
    Low,
    #[default]
    Medium,
    High,
    Max,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContext {
    #[default]
    Inline,
    Fork,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureKind {
    Instruction,
    Pipeline(Pipeline),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pipeline {
    pub stages: Vec<PipelineStage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineStage {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_tools: Vec<String>,
    #[serde(default)]
    pub parallel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGrade {
    Excellent,
    Good,
    Fair,
    Poor,
    Critical,
}

impl QualityGrade {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::Excellent
        } else if score >= 0.6 {
            Self::Good
        } else if score >= 0.4 {
            Self::Fair
        } else if score >= 0.2 {
            Self::Poor
        } else {
            Self::Critical
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    #[default]
    Discovered,
    Loaded,
    Active,
    Latent,
    Suspended,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillQuality {
    pub telemetry_score: f64,
    pub graph_score: f64,
    pub intake_score: f64,
    pub routing_score: f64,
}

impl SkillQuality {
    pub const fn new(telemetry: f64, graph: f64, intake: f64, routing: f64) -> Self {
        Self {
            telemetry_score: telemetry.clamp(0.0, 1.0),
            graph_score: graph.clamp(0.0, 1.0),
            intake_score: intake.clamp(0.0, 1.0),
            routing_score: routing.clamp(0.0, 1.0),
        }
    }

    /// Weighted total: telemetry 40%, graph 25%, intake 20%, routing 15%
    pub fn weighted_total(&self) -> f64 {
        self.telemetry_score.mul_add(
            0.40,
            self.graph_score.mul_add(
                0.25,
                self.intake_score.mul_add(0.20, self.routing_score * 0.15),
            ),
        )
    }

    pub fn grade(&self) -> QualityGrade {
        QualityGrade::from_score(self.weighted_total())
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn default_new() -> Self {
        Self {
            telemetry_score: 0.5,
            graph_score: 0.5,
            intake_score: 0.7,
            routing_score: 0.5,
        }
    }
}

impl Default for SkillQuality {
    fn default() -> Self {
        Self::default_new()
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub when_to_use: String,
    pub source: SkillSource,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    pub activation: ActivationSpec,
    #[serde(default)]
    pub effort: SkillEffortLevel,
    #[serde(default)]
    pub context: ExecutionContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure: Option<ProcedureKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    #[serde(default = "default_true")]
    pub user_invocable: bool,
    #[serde(default = "default_true")]
    pub model_invocable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gotchas: Vec<String>,
    #[serde(default)]
    pub quality: SkillQuality,
    #[serde(default)]
    pub lifecycle_state: LifecycleState,
    #[serde(skip)]
    pub content_path: PathBuf,
    #[serde(skip)]
    pub content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_spec_always() {
        let spec = ActivationSpec::always();
        assert_eq!(spec.mode, ActivationMode::Always);
        assert!(spec.paths.is_empty());
    }

    #[test]
    fn activation_spec_conditional_matches() {
        let spec = ActivationSpec::conditional(vec!["*.rs".to_string(), "src/**/*.rs".to_string()]);
        assert_eq!(spec.mode, ActivationMode::Conditional);
        assert!(spec.matches_path("main.rs"));
        assert!(spec.matches_path("src/lib.rs"));
        assert!(!spec.matches_path("main.py"));
    }

    #[test]
    fn activation_spec_empty_paths_no_match() {
        let spec = ActivationSpec::always();
        assert!(!spec.matches_path("anything.rs"));
    }

    #[test]
    fn quality_weighted_total() {
        let q = SkillQuality::new(1.0, 1.0, 1.0, 1.0);
        let total = q.weighted_total();
        assert!((total - 1.0).abs() < f64::EPSILON);
        assert_eq!(q.grade(), QualityGrade::Excellent);
    }

    #[test]
    fn quality_grades() {
        assert_eq!(QualityGrade::from_score(0.9), QualityGrade::Excellent);
        assert_eq!(QualityGrade::from_score(0.7), QualityGrade::Good);
        assert_eq!(QualityGrade::from_score(0.5), QualityGrade::Fair);
        assert_eq!(QualityGrade::from_score(0.3), QualityGrade::Poor);
        assert_eq!(QualityGrade::from_score(0.1), QualityGrade::Critical);
    }

    #[test]
    fn quality_clamps_scores() {
        let q = SkillQuality::new(2.0, -1.0, 0.5, 1.5);
        assert!((q.telemetry_score - 1.0).abs() < f64::EPSILON);
        assert!((q.graph_score - 0.0).abs() < f64::EPSILON);
        assert!((q.routing_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn quality_default_new() {
        let q = SkillQuality::default_new();
        assert!((q.telemetry_score - 0.5).abs() < f64::EPSILON);
        assert!((q.intake_score - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_definition_serialization() {
        let def = SkillDefinition {
            id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            description: "A test".to_string(),
            when_to_use: String::new(),
            source: SkillSource::Bundled,
            version: "1.0".to_string(),
            activation: ActivationSpec::always(),
            effort: SkillEffortLevel::default(),
            context: ExecutionContext::default(),
            procedure: None,
            allowed_tools: vec![],
            user_invocable: true,
            model_invocable: true,
            agent: None,
            model_override: None,
            argument_hint: None,
            categories: vec![],
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default(),
            lifecycle_state: LifecycleState::default(),
            content_path: PathBuf::from("/test/SKILL.md"),
            content: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("\"id\":\"test-skill\""));
        assert!(json.contains("\"name\":\"Test Skill\""));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("content_path").is_none());
        assert!(parsed.get("content").is_none());
    }

    #[test]
    fn effort_level_default() {
        assert_eq!(SkillEffortLevel::default(), SkillEffortLevel::Medium);
    }

    #[test]
    fn execution_context_default() {
        assert_eq!(ExecutionContext::default(), ExecutionContext::Inline);
    }

    #[test]
    fn lifecycle_state_default() {
        assert_eq!(LifecycleState::default(), LifecycleState::Discovered);
    }
}
