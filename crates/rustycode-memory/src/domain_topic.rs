//! Domain context as a memory topic file.
//!
//! Bridges `rustycode-config` domain metadata into the memory topic system
//! so project-specific conventions are discoverable via `TopicLoader`.

use anyhow::{Context, Result};
use rustycode_config::domain::DomainContext;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

/// File name for the domain context topic.
pub const DOMAIN_TOPIC_FILENAME: &str = "domain-context.md";

/// Convert a `DomainContext` into topic markdown content.
#[must_use]
pub fn domain_to_topic_content(ctx: &DomainContext) -> String {
    let mut out = String::with_capacity(2048);

    out.push_str("<!-- keywords: domain, project, architecture, build, test, autonomy -->\n\n");
    out.push_str("# Domain Context\n\n");

    if !ctx.project_name.is_empty() {
        let _ = writeln!(out, "**Project**: {}", ctx.project_name);
        out.push('\n');
    }
    if !ctx.language.is_empty() {
        let _ = writeln!(out, "**Language**: {}", ctx.language);
        out.push('\n');
    }

    if !ctx.build_commands.is_empty() {
        out.push_str("## Build Commands\n\n");
        for command in &ctx.build_commands {
            let _ = writeln!(out, "- `{command}`");
        }
        out.push('\n');
    }

    if !ctx.test_commands.is_empty() {
        out.push_str("## Test Commands\n\n");
        for command in &ctx.test_commands {
            let _ = writeln!(out, "- `{command}`");
        }
        out.push('\n');
    }

    if !ctx.architecture_rules.is_empty() {
        out.push_str("## Architecture Rules\n\n");
        for rule in &ctx.architecture_rules {
            let _ = writeln!(out, "- {rule}");
        }
        out.push('\n');
    }

    if !ctx.preferred_patterns.is_empty() {
        out.push_str("## Preferred Patterns\n\n");
        for pattern in &ctx.preferred_patterns {
            let _ = writeln!(out, "- {pattern}");
        }
        out.push('\n');
    }

    if let Some(strategy) = &ctx.test_strategy {
        out.push_str("## Test Strategy\n\n");
        out.push_str(strategy);
        out.push_str("\n\n");
    }

    if let Some(lint) = &ctx.lint_config {
        out.push_str("## Lint Configuration\n\n");
        if let Some(linter) = &lint.linter {
            let _ = writeln!(out, "- **Linter**: {linter}");
        }
        if let Some(config_file) = &lint.config_file {
            let _ = writeln!(out, "- **Config**: {config_file}");
        }
        out.push('\n');
    }

    if let Some(formatter) = &ctx.formatter_config {
        out.push_str("## Formatter Configuration\n\n");
        if let Some(name) = &formatter.formatter {
            let _ = writeln!(out, "- **Formatter**: {name}");
        }
        if let Some(config_file) = &formatter.config_file {
            let _ = writeln!(out, "- **Config**: {config_file}");
        }
        out.push('\n');
    }

    out.push_str("## Autonomy Configuration\n\n");
    let _ = writeln!(out, "- **Default**: {}", ctx.autonomy_default);
    if !ctx.autonomy_overrides.is_empty() {
        out.push_str("- **Overrides**:\n");
        let mut overrides: Vec<_> = ctx.autonomy_overrides.iter().collect();
        overrides.sort_by(|a, b| a.0.cmp(b.0));
        for (task_type, level) in overrides {
            let _ = writeln!(out, "  - `{task_type}`: {level}");
        }
    }

    out
}

/// Standard keywords for the domain topic.
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
pub fn save_domain_topic(topics_dir: &Path, ctx: &DomainContext) -> Result<PathBuf> {
    fs::create_dir_all(topics_dir)
        .with_context(|| format!("Failed to create topics directory {}", topics_dir.display()))?;

    let path = topics_dir.join(DOMAIN_TOPIC_FILENAME);
    fs::write(&path, domain_to_topic_content(ctx))
        .with_context(|| format!("Failed to write domain topic to {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_config::domain::AutonomyLevel;
    use std::collections::HashMap;

    #[test]
    fn domain_context_to_topic_content() {
        let ctx = DomainContext {
            project_name: "my-api".to_string(),
            language: "typescript".to_string(),
            build_commands: vec!["npm run build".to_string()],
            test_commands: vec!["npm test".to_string()],
            architecture_rules: vec!["No business logic in controllers".to_string()],
            preferred_patterns: vec!["repository-pattern".to_string()],
            test_strategy: Some("jest".to_string()),
            lint_config: None,
            formatter_config: None,
            autonomy_default: AutonomyLevel::L2,
            autonomy_overrides: HashMap::new(),
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

    #[test]
    fn domain_topic_keywords() {
        let keywords = domain_keywords(&DomainContext::default());
        assert!(keywords.contains(&"domain".to_string()));
        assert!(keywords.contains(&"project".to_string()));
        assert!(keywords.contains(&"architecture".to_string()));
        assert!(keywords.contains(&"build".to_string()));
    }

    #[test]
    fn empty_domain_produces_minimal_content() {
        let content = domain_to_topic_content(&DomainContext::default());
        assert!(content.contains("# Domain Context"));
    }

    #[test]
    fn domain_topic_includes_autonomy_overrides() {
        let mut overrides = HashMap::new();
        overrides.insert("code_review".to_string(), AutonomyLevel::L3);
        overrides.insert("deployment".to_string(), AutonomyLevel::L0);

        let ctx = DomainContext {
            project_name: "test".to_string(),
            language: "rust".to_string(),
            build_commands: vec![],
            test_commands: vec![],
            architecture_rules: vec![],
            preferred_patterns: vec![],
            test_strategy: None,
            lint_config: None,
            formatter_config: None,
            autonomy_default: AutonomyLevel::L2,
            autonomy_overrides: overrides,
        };

        let content = domain_to_topic_content(&ctx);
        assert!(content.contains("code_review"));
        assert!(content.contains("L3"));
        assert!(content.contains("deployment"));
        assert!(content.contains("L0"));
    }

    #[test]
    fn save_domain_as_topic_file() {
        let dir = tempfile::tempdir().unwrap();
        let topics_dir = dir.path().join("topics");

        let ctx = DomainContext {
            project_name: "test-project".to_string(),
            language: "go".to_string(),
            build_commands: vec!["go build ./...".to_string()],
            test_commands: vec![],
            architecture_rules: vec![],
            preferred_patterns: vec![],
            test_strategy: None,
            lint_config: None,
            formatter_config: None,
            autonomy_default: AutonomyLevel::L1,
            autonomy_overrides: HashMap::new(),
        };

        let path = save_domain_topic(&topics_dir, &ctx).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test-project"));
        assert!(content.contains("go build"));
    }

    #[test]
    fn domain_topic_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let topics_dir = dir.path().join("topics");
        let ctx = DomainContext::default();
        let path = save_domain_topic(&topics_dir, &ctx).unwrap();
        assert_eq!(
            path.file_name().unwrap(),
            std::ffi::OsStr::new(DOMAIN_TOPIC_FILENAME)
        );
    }
}
