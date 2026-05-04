//! Goal tracking with token budget enforcement.
//!
//! Provides LLM-facing tools to set objectives, track progress, and enforce
//! token spending limits per goal.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Status of a goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    Active,
    Completed,
    Abandoned,
}

/// A tracked goal with optional token budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub objective: String,
    pub status: GoalStatus,
    pub token_budget: Option<u64>,
    pub tokens_used: u64,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
}

/// Manages goal lifecycle and token budget checking.
#[derive(Debug, Clone)]
pub struct GoalManager {
    goals: Arc<Mutex<Vec<Goal>>>,
}

impl GoalManager {
    pub fn new() -> Self {
        Self {
            goals: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a new goal. Returns the goal ID.
    pub fn create_goal(&self, objective: &str, token_budget: Option<u64>) -> String {
        let id = format!("goal-{}", Self::next_id());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let goal = Goal {
            id: id.clone(),
            objective: objective.to_string(),
            status: GoalStatus::Active,
            token_budget,
            tokens_used: 0,
            created_at_secs: now,
            updated_at_secs: now,
        };

        self.goals.lock().unwrap_or_else(|e| e.into_inner()).push(goal);
        id
    }

    /// Get a goal by ID.
    pub fn get_goal(&self, id: &str) -> Option<Goal> {
        self.goals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|g| g.id == id)
            .cloned()
    }

    /// Update a goal's status.
    pub fn update_goal(&self, id: &str, status: GoalStatus) -> Option<Goal> {
        let mut goals = self.goals.lock().unwrap_or_else(|e| e.into_inner());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for goal in goals.iter_mut() {
            if goal.id == id {
                goal.status = status;
                goal.updated_at_secs = now;
                return Some(goal.clone());
            }
        }
        None
    }

    /// Add token usage to all active goals.
    pub fn add_token_usage(&self, tokens: u64) {
        let mut goals = self.goals.lock().unwrap_or_else(|e| e.into_inner());
        for goal in goals.iter_mut() {
            if goal.status == GoalStatus::Active {
                goal.tokens_used = goal.tokens_used.saturating_add(tokens);
            }
        }
    }

    /// Check if the current active goal should continue based on token budget.
    pub fn should_continue(&self) -> bool {
        let goals = self.goals.lock().unwrap_or_else(|e| e.into_inner());
        goals
            .iter()
            .filter(|g| g.status == GoalStatus::Active)
            .all(|g| match g.token_budget {
                Some(budget) => g.tokens_used < budget,
                None => true,
            })
    }

    /// Get the active goal, if any.
    pub fn active_goal(&self) -> Option<Goal> {
        self.goals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|g| g.status == GoalStatus::Active)
            .cloned()
    }

    /// List all goals.
    pub fn list_goals(&self) -> Vec<Goal> {
        self.goals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Generate a continuation prompt when budget is nearly exhausted.
    pub fn continuation_prompt(&self) -> Option<String> {
        let goal = self.active_goal()?;
        let budget = goal.token_budget?;
        let remaining = budget.saturating_sub(goal.tokens_used);
        let pct = (remaining as f64 / budget as f64) * 100.0;

        if pct < 20.0 {
            Some(format!(
                "Token budget nearly exhausted: {:.0}% remaining ({}/{}). \
                 Wrap up the current objective or request more budget. \
                 Objective: {}",
                pct, remaining, budget, goal.objective
            ))
        } else {
            None
        }
    }

    fn next_id() -> u64 {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for GoalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_goal() {
        let mgr = GoalManager::new();
        let id = mgr.create_goal("Fix the auth bug", Some(10_000));
        let goal = mgr.get_goal(&id).unwrap();
        assert_eq!(goal.objective, "Fix the auth bug");
        assert_eq!(goal.status, GoalStatus::Active);
        assert_eq!(goal.token_budget, Some(10_000));
    }

    #[test]
    fn test_update_goal_status() {
        let mgr = GoalManager::new();
        let id = mgr.create_goal("Write tests", None);
        let updated = mgr.update_goal(&id, GoalStatus::Completed).unwrap();
        assert_eq!(updated.status, GoalStatus::Completed);
    }

    #[test]
    fn test_token_budget_enforcement() {
        let mgr = GoalManager::new();
        let id = mgr.create_goal("Limited task", Some(100));
        assert!(mgr.should_continue());
        mgr.add_token_usage(50);
        assert!(mgr.should_continue());
        mgr.add_token_usage(60);
        assert!(!mgr.should_continue());
    }

    #[test]
    fn test_continuation_prompt() {
        let mgr = GoalManager::new();
        let id = mgr.create_goal("Big task", Some(100));
        assert!(mgr.continuation_prompt().is_none()); // 100% remaining
        mgr.add_token_usage(85);
        let prompt = mgr.continuation_prompt().unwrap();
        assert!(prompt.contains("15%"));
        assert!(prompt.contains("Big task"));
    }

    #[test]
    fn test_list_goals() {
        let mgr = GoalManager::new();
        mgr.create_goal("A", None);
        mgr.create_goal("B", None);
        assert_eq!(mgr.list_goals().len(), 2);
    }

    #[test]
    fn test_active_goal_returns_first_active() {
        let mgr = GoalManager::new();
        let id1 = mgr.create_goal("First", None);
        let _id2 = mgr.create_goal("Second", None);
        let active = mgr.active_goal().unwrap();
        assert_eq!(active.id, id1);
    }
}
