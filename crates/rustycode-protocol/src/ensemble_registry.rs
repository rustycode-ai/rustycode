//! Team Registry — Agent team grouping and coordination.
//!
//! This module provides registry for grouping agents/workers into teams:
//! - Team creation with task assignments
//! - Team lifecycle tracking (Created → Running → Completed → Deleted)
//! - Team listing and status queries
//! - Global registry for centralized access
//!
//! Inspired by claw-code's team_cron_registry module.
//!
//! # Architecture
//!
//! ```text
//! TeamRegistry → Team { team_id, name, task_ids, status, created_at, updated_at }
//!      │
//!      ├─ create("Security Team", vec!["task1", "task2"])
//!      ├─ list() → all teams
//!      └─ delete(team_id) → mark as deleted
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Team status lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TeamStatus {
    /// Team was created
    Created,
    /// Team is actively working
    Running,
    /// Team completed its work
    Completed,
    /// Team was deleted
    Deleted,
}

impl fmt::Display for TeamStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Deleted => write!(f, "deleted"),
        }
    }
}

/// A team of agents working on related tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    /// Unique team identifier
    pub team_id: String,
    /// Human-readable team name
    pub name: String,
    /// List of task IDs assigned to this team
    pub task_ids: Vec<String>,
    /// Current team status
    pub status: TeamStatus,
    /// Creation timestamp (unix epoch seconds)
    pub created_at: u64,
    /// Last update timestamp (unix epoch seconds)
    pub updated_at: u64,
}

/// Internal registry state
#[derive(Debug, Default)]
struct TeamRegistryInner {
    teams: HashMap<String, Team>,
    counter: u64,
}

/// Registry for agent team grouping
///
/// Provides centralized tracking of agent teams for coordinated work:
/// - Group multiple agents under a single team identity
/// - Track which tasks a team is working on
/// - Monitor team lifecycle state
#[derive(Debug, Clone, Default)]
pub struct TeamRegistry {
    inner: Arc<Mutex<TeamRegistryInner>>,
}

impl TeamRegistry {
    /// Create a new empty team registry
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new team
    ///
    /// # Arguments
    ///
    /// * `name` - Human-readable team name
    /// * `task_ids` - List of task IDs to assign to the team
    ///
    /// # Returns
    ///
    /// The newly created Team with status `Created`
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_protocol::team_registry::{TeamRegistry, TeamStatus};
    ///
    /// let registry = TeamRegistry::new();
    /// let team = registry.create(
    ///     "Security Team",
    ///     vec!["task_001".to_string(), "task_002".to_string()],
    /// );
    /// assert_eq!(team.name, "Security Team");
    /// assert_eq!(team.status, TeamStatus::Created);
    /// ```
    pub fn create(&self, name: &str, task_ids: Vec<String>) -> Team {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.counter += 1;
        let ts = now_secs();
        let team_id = format!("team_{:08x}_{:04x}", ts, inner.counter);

        let team = Team {
            team_id: team_id.clone(),
            name: name.to_owned(),
            task_ids,
            status: TeamStatus::Created,
            created_at: ts,
            updated_at: ts,
        };

        inner.teams.insert(team_id, team.clone());
        team
    }

    /// Get a team by ID
    ///
    /// # Returns
    ///
    /// `Some(Team)` if found, `None` otherwise
    #[must_use]
    pub fn get(&self, team_id: &str) -> Option<Team> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.teams.get(team_id).cloned()
    }

    /// List all teams
    #[must_use]
    pub fn list(&self) -> Vec<Team> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.teams.values().cloned().collect()
    }

    /// Delete a team (marks as Deleted)
    ///
    /// # Arguments
    ///
    /// * `team_id` - ID of team to delete
    ///
    /// # Returns
    ///
    /// `Ok(Team)` with status Deleted, `Err(String)` if not found
    pub fn delete(&self, team_id: &str) -> Result<Team, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let team = inner
            .teams
            .get_mut(team_id)
            .ok_or_else(|| format!("team not found: {team_id}"))?;
        team.status = TeamStatus::Deleted;
        team.updated_at = now_secs();
        Ok(team.clone())
    }

    /// Remove a team from the registry entirely
    ///
    /// # Arguments
    ///
    /// * `team_id` - ID of team to remove
    ///
    /// # Returns
    ///
    /// `Some(Team)` if removed, `None` if not found
    #[must_use]
    pub fn remove(&self, team_id: &str) -> Option<Team> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.teams.remove(team_id)
    }

    /// Mark a team as Running
    ///
    /// # Arguments
    ///
    /// * `team_id` - ID of team to update
    ///
    /// # Returns
    ///
    /// `Ok(Team)` with status Running, `Err(String)` if not found
    pub fn mark_running(&self, team_id: &str) -> Result<Team, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let team = inner
            .teams
            .get_mut(team_id)
            .ok_or_else(|| format!("team not found: {team_id}"))?;
        team.status = TeamStatus::Running;
        team.updated_at = now_secs();
        Ok(team.clone())
    }

    /// Mark a team as Completed
    ///
    /// # Arguments
    ///
    /// * `team_id` - ID of team to update
    ///
    /// # Returns
    ///
    /// `Ok(Team)` with status Completed, `Err(String)` if not found
    pub fn mark_completed(&self, team_id: &str) -> Result<Team, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let team = inner
            .teams
            .get_mut(team_id)
            .ok_or_else(|| format!("team not found: {team_id}"))?;
        team.status = TeamStatus::Completed;
        team.updated_at = now_secs();
        Ok(team.clone())
    }

    /// Add a task to a team
    ///
    /// # Arguments
    ///
    /// * `team_id` - ID of team to update
    /// * `task_id` - Task ID to add
    ///
    /// # Returns
    ///
    /// `Ok(Team)` with updated task list, `Err(String)` if not found
    pub fn add_task(&self, team_id: &str, task_id: &str) -> Result<Team, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let team = inner
            .teams
            .get_mut(team_id)
            .ok_or_else(|| format!("team not found: {team_id}"))?;
        team.task_ids.push(task_id.to_owned());
        team.updated_at = now_secs();
        Ok(team.clone())
    }

    /// Get count of teams
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.teams.len()
    }

    /// Check if registry is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get teams by status
    #[must_use]
    pub fn teams_by_status(&self, status: TeamStatus) -> Vec<Team> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .teams
            .values()
            .filter(|t| t.status == status)
            .cloned()
            .collect()
    }
}

// ── Global Registry Accessor ────────────────────────────────────────────────────────

use std::sync::OnceLock;

/// Global team registry accessor for centralized state management.
///
/// This follows the claw-code pattern of using OnceLock for global registries,
/// enabling any part of the codebase to access shared state without threading
/// Arc<Registry> through every layer.
///
/// # Example
///
/// ```
/// use rustycode_protocol::team_registry::global_team_registry;
/// let registry = global_team_registry();
/// let team = registry.create("My Team", vec!["task1".to_string()]);
/// ```
pub fn global_team_registry() -> &'static TeamRegistry {
    static REGISTRY: OnceLock<TeamRegistry> = OnceLock::new();
    REGISTRY.get_or_init(TeamRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_create_and_get() {
        let registry = TeamRegistry::new();

        let team = registry.create(
            "Security Team",
            vec!["task_001".to_string(), "task_002".to_string()],
        );

        assert!(team.team_id.starts_with("team_"));
        assert_eq!(team.name, "Security Team");
        assert_eq!(team.task_ids.len(), 2);
        assert_eq!(team.status, TeamStatus::Created);

        // Retrieve by ID
        let retrieved = registry.get(&team.team_id).unwrap();
        assert_eq!(retrieved.team_id, team.team_id);
        assert_eq!(retrieved.name, team.name);
    }

    #[test]
    fn test_team_list() {
        let registry = TeamRegistry::new();

        registry.create("Team A", vec!["t1".to_string()]);
        registry.create("Team B", vec!["t2".to_string()]);
        registry.create("Team C", vec!["t3".to_string()]);

        let teams = registry.list();
        assert_eq!(teams.len(), 3);

        let names: Vec<&str> = teams.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Team A"));
        assert!(names.contains(&"Team B"));
        assert!(names.contains(&"Team C"));
    }

    #[test]
    fn test_team_delete() {
        let registry = TeamRegistry::new();
        let team = registry.create("To Delete", vec![]);

        // Delete
        let deleted = registry.delete(&team.team_id).unwrap();
        assert_eq!(deleted.status, TeamStatus::Deleted);

        // Team still exists but marked deleted
        let retrieved = registry.get(&team.team_id).unwrap();
        assert_eq!(retrieved.status, TeamStatus::Deleted);
    }

    #[test]
    fn test_team_remove() {
        let registry = TeamRegistry::new();
        let team = registry.create("To Remove", vec![]);

        // Remove
        let removed = registry.remove(&team.team_id);
        assert!(removed.is_some());

        // Team is gone
        assert!(registry.get(&team.team_id).is_none());

        // Remove non-existent
        let result = registry.remove(&team.team_id);
        assert!(result.is_none());
    }

    #[test]
    fn test_team_mark_running() {
        let registry = TeamRegistry::new();
        let team = registry.create("Running Team", vec![]);

        assert_eq!(team.status, TeamStatus::Created);

        let team = registry.mark_running(&team.team_id).unwrap();
        assert_eq!(team.status, TeamStatus::Running);
    }

    #[test]
    fn test_team_mark_completed() {
        let registry = TeamRegistry::new();
        let team = registry.create("Worker Team", vec![]);

        registry.mark_running(&team.team_id).unwrap();
        let team = registry.mark_completed(&team.team_id).unwrap();
        assert_eq!(team.status, TeamStatus::Completed);
    }

    #[test]
    fn test_team_add_task() {
        let registry = TeamRegistry::new();
        let team = registry.create("Task Team", vec!["task_001".to_string()]);

        assert_eq!(team.task_ids.len(), 1);

        let team = registry.add_task(&team.team_id, "task_002").unwrap();
        assert_eq!(team.task_ids.len(), 2);
        assert!(team.task_ids.contains(&"task_002".to_string()));
    }

    #[test]
    fn test_teams_by_status() {
        let registry = TeamRegistry::new();

        let team1 = registry.create("Created Team", vec![]);
        let team2 = registry.create("Running Team 1", vec![]);
        let team3 = registry.create("Running Team 2", vec![]);

        registry.mark_running(&team2.team_id).unwrap();
        registry.mark_running(&team3.team_id).unwrap();

        let created = registry.teams_by_status(TeamStatus::Created);
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].team_id, team1.team_id);

        let running = registry.teams_by_status(TeamStatus::Running);
        assert_eq!(running.len(), 2);
    }

    #[test]
    fn test_team_len_and_is_empty() {
        let registry = TeamRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.create("Team", vec![]);
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_team_not_found_errors() {
        let registry = TeamRegistry::new();

        assert!(registry.get("nonexistent").is_none());
        assert!(registry.delete("nonexistent").is_err());
        assert!(registry.mark_running("nonexistent").is_err());
        assert!(registry.mark_completed("nonexistent").is_err());
        assert!(registry.add_task("nonexistent", "task").is_err());
        assert!(registry.remove("nonexistent").is_none());
    }

    #[test]
    fn test_global_registry() {
        // First call initializes
        let registry1 = global_team_registry();
        let team = registry1.create("Global Team", vec!["task1".to_string()]);

        // Second call returns same registry
        let registry2 = global_team_registry();
        let retrieved = registry2.get(&team.team_id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().team_id, team.team_id);
    }

    #[test]
    fn test_team_id_format() {
        let registry = TeamRegistry::new();
        let team = registry.create("Test Team", vec![]);

        // Team ID should start with "team_"
        assert!(team.team_id.starts_with("team_"));

        // Should have timestamp and counter parts
        let parts: Vec<&str> = team.team_id.split('_').collect();
        assert_eq!(parts.len(), 3); // "team", timestamp, counter
    }

    #[test]
    fn test_team_status_display() {
        assert_eq!(TeamStatus::Created.to_string(), "created");
        assert_eq!(TeamStatus::Running.to_string(), "running");
        assert_eq!(TeamStatus::Completed.to_string(), "completed");
        assert_eq!(TeamStatus::Deleted.to_string(), "deleted");
    }
}
