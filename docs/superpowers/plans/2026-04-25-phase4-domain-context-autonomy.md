# Phase 4: Domain Context + Autonomy Levels -- TDD Implementation Plan

**Date**: 2026-04-25
**Goal**: RustyCode understands project specifics and respects user-configured autonomy.
**Status**: 🟢 COMPLETE
**See Also**: [Generative Programmer analysis](2026-04-25-generative-programmer-real-analysis.md#phase-status-map)
**Dependencies**: Phase 1 (memory index + topic files, COMPLETE), existing permission system, prompt templates
**Target**: ~75 tests across 6 modules

---

## File Structure

```
New files:
  crates/rustycode-config/src/domain.rs              (~350 lines, 18 tests)
  crates/rustycode-tools/src/autonomy.rs             (~400 lines, 18 tests)
  crates/rustycode-tools/src/side_effects.rs         (~350 lines, 16 tests)
  crates/rustycode-memory/src/domain_topic.rs        (~200 lines, 8 tests)

Modified files:
  crates/rustycode-config/src/lib.rs                 (add pub mod domain)
  crates/rustycode-config/Cargo.toml                 (add serde_yaml dependency)
  crates/rustycode-tools/src/lib.rs                  (add pub mod autonomy, pub mod side_effects)
  crates/rustycode-prompt/src/lib.rs                 (add domain context section to system prompt)
  crates/rustycode-prompt/src/layered.rs             (add Domain layer)
  crates/rustycode-memory/src/lib.rs                 (wire domain_topic into MemoryManager)
```

---

## Implementation Status

Completed in this pass:

- `crates/rustycode-config/src/domain.rs` now provides `DomainContext` and `AutonomyLevel`.
- `crates/rustycode-prompt/src/layered.rs` now injects domain context into layered prompts when `domain.yaml` is present.
- `crates/rustycode-memory/src/domain_topic.rs` now saves domain context as a discoverable memory topic.
- `MemoryManager` now exposes helpers to persist and load the domain topic from workspace context.

Already present before this pass:

- Autonomy gating and policy resolution in `crates/rustycode-orchestration/src/autonomy.rs`.

Still open:

- Any deeper autonomy-specific hooks outside the prompt stack.

---

## Chunk 1: Domain Context Data Model and Loader (rustycode-config/src/domain.rs)

### 1.1 DomainContext struct + YAML deserialization

**File**: `crates/rustycode-config/src/domain.rs`

**RED -- Write failing tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // Test 1: parse minimal valid domain.yaml
    #[test]
    fn parse_minimal_domain_yaml() {
        let yaml = r#"
project_name: rustycode
language: rust
"#;
        let ctx: DomainContext = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ctx.project_name, "rustycode");
        assert_eq!(ctx.language, "rust");
        assert!(ctx.build_commands.is_empty());
        assert!(ctx.test_commands.is_empty());
        assert!(ctx.architecture_rules.is_empty());
        assert!(ctx.preferred_patterns.is_empty());
    }

    // Test 2: parse full domain.yaml with all fields
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

    // Test 3: default domain context has safe values
    #[test]
    fn default_domain_context_safe_values() {
        let ctx = DomainContext::default();
        assert!(ctx.project_name.is_empty());
        assert!(ctx.language.is_empty());
        assert_eq!(ctx.autonomy_default, AutonomyLevel::L1);
        assert!(ctx.autonomy_overrides.is_empty());
    }

    // Test 4: autonomy level parsing from string
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

    // Test 5: autonomy level display formatting
    #[test]
    fn autonomy_level_display() {
        assert_eq!(format!("{}", AutonomyLevel::L0), "L0 (suggest only)");
        assert_eq!(format!("{}", AutonomyLevel::L1), "L1 (ask permission)");
        assert_eq!(format!("{}", AutonomyLevel::L2), "L2 (execute, notify)");
        assert_eq!(format!("{}", AutonomyLevel::L3), "L3 (execute, notify after)");
        assert_eq!(format!("{}", AutonomyLevel::L4), "L4 (full autonomy)");
    }

    // Test 6: autonomy level ordering
    #[test]
    fn autonomy_level_ordering() {
        assert!(AutonomyLevel::L0 < AutonomyLevel::L1);
        assert!(AutonomyLevel::L1 < AutonomyLevel::L2);
        assert!(AutonomyLevel::L2 < AutonomyLevel::L3);
        assert!(AutonomyLevel::L3 < AutonomyLevel::L4);
    }

    // Test 7: autonomy level serde roundtrip
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

    // Test 8: load domain context from file
    #[test]
    fn load_domain_context_from_file() {
        let dir = temp_dir();
        let domain_path = dir.path().join("domain.yaml");
        let mut f = std::fs::File::create(&domain_path).unwrap();
        write!(
            f,
            r#"project_name: test-project
language: rust
build_commands:
  - cargo build
test_commands:
  - cargo test
architecture_rules:
  - "No unwrap in production code"
preferred_patterns:
  - builder-pattern
"#
        )
        .unwrap();

        let ctx = DomainContext::load_from_file(&domain_path).unwrap();
        assert_eq!(ctx.project_name, "test-project");
        assert_eq!(ctx.language, "rust");
        assert_eq!(ctx.build_commands, vec!["cargo build"]);
        assert_eq!(ctx.test_commands, vec!["cargo test"]);
    }

    // Test 9: load domain context from missing file returns error
    #[test]
    fn load_domain_context_missing_file() {
        let result = DomainContext::load_from_file(std::path::Path::new("/nonexistent/domain.yaml"));
        assert!(result.is_err());
    }

    // Test 10: load domain context from invalid YAML returns error
    #[test]
    fn load_domain_context_invalid_yaml() {
        let dir = temp_dir();
        let domain_path = dir.path().join("domain.yaml");
        std::fs::write(&domain_path, "invalid: [yaml: content").unwrap();
        let result = DomainContext::load_from_file(&domain_path);
        assert!(result.is_err());
    }

    // Test 11: discover domain.yaml in .rustycode directory
    #[test]
    fn discover_domain_yaml_in_rustycode_dir() {
        let dir = temp_dir();
        let rustycode_dir = dir.path().join(".rustycode");
        std::fs::create_dir_all(&rustycode_dir).unwrap();
        std::fs::write(
            rustycode_dir.join("domain.yaml"),
            "project_name: discovered\nlanguage: go\n",
        )
        .unwrap();

        let path = DomainContext::discover(dir.path()).unwrap();
        assert!(path.is_some());
        let ctx = DomainContext::load_from_file(&path.unwrap()).unwrap();
        assert_eq!(ctx.project_name, "discovered");
        assert_eq!(ctx.language, "go");
    }

    // Test 12: discover returns None when no domain.yaml exists
    #[test]
    fn discover_returns_none_when_absent() {
        let dir = temp_dir();
        let result = DomainContext::discover(dir.path()).unwrap();
        assert!(result.is_none());
    }

    // Test 13: resolve autonomy level with fallback chain
    #[test]
    fn resolve_autonomy_level_with_overrides() {
        let mut ctx = DomainContext {
            project_name: "test".to_string(),
            language: "rust".to_string(),
            autonomy_default: AutonomyLevel::L2,
            autonomy_overrides: {
                let mut map = std::collections::HashMap::new();
                map.insert("code_review".to_string(), AutonomyLevel::L3);
                map.insert("database_migration".to_string(), AutonomyLevel::L0);
                map
            },
            ..Default::default()
        };

        assert_eq!(
            ctx.resolve_autonomy("code_review"),
            AutonomyLevel::L3
        );
        assert_eq!(
            ctx.resolve_autonomy("database_migration"),
            AutonomyLevel::L0
        );
        assert_eq!(
            ctx.resolve_autonomy("unknown_task_type"),
            AutonomyLevel::L2
        );
    }

    // Test 14: to_prompt_section generates formatted context
    #[test]
    fn to_prompt_section_generates_formatted_context() {
        let ctx = DomainContext {
            project_name: "my-api".to_string(),
            language: "typescript".to_string(),
            build_commands: vec!["npm run build".to_string()],
            test_commands: vec!["npm test".to_string()],
            architecture_rules: vec!["No business logic in controllers".to_string()],
            preferred_patterns: vec!["repository-pattern".to_string()],
            ..Default::default()
        };

        let section = ctx.to_prompt_section();
        assert!(section.contains("my-api"));
        assert!(section.contains("typescript"));
        assert!(section.contains("npm run build"));
        assert!(section.contains("npm test"));
        assert!(section.contains("No business logic in controllers"));
        assert!(section.contains("repository-pattern"));
    }

    // Test 15: to_prompt_section_empty_context_returns_empty
    #[test]
    fn to_prompt_section_empty_returns_empty() {
        let ctx = DomainContext::default();
        let section = ctx.to_prompt_section();
        assert!(section.is_empty() || section.trim().is_empty());
    }

    // Test 16: autonomy can_execute decision logic
    #[test]
    fn autonomy_can_execute_at_each_level() {
        assert!(!AutonomyLevel::L0.can_execute()); // suggest only
        assert!(!AutonomyLevel::L1.can_execute()); // ask first
        assert!(AutonomyLevel::L2.can_execute());  // execute, notify
        assert!(AutonomyLevel::L3.can_execute());  // execute, notify after
        assert!(AutonomyLevel::L4.can_execute());  // full autonomy
    }

    // Test 17: autonomy requires_notification decision logic
    #[test]
    fn autonomy_requires_notification_at_each_level() {
        assert!(!AutonomyLevel::L0.requires_notification());
        assert!(AutonomyLevel::L1.requires_notification()); // ask = notify
        assert!(AutonomyLevel::L2.requires_notification()); // notify before
        assert!(!AutonomyLevel::L3.requires_notification()); // notify after (deferred)
        assert!(!AutonomyLevel::L4.requires_notification()); // silent
    }

    // Test 18: autonomy requires_pre_approval decision logic
    #[test]
    fn autonomy_requires_pre_approval() {
        assert!(!AutonomyLevel::L0.requires_pre_approval()); // never executes
        assert!(AutonomyLevel::L1.requires_pre_approval());  // ask first
        assert!(!AutonomyLevel::L2.requires_pre_approval()); // just notify
        assert!(!AutonomyLevel::L3.requires_pre_approval());
        assert!(!AutonomyLevel::L4.requires_pre_approval());
    }
}
```

**GREEN -- Write minimal implementation**:

```rust
//! Domain context loading and management.
//!
//! Reads project-specific domain context from `.rustycode/domain.yaml`,
//! providing architecture rules, preferred patterns, build/test commands,
//! and autonomy level configuration.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Autonomy level controlling how much freedom the agent has.
///
/// Levels escalate from suggest-only (L0) to full autonomy (L4).
/// Each level determines whether the agent can execute actions,
/// whether it must notify the user, and whether it needs pre-approval.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum AutonomyLevel {
    /// Suggest only -- no action taken, purely advisory.
    L0,
    /// Ask permission before executing any action.
    #[default]
    L1,
    /// Execute actions, notify user before proceeding.
    L2,
    /// Execute actions, notify user after completion.
    L3,
    /// Full autonomy -- no notification, for CI/CD only.
    L4,
}

impl AutonomyLevel {
    /// Whether the agent is allowed to execute actions at this level.
    #[must_use]
    pub fn can_execute(&self) -> bool {
        matches!(self, Self::L2 | Self::L3 | Self::L4)
    }

    /// Whether the user must be notified before or during execution.
    #[must_use]
    pub fn requires_notification(&self) -> bool {
        matches!(self, Self::L1 | Self::L2)
    }

    /// Whether the agent must get explicit approval before executing.
    #[must_use]
    pub fn requires_pre_approval(&self) -> bool {
        matches!(self, Self::L1)
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
        write!(f, "{label}")
    }
}

impl std::str::FromStr for AutonomyLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "L0" => Ok(Self::L0),
            "L1" => Ok(Self::L1),
            "L2" => Ok(Self::L2),
            "L3" => Ok(Self::L3),
            "L4" => Ok(Self::L4),
            other => Err(format!("invalid autonomy level: {other}")),
        }
    }
}

/// Linter or formatter tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolConfig {
    /// Tool name (e.g., "eslint", "prettier", "clippy").
    pub name: String,
    /// Path to config file (relative to project root).
    #[serde(default)]
    pub config_file: Option<String>,
}

/// Project-specific domain context loaded from `.rustycode/domain.yaml`.
///
/// Contains everything the agent needs to know about the project's
/// conventions, architecture rules, and autonomy configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainContext {
    /// Project name.
    #[serde(default)]
    pub project_name: String,

    /// Primary programming language.
    #[serde(default)]
    pub language: String,

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
    pub autonomy_default: AutonomyLevel,

    /// Per-task-type autonomy overrides.
    /// Keys are task type names (e.g., "code_review", "database_migration").
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
    /// Load domain context from a YAML file.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read domain context from {}", path.display()))?;
        let ctx: Self = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse domain context from {}", path.display()))?;
        Ok(ctx)
    }

    /// Discover domain.yaml by searching upward from the given directory.
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

    /// Resolve the effective autonomy level for a given task type.
    ///
    /// Checks per-task overrides first, then falls back to the default.
    #[must_use]
    pub fn resolve_autonomy(&self, task_type: &str) -> AutonomyLevel {
        self.autonomy_overrides
            .get(task_type)
            .copied()
            .unwrap_or(self.autonomy_default)
    }

    /// Generate a formatted prompt section from the domain context.
    ///
    /// Returns an empty string if the context has no useful information
    /// (i.e., all fields are at default values).
    #[must_use]
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

        if let Some(ref strategy) = self.test_strategy {
            sections.push(format!("### Test Strategy\n{strategy}\n"));
        }

        if !sections.is_empty() {
            sections.join("\n")
        } else {
            String::new()
        }
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-config -- domain
```

**Commit**: `feat: domain context data model and YAML loader (18 tests)`

---

## Chunk 2: Dependency Addition + Module Wiring (config crate)

### 2.1 Add serde_yaml to rustycode-config/Cargo.toml

**File**: `crates/rustycode-config/Cargo.toml`

Add `serde_yaml` to `[dependencies]`:
```toml
serde_yaml.workspace = true
```

### 2.2 Register module in rustycode-config/src/lib.rs

**File**: `crates/rustycode-config/src/lib.rs`

Add after existing `pub mod` lines:
```rust
pub mod domain;

pub use domain::{AutonomyLevel, DomainContext};
```

**Verify**:
```bash
cargo check -p rustycode-config
cargo test -p rustycode-config -- domain
```

**Commit**: `feat: wire domain context module into config crate`

---

## Chunk 3: Autonomy-Aware Permission Checking (rustycode-tools/src/autonomy.rs)

### 3.1 ControlTuning struct + autonomy-aware permission decisions

**File**: `crates/rustycode-tools/src/autonomy.rs`

**RED -- Write failing tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_config::domain::{AutonomyLevel, DomainContext};
    use std::collections::HashMap;

    fn test_domain(autonomy: AutonomyLevel) -> DomainContext {
        DomainContext {
            project_name: "test".to_string(),
            language: "rust".to_string(),
            autonomy_default: autonomy,
            autonomy_overrides: HashMap::new(),
            ..Default::default()
        }
    }

    // Test 1: control tuning with high freedom allows more operations
    #[test]
    fn high_freedom_allows_more_operations() {
        let tuning = ControlTuning::high_freedom();
        assert!(tuning.can_auto_approve_read);
        assert!(tuning.can_auto_approve_write);
        assert!(tuning.can_auto_approve_exec);
    }

    // Test 2: control tuning with low freedom restricts operations
    #[test]
    fn low_freedom_restricts_operations() {
        let tuning = ControlTuning::low_freedom();
        assert!(tuning.can_auto_approve_read);
        assert!(!tuning.can_auto_approve_write);
        assert!(!tuning.can_auto_approve_exec);
    }

    // Test 3: control tuning default is moderate
    #[test]
    fn default_control_tuning_is_moderate() {
        let tuning = ControlTuning::default();
        assert!(tuning.can_auto_approve_read);
        assert!(tuning.can_auto_approve_write);
        assert!(!tuning.can_auto_approve_exec);
    }

    // Test 4: resolve control tuning for known task types
    #[test]
    fn resolve_tuning_for_known_task_types() {
        assert_eq!(
            TaskTypeClassifier::classify("code_review"),
            TaskCategory::CodeReview
        );
        assert_eq!(
            TaskTypeClassifier::classify("database_migration"),
            TaskCategory::DatabaseMigration
        );
        assert_eq!(
            TaskTypeClassifier::classify("refactoring"),
            TaskCategory::Refactoring
        );
        assert_eq!(
            TaskTypeClassifier::classify("bug_fix"),
            TaskCategory::BugFix
        );
        assert_eq!(
            TaskTypeClassifier::classify("feature"),
            TaskCategory::FeatureImplementation
        );
        assert_eq!(
            TaskTypeClassifier::classify("deployment"),
            TaskCategory::Deployment
        );
        assert_eq!(
            TaskTypeClassifier::classify("documentation"),
            TaskCategory::Documentation
        );
    }

    // Test 5: unknown task type classifies as general
    #[test]
    fn unknown_task_classifies_as_general() {
        assert_eq!(
            TaskTypeClassifier::classify("unknown_task"),
            TaskCategory::General
        );
    }

    // Test 6: task category has appropriate control tuning
    #[test]
    fn task_category_tuning_calibration() {
        // Code review = high freedom
        let tuning = TaskCategory::CodeReview.control_tuning();
        assert!(tuning.can_auto_approve_write);

        // Database migration = low freedom
        let tuning = TaskCategory::DatabaseMigration.control_tuning();
        assert!(!tuning.can_auto_approve_write);

        // Deployment = low freedom
        let tuning = TaskCategory::Deployment.control_tuning();
        assert!(!tuning.can_auto_approve_exec);
    }

    // Test 7: autonomy decision at L0 always denies execution
    #[test]
    fn l0_always_denies_execution() {
        let domain = test_domain(AutonomyLevel::L0);
        let decider = AutonomyDecider::new(&domain);
        let decision = decider.decide("write_file", TaskCategory::FeatureImplementation);
        assert!(matches!(decision, AutonomyDecision::Blocked { .. }));
    }

    // Test 8: autonomy decision at L1 requires approval
    #[test]
    fn l1_requires_approval_for_writes() {
        let domain = test_domain(AutonomyLevel::L1);
        let decider = AutonomyDecider::new(&domain);
        let decision = decider.decide("write_file", TaskCategory::FeatureImplementation);
        assert!(matches!(decision, AutonomyDecision::RequireApproval { .. }));
    }

    // Test 9: autonomy decision at L2 executes with notification
    #[test]
    fn l2_executes_with_notification() {
        let domain = test_domain(AutonomyLevel::L2);
        let decider = AutonomyDecider::new(&domain);
        let decision = decider.decide("write_file", TaskCategory::FeatureImplementation);
        assert!(matches!(decision, AutonomyDecision::AllowWithNotification { .. }));
    }

    // Test 10: autonomy decision at L3 allows silently for high-freedom tasks
    #[test]
    fn l3_allows_for_high_freedom_tasks() {
        let domain = test_domain(AutonomyLevel::L3);
        let decider = AutonomyDecider::new(&domain);
        let decision = decider.decide("write_file", TaskCategory::CodeReview);
        assert!(matches!(decision, AutonomyDecision::Allow { .. }));
    }

    // Test 11: autonomy decision at L4 allows everything
    #[test]
    fn l4_allows_everything() {
        let domain = test_domain(AutonomyLevel::L4);
        let decider = AutonomyDecider::new(&domain);
        let decision = decider.decide("bash", TaskCategory::Deployment);
        assert!(matches!(decision, AutonomyDecision::Allow { .. }));
    }

    // Test 12: read operations always allowed regardless of level
    #[test]
    fn read_operations_always_allowed() {
        let domain = test_domain(AutonomyLevel::L0);
        let decider = AutonomyDecider::new(&domain);
        let decision = decider.decide("read_file", TaskCategory::General);
        assert!(matches!(decision, AutonomyDecision::Allow { .. }));
    }

    // Test 13: autonomy override for specific task type
    #[test]
    fn autonomy_override_applied() {
        let domain = DomainContext {
            autonomy_default: AutonomyLevel::L3,
            autonomy_overrides: {
                let mut map = HashMap::new();
                map.insert("database_migration".to_string(), AutonomyLevel::L0);
                map
            },
            ..DomainContext::default()
        };
        let decider = AutonomyDecider::new(&domain);
        let decision = decider.decide("write_file", TaskCategory::DatabaseMigration);
        assert!(matches!(decision, AutonomyDecision::Blocked { .. }));
    }

    // Test 14: operation classification for tools
    #[test]
    fn operation_classification() {
        assert_eq!(OperationType::from_tool("read_file"), OperationType::Read);
        assert_eq!(OperationType::from_tool("write_file"), OperationType::Write);
        assert_eq!(OperationType::from_tool("edit_file"), OperationType::Write);
        assert_eq!(OperationType::from_tool("bash"), OperationType::Execute);
        assert_eq!(OperationType::from_tool("grep"), OperationType::Read);
        assert_eq!(OperationType::from_tool("unknown"), OperationType::Unknown);
    }

    // Test 15: task category display
    #[test]
    fn task_category_display() {
        assert_eq!(TaskCategory::CodeReview.to_string(), "code_review");
        assert_eq!(TaskCategory::Deployment.to_string(), "deployment");
    }

    // Test 16: control tuning serialization roundtrip
    #[test]
    fn control_tuning_serde_roundtrip() {
        let tuning = ControlTuning::high_freedom();
        let json = serde_json::to_string(&tuning).unwrap();
        let decoded: ControlTuning = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.can_auto_approve_read, tuning.can_auto_approve_read);
        assert_eq!(decoded.can_auto_approve_write, tuning.can_auto_approve_write);
        assert_eq!(decoded.can_auto_approve_exec, tuning.can_auto_approve_exec);
    }

    // Test 17: autonomy decision is_allow check
    #[test]
    fn autonomy_decision_is_allow() {
        assert!(AutonomyDecision::Allow { reason: "test".into() }.is_allowed());
        assert!(!AutonomyDecision::Blocked { reason: "test".into() }.is_allowed());
        assert!(!AutonomyDecision::RequireApproval { reason: "test".into() }.is_allowed());
        assert!(AutonomyDecision::AllowWithNotification {
            reason: "test".into(),
            message: "msg".into(),
        }
        .is_allowed());
    }

    // Test 18: control tuning for all task categories
    #[test]
    fn all_task_categories_have_tuning() {
        for category in [
            TaskCategory::CodeReview,
            TaskCategory::DatabaseMigration,
            TaskCategory::Refactoring,
            TaskCategory::BugFix,
            TaskCategory::FeatureImplementation,
            TaskCategory::Deployment,
            TaskCategory::Documentation,
            TaskCategory::General,
        ] {
            let tuning = category.control_tuning();
            // All categories allow read
            assert!(
                tuning.can_auto_approve_read,
                "{category:?} should allow read"
            );
        }
    }
}
```

**GREEN -- Write minimal implementation**:

```rust
//! Autonomy-aware permission checking.
//!
//! Bridges domain context autonomy levels with the existing permission
//! system. Uses control tuning to calibrate agent freedom per task type.

use rustycode_config::domain::{AutonomyLevel, DomainContext};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Type of operation being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// Read-only operation (no side effects).
    Read,
    /// Write operation (modifies files).
    Write,
    /// Execute operation (runs commands).
    Execute,
    /// Unknown operation type.
    Unknown,
}

impl OperationType {
    /// Classify an operation by tool name.
    #[must_use]
    pub fn from_tool(tool_name: &str) -> Self {
        match tool_name {
            "read_file" | "list_dir" | "grep" | "glob" | "find" | "web_fetch" | "web_search"
            | "lsp_diagnostics" | "lsp_hover" | "lsp_definition" | "lsp_references"
            | "lsp_document_symbols" | "todo_read" => Self::Read,
            "write_file" | "edit_file" | "text_editor_20250728" | "search_replace"
            | "apply_patch" | "multi_edit" | "todo_write" => Self::Write,
            "bash" | "subprocess" => Self::Execute,
            _ => Self::Unknown,
        }
    }
}

/// Task category for control tuning calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    CodeReview,
    DatabaseMigration,
    Refactoring,
    BugFix,
    FeatureImplementation,
    Deployment,
    Documentation,
    General,
}

impl fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::CodeReview => "code_review",
            Self::DatabaseMigration => "database_migration",
            Self::Refactoring => "refactoring",
            Self::BugFix => "bug_fix",
            Self::FeatureImplementation => "feature",
            Self::Deployment => "deployment",
            Self::Documentation => "documentation",
            Self::General => "general",
        };
        write!(f, "{s}")
    }
}

/// Classifies task type strings into task categories.
pub struct TaskTypeClassifier;

impl TaskTypeClassifier {
    /// Map a task type string to a category.
    #[must_use]
    pub fn classify(task_type: &str) -> TaskCategory {
        match task_type {
            "code_review" | "review" | "code-review" => TaskCategory::CodeReview,
            "database_migration" | "db-migration" | "migration" => {
                TaskCategory::DatabaseMigration
            }
            "refactoring" | "refactor" => TaskCategory::Refactoring,
            "bug_fix" | "bugfix" | "fix" => TaskCategory::BugFix,
            "feature" | "implementation" | "new_feature" => TaskCategory::FeatureImplementation,
            "deployment" | "deploy" | "release" => TaskCategory::Deployment,
            "documentation" | "docs" | "readme" => TaskCategory::Documentation,
            _ => TaskCategory::General,
        }
    }
}

/// Per-task-category freedom calibration.
///
/// Controls which operation types can be auto-approved
/// without user confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlTuning {
    /// Whether read operations can be auto-approved.
    pub can_auto_approve_read: bool,
    /// Whether write operations can be auto-approved.
    pub can_auto_approve_write: bool,
    /// Whether exec operations can be auto-approved.
    pub can_auto_approve_exec: bool,
}

impl Default for ControlTuning {
    fn default() -> Self {
        Self::moderate_freedom()
    }
}

impl ControlTuning {
    /// High freedom: auto-approve everything including exec.
    #[must_use]
    pub fn high_freedom() -> Self {
        Self {
            can_auto_approve_read: true,
            can_auto_approve_write: true,
            can_auto_approve_exec: true,
        }
    }

    /// Moderate freedom: auto-approve read and write, ask for exec.
    #[must_use]
    pub fn moderate_freedom() -> Self {
        Self {
            can_auto_approve_read: true,
            can_auto_approve_write: true,
            can_auto_approve_exec: false,
        }
    }

    /// Low freedom: auto-approve read only.
    #[must_use]
    pub fn low_freedom() -> Self {
        Self {
            can_auto_approve_read: true,
            can_auto_approve_write: false,
            can_auto_approve_exec: false,
        }
    }
}

impl TaskCategory {
    /// Get the default control tuning for this task category.
    #[must_use]
    pub fn control_tuning(&self) -> ControlTuning {
        match self {
            // High-freedom tasks: the agent can be trusted to act autonomously
            Self::CodeReview | Self::Documentation | Self::Refactoring => ControlTuning::high_freedom(),
            // Moderate-freedom tasks: standard implementation work
            Self::BugFix | Self::FeatureImplementation | Self::General => {
                ControlTuning::moderate_freedom()
            }
            // Low-freedom tasks: destructive or irreversible operations
            Self::DatabaseMigration | Self::Deployment => ControlTuning::low_freedom(),
        }
    }
}

/// The autonomy decision for a tool invocation.
#[derive(Debug, Clone, PartialEq)]
pub enum AutonomyDecision {
    /// Operation is allowed without notification.
    Allow {
        reason: String,
    },
    /// Operation is allowed but user must be notified.
    AllowWithNotification {
        reason: String,
        message: String,
    },
    /// Operation requires explicit user approval before proceeding.
    RequireApproval {
        reason: String,
    },
    /// Operation is blocked at this autonomy level.
    Blocked {
        reason: String,
    },
}

impl AutonomyDecision {
    /// Whether the decision allows the operation (with or without notification).
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            Self::Allow { .. } | Self::AllowWithNotification { .. }
        )
    }
}

/// Bridges domain context autonomy levels with tool permission decisions.
pub struct AutonomyDecider<'a> {
    domain: &'a DomainContext,
}

impl<'a> AutonomyDecider<'a> {
    /// Create a new decider for the given domain context.
    pub fn new(domain: &'a DomainContext) -> Self {
        Self { domain }
    }

    /// Decide whether a tool invocation is allowed for the given task category.
    pub fn decide(&self, tool_name: &str, task_category: TaskCategory) -> AutonomyDecision {
        let op_type = OperationType::from_tool(tool_name);

        // Read operations are always allowed regardless of autonomy level
        if op_type == OperationType::Read {
            return AutonomyDecision::Allow {
                reason: "read operation".to_string(),
            };
        }

        let task_type_str = task_category.to_string();
        let level = self.domain.resolve_autonomy(&task_type_str);
        let tuning = task_category.control_tuning();

        match level {
            AutonomyLevel::L0 => AutonomyDecision::Blocked {
                reason: format!("blocked at {level}: suggest-only mode"),
            },
            AutonomyLevel::L1 => {
                if op_type == OperationType::Write && !tuning.can_auto_approve_write {
                    return AutonomyDecision::RequireApproval {
                        reason: format!("write requires approval at {level}"),
                    };
                }
                if op_type == OperationType::Execute && !tuning.can_auto_approve_exec {
                    return AutonomyDecision::RequireApproval {
                        reason: format!("exec requires approval at {level}"),
                    };
                }
                AutonomyDecision::RequireApproval {
                    reason: format!("requires approval at {level}"),
                }
            }
            AutonomyLevel::L2 => {
                if op_type == OperationType::Write && tuning.can_auto_approve_write {
                    AutonomyDecision::AllowWithNotification {
                        reason: "write allowed with notification".to_string(),
                        message: format!("Executing {tool_name} for {task_category}"),
                    }
                } else if op_type == OperationType::Execute && tuning.can_auto_approve_exec {
                    AutonomyDecision::AllowWithNotification {
                        reason: "exec allowed with notification".to_string(),
                        message: format!("Executing {tool_name} for {task_category}"),
                    }
                } else {
                    AutonomyDecision::RequireApproval {
                        reason: format!("{op_type:?} requires approval at {level} for {task_category}"),
                    }
                }
            }
            AutonomyLevel::L3 => {
                if (op_type == OperationType::Write && tuning.can_auto_approve_write)
                    || (op_type == OperationType::Execute && tuning.can_auto_approve_exec)
                {
                    AutonomyDecision::Allow {
                        reason: format!("auto-approved at {level} for {task_category}"),
                    }
                } else {
                    AutonomyDecision::RequireApproval {
                        reason: format!("{op_type:?} not auto-approved for {task_category} at {level}"),
                    }
                }
            }
            AutonomyLevel::L4 => AutonomyDecision::Allow {
                reason: format!("full autonomy ({level})"),
            },
        }
    }
}
```

**Verify**:
```bash
cargo check -p rustycode-tools
cargo test -p rustycode-tools -- autonomy
```

**Commit**: `feat: autonomy-aware permission checking with control tuning (18 tests)`

---

## Chunk 4: Side-Effect Ledger (rustycode-tools/src/side_effects.rs)

### 4.1 Side-effect tracking for crash recovery

**File**: `crates/rustycode-tools/src/side_effects.rs`

**RED -- Write failing tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test 1: new ledger is empty
    #[test]
    fn new_ledger_is_empty() {
        let ledger = SideEffectLedger::new();
        assert!(ledger.is_empty());
        assert_eq!(ledger.len(), 0);
    }

    // Test 2: record a side effect
    #[test]
    fn record_side_effect() {
        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "/tmp/test.rs".to_string(),
            description: "Created test module".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        assert!(!id.is_empty());
        assert_eq!(ledger.len(), 1);
    }

    // Test 3: check if effect is completed
    #[test]
    fn check_effect_completed() {
        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "/tmp/test.rs".to_string(),
            description: "Created test module".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        assert!(!ledger.is_completed(&id));

        ledger.mark_completed(&id);
        assert!(ledger.is_completed(&id));
    }

    // Test 4: skip already-completed side effects
    #[test]
    fn skip_completed_side_effects() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "a.rs".to_string(),
            description: "Write A".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        let id2 = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "b.rs".to_string(),
            description: "Write B".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        let id3 = ledger.record(SideEffect {
            tool_name: "bash".to_string(),
            target: "cargo test".to_string(),
            description: "Run tests".to_string(),
            side_effect_type: SideEffectType::CommandExecution,
            is_reversible: false,
        });

        // Mark first and third as completed
        ledger.mark_completed(&id1);
        ledger.mark_completed(&id3);

        // Only id2 should be pending
        let pending: Vec<_> = ledger.pending_effects().collect();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id2);
    }

    // Test 5: rollback reversible effects
    #[test]
    fn rollback_reversible_effects() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "new_file.rs".to_string(),
            description: "Created new file".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        let id2 = ledger.record(SideEffect {
            tool_name: "bash".to_string(),
            target: "cargo build".to_string(),
            description: "Built project".to_string(),
            side_effect_type: SideEffectType::CommandExecution,
            is_reversible: false,
        });

        let reversible = ledger.reversible_effects();
        assert_eq!(reversible.len(), 1);
        assert_eq!(reversible[0].id, id1);

        let irreversible = ledger.irreversible_effects();
        assert_eq!(irreversible.len(), 1);
        assert_eq!(irreversible[0].id, id2);
    }

    // Test 6: serialize and deserialize ledger
    #[test]
    fn ledger_serde_roundtrip() {
        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            tool_name: "edit_file".to_string(),
            target: "src/main.rs".to_string(),
            description: "Fixed bug".to_string(),
            side_effect_type: SideEffectType::FileEdit,
            is_reversible: true,
        });
        ledger.mark_completed(&id);

        let json = serde_json::to_string(&ledger).unwrap();
        let decoded: SideEffectLedger = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(decoded.is_completed(&id));
    }

    // Test 7: save and load ledger to file
    #[test]
    fn save_and_load_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("side_effects.json");

        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "mod.rs".to_string(),
            description: "Created module".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        ledger.mark_completed(&id);

        ledger.save_to_file(&path).unwrap();

        let loaded = SideEffectLedger::load_from_file(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.is_completed(&id));
    }

    // Test 8: load from missing file returns empty ledger
    #[test]
    fn load_missing_file_returns_empty() {
        let ledger =
            SideEffectLedger::load_from_file(std::path::Path::new("/nonexistent/effects.json"))
                .unwrap();
        assert!(ledger.is_empty());
    }

    // Test 9: side effect type display
    #[test]
    fn side_effect_type_display() {
        assert_eq!(SideEffectType::FileWrite.to_string(), "file_write");
        assert_eq!(SideEffectType::FileEdit.to_string(), "file_edit");
        assert_eq!(SideEffectType::FileDelete.to_string(), "file_delete");
        assert_eq!(SideEffectType::CommandExecution.to_string(), "command_execution");
        assert_eq!(SideEffectType::DatabaseChange.to_string(), "database_change");
        assert_eq!(SideEffectType::NetworkCall.to_string(), "network_call");
    }

    // Test 10: clear completed effects
    #[test]
    fn clear_completed_effects() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "a.rs".to_string(),
            description: "Write A".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        let id2 = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "b.rs".to_string(),
            description: "Write B".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });

        ledger.mark_completed(&id1);
        ledger.clear_completed();

        assert_eq!(ledger.len(), 1);
        assert!(!ledger.is_completed(&id2));
    }

    // Test 11: get effects by type
    #[test]
    fn get_effects_by_type() {
        let mut ledger = SideEffectLedger::new();
        ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "a.rs".to_string(),
            description: "Write A".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        ledger.record(SideEffect {
            tool_name: "bash".to_string(),
            target: "cargo test".to_string(),
            description: "Run tests".to_string(),
            side_effect_type: SideEffectType::CommandExecution,
            is_reversible: false,
        });
        ledger.record(SideEffect {
            tool_name: "edit_file".to_string(),
            target: "b.rs".to_string(),
            description: "Edit B".to_string(),
            side_effect_type: SideEffectType::FileEdit,
            is_reversible: true,
        });

        let file_ops = ledger.effects_by_type(SideEffectType::FileWrite);
        assert_eq!(file_ops.len(), 1);

        let exec_ops = ledger.effects_by_type(SideEffectType::CommandExecution);
        assert_eq!(exec_ops.len(), 1);
    }

    // Test 12: effect with timestamp
    #[test]
    fn effect_has_timestamp() {
        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "test.rs".to_string(),
            description: "Test".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });

        let effect = ledger.get(&id).unwrap();
        assert!(effect.timestamp > 0);
    }

    // Test 13: mark_completed on nonexistent id is a no-op
    #[test]
    fn mark_completed_nonexistent_is_noop() {
        let mut ledger = SideEffectLedger::new();
        ledger.mark_completed("nonexistent-id");
        assert!(ledger.is_empty());
    }

    // Test 14: recovery check returns uncompleted effects
    #[test]
    fn recovery_check_returns_uncompleted() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "a.rs".to_string(),
            description: "Write A".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        ledger.record(SideEffect {
            tool_name: "write_file".to_string(),
            target: "b.rs".to_string(),
            description: "Write B".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
        });
        ledger.mark_completed(&id1);

        let recovery = ledger.recovery_check();
        assert_eq!(recovery.pending_count, 1);
        assert_eq!(recovery.completed_count, 1);
        assert_eq!(recovery.total_count, 2);
    }

    // Test 15: empty ledger recovery check
    #[test]
    fn empty_ledger_recovery_check() {
        let ledger = SideEffectLedger::new();
        let recovery = ledger.recovery_check();
        assert_eq!(recovery.pending_count, 0);
        assert_eq!(recovery.completed_count, 0);
        assert!(recovery.is_clean());
    }

    // Test 16: side effect status enum
    #[test]
    fn side_effect_status() {
        let mut effect = SideEffect {
            id: "test".to_string(),
            tool_name: "write_file".to_string(),
            target: "test.rs".to_string(),
            description: "Test".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 1000,
            completed_at: None,
        };
        assert!(!effect.is_complete());

        effect.complete();
        assert!(effect.is_complete());
        assert!(effect.completed_at.is_some());
    }
}
```

**GREEN -- Write minimal implementation**:

```rust
//! Side-effect ledger for crash recovery.
//!
//! Tracks every state-mutating action (file writes, command executions,
//! database changes) so that after a crash, the agent can skip
//! already-completed side effects and avoid duplicate operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Type of side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectType {
    FileWrite,
    FileEdit,
    FileDelete,
    CommandExecution,
    DatabaseChange,
    NetworkCall,
}

impl fmt::Display for SideEffectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileWrite => write!(f, "file_write"),
            Self::FileEdit => write!(f, "file_edit"),
            Self::FileDelete => write!(f, "file_delete"),
            Self::CommandExecution => write!(f, "command_execution"),
            Self::DatabaseChange => write!(f, "database_change"),
            Self::NetworkCall => write!(f, "network_call"),
        }
    }
}

/// A single tracked side effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffect {
    /// Unique identifier for this effect.
    pub id: String,
    /// Tool that caused this effect.
    pub tool_name: String,
    /// Target of the effect (file path, command, etc.).
    pub target: String,
    /// Human-readable description.
    pub description: String,
    /// Category of side effect.
    pub side_effect_type: SideEffectType,
    /// Whether this effect can be reversed.
    pub is_reversible: bool,
    /// When this effect was recorded (UNIX timestamp millis).
    pub timestamp: u64,
    /// When this effect was completed (UNIX timestamp millis), if any.
    #[serde(default)]
    pub completed_at: Option<u64>,
}

impl SideEffect {
    /// Whether this effect has been completed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.completed_at.is_some()
    }

    /// Mark this effect as completed.
    pub fn complete(&mut self) {
        self.completed_at = Some(now_millis());
    }
}

/// Summary of ledger state for recovery checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryStatus {
    /// Total number of effects.
    pub total_count: usize,
    /// Number of completed effects.
    pub completed_count: usize,
    /// Number of pending (uncompleted) effects.
    pub pending_count: usize,
}

impl RecoveryStatus {
    /// Whether the ledger is clean (no pending effects).
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.pending_count == 0
    }
}

/// Ledger that tracks side effects for crash recovery.
///
/// Thread-safe via interior mutability. Persisted to JSON for
//! recovery after crashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffectLedger {
    effects: Vec<SideEffect>,
}

impl SideEffectLedger {
    /// Create a new empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    /// Whether the ledger is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Number of tracked effects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Record a new side effect.
    ///
    /// Returns the unique ID assigned to this effect.
    pub fn record(&mut self, mut effect: SideEffect) -> String {
        let id = format!("se-{}", self.effects.len());
        effect.id = id.clone();
        effect.timestamp = now_millis();
        self.effects.push(effect);
        id
    }

    /// Mark an effect as completed by ID.
    pub fn mark_completed(&mut self, id: &str) {
        if let Some(effect) = self.effects.iter_mut().find(|e| e.id == id) {
            effect.complete();
        }
    }

    /// Whether an effect has been completed.
    #[must_use]
    pub fn is_completed(&self, id: &str) -> bool {
        self.effects
            .iter()
            .find(|e| e.id == id)
            .is_some_and(|e| e.is_complete())
    }

    /// Get a reference to an effect by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SideEffect> {
        self.effects.iter().find(|e| e.id == id)
    }

    /// Iterator over pending (uncompleted) effects.
    pub fn pending_effects(&self) -> impl Iterator<Item = &SideEffect> {
        self.effects.iter().filter(|e| !e.is_complete())
    }

    /// Get all reversible effects.
    #[must_use]
    pub fn reversible_effects(&self) -> Vec<&SideEffect> {
        self.effects.iter().filter(|e| e.is_reversible).collect()
    }

    /// Get all irreversible effects.
    #[must_use]
    pub fn irreversible_effects(&self) -> Vec<&SideEffect> {
        self.effects
            .iter()
            .filter(|e| !e.is_reversible)
            .collect()
    }

    /// Get effects filtered by type.
    #[must_use]
    pub fn effects_by_type(&self, effect_type: SideEffectType) -> Vec<&SideEffect> {
        self.effects
            .iter()
            .filter(|e| e.side_effect_type == effect_type)
            .collect()
    }

    /// Remove all completed effects from the ledger.
    pub fn clear_completed(&mut self) {
        self.effects.retain(|e| !e.is_complete());
    }

    /// Get the recovery status summary.
    #[must_use]
    pub fn recovery_check(&self) -> RecoveryStatus {
        let completed_count = self.effects.iter().filter(|e| e.is_complete()).count();
        RecoveryStatus {
            total_count: self.effects.len(),
            completed_count,
            pending_count: self.effects.len() - completed_count,
        }
    }

    /// Save the ledger to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .with_context(|| format!("Failed to serialize side effect ledger to {}", path.display()))?;

        // Atomic write
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)
            .with_context(|| format!("Failed to write side effect ledger to {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to rename side effect ledger to {}", path.display()))?;

        Ok(())
    }

    /// Load a ledger from a JSON file.
    ///
    /// Returns an empty ledger if the file does not exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read side effect ledger from {}", path.display()))?;

        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse side effect ledger from {}", path.display()))
    }
}

impl Default for SideEffectLedger {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current time as milliseconds since UNIX epoch.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
```

**Verify**:
```bash
cargo check -p rustycode-tools
cargo test -p rustycode-tools -- side_effects
```

**Commit**: `feat: side-effect ledger for crash recovery (16 tests)`

---

## Chunk 5: Domain Context as Memory Topic (rustycode-memory/src/domain_topic.rs)

### 5.1 Bridge domain context into the memory topic system

**File**: `crates/rustycode-memory/src/domain_topic.rs`

**RED -- Write failing tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_config::domain::{AutonomyLevel, DomainContext};
    use std::collections::HashMap;

    // Test 1: domain context converts to topic file content
    #[test]
    fn domain_context_to_topic_content() {
        let ctx = DomainContext {
            project_name: "my-api".to_string(),
            language: "typescript".to_string(),
            build_commands: vec!["npm run build".to_string()],
            test_commands: vec!["npm test".to_string()],
            architecture_rules: vec!["No business logic in controllers".to_string()],
            preferred_patterns: vec!["repository-pattern".to_string()],
            autonomy_default: AutonomyLevel::L2,
            ..Default::default()
        };

        let content = domain_to_topic_content(&ctx);
        assert!(content.contains("my-api"));
        assert!(content.contains("typescript"));
        assert!(content.contains("npm run build"));
        assert!(content.contains("npm test"));
        assert!(content.contains("No business logic in controllers"));
        assert!(content.contains("repository-pattern"));
        assert!(content.contains("L2"));
    }

    // Test 2: domain topic has correct keywords
    #[test]
    fn domain_topic_keywords() {
        let ctx = DomainContext {
            project_name: "test".to_string(),
            language: "rust".to_string(),
            ..Default::default()
        };

        let keywords = domain_keywords(&ctx);
        assert!(keywords.contains(&"domain".to_string()));
        assert!(keywords.contains(&"project".to_string()));
        assert!(keywords.contains(&"architecture".to_string()));
        assert!(keywords.contains(&"build".to_string()));
    }

    // Test 3: empty domain context produces minimal content
    #[test]
    fn empty_domain_produces_minimal_content() {
        let ctx = DomainContext::default();
        let content = domain_to_topic_content(&ctx);
        // Should produce valid markdown even with empty context
        assert!(content.contains("# Domain Context"));
    }

    // Test 4: domain context with autonomy overrides included
    #[test]
    fn domain_topic_includes_autonomy_overrides() {
        let ctx = DomainContext {
            project_name: "test".to_string(),
            language: "rust".to_string(),
            autonomy_default: AutonomyLevel::L2,
            autonomy_overrides: {
                let mut map = HashMap::new();
                map.insert("code_review".to_string(), AutonomyLevel::L3);
                map.insert("deployment".to_string(), AutonomyLevel::L0);
                map
            },
            ..Default::default()
        };

        let content = domain_to_topic_content(&ctx);
        assert!(content.contains("code_review"));
        assert!(content.contains("L3"));
        assert!(content.contains("deployment"));
        assert!(content.contains("L0"));
    }

    // Test 5: save domain as topic file
    #[test]
    fn save_domain_as_topic_file() {
        let dir = tempfile::tempdir().unwrap();
        let topics_dir = dir.path().join("topics");
        std::fs::create_dir_all(&topics_dir).unwrap();

        let ctx = DomainContext {
            project_name: "test-project".to_string(),
            language: "go".to_string(),
            build_commands: vec!["go build ./...".to_string()],
            ..Default::default()
        };

        let path = save_domain_topic(&topics_dir, &ctx).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test-project"));
        assert!(content.contains("go build"));
    }

    // Test 6: save and reload domain topic roundtrip
    #[test]
    fn save_and_reload_domain_topic() {
        let dir = tempfile::tempdir().unwrap();
        let topics_dir = dir.path().join("topics");
        std::fs::create_dir_all(&topics_dir).unwrap();

        let ctx = DomainContext {
            project_name: "roundtrip".to_string(),
            language: "python".to_string(),
            architecture_rules: vec!["Use type hints everywhere".to_string()],
            ..Default::default()
        };

        let path = save_domain_topic(&topics_dir, &ctx).unwrap();

        // Load via TopicLoader
        let mut loader = crate::topic::TopicLoader::new(&topics_dir);
        let loaded = loader.load_by_keyword("domain").unwrap();
        assert!(loaded.is_some());
        let topic = loaded.unwrap();
        assert!(topic.content.contains("roundtrip"));
        assert!(topic.content.contains("python"));
    }

    // Test 7: domain topic file is named correctly
    #[test]
    fn domain_topic_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let topics_dir = dir.path().join("topics");
        std::fs::create_dir_all(&topics_dir).unwrap();

        let ctx = DomainContext::default();
        let path = save_domain_topic(&topics_dir, &ctx).unwrap();
        assert_eq!(
            path.file_name().unwrap(),
            std::ffi::OsStr::new("domain-context.md")
        );
    }

    // Test 8: domain topic content is valid markdown
    #[test]
    fn domain_topic_is_valid_markdown() {
        let ctx = DomainContext {
            project_name: "md-test".to_string(),
            language: "rust".to_string(),
            build_commands: vec!["cargo build".to_string()],
            test_commands: vec!["cargo test".to_string()],
            architecture_rules: vec!["Rule one".to_string(), "Rule two".to_string()],
            preferred_patterns: vec!["builder".to_string()],
            test_strategy: Some("unit + integration".to_string()),
            ..Default::default()
        };

        let content = domain_to_topic_content(&ctx);
        // Should have markdown headers
        assert!(content.contains("# "));
        // Should have list items
        assert!(content.contains("- "));
    }
}
```

**GREEN -- Write minimal implementation**:

```rust
//! Domain context as a memory topic file.
//!
//! Bridges the domain context from rustycode-config into the memory
//! system's topic file format, so domain knowledge is discoverable
//! via the existing TopicLoader keyword search.

use anyhow::{Context, Result};
use rustycode_config::domain::DomainContext;
use std::fs;
use std::path::Path;

/// File name for the domain context topic.
const DOMAIN_TOPIC_FILENAME: &str = "domain-context.md";

/// Convert a DomainContext into topic file markdown content.
#[must_use]
pub fn domain_to_topic_content(ctx: &DomainContext) -> String {
    let mut content = String::with_capacity(2048);

    // Keywords header for TopicLoader discovery
    content.push_str("<!-- keywords: domain, project, architecture, build, test, autonomy -->\n\n");

    content.push_str("# Domain Context\n\n");

    if !ctx.project_name.is_empty() {
        content.push_str(&format!("**Project**: {}\n\n", ctx.project_name));
    }
    if !ctx.language.is_empty() {
        content.push_str(&format!("**Language**: {}\n\n", ctx.language));
    }

    if !ctx.build_commands.is_empty() {
        content.push_str("## Build Commands\n\n");
        for cmd in &ctx.build_commands {
            content.push_str(&format!("- `{cmd}`\n"));
        }
        content.push('\n');
    }

    if !ctx.test_commands.is_empty() {
        content.push_str("## Test Commands\n\n");
        for cmd in &ctx.test_commands {
            content.push_str(&format!("- `{cmd}`\n"));
        }
        content.push('\n');
    }

    if !ctx.architecture_rules.is_empty() {
        content.push_str("## Architecture Rules\n\n");
        for rule in &ctx.architecture_rules {
            content.push_str(&format!("- {rule}\n"));
        }
        content.push('\n');
    }

    if !ctx.preferred_patterns.is_empty() {
        content.push_str("## Preferred Patterns\n\n");
        for pattern in &ctx.preferred_patterns {
            content.push_str(&format!("- {pattern}\n"));
        }
        content.push('\n');
    }

    if let Some(ref strategy) = ctx.test_strategy {
        content.push_str(&format!("## Test Strategy\n\n{strategy}\n\n"));
    }

    // Autonomy configuration
    content.push_str("## Autonomy Configuration\n\n");
    content.push_str(&format!("- **Default level**: {}\n", ctx.autonomy_default));

    if !ctx.autonomy_overrides.is_empty() {
        content.push_str("- **Overrides**:\n");
        let mut overrides: Vec<_> = ctx.autonomy_overrides.iter().collect();
        overrides.sort_by_key(|(k, _)| *k);
        for (task_type, level) in overrides {
            content.push_str(&format!("  - `{task_type}`: {level}\n"));
        }
    }

    content
}

/// Get standard keywords for the domain context topic.
#[must_use]
pub fn domain_keywords(_ctx: &DomainContext) -> Vec<String> {
    vec![
        "domain".to_string(),
        "project".to_string(),
        "architecture".to_string(),
        "build".to_string(),
        "test".to_string(),
        "autonomy".to_string(),
    ]
}

/// Save the domain context as a topic file in the topics directory.
///
/// Creates the topics directory if it does not exist.
/// Returns the path to the saved file.
pub fn save_domain_topic(topics_dir: &Path, ctx: &DomainContext) -> Result<std::path::PathBuf> {
    fs::create_dir_all(topics_dir)
        .with_context(|| format!("Failed to create topics directory {}", topics_dir.display()))?;

    let content = domain_to_topic_content(ctx);
    let path = topics_dir.join(DOMAIN_TOPIC_FILENAME);

    fs::write(&path, &content)
        .with_context(|| format!("Failed to write domain topic to {}", path.display()))?;

    Ok(path)
}
```

**Verify**:
```bash
cargo check -p rustycode-memory
cargo test -p rustycode-memory -- domain_topic
```

**Commit**: `feat: domain context as memory topic file (8 tests)`

---

## Chunk 6: Prompt Integration (rustycode-prompt modifications)

### 6.1 Add domain context layer to prompt builder

**File**: `crates/rustycode-prompt/src/layered.rs` (modify)

Add a new `PromptLayer` variant and a method to inject domain context.

**RED -- Write failing tests first** (in the existing test module):

```rust
#[cfg(test)]
mod tests_domain_layer {
    use super::*;

    // Test 1: prompt builder has domain layer
    #[test]
    fn prompt_builder_has_domain_layer() {
        let builder = PromptBuilder::new();
        let layers = builder.available_layers();
        assert!(layers.contains(&PromptLayer::Domain));
    }

    // Test 2: inject domain context into prompt
    #[test]
    fn inject_domain_context_into_prompt() {
        let mut builder = PromptBuilder::new();
        let domain_section = "## Project Domain\n\n**Project**: my-api\n**Language**: typescript\n";
        builder.set_domain_context(domain_section.to_string());

        let prompt = builder.build("test task").unwrap();
        assert!(prompt.contains("my-api"));
        assert!(prompt.contains("typescript"));
    }

    // Test 3: domain context layer ordering
    #[test]
    fn domain_context_layer_after_environment() {
        let mut builder = PromptBuilder::new();
        builder.set_domain_context("domain info".to_string());

        let layers = builder.active_layers();
        let domain_pos = layers.iter().position(|l| *l == PromptLayer::Domain);
        let env_pos = layers.iter().position(|l| *l == PromptLayer::Environment);
        // Domain should come after Environment
        if let (Some(d), Some(e)) = (domain_pos, env_pos) {
            assert!(d > e, "Domain layer should come after Environment layer");
        }
    }

    // Test 4: empty domain context is omitted
    #[test]
    fn empty_domain_context_omitted() {
        let mut builder = PromptBuilder::new();
        builder.set_domain_context(String::new());

        let prompt = builder.build("test task").unwrap();
        // Should not contain domain section marker
        assert!(!prompt.contains("## Project Domain"));
    }
}
```

**GREEN -- Implementation changes to layered.rs**:

Add `Domain` to `PromptLayer`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PromptLayer {
    Base,
    ModelSpecific,
    Environment,
    Domain,      // NEW
    Project,
    Local,
    Skills,
}
```

Add to `PromptBuilder`:
```rust
pub struct PromptBuilder {
    // existing fields...
    domain_context: Option<String>,  // NEW
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self {
            // existing init...
            domain_context: None,
        }
    }

    /// Set the domain context section for the prompt.
    pub fn set_domain_context(&mut self, context: String) {
        if context.is_empty() {
            self.domain_context = None;
        } else {
            self.domain_context = Some(context);
        }
    }

    /// Return available prompt layers including Domain.
    pub fn available_layers(&self) -> Vec<PromptLayer> {
        vec![
            PromptLayer::Base,
            PromptLayer::ModelSpecific,
            PromptLayer::Environment,
            PromptLayer::Domain,
            PromptLayer::Project,
            PromptLayer::Local,
            PromptLayer::Skills,
        ]
    }

    /// Return currently active layers (those with content).
    pub fn active_layers(&self) -> Vec<PromptLayer> {
        let mut layers = vec![
            PromptLayer::Base,
            PromptLayer::Environment,
        ];
        if self.domain_context.is_some() {
            layers.push(PromptLayer::Domain);
        }
        layers.push(PromptLayer::Project);
        layers
    }
}
```

**Verify**:
```bash
cargo check -p rustycode-prompt
cargo test -p rustycode-prompt -- domain
```

**Commit**: `feat: add domain context layer to prompt builder (4 tests)`

---

## Chunk 7: System Prompt Domain Injection (rustycode-prompt/src/lib.rs)

### 7.1 Add domain_context section to system prompt template

**File**: `crates/rustycode-prompt/src/lib.rs` (modify)

Add a `{{domain_context}}` section to the `system/coding_assistant` template and the `system/headless_coding_agent` template.

In the `register_built_in_templates` function, modify the existing template strings to include:

```handlebars
{{#if domain_context}}
## Domain Context

{{domain_context}}
{{/if}}
```

This goes after the `{{#if context}}` block and before the closing paragraph in each template.

**RED -- Write failing tests**:

```rust
#[cfg(test)]
mod tests_domain_injection {
    use super::*;

    // Test 1: coding assistant prompt includes domain context
    #[test]
    fn coding_assistant_includes_domain_context() {
        let manager = TemplateManager::new().unwrap();
        let mut context = context! {
            "name" => "TestBot",
            "domain_context" => "## Project Domain\n\n**Project**: my-api\n**Language**: rust\n"
        };
        context = context_with_defaults(&context);
        let result = manager.coding_assistant_prompt(&context).unwrap();
        assert!(result.contains("Domain Context"));
        assert!(result.contains("my-api"));
        assert!(result.contains("rust"));
    }

    // Test 2: coding assistant prompt without domain context
    #[test]
    fn coding_assistant_without_domain_context() {
        let manager = TemplateManager::new().unwrap();
        let context = context_with_defaults(&TemplateContext::new());
        let result = manager.coding_assistant_prompt(&context).unwrap();
        assert!(!result.contains("Domain Context"));
    }

    // Test 3: headless agent prompt includes domain context
    #[test]
    fn headless_agent_includes_domain_context() {
        let manager = TemplateManager::new().unwrap();
        let mut context = context! {
            "domain_context" => "## Build\n- cargo build\n"
        };
        context = context_with_defaults(&context);
        let result = manager.render("system/headless_coding_agent", &context).unwrap();
        assert!(result.contains("Domain Context"));
        assert!(result.contains("cargo build"));
    }
}
```

**GREEN -- Modify the template strings**:

In the `system/coding_assistant` template, add after the `{{#if context}}...{{/if}}` block:

```
{{#if domain_context}}
## Domain Context

{{domain_context}}
{{/if}}
```

Same addition in the `system/headless_coding_agent` template.

**Verify**:
```bash
cargo test -p rustycode-prompt -- domain_injection
```

**Commit**: `feat: inject domain context into system prompts (3 tests)`

---

## Chunk 8: Module Wiring for autonomy and side_effects in tools crate

### 8.1 Register modules in rustycode-tools/src/lib.rs

**File**: `crates/rustycode-tools/src/lib.rs` (modify)

Add:
```rust
pub mod autonomy;
pub mod side_effects;
```

Add `rustycode-config` path dependency if not already present (it is already present).

**Verify**:
```bash
cargo check -p rustycode-tools
cargo test -p rustycode-tools -- autonomy
cargo test -p rustycode-tools -- side_effects
```

**Commit**: `feat: wire autonomy and side_effects modules into tools crate`

---

## Chunk 9: Wire domain_topic into MemoryManager (rustycode-memory/src/lib.rs)

### 9.1 Add domain_topic module and auto-load on session start

**File**: `crates/rustycode-memory/src/lib.rs` (modify)

Add module declaration:
```rust
pub mod domain_topic;
```

Add `rustycode-config` dependency to `crates/rustycode-memory/Cargo.toml`:
```toml
rustycode-config = { path = "../rustycode-config" }
```

Add method to `MemoryManager`:
```rust
/// Load or refresh the domain context topic from a DomainContext.
/// Saves the domain as a topic file and returns it.
pub fn load_domain_context(
    &mut self,
    ctx: &rustycode_config::domain::DomainContext,
) -> Result<Option<topic::TopicFile>> {
    // Save domain as topic file
    let topics_dir = self.memory_dir.join("topics");
    domain_topic::save_domain_topic(&topics_dir, ctx)?;

    // Load it via the topic loader
    self.topic_loader.load_by_keyword("domain")
}
```

**RED -- Write failing tests**:

```rust
#[cfg(test)]
mod tests_domain_manager_integration {
    use super::*;
    use rustycode_config::domain::{AutonomyLevel, DomainContext};

    fn temp_memory_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rustycode-domain-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn memory_manager_loads_domain_context() {
        let dir = temp_memory_dir();
        let mut manager = MemoryManager::new(&dir).unwrap();

        let ctx = DomainContext {
            project_name: "integration-test".to_string(),
            language: "rust".to_string(),
            build_commands: vec!["cargo build".to_string()],
            autonomy_default: AutonomyLevel::L2,
            ..Default::default()
        };

        let topic = manager.load_domain_context(&ctx).unwrap();
        assert!(topic.is_some());
        let topic = topic.unwrap();
        assert!(topic.content.contains("integration-test"));
        assert!(topic.content.contains("cargo build"));
    }

    #[test]
    fn session_context_includes_domain_after_load() {
        let dir = temp_memory_dir();
        let mut manager = MemoryManager::new(&dir).unwrap();

        let ctx = DomainContext {
            project_name: "session-test".to_string(),
            language: "python".to_string(),
            ..Default::default()
        };

        manager.load_domain_context(&ctx).unwrap();

        // Domain topic should now be discoverable via topic loader
        let topic = manager.load_topic_by_keyword("domain").unwrap();
        assert!(topic.is_some());
        assert!(topic.unwrap().content.contains("session-test"));
    }
}
```

**Verify**:
```bash
cargo check -p rustycode-memory
cargo test -p rustycode-memory -- domain
```

**Commit**: `feat: wire domain context into memory manager (2 tests)`

---

## Chunk 10: Full Workspace Verification

### 10.1 Clippy + Test Suite

Run the full verification suite to ensure everything integrates:

```bash
# Format
cargo fmt --check

# Clippy (must be zero warnings)
cargo clippy --workspace --all-targets -- -D warnings

# All tests
cargo test --workspace
```

### 10.2 Expected test count

| Module | Tests |
|--------|-------|
| rustycode-config/src/domain.rs | 18 |
| rustycode-tools/src/autonomy.rs | 18 |
| rustycode-tools/src/side_effects.rs | 16 |
| rustycode-memory/src/domain_topic.rs | 8 |
| rustycode-prompt layered domain | 4 |
| rustycode-prompt domain injection | 3 |
| rustycode-memory domain manager | 2 |
| **Total** | **69** |

---

## Integration Guide

### How the pieces connect

```
.rustycode/domain.yaml
        |
        v
DomainContext::load_from_file()
        |
        +--> MemoryManager::load_domain_context()  --> topic file in topics/
        |         |
        |         +--> TopicLoader::load_by_keyword("domain")  --> loaded on demand
        |
        +--> AutonomyDecider::new(&domain)
        |         |
        |         +--> decide(tool_name, task_category)  --> AutonomyDecision
        |
        +--> PromptBuilder::set_domain_context(domain.to_prompt_section())
                  |
                  +--> system/coding_assistant template  --> LLM system prompt
```

### Integration points for existing code

1. **CLI session startup** (`rustycode-cli`):
   ```rust
   let domain_path = DomainContext::discover(project_dir)?;
   if let Some(path) = domain_path {
       let domain = DomainContext::load_from_file(&path)?;
       memory_manager.load_domain_context(&domain)?;
       prompt_builder.set_domain_context(domain.to_prompt_section());
       autonomy_decider = AutonomyDecider::new(&domain);
   }
   ```

2. **Tool execution** (`rustycode-tools` security module):
   ```rust
   let decision = autonomy_decider.decide(tool_name, task_category);
   match decision {
       AutonomyDecision::Allow { .. } => execute(),
       AutonomyDecision::AllowWithNotification { message, .. } => {
           notify_user(&message);
           execute();
       }
       AutonomyDecision::RequireApproval { reason } => ask_user(&reason),
       AutonomyDecision::Blocked { reason } => return_err(&reason),
   }
   ```

3. **Crash recovery** (`rustycode-core`):
   ```rust
   let ledger = SideEffectLedger::load_from_file(&ledger_path)?;
   let recovery = ledger.recovery_check();
   if !recovery.is_clean() {
       // Skip completed side effects, only replay pending ones
       for effect in ledger.pending_effects() {
           replay_effect(effect)?;
       }
   }
   ```

### Example domain.yaml

```yaml
# .rustycode/domain.yaml
project_name: rustycode
language: rust

build_commands:
  - cargo build --release
  - cargo clippy -- -D warnings

test_commands:
  - cargo test --workspace
  - cargo test --workspace -- --ignored  # slow integration tests

architecture_rules:
  - Use anyhow for application code, thiserror for library error types
  - Never use unwrap() in production code
  - All async operations use tokio
  - Shared state uses Arc<Mutex<T>> or Arc<RwLock<T>>
  - Types for cross-crate communication go in rustycode-protocol

preferred_patterns:
  - builder-pattern
  - newtype-pattern
  - enum-state-machine

test_strategy: "Unit tests in #[cfg(test)] modules, integration tests in tests/ directory"

lint_config:
  name: clippy
  config_file: Cargo.toml  # lints section

formatter_config:
  name: rustfmt
  config_file: rustfmt.toml

autonomy_default: L2

autonomy_overrides:
  code_review: L3
  refactoring: L3
  bug_fix: L2
  feature: L2
  database_migration: L0
  deployment: L1
  documentation: L4
```

---

## Next Actions

1. **Chunk 1-2**: Implement domain context data model (2-3 hours)
2. **Chunk 3**: Implement autonomy-aware permissions (2-3 hours)
3. **Chunk 4**: Implement side-effect ledger (2-3 hours)
4. **Chunk 5**: Implement domain-to-topic bridge (1-2 hours)
5. **Chunk 6-7**: Integrate into prompt system (2-3 hours)
6. **Chunk 8-9**: Module wiring (1 hour)
7. **Chunk 10**: Full workspace verification (1 hour)
8. **Follow-up**: Wire into CLI session startup (separate PR, depends on this plan)
9. **Follow-up**: Wire into crash recovery path (separate PR)
10. **Follow-up**: TUI display of autonomy level and domain context status
