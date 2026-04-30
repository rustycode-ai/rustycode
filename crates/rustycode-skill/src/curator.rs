use crate::registry::SkillRegistry;
use std::collections::HashMap;

pub struct CapabilityCurator {
    signal_counts: HashMap<String, u32>,
    unmatched_log: Vec<String>,
    min_evidence: u32,
}

impl CapabilityCurator {
    pub fn new() -> Self {
        Self {
            signal_counts: HashMap::new(),
            unmatched_log: Vec::new(),
            min_evidence: 3,
        }
    }

    pub const fn with_min_evidence(mut self, min: u32) -> Self {
        self.min_evidence = min;
        self
    }

    pub fn observe_tool_execution(&mut self, tool_name: &str, tool_input: &str) {
        let signals = self.extract_signals(tool_name, tool_input);
        for signal in signals {
            *self.signal_counts.entry(signal.clone()).or_insert(0) += 1;
        }
    }

    pub fn extract_signals(&self, tool_name: &str, tool_input: &str) -> Vec<String> {
        let mut signals = Vec::new();
        signals.push(tool_name.to_lowercase());

        let input_lower = tool_input.to_lowercase();
        for keyword in &[
            "auth",
            "login",
            "deploy",
            "test",
            "review",
            "refactor",
            "debug",
            "fix",
            "implement",
            "build",
            "database",
            "api",
            "component",
            "style",
            "animation",
            "security",
            "perf",
        ] {
            if input_lower.contains(keyword) {
                signals.push(keyword.to_string());
            }
        }

        signals
    }

    pub fn detect_unmatched_signals(&self, registry: &SkillRegistry) -> Vec<String> {
        let all_skills = registry.get_all();
        let covered_signals: Vec<String> = all_skills
            .iter()
            .flat_map(|s| -> Vec<String> {
                let mut covered: Vec<String> = Vec::new();
                for word in s.description.to_lowercase().split_whitespace() {
                    if word.len() > 3 {
                        covered.push(word.to_string());
                    }
                }
                for word in s.when_to_use.to_lowercase().split_whitespace() {
                    if word.len() > 3 {
                        covered.push(word.to_string());
                    }
                }
                for cat in &s.categories {
                    covered.push(cat.to_lowercase());
                }
                covered
            })
            .collect();

        self.signal_counts
            .keys()
            .filter(|signal| {
                let sig_lower = signal.to_lowercase();
                !covered_signals
                    .iter()
                    .any(|c: &String| c.contains(&sig_lower) || sig_lower.contains(c.as_str()))
            })
            .cloned()
            .collect()
    }

    pub fn suggest_for_unmatched(&self) -> Vec<String> {
        self.signal_counts
            .iter()
            .filter(|(_, count)| **count >= self.min_evidence)
            .map(|(signal, count)| {
                format!("Signal '{signal}' seen {count} times without a matching skill")
            })
            .collect()
    }

    pub fn observe_context(&mut self, context: &str) {
        let context_lower = context.to_lowercase();
        for keyword in &[
            "auth",
            "login",
            "deploy",
            "test",
            "review",
            "refactor",
            "debug",
            "fix",
            "implement",
            "build",
            "database",
            "api",
        ] {
            if context_lower.contains(keyword) {
                *self.signal_counts.entry(keyword.to_string()).or_insert(0) += 1;
            }
        }
    }

    pub fn signal_count(&self, signal: &str) -> u32 {
        self.signal_counts.get(signal).copied().unwrap_or(0)
    }

    pub fn unmatched_log(&self) -> &[String] {
        &self.unmatched_log
    }

    pub fn reset(&mut self) {
        self.signal_counts.clear();
        self.unmatched_log.clear();
    }
}

impl Default for CapabilityCurator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ActivationSpec, ExecutionContext, LifecycleState, SkillDefinition, SkillEffortLevel,
        SkillQuality, SkillSource,
    };
    use std::path::PathBuf;

    fn make_skill(name: &str, desc: &str, categories: Vec<&str>) -> SkillDefinition {
        SkillDefinition {
            id: name.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            when_to_use: String::new(),
            source: SkillSource::Bundled,
            version: String::new(),
            activation: ActivationSpec::always(),
            effort: SkillEffortLevel::Medium,
            context: ExecutionContext::Inline,
            procedure: None,
            allowed_tools: vec![],
            user_invocable: true,
            model_invocable: true,
            agent: None,
            model_override: None,
            argument_hint: None,
            categories: categories.into_iter().map(String::from).collect(),
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default(),
            lifecycle_state: LifecycleState::Active,
            content_path: PathBuf::new(),
            content: None,
        }
    }

    #[test]
    fn new_curator_is_empty() {
        let c = CapabilityCurator::new();
        assert!(c.signal_counts.is_empty());
    }

    #[test]
    fn extract_signals_from_tool() {
        let c = CapabilityCurator::new();
        let signals = c.extract_signals("write_file", "{\"path\": \"/src/auth/login.rs\"}");
        assert!(signals.contains(&"write_file".to_string()));
        assert!(signals.contains(&"auth".to_string()));
    }

    #[test]
    fn extract_signals_no_keywords() {
        let c = CapabilityCurator::new();
        let signals = c.extract_signals("read_file", "{\"path\": \"/README.md\"}");
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0], "read_file");
    }

    #[test]
    fn observe_tool_increments_counts() {
        let mut c = CapabilityCurator::new();
        c.observe_tool_execution("bash", "npm run deploy");
        assert_eq!(c.signal_count("bash"), 1);
        assert_eq!(c.signal_count("deploy"), 1);
    }

    #[test]
    fn observe_context_increments_counts() {
        let mut c = CapabilityCurator::new();
        c.observe_context("please help me implement auth");
        assert_eq!(c.signal_count("auth"), 1);
        assert_eq!(c.signal_count("implement"), 1);
    }

    #[test]
    fn detect_unmatched_signals() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_skill(
            "code-review",
            "Reviews code quality",
            vec!["review"],
        ));

        let mut c = CapabilityCurator::new();
        c.observe_tool_execution("bash", "deploy to production");
        c.observe_tool_execution("bash", "deploy to staging");

        let unmatched = c.detect_unmatched_signals(&reg);
        assert!(unmatched.contains(&"deploy".to_string()));
    }

    #[test]
    fn suggest_for_unmatched_requires_min_evidence() {
        let c = CapabilityCurator::new().with_min_evidence(3);
        let suggestions = c.suggest_for_unmatched();
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggest_for_unmatched_with_evidence() {
        let mut c = CapabilityCurator::new().with_min_evidence(2);
        c.observe_tool_execution("bash", "run deploy");
        c.observe_tool_execution("bash", "run deploy again");

        let suggestions = c.suggest_for_unmatched();
        assert!(!suggestions.is_empty());
        let has_deploy = suggestions.iter().any(|s| s.contains("deploy"));
        assert!(
            has_deploy,
            "Expected a suggestion about deploy, got: {:?}",
            suggestions
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut c = CapabilityCurator::new();
        c.observe_tool_execution("bash", "test");
        assert!(!c.signal_counts.is_empty());
        c.reset();
        assert!(c.signal_counts.is_empty());
    }

    #[test]
    fn default_curator_works() {
        let c = CapabilityCurator::default();
        assert!(c.signal_counts.is_empty());
        assert_eq!(c.min_evidence, 3);
    }
}
