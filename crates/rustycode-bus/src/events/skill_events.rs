// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Skill-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillActivatedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub trigger: String,
    pub activated_at: DateTime<Utc>,
}

impl SkillActivatedEvent {
    pub fn new(skill_name: String, trigger: String) -> Self {
        let now = Utc::now();
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: now,
            activated_at: now,
            skill_name,
            trigger,
        }
    }
}

impl Event for SkillActivatedEvent {
    fn event_type(&self) -> &'static str {
        "skill.activated"
    }
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillDeactivatedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub reason: String,
    pub duration_secs: f64,
    pub tokens_used: u64,
}

impl SkillDeactivatedEvent {
    pub fn new(skill_name: String, reason: String, duration_secs: f64, tokens_used: u64) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            skill_name,
            reason,
            duration_secs,
            tokens_used,
        }
    }
}

impl Event for SkillDeactivatedEvent {
    fn event_type(&self) -> &'static str {
        "skill.deactivated"
    }
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillSuggestedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub reason: String,
    pub score: f64,
    pub unmatched_signals: Vec<String>,
}

impl SkillSuggestedEvent {
    pub fn new(
        skill_name: String,
        reason: String,
        score: f64,
        unmatched_signals: Vec<String>,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            skill_name,
            reason,
            score,
            unmatched_signals,
        }
    }
}

impl Event for SkillSuggestedEvent {
    fn event_type(&self) -> &'static str {
        "skill.suggested"
    }
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillQualityAssessedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub skill_name: String,
    pub grade: String,
    pub telemetry_score: f64,
    pub graph_score: f64,
    pub intake_score: f64,
    pub routing_score: f64,
}

impl SkillQualityAssessedEvent {
    pub fn new(
        skill_name: String,
        grade: String,
        telemetry_score: f64,
        graph_score: f64,
        intake_score: f64,
        routing_score: f64,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            skill_name,
            grade,
            telemetry_score,
            graph_score,
            intake_score,
            routing_score,
        }
    }
}

impl Event for SkillQualityAssessedEvent {
    fn event_type(&self) -> &'static str {
        "skill.quality.assessed"
    }
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_activated_event() {
        let event = SkillActivatedEvent::new("code-review".to_string(), "path-match".to_string());
        assert_eq!(event.event_type(), "skill.activated");
        assert!(event.event_id.is_some());
        assert_eq!(event.skill_name, "code-review");
        assert_eq!(event.trigger, "path-match");
    }

    #[test]
    fn test_skill_deactivated_event() {
        let event = SkillDeactivatedEvent::new(
            "code-review".to_string(),
            "session-ended".to_string(),
            120.5,
            5000,
        );
        assert_eq!(event.event_type(), "skill.deactivated");
        assert_eq!(event.duration_secs, 120.5);
        assert_eq!(event.tokens_used, 5000);
    }

    #[test]
    fn test_skill_suggested_event() {
        let event = SkillSuggestedEvent::new(
            "tdd".to_string(),
            "file-pattern-match".to_string(),
            0.85,
            vec!["test-file".to_string()],
        );
        assert_eq!(event.event_type(), "skill.suggested");
        assert_eq!(event.score, 0.85);
        assert_eq!(event.unmatched_signals.len(), 1);
    }

    #[test]
    fn test_skill_quality_assessed_event() {
        let event = SkillQualityAssessedEvent::new(
            "code-review".to_string(),
            "good".to_string(),
            0.9,
            0.7,
            0.8,
            0.6,
        );
        assert_eq!(event.event_type(), "skill.quality.assessed");
        assert_eq!(event.telemetry_score, 0.9);
    }

    #[test]
    fn test_skill_activated_serialization_roundtrip() {
        let event = SkillActivatedEvent::new("test-skill".to_string(), "manual".to_string());
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["skill_name"], "test-skill");
        assert_eq!(json["trigger"], "manual");
        let decoded: SkillActivatedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.skill_name, "test-skill");
    }

    #[test]
    fn test_skill_deactivated_serialization_roundtrip() {
        let event =
            SkillDeactivatedEvent::new("test-skill".to_string(), "timeout".to_string(), 30.0, 1000);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["duration_secs"], 30.0);
        let decoded: SkillDeactivatedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.tokens_used, 1000);
    }

    #[test]
    fn test_skill_suggested_serialization_roundtrip() {
        let event = SkillSuggestedEvent::new(
            "tdd".to_string(),
            "context-match".to_string(),
            0.9,
            vec!["a".to_string(), "b".to_string()],
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SkillSuggestedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.unmatched_signals, vec!["a", "b"]);
    }

    #[test]
    fn test_skill_quality_assessed_serialization_roundtrip() {
        let event = SkillQualityAssessedEvent::new(
            "review".to_string(),
            "excellent".to_string(),
            0.95,
            0.88,
            0.92,
            0.85,
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SkillQualityAssessedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.grade, "excellent");
    }
}
