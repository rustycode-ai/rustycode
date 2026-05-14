//! Delegated-agent contract: role, mission, and scope for sub-agent sessions.

use rustycode_protocol::agent_protocol::AgentRole;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Lightweight session-local snapshot carrying the delegated-agent contract.
///
/// Constructed once at delegation time and attached to `ThoughtFrame` for the
/// duration of the sub-agent session. Shapes nudges, tool exposure, and scope
/// enforcement.
///
/// Uses `AgentRole` from protocol directly — no separate role enum to sync.
/// `allowed_tools` is deny-by-default: only listed tools are visible/executable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBrief {
    /// Delegated semantic role — protocol's `AgentRole`.
    pub role: AgentRole,
    /// Original delegated task description.
    pub brief: String,
    /// Repo paths the agent is expected to focus on.
    pub path_scope: Vec<PathBuf>,
    /// Tool names allowed for this delegated session (deny-by-default).
    /// Computed from `TaskRole::allowed_tools()`, applied to `ToolActivationManager::set_scope()`.
    pub allowed_tools: Vec<String>,
}

impl TaskBrief {
    /// Check whether a file path falls within the assigned scope.
    ///
    /// Returns `true` when `path_scope` is empty (no restriction).
    pub fn is_in_scope(&self, file_path: &Path) -> bool {
        if self.path_scope.is_empty() {
            return true;
        }
        self.path_scope
            .iter()
            .any(|scope| file_path.starts_with(scope))
    }

    /// Short role-specific hint string for nudge generation.
    pub fn role_hint(&self) -> &'static str {
        match self.role {
            AgentRole::Researcher => "Explorer: read and map the area. Do not edit.",
            AgentRole::Builder => "Implementer: make targeted changes and verify them.",
            AgentRole::Skeptic => "Reviewer: inspect and critique. Do not edit.",
            AgentRole::Judge => "Verifier: run checks and prove correctness.",
            AgentRole::Planner => "Planner: analyze and produce a plan. Do not implement.",
            AgentRole::Scalpel => "Debugger: find the root cause and apply a minimal fix.",
            AgentRole::Worker => "Worker: execute the assigned task within scope.",
            _ => "Agent: complete the delegated mission within scope.",
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn sample_brief() -> TaskBrief {
        use rustycode_protocol::tool_names as tn;

        TaskBrief {
            role: AgentRole::Researcher,
            brief: "Investigate auth module".into(),
            path_scope: vec![PathBuf::from("src/auth")],
            allowed_tools: vec![
                tn::READ.into(),
                tn::GREP.into(),
                tn::LIST_DIR.into(),
                tn::GLOB.into(),
            ],
        }
    }

    #[test]
    fn in_scope_matches_prefix() {
        let brief = sample_brief();
        assert!(brief.is_in_scope(Path::new("src/auth/mod.rs")));
        assert!(brief.is_in_scope(Path::new("src/auth")));
    }

    #[test]
    fn out_of_scope_rejected() {
        let brief = sample_brief();
        assert!(!brief.is_in_scope(Path::new("src/payments/mod.rs")));
    }

    #[test]
    fn empty_scope_allows_all() {
        let brief = TaskBrief {
            role: AgentRole::Builder,
            brief: String::new(),
            path_scope: vec![],
            allowed_tools: vec![
                rustycode_protocol::tool_names::READ.into(),
                rustycode_protocol::tool_names::BASH.into(),
            ],
        };
        assert!(brief.is_in_scope(Path::new("any/path")));
    }

    #[test]
    fn role_hint_covers_agent_role_variants() {
        let mut brief = sample_brief();

        brief.role = AgentRole::Researcher;
        assert!(brief.role_hint().starts_with("Explorer"));

        brief.role = AgentRole::Builder;
        assert!(brief.role_hint().starts_with("Implementer"));

        brief.role = AgentRole::Judge;
        assert!(brief.role_hint().starts_with("Verifier"));

        brief.role = AgentRole::Scalpel;
        assert!(brief.role_hint().starts_with("Debugger"));

        brief.role = AgentRole::Planner;
        assert!(brief.role_hint().starts_with("Planner"));

        brief.role = AgentRole::Skeptic;
        assert!(brief.role_hint().starts_with("Reviewer"));

        brief.role = AgentRole::Worker;
        assert!(brief.role_hint().starts_with("Worker"));
    }

    #[test]
    fn serialization_round_trip() {
        use rustycode_protocol::tool_names as tn;

        let brief = sample_brief();
        let json = serde_json::to_string(&brief).expect("serialize");
        let back: TaskBrief = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.role, AgentRole::Researcher);
        assert_eq!(back.brief, "Investigate auth module");
        assert!(back.allowed_tools.contains(&tn::READ.to_string()));
        assert!(!back.allowed_tools.contains(&tn::BASH.to_string()));
    }
}
