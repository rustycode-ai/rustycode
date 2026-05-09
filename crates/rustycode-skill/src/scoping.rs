use crate::types::SkillDefinition;
use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillToolScope {
    allowed: Option<HashSet<String>>,
}

impl SkillToolScope {
    pub fn allowed(tools: Vec<String>) -> Self {
        Self {
            allowed: Some(tools.into_iter().collect()),
        }
    }

    pub const fn unrestricted() -> Self {
        Self { allowed: None }
    }

    pub const fn is_unrestricted(&self) -> bool {
        self.allowed.is_none()
    }

    pub fn is_allowed(&self, tool: &str) -> bool {
        self.allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(tool))
    }

    pub fn allowed_tools(&self) -> Option<Vec<String>> {
        self.allowed
            .as_ref()
            .map(|allowed| allowed.iter().cloned().collect())
    }

    pub fn intersect(mut self, other: &Self) -> Self {
        match (&mut self.allowed, &other.allowed) {
            (None, Some(other_allowed)) => {
                self.allowed = Some(other_allowed.clone());
            }
            (Some(allowed), Some(other_allowed)) => {
                allowed.retain(|tool| other_allowed.contains(tool));
            }
            _ => {}
        }
        self
    }
}

#[must_use]
pub fn resolve_allowed_tools(def: &SkillDefinition) -> Vec<String> {
    if def.allowed_tools.is_empty()
        || def
            .allowed_tools
            .iter()
            .any(|tool| tool == "*" || tool == "all")
    {
        Vec::new()
    } else {
        def.allowed_tools.clone()
    }
}

#[must_use]
pub fn scope_from_definition(def: &SkillDefinition) -> SkillToolScope {
    let tools = resolve_allowed_tools(def);
    if tools.is_empty() {
        SkillToolScope::unrestricted()
    } else {
        SkillToolScope::allowed(tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ActivationMode, ActivationSpec, ExecutionContext, LifecycleState, ProcedureKind,
        SkillEffortLevel, SkillQuality, SkillSource,
    };
    use std::path::PathBuf;

    fn base_definition(allowed_tools: Vec<String>) -> SkillDefinition {
        SkillDefinition {
            id: "skill-1".to_string(),
            name: "Example".to_string(),
            description: "Example".to_string(),
            when_to_use: String::new(),
            source: SkillSource::Bundled,
            version: String::new(),
            activation: ActivationSpec {
                mode: ActivationMode::Always,
                paths: vec![],
                trigger_tools: vec![],
            },
            effort: SkillEffortLevel::default(),
            context: ExecutionContext::default(),
            procedure: None::<ProcedureKind>,
            allowed_tools,
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
            content_path: PathBuf::new(),
            content: None,
        }
    }

    #[test]
    fn scope_allows_declared_tools() {
        let scope = SkillToolScope::allowed(vec!["Read".to_string(), "Bash".to_string()]);
        assert!(scope.is_allowed("Read"));
        assert!(scope.is_allowed("Bash"));
        assert!(!scope.is_allowed("WebFetch"));
    }

    #[test]
    fn unrestricted_scope_allows_everything() {
        let scope = SkillToolScope::unrestricted();
        assert!(scope.is_unrestricted());
        assert!(scope.is_allowed("anything"));
        assert!(scope.allowed_tools().is_none());
    }

    #[test]
    fn resolve_allowed_tools_from_skill_definition() {
        let def = base_definition(vec!["Read".to_string(), "Bash".to_string()]);
        let tools = resolve_allowed_tools(&def);
        assert_eq!(tools, vec!["Read".to_string(), "Bash".to_string()]);
    }

    #[test]
    fn resolve_allowed_tools_empty_means_all() {
        let def = base_definition(vec![]);
        let tools = resolve_allowed_tools(&def);
        assert!(tools.is_empty());
        let scope = scope_from_definition(&def);
        assert!(scope.is_unrestricted());
    }

    #[test]
    fn resolve_allowed_tools_wildcard() {
        let def = base_definition(vec!["*".to_string()]);
        let tools = resolve_allowed_tools(&def);
        assert!(tools.is_empty());
        assert!(scope_from_definition(&def).is_unrestricted());
    }

    #[test]
    fn scope_intersection_filters_tools() {
        let scope1 = SkillToolScope::allowed(vec!["Read".to_string(), "Bash".to_string()]);
        let scope2 = SkillToolScope::allowed(vec!["Bash".to_string(), "Grep".to_string()]);
        let merged = scope1.intersect(&scope2);
        assert!(merged.is_allowed("Bash"));
        assert!(!merged.is_allowed("Read"));
        assert!(!merged.is_allowed("Grep"));
    }
}
