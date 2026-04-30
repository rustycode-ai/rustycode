//! Sequence diagram generator for agent interactions.
//!
//! Generates ASCII sequence diagrams showing message flow between agents.

use crate::team::agent_timeline::{AgentTimeline, TimelineEvent};
use rustycode_protocol::agent_protocol::AgentRole;
use std::collections::HashSet;

/// Generate a sequence diagram showing agent interactions.
pub fn generate_sequence_diagram(timeline: &AgentTimeline) -> String {
    let mut output = String::new();

    output.push_str("Agent Interaction Sequence:\n");
    output.push_str(&"═".repeat(80));
    output.push('\n');

    // Collect all events with their turns
    let mut events: Vec<(u32, AgentRole, &TimelineEvent)> = Vec::new();
    for (role, track) in &timeline.agents {
        for event in &track.events {
            if let Some(turn) = event.turn() {
                events.push((turn, *role, event));
            }
        }
    }

    // Sort by turn
    events.sort_by_key(|(turn, _, _)| *turn);

    // Generate diagram header
    let roles: HashSet<AgentRole> = events.iter().map(|(_, role, _)| *role).collect();
    let mut role_vec: Vec<AgentRole> = roles.into_iter().collect();
    role_vec.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));

    // Header with participant names
    output.push('\n');
    for role in &role_vec {
        output.push_str(&format!("{:<14}", format!("{:?}", role)));
    }
    output.push('\n');

    // Separator lines
    for _role in &role_vec {
        output.push_str(&"─".repeat(14));
    }
    output.push('\n');

    // Events grouped by turn
    let mut current_turn: Option<u32> = None;

    for (turn, role, event) in events {
        if current_turn != Some(turn) {
            if current_turn.is_some() {
                output.push('\n');
            }
            output.push_str(&format!("\n[Turn {}]\n", turn));
            current_turn = Some(turn);
        }

        // Generate event visualization
        match event {
            TimelineEvent::Activated { reason, .. } => {
                output.push_str(&format!(
                    "{:<14} >> ACTIVATE: {}\n",
                    format!("{:?}", role),
                    reason
                ));
            }
            TimelineEvent::Deactivated { reason, .. } => {
                output.push_str(&format!(
                    "{:<14} << DEACTIVATE: {}\n",
                    format!("{:?}", role),
                    reason
                ));
            }
            TimelineEvent::StateChange { from, to, .. } => {
                output.push_str(&format!(
                    "{:<14} :: {:?} → {:?}\n",
                    format!("{:?}", role),
                    from,
                    to
                ));
            }
            TimelineEvent::Insight { message, .. } => {
                output.push_str(&format!(
                    "{:<14} !! INSIGHT: {}\n",
                    format!("{:?}", role),
                    message
                ));
            }
        }
    }

    output.push('\n');
    output
}

/// Generate a flow diagram showing the order of agent activations.
pub fn generate_flow_diagram(timeline: &AgentTimeline) -> String {
    let mut output = String::new();

    output.push_str("Agent Flow Diagram:\n");
    output.push_str(&"═".repeat(60));
    output.push('\n');

    // Collect activations in order
    let mut activations: Vec<(u32, AgentRole, String)> = Vec::new();
    for (role, track) in &timeline.agents {
        for event in &track.events {
            if let TimelineEvent::Activated { reason, turn, .. } = event {
                activations.push((*turn, *role, reason.clone()));
            }
        }
    }

    activations.sort_by_key(|(turn, _, _)| *turn);

    // Build flow visualization
    let mut prev_turn: Option<u32> = None;
    for (turn, role, reason) in activations {
        if prev_turn != Some(turn) {
            if prev_turn.is_some() {
                output.push('\n');
                output.push_str(&" ".repeat(30));
                output.push_str("│\n");
            }
            output.push_str(&format!("Turn {}:\n", turn));
            prev_turn = Some(turn);
        }
        output.push_str(&" ".repeat(10));
        output.push_str("│\n");
        output.push_str(&" ".repeat(10));
        output.push_str("▼\n");
        output.push_str(&" ".repeat(10));
        output.push_str(&format!("┌{}┐\n", "-".repeat(20)));
        output.push_str(&" ".repeat(10));
        output.push_str(&format!("│ {:<18} │\n", format!("{:?}", role)));
        output.push_str(&" ".repeat(10));
        output.push_str(&format!("└{}┘\n", "-".repeat(20)));
        output.push_str(&" ".repeat(10));
        output.push_str(&format!("│ {}\n", reason));
    }

    output.push('\n');
    output
}

/// Generate statistics about agent interactions.
pub fn generate_statistics(timeline: &AgentTimeline) -> String {
    let mut output = String::new();

    output.push_str("Agent Interaction Statistics:\n");
    output.push_str(&"─".repeat(50));
    output.push('\n');

    let summary = timeline.summary();

    output.push_str(&format!("Total Turns: {}\n", summary.total_turns));
    output.push_str(&format!("Status: {:?}\n\n", summary.status));

    output.push_str("Activation Count:\n");
    for (role, agent) in &summary.agents {
        if agent.activation_count > 0 {
            let bar = "█".repeat(agent.activation_count as usize);
            output.push_str(&format!(
                "  {:<12} {} ({})\n",
                format!("{:?}", role),
                bar,
                agent.activation_count
            ));
        }
    }

    output.push('\n');

    // Calculate total events
    let total_events: usize = timeline.agents.values().map(|t| t.events.len()).sum();
    output.push_str(&format!("Total Events: {}\n", total_events));

    // Event type breakdown
    let mut activation_count = 0;
    let mut deactivation_count = 0;
    let mut state_change_count = 0;
    let mut insight_count = 0;

    for track in timeline.agents.values() {
        for event in &track.events {
            match event {
                TimelineEvent::Activated { .. } => activation_count += 1,
                TimelineEvent::Deactivated { .. } => deactivation_count += 1,
                TimelineEvent::StateChange { .. } => state_change_count += 1,
                TimelineEvent::Insight { .. } => insight_count += 1,
            }
        }
    }

    output.push_str(&format!("  Activations:   {}\n", activation_count));
    output.push_str(&format!("  Deactivations: {}\n", deactivation_count));
    output.push_str(&format!("  State Changes: {}\n", state_change_count));
    output.push_str(&format!("  Insights:      {}\n", insight_count));

    output
}

/// Generate comprehensive visualization with all views.
pub fn generate_full_visualization(timeline: &AgentTimeline) -> String {
    let mut output = String::new();

    output.push_str(&"═".repeat(80));
    output.push('\n');
    output.push_str("  TEAM AGENT INTERACTION VISUALIZATION\n");
    output.push_str(&"═".repeat(80));
    output.push_str("\n\n");

    output.push_str(&timeline.ascii_visualization());
    output.push('\n');
    output.push_str(&generate_sequence_diagram(timeline));
    output.push_str(&generate_flow_diagram(timeline));
    output.push_str(&generate_statistics(timeline));

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::agent_timeline::{AgentState, TaskStatus};

    #[test]
    fn test_sequence_diagram_generation() {
        let mut timeline = AgentTimeline::new("test-sequence");

        timeline.activate_agent(AgentRole::Architect, "High risk");
        timeline.record_state_change(AgentRole::Architect, AgentState::Idle, AgentState::Reading);
        timeline.deactivate_agent(AgentRole::Architect, "Done");

        timeline.next_turn();
        timeline.activate_agent(AgentRole::Builder, "Implement");
        timeline.deactivate_agent(AgentRole::Builder, "Done");

        let diagram = generate_sequence_diagram(&timeline);

        assert!(diagram.contains("Architect"));
        assert!(diagram.contains("Builder"));
        assert!(diagram.contains("ACTIVATE"));
    }

    #[test]
    fn test_statistics_generation() {
        let mut timeline = AgentTimeline::new("test-stats");

        for _ in 0..3 {
            timeline.activate_agent(AgentRole::Builder, "Step");
            timeline.deactivate_agent(AgentRole::Builder, "Done");
            timeline.next_turn();
        }

        let stats = generate_statistics(&timeline);

        assert!(stats.contains("Total Turns: 3"));
        assert!(stats.contains("Builder"));
        assert!(stats.contains("Activation Count"));
    }

    #[test]
    fn test_full_visualization() {
        let mut timeline = AgentTimeline::new("test-full");

        timeline.activate_agent(AgentRole::Architect, "Plan");
        timeline.deactivate_agent(AgentRole::Architect, "Done");
        timeline.next_turn();
        timeline.activate_agent(AgentRole::Builder, "Build");
        timeline.deactivate_agent(AgentRole::Builder, "Done");
        timeline.set_status(TaskStatus::Success);

        let viz = generate_full_visualization(&timeline);

        assert!(viz.contains("TEAM AGENT INTERACTION VISUALIZATION"));
        assert!(viz.contains("Agent Activation Timeline"));
        assert!(viz.contains("Sequence"));
        assert!(viz.contains("Statistics"));
    }
}
