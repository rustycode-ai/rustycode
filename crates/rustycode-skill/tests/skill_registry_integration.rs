#![allow(clippy::unwrap_used)]

use rustycode_skill::registry::SkillRegistry;
use rustycode_skill::types::{ActivationMode, SkillSource};
use std::fs;

fn setup_skills() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();

    let skills = [
        ("brace-skill", "---\nname: brace-skill\ndescription: Test brace expansion\npaths:\n  - \"src/*.{ts,tsx}\"\n  - \"lib/**/*.js\"\n---\n# Brace Skill\n"),
        ("comma-skill", "---\nname: comma-skill\ndescription: Test comma paths\npaths: \"*.rs, *.toml\"\n---\n# Comma Skill\n"),
        ("nested-skill", "---\nname: nested-skill\ndescription: Nested braces\npaths:\n  - \"{src,lib}/**/*.{rs,ts}\"\n---\n# Nested Skill\n"),
        ("multi-skill", "---\nname: multi-skill\ndescription: Combined\npaths:\n  - \"src/*.{ts,tsx}, *.json\"\n  - \"**/*.rs\"\n---\n# Multi Skill\n"),
        ("always-skill", "---\nname: always-skill\ndescription: No paths\nuser-invocable: false\n---\n# Always Skill\n"),
        ("cond-skill", "---\nname: cond-skill\ndescription: Conditional\npaths:\n  - \"*.py\"\n  - \"scripts/**/*.py\"\nuser-invocable: true\n---\n# Cond Skill\n"),
    ];

    for (name, content) in &skills {
        let skill_dir = dir.path().join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    dir
}

#[test]
fn registry_loads_all_skills() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    assert_eq!(reg.active_count(), 1, "only always-skill should be active");
    assert_eq!(
        reg.conditional_count(),
        5,
        "5 path-based skills should be conditional"
    );
}

#[test]
fn brace_expansion_in_paths() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let conditional = reg.get_conditional();
    let brace = conditional.iter().find(|s| s.id == "brace-skill").unwrap();

    assert!(brace.activation.paths.contains(&"src/*.ts".to_string()));
    assert!(brace.activation.paths.contains(&"src/*.tsx".to_string()));
    assert!(brace.activation.paths.contains(&"lib/**/*.js".to_string()));
    assert_eq!(brace.activation.paths.len(), 3);
}

#[test]
fn comma_separated_paths() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let conditional = reg.get_conditional();
    let comma = conditional.iter().find(|s| s.id == "comma-skill").unwrap();

    assert!(comma.activation.paths.contains(&"*.rs".to_string()));
    assert!(comma.activation.paths.contains(&"*.toml".to_string()));
    assert_eq!(comma.activation.paths.len(), 2);
}

#[test]
fn nested_brace_expansion() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let conditional = reg.get_conditional();
    let nested = conditional.iter().find(|s| s.id == "nested-skill").unwrap();

    let expected = ["src/**/*.rs", "src/**/*.ts", "lib/**/*.rs", "lib/**/*.ts"];
    for exp in &expected {
        assert!(
            nested.activation.paths.contains(&exp.to_string()),
            "expected path '{exp}' in {:?}",
            nested.activation.paths
        );
    }
    assert_eq!(nested.activation.paths.len(), 4);
}

#[test]
fn combined_brace_and_comma() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let conditional = reg.get_conditional();
    let multi = conditional.iter().find(|s| s.id == "multi-skill").unwrap();

    assert!(multi.activation.paths.contains(&"src/*.ts".to_string()));
    assert!(multi.activation.paths.contains(&"src/*.tsx".to_string()));
    assert!(multi.activation.paths.contains(&"*.json".to_string()));
    assert!(multi.activation.paths.contains(&"**/*.rs".to_string()));
    assert_eq!(multi.activation.paths.len(), 4);
}

#[test]
fn always_active_without_paths() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let always = reg.get("always-skill").unwrap();
    assert_eq!(always.activation.mode, ActivationMode::Always);
    assert!(always.activation.paths.is_empty());
    assert_eq!(always.description, "No paths");
}

#[test]
fn promote_conditional_skill() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    assert_eq!(reg.conditional_count(), 5);

    let promoted = reg.promote_conditional("cond-skill").unwrap();
    assert_eq!(promoted.id, "cond-skill");
    assert_eq!(promoted.activation.paths.len(), 2);

    assert_eq!(reg.active_count(), 2);
    assert_eq!(reg.conditional_count(), 4);

    let skill = reg.get("cond-skill").unwrap();
    assert_eq!(skill.description, "Conditional");
}

#[test]
fn path_matching_after_normalization() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let conditional = reg.get_conditional();
    let brace = conditional.iter().find(|s| s.id == "brace-skill").unwrap();

    assert!(brace.activation.matches_path("src/app.tsx"));
    assert!(brace.activation.matches_path("src/utils.ts"));
    assert!(brace.activation.matches_path("lib/deep/nested.js"));
    assert!(!brace.activation.matches_path("src/main.rs"));
    assert!(!brace.activation.matches_path("config.toml"));
}

#[test]
fn body_content_extracted_correctly() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let brace = reg.get_conditional();
    let skill = brace.iter().find(|s| s.id == "brace-skill").unwrap();

    let content = skill.content.as_ref().unwrap();
    assert!(content.contains("# Brace Skill"));
    assert!(!content.contains("name:"));
    assert!(!content.contains("---"));
}

#[test]
fn frontmatter_fields_parsed_correctly() {
    let dir = setup_skills();
    let mut reg = SkillRegistry::new();
    reg.load_from_dir(dir.path(), SkillSource::User).unwrap();

    let cond = reg.get_conditional();
    let skill = cond.iter().find(|s| s.id == "cond-skill").unwrap();

    assert!(skill.user_invocable);
    assert_eq!(skill.activation.mode, ActivationMode::Conditional);
    assert_eq!(skill.description, "Conditional");
}
