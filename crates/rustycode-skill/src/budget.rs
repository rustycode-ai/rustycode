use anyhow::{bail, Result};
use std::collections::HashMap;

pub const DEFAULT_WARNING_THRESHOLD: f64 = 0.8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillBudgetEntry {
    pub skill_name: String,
    pub tokens: u64,
    pub priority: u8,
}

#[derive(Debug, Clone)]
pub struct ContextBudget {
    pub total: u64,
    pub entries: HashMap<String, SkillBudgetEntry>,
}

impl ContextBudget {
    pub fn new(total: u64) -> Self {
        Self {
            total,
            entries: HashMap::new(),
        }
    }

    pub fn used(&self) -> u64 {
        self.entries.values().map(|entry| entry.tokens).sum()
    }

    pub fn remaining(&self) -> u64 {
        self.total.saturating_sub(self.used())
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn utilization(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.used() as f64 / self.total as f64
    }

    pub fn is_over_warning_threshold(&self) -> bool {
        self.utilization() >= DEFAULT_WARNING_THRESHOLD
    }

    pub fn is_active(&self, skill_name: &str) -> bool {
        self.entries.contains_key(skill_name)
    }

    pub fn allocate(&mut self, skill_name: &str, tokens: u64) -> Result<()> {
        let current = self.used();
        let proposed = current.saturating_add(tokens);
        if proposed > self.total {
            bail!(
                "allocating {} tokens for '{}' (total {} would exceed budget {})",
                tokens,
                skill_name,
                proposed,
                self.total
            );
        }
        self.entries.insert(
            skill_name.to_string(),
            SkillBudgetEntry {
                skill_name: skill_name.to_string(),
                tokens,
                priority: 5,
            },
        );
        Ok(())
    }

    pub fn deallocate(&mut self, skill_name: &str) {
        self.entries.remove(skill_name);
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self::new(100_000)
    }
}

#[derive(Debug, Clone)]
pub struct BudgetEnforcer {
    budget: ContextBudget,
}

impl BudgetEnforcer {
    pub fn new(total_budget: u64) -> Self {
        Self {
            budget: ContextBudget::new(total_budget),
        }
    }

    pub const fn budget(&self) -> &ContextBudget {
        &self.budget
    }

    pub fn add_skill(&mut self, name: &str, tokens: u64, priority: u8) {
        self.budget.entries.insert(
            name.to_string(),
            SkillBudgetEntry {
                skill_name: name.to_string(),
                tokens,
                priority,
            },
        );
    }

    pub fn deactivate_skill(&mut self, name: &str) {
        self.budget.deallocate(name);
    }

    pub fn enforce_budget(&mut self) -> Vec<String> {
        let mut evicted = Vec::new();

        while self.budget.used() > self.budget.total {
            let to_evict = self
                .budget
                .entries
                .values()
                .max_by_key(|entry| entry.priority)
                .map(|entry| entry.skill_name.clone());

            if let Some(name) = to_evict {
                self.budget.deallocate(&name);
                evicted.push(name);
            } else {
                break;
            }
        }

        evicted
    }

    pub fn active_skills(&self) -> Vec<&SkillBudgetEntry> {
        let mut skills: Vec<_> = self.budget.entries.values().collect();
        skills.sort_by_key(|entry| entry.priority);
        skills
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_budget_default_is_sane() {
        let budget = ContextBudget::default();
        assert_eq!(budget.total, 100_000);
        assert_eq!(budget.used(), 0);
        assert_eq!(budget.remaining(), 100_000);
    }

    #[test]
    fn context_budget_new_and_usage_tracking() {
        let mut budget = ContextBudget::new(200_000);
        budget.allocate("skill-a", 50_000).unwrap();
        budget.allocate("skill-b", 30_000).unwrap();
        assert_eq!(budget.used(), 80_000);
        assert_eq!(budget.remaining(), 120_000);
    }

    #[test]
    fn context_budget_warning_threshold_trips() {
        let mut budget = ContextBudget::new(100_000);
        budget.allocate("skill-a", 80_000).unwrap();
        assert!(budget.is_over_warning_threshold());
    }

    #[test]
    fn context_budget_allocation_rejects_overflow() {
        let mut budget = ContextBudget::new(100_000);
        budget.allocate("skill-a", 80_000).unwrap();
        assert!(budget.allocate("skill-b", 30_000).is_err());
    }

    #[test]
    fn context_budget_deallocate_frees_space() {
        let mut budget = ContextBudget::new(100_000);
        budget.allocate("skill-a", 40_000).unwrap();
        budget.deallocate("skill-a");
        assert_eq!(budget.used(), 0);
        assert!(!budget.is_active("skill-a"));
    }

    #[test]
    fn context_budget_zero_total() {
        let budget = ContextBudget::new(0);
        assert_eq!(budget.remaining(), 0);
        assert_eq!(budget.used(), 0);
        assert_eq!(budget.utilization(), 0.0);
    }

    #[test]
    fn context_budget_reallocate_same_skill() {
        let mut budget = ContextBudget::new(100_000);
        budget.allocate("skill-a", 40_000).unwrap();
        budget.deallocate("skill-a");
        budget.allocate("skill-a", 20_000).unwrap();
        assert_eq!(budget.used(), 20_000);
    }

    #[test]
    fn budget_enforcer_no_eviction_when_under_budget() {
        let mut enforcer = BudgetEnforcer::new(100_000);
        enforcer.add_skill("skill-a", 30_000, 5);
        enforcer.add_skill("skill-b", 20_000, 3);

        let evicted = enforcer.enforce_budget();
        assert!(evicted.is_empty());
    }

    #[test]
    fn budget_enforcer_evicts_lowest_priority() {
        let mut enforcer = BudgetEnforcer::new(100_000);
        enforcer.add_skill("low-priority", 60_000, 8);
        enforcer.add_skill("high-priority", 30_000, 2);

        let evicted = enforcer.enforce_budget();
        assert_eq!(evicted.len(), 0);

        enforcer.add_skill("medium-priority", 20_000, 5);
        let evicted = enforcer.enforce_budget();
        assert!(evicted.contains(&"low-priority".to_string()));
    }

    #[test]
    fn budget_enforcer_evicts_multiple_if_needed() {
        let mut enforcer = BudgetEnforcer::new(100_000);
        enforcer.add_skill("skill-a", 50_000, 9);
        enforcer.add_skill("skill-b", 40_000, 8);
        enforcer.add_skill("skill-c", 30_000, 2);

        let evicted = enforcer.enforce_budget();
        assert!(evicted.contains(&"skill-a".to_string()));
        assert_eq!(enforcer.budget().used(), 70_000);
    }

    #[test]
    fn budget_enforcer_eviction_preserves_high_priority() {
        let mut enforcer = BudgetEnforcer::new(100_000);
        enforcer.add_skill("critical", 40_000, 1);
        enforcer.add_skill("expendable", 80_000, 10);

        let evicted = enforcer.enforce_budget();
        assert!(evicted.contains(&"expendable".to_string()));
        assert!(!evicted.contains(&"critical".to_string()));
        assert!(enforcer.budget().is_active("critical"));
    }

    #[test]
    fn budget_enforcer_deactivate_skill() {
        let mut enforcer = BudgetEnforcer::new(100_000);
        enforcer.add_skill("skill-a", 40_000, 5);
        enforcer.deactivate_skill("skill-a");
        assert!(!enforcer.budget().is_active("skill-a"));
        assert_eq!(enforcer.budget().used(), 0);
    }

    #[test]
    fn budget_enforcer_list_active_skills_sorted_by_priority() {
        let mut enforcer = BudgetEnforcer::new(200_000);
        enforcer.add_skill("medium", 10_000, 5);
        enforcer.add_skill("high", 10_000, 1);
        enforcer.add_skill("low", 10_000, 9);

        let active = enforcer.active_skills();
        assert_eq!(active[0].skill_name, "high");
        assert_eq!(active[1].skill_name, "medium");
        assert_eq!(active[2].skill_name, "low");
    }

    #[test]
    fn budget_enforcer_available_after_eviction() {
        let mut enforcer = BudgetEnforcer::new(100_000);
        enforcer.add_skill("big", 90_000, 5);
        assert_eq!(enforcer.budget().remaining(), 10_000);

        enforcer.deactivate_skill("big");
        assert_eq!(enforcer.budget().remaining(), 100_000);
    }

    #[test]
    fn budget_enforcer_skills_at_same_priority() {
        let mut enforcer = BudgetEnforcer::new(100_000);
        enforcer.add_skill("a", 60_000, 5);
        enforcer.add_skill("b", 60_000, 5);
        let evicted = enforcer.enforce_budget();
        assert_eq!(evicted.len(), 1);
        assert_eq!(enforcer.budget().used(), 60_000);
    }
}
