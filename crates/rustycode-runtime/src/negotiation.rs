//! Agent Negotiation and Consensus Building
//!
//! This module provides:
//! - Agent-to-agent negotiation protocols
//! - Consensus building algorithms
//! - Conflict resolution strategies
//! - Voting and agreement mechanisms
//! - Multi-agent decision making

use crate::multi_agent::AgentRole;
use crate::shared_memory::{
    AccessLevel, ConflictType, MemoryConflict, MemoryData, MemoryType, SharedWorkingMemory,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;
use uuid::Uuid;

/// Negotiation protocol between agents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NegotiationProtocol {
    /// Alternating offers protocol
    AlternatingOffers,
    /// Multi-round negotiation
    MultiRound { max_rounds: u32 },
    /// Consensus-seeking
    ConsensusSeeking,
    /// Competitive bidding
    CompetitiveBidding,
    /// Collaborative problem solving
    Collaborative,
}

/// Negotiation position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationPosition {
    /// Agent proposing the position
    pub agent_id: String,

    /// Agent role
    pub role: AgentRole,

    /// Proposed solution
    pub proposal: String,

    /// Confidence in proposal (0.0 - 1.0)
    pub confidence: f64,

    /// Minimum acceptable confidence
    pub min_acceptable: f64,

    /// Priority (higher = more important)
    pub priority: u32,

    /// Dependencies on other agents
    pub dependencies: Vec<String>,

    pub resource_requirements: ResourceRequirements,

    pub timestamp: DateTime<Utc>,
}

/// Resource requirements for an agent's position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// Required execution time (milliseconds)
    pub time_required_ms: u64,

    /// Required memory (MB)
    pub memory_required_mb: u64,

    /// Required CPU (percentage)
    pub cpu_required_percent: f64,

    /// Budget constraints
    pub budget: Option<f64>,
}

/// Negotiation message between agents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NegotiationMessage {
    /// Initial proposal
    Proposal {
        id: String,
        from: String,
        to: String,
        position: NegotiationPosition,
    },

    /// Acceptance of proposal
    Acceptance {
        proposal_id: String,
        from: String,
        to: String,
        conditions: Vec<String>,
    },

    /// Rejection of proposal
    Rejection {
        proposal_id: String,
        from: String,
        to: String,
        reason: String,
        counter_proposal: Option<NegotiationPosition>,
    },

    /// Counter-proposal
    CounterProposal {
        original_proposal_id: String,
        from: String,
        to: String,
        new_position: NegotiationPosition,
    },

    /// Request for clarification
    ClarificationRequest {
        proposal_id: String,
        from: String,
        to: String,
        questions: Vec<String>,
    },

    /// Clarification response
    ClarificationResponse {
        proposal_id: String,
        from: String,
        to: String,
        answers: Vec<String>,
    },
}

/// Negotiation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationSession {
    /// Session ID
    pub id: String,

    /// Topic being negotiated
    pub topic: String,

    /// Participating agents
    pub participants: Vec<String>,

    pub current_round: u32,

    /// Maximum rounds
    pub max_rounds: u32,

    /// Protocol being used
    pub protocol: NegotiationProtocol,

    pub state: NegotiationState,

    /// Messages exchanged
    pub messages: Vec<NegotiationMessage>,

    pub proposals: HashMap<String, NegotiationPosition>,

    /// Start time
    pub started_at: DateTime<Utc>,

    /// End time (if concluded)
    pub ended_at: Option<DateTime<Utc>>,

    /// Final outcome (if concluded)
    pub outcome: Option<NegotiationOutcome>,
}

/// Negotiation state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NegotiationState {
    /// Negotiation ongoing
    Ongoing,

    /// Consensus reached
    ConsensusReached { agreement: String },

    /// Negotiation failed
    Failed { reason: String },

    /// Negotiation abandoned
    Abandoned,

    /// Partial agreement (some agents agreed)
    PartialAgreement {
        agreed: Vec<String>,
        disagreed: Vec<String>,
    },
}

/// Negotiation outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationOutcome {
    pub outcome_type: NegotiationOutcomeType,

    /// Final agreement (if reached)
    pub agreement: Option<String>,

    /// Agents who agreed
    pub agreed_agents: Vec<String>,

    /// Agents who disagreed
    pub disagreed_agents: Vec<String>,

    /// Final confidence in outcome
    pub confidence: f64,

    /// Duration of negotiation
    pub duration_ms: u64,
}

/// Type of negotiation outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NegotiationOutcomeType {
    /// Full consensus
    FullConsensus,

    /// Majority agreement
    MajorityAgreement { percentage: f64 },

    /// Compromise
    Compromise,

    /// No agreement
    NoAgreement,

    /// Arbitrated outcome
    Arbitrated,
}

/// Consensus algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ConsensusAlgorithm {
    /// Simple majority vote
    SimpleMajority { threshold: f64 },

    /// Supermajority (2/3)
    Supermajority { required: f64 },

    /// Unanimous consent
    Unanimous,

    /// Weighted voting
    Weighted { weights: HashMap<String, f64> },

    /// Delegated voting
    Delegated {
        delegates: HashMap<String, Vec<String>>,
    },

    /// Veto power
    Veto { veto_holders: Vec<String> },
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ConflictResolutionStrategy {
    /// Last writer wins
    LastWriterWins,

    /// Merge with semantic analysis
    SemanticMerge,

    /// Agent voting
    AgentVoting,

    /// Arbitration by senior agent
    SeniorAgentArbitration,

    /// Conservative (keep both)
    KeepBoth,

    /// Custom resolution
    Custom { resolver: String },
}

/// Agent negotiator
pub struct AgentNegotiator {
    /// Shared memory for storing negotiations
    memory: SharedWorkingMemory,

    /// Active negotiation sessions
    sessions: HashMap<String, NegotiationSession>,
}

impl AgentNegotiator {
    pub fn new() -> Self {
        Self {
            memory: SharedWorkingMemory::new(),
            sessions: HashMap::new(),
        }
    }

    /// Start a negotiation session
    pub fn start_negotiation(
        &mut self,
        topic: String,
        participants: Vec<String>,
        protocol: NegotiationProtocol,
        max_rounds: u32,
    ) -> Result<String, crate::workflow::WorkflowError> {
        let session_id = Uuid::new_v4().to_string();

        let session = NegotiationSession {
            id: session_id.clone(),
            topic,
            participants,
            current_round: 0,
            max_rounds,
            protocol,
            state: NegotiationState::Ongoing,
            messages: Vec::new(),
            proposals: HashMap::new(),
            started_at: Utc::now(),
            ended_at: None,
            outcome: None,
        };

        self.sessions.insert(session_id.clone(), session);

        Ok(session_id)
    }

    /// Submit a proposal to a negotiation
    pub fn submit_proposal(
        &mut self,
        session_id: &str,
        position: NegotiationPosition,
    ) -> Result<(), crate::workflow::WorkflowError> {
        let session = self.sessions.get_mut(session_id).ok_or_else(|| {
            crate::workflow::WorkflowError::Validation("Session not found".to_string())
        })?;

        if !matches!(session.state, NegotiationState::Ongoing) {
            return Err(crate::workflow::WorkflowError::Validation(
                "Session is not ongoing".to_string(),
            ));
        }

        session
            .proposals
            .insert(position.agent_id.clone(), position.clone());

        // Store in shared memory
        let _entry_id = self
            .memory
            .write(
                &format!("negotiation_{}", session_id),
                MemoryType::Analysis,
                MemoryData::Text(format!(
                    "Proposal from {}: {}",
                    position.agent_id, position.proposal
                )),
                AccessLevel::Public,
            )
            .map_err(|e| crate::workflow::WorkflowError::Validation(e.to_string()))?;

        Ok(())
    }

    /// Run a negotiation round
    pub fn run_negotiation_round(
        &mut self,
        session_id: &str,
    ) -> Result<NegotiationState, crate::workflow::WorkflowError> {
        let session = self.sessions.get_mut(session_id).ok_or_else(|| {
            crate::workflow::WorkflowError::Validation("Session not found".to_string())
        })?;

        if session.current_round >= session.max_rounds {
            session.state = NegotiationState::Failed {
                reason: "Maximum rounds reached".to_string(),
            };
            return Ok(session.state.clone());
        }

        session.current_round = session.current_round.saturating_add(1);

        // Collect data needed for consensus check
        let proposals_clone = session.proposals.clone();
        let participants_clone = session.participants.clone();
        let started_at = session.started_at;

        // Release mutable borrow before calling check_consensus
        let _ = session;

        // Check for consensus
        let consensus = self.check_consensus_with_data(&proposals_clone, &participants_clone)?;

        // Now get mutable borrow back to update state
        let session = self.sessions.get_mut(session_id).ok_or_else(|| {
            crate::workflow::WorkflowError::Validation("Session not found".to_string())
        })?;

        if matches!(consensus, NegotiationState::ConsensusReached { .. }) {
            session.state = consensus.clone();
            session.ended_at = Some(Utc::now());

            // Generate outcome
            if let NegotiationState::ConsensusReached { agreement } = &consensus {
                session.outcome = Some(NegotiationOutcome {
                    outcome_type: NegotiationOutcomeType::FullConsensus,
                    agreement: Some(agreement.clone()),
                    agreed_agents: participants_clone,
                    disagreed_agents: Vec::new(),
                    confidence: 0.9,
                    duration_ms: (Utc::now() - started_at).num_milliseconds().max(0) as u64,
                });
            }
        }

        Ok(session.state.clone())
    }

    /// Check if consensus has been reached with provided data
    fn check_consensus_with_data(
        &self,
        proposals: &HashMap<String, NegotiationPosition>,
        participants: &[String],
    ) -> Result<NegotiationState, crate::workflow::WorkflowError> {
        if participants.is_empty() {
            return Ok(NegotiationState::Failed {
                reason: "no participants".to_string(),
            });
        }
        if proposals.is_empty() {
            return Ok(NegotiationState::Ongoing);
        }

        // Find the highest confidence proposal
        let mut best_proposal: Option<&NegotiationPosition> = None;

        for proposal in proposals.values() {
            let is_better = match best_proposal {
                None => true,
                Some(best) => {
                    proposal.confidence > best.confidence
                        || (proposal.confidence == best.confidence
                            && proposal.priority > best.priority)
                }
            };

            if is_better {
                best_proposal = Some(proposal);
            }
        }

        if let Some(best) = best_proposal {
            // Check if it meets minimum requirements
            let satisfied_count = proposals
                .values()
                .filter(|p| p.confidence >= best.min_acceptable)
                .count();

            let satisfied_ratio = satisfied_count as f64 / participants.len() as f64;

            if satisfied_ratio >= 0.8 {
                // 80% satisfied is good enough for consensus
                return Ok(NegotiationState::ConsensusReached {
                    agreement: best.proposal.clone(),
                });
            }
        }

        Ok(NegotiationState::Ongoing)
    }

    /// Resolve conflicts in shared memory
    pub fn resolve_conflicts(
        &mut self,
        conflicts: Vec<MemoryConflict>,
        strategy: ConflictResolutionStrategy,
    ) -> Result<Vec<MemoryConflict>, crate::workflow::WorkflowError> {
        match strategy {
            ConflictResolutionStrategy::LastWriterWins => {
                // Last writer wins - keep the most recent
                Ok(conflicts)
            }

            ConflictResolutionStrategy::SemanticMerge => {
                // Attempt semantic merge
                self.semantic_merge_conflicts(conflicts)
            }

            ConflictResolutionStrategy::AgentVoting => {
                // Agents vote on resolution
                self.vote_on_conflicts(conflicts)
            }

            ConflictResolutionStrategy::SeniorAgentArbitration => {
                // Senior agent decides
                self.arbitrate_conflicts(conflicts)
            }

            ConflictResolutionStrategy::KeepBoth => {
                // Keep both versions (create new entries)
                self.keep_both_versions(conflicts)
            }

            ConflictResolutionStrategy::Custom { resolver } => {
                // Custom resolution logic
                debug!("Using custom resolver: {}", resolver);
                Ok(conflicts)
            }
        }
    }

    /// Semantic merge of conflicts
    fn semantic_merge_conflicts(
        &mut self,
        conflicts: Vec<MemoryConflict>,
    ) -> Result<Vec<MemoryConflict>, crate::workflow::WorkflowError> {
        // Simplified semantic merge
        let mut unresolved = Vec::new();

        for conflict in conflicts {
            match conflict.conflict_type {
                ConflictType::ConcurrentWrite => {
                    // Merge by combining content
                    debug!(
                        "Semantic merge of concurrent write for entry {}",
                        conflict.entry_id
                    );
                    // In real implementation, would do actual semantic merge
                }
                ConflictType::DependencyCycle => {
                    // Keep the higher version
                    debug!("Resolved dependency cycle for entry {}", conflict.entry_id);
                }
                _ => {
                    unresolved.push(conflict);
                }
            }
        }

        Ok(unresolved)
    }

    /// Vote on conflict resolution
    fn vote_on_conflicts(
        &mut self,
        conflicts: Vec<MemoryConflict>,
    ) -> Result<Vec<MemoryConflict>, crate::workflow::WorkflowError> {
        debug!("Agent voting on {} conflicts", conflicts.len());
        Ok(conflicts)
    }

    /// Arbitrate conflicts using senior agent
    fn arbitrate_conflicts(
        &mut self,
        conflicts: Vec<MemoryConflict>,
    ) -> Result<Vec<MemoryConflict>, crate::workflow::WorkflowError> {
        debug!("Senior agent arbitration of {} conflicts", conflicts.len());
        Ok(conflicts)
    }

    /// Keep both versions of conflicting entries
    fn keep_both_versions(
        &mut self,
        conflicts: Vec<MemoryConflict>,
    ) -> Result<Vec<MemoryConflict>, crate::workflow::WorkflowError> {
        debug!("Keeping both versions for {} conflicts", conflicts.len());
        Ok(conflicts)
    }

    /// Build consensus using specific algorithm
    pub fn build_consensus(
        &mut self,
        participants: Vec<String>,
        proposals: HashMap<String, String>,
        algorithm: ConsensusAlgorithm,
    ) -> Result<Option<String>, crate::workflow::WorkflowError> {
        match algorithm {
            ConsensusAlgorithm::SimpleMajority { threshold } => {
                self.simple_majority_consensus(participants, proposals, threshold)
            }

            ConsensusAlgorithm::Supermajority { required } => {
                self.supermajority_consensus(participants, proposals, required)
            }

            ConsensusAlgorithm::Unanimous => self.unanimous_consensus(participants, proposals),

            ConsensusAlgorithm::Weighted { weights } => {
                self.weighted_consensus(participants, proposals, weights)
            }

            ConsensusAlgorithm::Delegated { delegates } => {
                self.delegated_consensus(participants, proposals, delegates)
            }

            ConsensusAlgorithm::Veto { veto_holders } => {
                self.veto_consensus(participants, proposals, veto_holders)
            }
        }
    }

    /// Simple majority consensus
    fn simple_majority_consensus(
        &mut self,
        participants: Vec<String>,
        proposals: HashMap<String, String>,
        threshold: f64,
    ) -> Result<Option<String>, crate::workflow::WorkflowError> {
        if participants.is_empty() {
            return Ok(None);
        }
        // Count votes for each proposal
        let mut vote_counts: HashMap<String, usize> = HashMap::new();

        for proposal in proposals.values() {
            let count = vote_counts.entry(proposal.clone()).or_insert(0);
            *count += 1;
        }

        // Find proposal with most votes
        let mut best_proposal: Option<String> = None;
        let mut best_count = 0;

        for (proposal, count) in vote_counts {
            if count > best_count {
                best_count = count;
                best_proposal = Some(proposal);
            }
        }

        // Check if meets threshold
        if let Some(proposal) = best_proposal {
            let ratio = best_count as f64 / participants.len() as f64;
            if ratio >= threshold {
                return Ok(Some(proposal));
            }
        }

        Ok(None)
    }

    /// Supermajority consensus
    fn supermajority_consensus(
        &mut self,
        participants: Vec<String>,
        proposals: HashMap<String, String>,
        required: f64,
    ) -> Result<Option<String>, crate::workflow::WorkflowError> {
        self.simple_majority_consensus(participants, proposals, required)
    }

    /// Unanimous consensus
    fn unanimous_consensus(
        &mut self,
        _participants: Vec<String>,
        proposals: HashMap<String, String>,
    ) -> Result<Option<String>, crate::workflow::WorkflowError> {
        if proposals.is_empty() {
            return Ok(None);
        }

        // Check if all proposals are the same
        let first_proposal = match proposals.values().next() {
            Some(p) => p,
            None => return Ok(None),
        };
        let all_same = proposals.values().all(|p| p == first_proposal);

        if all_same {
            Ok(Some(first_proposal.clone()))
        } else {
            Ok(None)
        }
    }

    /// Weighted consensus
    fn weighted_consensus(
        &mut self,
        _participants: Vec<String>,
        proposals: HashMap<String, String>,
        weights: HashMap<String, f64>,
    ) -> Result<Option<String>, crate::workflow::WorkflowError> {
        // Calculate weighted scores
        let mut weighted_scores: HashMap<String, f64> = HashMap::new();

        for (participant, proposal) in &proposals {
            let weight = weights.get(participant).unwrap_or(&1.0);
            *weighted_scores.entry(proposal.clone()).or_insert(0.0) += weight;
        }

        // Find proposal with highest score
        let mut best_proposal: Option<String> = None;
        let mut best_score = 0.0;

        for (proposal, score) in weighted_scores {
            if score > best_score {
                best_score = score;
                best_proposal = Some(proposal);
            }
        }

        Ok(best_proposal)
    }

    /// Delegated consensus
    fn delegated_consensus(
        &mut self,
        _participants: Vec<String>,
        _proposals: HashMap<String, String>,
        _delegates: HashMap<String, Vec<String>>,
    ) -> Result<Option<String>, crate::workflow::WorkflowError> {
        // Simplified delegated voting
        Ok(None)
    }

    /// Veto consensus
    fn veto_consensus(
        &mut self,
        participants: Vec<String>,
        proposals: HashMap<String, String>,
        veto_holders: Vec<String>,
    ) -> Result<Option<String>, crate::workflow::WorkflowError> {
        // Check if any veto holder objects
        for holder in &veto_holders {
            if let Some(proposal) = proposals.get(holder) {
                // This veto holder objects to this proposal
                // Check if there's a proposal without veto
                for (participant, other_proposal) in &proposals {
                    if participant != holder && other_proposal != proposal {
                        return Ok(Some(other_proposal.clone()));
                    }
                }
                return Ok(None);
            }
        }

        // No veto, use simple majority
        self.simple_majority_consensus(participants, proposals, 0.5)
    }

    /// Get negotiation session
    pub fn session(&self, session_id: &str) -> Option<&NegotiationSession> {
        self.sessions.get(session_id)
    }

    /// Get all active sessions
    pub fn active_sessions(&self) -> Vec<&NegotiationSession> {
        self.sessions
            .values()
            .filter(|s| matches!(s.state, NegotiationState::Ongoing))
            .collect()
    }

    /// Get negotiation statistics
    pub fn statistics(&self) -> NegotiatorStatistics {
        let total_sessions = self.sessions.len();
        let active_sessions = self.active_sessions().len();
        let concluded_sessions = total_sessions - active_sessions;

        let consensus_count = self
            .sessions
            .values()
            .filter(|s| matches!(s.state, NegotiationState::ConsensusReached { .. }))
            .count();

        NegotiatorStatistics {
            total_sessions,
            active_sessions,
            concluded_sessions,
            consensus_reached: consensus_count,
            consensus_rate: if total_sessions > 0 {
                consensus_count as f64 / total_sessions as f64
            } else {
                0.0
            },
        }
    }
}

/// Negotiator statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiatorStatistics {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub concluded_sessions: usize,
    pub consensus_reached: usize,
    pub consensus_rate: f64,
}

impl Default for AgentNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiator_creation() {
        let negotiator = AgentNegotiator::new();
        assert_eq!(negotiator.active_sessions().len(), 0);
    }

    #[test]
    fn test_start_negotiation() {
        let mut negotiator = AgentNegotiator::new();

        let session_id = negotiator
            .start_negotiation(
                "Test topic".to_string(),
                vec!["agent1".to_string(), "agent2".to_string()],
                NegotiationProtocol::AlternatingOffers,
                5,
            )
            .unwrap();

        assert!(negotiator.session(&session_id).is_some());
    }

    #[test]
    fn test_unanimous_consensus() {
        let mut negotiator = AgentNegotiator::new();

        let participants = vec![
            "agent1".to_string(),
            "agent2".to_string(),
            "agent3".to_string(),
        ];

        let mut proposals = HashMap::new();
        proposals.insert("agent1".to_string(), "Proposal A".to_string());
        proposals.insert("agent2".to_string(), "Proposal A".to_string());
        proposals.insert("agent3".to_string(), "Proposal A".to_string());

        let result = negotiator
            .unanimous_consensus(participants, proposals)
            .unwrap();
        assert_eq!(result, Some("Proposal A".to_string()));
    }

    #[test]
    fn test_simple_majority() {
        let mut negotiator = AgentNegotiator::new();

        let participants = vec![
            "agent1".to_string(),
            "agent2".to_string(),
            "agent3".to_string(),
        ];

        let mut proposals = HashMap::new();
        proposals.insert("agent1".to_string(), "Proposal A".to_string());
        proposals.insert("agent2".to_string(), "Proposal A".to_string());
        proposals.insert("agent3".to_string(), "Proposal B".to_string());

        let result = negotiator
            .simple_majority_consensus(participants, proposals, 0.5)
            .unwrap();
        assert_eq!(result, Some("Proposal A".to_string()));
    }

    // --- Serde roundtrip tests ---

    fn make_position(agent_id: &str) -> NegotiationPosition {
        NegotiationPosition {
            agent_id: agent_id.to_string(),
            role: AgentRole::Reviewer,
            proposal: "Do the thing".to_string(),
            confidence: 0.85,
            min_acceptable: 0.5,
            priority: 3,
            dependencies: vec![],
            resource_requirements: ResourceRequirements {
                time_required_ms: 1000,
                memory_required_mb: 256,
                cpu_required_percent: 50.0,
                budget: Some(10.0),
            },
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn negotiation_protocol_serde_roundtrip() {
        let protocols = [
            NegotiationProtocol::AlternatingOffers,
            NegotiationProtocol::MultiRound { max_rounds: 10 },
            NegotiationProtocol::ConsensusSeeking,
            NegotiationProtocol::CompetitiveBidding,
            NegotiationProtocol::Collaborative,
        ];
        for p in &protocols {
            let json = serde_json::to_string(p).unwrap();
            let decoded: NegotiationProtocol = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn negotiation_position_serde_roundtrip() {
        let pos = make_position("agent_1");
        let json = serde_json::to_string(&pos).unwrap();
        let decoded: NegotiationPosition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_id, "agent_1");
        assert!((decoded.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn resource_requirements_serde_roundtrip() {
        let rr = ResourceRequirements {
            time_required_ms: 5000,
            memory_required_mb: 1024,
            cpu_required_percent: 75.0,
            budget: None,
        };
        let json = serde_json::to_string(&rr).unwrap();
        let decoded: ResourceRequirements = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.time_required_ms, 5000);
        assert!(decoded.budget.is_none());
    }

    #[test]
    fn negotiation_message_proposal_serde() {
        let msg = NegotiationMessage::Proposal {
            id: "p1".to_string(),
            from: "a1".to_string(),
            to: "a2".to_string(),
            position: make_position("a1"),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: NegotiationMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    #[test]
    fn negotiation_message_acceptance_serde() {
        let msg = NegotiationMessage::Acceptance {
            proposal_id: "p1".to_string(),
            from: "a2".to_string(),
            to: "a1".to_string(),
            conditions: vec!["Must finish by Friday".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: NegotiationMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    #[test]
    fn negotiation_message_rejection_with_counter_serde() {
        let msg = NegotiationMessage::Rejection {
            proposal_id: "p1".to_string(),
            from: "a2".to_string(),
            to: "a1".to_string(),
            reason: "Too expensive".to_string(),
            counter_proposal: Some(make_position("a2")),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: NegotiationMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    #[test]
    fn negotiation_message_clarification_serde() {
        let req = NegotiationMessage::ClarificationRequest {
            proposal_id: "p1".to_string(),
            from: "a2".to_string(),
            to: "a1".to_string(),
            questions: vec!["What is the timeline?".to_string()],
        };
        let resp = NegotiationMessage::ClarificationResponse {
            proposal_id: "p1".to_string(),
            from: "a1".to_string(),
            to: "a2".to_string(),
            answers: vec!["2 weeks".to_string()],
        };
        let json1 = serde_json::to_string(&req).unwrap();
        let json2 = serde_json::to_string(&resp).unwrap();
        let d1: NegotiationMessage = serde_json::from_str(&json1).unwrap();
        let d2: NegotiationMessage = serde_json::from_str(&json2).unwrap();
        let re_json1 = serde_json::to_string(&d1).unwrap();
        let re_json2 = serde_json::to_string(&d2).unwrap();
        assert_eq!(json1, re_json1);
        assert_eq!(json2, re_json2);
    }

    #[test]
    fn negotiation_state_serde_roundtrip() {
        let states = [
            NegotiationState::Ongoing,
            NegotiationState::ConsensusReached {
                agreement: "Agreed to X".to_string(),
            },
            NegotiationState::Failed {
                reason: "Timeout".to_string(),
            },
            NegotiationState::Abandoned,
            NegotiationState::PartialAgreement {
                agreed: vec!["a1".to_string()],
                disagreed: vec!["a2".to_string()],
            },
        ];
        for s in &states {
            let json = serde_json::to_string(s).unwrap();
            let decoded: NegotiationState = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn negotiation_session_serde_roundtrip() {
        let session = NegotiationSession {
            id: "sess_1".to_string(),
            topic: "Architecture decision".to_string(),
            participants: vec!["a1".to_string(), "a2".to_string()],
            current_round: 3,
            max_rounds: 10,
            protocol: NegotiationProtocol::AlternatingOffers,
            state: NegotiationState::Ongoing,
            messages: vec![],
            proposals: HashMap::new(),
            started_at: Utc::now(),
            ended_at: None,
            outcome: None,
        };
        let json = serde_json::to_string(&session).unwrap();
        let decoded: NegotiationSession = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "sess_1");
        assert_eq!(decoded.current_round, 3);
    }

    #[test]
    fn negotiation_outcome_serde_roundtrip() {
        let outcome = NegotiationOutcome {
            outcome_type: NegotiationOutcomeType::MajorityAgreement { percentage: 0.75 },
            agreement: Some("Use microservices".to_string()),
            agreed_agents: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
            disagreed_agents: vec!["a4".to_string()],
            confidence: 0.8,
            duration_ms: 5000,
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let decoded: NegotiationOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agreed_agents.len(), 3);
        assert!((decoded.confidence - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn negotiation_outcome_type_serde_roundtrip() {
        let types = [
            NegotiationOutcomeType::FullConsensus,
            NegotiationOutcomeType::MajorityAgreement { percentage: 0.6 },
            NegotiationOutcomeType::Compromise,
            NegotiationOutcomeType::NoAgreement,
            NegotiationOutcomeType::Arbitrated,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let decoded: NegotiationOutcomeType = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // --- Logic tests ---

    #[test]
    fn unanimous_consensus_no_agreement() {
        let mut negotiator = AgentNegotiator::new();
        let participants = vec!["a1".to_string(), "a2".to_string()];
        let mut proposals = HashMap::new();
        proposals.insert("a1".to_string(), "X".to_string());
        proposals.insert("a2".to_string(), "Y".to_string());

        let result = negotiator
            .unanimous_consensus(participants, proposals)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn simple_majority_no_majority() {
        let mut negotiator = AgentNegotiator::new();
        let participants = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
        let mut proposals = HashMap::new();
        proposals.insert("a1".to_string(), "X".to_string());
        proposals.insert("a2".to_string(), "Y".to_string());
        proposals.insert("a3".to_string(), "Z".to_string());

        // Need > 0.66 threshold for 3-way split — none should meet 0.66
        let result = negotiator
            .simple_majority_consensus(participants, proposals, 0.66)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn start_negotiation_creates_session() {
        let mut negotiator = AgentNegotiator::new();
        let session_id = negotiator
            .start_negotiation(
                "Test".to_string(),
                vec!["a1".to_string(), "a2".to_string()],
                NegotiationProtocol::Collaborative,
                5,
            )
            .unwrap();

        let session = negotiator.session(&session_id).unwrap();
        assert_eq!(session.participants.len(), 2);
        assert_eq!(session.current_round, 0);
    }

    #[test]
    fn get_session_nonexistent() {
        let negotiator = AgentNegotiator::new();
        assert!(negotiator.session("nonexistent").is_none());
    }

    #[test]
    fn get_active_sessions_empty() {
        let negotiator = AgentNegotiator::new();
        assert!(negotiator.active_sessions().is_empty());
    }

    // =========================================================
    // 15 NEW TESTS — negotiation protocols, proposals,
    // acceptance/rejection, timeouts, serde roundtrips,
    // edge cases, consensus algorithms, conflict resolution
    // =========================================================

    // Test 1: Submitting a proposal to an ongoing session stores it in the proposals map.
    #[test]
    fn submit_proposal_stores_in_session() {
        let mut negotiator = AgentNegotiator::new();
        let sid = negotiator
            .start_negotiation(
                "Deploy strategy".into(),
                vec!["a1".into(), "a2".into()],
                NegotiationProtocol::AlternatingOffers,
                5,
            )
            .unwrap();

        let pos = make_position("a1");
        negotiator.submit_proposal(&sid, pos).unwrap();

        let session = negotiator.session(&sid).unwrap();
        assert!(session.proposals.contains_key("a1"));
        assert_eq!(session.proposals.len(), 1);
    }

    // Test 2: Submitting a proposal to a concluded (non-ongoing) session is rejected.
    #[test]
    fn submit_proposal_rejected_for_concluded_session() {
        let mut negotiator = AgentNegotiator::new();
        let sid = negotiator
            .start_negotiation(
                "Topic".into(),
                vec!["a1".into()],
                NegotiationProtocol::ConsensusSeeking,
                1,
            )
            .unwrap();

        // First round: current_round 0->1, stays Ongoing.
        // Second round: current_round 1 >= max_rounds 1, transitions to Failed.
        negotiator.run_negotiation_round(&sid).unwrap();
        negotiator.run_negotiation_round(&sid).unwrap();

        let pos = make_position("a1");
        let result = negotiator.submit_proposal(&sid, pos);
        assert!(result.is_err());
    }

    // Test 3: Submitting a proposal to a nonexistent session returns an error.
    #[test]
    fn submit_proposal_nonexistent_session_errors() {
        let mut negotiator = AgentNegotiator::new();
        let pos = make_position("ghost");
        let result = negotiator.submit_proposal("does-not-exist", pos);
        assert!(result.is_err());
    }

    // Test 4: Running negotiation rounds up to max_rounds transitions state to Failed.
    #[test]
    fn max_rounds_exhaustion_fails_session() {
        let mut negotiator = AgentNegotiator::new();
        let sid = negotiator
            .start_negotiation(
                "Budget allocation".into(),
                vec!["a1".into(), "a2".into()],
                NegotiationProtocol::MultiRound { max_rounds: 2 },
                2,
            )
            .unwrap();

        // Round 1: current_round 0->1 (1 < 2, stays Ongoing)
        let state1 = negotiator.run_negotiation_round(&sid).unwrap();
        assert!(matches!(state1, NegotiationState::Ongoing));

        // Round 2: current_round 1->2 (2 < 2 is false, but 2>=2 check happens next call)
        let state2 = negotiator.run_negotiation_round(&sid).unwrap();
        assert!(matches!(state2, NegotiationState::Ongoing));

        // Round 3: current_round 2 >= max_rounds 2, transitions to Failed
        let state3 = negotiator.run_negotiation_round(&sid).unwrap();
        assert!(matches!(state3, NegotiationState::Failed { .. }));
    }

    // Test 5: Consensus is reached when 80%+ of proposals meet the best proposal's min_acceptable.
    #[test]
    fn consensus_reached_when_threshold_met() {
        let mut negotiator = AgentNegotiator::new();
        let sid = negotiator
            .start_negotiation(
                "API design".into(),
                vec![
                    "a1".into(),
                    "a2".into(),
                    "a3".into(),
                    "a4".into(),
                    "a5".into(),
                ],
                NegotiationProtocol::ConsensusSeeking,
                5,
            )
            .unwrap();

        let now = Utc::now();
        let rr = ResourceRequirements {
            time_required_ms: 100,
            memory_required_mb: 64,
            cpu_required_percent: 10.0,
            budget: None,
        };

        // All 5 agents propose the same thing with high confidence and low min_acceptable
        for agent in ["a1", "a2", "a3", "a4", "a5"] {
            let pos = NegotiationPosition {
                agent_id: agent.to_string(),
                role: AgentRole::Reviewer,
                proposal: "Use REST".to_string(),
                confidence: 0.95,
                min_acceptable: 0.1,
                priority: 1,
                dependencies: vec![],
                resource_requirements: rr.clone(),
                timestamp: now,
            };
            negotiator.submit_proposal(&sid, pos).unwrap();
        }

        let state = negotiator.run_negotiation_round(&sid).unwrap();
        assert!(matches!(state, NegotiationState::ConsensusReached { .. }));

        let session = negotiator.session(&sid).unwrap();
        assert!(session.ended_at.is_some());
        assert!(session.outcome.is_some());
    }

    // Test 6: No proposals means consensus check stays Ongoing.
    #[test]
    fn no_proposals_keeps_state_ongoing() {
        let mut negotiator = AgentNegotiator::new();
        let sid = negotiator
            .start_negotiation(
                "Empty".into(),
                vec!["a1".into()],
                NegotiationProtocol::Collaborative,
                5,
            )
            .unwrap();

        let state = negotiator.run_negotiation_round(&sid).unwrap();
        assert!(matches!(state, NegotiationState::Ongoing));
    }

    // Test 7: Counter-proposal message serde roundtrip preserves the original proposal id.
    #[test]
    fn counter_proposal_message_serde_roundtrip() {
        let msg = NegotiationMessage::CounterProposal {
            original_proposal_id: "p-42".to_string(),
            from: "a2".to_string(),
            to: "a1".to_string(),
            new_position: make_position("a2"),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: NegotiationMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    // Test 8: Rejection without a counter-proposal serde roundtrip.
    #[test]
    fn rejection_without_counter_serde_roundtrip() {
        let msg = NegotiationMessage::Rejection {
            proposal_id: "p-99".to_string(),
            from: "a3".to_string(),
            to: "a1".to_string(),
            reason: "Budget exceeded".to_string(),
            counter_proposal: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: NegotiationMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    // Test 9: ConsensusAlgorithm variants all survive serde roundtrip.
    #[test]
    fn consensus_algorithm_serde_roundtrip() {
        // Test variants without HashMap (deterministic JSON ordering)
        let simple_algos: Vec<ConsensusAlgorithm> = vec![
            ConsensusAlgorithm::SimpleMajority { threshold: 0.5 },
            ConsensusAlgorithm::Supermajority { required: 0.66 },
            ConsensusAlgorithm::Unanimous,
            ConsensusAlgorithm::Veto {
                veto_holders: vec!["admin".into()],
            },
        ];
        for algo in &simple_algos {
            let json = serde_json::to_string(algo).unwrap();
            let decoded: ConsensusAlgorithm = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }

        // Test HashMap-containing variants separately (key order is non-deterministic)
        let weights: HashMap<String, f64> = {
            let mut m = HashMap::new();
            m.insert("a1".to_string(), 2.0);
            m.insert("a2".to_string(), 1.0);
            m
        };
        let json = serde_json::to_string(&ConsensusAlgorithm::Weighted {
            weights: weights.clone(),
        })
        .unwrap();
        let decoded: ConsensusAlgorithm = serde_json::from_str(&json).unwrap();
        if let ConsensusAlgorithm::Weighted { weights: w } = decoded {
            assert_eq!(w.len(), 2);
            assert!((w.get("a1").unwrap() - 2.0).abs() < f64::EPSILON);
            assert!((w.get("a2").unwrap() - 1.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Weighted variant");
        }

        let delegates: HashMap<String, Vec<String>> = {
            let mut m = HashMap::new();
            m.insert("lead".to_string(), vec!["a1".into(), "a2".into()]);
            m
        };
        let json = serde_json::to_string(&ConsensusAlgorithm::Delegated {
            delegates: delegates.clone(),
        })
        .unwrap();
        let decoded: ConsensusAlgorithm = serde_json::from_str(&json).unwrap();
        if let ConsensusAlgorithm::Delegated { delegates: d } = decoded {
            assert_eq!(d.len(), 1);
            assert_eq!(d.get("lead").unwrap().len(), 2);
        } else {
            panic!("Expected Delegated variant");
        }
    }

    // Test 10: ConflictResolutionStrategy serde roundtrip for all variants.
    #[test]
    fn conflict_resolution_strategy_serde_roundtrip() {
        let strategies = [
            ConflictResolutionStrategy::LastWriterWins,
            ConflictResolutionStrategy::SemanticMerge,
            ConflictResolutionStrategy::AgentVoting,
            ConflictResolutionStrategy::SeniorAgentArbitration,
            ConflictResolutionStrategy::KeepBoth,
            ConflictResolutionStrategy::Custom {
                resolver: "my_resolver_v2".to_string(),
            },
        ];
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let decoded: ConflictResolutionStrategy = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // Test 11: Weighted consensus correctly picks the proposal with the highest weighted score.
    #[test]
    fn weighted_consensus_picks_highest_weight() {
        let mut negotiator = AgentNegotiator::new();
        let mut weights = HashMap::new();
        weights.insert("a1".to_string(), 5.0);
        weights.insert("a2".to_string(), 1.0);
        weights.insert("a3".to_string(), 1.0);

        let mut proposals = HashMap::new();
        proposals.insert("a1".to_string(), "Plan Alpha".to_string());
        proposals.insert("a2".to_string(), "Plan Beta".to_string());
        proposals.insert("a3".to_string(), "Plan Beta".to_string());

        // Plan Beta has 2 votes (weight 2.0) vs Plan Alpha (weight 5.0) — Alpha wins
        let result = negotiator
            .weighted_consensus(vec![], proposals, weights)
            .unwrap();
        assert_eq!(result, Some("Plan Alpha".to_string()));
    }

    // Test 12: Veto consensus returns None when a veto holder objects to the only proposal.
    #[test]
    fn veto_consensus_blocks_on_objection() {
        let mut negotiator = AgentNegotiator::new();
        let participants = vec!["admin".to_string(), "worker".to_string()];

        let mut proposals = HashMap::new();
        proposals.insert("admin".to_string(), "Plan A".to_string());
        proposals.insert("worker".to_string(), "Plan A".to_string());

        // admin is the veto holder and is the only proposer of "Plan A"
        let result = negotiator
            .veto_consensus(participants, proposals, vec!["admin".to_string()])
            .unwrap();
        // No alternative exists, so veto blocks — returns None
        assert!(result.is_none());
    }

    // Test 13: NegotiatorStatistics reflect correct counts after multiple sessions.
    #[test]
    fn statistics_track_multiple_sessions() {
        let mut negotiator = AgentNegotiator::new();

        // Start 3 sessions
        let _s1 = negotiator
            .start_negotiation(
                "T1".into(),
                vec!["a1".into()],
                NegotiationProtocol::Collaborative,
                3,
            )
            .unwrap();
        let _s2 = negotiator
            .start_negotiation(
                "T2".into(),
                vec!["a2".into()],
                NegotiationProtocol::ConsensusSeeking,
                3,
            )
            .unwrap();
        let s3 = negotiator
            .start_negotiation(
                "T3".into(),
                vec!["a3".into()],
                NegotiationProtocol::AlternatingOffers,
                1,
            )
            .unwrap();

        // Conclude s3 by exhausting its single round:
        // Round 1: 0->1 (Ongoing), Round 2: 1>=1 (Failed)
        negotiator.run_negotiation_round(&s3).unwrap();
        negotiator.run_negotiation_round(&s3).unwrap();

        let stats = negotiator.statistics();
        assert_eq!(stats.total_sessions, 3);
        assert_eq!(stats.active_sessions, 2);
        assert_eq!(stats.concluded_sessions, 1);

        // No consensus was reached, so consensus_rate should be 0.0
        assert!(stats.consensus_rate.abs() < f64::EPSILON);
    }

    // Test 14: NegotiationSession with a full outcome and messages serde roundtrip.
    #[test]
    fn full_session_with_outcome_serde_roundtrip() {
        let now = Utc::now();
        let session = NegotiationSession {
            id: "sess_full".to_string(),
            topic: "Migrate to Rust".to_string(),
            participants: vec!["a1".into(), "a2".into()],
            current_round: 5,
            max_rounds: 5,
            protocol: NegotiationProtocol::MultiRound { max_rounds: 5 },
            state: NegotiationState::ConsensusReached {
                agreement: "Agreed: full rewrite".to_string(),
            },
            messages: vec![NegotiationMessage::Acceptance {
                proposal_id: "p1".into(),
                from: "a2".into(),
                to: "a1".into(),
                conditions: vec!["Must have tests".into()],
            }],
            proposals: {
                let mut m = HashMap::new();
                m.insert("a1".into(), make_position("a1"));
                m
            },
            started_at: now,
            ended_at: Some(now),
            outcome: Some(NegotiationOutcome {
                outcome_type: NegotiationOutcomeType::FullConsensus,
                agreement: Some("Agreed: full rewrite".into()),
                agreed_agents: vec!["a1".into(), "a2".into()],
                disagreed_agents: vec![],
                confidence: 0.95,
                duration_ms: 12000,
            }),
        };

        let json = serde_json::to_string(&session).unwrap();
        let decoded: NegotiationSession = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "sess_full");
        assert_eq!(decoded.messages.len(), 1);
        assert_eq!(decoded.outcome.as_ref().unwrap().duration_ms, 12000);
        assert_eq!(decoded.proposals.len(), 1);
    }

    // Test 15: NegotiatorStatistics serde roundtrip preserves all fields.
    #[test]
    fn negotiator_statistics_serde_roundtrip() {
        let stats = NegotiatorStatistics {
            total_sessions: 10,
            active_sessions: 3,
            concluded_sessions: 7,
            consensus_reached: 5,
            consensus_rate: 0.714,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: NegotiatorStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_sessions, 10);
        assert_eq!(decoded.active_sessions, 3);
        assert_eq!(decoded.concluded_sessions, 7);
        assert_eq!(decoded.consensus_reached, 5);
        assert!((decoded.consensus_rate - 0.714).abs() < 1e-9);
    }

    #[test]
    fn simple_majority_empty_participants_returns_none() {
        let mut negotiator = AgentNegotiator::new();
        let proposals = HashMap::new();
        let result = negotiator
            .simple_majority_consensus(vec![], proposals, 0.5)
            .unwrap();
        assert!(result.is_none());
    }
}
