//! Agent lifetime visualization and tracking.
//!
//! This module provides real-time visualization of agent activation patterns
//! during task execution. Each agent emits events on state changes, which
//! are collected into a timeline that can be rendered or analyzed.

use rustycode_protocol::agent_protocol::AgentRole;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Represents the current state of an agent in its lifecycle.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Agent is idle, waiting for activation
    Idle,
    /// Agent is receiving briefing/context
    Briefing,
    /// Agent is reasoning about the task
    Reasoning,
    /// Agent is reading files or analyzing
    Reading,
    /// Agent is analyzing (Architect-specific)
    Analyzing,
    /// Agent is declaring structure (Architect-specific)
    Declaring,
    /// Agent is implementing changes (Builder-specific)
    Implementing,
    /// Agent is reviewing (Skeptic-specific)
    Reviewing,
    /// Agent is verifying claims (Skeptic-specific)
    Verifying,
    /// Agent is compiling (Judge-specific)
    Compiling,
    /// Agent is testing (Judge-specific)
    Testing,
    /// Agent is diagnosing errors (Scalpel-specific)
    Diagnosing,
    /// Agent is fixing errors (Scalpel-specific)
    Fixing,
    /// Agent has completed its turn
    Complete,
}

/// Events that occur during an agent's lifetime.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineEvent {
    /// Agent was activated for a turn
    Activated {
        turn: u32,
        reason: String,
        timestamp_ms: u64,
    },
    /// Agent changed state (e.g., Reading → Analyzing)
    StateChange {
        from: AgentState,
        to: AgentState,
        timestamp_ms: u64,
    },
    /// Agent was deactivated after completing its turn
    Deactivated {
        turn: u32,
        reason: String,
        timestamp_ms: u64,
    },
    /// Agent emitted an insight or observation
    Insight {
        message: String,
        turn: u32,
        timestamp_ms: u64,
    },
}

impl TimelineEvent {
    /// Create an Activated event with the current timestamp.
    pub fn activated(turn: u32, reason: impl Into<String>, start_time: Instant) -> Self {
        Self::Activated {
            turn,
            reason: reason.into(),
            timestamp_ms: start_time.elapsed().as_millis() as u64,
        }
    }

    /// Create a StateChange event with the current timestamp.
    pub fn state_change(from: AgentState, to: AgentState, start_time: Instant) -> Self {
        Self::StateChange {
            from,
            to,
            timestamp_ms: start_time.elapsed().as_millis() as u64,
        }
    }

    /// Create a Deactivated event with the current timestamp.
    pub fn deactivated(turn: u32, reason: impl Into<String>, start_time: Instant) -> Self {
        Self::Deactivated {
            turn,
            reason: reason.into(),
            timestamp_ms: start_time.elapsed().as_millis() as u64,
        }
    }

    /// Create an Insight event with the current timestamp.
    pub fn insight(message: impl Into<String>, turn: u32, start_time: Instant) -> Self {
        Self::Insight {
            message: message.into(),
            turn,
            timestamp_ms: start_time.elapsed().as_millis() as u64,
        }
    }

    /// Get the timestamp of this event in milliseconds.
    pub fn timestamp_ms(&self) -> u64 {
        match self {
            TimelineEvent::Activated { timestamp_ms, .. } => *timestamp_ms,
            TimelineEvent::StateChange { timestamp_ms, .. } => *timestamp_ms,
            TimelineEvent::Deactivated { timestamp_ms, .. } => *timestamp_ms,
            TimelineEvent::Insight { timestamp_ms, .. } => *timestamp_ms,
        }
    }

    /// Get the turn number associated with this event.
    pub fn turn(&self) -> Option<u32> {
        match self {
            TimelineEvent::Activated { turn, .. } => Some(*turn),
            TimelineEvent::StateChange { .. } => None,
            TimelineEvent::Deactivated { turn, .. } => Some(*turn),
            TimelineEvent::Insight { turn, .. } => Some(*turn),
        }
    }
}

/// Tracks all events for a single agent role during a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrack {
    /// The role this track represents
    pub role: AgentRole,
    /// All events in chronological order
    pub events: Vec<TimelineEvent>,
    /// Total time spent active (ms)
    pub active_duration_ms: u64,
    /// Number of times this agent was activated
    pub activation_count: u32,
}

impl AgentTrack {
    /// Create a new empty track for the given role.
    pub fn new(role: AgentRole) -> Self {
        Self {
            role,
            events: Vec::new(),
            active_duration_ms: 0,
            activation_count: 0,
        }
    }

    /// Add an event to this track.
    pub fn add_event(&mut self, event: TimelineEvent) {
        if matches!(event, TimelineEvent::Activated { .. }) {
            self.activation_count += 1;
        }
        self.events.push(event);
    }

    /// Calculate total active duration from events.
    pub fn calculate_active_duration(&mut self) -> u64 {
        let mut total_ms = 0u64;
        let mut last_activate: Option<u64> = None;

        for event in &self.events {
            match event {
                TimelineEvent::Activated { timestamp_ms, .. } => {
                    last_activate = Some(*timestamp_ms);
                }
                TimelineEvent::Deactivated { timestamp_ms, .. } => {
                    if let Some(start) = last_activate {
                        total_ms += timestamp_ms - start;
                    }
                    last_activate = None;
                }
                _ => {}
            }
        }

        self.active_duration_ms = total_ms;
        total_ms
    }
}

/// Status of the overall task.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task is in progress
    InProgress,
    /// Task completed successfully
    Success,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Complete timeline for all agents during a task.
#[derive(Debug, Clone)]
pub struct AgentTimeline {
    /// Unique identifier for the task
    pub task_id: String,
    /// When the task started (not serialized, transient)
    pub start_time: Instant,
    /// Tracks for each agent role
    pub agents: HashMap<AgentRole, AgentTrack>,
    /// Current turn number
    pub current_turn: u32,
    /// Task completion status
    pub status: TaskStatus,
}

impl Default for AgentTimeline {
    fn default() -> Self {
        Self::new("unknown")
    }
}

impl AgentTimeline {
    /// Create a new timeline for the given task.
    pub fn new(task_id: &str) -> Self {
        let mut agents = HashMap::new();

        // Initialize tracks for all possible agent roles
        for role in [
            AgentRole::Coordinator,
            AgentRole::Architect,
            AgentRole::Builder,
            AgentRole::Skeptic,
            AgentRole::Judge,
            AgentRole::Scalpel,
        ] {
            agents.insert(role, AgentTrack::new(role));
        }

        Self {
            task_id: task_id.to_string(),
            start_time: Instant::now(),
            agents,
            current_turn: 0,
            status: TaskStatus::InProgress,
        }
    }

    /// Activate an agent for a turn.
    pub fn activate_agent(&mut self, role: AgentRole, reason: impl Into<String>) {
        if let Some(track) = self.agents.get_mut(&role) {
            let event = TimelineEvent::activated(self.current_turn, reason, self.start_time);
            track.add_event(event);
        }
    }

    /// Deactivate an agent after its turn.
    pub fn deactivate_agent(&mut self, role: AgentRole, reason: impl Into<String>) {
        if let Some(track) = self.agents.get_mut(&role) {
            let event = TimelineEvent::deactivated(self.current_turn, reason, self.start_time);
            track.add_event(event);
        }
    }

    /// Record a state change for an agent.
    pub fn record_state_change(&mut self, role: AgentRole, from: AgentState, to: AgentState) {
        if let Some(track) = self.agents.get_mut(&role) {
            let event = TimelineEvent::state_change(from, to, self.start_time);
            track.add_event(event);
        }
    }

    /// Record an insight from an agent.
    pub fn record_insight(&mut self, role: AgentRole, message: impl Into<String>) {
        if let Some(track) = self.agents.get_mut(&role) {
            let event = TimelineEvent::insight(message, self.current_turn, self.start_time);
            track.add_event(event);
        }
    }

    /// Advance to the next turn.
    pub fn next_turn(&mut self) {
        self.current_turn += 1;
    }

    /// Mark the task as completed with the given status.
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
    }

    /// Get a summary of agent activations.
    pub fn summary(&self) -> AgentTimelineSummary {
        let mut summary = AgentTimelineSummary {
            task_id: self.task_id.clone(),
            total_turns: self.current_turn,
            status: self.status.clone(),
            agents: HashMap::new(),
        };

        for (role, track) in &self.agents {
            summary.agents.insert(
                *role,
                AgentSummary {
                    role: *role,
                    activation_count: track.activation_count,
                    event_count: track.events.len(),
                },
            );
        }

        summary
    }

    /// Generate an ASCII visualization of the timeline.
    pub fn ascii_visualization(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("Task: {}\n", self.task_id));
        output.push_str(&format!("Turns: {}\n", self.current_turn));
        output.push_str(&format!("Status: {:?}\n\n", self.status));

        output.push_str("Agent Activation Timeline:\n");
        output.push_str(&"─".repeat(70));
        output.push('\n');

        let roles = [
            AgentRole::Coordinator,
            AgentRole::Architect,
            AgentRole::Builder,
            AgentRole::Skeptic,
            AgentRole::Judge,
            AgentRole::Scalpel,
        ];

        for role in &roles {
            if let Some(track) = self.agents.get(role) {
                let role_name = format!("{:?}", role);
                output.push_str(&format!("{:<12} ", role_name));

                // Generate activation bars
                for turn in 0..self.current_turn.min(20) {
                    let active = track.events.iter().any(|e| {
                        e.turn() == Some(turn + 1) && matches!(e, TimelineEvent::Activated { .. })
                    });
                    output.push(if active { '█' } else { '░' });
                }

                if self.current_turn > 20 {
                    output.push_str("...");
                }

                output.push('\n');
            }
        }

        output.push_str(&"─".repeat(70));
        output.push('\n');
        output.push_str("Legend: █ Active  ░ Inactive\n\n");

        // Add detailed interaction log
        output.push_str("Agent Interactions:\n");
        output.push_str(&"─".repeat(70));
        output.push('\n');

        let mut events_by_turn: HashMap<u32, Vec<&TimelineEvent>> = HashMap::new();
        for track in self.agents.values() {
            for event in &track.events {
                if let Some(turn) = event.turn() {
                    events_by_turn.entry(turn).or_default().push(event);
                }
            }
        }

        let mut turns: Vec<_> = events_by_turn.keys().collect();
        turns.sort();

        for turn in turns {
            let events = events_by_turn.get(turn).unwrap();
            output.push_str(&format!("\nTurn {}:\n", turn));
            for event in events {
                match event {
                    TimelineEvent::Activated { reason, .. } => {
                        output.push_str(&format!("  [+] Activated: {}\n", reason));
                    }
                    TimelineEvent::Deactivated { reason, .. } => {
                        output.push_str(&format!("  [-] Deactivated: {}\n", reason));
                    }
                    TimelineEvent::StateChange { from, to, .. } => {
                        output.push_str(&format!("  [→] State: {:?} → {:?}\n", from, to));
                    }
                    TimelineEvent::Insight { message, .. } => {
                        output.push_str(&format!("  [!] Insight: {}\n", message));
                    }
                }
            }
        }

        output
    }
}

/// Summary of agent activations for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTimelineSummary {
    pub task_id: String,
    pub total_turns: u32,
    pub status: TaskStatus,
    pub agents: HashMap<AgentRole, AgentSummary>,
}

/// Summary for a single agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSummary {
    pub role: AgentRole,
    pub activation_count: u32,
    pub event_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_creation() {
        let timeline = AgentTimeline::new("test-task");

        assert_eq!(timeline.task_id, "test-task");
        assert_eq!(timeline.current_turn, 0);
        assert_eq!(timeline.status, TaskStatus::InProgress);
        assert_eq!(timeline.agents.len(), 6);
    }

    #[test]
    fn test_agent_activation() {
        let mut timeline = AgentTimeline::new("test-task");

        timeline.activate_agent(AgentRole::Architect, "High risk task");

        let track = timeline.agents.get(&AgentRole::Architect).unwrap();
        assert_eq!(track.activation_count, 1);
        assert_eq!(track.events.len(), 1);

        if let TimelineEvent::Activated { reason, .. } = &track.events[0] {
            assert_eq!(reason, "High risk task");
        } else {
            panic!("Expected Activated event");
        }
    }

    #[test]
    fn test_state_changes() {
        let mut timeline = AgentTimeline::new("test-task");

        timeline.activate_agent(AgentRole::Architect, "High risk");
        timeline.record_state_change(AgentRole::Architect, AgentState::Idle, AgentState::Reading);
        timeline.record_state_change(
            AgentRole::Architect,
            AgentState::Reading,
            AgentState::Analyzing,
        );
        timeline.deactivate_agent(AgentRole::Architect, "Done");

        let track = timeline.agents.get(&AgentRole::Architect).unwrap();
        assert_eq!(track.events.len(), 4);
    }

    #[test]
    fn test_turn_progression() {
        let mut timeline = AgentTimeline::new("test-task");

        for _ in 0..5 {
            timeline.next_turn();
        }

        assert_eq!(timeline.current_turn, 5);
    }

    #[test]
    fn test_summary_generation() {
        let mut timeline = AgentTimeline::new("test-task");

        timeline.activate_agent(AgentRole::Builder, "Step 1");
        timeline.activate_agent(AgentRole::Skeptic, "Review");
        timeline.next_turn();
        timeline.next_turn();
        timeline.set_status(TaskStatus::Success);

        let summary = timeline.summary();

        assert_eq!(summary.total_turns, 2);
        assert_eq!(summary.status, TaskStatus::Success);
        assert_eq!(
            summary
                .agents
                .get(&AgentRole::Builder)
                .unwrap()
                .activation_count,
            1
        );
        assert_eq!(
            summary
                .agents
                .get(&AgentRole::Skeptic)
                .unwrap()
                .activation_count,
            1
        );
    }

    #[test]
    fn test_ascii_visualization() {
        let mut timeline = AgentTimeline::new("visual-test");

        timeline.activate_agent(AgentRole::Builder, "Implement");
        timeline.next_turn();
        timeline.activate_agent(AgentRole::Skeptic, "Review");
        timeline.next_turn();
        timeline.set_status(TaskStatus::Success);

        let visualization = timeline.ascii_visualization();

        assert!(visualization.contains("Task: visual-test"));
        assert!(visualization.contains("Builder"));
        assert!(visualization.contains("Skeptic"));
        assert!(visualization.contains('█'));
    }
}
