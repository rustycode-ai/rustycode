use crate::types::SkillDefinition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdateProposal {
    pub skill_id: String,
    pub field: String,
    pub current: String,
    pub proposed: String,
    pub reason: String,
}

pub struct ImprovementResult {
    pub proposals: Vec<SkillUpdateProposal>,
}

pub struct SkillImprover {
    turn_interval: u32,
    turns_since_last: u32,
    min_corrections: u32,
    correction_count: u32,
}

impl SkillImprover {
    pub const fn new(turn_interval: u32) -> Self {
        Self {
            turn_interval,
            turns_since_last: 0,
            min_corrections: 1,
            correction_count: 0,
        }
    }

    pub const fn should_run(&self) -> bool {
        self.turns_since_last >= self.turn_interval && self.correction_count >= self.min_corrections
    }

    pub const fn record_turn(&mut self, had_correction: bool) {
        self.turns_since_last = self.turns_since_last.saturating_add(1);
        if had_correction {
            self.correction_count = self.correction_count.saturating_add(1);
        }
    }

    pub const fn reset(&mut self) {
        self.turns_since_last = 0;
        self.correction_count = 0;
    }

    pub fn analyze_corrections(
        &self,
        skill: &SkillDefinition,
        corrections: &[String],
    ) -> ImprovementResult {
        let mut proposals = Vec::new();

        for correction in corrections {
            let lower = correction.to_lowercase();

            if (lower.contains("trigger")
                || lower.contains("when to use")
                || lower.contains("activate"))
                && !skill.when_to_use.is_empty()
            {
                proposals.push(SkillUpdateProposal {
                    skill_id: skill.id.clone(),
                    field: "when_to_use".to_string(),
                    current: skill.when_to_use.clone(),
                    proposed: format!("Updated based on: {correction}"),
                    reason: "Correction suggests trigger mismatch".to_string(),
                });
            }

            if lower.contains("tool") || lower.contains("permission") || lower.contains("allowed") {
                proposals.push(SkillUpdateProposal {
                    skill_id: skill.id.clone(),
                    field: "allowed_tools".to_string(),
                    current: skill.allowed_tools.join(", "),
                    proposed: "Needs tool review based on correction".to_string(),
                    reason: "Correction suggests tool access issue".to_string(),
                });
            }

            if lower.contains("step") || lower.contains("procedure") || lower.contains("pipeline") {
                proposals.push(SkillUpdateProposal {
                    skill_id: skill.id.clone(),
                    field: "procedure".to_string(),
                    current: "current procedure".to_string(),
                    proposed: format!("Refine procedure based on: {correction}"),
                    reason: "Correction suggests procedural gap".to_string(),
                });
            }
        }

        ImprovementResult { proposals }
    }

    pub fn rewrite_preserving_frontmatter(
        &self,
        original: &str,
        _updates: &[SkillUpdateProposal],
    ) -> String {
        let (fm, body) = rustycode_protocol::frontmatter::split_frontmatter(original).map_or_else(
            || (String::new(), original.to_string()),
            |(yaml, body)| (format!("---\n{yaml}\n---"), body),
        );

        // Currently no updates modify the body; kept for future field-specific body rewrites.
        let updated_body = body;

        if fm.is_empty() {
            updated_body
        } else {
            format!("{fm}\n{updated_body}")
        }
    }

    pub const fn turns_since_last(&self) -> u32 {
        self.turns_since_last
    }

    pub const fn correction_count(&self) -> u32 {
        self.correction_count
    }
}

impl Default for SkillImprover {
    fn default() -> Self {
        Self::new(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ActivationSpec, ExecutionContext, LifecycleState, SkillEffortLevel, SkillQuality,
        SkillSource,
    };
    use std::path::PathBuf;

    fn make_skill(name: &str, when: &str) -> SkillDefinition {
        SkillDefinition {
            id: name.to_string(),
            name: name.to_string(),
            description: "Test".to_string(),
            when_to_use: when.to_string(),
            source: SkillSource::Bundled,
            version: String::new(),
            activation: ActivationSpec::always(),
            effort: SkillEffortLevel::Medium,
            context: ExecutionContext::Inline,
            procedure: None,
            allowed_tools: vec!["Bash".to_string()],
            user_invocable: true,
            model_invocable: true,
            agent: None,
            model_override: None,
            argument_hint: None,
            categories: vec![],
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default(),
            lifecycle_state: LifecycleState::Active,
            content_path: PathBuf::new(),
            content: None,
        }
    }

    #[test]
    fn new_improver_default_interval() {
        let imp = SkillImprover::default();
        assert_eq!(imp.turns_since_last(), 0);
        assert_eq!(imp.correction_count(), 0);
    }

    #[test]
    fn should_run_initially_false() {
        let imp = SkillImprover::new(5);
        assert!(!imp.should_run());
    }

    #[test]
    fn should_run_after_interval_with_corrections() {
        let mut imp = SkillImprover::new(3);
        imp.record_turn(true);
        imp.record_turn(false);
        imp.record_turn(true);
        assert!(imp.should_run());
    }

    #[test]
    fn should_not_run_without_corrections() {
        let mut imp = SkillImprover::new(2);
        imp.record_turn(false);
        imp.record_turn(false);
        assert!(!imp.should_run());
    }

    #[test]
    fn reset_clears_counters() {
        let mut imp = SkillImprover::new(1);
        imp.record_turn(true);
        imp.reset();
        assert_eq!(imp.turns_since_last(), 0);
        assert_eq!(imp.correction_count(), 0);
    }

    #[test]
    fn analyze_corrections_trigger_mismatch() {
        let imp = SkillImprover::new(1);
        let skill = make_skill("test", "Use for code review");
        let result =
            imp.analyze_corrections(&skill, &["The trigger for this skill is wrong".to_string()]);
        assert!(!result.proposals.is_empty());
        assert!(result.proposals[0].field == "when_to_use");
    }

    #[test]
    fn analyze_corrections_tool_issue() {
        let imp = SkillImprover::new(1);
        let skill = make_skill("test", "Use for testing");
        let result = imp.analyze_corrections(
            &skill,
            &["Need permission to use the bash tool".to_string()],
        );
        assert!(!result.proposals.is_empty());
        assert!(result.proposals.iter().any(|p| p.field == "allowed_tools"));
    }

    #[test]
    fn analyze_corrections_procedure_issue() {
        let imp = SkillImprover::new(1);
        let skill = make_skill("test", "Use for testing");
        let result =
            imp.analyze_corrections(&skill, &["Missing step in the procedure".to_string()]);
        assert!(!result.proposals.is_empty());
        assert!(result.proposals.iter().any(|p| p.field == "procedure"));
    }

    #[test]
    fn analyze_corrections_no_match() {
        let imp = SkillImprover::new(1);
        let skill = make_skill("test", "Use for testing");
        let result = imp.analyze_corrections(&skill, &["Just a general comment".to_string()]);
        assert!(result.proposals.is_empty());
    }

    #[test]
    fn rewrite_preserves_frontmatter() {
        let imp = SkillImprover::new(1);
        let original = "---\nname: test\neffort: high\n---\n# Test\n\nBody content.\n";
        let result = imp.rewrite_preserving_frontmatter(original, &[]);
        assert!(result.starts_with("---"));
        assert!(result.contains("name: test"));
        assert!(result.contains("effort: high"));
    }

    #[test]
    fn rewrite_no_frontmatter() {
        let imp = SkillImprover::new(1);
        let original = "# Test\n\nBody content.\n";
        let result = imp.rewrite_preserving_frontmatter(original, &[]);
        assert!(result.contains("# Test"));
        assert!(!result.starts_with("---"));
    }
}
