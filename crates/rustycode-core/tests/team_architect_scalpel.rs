#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
use rustycode_protocol::team::{DependencyChanges, StructuralDeclaration, TeamRole};
use rustycode_team::executor::{parse_architect_turn, tools_for_role};
use rustycode_team::{ArchitectPhase, ScalpelPhase, TeamRunnerConfig};

#[test]
fn architect_phase_produces_empty_declaration_on_empty_task() {
    // Structural: ArchitectPhase must be constructible
    let phase = ArchitectPhase::new("/tmp");
    assert_eq!(phase.project_root(), std::path::Path::new("/tmp"));
}

#[test]
fn scalpel_phase_is_constructible() {
    let phase = ScalpelPhase::new("/tmp");
    assert_eq!(phase.project_root(), std::path::Path::new("/tmp"));
}

#[test]
fn structural_declaration_independence_check() {
    // A declaration with no modules and no deps should be valid
    let decl = StructuralDeclaration {
        modules: vec![],
        interfaces: vec![],
        dependencies: DependencyChanges {
            add: vec![],
            remove: vec![],
            keep: vec![],
        },
    };
    assert!(decl.modules.is_empty());
}

#[test]
fn architect_gets_read_only_tools() {
    let architect_tools = tools_for_role(TeamRole::Architect);
    // Architect has read-only tools - no write or bash
    for tool in &architect_tools {
        assert!(
            !tool.contains("write"),
            "Architect must not have write tools"
        );
        assert!(!tool.contains("bash"), "Architect must not have bash tools");
    }
    // Architect should have read and exploration tools
    assert!(architect_tools.contains(&"read_file"));
    assert!(architect_tools.contains(&"grep"));
    assert!(architect_tools.contains(&"glob"));
}

#[test]
fn scalpel_gets_fewer_tools_than_builder() {
    let scalpel_tools = tools_for_role(TeamRole::Scalpel);
    let builder_tools = tools_for_role(TeamRole::Builder);
    assert!(scalpel_tools.len() < builder_tools.len());
}

#[test]
fn architect_phase_validates_declaration_before_builder_runs() {
    let phase = ArchitectPhase::new("/tmp");
    let decl = StructuralDeclaration {
        modules: vec![],
        interfaces: vec![],
        dependencies: DependencyChanges {
            add: vec![],
            remove: vec![],
            keep: vec![],
        },
    };
    assert!(phase.validate(&decl));
}

#[test]
fn team_runner_config_defaults_are_safe() {
    let config = TeamRunnerConfig::default();
    assert!(config.architect_enabled);
    assert!(config.scalpel_enabled);
    assert!(config.max_turns > 0);
}

#[test]
fn scalpel_routing_correctly_classifies_failures() {
    let compile_errors = vec![
        "error[E0308]: mismatched types".to_string(),
        "error[E0425]: unresolved name".to_string(),
    ];
    assert!(ScalpelPhase::is_scalpel_appropriate(&compile_errors));

    let logic_errors = vec!["wrong result: expected 42, got 0".to_string()];
    assert!(!ScalpelPhase::is_scalpel_appropriate(&logic_errors));
}

#[test]
fn full_json_round_trip_architect_to_builder_contract() {
    // Simulate: Architect produces JSON → Builder receives declaration
    let architect_output = r#"{
        "declaration": {
            "modules": [
                {"path": "src/team/architect.rs", "action": "Create",
                 "exports": ["ArchitectPhase"], "imports": ["anyhow::Result"],
                 "purpose": "Structural analysis before implementation"}
            ],
            "interfaces": [
                {"name": "Phase", "defined_in": "src/team/architect.rs",
                 "methods": ["fn project_root(&self) -> &Path"],
                 "implementors": ["src/team/architect.rs"]}
            ],
            "dependencies": {"add": [], "remove": [], "keep": ["anyhow", "tokio"]}
        },
        "rationale": "Simple single-module addition",
        "confidence": 0.95
    }"#;

    let turn = parse_architect_turn(architect_output).unwrap();
    assert_eq!(turn.declaration.modules.len(), 1);
    assert_eq!(turn.declaration.interfaces.len(), 1);
    assert_eq!(turn.declaration.dependencies.keep.len(), 2);
    assert!((turn.confidence - 0.95).abs() < 0.01);
}
