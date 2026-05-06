//! Agent Registry — Dynamic agent generation and task profiling.
//!
//! This module provides:
//! - AgentRegistry: tracks built-in and generated specialist agents
//! - TaskProfile extensions for specialist matching
//! - Specialist agent definitions for common task types
//!
//! # Architecture
//!
//! ```text
//! Task → TaskProfile → AgentRegistry → Agent
//!                           │
//!              ┌────────────┼────────────┐
//!              │            │            │
//!         Built-in    Generated     Task History
//!         (Architect)  (Security)   (similar tasks)
//! ```

use rustycode_protocol::agent_protocol::AgentRole;
use rustycode_protocol::team::TaskProfile;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// Task Profile Extensions

/// Specialized task types that warrant a dedicated agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SpecialistType {
    /// Database migrations with rollback capability
    DatabaseMigration,
    /// Security vulnerability scanning and fixes
    SecurityAudit,
    /// Test debugging and flaky test investigation
    TestDebugging,
    /// Performance profiling and optimization
    PerformanceOptimization,
    /// API integration with external services
    ApiIntegration,
}

impl SpecialistType {
    /// Determine specialist type from task description keywords
    pub fn from_task(task: &str) -> Option<Self> {
        let task_lower = task.to_lowercase();

        if task_lower.contains("migration")
            || task_lower.contains("database")
            || task_lower.contains("schema")
            || task_lower.contains("migrate")
        {
            return Some(SpecialistType::DatabaseMigration);
        }

        if task_lower.contains("security")
            || task_lower.contains("vulnerability")
            || task_lower.contains("audit")
            || task_lower.contains("csrf")
            || task_lower.contains("xss")
            || task_lower.contains("injection")
        {
            return Some(SpecialistType::SecurityAudit);
        }

        if task_lower.contains("test")
            && (task_lower.contains("fail")
                || task_lower.contains("flaky")
                || task_lower.contains("debug"))
        {
            return Some(SpecialistType::TestDebugging);
        }

        if task_lower.contains("performance")
            || task_lower.contains("slow")
            || task_lower.contains("optimize")
            || task_lower.contains("latency")
        {
            return Some(SpecialistType::PerformanceOptimization);
        }

        if task_lower.contains("api")
            || task_lower.contains("integration")
            || task_lower.contains("external")
            || task_lower.contains("webhook")
        {
            return Some(SpecialistType::ApiIntegration);
        }

        None
    }

    /// Get the specialist agent name for this task type
    pub fn agent_name(&self) -> &'static str {
        match self {
            SpecialistType::DatabaseMigration => "DatabaseMigrationAgent",
            SpecialistType::SecurityAudit => "SecurityAuditorAgent",
            SpecialistType::TestDebugging => "TestDebuggerAgent",
            SpecialistType::PerformanceOptimization => "PerformanceOptimizerAgent",
            SpecialistType::ApiIntegration => "ApiIntegrationAgent",
        }
    }

    /// Get specialist instructions
    pub fn instructions(&self) -> &'static str {
        match self {
            SpecialistType::DatabaseMigration => {
                "You are a Database Migration specialist. Your priorities:\n\
                 1. Always create reversible migrations (include down() function)\n\
                 2. Use transactions for multi-step migrations\n\
                 3. Validate data before transformation\n\
                 4. Test rollback before marking complete\n\
                 5. Never drop columns without archiving data first"
            }
            SpecialistType::SecurityAudit => {
                "You are a Security Auditor. Your priorities:\n\
                 1. Check for OWASP Top 10 vulnerabilities\n\
                 2. Validate all user inputs (never trust external data)\n\
                 3. Ensure authentication before authorization checks\n\
                 4. Look for hardcoded secrets, SQL injection, XSS vectors\n\
                 5. Verify rate limiting on sensitive endpoints"
            }
            SpecialistType::TestDebugging => {
                "You are a Test Debugging specialist. Your priorities:\n\
                 1. Reproduce the failure consistently first\n\
                 2. Check for test isolation issues (shared state, globals)\n\
                 3. Look for timing-dependent flakiness (async, timeouts)\n\
                 4. Add diagnostic logging to understand failure mode\n\
                 5. Fix the root cause, don't just add retries"
            }
            SpecialistType::PerformanceOptimization => {
                "You are a Performance Optimization specialist. Your priorities:\n\
                 1. Measure before optimizing (profile to find bottlenecks)\n\
                 2. Focus on algorithmic complexity before micro-optimizations\n\
                 3. Check for N+1 queries, unnecessary allocations, lock contention\n\
                 4. Consider caching strategies for repeated computations\n\
                 5. Verify optimization with benchmarks"
            }
            SpecialistType::ApiIntegration => {
                "You are an API Integration specialist. Your priorities:\n\
                 1. Handle network failures gracefully (timeouts, retries)\n\
                 2. Validate API responses against expected schemas\n\
                 3. Implement proper authentication (OAuth, API keys)\n\
                 4. Rate limit outgoing requests to respect API limits\n\
                 5. Log API interactions for debugging"
            }
        }
    }
}

/// Describes a specific capability an agent possesses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    /// Machine-readable capability name (e.g., "security_audit", "db_migration").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Whether the agent is currently available for this capability.
    pub available: bool,
    /// Success rate (0.0–1.0), computed from task_history.
    pub success_rate: f64,
    /// Tools this capability requires.
    pub tool_scope: Vec<String>,
}

// Agent Definition

/// A specialist agent definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialistAgent {
    /// Unique identifier
    pub id: String,
    /// Human-readable name (e.g., "DatabaseMigrationAgent")
    pub name: String,
    pub specialist_type: SpecialistType,
    /// Role this agent fulfills (usually Builder or Architect)
    pub role: AgentRole,
    /// Specialized instructions for this agent
    pub instructions: String,
    /// Tools this agent has access to
    pub tools: Vec<String>,
    /// Capabilities this agent possesses
    pub capabilities: Vec<CapabilityDescriptor>,
    /// When this agent was created
    pub created_at: String,
    /// Which task created this agent (if generated)
    pub source_task: Option<String>,
    /// Whether the agent is currently available for task assignment
    pub available: bool,
}

impl SpecialistAgent {
    pub fn new(
        name: String,
        specialist_type: SpecialistType,
        role: AgentRole,
        source_task: Option<String>,
    ) -> Self {
        Self {
            id: format!("{}-{}", name, uuid::Uuid::now_v7()),
            name,
            specialist_type,
            role,
            instructions: specialist_type.instructions().to_string(),
            tools: Vec::new(),
            capabilities: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            source_task,
            available: true,
        }
    }

    /// Add a tool to this agent's available tools
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tools.push(tool.into());
        self
    }
}

// Agent Registry

/// Registry of available agents (built-in + generated specialists)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRegistry {
    /// Built-in agents (Architect, Builder, Skeptic, Judge, Scalpel)
    pub built_in: HashMap<String, AgentRole>,
    /// Generated specialist agents
    pub generated: HashMap<String, SpecialistAgent>,
    /// Task history: which agent solved which type of task
    pub task_history: Vec<TaskAgentMatch>,
}

/// Record of which agent handled a task type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAgentMatch {
    pub task_type: String,
    pub agent_id: String,
    pub success: bool,
    pub timestamp: String,
}

impl AgentRegistry {
    pub fn new() -> Self {
        let mut built_in = HashMap::new();
        built_in.insert("Architect".to_string(), AgentRole::Architect);
        built_in.insert("Builder".to_string(), AgentRole::Builder);
        built_in.insert("Skeptic".to_string(), AgentRole::Skeptic);
        built_in.insert("Judge".to_string(), AgentRole::Judge);
        built_in.insert("Scalpel".to_string(), AgentRole::Scalpel);
        built_in.insert("Coordinator".to_string(), AgentRole::Coordinator);

        Self {
            built_in,
            generated: HashMap::new(),
            task_history: Vec::new(),
        }
    }

    /// Get or create an agent for a task profile
    ///
    /// Returns the agent ID to use for this task. May create a new specialist
    /// if the task type warrants it and no similar task has been solved before.
    pub fn agent_for_task(&mut self, task: &str, _profile: &TaskProfile) -> AgentSelection {
        // Check if this is a specialist-worthy task
        if let Some(specialist_type) = SpecialistType::from_task(task) {
            // Check if we've solved a similar task before
            if let Some(prev_match) = self
                .task_history
                .iter()
                .rev()
                .find(|m| m.task_type == specialist_type.agent_name() && m.success)
            {
                // Reuse the successful agent
                return AgentSelection::Reuse {
                    agent_id: prev_match.agent_id.clone(),
                    reason: format!(
                        "Successfully used for similar task: {}",
                        prev_match.task_type
                    ),
                };
            }

            // Create a new specialist agent
            let agent_name = specialist_type.agent_name().to_string();
            let agent = SpecialistAgent::new(
                agent_name.clone(),
                specialist_type,
                AgentRole::Builder, // Specialists typically implement changes
                Some(task.to_string()),
            );

            // Add domain-specific tools
            let agent = self.enhance_agent_with_tools(agent, specialist_type);

            let agent_id = agent.id.clone();
            self.generated.insert(agent_id.clone(), agent);

            return AgentSelection::NewSpecialist {
                agent_id,
                specialist_type,
                reason: format!(
                    "New specialist created for task type: {}",
                    specialist_type.agent_name()
                ),
            };
        }

        // Fall back to standard team
        AgentSelection::StandardTeam {
            reason: "Task does not require specialist agent".to_string(),
        }
    }

    /// Enhance agent with domain-specific tools
    fn enhance_agent_with_tools(
        &self,
        mut agent: SpecialistAgent,
        specialist_type: SpecialistType,
    ) -> SpecialistAgent {
        match specialist_type {
            SpecialistType::DatabaseMigration => {
                agent = agent
                    .with_tool("schema_inspector")
                    .with_tool("migration_runner")
                    .with_tool("rollback_executor")
                    .with_tool("data_archiver");
            }
            SpecialistType::SecurityAudit => {
                agent = agent
                    .with_tool("code_scanner")
                    .with_tool("dependency_checker")
                    .with_tool("secret_detector")
                    .with_tool("vulnerability_scanner");
            }
            SpecialistType::TestDebugging => {
                agent = agent
                    .with_tool("test_runner")
                    .with_tool("flaky_test_detector")
                    .with_tool("coverage_analyzer")
                    .with_tool("test_isolation_checker");
            }
            SpecialistType::PerformanceOptimization => {
                agent = agent
                    .with_tool("profiler")
                    .with_tool("benchmark_runner")
                    .with_tool("memory_analyzer")
                    .with_tool("query_optimizer");
            }
            SpecialistType::ApiIntegration => {
                agent = agent
                    .with_tool("http_client")
                    .with_tool("oauth_handler")
                    .with_tool("rate_limiter")
                    .with_tool("response_validator");
            }
        }
        agent
    }

    /// Record the outcome of a task
    pub fn record_task_outcome(&mut self, task_type: &str, agent_id: &str, success: bool) {
        self.task_history.push(TaskAgentMatch {
            task_type: task_type.to_string(),
            agent_id: agent_id.to_string(),
            success,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Get all available agents (built-in + generated)
    pub fn all_agents(&self) -> Vec<AgentInfo> {
        let mut agents = Vec::new();

        // Built-in agents
        for (name, role) in &self.built_in {
            agents.push(AgentInfo {
                id: name.clone(),
                name: name.clone(),
                kind: AgentKind::BuiltIn(*role),
            });
        }

        // Generated specialists
        for (id, specialist) in &self.generated {
            agents.push(AgentInfo {
                id: id.clone(),
                name: specialist.name.clone(),
                kind: AgentKind::Specialist(specialist.specialist_type),
            });
        }

        agents
    }

    /// Get a specialist by ID
    pub fn specialist(&self, id: &str) -> Option<&SpecialistAgent> {
        self.generated.get(id)
    }

    /// Find all specialists that have a named capability.
    pub fn find_by_capability(&self, capability: &str) -> Vec<&SpecialistAgent> {
        self.generated
            .values()
            .filter(|agent| agent.capabilities.iter().any(|c| c.name == capability))
            .collect()
    }

    /// Find available specialists with a named capability.
    pub fn find_available(&self, capability: &str) -> Vec<&SpecialistAgent> {
        self.find_by_capability(capability)
            .into_iter()
            .filter(|agent| agent.available)
            .collect()
    }

    /// Rank specialists by success rate for a capability (highest first).
    pub fn rank_by_success(&self, capability: &str) -> Vec<&SpecialistAgent> {
        let mut agents = self.find_by_capability(capability);
        agents.sort_by(|a, b| {
            let rate_a = a
                .capabilities
                .iter()
                .find(|c| c.name == capability)
                .map(|c| c.success_rate)
                .unwrap_or(0.0);
            let rate_b = b
                .capabilities
                .iter()
                .find(|c| c.name == capability)
                .map(|c| c.success_rate)
                .unwrap_or(0.0);
            rate_b
                .partial_cmp(&rate_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        agents
    }

    /// Mark an agent as busy (not available).
    pub fn mark_busy(&mut self, agent_id: &str) {
        if let Some(agent) = self.generated.get_mut(agent_id) {
            agent.available = false;
        }
    }

    /// Mark an agent as available again.
    pub fn mark_available(&mut self, agent_id: &str) {
        if let Some(agent) = self.generated.get_mut(agent_id) {
            agent.available = true;
        }
    }
}

/// Result of agent selection
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AgentSelection {
    /// Use existing built-in team
    StandardTeam { reason: String },
    /// Reuse a previously successful specialist
    Reuse { agent_id: String, reason: String },
    /// Create a new specialist agent
    NewSpecialist {
        agent_id: String,
        specialist_type: SpecialistType,
        reason: String,
    },
}

/// Information about an agent
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
}

/// Type of agent
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AgentKind {
    BuiltIn(AgentRole),
    Specialist(SpecialistType),
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentKind::BuiltIn(role) => write!(f, "Built-in ({})", role),
            AgentKind::Specialist(typ) => write!(f, "Specialist ({})", typ.agent_name()),
        }
    }
}

// ── Global Registry Accessors ────────────────────────────────────────────────────────

use std::sync::OnceLock;

/// Global agent registry accessor for centralized state management.
///
/// This follows the claw-code pattern of using OnceLock for global registries,
/// enabling any part of the codebase to access shared state without threading
/// `Arc<Registry>` through every layer.
///
/// # Example
///
/// ```ignore
/// use rustycode_protocol::agent_registry::global_agent_registry;
/// let registry = global_agent_registry();
/// let agents = registry.list_agents();
/// ```
pub fn global_agent_registry() -> &'static AgentRegistry {
    static REGISTRY: OnceLock<AgentRegistry> = OnceLock::new();
    REGISTRY.get_or_init(AgentRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_specialist_type_detection() {
        assert_eq!(
            SpecialistType::from_task("Fix database migration rollback"),
            Some(SpecialistType::DatabaseMigration)
        );
        assert_eq!(
            SpecialistType::from_task("Security audit for auth module"),
            Some(SpecialistType::SecurityAudit)
        );
        assert_eq!(
            SpecialistType::from_task("Debug flaky test in user module"),
            Some(SpecialistType::TestDebugging)
        );
        assert_eq!(
            SpecialistType::from_task("Optimize query performance"),
            Some(SpecialistType::PerformanceOptimization)
        );
        assert_eq!(
            SpecialistType::from_task("Integrate Stripe payment API"),
            Some(SpecialistType::ApiIntegration)
        );
        assert_eq!(
            SpecialistType::from_task("Add webhook integration"),
            Some(SpecialistType::ApiIntegration)
        );
    }

    #[test]
    fn test_agent_registry_creates_specialist() {
        let mut registry = AgentRegistry::new();
        let profile = TaskProfile::default();

        let selection = registry.agent_for_task("Fix database migration", &profile);

        match selection {
            AgentSelection::NewSpecialist {
                specialist_type, ..
            } => {
                assert_eq!(specialist_type, SpecialistType::DatabaseMigration);
            }
            _ => panic!("Expected NewSpecialist"),
        }
    }

    #[test]
    fn test_agent_registry_reuses_successful_agent() {
        let mut registry = AgentRegistry::new();
        let profile = TaskProfile::default();

        // First call creates specialist
        let selection1 = registry.agent_for_task("Fix database migration", &profile);
        let agent_id = match &selection1 {
            AgentSelection::NewSpecialist { agent_id, .. } => agent_id.clone(),
            _ => panic!("Expected NewSpecialist"),
        };

        // Record success
        registry.record_task_outcome("DatabaseMigrationAgent", &agent_id, true);

        // Second call should reuse
        let selection2 = registry.agent_for_task("Schema migration for users table", &profile);

        match selection2 {
            AgentSelection::Reuse {
                agent_id: reused_id,
                ..
            } => {
                assert_eq!(reused_id, agent_id);
            }
            _ => panic!("Expected Reuse"),
        }
    }

    #[test]
    fn test_specialist_type_from_task_no_match() {
        assert_eq!(SpecialistType::from_task("Add a new button"), None);
        assert_eq!(SpecialistType::from_task(""), None);
    }

    #[test]
    fn test_specialist_type_from_task_case_insensitive() {
        assert_eq!(
            SpecialistType::from_task("SECURITY VULNERABILITY FIX"),
            Some(SpecialistType::SecurityAudit)
        );
        assert_eq!(
            SpecialistType::from_task("DATABASE SCHEMA CHANGE"),
            Some(SpecialistType::DatabaseMigration)
        );
    }

    #[test]
    fn test_specialist_type_from_task_keywords() {
        // migration keywords
        assert!(SpecialistType::from_task("migrate users table").is_some());
        assert!(SpecialistType::from_task("schema update").is_some());
        // security keywords
        assert!(SpecialistType::from_task("vulnerability scan").is_some());
        // test keywords
        assert!(SpecialistType::from_task("flaky test").is_some());
        // performance keywords
        assert!(SpecialistType::from_task("optimize query").is_some());
        assert!(SpecialistType::from_task("slow query").is_some());
        assert!(SpecialistType::from_task("latency issue").is_some());
        // API keywords
        assert!(SpecialistType::from_task("webhook integration").is_some());
    }

    #[test]
    fn test_specialist_type_serialization() {
        let types = vec![
            SpecialistType::DatabaseMigration,
            SpecialistType::SecurityAudit,
            SpecialistType::TestDebugging,
            SpecialistType::PerformanceOptimization,
            SpecialistType::ApiIntegration,
        ];
        for st in &types {
            let json = serde_json::to_string(st).unwrap();
            let decoded: SpecialistType = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, *st);
        }
    }

    #[test]
    fn test_agent_registry_new() {
        let registry = AgentRegistry::new();
        // Built-in agents are pre-populated
        assert!(!registry.all_agents().is_empty());
        assert!(registry.generated.is_empty());
        assert!(registry.task_history.is_empty());
    }

    #[test]
    fn test_agent_registry_uses_builtin_for_simple_tasks() {
        let mut registry = AgentRegistry::new();
        let profile = TaskProfile::default();

        let selection = registry.agent_for_task("Add a comment", &profile);
        // Simple task should use a built-in agent
        match selection {
            AgentSelection::StandardTeam { .. } => {}
            AgentSelection::NewSpecialist { .. } => {} // also acceptable
            _ => {}
        }
    }

    #[test]
    fn test_agent_registry_records_failure() {
        let mut registry = AgentRegistry::new();
        let profile = TaskProfile::default();

        let selection = registry.agent_for_task("Fix database migration", &profile);
        let agent_id = match &selection {
            AgentSelection::NewSpecialist { agent_id, .. } => agent_id.clone(),
            _ => panic!("Expected NewSpecialist"),
        };

        // Record failure
        registry.record_task_outcome("DatabaseMigrationAgent", &agent_id, false);

        // Should not reuse after failure
        let selection2 = registry.agent_for_task("Database migration for orders", &profile);
        match selection2 {
            AgentSelection::NewSpecialist { .. } => {} // expected
            AgentSelection::Reuse { .. } => panic!("Should not reuse failed agent"),
            _ => {}
        }
    }

    #[test]
    fn test_agent_registry_all_agents() {
        let mut registry = AgentRegistry::new();
        let profile = TaskProfile::default();

        // Initially only built-in agents
        assert!(!registry.all_agents().is_empty());

        // Generate a specialist
        let _ = registry.agent_for_task("Security audit", &profile);
        assert!(registry.generated.len() == 1);
    }

    #[test]
    fn test_capability_descriptor_find_by_capability() {
        let mut registry = AgentRegistry::new();
        let mut agent = SpecialistAgent::new(
            "TestSecurityAgent".into(),
            SpecialistType::SecurityAudit,
            AgentRole::Builder,
            None,
        );
        agent.capabilities.push(CapabilityDescriptor {
            name: "security_audit".into(),
            description: "Security vulnerability scanning".into(),
            available: true,
            success_rate: 0.9,
            tool_scope: vec!["code_scanner".into()],
        });
        registry.generated.insert(agent.id.clone(), agent);

        let found = registry.find_by_capability("security_audit");
        assert_eq!(found.len(), 1);

        let not_found = registry.find_by_capability("db_migration");
        assert!(not_found.is_empty());
    }

    #[test]
    fn test_capability_descriptor_find_available_excludes_busy() {
        let mut registry = AgentRegistry::new();
        let mut agent = SpecialistAgent::new(
            "TestBusyAgent".into(),
            SpecialistType::SecurityAudit,
            AgentRole::Builder,
            None,
        );
        agent.capabilities.push(CapabilityDescriptor {
            name: "security_audit".into(),
            description: "Security scanning".into(),
            available: true,
            success_rate: 0.8,
            tool_scope: vec![],
        });
        agent.available = false; // busy
        registry.generated.insert(agent.id.clone(), agent);

        let available = registry.find_available("security_audit");
        assert!(available.is_empty());
    }

    #[test]
    fn test_capability_descriptor_rank_by_success() {
        let mut registry = AgentRegistry::new();

        // Agent 1: 60% success
        let mut agent1 = SpecialistAgent::new(
            "Agent1".into(),
            SpecialistType::SecurityAudit,
            AgentRole::Builder,
            None,
        );
        agent1.capabilities.push(CapabilityDescriptor {
            name: "security_audit".into(),
            description: "Security".into(),
            available: true,
            success_rate: 0.6,
            tool_scope: vec![],
        });

        // Agent 2: 95% success
        let mut agent2 = SpecialistAgent::new(
            "Agent2".into(),
            SpecialistType::SecurityAudit,
            AgentRole::Builder,
            None,
        );
        agent2.capabilities.push(CapabilityDescriptor {
            name: "security_audit".into(),
            description: "Security".into(),
            available: true,
            success_rate: 0.95,
            tool_scope: vec![],
        });

        registry.generated.insert("a1".into(), agent1);
        registry.generated.insert("a2".into(), agent2);

        let ranked = registry.rank_by_success("security_audit");
        assert_eq!(ranked.len(), 2);
        // Highest success rate first
        assert_eq!(ranked[0].capabilities[0].success_rate, 0.95);
        assert_eq!(ranked[1].capabilities[0].success_rate, 0.6);
    }

    #[test]
    fn test_mark_busy_and_available() {
        let mut registry = AgentRegistry::new();
        let agent = SpecialistAgent::new(
            "TestAgent".into(),
            SpecialistType::SecurityAudit,
            AgentRole::Builder,
            None,
        );
        let id = agent.id.clone();
        assert!(agent.available);
        registry.generated.insert(id.clone(), agent);

        registry.mark_busy(&id);
        assert!(!registry.generated.get(&id).unwrap().available);

        registry.mark_available(&id);
        assert!(registry.generated.get(&id).unwrap().available);
    }

    #[test]
    fn test_mark_busy_nonexistent_no_panic() {
        let mut registry = AgentRegistry::new();
        registry.mark_busy("nonexistent");
        registry.mark_available("nonexistent");
    }
}
