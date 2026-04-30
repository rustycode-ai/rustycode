//! Event-Driven Agent Orchestration — Phase 4.
//!
//! This module implements the event engine for proactive agent coordination.
//! Instead of linear flows, agents react to events they subscribe to.
//!
//! # Architecture
//!
//! ```text
//! TeamEvent → EventEngine → dispatch() → AgentListener[] → AgentAction
//!                                       │
//!                                       ├─ Skeptic reviews on CodeChanged
//!                                       ├─ Scalpel fixes on CompilationFailed
//!                                       └─ SecurityAuditor on SecurityIssueDetected
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! let mut engine = EventEngine::new();
//!
//! // Register a listener
//! let skeptic_listener = AgentListener {
//!     agent_id: "Skeptic".to_string(),
//!     subscribes_to: vec![TeamEventType::CodeChanged],
//!     handler: |event| match event {
//!         TeamEvent::CodeChanged { files, .. } => AgentAction::ReviewCode(files),
//!         _ => AgentAction::Noop,
//!     },
//! };
//! engine.register_listener(skeptic_listener);
//!
//! // Emit an event
//! let actions = engine.dispatch(&TeamEvent::CodeChanged {
//!     files: vec!["src/auth.rs".to_string()],
//!     author: "Builder".to_string(),
//!     generation: 1,
//! });
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use super::orchestrator::TeamEvent;

// ============================================================================
// Event Type (for subscription matching)
// ============================================================================

/// Event types for subscription matching.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamEventType {
    // Core lifecycle events
    AgentActivated,
    AgentStateChanged,
    AgentDeactivated,
    StepCompleted,
    TaskCompleted,
    Insight,
    // Phase 4: Event-driven orchestration
    CodeChanged,
    CompilationFailed,
    TestsFailed,
    TrustChanged,
    VerificationPassed,
    PatternDiscovered,
    SecurityIssueDetected,
    StructuralDeclarationSet,
    PlanAdapted,
    SpecialistCreated,
    ParallelExecutionRequested,
}

impl TeamEventType {
    /// Extract event type from a TeamEvent.
    pub fn from_event(event: &TeamEvent) -> Self {
        match event {
            TeamEvent::AgentActivated { .. } => TeamEventType::AgentActivated,
            TeamEvent::AgentStateChanged { .. } => TeamEventType::AgentStateChanged,
            TeamEvent::AgentDeactivated { .. } => TeamEventType::AgentDeactivated,
            TeamEvent::StepCompleted { .. } => TeamEventType::StepCompleted,
            TeamEvent::TaskCompleted { .. } => TeamEventType::TaskCompleted,
            TeamEvent::Insight { .. } => TeamEventType::Insight,
            TeamEvent::CodeChanged { .. } => TeamEventType::CodeChanged,
            TeamEvent::CompilationFailed { .. } => TeamEventType::CompilationFailed,
            TeamEvent::TestsFailed { .. } => TeamEventType::TestsFailed,
            TeamEvent::TrustChanged { .. } => TeamEventType::TrustChanged,
            TeamEvent::VerificationPassed { .. } => TeamEventType::VerificationPassed,
            TeamEvent::PatternDiscovered { .. } => TeamEventType::PatternDiscovered,
            TeamEvent::SecurityIssueDetected { .. } => TeamEventType::SecurityIssueDetected,
            TeamEvent::StructuralDeclarationSet { .. } => TeamEventType::StructuralDeclarationSet,
            TeamEvent::PlanAdapted { .. } => TeamEventType::PlanAdapted,
            TeamEvent::SpecialistCreated { .. } => TeamEventType::SpecialistCreated,
            TeamEvent::ParallelExecutionRequested { .. } => {
                TeamEventType::ParallelExecutionRequested
            }
            TeamEvent::ToolStarted { .. }
            | TeamEvent::ToolCompleted { .. }
            | TeamEvent::ToolLoopIteration { .. }
            | TeamEvent::AdvisorGuidance { .. }
            | TeamEvent::LLMTextChunk { .. }
            | TeamEvent::LLMThinkingChunk { .. } => TeamEventType::AgentStateChanged,
        }
    }
}

// ============================================================================
// Agent Action (what an agent does in response to an event)
// ============================================================================

/// Action an agent should take in response to an event.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    /// No action needed.
    Noop,
    /// Review code changes.
    ReviewCode { files: Vec<String> },
    /// Fix compilation errors.
    FixCompilation { errors: String, files: Vec<String> },
    /// Debug failing tests.
    DebugTests { failed_tests: Vec<String> },
    /// Investigate security issue.
    InvestigateSecurity {
        severity: String,
        issue_type: String,
        location: String,
    },
    /// Perform security scan.
    SecurityScan { files: Vec<String> },
    /// Optimize performance.
    OptimizePerformance { files: Vec<String> },
    /// Run verification checks.
    RunVerification { check_type: String },
    /// Escalate to human.
    Escalate { reason: String },
    /// Log insight.
    LogInsight { message: String },
    /// Parallel execution requested.
    ParallelExecute { agents: Vec<String>, task: String },
}

// ============================================================================
// Agent Listener (subscribes to events, produces actions)
// ============================================================================

/// A listener that subscribes to specific event types and produces actions.
pub type EventHandler = dyn Fn(&TeamEvent) -> AgentAction + Send + Sync;

pub struct AgentListener {
    /// ID of the agent this listener belongs to.
    pub agent_id: String,
    /// Event types this listener subscribes to.
    pub subscribes_to: Vec<TeamEventType>,
    /// Handler function that produces actions.
    pub handler: Box<EventHandler>,
}

impl AgentListener {
    /// Create a new listener for a specific agent.
    pub fn new<F>(agent_id: String, subscribes_to: Vec<TeamEventType>, handler: F) -> Self
    where
        F: Fn(&TeamEvent) -> AgentAction + Send + Sync + 'static,
    {
        Self {
            agent_id,
            subscribes_to,
            handler: Box::new(handler),
        }
    }

    /// Check if this listener is interested in the given event.
    pub fn is_interested(&self, event: &TeamEvent) -> bool {
        let event_type = TeamEventType::from_event(event);
        self.subscribes_to.contains(&event_type)
    }

    /// Handle an event and produce an action.
    pub fn handle(&self, event: &TeamEvent) -> AgentAction {
        (self.handler)(event)
    }
}

// ============================================================================
// Event Engine (dispatches events to listeners)
// ============================================================================

/// Event engine that dispatches events to registered listeners.
pub struct EventEngine {
    /// Registered listeners.
    listeners: Vec<AgentListener>,
    /// Event history for debugging/auditing.
    event_history: Vec<TeamEvent>,
    /// Count of events dispatched per type.
    dispatch_counts: HashMap<TeamEventType, u32>,
}

impl EventEngine {
    /// Create a new event engine.
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
            event_history: Vec::new(),
            dispatch_counts: HashMap::new(),
        }
    }

    /// Register a listener.
    pub fn register_listener(&mut self, listener: AgentListener) {
        debug!(
            "Registering listener for agent {} (subscribes to {:?})",
            listener.agent_id, listener.subscribes_to
        );
        self.listeners.push(listener);
    }

    /// Unregister all listeners for an agent.
    pub fn unregister_agent(&mut self, agent_id: &str) {
        self.listeners.retain(|l| l.agent_id != agent_id);
        info!("Unregistered all listeners for agent {}", agent_id);
    }

    /// Dispatch an event to all interested listeners.
    ///
    /// Returns a list of (agent_id, action) pairs for each interested listener.
    /// Uses a Vec to support multiple subscriptions from the same agent.
    pub fn dispatch(&mut self, event: &TeamEvent) -> Vec<(String, AgentAction)> {
        let event_type = TeamEventType::from_event(event);
        debug!("Dispatching event {:?}", event_type);

        // Record event in history (bounded to last 256 events)
        if self.event_history.len() >= 256 {
            self.event_history.drain(0..self.event_history.len() - 192);
        }
        self.event_history.push(event.clone());

        // Update dispatch count
        *self.dispatch_counts.entry(event_type).or_insert(0) += 1;

        // Collect actions from interested listeners (Vec preserves multiple per agent)
        let mut actions = Vec::new();
        for listener in &self.listeners {
            if listener.is_interested(event) {
                let action = listener.handle(event);
                if !matches!(action, AgentAction::Noop) {
                    debug!(
                        "Agent {} taking action {:?} for event {:?}",
                        listener.agent_id, action, event_type
                    );
                    actions.push((listener.agent_id.clone(), action));
                }
            }
        }

        actions
    }

    /// Get the event history (bounded to last 256 events).
    pub fn event_history(&self) -> &[TeamEvent] {
        &self.event_history
    }

    /// Clear event history to free memory.
    pub fn clear_history(&mut self) {
        self.event_history.clear();
    }

    /// Get dispatch statistics.
    pub fn dispatch_stats(&self) -> &HashMap<TeamEventType, u32> {
        &self.dispatch_counts
    }

    /// Get the number of registered listeners.
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}

impl Default for EventEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Listeners (pre-configured for standard team)
// ============================================================================

impl EventEngine {
    /// Register built-in listeners for the standard team.
    pub fn register_standard_team(&mut self) {
        // Skeptic: reviews on code changes
        self.register_listener(AgentListener::new(
            "Skeptic".to_string(),
            vec![TeamEventType::CodeChanged],
            |event| match event {
                TeamEvent::CodeChanged { files, .. } => AgentAction::ReviewCode {
                    files: files.clone(),
                },
                _ => AgentAction::Noop,
            },
        ));

        // Scalpel: fixes compilation errors
        self.register_listener(AgentListener::new(
            "Scalpel".to_string(),
            vec![
                TeamEventType::CompilationFailed,
                TeamEventType::VerificationPassed,
            ],
            |event| match event {
                TeamEvent::CompilationFailed { errors, files, .. } => AgentAction::FixCompilation {
                    errors: errors.clone(),
                    files: files.clone(),
                },
                TeamEvent::VerificationPassed { check_type, .. } => {
                    if check_type == "compilation" {
                        AgentAction::LogInsight {
                            message: "Compilation verification passed".to_string(),
                        }
                    } else {
                        AgentAction::Noop
                    }
                }
                _ => AgentAction::Noop,
            },
        ));

        // TestDebugger: investigates failing tests
        self.register_listener(AgentListener::new(
            "TestDebugger".to_string(),
            vec![TeamEventType::TestsFailed],
            |event| match event {
                TeamEvent::TestsFailed {
                    failed_tests,
                    total_failed: _,
                    error_output: _,
                } => AgentAction::DebugTests {
                    failed_tests: failed_tests.clone(),
                },
                _ => AgentAction::Noop,
            },
        ));

        // SecurityAuditor: investigates security issues
        self.register_listener(AgentListener::new(
            "SecurityAuditor".to_string(),
            vec![
                TeamEventType::SecurityIssueDetected,
                TeamEventType::CodeChanged,
            ],
            |event| match event {
                TeamEvent::SecurityIssueDetected {
                    severity,
                    issue_type,
                    location,
                    ..
                } => AgentAction::InvestigateSecurity {
                    severity: severity.clone(),
                    issue_type: issue_type.clone(),
                    location: location.clone(),
                },
                TeamEvent::CodeChanged { files, .. } => AgentAction::SecurityScan {
                    files: files.clone(),
                },
                _ => AgentAction::Noop,
            },
        ));

        // PerformanceOptimizer: runs on verification passed
        self.register_listener(AgentListener::new(
            "PerformanceOptimizer".to_string(),
            vec![TeamEventType::VerificationPassed],
            |event| match event {
                TeamEvent::VerificationPassed { check_type, .. } => AgentAction::RunVerification {
                    check_type: check_type.clone(),
                },
                _ => AgentAction::Noop,
            },
        ));

        info!("Registered standard team listeners");
    }
}

// ============================================================================
// Integration with TeamOrchestrator
// ============================================================================

/// Extension trait for TeamOrchestrator to integrate with EventEngine.
pub trait EventDrivenOrchestrator {
    /// Get a reference to the event engine.
    fn event_engine(&self) -> &EventEngine;
    /// Get a mutable reference to the event engine.
    fn event_engine_mut(&mut self) -> &mut EventEngine;
    /// Emit an event and dispatch to listeners.
    fn emit_and_dispatch(&mut self, event: TeamEvent) -> Vec<(String, AgentAction)>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_from_event() {
        let event = TeamEvent::CodeChanged {
            files: vec!["src/main.rs".to_string()],
            author: "Builder".to_string(),
            generation: 1,
        };
        assert_eq!(
            TeamEventType::from_event(&event),
            TeamEventType::CodeChanged
        );

        let event = TeamEvent::CompilationFailed {
            errors: "error[E0308]: mismatched types".to_string(),
            files: vec!["src/lib.rs".to_string()],
            severity: "error".to_string(),
        };
        assert_eq!(
            TeamEventType::from_event(&event),
            TeamEventType::CompilationFailed
        );
    }

    #[test]
    fn test_listener_subscription() {
        let listener = AgentListener::new(
            "Skeptic".to_string(),
            vec![TeamEventType::CodeChanged],
            |_| AgentAction::Noop,
        );

        let code_event = TeamEvent::CodeChanged {
            files: vec!["src/main.rs".to_string()],
            author: "Builder".to_string(),
            generation: 1,
        };
        assert!(listener.is_interested(&code_event));

        let compile_event = TeamEvent::CompilationFailed {
            errors: "error".to_string(),
            files: vec![],
            severity: "error".to_string(),
        };
        assert!(!listener.is_interested(&compile_event));
    }

    #[test]
    fn test_event_engine_dispatch() {
        let mut engine = EventEngine::new();

        // Register a listener
        engine.register_listener(AgentListener::new(
            "Skeptic".to_string(),
            vec![TeamEventType::CodeChanged],
            |event| match event {
                TeamEvent::CodeChanged { files, .. } => AgentAction::ReviewCode {
                    files: files.clone(),
                },
                _ => AgentAction::Noop,
            },
        ));

        // Dispatch an event
        let event = TeamEvent::CodeChanged {
            files: vec!["src/main.rs".to_string()],
            author: "Builder".to_string(),
            generation: 1,
        };
        let actions = engine.dispatch(&event);

        assert_eq!(actions.len(), 1);
        let skeptic_action = actions.iter().find(|(id, _)| id == "Skeptic");
        assert!(skeptic_action.is_some());
        match &skeptic_action.unwrap().1 {
            AgentAction::ReviewCode { files } => {
                assert_eq!(files, &vec!["src/main.rs".to_string()]);
            }
            _ => panic!("Expected ReviewCode action"),
        }
    }

    #[test]
    fn test_event_engine_noop_filtering() {
        let mut engine = EventEngine::new();

        // Register a listener that always returns Noop
        engine.register_listener(AgentListener::new(
            "Judge".to_string(),
            vec![TeamEventType::CodeChanged],
            |_| AgentAction::Noop,
        ));

        // Dispatch an event
        let event = TeamEvent::CodeChanged {
            files: vec![],
            author: "Builder".to_string(),
            generation: 1,
        };
        let actions = engine.dispatch(&event);

        // Noop actions should be filtered out
        assert!(actions.is_empty());
    }

    #[test]
    fn test_standard_team_registration() {
        let mut engine = EventEngine::new();
        engine.register_standard_team();

        assert!(engine.listener_count() >= 4); // Skeptic, Scalpel, TestDebugger, SecurityAuditor

        // Test Skeptic subscription
        let code_event = TeamEvent::CodeChanged {
            files: vec!["src/main.rs".to_string()],
            author: "Builder".to_string(),
            generation: 1,
        };
        let actions = engine.dispatch(&code_event);
        assert!(actions.iter().any(|(id, _)| id == "Skeptic"));
    }

    #[test]
    fn test_event_history() {
        let mut engine = EventEngine::new();

        let event1 = TeamEvent::CodeChanged {
            files: vec![],
            author: "Builder".to_string(),
            generation: 1,
        };
        let event2 = TeamEvent::CompilationFailed {
            errors: "error".to_string(),
            files: vec![],
            severity: "error".to_string(),
        };

        engine.dispatch(&event1);
        engine.dispatch(&event2);

        assert_eq!(engine.event_history().len(), 2);
    }

    #[test]
    fn test_dispatch_stats() {
        let mut engine = EventEngine::new();

        let event1 = TeamEvent::CodeChanged {
            files: vec![],
            author: "Builder".to_string(),
            generation: 1,
        };
        let event2 = TeamEvent::CodeChanged {
            files: vec![],
            author: "Builder".to_string(),
            generation: 2,
        };
        let event3 = TeamEvent::CompilationFailed {
            errors: "error".to_string(),
            files: vec![],
            severity: "error".to_string(),
        };

        engine.dispatch(&event1);
        engine.dispatch(&event2);
        engine.dispatch(&event3);

        let stats = engine.dispatch_stats();
        assert_eq!(stats.get(&TeamEventType::CodeChanged), Some(&2));
        assert_eq!(stats.get(&TeamEventType::CompilationFailed), Some(&1));
    }
}
