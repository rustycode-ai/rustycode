use crate::exclusions::ExclusionClauseSet;
use crate::registry::SkillRegistry;
use crate::types::{ActivationMode, SkillDefinition, SkillId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSkill {
    pub skill_id: SkillId,
    pub trigger: String,
    pub activated_at: DateTime<Utc>,
    pub token_budget: u32,
    pub tokens_used: u32,
    #[serde(skip)]
    pub last_accessed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecommendation {
    pub skill_id: SkillId,
    pub score: f64,
    pub activation_mode: ActivationMode,
    pub estimated_tokens: u32,
}

pub struct ActivationManager {
    active: HashMap<SkillId, ActiveSkill>,
    total_budget: u32,
    used_budget: u32,
}

impl ActivationManager {
    pub fn new(total_budget: u32) -> Self {
        Self {
            active: HashMap::new(),
            total_budget,
            used_budget: 0,
        }
    }

    pub fn evaluate_for_context(
        &self,
        registry: &SkillRegistry,
        context: &str,
    ) -> Vec<SkillRecommendation> {
        let context_lower = context.to_lowercase();
        let mut recommendations: Vec<SkillRecommendation> = Vec::new();

        for skill in registry.get_all() {
            let score = self.score_skill(skill, &context_lower);
            if score > 0.3 {
                recommendations.push(SkillRecommendation {
                    skill_id: skill.id.clone(),
                    score,
                    activation_mode: skill.activation.mode,
                    estimated_tokens: self.estimate_tokens(skill),
                });
            }
        }

        recommendations.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recommendations
    }

    pub fn activate(
        &mut self,
        registry: &mut SkillRegistry,
        skill_id: &str,
        trigger: String,
    ) -> anyhow::Result<Option<&ActiveSkill>> {
        let skill = registry
            .get(skill_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Skill not found: {skill_id}"))?;

        if self.active.contains_key(skill_id) {
            if let Some(active) = self.active.get_mut(skill_id) {
                active.last_accessed = Utc::now().timestamp();
            }
            return Ok(self.active.get(skill_id));
        }

        let token_budget = self.estimate_tokens(&skill);
        while self.used_budget + token_budget > self.total_budget && !self.active.is_empty() {
            self.evict_lowest_priority();
        }

        if self.used_budget + token_budget > self.total_budget {
            return Ok(None);
        }

        self.used_budget += token_budget;

        let active_skill = ActiveSkill {
            skill_id: skill_id.to_string(),
            trigger,
            activated_at: Utc::now(),
            token_budget,
            tokens_used: 0,
            last_accessed: Utc::now().timestamp(),
        };

        self.active.insert(skill_id.to_string(), active_skill);
        Ok(self.active.get(skill_id))
    }

    pub fn deactivate(&mut self, skill_id: &str) -> Option<ActiveSkill> {
        let removed = self.active.remove(skill_id)?;
        self.used_budget = self.used_budget.saturating_sub(removed.token_budget);
        Some(removed)
    }

    pub fn is_active(&self, skill_id: &str) -> bool {
        self.active.contains_key(skill_id)
    }

    pub fn get_active_skills(&self) -> Vec<&ActiveSkill> {
        self.active.values().collect()
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn budget_remaining(&self) -> u32 {
        self.total_budget.saturating_sub(self.used_budget)
    }

    pub fn activate_for_paths(
        &mut self,
        registry: &mut SkillRegistry,
        file_paths: &[&str],
    ) -> Vec<String> {
        let mut activated = Vec::new();

        let conditional_skills: Vec<(SkillId, Vec<String>)> = registry
            .get_conditional()
            .iter()
            .map(|s| (s.id.clone(), s.activation.paths.clone()))
            .collect();

        for (skill_id, patterns) in &conditional_skills {
            let matches_any = file_paths.iter().any(|fp| {
                patterns
                    .iter()
                    .any(|pattern| glob::Pattern::new(pattern).is_ok_and(|p| p.matches(fp)))
            });

            if matches_any && !self.active.contains_key(skill_id) {
                let skill = registry.promote_conditional(skill_id);
                if let Some(skill_def) = skill {
                    let token_budget = self.estimate_tokens(&skill_def);
                    if self.used_budget + token_budget <= self.total_budget {
                        self.used_budget += token_budget;
                        self.active.insert(
                            skill_id.clone(),
                            ActiveSkill {
                                skill_id: skill_id.clone(),
                                trigger: "conditional".to_string(),
                                activated_at: Utc::now(),
                                token_budget,
                                tokens_used: 0,
                                last_accessed: Utc::now().timestamp_millis(),
                            },
                        );
                        activated.push(skill_id.clone());
                    }
                }
            }
        }

        activated
    }

    #[allow(clippy::unused_self)]
    #[allow(clippy::unused_self)]
    fn score_skill(&self, skill: &SkillDefinition, context_lower: &str) -> f64 {
        let mut score = 0.0;

        if ExclusionClauseSet::from_list(&skill.excludes).matches_any(context_lower) {
            return 0.0;
        }

        if context_lower.contains(&skill.name.to_lowercase()) {
            score += 2.0;
        }

        for word in skill.description.to_lowercase().split_whitespace() {
            if word.len() > 3 && context_lower.contains(word) {
                score += 0.3;
            }
        }

        if !skill.when_to_use.is_empty() {
            for word in skill.when_to_use.to_lowercase().split_whitespace() {
                if word.len() > 3 && context_lower.contains(word) {
                    score += 0.2;
                }
            }
        }

        if !skill.categories.is_empty() {
            for cat in &skill.categories {
                if context_lower.contains(&cat.to_lowercase()) {
                    score += 0.5;
                }
            }
        }

        let quality_bonus = skill.quality.weighted_total() * 0.5;
        score += quality_bonus;

        score
    }

    #[allow(clippy::unused_self)]
    const fn estimate_tokens(&self, skill: &SkillDefinition) -> u32 {
        let base: u32 = match skill.effort {
            crate::types::SkillEffortLevel::Low => 500,
            crate::types::SkillEffortLevel::Medium => 1_000,
            crate::types::SkillEffortLevel::High => 2_000,
            crate::types::SkillEffortLevel::Max => 4_000,
        };
        base + (skill.description.len() as u32 / 4) + (skill.when_to_use.len() as u32 / 4)
    }

    fn evict_lowest_priority(&mut self) {
        if self.active.is_empty() {
            return;
        }

        let lowest = self
            .active
            .iter()
            .min_by_key(|(_, active)| (active.last_accessed, active.token_budget))
            .map(|(id, _)| id.clone());

        if let Some(id) = lowest {
            self.deactivate(&id);
        }
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

    fn make_skill(name: &str, desc: &str, when: &str) -> SkillDefinition {
        SkillDefinition {
            id: name.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            when_to_use: when.to_string(),
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
            categories: vec![],
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default(),
            lifecycle_state: LifecycleState::Active,
            content_path: PathBuf::new(),
            content: None,
        }
    }

    fn make_conditional_skill(name: &str, paths: Vec<&str>) -> SkillDefinition {
        SkillDefinition {
            id: name.to_string(),
            activation: ActivationSpec::conditional(paths.into_iter().map(String::from).collect()),
            lifecycle_state: LifecycleState::Latent,
            ..make_skill(name, "conditional", "")
        }
    }

    #[test]
    fn new_manager_is_empty() {
        let mgr = ActivationManager::new(10_000);
        assert_eq!(mgr.active_count(), 0);
        assert_eq!(mgr.budget_remaining(), 10_000);
    }

    #[test]
    fn activate_and_deactivate() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_skill("test-skill", "A test", ""));
        let mut mgr = ActivationManager::new(10_000);

        let result = mgr
            .activate(&mut reg, "test-skill", "manual".to_string())
            .unwrap();
        assert!(result.is_some());
        assert_eq!(mgr.active_count(), 1);
        assert!(mgr.is_active("test-skill"));

        let deactivated = mgr.deactivate("test-skill");
        assert!(deactivated.is_some());
        assert_eq!(mgr.active_count(), 0);
        assert!(!mgr.is_active("test-skill"));
    }

    #[test]
    fn deactivate_nonexistent_returns_none() {
        let mut mgr = ActivationManager::new(10_000);
        assert!(mgr.deactivate("nope").is_none());
    }

    #[test]
    fn activate_nonexistent_returns_error() {
        let mut reg = SkillRegistry::new();
        let mut mgr = ActivationManager::new(10_000);
        let result = mgr.activate(&mut reg, "nope", "manual".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn budget_enforcement() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_skill("big-skill", "A big skill", ""));
        let mut mgr = ActivationManager::new(10);

        let result = mgr
            .activate(&mut reg, "big-skill", "manual".to_string())
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn eviction_frees_budget() {
        let mut reg = SkillRegistry::new();
        let mut skill1 = make_skill("first", "First skill", "");
        skill1.effort = SkillEffortLevel::Low;
        let mut skill2 = make_skill("second", "Second skill", "");
        skill2.effort = SkillEffortLevel::Low;
        reg.register_bundled(skill1);
        reg.register_bundled(skill2);

        let mut mgr = ActivationManager::new(3_000);
        let _ = mgr.activate(&mut reg, "first", "manual".to_string());
        let _ = mgr.activate(&mut reg, "second", "manual".to_string());
        assert_eq!(mgr.active_count(), 2);

        let mut skill3 = make_skill("third", "Third skill", "");
        skill3.effort = SkillEffortLevel::High;
        reg.register_bundled(skill3);

        let result = mgr
            .activate(&mut reg, "third", "manual".to_string())
            .unwrap();
        assert!(result.is_some());
        assert!(mgr.active_count() <= 2);
    }

    #[test]
    fn evaluate_for_context_scoring() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_skill(
            "code-review",
            "Reviews code for quality",
            "Use when reviewing pull requests",
        ));
        reg.register_bundled(make_skill(
            "deploy",
            "Deploys to production",
            "Use when deploying",
        ));
        let mgr = ActivationManager::new(10_000);

        let recs = mgr.evaluate_for_context(&reg, "please review my code changes");
        assert!(!recs.is_empty());
        assert_eq!(recs[0].skill_id, "code-review");
        assert!(recs[0].score > 0.0);
    }

    #[test]
    fn evaluate_skips_low_scores() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_skill(
            "unrelated",
            "Completely unrelated",
            "For underwater basket weaving",
        ));
        let mgr = ActivationManager::new(10_000);

        let recs = mgr.evaluate_for_context(&reg, "write a rust function");
        assert!(recs.is_empty());
    }

    #[test]
    fn activate_for_paths_matches_conditional() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_conditional_skill(
            "rust-skill",
            vec!["*.rs", "src/**/*.rs"],
        ));
        let mut mgr = ActivationManager::new(10_000);

        let activated = mgr.activate_for_paths(&mut reg, &["src/main.rs", "lib.rs"]);
        assert_eq!(activated.len(), 1);
        assert_eq!(activated[0], "rust-skill");
        assert!(mgr.is_active("rust-skill"));
    }

    #[test]
    fn activate_for_paths_no_match() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_conditional_skill("rust-skill", vec!["*.rs"]));
        let mut mgr = ActivationManager::new(10_000);

        let activated = mgr.activate_for_paths(&mut reg, &["main.py", "index.ts"]);
        assert!(activated.is_empty());
        assert!(!mgr.is_active("rust-skill"));
    }

    #[test]
    fn activate_for_paths_budget_limited() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_conditional_skill("rs1", vec!["*.rs"]));
        reg.register_bundled(make_conditional_skill("rs2", vec!["*.rs"]));
        let mut mgr = ActivationManager::new(10);

        let activated = mgr.activate_for_paths(&mut reg, &["main.rs"]);
        assert!(activated.len() <= 1);
    }

    #[test]
    fn reactivate_updates_last_accessed() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_skill("test", "test", ""));
        let mut mgr = ActivationManager::new(10_000);

        let _ = mgr.activate(&mut reg, "test", "first".to_string()).unwrap();
        let _ = mgr
            .activate(&mut reg, "test", "second".to_string())
            .unwrap();
        assert_eq!(mgr.active_count(), 1);
        assert_eq!(mgr.active.get("test").unwrap().trigger, "first");
    }

    #[test]
    fn active_skills_returns_all() {
        let mut reg = SkillRegistry::new();
        reg.register_bundled(make_skill("a", "A", ""));
        reg.register_bundled(make_skill("b", "B", ""));
        let mut mgr = ActivationManager::new(10_000);

        let _ = mgr.activate(&mut reg, "a", "manual".to_string());
        let _ = mgr.activate(&mut reg, "b", "manual".to_string());

        let active = mgr.get_active_skills();
        assert_eq!(active.len(), 2);
    }
}
