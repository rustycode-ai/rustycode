use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum AutonomyLevel {
    #[serde(alias = "L0")]
    L0,
    #[default]
    #[serde(alias = "L1")]
    L1,
    #[serde(alias = "L2")]
    L2,
    #[serde(alias = "L3")]
    L3,
    #[serde(alias = "L4")]
    L4,
}

impl AutonomyLevel {
    pub const fn can_execute(self) -> bool {
        matches!(self, Self::L2 | Self::L3 | Self::L4)
    }
}

impl fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::L0 => "L0 (suggest only)",
            Self::L1 => "L1 (ask permission)",
            Self::L2 => "L2 (execute, notify)",
            Self::L3 => "L3 (execute, notify after)",
            Self::L4 => "L4 (full autonomy)",
        };
        f.write_str(label)
    }
}

impl std::str::FromStr for AutonomyLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "L0" => Ok(Self::L0),
            "L1" => Ok(Self::L1),
            "L2" => Ok(Self::L2),
            "L3" => Ok(Self::L3),
            "L4" => Ok(Self::L4),
            other => Err(format!("invalid autonomy level: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DomainLintConfig {
    pub linter: Option<String>,
    pub config_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DomainFormatterConfig {
    pub formatter: Option<String>,
    pub config_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainContext {
    pub project_name: String,
    pub language: String,
    #[serde(default)]
    pub build_commands: Vec<String>,
    #[serde(default)]
    pub test_commands: Vec<String>,
    #[serde(default)]
    pub architecture_rules: Vec<String>,
    #[serde(default)]
    pub preferred_patterns: Vec<String>,
    #[serde(default)]
    pub test_strategy: Option<String>,
    #[serde(default)]
    pub lint_config: Option<DomainLintConfig>,
    #[serde(default)]
    pub formatter_config: Option<DomainFormatterConfig>,
    #[serde(default)]
    pub autonomy_default: AutonomyLevel,
    #[serde(default)]
    pub autonomy_overrides: HashMap<String, AutonomyLevel>,
}

impl Default for DomainContext {
    fn default() -> Self {
        Self {
            project_name: String::new(),
            language: String::new(),
            build_commands: Vec::new(),
            test_commands: Vec::new(),
            architecture_rules: Vec::new(),
            preferred_patterns: Vec::new(),
            test_strategy: None,
            lint_config: None,
            formatter_config: None,
            autonomy_default: AutonomyLevel::L1,
            autonomy_overrides: HashMap::new(),
        }
    }
}

impl DomainContext {
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str::<Self>(&content)?)
    }

    pub fn discover(root: &Path) -> anyhow::Result<Option<PathBuf>> {
        let candidates = [
            root.join(".rustycode/domain.yaml"),
            root.join(".rustycode/domain.yml"),
            root.join("domain.yaml"),
            root.join("domain.yml"),
        ];

        Ok(candidates.into_iter().find(|path| path.exists()))
    }

    pub fn resolve_autonomy(&self, task_type: &str) -> AutonomyLevel {
        self.autonomy_overrides
            .get(task_type)
            .copied()
            .unwrap_or(self.autonomy_default)
    }

    #[allow(clippy::format_push_string)]
    pub fn format_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("## Domain Context\n\n");
        if !self.project_name.is_empty() {
            out.push_str(&format!("project: {}\n", self.project_name));
        }
        if !self.language.is_empty() {
            out.push_str(&format!("language: {}\n", self.language));
        }
        if !self.build_commands.is_empty() {
            out.push_str("build_commands:\n");
            for command in &self.build_commands {
                out.push_str(&format!("- {command}\n"));
            }
        }
        if !self.test_commands.is_empty() {
            out.push_str("test_commands:\n");
            for command in &self.test_commands {
                out.push_str(&format!("- {command}\n"));
            }
        }
        if !self.architecture_rules.is_empty() {
            out.push_str("architecture_rules:\n");
            for rule in &self.architecture_rules {
                out.push_str(&format!("- {rule}\n"));
            }
        }
        if !self.preferred_patterns.is_empty() {
            out.push_str("preferred_patterns:\n");
            for pattern in &self.preferred_patterns {
                out.push_str(&format!("- {pattern}\n"));
            }
        }
        if let Some(strategy) = &self.test_strategy {
            out.push_str(&format!("test_strategy: {strategy}\n"));
        }
        out.push_str(&format!("autonomy_default: {}\n", self.autonomy_default));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_domain_yaml() {
        let yaml = r"
project_name: rustycode
language: rust
";
        let ctx: DomainContext = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ctx.project_name, "rustycode");
        assert_eq!(ctx.language, "rust");
        assert!(ctx.build_commands.is_empty());
        assert!(ctx.test_commands.is_empty());
        assert!(ctx.architecture_rules.is_empty());
        assert!(ctx.preferred_patterns.is_empty());
    }

    #[test]
    fn parse_full_domain_yaml() {
        let yaml = r#"
project_name: my-api
language: typescript
build_commands:
  - npm run build
  - npm run lint
test_commands:
  - npm test
  - npm run e2e
architecture_rules:
  - "Controllers must not contain business logic"
  - "All database access through repository layer"
  - "Use dependency injection for services"
preferred_patterns:
  - repository-pattern
  - service-layer
  - dependency-injection
test_strategy: jest-with-coverage
lint_config:
  linter: eslint
  config_file: .eslintrc.json
formatter_config:
  formatter: prettier
  config_file: .prettierrc
autonomy_default: L2
autonomy_overrides:
  code_review: L3
  database_migration: L0
  deployment: L1
"#;
        let ctx: DomainContext = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ctx.project_name, "my-api");
        assert_eq!(ctx.language, "typescript");
        assert_eq!(ctx.build_commands.len(), 2);
        assert_eq!(ctx.test_commands.len(), 2);
        assert_eq!(ctx.architecture_rules.len(), 3);
        assert_eq!(ctx.preferred_patterns.len(), 3);
        assert_eq!(ctx.test_strategy.as_deref(), Some("jest-with-coverage"));
        assert_eq!(ctx.autonomy_default, AutonomyLevel::L2);
        assert_eq!(
            ctx.autonomy_overrides.get("code_review"),
            Some(&AutonomyLevel::L3)
        );
        assert_eq!(
            ctx.autonomy_overrides.get("database_migration"),
            Some(&AutonomyLevel::L0)
        );
    }

    #[test]
    fn default_domain_context_safe_values() {
        let ctx = DomainContext::default();
        assert!(ctx.project_name.is_empty());
        assert!(ctx.language.is_empty());
        assert_eq!(ctx.autonomy_default, AutonomyLevel::L1);
        assert!(ctx.autonomy_overrides.is_empty());
    }

    #[test]
    fn autonomy_level_from_str() {
        assert_eq!("L0".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L0));
        assert_eq!("L1".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L1));
        assert_eq!("L2".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L2));
        assert_eq!("L3".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L3));
        assert_eq!("L4".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L4));
        assert!("L5".parse::<AutonomyLevel>().is_err());
        assert!("invalid".parse::<AutonomyLevel>().is_err());
    }

    #[test]
    fn autonomy_level_display() {
        assert_eq!(format!("{}", AutonomyLevel::L0), "L0 (suggest only)");
        assert_eq!(format!("{}", AutonomyLevel::L1), "L1 (ask permission)");
        assert_eq!(format!("{}", AutonomyLevel::L2), "L2 (execute, notify)");
        assert_eq!(
            format!("{}", AutonomyLevel::L3),
            "L3 (execute, notify after)"
        );
        assert_eq!(format!("{}", AutonomyLevel::L4), "L4 (full autonomy)");
    }

    #[test]
    fn autonomy_level_ordering() {
        assert!(AutonomyLevel::L0 < AutonomyLevel::L1);
        assert!(AutonomyLevel::L1 < AutonomyLevel::L2);
        assert!(AutonomyLevel::L2 < AutonomyLevel::L3);
        assert!(AutonomyLevel::L3 < AutonomyLevel::L4);
    }

    #[test]
    fn autonomy_level_serde_roundtrip() {
        for level in [
            AutonomyLevel::L0,
            AutonomyLevel::L1,
            AutonomyLevel::L2,
            AutonomyLevel::L3,
            AutonomyLevel::L4,
        ] {
            let yaml = serde_yaml::to_string(&level).unwrap();
            let decoded: AutonomyLevel = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(decoded, level);
        }
    }

    #[test]
    fn resolve_autonomy_level_with_overrides() {
        let mut ctx = DomainContext {
            project_name: "test".to_string(),
            language: "rust".to_string(),
            autonomy_default: AutonomyLevel::L2,
            autonomy_overrides: {
                let mut map = HashMap::new();
                map.insert("code_review".to_string(), AutonomyLevel::L3);
                map.insert("database_migration".to_string(), AutonomyLevel::L0);
                map
            },
            ..Default::default()
        };
        assert_eq!(ctx.resolve_autonomy("code_review"), AutonomyLevel::L3);
        assert_eq!(
            ctx.resolve_autonomy("database_migration"),
            AutonomyLevel::L0
        );
        assert_eq!(ctx.resolve_autonomy("unknown"), AutonomyLevel::L2);
        ctx.autonomy_overrides
            .insert("unknown".to_string(), AutonomyLevel::L4);
        assert_eq!(ctx.resolve_autonomy("unknown"), AutonomyLevel::L4);
    }
}
