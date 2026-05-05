//! Agent Lifecycle Management System
//!
//! This module provides comprehensive agent lifecycle management with:
//! - Agent creation and initialization
//! - State management and transitions
//! - Health monitoring integration
//! - Graceful shutdown and cleanup
//! - Agent pool management
//! - Lifecycle hooks and callbacks
//! - Dependency management

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};

/// Agent lifecycle states
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AgentState {
    /// Agent is being created
    Creating,
    /// Agent is initializing
    Initializing,
    /// Agent is ready to accept tasks
    Ready,
    /// Agent is processing a task
    Busy,
    /// Agent is temporarily suspended
    Suspended,
    /// Agent is being terminated
    Terminating,
    /// Agent has terminated
    Terminated,
    /// Agent encountered an error
    Error,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentState::Creating => write!(f, "Creating"),
            AgentState::Initializing => write!(f, "Initializing"),
            AgentState::Ready => write!(f, "Ready"),
            AgentState::Busy => write!(f, "Busy"),
            AgentState::Suspended => write!(f, "Suspended"),
            AgentState::Terminating => write!(f, "Terminating"),
            AgentState::Terminated => write!(f, "Terminated"),
            AgentState::Error => write!(f, "Error"),
        }
    }
}

/// Valid state transitions
pub const VALID_TRANSITIONS: &[(AgentState, AgentState)] = &[
    (AgentState::Creating, AgentState::Initializing),
    (AgentState::Creating, AgentState::Error),
    (AgentState::Initializing, AgentState::Ready),
    (AgentState::Initializing, AgentState::Error),
    (AgentState::Ready, AgentState::Busy),
    (AgentState::Ready, AgentState::Suspended),
    (AgentState::Ready, AgentState::Terminating),
    (AgentState::Ready, AgentState::Error), // Ready agents can encounter errors
    (AgentState::Busy, AgentState::Ready),
    (AgentState::Busy, AgentState::Error),
    (AgentState::Busy, AgentState::Terminating),
    (AgentState::Suspended, AgentState::Ready),
    (AgentState::Suspended, AgentState::Terminating),
    (AgentState::Suspended, AgentState::Error), // Suspended agents can encounter errors
    (AgentState::Terminating, AgentState::Terminated),
    (AgentState::Error, AgentState::Initializing),
    (AgentState::Error, AgentState::Terminating),
];

/// Agent instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInstance {
    pub id: String,
    pub name: String,
    pub role: AgentRole,
    pub state: AgentState,
    pub pid: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub initialized_at: Option<DateTime<Utc>>,
    pub last_activity: DateTime<Utc>,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub total_runtime_ms: u64,
    pub dependencies: Vec<String>, // Depends on other agents
    pub dependents: Vec<String>,   // Agents that depend on this one
    pub metadata: HashMap<String, String>,
}

impl AgentInstance {
    /// Check if agent can transition to new state
    pub fn can_transition_to(&self, new_state: AgentState) -> bool {
        VALID_TRANSITIONS
            .iter()
            .any(|(from, to)| *from == self.state && *to == new_state)
    }

    /// Get agent age in seconds
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.created_at).num_seconds()
    }

    /// Get agent uptime (time since initialization)
    pub fn uptime_seconds(&self) -> Option<i64> {
        self.initialized_at.map(|t| (Utc::now() - t).num_seconds())
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.tasks_completed + self.tasks_failed;
        if total == 0 {
            0.0
        } else {
            self.tasks_completed as f64 / total as f64
        }
    }
}

/// Lifecycle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    pub max_agents_per_role: usize,
    pub agent_idle_timeout_seconds: u64,
    pub agent_startup_timeout_seconds: u64,
    pub enable_auto_restart: bool,
    pub max_restart_attempts: u32,
    pub graceful_shutdown_timeout_seconds: u64,
    pub enable_dependency_tracking: bool,
    pub cleanup_orphaned_agents: bool,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            max_agents_per_role: 10,
            agent_idle_timeout_seconds: 300, // 5 minutes
            agent_startup_timeout_seconds: 60,
            enable_auto_restart: true,
            max_restart_attempts: 3,
            graceful_shutdown_timeout_seconds: 30,
            enable_dependency_tracking: true,
            cleanup_orphaned_agents: true,
        }
    }
}

/// Lifecycle event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub agent_id: String,
    pub event_type: LifecycleEventType,
    pub previous_state: AgentState,
    pub new_state: AgentState,
    pub timestamp: DateTime<Utc>,
    pub reason: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Lifecycle event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum LifecycleEventType {
    Created,
    Initialized,
    StateChanged,
    Suspended,
    Resumed,
    Terminated,
    ErrorOccurred,
    Restarted,
}

/// Lifecycle hooks for agent customization
pub trait LifecycleHooks: Send + Sync {
    /// Called when agent is created
    fn on_create(&self, agent: &AgentInstance) -> Result<(), String>;

    /// Called when agent is initialized
    fn on_init(&self, agent: &AgentInstance) -> Result<(), String>;

    /// Called before state transition
    fn before_transition(&self, agent: &AgentInstance, new_state: AgentState)
        -> Result<(), String>;

    /// Called after state transition
    fn after_transition(&self, agent: &AgentInstance, old_state: AgentState) -> Result<(), String>;

    /// Called when agent is terminated
    fn on_terminate(&self, agent: &AgentInstance) -> Result<(), String>;
}

/// Default lifecycle hooks implementation
pub struct DefaultLifecycleHooks;

impl LifecycleHooks for DefaultLifecycleHooks {
    fn on_create(&self, _agent: &AgentInstance) -> Result<(), String> {
        Ok(())
    }

    fn on_init(&self, _agent: &AgentInstance) -> Result<(), String> {
        Ok(())
    }

    fn before_transition(
        &self,
        _agent: &AgentInstance,
        _new_state: AgentState,
    ) -> Result<(), String> {
        Ok(())
    }

    fn after_transition(
        &self,
        _agent: &AgentInstance,
        _old_state: AgentState,
    ) -> Result<(), String> {
        Ok(())
    }

    fn on_terminate(&self, _agent: &AgentInstance) -> Result<(), String> {
        Ok(())
    }
}

/// Agent lifecycle manager
pub struct AgentLifecycleManager {
    agents: Arc<RwLock<HashMap<String, AgentInstance>>>,
    lifecycle_events: Arc<RwLock<Vec<LifecycleEvent>>>,
    config: LifecycleConfig,
    agent_counter: Arc<RwLock<u64>>,
    hooks: Arc<Box<dyn LifecycleHooks>>,
    semaphore: Arc<Semaphore>,
}

impl AgentLifecycleManager {
    pub fn new(config: LifecycleConfig) -> Self {
        let max_total_agents = config.max_agents_per_role * 8; // 8 agent roles
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            lifecycle_events: Arc::new(RwLock::new(Vec::new())),
            config,
            agent_counter: Arc::new(RwLock::new(0)),
            hooks: Arc::new(Box::new(DefaultLifecycleHooks)),
            semaphore: Arc::new(Semaphore::new(max_total_agents)),
        }
    }

    /// Set custom lifecycle hooks
    pub fn with_hooks(mut self, hooks: Box<dyn LifecycleHooks>) -> Self {
        self.hooks = Arc::new(hooks);
        self
    }

    /// Create a new agent
    pub async fn create_agent(
        &self,
        name: String,
        role: AgentRole,
        dependencies: Vec<String>,
    ) -> Result<String, String> {
        // Check if we can create more agents of this role
        {
            let agents = self.agents.read().await;
            let role_count = agents
                .values()
                .filter(|a| a.role == role && a.state != AgentState::Terminated)
                .count();

            if role_count >= self.config.max_agents_per_role {
                return Err(format!(
                    "Maximum agents ({}) reached for role {:?}",
                    self.config.max_agents_per_role, role
                ));
            }
        }

        // Acquire semaphore
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| format!("Failed to acquire semaphore: {}", e))?;

        // Generate agent ID
        let mut counter = self.agent_counter.write().await;
        *counter += 1;
        let agent_id = format!("agent_{}", *counter);

        // Create agent instance
        let agent = AgentInstance {
            id: agent_id.clone(),
            name: name.clone(),
            role,
            state: AgentState::Creating,
            pid: None,
            created_at: Utc::now(),
            initialized_at: None,
            last_activity: Utc::now(),
            tasks_completed: 0,
            tasks_failed: 0,
            total_runtime_ms: 0,
            dependencies: dependencies.clone(),
            dependents: Vec::new(),
            metadata: HashMap::new(),
        };

        // Call create hook
        self.hooks.on_create(&agent)?;

        // Store agent
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent_id.clone(), agent.clone());
        }

        // Record event
        self.record_event(LifecycleEvent {
            agent_id: agent_id.clone(),
            event_type: LifecycleEventType::Created,
            previous_state: AgentState::Creating,
            new_state: AgentState::Creating,
            timestamp: Utc::now(),
            reason: Some(format!("Created agent '{}' with role {:?}", name, role)),
            metadata: HashMap::new(),
        })
        .await;

        // Initialize dependencies
        if self.config.enable_dependency_tracking {
            self.update_dependencies(&agent_id, &dependencies).await?;
        }

        Ok(agent_id)
    }

    /// Initialize an agent
    pub async fn initialize_agent(&self, agent_id: &str) -> Result<(), String> {
        let agent = self.agent(agent_id).await?;

        // Check if we can transition to Initializing first
        if !agent.can_transition_to(AgentState::Initializing) {
            return Err(format!(
                "Agent {} cannot transition from {:?} to Initializing",
                agent_id, agent.state
            ));
        }

        // Transition to Initializing
        self.transition_state(agent_id, AgentState::Initializing)
            .await?;

        // Simulate initialization (in real system, would spawn process)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Update agent
        {
            let mut agents = self.agents.write().await;
            if let Some(a) = agents.get_mut(agent_id) {
                a.initialized_at = Some(Utc::now());
                a.last_activity = Utc::now();
                a.pid = Some(std::process::id()); // Use current process ID as placeholder
            }
        }

        // Now check if we can transition to Ready from Initializing
        let agent = self.agent(agent_id).await?;
        if !agent.can_transition_to(AgentState::Ready) {
            return Err(format!(
                "Agent {} cannot transition from {:?} to Ready",
                agent_id, agent.state
            ));
        }

        // Transition to Ready
        self.transition_state(agent_id, AgentState::Ready).await?;

        // Call init hook
        let agent = self.agent(agent_id).await?;
        self.hooks.on_init(&agent)?;

        // Record event
        self.record_event(LifecycleEvent {
            agent_id: agent_id.to_string(),
            event_type: LifecycleEventType::Initialized,
            previous_state: AgentState::Initializing,
            new_state: AgentState::Ready,
            timestamp: Utc::now(),
            reason: Some("Agent initialized successfully".to_string()),
            metadata: HashMap::new(),
        })
        .await;

        Ok(())
    }

    /// Get agent by ID
    pub async fn agent(&self, agent_id: &str) -> Result<AgentInstance, String> {
        let agents = self.agents.read().await;
        agents
            .get(agent_id)
            .cloned()
            .ok_or_else(|| format!("Agent {} not found", agent_id))
    }

    /// Transition agent to new state
    pub async fn transition_state(
        &self,
        agent_id: &str,
        new_state: AgentState,
    ) -> Result<(), String> {
        let agent = self.agent(agent_id).await?;
        let old_state = agent.state;

        // Check if transition is valid
        if !agent.can_transition_to(new_state) {
            return Err(format!(
                "Invalid state transition from {:?} to {:?}",
                old_state, new_state
            ));
        }

        // Call before transition hook
        self.hooks.before_transition(&agent, new_state)?;

        // Update agent state
        {
            let mut agents = self.agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.state = new_state;
                agent.last_activity = Utc::now();
            }
        }

        // Call after transition hook
        let agent = self.agent(agent_id).await?;
        self.hooks.after_transition(&agent, old_state)?;

        // Record event
        self.record_event(LifecycleEvent {
            agent_id: agent_id.to_string(),
            event_type: LifecycleEventType::StateChanged,
            previous_state: old_state,
            new_state,
            timestamp: Utc::now(),
            reason: None,
            metadata: HashMap::new(),
        })
        .await;

        Ok(())
    }

    /// Suspend an agent
    pub async fn suspend_agent(&self, agent_id: &str) -> Result<(), String> {
        self.transition_state(agent_id, AgentState::Suspended)
            .await?;

        let _agent = self.agent(agent_id).await?;
        self.record_event(LifecycleEvent {
            agent_id: agent_id.to_string(),
            event_type: LifecycleEventType::Suspended,
            previous_state: AgentState::Ready,
            new_state: AgentState::Suspended,
            timestamp: Utc::now(),
            reason: Some("Agent suspended by request".to_string()),
            metadata: HashMap::new(),
        })
        .await;

        Ok(())
    }

    /// Resume a suspended agent
    pub async fn resume_agent(&self, agent_id: &str) -> Result<(), String> {
        self.transition_state(agent_id, AgentState::Ready).await?;

        let _agent = self.agent(agent_id).await?;
        self.record_event(LifecycleEvent {
            agent_id: agent_id.to_string(),
            event_type: LifecycleEventType::Resumed,
            previous_state: AgentState::Suspended,
            new_state: AgentState::Ready,
            timestamp: Utc::now(),
            reason: Some("Agent resumed by request".to_string()),
            metadata: HashMap::new(),
        })
        .await;

        Ok(())
    }

    /// Terminate an agent
    pub async fn terminate_agent(&self, agent_id: &str) -> Result<(), String> {
        // Check if agent can be terminated
        let agent = self.agent(agent_id).await?;

        // Check if any agents depend on this one
        if !agent.dependents.is_empty() {
            return Err(format!(
                "Cannot terminate agent {}: {} agents depend on it",
                agent_id,
                agent.dependents.len()
            ));
        }

        // Special case: if agent is in Creating state, move to Error first
        if agent.state == AgentState::Creating {
            self.transition_state(agent_id, AgentState::Error).await?;
        }

        // Transition to Terminating
        self.transition_state(agent_id, AgentState::Terminating)
            .await?;

        // Simulate graceful shutdown (in real system, would send signal and wait)
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Transition to Terminated
        self.transition_state(agent_id, AgentState::Terminated)
            .await?;

        // Update dependencies
        if self.config.enable_dependency_tracking {
            self.remove_dependencies(agent_id).await?;
        }

        // Call terminate hook
        let agent = self.agent(agent_id).await?;
        self.hooks.on_terminate(&agent)?;

        // Record event
        self.record_event(LifecycleEvent {
            agent_id: agent_id.to_string(),
            event_type: LifecycleEventType::Terminated,
            previous_state: AgentState::Terminating,
            new_state: AgentState::Terminated,
            timestamp: Utc::now(),
            reason: Some("Agent terminated by request".to_string()),
            metadata: HashMap::new(),
        })
        .await;

        Ok(())
    }

    /// Update agent dependencies
    async fn update_dependencies(
        &self,
        agent_id: &str,
        dependencies: &[String],
    ) -> Result<(), String> {
        let mut agents = self.agents.write().await;

        // Add this agent as dependent to its dependencies
        for dep_id in dependencies {
            if let Some(dep_agent) = agents.get_mut(dep_id) {
                dep_agent.dependents.push(agent_id.to_string());
            }
        }

        Ok(())
    }

    /// Remove agent from dependencies
    async fn remove_dependencies(&self, agent_id: &str) -> Result<(), String> {
        let mut agents = self.agents.write().await;

        // Remove this agent from dependencies' dependent lists
        for agent in agents.values_mut() {
            agent.dependents.retain(|id| id != agent_id);
        }

        Ok(())
    }

    /// Get all agents
    pub async fn all_agents(&self) -> Vec<AgentInstance> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// Get agents by role
    pub async fn agents_by_role(&self, role: AgentRole) -> Vec<AgentInstance> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| a.role == role)
            .cloned()
            .collect()
    }

    /// Get agents by state
    pub async fn agents_by_state(&self, state: AgentState) -> Vec<AgentInstance> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| a.state == state)
            .cloned()
            .collect()
    }

    /// Get ready agents
    pub async fn ready_agents(&self) -> Vec<AgentInstance> {
        self.agents_by_state(AgentState::Ready).await
    }

    /// Get lifecycle statistics
    pub async fn lifecycle_statistics(&self) -> LifecycleStatistics {
        let agents = self.agents.read().await;

        let by_state = {
            let mut stats = HashMap::new();
            for agent in agents.values() {
                let count = stats.entry(agent.state).or_insert(0);
                *count += 1;
            }
            stats
        };

        let by_role = {
            let mut stats = HashMap::new();
            for agent in agents.values() {
                let count = stats.entry(agent.role).or_insert(0);
                *count += 1;
            }
            stats
        };

        let total_agents = agents.len();
        let active_agents = agents
            .values()
            .filter(|a| matches!(a.state, AgentState::Ready | AgentState::Busy))
            .count();

        let avg_tasks_completed = if total_agents > 0 {
            agents.values().map(|a| a.tasks_completed).sum::<u64>() as f64 / total_agents as f64
        } else {
            0.0
        };

        LifecycleStatistics {
            total_agents,
            active_agents,
            by_state,
            by_role,
            avg_tasks_completed,
            total_uptime_seconds: agents
                .values()
                .filter_map(AgentInstance::uptime_seconds)
                .sum::<i64>() as u64,
        }
    }

    /// Clean up idle agents
    pub async fn cleanup_idle_agents(&self) -> Result<usize, String> {
        let idle_timeout = chrono::Duration::seconds(self.config.agent_idle_timeout_seconds as i64);
        let now = Utc::now();

        let agents_to_cleanup: Vec<String> = {
            let agents = self.agents.read().await;
            agents
                .values()
                .filter(|a| a.state == AgentState::Ready && now - a.last_activity > idle_timeout)
                .map(|a| a.id.clone())
                .collect()
        };

        let mut cleaned = 0;
        for agent_id in agents_to_cleanup {
            if self.terminate_agent(&agent_id).await.is_ok() {
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }

    /// Restart a failed agent
    pub async fn restart_agent(&self, agent_id: &str) -> Result<(), String> {
        let agent = self.agent(agent_id).await?;

        if agent.state != AgentState::Error {
            return Err(format!("Agent {} is not in Error state", agent_id));
        }

        // Check restart count
        let restart_count = agent
            .metadata
            .get("restart_count")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        if restart_count >= self.config.max_restart_attempts {
            return Err(format!(
                "Agent {} has exceeded max restart attempts ({})",
                agent_id, self.config.max_restart_attempts
            ));
        }

        // Record restart event
        self.record_event(LifecycleEvent {
            agent_id: agent_id.to_string(),
            event_type: LifecycleEventType::Restarted,
            previous_state: AgentState::Error,
            new_state: AgentState::Initializing,
            timestamp: Utc::now(),
            reason: Some(format!("Restart attempt {}", restart_count + 1)),
            metadata: HashMap::new(),
        })
        .await;

        // Re-initialize (initialize_agent will handle the transitions)
        self.initialize_agent(agent_id).await?;

        // Update restart count
        {
            let mut agents = self.agents.write().await;
            if let Some(agent) = agents.get_mut(agent_id) {
                agent
                    .metadata
                    .insert("restart_count".to_string(), (restart_count + 1).to_string());
            }
        }

        Ok(())
    }

    /// Get lifecycle events
    pub async fn lifecycle_events(&self, agent_id: Option<&str>) -> Vec<LifecycleEvent> {
        let events = self.lifecycle_events.read().await;
        if let Some(id) = agent_id {
            events
                .iter()
                .filter(|e| e.agent_id == id)
                .cloned()
                .collect()
        } else {
            events.clone()
        }
    }

    /// Record lifecycle event
    async fn record_event(&self, event: LifecycleEvent) {
        let mut events = self.lifecycle_events.write().await;
        events.push(event);
    }
}

/// Lifecycle statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleStatistics {
    pub total_agents: usize,
    pub active_agents: usize,
    pub by_state: HashMap<AgentState, usize>,
    pub by_role: HashMap<AgentRole, usize>,
    pub avg_tasks_completed: f64,
    pub total_uptime_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        let agent = AgentInstance {
            id: "test".to_string(),
            name: "Test".to_string(),
            role: AgentRole::Reviewer,
            state: AgentState::Creating,
            pid: None,
            created_at: Utc::now(),
            initialized_at: None,
            last_activity: Utc::now(),
            tasks_completed: 0,
            tasks_failed: 0,
            total_runtime_ms: 0,
            dependencies: vec![],
            dependents: vec![],
            metadata: HashMap::new(),
        };

        assert!(agent.can_transition_to(AgentState::Initializing));
        assert!(!agent.can_transition_to(AgentState::Terminated));
    }

    #[test]
    fn test_success_rate() {
        let agent = AgentInstance {
            id: "test".to_string(),
            name: "Test".to_string(),
            role: AgentRole::Reviewer,
            state: AgentState::Ready,
            pid: None,
            created_at: Utc::now(),
            initialized_at: Some(Utc::now()),
            last_activity: Utc::now(),
            tasks_completed: 80,
            tasks_failed: 20,
            total_runtime_ms: 1000,
            dependencies: vec![],
            dependents: vec![],
            metadata: HashMap::new(),
        };

        assert_eq!(agent.success_rate(), 0.8);
    }

    #[tokio::test]
    async fn test_create_agent() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());

        let agent_id = manager
            .create_agent("test_agent".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        let agent = manager.agent(&agent_id).await.unwrap();
        assert_eq!(agent.state, AgentState::Creating);
        assert_eq!(agent.name, "test_agent");
    }

    #[tokio::test]
    async fn test_initialize_agent() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());

        let agent_id = manager
            .create_agent("test".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        manager.initialize_agent(&agent_id).await.unwrap();

        let agent = manager.agent(&agent_id).await.unwrap();
        assert_eq!(agent.state, AgentState::Ready);
        assert!(agent.initialized_at.is_some());
    }

    #[tokio::test]
    async fn test_suspend_resume() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());

        let agent_id = manager
            .create_agent("test".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        manager.initialize_agent(&agent_id).await.unwrap();
        manager.suspend_agent(&agent_id).await.unwrap();

        let agent = manager.agent(&agent_id).await.unwrap();
        assert_eq!(agent.state, AgentState::Suspended);

        manager.resume_agent(&agent_id).await.unwrap();

        let agent = manager.agent(&agent_id).await.unwrap();
        assert_eq!(agent.state, AgentState::Ready);
    }

    #[tokio::test]
    async fn test_terminate_agent() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());

        let agent_id = manager
            .create_agent("test".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        manager.terminate_agent(&agent_id).await.unwrap();

        let agent = manager.agent(&agent_id).await.unwrap();
        assert_eq!(agent.state, AgentState::Terminated);
    }

    #[tokio::test]
    async fn test_lifecycle_statistics() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());

        // Create some agents
        for i in 0..3 {
            let agent_id = manager
                .create_agent(format!("agent_{}", i), AgentRole::Reviewer, vec![])
                .await
                .unwrap();

            manager.initialize_agent(&agent_id).await.unwrap();
        }

        let stats = manager.get_lifecycle_statistics().await;
        assert_eq!(stats.total_agents, 3);
        assert_eq!(stats.active_agents, 3);
    }

    // --- AgentState Display & serde tests ---

    #[test]
    fn agent_state_display_all_variants() {
        assert_eq!(AgentState::Creating.to_string(), "Creating");
        assert_eq!(AgentState::Initializing.to_string(), "Initializing");
        assert_eq!(AgentState::Ready.to_string(), "Ready");
        assert_eq!(AgentState::Busy.to_string(), "Busy");
        assert_eq!(AgentState::Suspended.to_string(), "Suspended");
        assert_eq!(AgentState::Terminating.to_string(), "Terminating");
        assert_eq!(AgentState::Terminated.to_string(), "Terminated");
        assert_eq!(AgentState::Error.to_string(), "Error");
    }

    #[test]
    fn agent_state_serde_roundtrip() {
        let states = [
            AgentState::Creating,
            AgentState::Initializing,
            AgentState::Ready,
            AgentState::Busy,
            AgentState::Suspended,
            AgentState::Terminating,
            AgentState::Terminated,
            AgentState::Error,
        ];
        for state in &states {
            let json = serde_json::to_string(state).unwrap();
            let decoded: AgentState = serde_json::from_str(&json).unwrap();
            assert_eq!(*state, decoded);
        }
    }

    #[test]
    fn agent_state_equality_and_hash() {
        use std::collections::HashSet;
        let set: HashSet<AgentState> = [AgentState::Ready, AgentState::Ready, AgentState::Busy]
            .into_iter()
            .collect();
        assert_eq!(set.len(), 2);
    }

    // --- VALID_TRANSITIONS coverage ---

    #[test]
    fn valid_transitions_from_creating() {
        let agent = make_agent(AgentState::Creating);
        assert!(agent.can_transition_to(AgentState::Initializing));
        assert!(agent.can_transition_to(AgentState::Error));
        assert!(!agent.can_transition_to(AgentState::Ready));
        assert!(!agent.can_transition_to(AgentState::Terminated));
    }

    #[test]
    fn valid_transitions_from_initializing() {
        let agent = make_agent(AgentState::Initializing);
        assert!(agent.can_transition_to(AgentState::Ready));
        assert!(agent.can_transition_to(AgentState::Error));
        assert!(!agent.can_transition_to(AgentState::Creating));
    }

    #[test]
    fn valid_transitions_from_ready() {
        let agent = make_agent(AgentState::Ready);
        assert!(agent.can_transition_to(AgentState::Busy));
        assert!(agent.can_transition_to(AgentState::Suspended));
        assert!(agent.can_transition_to(AgentState::Terminating));
        assert!(agent.can_transition_to(AgentState::Error));
        assert!(!agent.can_transition_to(AgentState::Creating));
    }

    #[test]
    fn valid_transitions_from_busy() {
        let agent = make_agent(AgentState::Busy);
        assert!(agent.can_transition_to(AgentState::Ready));
        assert!(agent.can_transition_to(AgentState::Error));
        assert!(agent.can_transition_to(AgentState::Terminating));
        assert!(!agent.can_transition_to(AgentState::Suspended));
    }

    #[test]
    fn valid_transitions_from_suspended() {
        let agent = make_agent(AgentState::Suspended);
        assert!(agent.can_transition_to(AgentState::Ready));
        assert!(agent.can_transition_to(AgentState::Terminating));
        assert!(agent.can_transition_to(AgentState::Error));
        assert!(!agent.can_transition_to(AgentState::Busy));
    }

    #[test]
    fn valid_transitions_from_error() {
        let agent = make_agent(AgentState::Error);
        assert!(agent.can_transition_to(AgentState::Initializing));
        assert!(agent.can_transition_to(AgentState::Terminating));
        assert!(!agent.can_transition_to(AgentState::Ready));
    }

    #[test]
    fn valid_transitions_from_terminating() {
        let agent = make_agent(AgentState::Terminating);
        assert!(agent.can_transition_to(AgentState::Terminated));
        assert!(!agent.can_transition_to(AgentState::Ready));
    }

    #[test]
    fn terminated_is_dead_end() {
        let agent = make_agent(AgentState::Terminated);
        // No transitions from Terminated
        assert!(!agent.can_transition_to(AgentState::Creating));
        assert!(!agent.can_transition_to(AgentState::Ready));
        assert!(!agent.can_transition_to(AgentState::Error));
    }

    #[test]
    fn valid_transitions_count() {
        assert_eq!(VALID_TRANSITIONS.len(), 17);
    }

    // --- AgentInstance method tests ---

    #[test]
    fn age_seconds_non_negative() {
        let agent = make_agent(AgentState::Ready);
        assert!(agent.age_seconds() >= 0);
    }

    #[test]
    fn uptime_seconds_none_when_uninitialized() {
        let agent = AgentInstance {
            initialized_at: None,
            ..make_agent(AgentState::Creating)
        };
        assert!(agent.uptime_seconds().is_none());
    }

    #[test]
    fn uptime_seconds_some_when_initialized() {
        let agent = AgentInstance {
            initialized_at: Some(Utc::now()),
            ..make_agent(AgentState::Ready)
        };
        assert!(agent.uptime_seconds().is_some());
        assert!(agent.uptime_seconds().unwrap() >= 0);
    }

    #[test]
    fn success_rate_zero_when_no_tasks() {
        let agent = make_agent(AgentState::Ready);
        assert_eq!(agent.success_rate(), 0.0);
    }

    #[test]
    fn success_rate_perfect() {
        let mut agent = make_agent(AgentState::Ready);
        agent.tasks_completed = 100;
        agent.tasks_failed = 0;
        assert!((agent.success_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn success_rate_partial() {
        let mut agent = make_agent(AgentState::Ready);
        agent.tasks_completed = 3;
        agent.tasks_failed = 1;
        assert!((agent.success_rate() - 0.75).abs() < 0.001);
    }

    #[test]
    fn agent_instance_serde_roundtrip() {
        let agent = AgentInstance {
            id: "agent_42".to_string(),
            name: "TestAgent".to_string(),
            role: AgentRole::Reviewer,
            state: AgentState::Ready,
            pid: Some(1234),
            created_at: Utc::now(),
            initialized_at: Some(Utc::now()),
            last_activity: Utc::now(),
            tasks_completed: 50,
            tasks_failed: 5,
            total_runtime_ms: 30000,
            dependencies: vec!["agent_1".to_string()],
            dependents: vec!["agent_99".to_string()],
            metadata: {
                let mut m = HashMap::new();
                m.insert("key".to_string(), "value".to_string());
                m
            },
        };
        let json = serde_json::to_string(&agent).unwrap();
        let decoded: AgentInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, agent.id);
        assert_eq!(decoded.name, agent.name);
        assert_eq!(decoded.state, agent.state);
        assert_eq!(decoded.tasks_completed, 50);
        assert_eq!(decoded.tasks_failed, 5);
        assert_eq!(decoded.dependencies.len(), 1);
        assert_eq!(decoded.metadata.get("key").unwrap(), "value");
    }

    // --- LifecycleConfig tests ---

    #[test]
    fn lifecycle_config_default_values() {
        let config = LifecycleConfig::default();
        assert_eq!(config.max_agents_per_role, 10);
        assert_eq!(config.agent_idle_timeout_seconds, 300);
        assert_eq!(config.agent_startup_timeout_seconds, 60);
        assert!(config.enable_auto_restart);
        assert_eq!(config.max_restart_attempts, 3);
        assert_eq!(config.graceful_shutdown_timeout_seconds, 30);
        assert!(config.enable_dependency_tracking);
        assert!(config.cleanup_orphaned_agents);
    }

    #[test]
    fn lifecycle_config_serde_roundtrip() {
        let config = LifecycleConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: LifecycleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_agents_per_role, config.max_agents_per_role);
        assert_eq!(decoded.enable_auto_restart, config.enable_auto_restart);
        assert_eq!(decoded.max_restart_attempts, config.max_restart_attempts);
    }

    // --- LifecycleEvent & LifecycleEventType tests ---

    #[test]
    fn lifecycle_event_type_serde_roundtrip() {
        let types = [
            LifecycleEventType::Created,
            LifecycleEventType::Initialized,
            LifecycleEventType::StateChanged,
            LifecycleEventType::Suspended,
            LifecycleEventType::Resumed,
            LifecycleEventType::Terminated,
            LifecycleEventType::ErrorOccurred,
            LifecycleEventType::Restarted,
        ];
        for et in &types {
            let json = serde_json::to_string(et).unwrap();
            let decoded: LifecycleEventType = serde_json::from_str(&json).unwrap();
            assert_eq!(*et, decoded);
        }
    }

    #[test]
    fn lifecycle_event_serde_roundtrip() {
        let event = LifecycleEvent {
            agent_id: "agent_1".to_string(),
            event_type: LifecycleEventType::StateChanged,
            previous_state: AgentState::Ready,
            new_state: AgentState::Busy,
            timestamp: Utc::now(),
            reason: Some("Task assigned".to_string()),
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: LifecycleEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_id, event.agent_id);
        assert_eq!(decoded.event_type, LifecycleEventType::StateChanged);
        assert_eq!(decoded.previous_state, AgentState::Ready);
        assert_eq!(decoded.new_state, AgentState::Busy);
    }

    #[test]
    fn lifecycle_event_with_none_reason() {
        let event = LifecycleEvent {
            agent_id: "a".to_string(),
            event_type: LifecycleEventType::StateChanged,
            previous_state: AgentState::Busy,
            new_state: AgentState::Ready,
            timestamp: Utc::now(),
            reason: None,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: LifecycleEvent = serde_json::from_str(&json).unwrap();
        assert!(decoded.reason.is_none());
    }

    // --- LifecycleStatistics serde ---

    #[test]
    fn lifecycle_statistics_serde_roundtrip() {
        let stats = LifecycleStatistics {
            total_agents: 5,
            active_agents: 3,
            by_state: {
                let mut m = HashMap::new();
                m.insert(AgentState::Ready, 3);
                m.insert(AgentState::Busy, 2);
                m
            },
            by_role: {
                let mut m = HashMap::new();
                m.insert(AgentRole::Reviewer, 5);
                m
            },
            avg_tasks_completed: 12.5,
            total_uptime_seconds: 3600,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: LifecycleStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_agents, 5);
        assert_eq!(decoded.active_agents, 3);
        assert_eq!(decoded.by_state.len(), 2);
        assert!((decoded.avg_tasks_completed - 12.5).abs() < 0.001);
    }

    // --- Manager: get_agent not found ---

    #[tokio::test]
    async fn test_get_agent_not_found() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());
        let result = manager.agent("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // --- Manager: max agents per role ---

    #[tokio::test]
    async fn test_max_agents_per_role() {
        let config = LifecycleConfig {
            max_agents_per_role: 2,
            ..LifecycleConfig::default()
        };
        let manager = AgentLifecycleManager::new(config);

        // Create 2 agents (should succeed)
        let a1 = manager
            .create_agent("a1".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();
        let a2 = manager
            .create_agent("a2".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        // Third agent should fail
        let result = manager
            .create_agent("a3".to_string(), AgentRole::Reviewer, vec![])
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum agents"));

        // Different role should still work
        let a4 = manager
            .create_agent("a4".to_string(), AgentRole::Skeptic, vec![])
            .await
            .unwrap();
        assert!(!a4.is_empty());
        drop(a1);
        drop(a2);
        drop(a4);
    }

    // --- Manager: invalid transitions ---

    #[tokio::test]
    async fn test_invalid_transition_blocked() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());
        let agent_id = manager
            .create_agent("test".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        // Agent is in Creating state, cannot go directly to Ready
        let result = manager.transition_state(&agent_id, AgentState::Ready).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid state transition"));
    }

    // --- Manager: get_all_agents ---

    #[tokio::test]
    async fn test_get_all_agents() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());
        assert!(manager.get_all_agents().await.is_empty());

        manager
            .create_agent("a1".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();
        manager
            .create_agent("a2".to_string(), AgentRole::Skeptic, vec![])
            .await
            .unwrap();

        let all = manager.get_all_agents().await;
        assert_eq!(all.len(), 2);
    }

    // --- Manager: get_agents_by_role ---

    #[tokio::test]
    async fn test_get_agents_by_role() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());
        manager
            .create_agent("a1".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();
        manager
            .create_agent("a2".to_string(), AgentRole::Skeptic, vec![])
            .await
            .unwrap();
        manager
            .create_agent("a3".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        let seniors = manager.get_agents_by_role(AgentRole::Reviewer).await;
        assert_eq!(seniors.len(), 2);

        let juniors = manager.get_agents_by_role(AgentRole::Skeptic).await;
        assert_eq!(juniors.len(), 1);
    }

    // --- Manager: get_agents_by_state ---

    #[tokio::test]
    async fn test_get_agents_by_state() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());
        let a1 = manager
            .create_agent("a1".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();
        manager.initialize_agent(&a1).await.unwrap();

        let creating = manager.agents_by_state(AgentState::Creating).await;
        let ready = manager.agents_by_state(AgentState::Ready).await;
        assert!(creating.is_empty());
        assert_eq!(ready.len(), 1);
    }

    // --- Manager: get_ready_agents ---

    #[tokio::test]
    async fn test_get_ready_agents() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());
        assert!(manager.get_ready_agents().await.is_empty());

        let a1 = manager
            .create_agent("a1".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();
        manager.initialize_agent(&a1).await.unwrap();
        assert_eq!(manager.get_ready_agents().await.len(), 1);
    }

    // --- Manager: lifecycle events tracking ---

    #[tokio::test]
    async fn test_lifecycle_events_tracking() {
        let manager = AgentLifecycleManager::new(LifecycleConfig::default());
        let agent_id = manager
            .create_agent("test".to_string(), AgentRole::Reviewer, vec![])
            .await
            .unwrap();

        // Created event
        let events = manager.get_lifecycle_events(None).await;
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| e.agent_id == agent_id));

        // Filter by agent_id
        let agent_events = manager.get_lifecycle_events(Some(&agent_id)).await;
        assert!(agent_events.iter().all(|e| e.agent_id == agent_id));
    }

    // --- DefaultLifecycleHooks ---

    #[test]
    fn default_lifecycle_hooks_all_ok() {
        let hooks = DefaultLifecycleHooks;
        let agent = make_agent(AgentState::Creating);
        assert!(hooks.on_create(&agent).is_ok());
        assert!(hooks.on_init(&agent).is_ok());
        assert!(hooks
            .before_transition(&agent, AgentState::Initializing)
            .is_ok());
        assert!(hooks.after_transition(&agent, AgentState::Creating).is_ok());
        assert!(hooks.on_terminate(&agent).is_ok());
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for agent_lifecycle
    // =========================================================================

    // 1. AgentState clone equal
    #[test]
    fn agent_state_clone_equal() {
        let state = AgentState::Ready;
        let cloned = state;
        assert_eq!(state, cloned);
    }

    // 2. AgentState copy semantics
    #[test]
    fn agent_state_copy_semantics() {
        let a = AgentState::Busy;
        let b = a; // Copy, not move
        assert_eq!(a, b); // a still usable
    }

    // 3. AgentState debug format
    #[test]
    fn agent_state_debug_format() {
        let debug = format!("{:?}", AgentState::Creating);
        assert!(debug.contains("Creating"));
        let debug = format!("{:?}", AgentState::Error);
        assert!(debug.contains("Error"));
    }

    // 4. AgentInstance clone equal
    #[test]
    fn agent_instance_clone_equal() {
        let agent = AgentInstance {
            id: "clone-test".to_string(),
            name: "CloneAgent".to_string(),
            role: AgentRole::Skeptic,
            state: AgentState::Ready,
            pid: Some(9999),
            created_at: Utc::now(),
            initialized_at: Some(Utc::now()),
            last_activity: Utc::now(),
            tasks_completed: 10,
            tasks_failed: 2,
            total_runtime_ms: 5000,
            dependencies: vec!["dep1".to_string()],
            dependents: vec![],
            metadata: HashMap::new(),
        };
        let cloned = agent.clone();
        assert_eq!(cloned.id, agent.id);
        assert_eq!(cloned.name, agent.name);
        assert_eq!(cloned.state, agent.state);
        assert_eq!(cloned.pid, agent.pid);
        assert_eq!(cloned.tasks_completed, agent.tasks_completed);
    }

    // 5. AgentInstance debug format
    #[test]
    fn agent_instance_debug_format() {
        let agent = make_agent(AgentState::Ready);
        let debug = format!("{:?}", agent);
        assert!(debug.contains("test"));
        assert!(debug.contains("AgentInstance"));
    }

    // 6. LifecycleConfig clone equal
    #[test]
    fn lifecycle_config_clone_equal() {
        let config = LifecycleConfig {
            max_agents_per_role: 5,
            agent_idle_timeout_seconds: 120,
            agent_startup_timeout_seconds: 30,
            enable_auto_restart: false,
            max_restart_attempts: 1,
            graceful_shutdown_timeout_seconds: 10,
            enable_dependency_tracking: false,
            cleanup_orphaned_agents: false,
        };
        let cloned = config.clone();
        assert_eq!(cloned.max_agents_per_role, config.max_agents_per_role);
        assert_eq!(cloned.enable_auto_restart, config.enable_auto_restart);
        assert_eq!(cloned.max_restart_attempts, config.max_restart_attempts);
    }

    // 7. LifecycleConfig custom serde roundtrip
    #[test]
    fn lifecycle_config_custom_serde_roundtrip() {
        let config = LifecycleConfig {
            max_agents_per_role: 20,
            agent_idle_timeout_seconds: 600,
            agent_startup_timeout_seconds: 120,
            enable_auto_restart: false,
            max_restart_attempts: 0,
            graceful_shutdown_timeout_seconds: 60,
            enable_dependency_tracking: false,
            cleanup_orphaned_agents: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: LifecycleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_agents_per_role, 20);
        assert_eq!(decoded.agent_idle_timeout_seconds, 600);
        assert!(!decoded.enable_auto_restart);
        assert_eq!(decoded.max_restart_attempts, 0);
    }

    // 8. LifecycleConfig debug format
    #[test]
    fn lifecycle_config_debug_format() {
        let config = LifecycleConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("max_agents_per_role"));
        assert!(debug.contains("enable_auto_restart"));
    }

    // 9. LifecycleEvent clone equal
    #[test]
    fn lifecycle_event_clone_equal() {
        let event = LifecycleEvent {
            agent_id: "agent_1".to_string(),
            event_type: LifecycleEventType::Created,
            previous_state: AgentState::Creating,
            new_state: AgentState::Initializing,
            timestamp: Utc::now(),
            reason: Some("init".to_string()),
            metadata: HashMap::new(),
        };
        let cloned = event.clone();
        assert_eq!(cloned.agent_id, event.agent_id);
        assert_eq!(cloned.event_type, event.event_type);
        assert_eq!(cloned.reason, event.reason);
    }

    // 10. LifecycleEvent debug format
    #[test]
    fn lifecycle_event_debug_format() {
        let event = LifecycleEvent {
            agent_id: "debug_agent".to_string(),
            event_type: LifecycleEventType::ErrorOccurred,
            previous_state: AgentState::Ready,
            new_state: AgentState::Error,
            timestamp: Utc::now(),
            reason: None,
            metadata: HashMap::new(),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("debug_agent"));
        assert!(debug.contains("ErrorOccurred"));
    }

    // 11. LifecycleEventType debug format
    #[test]
    fn lifecycle_event_type_debug_format() {
        let debug = format!("{:?}", LifecycleEventType::Restarted);
        assert!(debug.contains("Restarted"));
        let debug = format!("{:?}", LifecycleEventType::Suspended);
        assert!(debug.contains("Suspended"));
    }

    // 12. LifecycleStatistics zero values serde
    #[test]
    fn lifecycle_statistics_zero_values_serde() {
        let stats = LifecycleStatistics {
            total_agents: 0,
            active_agents: 0,
            by_state: HashMap::new(),
            by_role: HashMap::new(),
            avg_tasks_completed: 0.0,
            total_uptime_seconds: 0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: LifecycleStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_agents, 0);
        assert!(decoded.by_state.is_empty());
        assert!((decoded.avg_tasks_completed - 0.0).abs() < f64::EPSILON);
    }

    // 13. LifecycleStatistics clone equal
    #[test]
    fn lifecycle_statistics_clone_equal() {
        let stats = LifecycleStatistics {
            total_agents: 10,
            active_agents: 7,
            by_state: HashMap::new(),
            by_role: HashMap::new(),
            avg_tasks_completed: 3.5,
            total_uptime_seconds: 7200,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.total_agents, stats.total_agents);
        assert_eq!(cloned.active_agents, stats.active_agents);
        assert!((cloned.avg_tasks_completed - stats.avg_tasks_completed).abs() < f64::EPSILON);
    }

    // 14. LifecycleStatistics debug format
    #[test]
    fn lifecycle_statistics_debug_format() {
        let stats = LifecycleStatistics {
            total_agents: 5,
            active_agents: 3,
            by_state: HashMap::new(),
            by_role: HashMap::new(),
            avg_tasks_completed: 2.0,
            total_uptime_seconds: 1000,
        };
        let debug = format!("{:?}", stats);
        assert!(debug.contains("total_agents"));
        assert!(debug.contains("active_agents"));
    }

    // 15. AgentInstance with dependencies and metadata serde
    #[test]
    fn agent_instance_with_deps_metadata_serde() {
        let agent = AgentInstance {
            id: "dep-agent".to_string(),
            name: "DepAgent".to_string(),
            role: AgentRole::Reviewer,
            state: AgentState::Busy,
            pid: Some(4242),
            created_at: Utc::now(),
            initialized_at: Some(Utc::now()),
            last_activity: Utc::now(),
            tasks_completed: 7,
            tasks_failed: 1,
            total_runtime_ms: 8800,
            dependencies: vec!["agent_1".to_string(), "agent_2".to_string()],
            dependents: vec!["agent_99".to_string()],
            metadata: {
                let mut m = HashMap::new();
                m.insert("version".to_string(), "1.0".to_string());
                m.insert("region".to_string(), "us-west".to_string());
                m
            },
        };
        let json = serde_json::to_string(&agent).unwrap();
        let decoded: AgentInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.dependencies.len(), 2);
        assert_eq!(decoded.dependents.len(), 1);
        assert_eq!(decoded.pid, Some(4242));
        assert_eq!(decoded.metadata.get("version").unwrap(), "1.0");
        assert_eq!(decoded.metadata.get("region").unwrap(), "us-west");
    }

    // Helper to create a minimal AgentInstance for unit tests
    fn make_agent(state: AgentState) -> AgentInstance {
        AgentInstance {
            id: "test".to_string(),
            name: "Test".to_string(),
            role: AgentRole::Reviewer,
            state,
            pid: None,
            created_at: Utc::now(),
            initialized_at: None,
            last_activity: Utc::now(),
            tasks_completed: 0,
            tasks_failed: 0,
            total_runtime_ms: 0,
            dependencies: vec![],
            dependents: vec![],
            metadata: HashMap::new(),
        }
    }
}
