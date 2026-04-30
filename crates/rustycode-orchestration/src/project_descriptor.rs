//! Project descriptor YAML schema for domain context files.
//!
//! Defines the `ProjectDescriptor` struct and supporting types that represent
//! the YAML schema for `.rustycode/domain.yaml` files. This is the raw schema
//! that gets parsed from disk, validated, and then used to construct a
//! `DomainContext`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// A technology component in the project's tech stack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TechComponent {
    /// Name of the technology (e.g., "tokio", "react", "postgresql").
    pub name: String,
    /// Version constraint or specific version (e.g., "1.x", "18.2.0").
    #[serde(default)]
    pub version: Option<String>,
    /// Category of the technology (e.g., "runtime", "framework", "database").
    #[serde(default)]
    pub category: Option<String>,
}

/// A project convention or coding standard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Convention {
    /// Human-readable description of the convention.
    pub description: String,
    /// Category of the convention (e.g., "naming", "error-handling", "testing").
    #[serde(default)]
    pub category: Option<String>,
    /// Severity: "required" or "preferred".
    #[serde(default = "default_severity")]
    pub severity: String,
}

fn default_severity() -> String {
    "preferred".to_string()
}

/// A project boundary or constraint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Boundary {
    /// Description of the boundary.
    pub description: String,
    /// What is constrained by this boundary.
    pub constrained: String,
    /// Boundary type: "architectural", "security", "performance", "dependency".
    #[serde(default)]
    pub boundary_type: Option<String>,
}

/// Linter or formatter tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolConfig {
    /// Tool name (e.g., "eslint", "prettier", "clippy").
    pub name: String,
    /// Path to config file (relative to project root).
    #[serde(default)]
    pub config_file: Option<String>,
}

/// Project descriptor: the raw YAML schema for `.rustycode/domain.yaml`.
///
/// Contains all metadata about a project that the agent needs to understand
/// the project's conventions, architecture rules, tech stack, and boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectDescriptor {
    /// Project name.
    #[serde(default)]
    pub project_name: String,

    /// Primary programming language.
    #[serde(default)]
    pub language: String,

    /// Technology stack components.
    #[serde(default)]
    pub tech_stack: Vec<TechComponent>,

    /// Commands to build the project.
    #[serde(default)]
    pub build_commands: Vec<String>,

    /// Commands to run tests.
    #[serde(default)]
    pub test_commands: Vec<String>,

    /// Architecture rules that the agent must follow.
    #[serde(default)]
    pub architecture_rules: Vec<String>,

    /// Preferred design patterns (e.g., "repository-pattern").
    #[serde(default)]
    pub preferred_patterns: Vec<String>,

    /// Project conventions and coding standards.
    #[serde(default)]
    pub conventions: Vec<Convention>,

    /// Project boundaries and constraints.
    #[serde(default)]
    pub boundaries: Vec<Boundary>,

    /// Test strategy description (e.g., "jest-with-coverage").
    #[serde(default)]
    pub test_strategy: Option<String>,

    /// Linter configuration.
    #[serde(default)]
    pub lint_config: Option<ToolConfig>,

    /// Formatter configuration.
    #[serde(default)]
    pub formatter_config: Option<ToolConfig>,

    /// Default autonomy level for all tasks.
    #[serde(default)]
    pub autonomy_default: crate::autonomy::AutonomyLevel,

    /// Per-task-type autonomy overrides.
    #[serde(default)]
    pub autonomy_overrides: std::collections::HashMap<String, crate::autonomy::AutonomyLevel>,

    /// Path the descriptor was loaded from (not serialized).
    #[serde(skip)]
    pub loaded_from: Option<PathBuf>,
}

impl ProjectDescriptor {
    /// Parse a project descriptor from a YAML string.
    pub fn parse(yaml: &str) -> Result<Self> {
        let descriptor: Self = serde_yaml::from_str(yaml)
            .with_context(|| "Failed to parse project descriptor YAML")?;
        Ok(descriptor)
    }

    /// Serialize the project descriptor to a YAML string.
    pub fn serialize(&self) -> Result<String> {
        serde_yaml::to_string(self)
            .with_context(|| "Failed to serialize project descriptor to YAML")
    }

    /// Validate the project descriptor for consistency and completeness.
    pub fn validate(&self) -> Result<Vec<ValidationWarning>> {
        let mut warnings = Vec::new();

        if self.project_name.is_empty() {
            warnings.push(ValidationWarning {
                field: "project_name".to_string(),
                message: "project_name is empty".to_string(),
                severity: Severity::Low,
            });
        }

        if self.language.is_empty() {
            warnings.push(ValidationWarning {
                field: "language".to_string(),
                message: "language is empty, agent may not understand project conventions"
                    .to_string(),
                severity: Severity::Medium,
            });
        }

        if self.build_commands.is_empty() {
            warnings.push(ValidationWarning {
                field: "build_commands".to_string(),
                message: "no build_commands specified, agent cannot verify builds".to_string(),
                severity: Severity::Medium,
            });
        }

        if self.test_commands.is_empty() {
            warnings.push(ValidationWarning {
                field: "test_commands".to_string(),
                message: "no test_commands specified, agent cannot run tests".to_string(),
                severity: Severity::Medium,
            });
        }

        // Check for conventions with empty descriptions
        for (i, conv) in self.conventions.iter().enumerate() {
            if conv.description.is_empty() {
                warnings.push(ValidationWarning {
                    field: format!("conventions[{i}].description"),
                    message: "convention has empty description".to_string(),
                    severity: Severity::Low,
                });
            }
        }

        // Check for boundaries with empty descriptions
        for (i, boundary) in self.boundaries.iter().enumerate() {
            if boundary.description.is_empty() {
                warnings.push(ValidationWarning {
                    field: format!("boundaries[{i}].description"),
                    message: "boundary has empty description".to_string(),
                    severity: Severity::Low,
                });
            }
        }

        // Validate tech stack has at least one component if language is set
        if !self.language.is_empty() && self.tech_stack.is_empty() {
            warnings.push(ValidationWarning {
                field: "tech_stack".to_string(),
                message: format!("language is '{}' but tech_stack is empty", self.language),
                severity: Severity::Low,
            });
        }

        Ok(warnings)
    }

    /// Load a project descriptor from a YAML file.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read project descriptor from {}", path.display())
        })?;
        let mut descriptor = Self::parse(&content)?;
        descriptor.loaded_from = Some(path.to_path_buf());
        Ok(descriptor)
    }

    /// Save the project descriptor to a YAML file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let yaml = self.serialize()?;
        // Atomic write via temp file
        let tmp_path = path.with_extension("yaml.tmp");
        std::fs::write(&tmp_path, &yaml).with_context(|| {
            format!(
                "Failed to write project descriptor to {}",
                tmp_path.display()
            )
        })?;
        std::fs::rename(&tmp_path, path).with_context(|| {
            format!("Failed to rename project descriptor to {}", path.display())
        })?;
        Ok(())
    }

    /// Discover domain.yaml by searching in the given directory.
    ///
    /// Search order:
    /// 1. `<dir>/.rustycode/domain.yaml`
    /// 2. `<dir>/domain.yaml`
    ///
    /// Returns `Ok(None)` if no domain file is found.
    pub fn discover(dir: &Path) -> Result<Option<PathBuf>> {
        let candidates = [
            dir.join(".rustycode").join("domain.yaml"),
            dir.join("domain.yaml"),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                return Ok(Some(candidate.clone()));
            }
        }

        Ok(None)
    }

    /// Generate a minimal default descriptor for a given project path.
    pub fn default_for_path(project_dir: &Path) -> Self {
        let project_name = project_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            project_name,
            loaded_from: None,
            ..Default::default()
        }
    }

    /// Generate a formatted prompt section from the project descriptor.
    ///
    /// Returns an empty string if the descriptor has no useful information.
    #[must_use]
    #[allow(clippy::format_push_string)]
    pub fn to_prompt_section(&self) -> String {
        let mut sections = Vec::new();

        if !self.project_name.is_empty() || !self.language.is_empty() {
            let mut header = String::from("## Project Domain\n");
            if !self.project_name.is_empty() {
                header.push_str(&format!("**Project**: {}\n", self.project_name));
            }
            if !self.language.is_empty() {
                header.push_str(&format!("**Language**: {}\n", self.language));
            }
            sections.push(header);
        }

        if !self.tech_stack.is_empty() {
            let mut s = String::from("### Tech Stack\n");
            for comp in &self.tech_stack {
                let version_str = comp
                    .version
                    .as_ref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default();
                s.push_str(&format!("- {}{version_str}\n", comp.name));
            }
            sections.push(s);
        }

        if !self.build_commands.is_empty() {
            let mut s = String::from("### Build Commands\n");
            for cmd in &self.build_commands {
                s.push_str(&format!("- `{cmd}`\n"));
            }
            sections.push(s);
        }

        if !self.test_commands.is_empty() {
            let mut s = String::from("### Test Commands\n");
            for cmd in &self.test_commands {
                s.push_str(&format!("- `{cmd}`\n"));
            }
            sections.push(s);
        }

        if !self.architecture_rules.is_empty() {
            let mut s = String::from("### Architecture Rules\n");
            for rule in &self.architecture_rules {
                s.push_str(&format!("- {rule}\n"));
            }
            sections.push(s);
        }

        if !self.preferred_patterns.is_empty() {
            let mut s = String::from("### Preferred Patterns\n");
            for pattern in &self.preferred_patterns {
                s.push_str(&format!("- {pattern}\n"));
            }
            sections.push(s);
        }

        if !self.conventions.is_empty() {
            let mut s = String::from("### Conventions\n");
            for conv in &self.conventions {
                s.push_str(&format!("- [{}] {}\n", conv.severity, conv.description));
            }
            sections.push(s);
        }

        if !self.boundaries.is_empty() {
            let mut s = String::from("### Boundaries\n");
            for boundary in &self.boundaries {
                s.push_str(&format!(
                    "- {} ({})\n",
                    boundary.description, boundary.constrained
                ));
            }
            sections.push(s);
        }

        if let Some(ref strategy) = self.test_strategy {
            sections.push(format!("### Test Strategy\n{strategy}\n"));
        }

        if sections.is_empty() {
            String::new()
        } else {
            sections.join("\n")
        }
    }
}

/// Severity level for validation warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

/// A validation warning from project descriptor validation.
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    /// Field that triggered the warning.
    pub field: String,
    /// Human-readable warning message.
    pub message: String,
    /// Severity level.
    pub severity: Severity,
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.field, self.message)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // --- Parsing tests ---

    #[test]
    fn parse_minimal_descriptor() {
        let yaml = r"
project_name: rustycode
language: rust
";
        let desc = ProjectDescriptor::parse(yaml).unwrap();
        assert_eq!(desc.project_name, "rustycode");
        assert_eq!(desc.language, "rust");
        assert!(desc.build_commands.is_empty());
        assert!(desc.test_commands.is_empty());
        assert!(desc.tech_stack.is_empty());
        assert!(desc.conventions.is_empty());
        assert!(desc.boundaries.is_empty());
    }

    #[test]
    fn parse_full_descriptor() {
        let yaml = r#"
project_name: my-api
language: typescript
tech_stack:
  - name: react
    version: "18.x"
    category: framework
  - name: express
    version: "4.x"
    category: framework
build_commands:
  - npm run build
  - npm run lint
test_commands:
  - npm test
  - npm run e2e
architecture_rules:
  - "Controllers must not contain business logic"
  - "All database access through repository layer"
preferred_patterns:
  - repository-pattern
  - service-layer
conventions:
  - description: "Use camelCase for variables"
    category: naming
    severity: required
  - description: "All exports must be named"
    severity: preferred
boundaries:
  - description: "No direct database calls in route handlers"
    constrained: "route handlers"
    boundary_type: architectural
test_strategy: jest-with-coverage
lint_config:
  name: eslint
  config_file: .eslintrc.json
formatter_config:
  name: prettier
  config_file: .prettierrc
autonomy_default: L2
autonomy_overrides:
  code_review: L3
  database_migration: L0
"#;
        let desc = ProjectDescriptor::parse(yaml).unwrap();
        assert_eq!(desc.project_name, "my-api");
        assert_eq!(desc.language, "typescript");
        assert_eq!(desc.tech_stack.len(), 2);
        assert_eq!(desc.tech_stack[0].name, "react");
        assert_eq!(desc.tech_stack[0].version.as_ref().unwrap(), "18.x");
        assert_eq!(desc.build_commands.len(), 2);
        assert_eq!(desc.test_commands.len(), 2);
        assert_eq!(desc.architecture_rules.len(), 2);
        assert_eq!(desc.preferred_patterns.len(), 2);
        assert_eq!(desc.conventions.len(), 2);
        assert_eq!(desc.boundaries.len(), 1);
        assert_eq!(desc.test_strategy.as_deref(), Some("jest-with-coverage"));
        assert_eq!(desc.lint_config.as_ref().unwrap().name, "eslint");
        assert_eq!(desc.formatter_config.as_ref().unwrap().name, "prettier");
        assert_eq!(desc.autonomy_default, crate::autonomy::AutonomyLevel::L2);
        assert_eq!(
            desc.autonomy_overrides.get("code_review"),
            Some(&crate::autonomy::AutonomyLevel::L3)
        );
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        let result = ProjectDescriptor::parse("invalid: [yaml: content");
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_yaml_returns_default() {
        let desc = ProjectDescriptor::parse("").unwrap();
        assert!(desc.project_name.is_empty());
        assert!(desc.language.is_empty());
    }

    // --- Serialization tests ---

    #[test]
    fn serialize_and_parse_roundtrip() {
        let desc = ProjectDescriptor {
            project_name: "roundtrip".to_string(),
            language: "rust".to_string(),
            tech_stack: vec![TechComponent {
                name: "tokio".to_string(),
                version: Some("1.x".to_string()),
                category: Some("runtime".to_string()),
            }],
            build_commands: vec!["cargo build".to_string()],
            test_commands: vec!["cargo test".to_string()],
            architecture_rules: vec!["No unwrap".to_string()],
            ..Default::default()
        };
        let yaml = desc.serialize().unwrap();
        let parsed = ProjectDescriptor::parse(&yaml).unwrap();
        assert_eq!(parsed.project_name, "roundtrip");
        assert_eq!(parsed.language, "rust");
        assert_eq!(parsed.tech_stack.len(), 1);
        assert_eq!(parsed.build_commands, vec!["cargo build"]);
    }

    // --- Validation tests ---

    #[test]
    fn validate_empty_descriptor_produces_warnings() {
        let desc = ProjectDescriptor::default();
        let warnings = desc.validate().unwrap();
        assert!(!warnings.is_empty());
        let has_name_warning = warnings.iter().any(|w| w.field == "project_name");
        assert!(has_name_warning);
    }

    #[test]
    fn validate_complete_descriptor_has_fewer_warnings() {
        let desc = ProjectDescriptor {
            project_name: "complete".to_string(),
            language: "rust".to_string(),
            tech_stack: vec![TechComponent {
                name: "tokio".to_string(),
                version: None,
                category: None,
            }],
            build_commands: vec!["cargo build".to_string()],
            test_commands: vec!["cargo test".to_string()],
            ..Default::default()
        };
        let warnings = desc.validate().unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_warns_on_empty_convention_descriptions() {
        let desc = ProjectDescriptor {
            project_name: "test".to_string(),
            language: "rust".to_string(),
            build_commands: vec!["cargo build".to_string()],
            test_commands: vec!["cargo test".to_string()],
            conventions: vec![Convention {
                description: String::new(),
                category: None,
                severity: "preferred".to_string(),
            }],
            ..Default::default()
        };
        let warnings = desc.validate().unwrap();
        assert!(warnings.iter().any(|w| w.field.contains("conventions")));
    }

    // --- File I/O tests ---

    #[test]
    fn load_from_file() {
        let dir = temp_dir();
        let path = dir.path().join("domain.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r"project_name: test-project
language: rust
build_commands:
  - cargo build
test_commands:
  - cargo test
"
        )
        .unwrap();

        let desc = ProjectDescriptor::load_from_file(&path).unwrap();
        assert_eq!(desc.project_name, "test-project");
        assert_eq!(desc.language, "rust");
        assert_eq!(desc.build_commands, vec!["cargo build"]);
        assert_eq!(desc.loaded_from, Some(path));
    }

    #[test]
    fn load_from_missing_file_returns_error() {
        let result = ProjectDescriptor::load_from_file(Path::new("/nonexistent/domain.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn save_and_reload() {
        let dir = temp_dir();
        let path = dir.path().join("domain.yaml");

        let desc = ProjectDescriptor {
            project_name: "save-test".to_string(),
            language: "go".to_string(),
            build_commands: vec!["go build ./...".to_string()],
            ..Default::default()
        };

        desc.save_to_file(&path).unwrap();
        assert!(path.exists());

        let loaded = ProjectDescriptor::load_from_file(&path).unwrap();
        assert_eq!(loaded.project_name, "save-test");
        assert_eq!(loaded.language, "go");
    }

    // --- Discovery tests ---

    #[test]
    fn discover_in_rustycode_dir() {
        let dir = temp_dir();
        let rustycode_dir = dir.path().join(".rustycode");
        std::fs::create_dir_all(&rustycode_dir).unwrap();
        std::fs::write(
            rustycode_dir.join("domain.yaml"),
            "project_name: discovered\nlanguage: go\n",
        )
        .unwrap();

        let path = ProjectDescriptor::discover(dir.path()).unwrap();
        assert!(path.is_some());
        let desc = ProjectDescriptor::load_from_file(&path.unwrap()).unwrap();
        assert_eq!(desc.project_name, "discovered");
        assert_eq!(desc.language, "go");
    }

    #[test]
    fn discover_returns_none_when_absent() {
        let dir = temp_dir();
        let result = ProjectDescriptor::discover(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn discover_prefers_rustycode_dir_over_root() {
        let dir = temp_dir();
        let rustycode_dir = dir.path().join(".rustycode");
        std::fs::create_dir_all(&rustycode_dir).unwrap();
        std::fs::write(
            rustycode_dir.join("domain.yaml"),
            "project_name: preferred\nlanguage: rust\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("domain.yaml"),
            "project_name: fallback\nlanguage: go\n",
        )
        .unwrap();

        let path = ProjectDescriptor::discover(dir.path()).unwrap().unwrap();
        let desc = ProjectDescriptor::load_from_file(&path).unwrap();
        assert_eq!(desc.project_name, "preferred");
    }

    // --- Default-for-path tests ---

    #[test]
    fn default_for_path_extracts_project_name() {
        let desc = ProjectDescriptor::default_for_path(Path::new("/home/user/my-project"));
        assert_eq!(desc.project_name, "my-project");
    }

    // --- Prompt section tests ---

    #[test]
    fn to_prompt_section_generates_formatted_context() {
        let desc = ProjectDescriptor {
            project_name: "my-api".to_string(),
            language: "typescript".to_string(),
            tech_stack: vec![TechComponent {
                name: "react".to_string(),
                version: Some("18.x".to_string()),
                category: Some("framework".to_string()),
            }],
            build_commands: vec!["npm run build".to_string()],
            test_commands: vec!["npm test".to_string()],
            architecture_rules: vec!["No business logic in controllers".to_string()],
            preferred_patterns: vec!["repository-pattern".to_string()],
            conventions: vec![Convention {
                description: "Use camelCase".to_string(),
                category: Some("naming".to_string()),
                severity: "required".to_string(),
            }],
            boundaries: vec![Boundary {
                description: "No direct DB calls".to_string(),
                constrained: "route handlers".to_string(),
                boundary_type: Some("architectural".to_string()),
            }],
            ..Default::default()
        };

        let section = desc.to_prompt_section();
        assert!(section.contains("my-api"));
        assert!(section.contains("typescript"));
        assert!(section.contains("react (18.x)"));
        assert!(section.contains("npm run build"));
        assert!(section.contains("npm test"));
        assert!(section.contains("No business logic in controllers"));
        assert!(section.contains("repository-pattern"));
        assert!(section.contains("Use camelCase"));
        assert!(section.contains("No direct DB calls"));
    }

    #[test]
    fn to_prompt_section_empty_returns_empty() {
        let desc = ProjectDescriptor::default();
        let section = desc.to_prompt_section();
        assert!(section.is_empty() || section.trim().is_empty());
    }

    // --- Severity tests ---

    #[test]
    fn severity_ordering() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
    }

    #[test]
    fn severity_display() {
        assert_eq!(Severity::Low.to_string(), "low");
        assert_eq!(Severity::Medium.to_string(), "medium");
        assert_eq!(Severity::High.to_string(), "high");
    }

    // --- ValidationWarning display ---

    #[test]
    fn validation_warning_display() {
        let warning = ValidationWarning {
            field: "project_name".to_string(),
            message: "is empty".to_string(),
            severity: Severity::Low,
        };
        assert_eq!(warning.to_string(), "[low] project_name: is empty");
    }

    // --- Convention default severity ---

    #[test]
    fn convention_default_severity_is_preferred() {
        let conv = Convention {
            description: "test".to_string(),
            category: None,
            severity: default_severity(),
        };
        assert_eq!(conv.severity, "preferred");
    }
}
