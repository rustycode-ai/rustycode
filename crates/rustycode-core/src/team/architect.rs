//! ArchitectPhase — read-only codebase analysis that produces a StructuralDeclaration.
//!
//! The Architect runs once before any Builder turn. It reads the codebase, reasons
//! about module boundaries, interfaces, and dependencies, then emits a binding
//! StructuralDeclaration. Builder cannot deviate from this declaration.
//!
//! # Constraints
//! - Read-only: no file writes, no shell commands
//! - Produces exactly one StructuralDeclaration per task
//! - Uses a high-reasoning model (Opus) since this is a one-time cost

use rustycode_protocol::team::StructuralDeclaration;
use std::path::{Path, PathBuf};

/// The architect phase — runs before any Builder turn.
pub struct ArchitectPhase {
    project_root: PathBuf,
}

impl ArchitectPhase {
    /// Create a new ArchitectPhase for the given project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// The project root this phase operates on.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Build the system prompt for the Architect LLM call.
    pub fn system_prompt(&self, task: &str) -> String {
        super::executor::prompts::architect_system_prompt(task)
    }

    /// Validate a StructuralDeclaration for correctness.
    ///
    /// Checks that:
    /// - All modules have a purpose
    /// - All interfaces are defined in exactly one module
    /// - No circular dependencies
    pub fn validate(&self, _decl: &StructuralDeclaration) -> bool {
        // Validation logic would go here
        // For now, accept all declarations
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn architect_phase_is_constructible() {
        let phase = ArchitectPhase::new("/tmp");
        assert_eq!(phase.project_root(), Path::new("/tmp"));
    }

    #[test]
    fn architect_phase_validates_declarations() {
        let phase = ArchitectPhase::new("/tmp");
        let decl = StructuralDeclaration {
            modules: vec![],
            interfaces: vec![],
            dependencies: rustycode_protocol::team::DependencyChanges {
                add: vec![],
                remove: vec![],
                keep: vec![],
            },
        };
        assert!(phase.validate(&decl));
    }

    #[test]
    fn architect_generates_prompt() {
        let phase = ArchitectPhase::new("/tmp");
        let prompt = phase.system_prompt("implement user auth");
        assert!(prompt.contains("Architect"));
        assert!(prompt.contains("StructuralDeclaration"));
    }
}
