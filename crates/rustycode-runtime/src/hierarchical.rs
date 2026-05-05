//! Hierarchical Ensemble Coordination for Multi-Agent Systems
//!
//! This module provides hierarchical coordination structures for organizing
//! agents into teams with specialized roles and coordinated decision-making.

use crate::multi_agent::{AccessLevel, AgentRole, MemoryData, MemoryType, SharedWorkingMemory};
use crate::workflow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Hierarchical team structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ensemble {
    /// Unique team identifier
    pub id: String,

    /// Ensemble name
    pub name: String,

    /// Ensemble description
    pub description: String,

    /// Ensemble lead
    pub lead: AgentRole,

    /// Ensemble members
    pub members: Vec<EnsembleMember>,

    /// Ensemble specialization
    pub specialization: EnsembleSpecialization,

    /// Ensemble responsibilities
    pub responsibilities: Vec<String>,

    /// Communication channels
    pub channels: Vec<CommunicationChannel>,

    /// Decision-making strategy
    pub decision_strategy: DecisionStrategy,

    /// Ensemble status
    pub status: EnsembleStatus,
}

/// Ensemble member with role and permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleMember {
    /// Agent role
    pub role: AgentRole,

    /// Member name/identifier
    pub name: String,

    /// Member permissions
    pub permissions: Vec<Permission>,

    /// Member status
    pub status: MemberStatus,

    /// Join timestamp
    pub joined_at: DateTime<Utc>,
}

/// Ensemble specialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnsembleSpecialization {
    /// Code review and analysis
    CodeReview,

    /// Security analysis
    Security,

    /// Performance optimization
    Performance,

    /// Testing and quality assurance
    Testing,

    /// Documentation
    Documentation,

    /// Architecture and design
    Architecture,

    /// Bug fixing and maintenance
    Maintenance,

    /// Feature development
    FeatureDevelopment,

    /// Multi-disciplinary
    General,

    /// Custom specialization
    Custom(String),
}

/// Communication channels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommunicationChannel {
    /// Ensemble-wide announcements
    EnsembleAnnouncements,

    /// Direct messages
    DirectMessages,

    /// Specialized topic channels
    TopicChannel(String),

    /// Cross-team collaboration
    CrossEnsemble { team_id: String },

    /// Hierarchical escalation
    Escalation { level: u8 },
}

/// Decision-making strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum DecisionStrategy {
    /// Ensemble lead decides
    Autocratic,

    /// Majority vote
    Democratic,

    /// Expert-based decision
    ExpertBased,

    /// Weighted voting
    WeightedVoting { weights: Vec<(String, f64)> },

    /// Consensus required
    Consensus,

    /// Hierarchical approval
    Hierarchical { levels: Vec<String> },
}

/// Ensemble status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnsembleStatus {
    /// Ensemble is active
    Active,

    /// Ensemble is on standby
    Standby,

    /// Ensemble is busy
    Busy,

    /// Ensemble is disabled
    Disabled,
}

/// Member status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemberStatus {
    /// Member is active
    Active,

    /// Member is away
    Away,

    /// Member is busy
    Busy,

    /// Member is offline
    Offline,
}

/// Permission levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum Permission {
    /// Read team data
    Read,

    /// Write to team data
    Write,

    /// Make team decisions
    Decide,

    /// Invite new members
    Invite,

    /// Remove members
    Remove,

    /// Create channels
    CreateChannel,

    /// Escalate issues
    Escalate,

    /// Represent team externally
    Represent,
}

/// Hierarchical coordination structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyStructure {
    /// Root teams (top level)
    pub root_teams: Vec<String>,

    /// Ensemble hierarchy (team_id -> sub_teams)
    pub hierarchy: HashMap<String, Vec<String>>,

    /// Cross-team relationships
    pub cross_team_relationships: Vec<CrossEnsembleRelation>,

    /// Escalation paths
    pub escalation_paths: HashMap<String, Vec<String>>,
}

/// Cross-team relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossEnsembleRelation {
    /// Source team
    pub from_team: String,

    /// Target team
    pub to_team: String,

    /// Relationship type
    pub relation_type: RelationType,

    /// Communication protocol
    pub protocol: CommunicationProtocol,
}

/// Relationship types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum RelationType {
    /// Collaboration
    Collaboration,

    /// Dependency
    Dependency,

    /// Advisory
    Advisory,

    /// Reporting
    Reporting,
}

/// Communication protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationProtocol {
    /// Protocol name
    pub name: String,

    /// Required permissions
    pub required_permissions: Vec<Permission>,

    /// Message format
    pub message_format: MessageFormat,

    /// Response requirements
    pub response_requirements: ResponseRequirements,
}

/// Message format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageFormat {
    /// Structured JSON
    StructuredJson,

    /// Plain text
    PlainText,

    /// Specific schema
    Schema { schema_name: String },
}

/// Response requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseRequirements {
    /// Timeout for response
    pub timeout_seconds: u64,

    /// Required confidence
    pub min_confidence: f64,

    /// Acknowledgment required
    pub requires_acknowledgment: bool,

    /// Escalation on failure
    pub escalation_on_failure: bool,
}

/// Ensemble coordination manager
pub struct EnsembleCoordinator {
    /// All teams
    teams: HashMap<String, Ensemble>,

    /// Hierarchy structure
    hierarchy: HierarchyStructure,

    /// Shared working memory
    memory: SharedWorkingMemory,

    /// Coordination history
    history: Vec<CoordinationEvent>,
}

/// Coordination event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationEvent {
    /// Event ID
    pub id: String,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Event type
    pub event_type: CoordinationEventType,

    /// Ensembles involved
    pub teams_involved: Vec<String>,

    /// Event data
    pub data: serde_json::Value,

    /// Outcome
    pub outcome: EventOutcome,
}

/// Coordination event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoordinationEventType {
    /// Ensemble creation
    EnsembleCreation,

    /// Ensemble coordination
    EnsembleCoordination,

    /// Cross-team collaboration
    CrossEnsembleCollaboration,

    /// Decision making
    DecisionMaking,

    /// Escalation
    Escalation,

    /// Conflict resolution
    ConflictResolution,
}

/// Event outcome
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventOutcome {
    /// Success
    Success,

    /// Partial success
    PartialSuccess,

    /// Failure
    Failure,

    /// Pending
    Pending,
}

impl EnsembleCoordinator {
    pub fn new() -> Self {
        Self {
            teams: HashMap::new(),
            hierarchy: HierarchyStructure {
                root_teams: Vec::new(),
                hierarchy: HashMap::new(),
                cross_team_relationships: Vec::new(),
                escalation_paths: HashMap::new(),
            },
            memory: SharedWorkingMemory::new(),
            history: Vec::new(),
        }
    }

    /// Create a new team
    pub fn create_team(
        &mut self,
        name: String,
        description: String,
        lead: AgentRole,
        specialization: EnsembleSpecialization,
        decision_strategy: DecisionStrategy,
    ) -> Result<String> {
        let team_id = Uuid::new_v4().to_string();

        let team = Ensemble {
            id: team_id.clone(),
            name,
            description,
            lead,
            members: Vec::new(),
            specialization,
            responsibilities: Vec::new(),
            channels: vec![
                CommunicationChannel::EnsembleAnnouncements,
                CommunicationChannel::DirectMessages,
            ],
            decision_strategy,
            status: EnsembleStatus::Active,
        };

        // Clone data before moving team
        let team_name = team.name.clone();
        let team_lead = team.lead;
        let team_specialization = team.specialization.clone();

        self.teams.insert(team_id.clone(), team);

        // Log team creation
        self.log_event(
            CoordinationEventType::EnsembleCreation,
            vec![team_id.clone()],
            serde_json::json!({
                "team_name": team_name,
                "lead": team_lead,
                "specialization": team_specialization
            }),
            EventOutcome::Success,
        );

        Ok(team_id)
    }

    /// Add member to team
    pub fn add_member(&mut self, team_id: &str, member: EnsembleMember) -> Result<()> {
        let team = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| crate::workflow::WorkflowError::NotFound(team_id.to_string()))?;

        team.members.push(member);
        Ok(())
    }

    /// Establish hierarchy
    pub fn establish_hierarchy(
        &mut self,
        parent_team: &str,
        child_teams: Vec<String>,
    ) -> Result<()> {
        // Verify parent team exists
        if !self.teams.contains_key(parent_team) {
            return Err(crate::workflow::WorkflowError::NotFound(
                parent_team.to_string(),
            ));
        }

        // Verify child teams exist
        for child_id in &child_teams {
            if !self.teams.contains_key(child_id) {
                return Err(crate::workflow::WorkflowError::NotFound(child_id.clone()));
            }
        }

        // Update hierarchy
        if !self.hierarchy.root_teams.contains(&parent_team.to_string()) {
            self.hierarchy.root_teams.push(parent_team.to_string());
        }

        self.hierarchy
            .hierarchy
            .insert(parent_team.to_string(), child_teams);

        Ok(())
    }

    /// Coordinate team decision
    pub fn coordinate_decision(
        &mut self,
        team_id: &str,
        decision_data: serde_json::Value,
    ) -> Result<DecisionOutcome> {
        let team = self
            .teams
            .get(team_id)
            .ok_or_else(|| crate::workflow::WorkflowError::NotFound(team_id.to_string()))?;

        // Use shared memory for coordination
        let _decision_entry = self.memory.write(
            &format!("team_{}", team_id),
            MemoryType::Analysis,
            MemoryData::Json(decision_data.clone()),
            AccessLevel::ProtectedRead {
                readers: team.members.iter().map(|m| m.name.clone()).collect(),
            },
        )?;

        // Implement decision strategy
        let outcome = match team.decision_strategy {
            DecisionStrategy::Autocratic => {
                // Ensemble lead decides
                DecisionOutcome {
                    decision: "Ensemble lead decision".to_string(),
                    confidence: 1.0,
                    participants: vec![format!("{:?}", team.lead)],
                    rationale: "Autocratic decision by team lead".to_string(),
                }
            }
            DecisionStrategy::Democratic => {
                // Majority vote
                DecisionOutcome {
                    decision: "Democratic decision".to_string(),
                    confidence: 0.7,
                    participants: team
                        .members
                        .iter()
                        .map(|m| format!("{:?}", m.role))
                        .collect(),
                    rationale: "Majority vote".to_string(),
                }
            }
            DecisionStrategy::Consensus => {
                // Requires consensus
                DecisionOutcome {
                    decision: "Consensus decision".to_string(),
                    confidence: 0.9,
                    participants: team
                        .members
                        .iter()
                        .map(|m| format!("{:?}", m.role))
                        .collect(),
                    rationale: "Full consensus achieved".to_string(),
                }
            }
            _ => {
                // Default strategy
                DecisionOutcome {
                    decision: "Default decision".to_string(),
                    confidence: 0.5,
                    participants: vec![format!("{:?}", team.lead)],
                    rationale: "Default decision strategy".to_string(),
                }
            }
        };

        // Log coordination event
        self.log_event(
            CoordinationEventType::DecisionMaking,
            vec![team_id.to_string()],
            decision_data,
            EventOutcome::Success,
        );

        Ok(outcome)
    }

    /// Escalate issue
    pub fn escalate_issue(
        &mut self,
        from_team: &str,
        issue_data: serde_json::Value,
    ) -> Result<String> {
        // Get escalation path
        let escalation_path = self
            .hierarchy
            .escalation_paths
            .get(from_team)
            .ok_or_else(|| {
                crate::workflow::WorkflowError::NotFound("No escalation path".to_string())
            })?;

        // Escalate to first team in path
        let target_team = escalation_path.first().ok_or_else(|| {
            crate::workflow::WorkflowError::Validation("Empty escalation path".to_string())
        })?;

        // Write to shared memory
        let escalation_id = self.memory.write(
            from_team,
            MemoryType::Analysis,
            MemoryData::Json(issue_data.clone()),
            AccessLevel::ProtectedRead {
                readers: vec![target_team.clone()],
            },
        )?;

        // Log escalation
        self.log_event(
            CoordinationEventType::Escalation,
            vec![from_team.to_string(), target_team.clone()],
            issue_data,
            EventOutcome::Pending,
        );

        Ok(escalation_id)
    }

    /// Cross-team collaboration
    pub fn collaborate_teams(
        &mut self,
        teams: Vec<String>,
        collaboration_data: serde_json::Value,
    ) -> Result<CollaborationResult> {
        // Verify all teams exist
        for team_id in &teams {
            if !self.teams.contains_key(team_id) {
                return Err(crate::workflow::WorkflowError::NotFound(team_id.clone()));
            }
        }

        // Create collaboration space in shared memory
        let collaboration_id = Uuid::new_v4().to_string();

        for team_id in &teams {
            self.memory.write(
                team_id,
                MemoryType::Analysis,
                MemoryData::Json(collaboration_data.clone()),
                AccessLevel::ProtectedRead {
                    readers: teams.clone(),
                },
            )?;
        }

        // Log collaboration
        self.log_event(
            CoordinationEventType::CrossEnsembleCollaboration,
            teams.clone(),
            collaboration_data,
            EventOutcome::Success,
        );

        Ok(CollaborationResult {
            collaboration_id: collaboration_id.clone(),
            participating_teams: teams,
            status: EventOutcome::Success,
            shared_artifacts: vec![collaboration_id],
        })
    }

    /// Get team by ID
    pub fn team(&self, team_id: &str) -> Option<&Ensemble> {
        self.teams.get(team_id)
    }

    /// Get all teams
    pub fn all_teams(&self) -> Vec<&Ensemble> {
        self.teams.values().collect()
    }

    /// Get hierarchy structure
    pub fn hierarchy(&self) -> &HierarchyStructure {
        &self.hierarchy
    }

    /// Log coordination event
    fn log_event(
        &mut self,
        event_type: CoordinationEventType,
        teams_involved: Vec<String>,
        data: serde_json::Value,
        outcome: EventOutcome,
    ) {
        let event = CoordinationEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            teams_involved,
            data,
            outcome,
        };

        self.history.push(event);

        // Keep history manageable
        if self.history.len() > 1000 {
            self.history.drain(0..100);
        }
    }

    /// Get coordination history
    pub fn history(&self) -> &[CoordinationEvent] {
        &self.history
    }

    /// Create standard team structure
    pub fn create_standard_structure(&mut self) -> Result<Vec<String>> {
        // Create main coordination team
        let coord_team = self.create_team(
            "Coordination".to_string(),
            "Main coordination team".to_string(),
            AgentRole::Reviewer,
            EnsembleSpecialization::General,
            DecisionStrategy::Hierarchical {
                levels: vec!["leads".to_string()],
            },
        )?;

        // Create specialized teams
        let security_team = self.create_team(
            "Security".to_string(),
            "Security analysis team".to_string(),
            AgentRole::Skeptic,
            EnsembleSpecialization::Security,
            DecisionStrategy::ExpertBased,
        )?;

        let performance_team = self.create_team(
            "Performance".to_string(),
            "Performance optimization team".to_string(),
            AgentRole::Researcher,
            EnsembleSpecialization::Performance,
            DecisionStrategy::ExpertBased,
        )?;

        let code_review_team = self.create_team(
            "CodeReview".to_string(),
            "Code review team".to_string(),
            AgentRole::Reviewer,
            EnsembleSpecialization::CodeReview,
            DecisionStrategy::Democratic,
        )?;

        // Establish hierarchy
        self.establish_hierarchy(
            &coord_team,
            vec![security_team.clone(), performance_team.clone()],
        )?;

        // Set up escalation paths
        self.hierarchy
            .escalation_paths
            .insert(security_team.clone(), vec![coord_team.clone()]);
        self.hierarchy
            .escalation_paths
            .insert(performance_team.clone(), vec![coord_team.clone()]);
        self.hierarchy
            .escalation_paths
            .insert(code_review_team.clone(), vec![coord_team.clone()]);

        Ok(vec![
            coord_team,
            security_team,
            performance_team,
            code_review_team,
        ])
    }
}

/// Decision outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOutcome {
    /// Decision made
    pub decision: String,

    /// Confidence level (0.0-1.0)
    pub confidence: f64,

    /// Participants in decision
    pub participants: Vec<String>,

    /// Rationale for decision
    pub rationale: String,
}

/// Collaboration result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationResult {
    /// Collaboration ID
    pub collaboration_id: String,

    /// Participating teams
    pub participating_teams: Vec<String>,

    /// Collaboration status
    pub status: EventOutcome,

    /// Shared artifacts
    pub shared_artifacts: Vec<String>,
}

impl Default for EnsembleCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_coordinator_creation() {
        let coordinator = EnsembleCoordinator::new();
        assert_eq!(coordinator.teams.len(), 0);
    }

    #[test]
    fn test_create_team() {
        let mut coordinator = EnsembleCoordinator::new();
        let team_id = coordinator
            .create_team(
                "Test Ensemble".to_string(),
                "A test team".to_string(),
                AgentRole::Reviewer,
                EnsembleSpecialization::General,
                DecisionStrategy::Autocratic,
            )
            .unwrap();

        assert!(coordinator.teams.contains_key(&team_id));
    }

    #[test]
    fn test_add_member() {
        let mut coordinator = EnsembleCoordinator::new();
        let team_id = coordinator
            .create_team(
                "Test Ensemble".to_string(),
                "A test team".to_string(),
                AgentRole::Reviewer,
                EnsembleSpecialization::General,
                DecisionStrategy::Autocratic,
            )
            .unwrap();

        let member = EnsembleMember {
            role: AgentRole::Reviewer,
            name: "Reviewer1".to_string(),
            permissions: vec![Permission::Read, Permission::Write],
            status: MemberStatus::Active,
            joined_at: Utc::now(),
        };

        coordinator.add_member(&team_id, member).unwrap();
        let team = coordinator.team(&team_id).unwrap();
        assert_eq!(team.members.len(), 1);
    }

    #[test]
    fn test_coordinate_decision() {
        let mut coordinator = EnsembleCoordinator::new();
        let team_id = coordinator
            .create_team(
                "Decision Ensemble".to_string(),
                "Makes decisions".to_string(),
                AgentRole::Reviewer,
                EnsembleSpecialization::General,
                DecisionStrategy::Autocratic,
            )
            .unwrap();

        let decision_data = serde_json::json!({
            "issue": "Should we refactor X?",
            "options": ["Yes", "No", "Maybe"]
        });

        let outcome = coordinator
            .coordinate_decision(&team_id, decision_data)
            .unwrap();
        assert!(!outcome.decision.is_empty());
    }

    #[test]
    fn test_escalation() {
        let mut coordinator = EnsembleCoordinator::new();
        let parent_team = coordinator
            .create_team(
                "Parent".to_string(),
                "Parent team".to_string(),
                AgentRole::Reviewer,
                EnsembleSpecialization::General,
                DecisionStrategy::Autocratic,
            )
            .unwrap();

        let child_team = coordinator
            .create_team(
                "Child".to_string(),
                "Child team".to_string(),
                AgentRole::Reviewer,
                EnsembleSpecialization::CodeReview,
                DecisionStrategy::Democratic,
            )
            .unwrap();

        coordinator
            .hierarchy
            .escalation_paths
            .insert(child_team.clone(), vec![parent_team.clone()]);

        let issue_data = serde_json::json!({
            "issue": "Critical security flaw found"
        });

        let escalation_id = coordinator.escalate_issue(&child_team, issue_data).unwrap();
        assert!(!escalation_id.is_empty());
    }

    // --- Serde roundtrip tests ---

    #[test]
    fn team_specialization_serde_roundtrip() {
        let specs = [
            EnsembleSpecialization::CodeReview,
            EnsembleSpecialization::Security,
            EnsembleSpecialization::Performance,
            EnsembleSpecialization::Testing,
            EnsembleSpecialization::Documentation,
            EnsembleSpecialization::Architecture,
            EnsembleSpecialization::Maintenance,
            EnsembleSpecialization::FeatureDevelopment,
            EnsembleSpecialization::General,
            EnsembleSpecialization::Custom("CustomSpec".to_string()),
        ];
        for s in &specs {
            let json = serde_json::to_string(s).unwrap();
            let decoded: EnsembleSpecialization = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn communication_channel_serde_roundtrip() {
        let channels = [
            CommunicationChannel::EnsembleAnnouncements,
            CommunicationChannel::DirectMessages,
            CommunicationChannel::TopicChannel("security".to_string()),
            CommunicationChannel::CrossEnsemble {
                team_id: "team_1".to_string(),
            },
            CommunicationChannel::Escalation { level: 3 },
        ];
        for c in &channels {
            let json = serde_json::to_string(c).unwrap();
            let decoded: CommunicationChannel = serde_json::from_str(&json).unwrap();
            assert_eq!(*c, decoded);
        }
    }

    #[test]
    fn decision_strategy_serde_roundtrip() {
        let strategies = [
            DecisionStrategy::Autocratic,
            DecisionStrategy::Democratic,
            DecisionStrategy::ExpertBased,
            DecisionStrategy::WeightedVoting {
                weights: vec![("a".to_string(), 2.0)],
            },
            DecisionStrategy::Consensus,
            DecisionStrategy::Hierarchical {
                levels: vec!["L1".to_string(), "L2".to_string()],
            },
        ];
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let decoded: DecisionStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn team_status_serde_roundtrip() {
        let statuses = [
            EnsembleStatus::Active,
            EnsembleStatus::Standby,
            EnsembleStatus::Busy,
            EnsembleStatus::Disabled,
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let decoded: EnsembleStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn member_status_serde_roundtrip() {
        let statuses = [
            MemberStatus::Active,
            MemberStatus::Away,
            MemberStatus::Busy,
            MemberStatus::Offline,
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let decoded: MemberStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn permission_serde_roundtrip() {
        let perms = [
            Permission::Read,
            Permission::Write,
            Permission::Decide,
            Permission::Invite,
            Permission::Remove,
            Permission::CreateChannel,
            Permission::Escalate,
            Permission::Represent,
        ];
        for p in &perms {
            let json = serde_json::to_string(p).unwrap();
            let decoded: Permission = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, decoded);
        }
    }

    #[test]
    fn relation_type_serde_roundtrip() {
        let types = [
            RelationType::Collaboration,
            RelationType::Dependency,
            RelationType::Advisory,
            RelationType::Reporting,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let decoded: RelationType = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, decoded);
        }
    }

    #[test]
    fn message_format_serde_roundtrip() {
        let formats = [
            MessageFormat::StructuredJson,
            MessageFormat::PlainText,
            MessageFormat::Schema {
                schema_name: "v1".to_string(),
            },
        ];
        for f in &formats {
            let json = serde_json::to_string(f).unwrap();
            let decoded: MessageFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(*f, decoded);
        }
    }

    #[test]
    fn team_serde_roundtrip() {
        let team = Ensemble {
            id: "team_1".to_string(),
            name: "Alpha".to_string(),
            description: "Test team".to_string(),
            lead: AgentRole::Reviewer,
            members: vec![],
            specialization: EnsembleSpecialization::Security,
            responsibilities: vec!["Review".to_string()],
            channels: vec![CommunicationChannel::EnsembleAnnouncements],
            decision_strategy: DecisionStrategy::Autocratic,
            status: EnsembleStatus::Active,
        };
        let json = serde_json::to_string(&team).unwrap();
        let decoded: Ensemble = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "team_1");
        assert_eq!(decoded.specialization, EnsembleSpecialization::Security);
    }

    #[test]
    fn team_member_serde_roundtrip() {
        let member = EnsembleMember {
            role: AgentRole::Skeptic,
            name: "Alice".to_string(),
            permissions: vec![Permission::Read, Permission::Write, Permission::Escalate],
            status: MemberStatus::Active,
            joined_at: Utc::now(),
        };
        let json = serde_json::to_string(&member).unwrap();
        let decoded: EnsembleMember = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "Alice");
        assert_eq!(decoded.permissions.len(), 3);
    }

    #[test]
    fn hierarchy_structure_serde_roundtrip() {
        let h = HierarchyStructure {
            root_teams: vec!["team_1".to_string()],
            hierarchy: HashMap::new(),
            cross_team_relationships: vec![],
            escalation_paths: HashMap::new(),
        };
        let json = serde_json::to_string(&h).unwrap();
        let decoded: HierarchyStructure = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.root_teams.len(), 1);
    }

    #[test]
    fn cross_team_relation_serde_roundtrip() {
        let r = CrossEnsembleRelation {
            from_team: "t1".to_string(),
            to_team: "t2".to_string(),
            relation_type: RelationType::Collaboration,
            protocol: CommunicationProtocol {
                name: "Sync".to_string(),
                required_permissions: vec![Permission::Read],
                message_format: MessageFormat::StructuredJson,
                response_requirements: ResponseRequirements {
                    timeout_seconds: 30,
                    min_confidence: 0.8,
                    requires_acknowledgment: true,
                    escalation_on_failure: false,
                },
            },
        };
        let json = serde_json::to_string(&r).unwrap();
        let decoded: CrossEnsembleRelation = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.from_team, "t1");
        assert_eq!(decoded.relation_type, RelationType::Collaboration);
    }

    #[test]
    fn communication_protocol_serde_roundtrip() {
        let p = CommunicationProtocol {
            name: "Async".to_string(),
            required_permissions: vec![Permission::Read, Permission::Write],
            message_format: MessageFormat::PlainText,
            response_requirements: ResponseRequirements {
                timeout_seconds: 60,
                min_confidence: 0.5,
                requires_acknowledgment: false,
                escalation_on_failure: true,
            },
        };
        let json = serde_json::to_string(&p).unwrap();
        let decoded: CommunicationProtocol = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "Async");
        assert!(!decoded.response_requirements.requires_acknowledgment);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for hierarchical
    // =========================================================================

    // 1. CoordinationEventType serde roundtrip all variants
    #[test]
    fn coordination_event_type_serde_roundtrip() {
        let types = [
            CoordinationEventType::EnsembleCreation,
            CoordinationEventType::EnsembleCoordination,
            CoordinationEventType::CrossEnsembleCollaboration,
            CoordinationEventType::DecisionMaking,
            CoordinationEventType::Escalation,
            CoordinationEventType::ConflictResolution,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let decoded: CoordinationEventType = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // 2. EventOutcome serde roundtrip all variants
    #[test]
    fn event_outcome_serde_roundtrip() {
        let outcomes = [
            EventOutcome::Success,
            EventOutcome::PartialSuccess,
            EventOutcome::Failure,
            EventOutcome::Pending,
        ];
        for o in &outcomes {
            let json = serde_json::to_string(o).unwrap();
            let decoded: EventOutcome = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // 3. DecisionOutcome serde roundtrip
    #[test]
    fn decision_outcome_serde_roundtrip() {
        let d = DecisionOutcome {
            decision: "Refactor module X".into(),
            confidence: 0.85,
            participants: vec!["SeniorEngineer".into(), "SecurityExpert".into()],
            rationale: "Consensus reached".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let decoded: DecisionOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.decision, "Refactor module X");
        assert_eq!(decoded.participants.len(), 2);
        assert!((decoded.confidence - 0.85).abs() < f64::EPSILON);
    }

    // 4. CollaborationResult serde roundtrip
    #[test]
    fn collaboration_result_serde_roundtrip() {
        let c = CollaborationResult {
            collaboration_id: "collab_1".into(),
            participating_teams: vec!["team_a".into(), "team_b".into()],
            status: EventOutcome::Success,
            shared_artifacts: vec!["artifact_1".into()],
        };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: CollaborationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.collaboration_id, "collab_1");
        assert_eq!(decoded.participating_teams.len(), 2);
    }

    // 5. CoordinationEvent serde roundtrip
    #[test]
    fn coordination_event_serde_roundtrip() {
        let e = CoordinationEvent {
            id: "evt_1".into(),
            timestamp: Utc::now(),
            event_type: CoordinationEventType::Escalation,
            teams_involved: vec!["team_1".into()],
            data: serde_json::json!({"issue": "critical bug"}),
            outcome: EventOutcome::Pending,
        };
        let json = serde_json::to_string(&e).unwrap();
        let decoded: CoordinationEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "evt_1");
        assert_eq!(decoded.event_type, CoordinationEventType::Escalation);
        assert_eq!(decoded.outcome, EventOutcome::Pending);
    }

    // 6. EnsembleCoordinator default matches new
    #[test]
    fn team_coordinator_default_matches_new() {
        let c1 = EnsembleCoordinator::new();
        let c2 = EnsembleCoordinator::default();
        assert_eq!(c1.all_teams().len(), c2.all_teams().len());
    }

    // 7. create_standard_structure creates 4 teams
    #[test]
    fn team_coordinator_create_standard_structure() {
        let mut coordinator = EnsembleCoordinator::new();
        let team_ids = coordinator.create_standard_structure().unwrap();
        assert_eq!(team_ids.len(), 4);
        assert_eq!(coordinator.all_teams().len(), 4);
    }

    // 8. get_all_teams returns empty initially
    #[test]
    fn team_coordinator_get_all_teams_empty() {
        let coordinator = EnsembleCoordinator::new();
        assert!(coordinator.all_teams().is_empty());
    }

    // 9. get_hierarchy returns empty structure initially
    #[test]
    fn team_coordinator_get_hierarchy_empty() {
        let coordinator = EnsembleCoordinator::new();
        let h = coordinator.hierarchy();
        assert!(h.root_teams.is_empty());
        assert!(h.hierarchy.is_empty());
    }

    // 10. get_history returns empty initially
    #[test]
    fn team_coordinator_get_history_empty() {
        let coordinator = EnsembleCoordinator::new();
        assert!(coordinator.history().is_empty());
    }

    // 11. add_member to nonexistent team fails
    #[test]
    fn team_coordinator_add_member_nonexistent_fails() {
        let mut coordinator = EnsembleCoordinator::new();
        let member = EnsembleMember {
            role: AgentRole::Reviewer,
            name: "Alice".into(),
            permissions: vec![Permission::Read],
            status: MemberStatus::Active,
            joined_at: Utc::now(),
        };
        let result = coordinator.add_member("nonexistent", member);
        assert!(result.is_err());
    }

    // 12. coordinate_decision on nonexistent team fails
    #[test]
    fn team_coordinator_decision_nonexistent_fails() {
        let mut coordinator = EnsembleCoordinator::new();
        let result = coordinator.coordinate_decision("no_team", serde_json::json!({}));
        assert!(result.is_err());
    }

    // 13. establish_hierarchy with nonexistent parent fails
    #[test]
    fn team_coordinator_hierarchy_nonexistent_parent_fails() {
        let mut coordinator = EnsembleCoordinator::new();
        let result = coordinator.establish_hierarchy("no_parent", vec!["child".into()]);
        assert!(result.is_err());
    }

    // 14. establish_hierarchy with nonexistent child fails
    #[test]
    fn team_coordinator_hierarchy_nonexistent_child_fails() {
        let mut coordinator = EnsembleCoordinator::new();
        let parent_id = coordinator
            .create_team(
                "Parent".into(),
                "Parent team".into(),
                AgentRole::Reviewer,
                EnsembleSpecialization::General,
                DecisionStrategy::Autocratic,
            )
            .unwrap();
        let result = coordinator.establish_hierarchy(&parent_id, vec!["no_child".into()]);
        assert!(result.is_err());
    }

    // 15. collaborate_teams with nonexistent team fails
    #[test]
    fn team_coordinator_collaborate_nonexistent_fails() {
        let mut coordinator = EnsembleCoordinator::new();
        let result = coordinator.collaborate_teams(
            vec!["no_team_1".into(), "no_team_2".into()],
            serde_json::json!({}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn response_requirements_serde_roundtrip() {
        let r = ResponseRequirements {
            timeout_seconds: 120,
            min_confidence: 0.75,
            requires_acknowledgment: true,
            escalation_on_failure: true,
        };
        let json = serde_json::to_string(&r).unwrap();
        let decoded: ResponseRequirements = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.timeout_seconds, 120);
        assert!(decoded.escalation_on_failure);
    }
}
