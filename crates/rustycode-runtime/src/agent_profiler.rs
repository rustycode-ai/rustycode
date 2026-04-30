//! Agent Performance Profiling and Adaptive Selection
//!
//! This module provides:
//! - Detailed performance profiling for each agent
//! - Adaptive agent selection based on historical performance
//! - Agent specialization tracking
//! - Performance-based routing

use crate::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Performance profile for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPerformanceProfile {
    /// Agent role
    pub role: AgentRole,
    /// Total analyses performed
    pub total_analyses: usize,
    /// Successful analyses
    pub successful_analyses: usize,
    /// Average execution time (milliseconds)
    pub avg_execution_time_ms: f64,
    /// Average confidence score
    pub avg_confidence: f64,
    /// Specializations (task types this agent excels at)
    pub specializations: Vec<String>,
    /// Performance history
    pub performance_history: Vec<PerformanceRecord>,
    /// Last updated
    pub last_updated: DateTime<Utc>,
}

/// Single performance record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecord {
    /// Record ID
    pub id: String,
    /// Task description
    pub task: String,
    /// Execution time (milliseconds)
    pub execution_time_ms: u64,
    /// Confidence score
    pub confidence: f64,
    /// Success status
    pub success: bool,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Quality metrics
    pub quality_metrics: QualityMetrics,
}

/// Quality metrics for an analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Completeness (0.0 - 1.0)
    pub completeness: f64,
    /// Accuracy (0.0 - 1.0)
    pub accuracy: f64,
    /// Relevance (0.0 - 1.0)
    pub relevance: f64,
    /// Novel insights (0.0 - 1.0)
    pub novel_insights: f64,
    /// Overall quality score
    pub overall_quality: f64,
}

/// Agent selection criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionCriteria {
    /// Required specializations
    pub required_specializations: Vec<String>,
    /// Minimum confidence threshold
    pub min_confidence: f64,
    /// Maximum acceptable execution time
    pub max_execution_time_ms: u64,
    /// Prioritize speed over quality
    pub prioritize_speed: bool,
    /// Prioritize quality over speed
    pub prioritize_quality: bool,
    /// Budget constraints
    pub budget_constraint: Option<BudgetConstraint>,
}

/// Budget constraint for agent selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConstraint {
    /// Maximum total execution time (milliseconds)
    pub max_total_time_ms: u64,
    /// Maximum number of agents
    pub max_agents: usize,
    /// Cost per agent (arbitrary units)
    pub cost_per_agent: f64,
    /// Maximum total cost
    pub max_total_cost: f64,
}

/// Agent profiler and selector
pub struct AgentProfiler {
    /// Performance profiles for each agent role
    profiles: HashMap<AgentRole, AgentPerformanceProfile>,
    /// Maximum history size per agent
    max_history_size: usize,
}

impl AgentProfiler {
    /// Create a new agent profiler
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            max_history_size: 1000,
        }
    }

    /// Record agent performance
    pub fn record_performance(
        &mut self,
        role: AgentRole,
        task: String,
        execution_time_ms: u64,
        confidence: f64,
        success: bool,
        quality_metrics: QualityMetrics,
    ) {
        let profile = self
            .profiles
            .entry(role)
            .or_insert_with(|| AgentPerformanceProfile {
                role,
                total_analyses: 0,
                successful_analyses: 0,
                avg_execution_time_ms: 0.0,
                avg_confidence: 0.0,
                specializations: Vec::new(),
                performance_history: Vec::new(),
                last_updated: Utc::now(),
            });

        // Create performance record
        let record = PerformanceRecord {
            id: Uuid::new_v4().to_string(),
            task,
            execution_time_ms,
            confidence,
            success,
            timestamp: Utc::now(),
            quality_metrics,
        };

        // Add to history
        profile.performance_history.push(record);

        // Trim history if necessary
        if profile.performance_history.len() > self.max_history_size {
            profile
                .performance_history
                .drain(0..profile.performance_history.len() - self.max_history_size);
        }

        // Update statistics
        profile.total_analyses += 1;
        if success {
            profile.successful_analyses += 1;
        }

        // Update averages
        let total_time: u64 = profile
            .performance_history
            .iter()
            .map(|r| r.execution_time_ms)
            .sum();
        profile.avg_execution_time_ms = total_time as f64 / profile.total_analyses as f64;

        let total_confidence: f64 = profile
            .performance_history
            .iter()
            .map(|r| r.confidence)
            .sum();
        profile.avg_confidence = total_confidence / profile.total_analyses as f64;

        // Collect history for specialization update
        let recent_history: Vec<_> = profile
            .performance_history
            .iter()
            .rev()
            .take(20)
            .cloned()
            .collect();

        // Update specializations based on recent performance
        let mut specialization_scores: HashMap<String, f64> = HashMap::new();

        for record in &recent_history {
            let task_lower = record.task.to_lowercase();

            // Analyze task keywords
            if task_lower.contains("security") || task_lower.contains("vulnerability") {
                *specialization_scores
                    .entry("Security".to_string())
                    .or_insert(0.0_f64) += record.quality_metrics.overall_quality;
            }
            if task_lower.contains("performance") || task_lower.contains("optimization") {
                *specialization_scores
                    .entry("Performance".to_string())
                    .or_insert(0.0_f64) += record.quality_metrics.overall_quality;
            }
            if task_lower.contains("architecture") || task_lower.contains("design") {
                *specialization_scores
                    .entry("Architecture".to_string())
                    .or_insert(0.0_f64) += record.quality_metrics.overall_quality;
            }
            if task_lower.contains("test") || task_lower.contains("coverage") {
                *specialization_scores
                    .entry("Testing".to_string())
                    .or_insert(0.0_f64) += record.quality_metrics.overall_quality;
            }
            if task_lower.contains("document") {
                *specialization_scores
                    .entry("Documentation".to_string())
                    .or_insert(0.0_f64) += record.quality_metrics.overall_quality;
            }
        }

        // Update specializations (keep top performers)
        profile.specializations = specialization_scores
            .into_iter()
            .filter(|(_, score)| *score > 3.0_f64) // Threshold for good performance
            .map(|(spec, _)| spec)
            .collect();

        profile.last_updated = Utc::now();
    }

    /// Select best agents for a task based on criteria
    pub fn select_agents(
        &self,
        task: &str,
        available_roles: &[AgentRole],
        criteria: &SelectionCriteria,
    ) -> Vec<AgentRole> {
        let mut scored_agents: Vec<(AgentRole, f64)> = available_roles
            .iter()
            .map(|role| {
                let score = self.calculate_agent_score(role, task, criteria);
                (*role, score)
            })
            .collect();

        // Sort by score (descending)
        scored_agents.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Apply budget constraints
        let max_agents = criteria
            .budget_constraint
            .as_ref()
            .map_or(available_roles.len(), |b| b.max_agents);

        // Filter by minimum confidence
        let selected: Vec<AgentRole> = scored_agents
            .into_iter()
            .filter(|(_, score)| *score >= criteria.min_confidence)
            .take(max_agents)
            .map(|(role, _)| role)
            .collect();

        selected
    }

    /// Calculate score for an agent based on task and criteria
    fn calculate_agent_score(
        &self,
        role: &AgentRole,
        task: &str,
        criteria: &SelectionCriteria,
    ) -> f64 {
        let profile = match self.profiles.get(role) {
            Some(p) => p,
            None => return 0.5_f64, // Default score for unknown agent
        };

        let mut score: f64 = 0.0;

        // Base score from success rate
        let success_rate = if profile.total_analyses > 0 {
            profile.successful_analyses as f64 / profile.total_analyses as f64
        } else {
            0.5_f64
        };
        score += success_rate * 0.3_f64;

        // Confidence score
        score += profile.avg_confidence * 0.2_f64;

        // Specialization match
        let task_lower = task.to_lowercase();
        for spec in &profile.specializations {
            if task_lower.contains(&spec.to_lowercase()) {
                score += 0.3_f64;
                break;
            }
        }

        // Speed vs Quality tradeoff
        if criteria.prioritize_speed {
            // Prefer faster agents (lower execution time)
            let speed_score = if profile.avg_execution_time_ms > 0.0 {
                (1000.0_f64 / profile.avg_execution_time_ms).min(1.0_f64)
            } else {
                0.5_f64
            };
            score += speed_score * 0.2_f64;
        } else if criteria.prioritize_quality {
            // Prefer higher quality (using avg_confidence as proxy)
            score += profile.avg_confidence * 0.2_f64;
        }

        score.min(1.0_f64)
    }

    /// Get performance profile for an agent
    pub fn get_profile(&self, role: AgentRole) -> Option<&AgentPerformanceProfile> {
        self.profiles.get(&role)
    }

    /// Get all profiles
    pub fn get_all_profiles(&self) -> &HashMap<AgentRole, AgentPerformanceProfile> {
        &self.profiles
    }

    /// Calculate performance statistics
    pub fn get_statistics(&self) -> ProfilerStatistics {
        let total_analyses: usize = self.profiles.values().map(|p| p.total_analyses).sum();

        let total_successful: usize = self.profiles.values().map(|p| p.successful_analyses).sum();

        let avg_execution_time: f64 = if self.profiles.is_empty() {
            0.0_f64
        } else {
            self.profiles
                .values()
                .map(|p| p.avg_execution_time_ms)
                .sum::<f64>()
                / self.profiles.len() as f64
        };

        let avg_confidence: f64 = if self.profiles.is_empty() {
            0.0_f64
        } else {
            self.profiles
                .values()
                .map(|p| p.avg_confidence)
                .sum::<f64>()
                / self.profiles.len() as f64
        };

        ProfilerStatistics {
            total_agents: self.profiles.len(),
            total_analyses,
            total_successful,
            overall_success_rate: if total_analyses > 0 {
                total_successful as f64 / total_analyses as f64
            } else {
                0.0_f64
            },
            avg_execution_time_ms: avg_execution_time,
            avg_confidence,
        }
    }

    /// Export profiles to JSON
    pub fn export_profiles(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.profiles)
    }

    /// Import profiles from JSON
    pub fn import_profiles(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let imported: HashMap<AgentRole, AgentPerformanceProfile> = serde_json::from_str(json)?;
        self.profiles = imported;
        Ok(())
    }
}

/// Profiler statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerStatistics {
    pub total_agents: usize,
    pub total_analyses: usize,
    pub total_successful: usize,
    pub overall_success_rate: f64,
    pub avg_execution_time_ms: f64,
    pub avg_confidence: f64,
}

impl Default for AgentProfiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_creation() {
        let profiler = AgentProfiler::new();
        assert_eq!(profiler.get_all_profiles().len(), 0);
    }

    #[test]
    fn test_record_performance() {
        let mut profiler = AgentProfiler::new();

        profiler.record_performance(
            AgentRole::Skeptic,
            "Review security".to_string(),
            100,
            0.9,
            true,
            QualityMetrics {
                completeness: 0.9,
                accuracy: 0.95,
                relevance: 0.88,
                novel_insights: 0.75,
                overall_quality: 0.87,
            },
        );

        let profile = profiler.get_profile(AgentRole::Skeptic);
        assert!(profile.is_some());
        let profile = profile.unwrap();
        assert_eq!(profile.total_analyses, 1);
        assert_eq!(profile.successful_analyses, 1);
    }

    #[test]
    fn test_agent_selection() {
        let mut profiler = AgentProfiler::new();

        // Add some performance data
        profiler.record_performance(
            AgentRole::Skeptic,
            "Security review".to_string(),
            50,
            0.95,
            true,
            QualityMetrics {
                completeness: 0.95,
                accuracy: 0.98,
                relevance: 0.92,
                novel_insights: 0.85,
                overall_quality: 0.93,
            },
        );

        profiler.record_performance(
            AgentRole::Researcher,
            "Performance review".to_string(),
            80,
            0.85,
            true,
            QualityMetrics {
                completeness: 0.85,
                accuracy: 0.88,
                relevance: 0.90,
                novel_insights: 0.80,
                overall_quality: 0.86,
            },
        );

        let criteria = SelectionCriteria {
            required_specializations: vec![],
            min_confidence: 0.6, // Lower threshold since agents have limited history
            max_execution_time_ms: 1000,
            prioritize_speed: true,
            prioritize_quality: false,
            budget_constraint: None,
        };

        let selected = profiler.select_agents(
            "Security review",
            &[AgentRole::Skeptic, AgentRole::Researcher],
            &criteria,
        );

        // At least one agent should be selected
        assert!(!selected.is_empty());
        // SecurityExpert should be selected (fastest and most relevant)
        assert!(selected.contains(&AgentRole::Skeptic));
    }

    // --- Serde roundtrip tests ---

    #[test]
    fn quality_metrics_serde_roundtrip() {
        let m = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.95,
            relevance: 0.88,
            novel_insights: 0.75,
            overall_quality: 0.87,
        };
        let json = serde_json::to_string(&m).unwrap();
        let decoded: QualityMetrics = serde_json::from_str(&json).unwrap();
        assert!((decoded.completeness - 0.9).abs() < f64::EPSILON);
        assert!((decoded.overall_quality - 0.87).abs() < f64::EPSILON);
    }

    #[test]
    fn performance_record_serde_roundtrip() {
        let r = PerformanceRecord {
            id: "rec_1".to_string(),
            task: "Review code".to_string(),
            execution_time_ms: 500,
            confidence: 0.92,
            success: true,
            timestamp: Utc::now(),
            quality_metrics: QualityMetrics {
                completeness: 1.0,
                accuracy: 1.0,
                relevance: 1.0,
                novel_insights: 0.5,
                overall_quality: 0.9,
            },
        };
        let json = serde_json::to_string(&r).unwrap();
        let decoded: PerformanceRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "rec_1");
        assert_eq!(decoded.execution_time_ms, 500);
        assert!(decoded.success);
    }

    #[test]
    fn agent_performance_profile_serde_roundtrip() {
        let p = AgentPerformanceProfile {
            role: AgentRole::Skeptic,
            total_analyses: 10,
            successful_analyses: 8,
            avg_execution_time_ms: 120.5,
            avg_confidence: 0.88,
            specializations: vec!["Security".to_string()],
            performance_history: vec![],
            last_updated: Utc::now(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let decoded: AgentPerformanceProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_analyses, 10);
        assert_eq!(decoded.successful_analyses, 8);
        assert_eq!(decoded.specializations.len(), 1);
    }

    #[test]
    fn selection_criteria_serde_roundtrip() {
        let c = SelectionCriteria {
            required_specializations: vec!["Security".to_string()],
            min_confidence: 0.8,
            max_execution_time_ms: 5000,
            prioritize_speed: true,
            prioritize_quality: false,
            budget_constraint: Some(BudgetConstraint {
                max_total_time_ms: 30000,
                max_agents: 5,
                cost_per_agent: 0.10,
                max_total_cost: 1.0,
            }),
        };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: SelectionCriteria = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.required_specializations.len(), 1);
        assert!(decoded.budget_constraint.is_some());
        assert_eq!(decoded.budget_constraint.unwrap().max_agents, 5);
    }

    #[test]
    fn budget_constraint_serde_roundtrip() {
        let b = BudgetConstraint {
            max_total_time_ms: 60000,
            max_agents: 3,
            cost_per_agent: 0.25,
            max_total_cost: 5.0,
        };
        let json = serde_json::to_string(&b).unwrap();
        let decoded: BudgetConstraint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_total_time_ms, 60000);
        assert!((decoded.cost_per_agent - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn profiler_statistics_serde_roundtrip() {
        let s = ProfilerStatistics {
            total_agents: 4,
            total_analyses: 100,
            total_successful: 92,
            overall_success_rate: 0.92,
            avg_execution_time_ms: 150.0,
            avg_confidence: 0.85,
        };
        let json = serde_json::to_string(&s).unwrap();
        let decoded: ProfilerStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_agents, 4);
        assert!((decoded.overall_success_rate - 0.92).abs() < 0.001);
    }

    // --- Profiler logic tests ---

    #[test]
    fn profiler_default_matches_new() {
        let p1 = AgentProfiler::new();
        let p2 = AgentProfiler::default();
        assert_eq!(p1.get_all_profiles().len(), p2.get_all_profiles().len());
    }

    #[test]
    fn profiler_records_multiple_performances() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.5,
            overall_quality: 0.8,
        };

        for _ in 0..5 {
            profiler.record_performance(
                AgentRole::Reviewer,
                "Code review".to_string(),
                100,
                0.9,
                true,
                qm.clone(),
            );
        }

        let profile = profiler.get_profile(AgentRole::Reviewer).unwrap();
        assert_eq!(profile.total_analyses, 5);
        assert_eq!(profile.successful_analyses, 5);
        assert!((profile.avg_confidence - 0.9).abs() < 0.001);
    }

    #[test]
    fn profiler_tracks_failed_analyses() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.5,
            accuracy: 0.5,
            relevance: 0.5,
            novel_insights: 0.1,
            overall_quality: 0.4,
        };

        profiler.record_performance(AgentRole::Reviewer, "task".to_string(), 200, 0.3, false, qm);

        let profile = profiler.get_profile(AgentRole::Reviewer).unwrap();
        assert_eq!(profile.total_analyses, 1);
        assert_eq!(profile.successful_analyses, 0);
    }

    #[test]
    fn profiler_detects_specializations() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.8,
            overall_quality: 0.9,
        };

        // Add enough security-related records to exceed the 3.0 threshold
        for _ in 0..5 {
            profiler.record_performance(
                AgentRole::Skeptic,
                "security vulnerability analysis".to_string(),
                100,
                0.9,
                true,
                qm.clone(),
            );
        }

        let profile = profiler.get_profile(AgentRole::Skeptic).unwrap();
        assert!(profile.specializations.contains(&"Security".to_string()));
    }

    #[test]
    fn profiler_get_statistics_empty() {
        let profiler = AgentProfiler::new();
        let stats = profiler.get_statistics();
        assert_eq!(stats.total_agents, 0);
        assert_eq!(stats.total_analyses, 0);
        assert_eq!(stats.overall_success_rate, 0.0);
    }

    #[test]
    fn profiler_get_statistics_with_data() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.5,
            overall_quality: 0.85,
        };

        profiler.record_performance(
            AgentRole::Reviewer,
            "task1".to_string(),
            100,
            0.9,
            true,
            qm.clone(),
        );
        profiler.record_performance(AgentRole::Skeptic, "task2".to_string(), 200, 0.8, false, qm);

        let stats = profiler.get_statistics();
        assert_eq!(stats.total_agents, 2);
        assert_eq!(stats.total_analyses, 2);
        assert_eq!(stats.total_successful, 1);
        assert!((stats.overall_success_rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn profiler_export_import_roundtrip() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.5,
            overall_quality: 0.85,
        };
        profiler.record_performance(AgentRole::Reviewer, "task".to_string(), 100, 0.9, true, qm);

        let json = profiler.export_profiles().unwrap();

        let mut profiler2 = AgentProfiler::new();
        profiler2.import_profiles(&json).unwrap();

        assert_eq!(
            profiler.get_all_profiles().len(),
            profiler2.get_all_profiles().len()
        );
        let p2 = profiler2.get_profile(AgentRole::Reviewer).unwrap();
        assert_eq!(p2.total_analyses, 1);
    }

    #[test]
    fn profiler_select_agents_empty_profiles() {
        let profiler = AgentProfiler::new();
        let criteria = SelectionCriteria {
            required_specializations: vec![],
            min_confidence: 0.0,
            max_execution_time_ms: 10000,
            prioritize_speed: false,
            prioritize_quality: false,
            budget_constraint: None,
        };
        let selected = profiler.select_agents("any task", &[AgentRole::Reviewer], &criteria);
        // Unknown agents get default score of 0.5 which is >= 0.0 min_confidence
        assert!(!selected.is_empty());
    }

    #[test]
    fn profiler_select_agents_with_budget_constraint() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.5,
            overall_quality: 0.85,
        };

        for _ in 0..3 {
            profiler.record_performance(
                AgentRole::Reviewer,
                "task".to_string(),
                100,
                0.9,
                true,
                qm.clone(),
            );
            profiler.record_performance(
                AgentRole::Skeptic,
                "security task".to_string(),
                100,
                0.9,
                true,
                qm.clone(),
            );
            profiler.record_performance(
                AgentRole::Researcher,
                "perf task".to_string(),
                100,
                0.9,
                true,
                qm.clone(),
            );
        }

        let criteria = SelectionCriteria {
            required_specializations: vec![],
            min_confidence: 0.0,
            max_execution_time_ms: 10000,
            prioritize_speed: false,
            prioritize_quality: false,
            budget_constraint: Some(BudgetConstraint {
                max_total_time_ms: 30000,
                max_agents: 2,
                cost_per_agent: 0.10,
                max_total_cost: 1.0,
            }),
        };

        let selected = profiler.select_agents(
            "task",
            &[
                AgentRole::Reviewer,
                AgentRole::Skeptic,
                AgentRole::Researcher,
            ],
            &criteria,
        );
        assert!(selected.len() <= 2);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for agent_profiler
    // =========================================================================

    // 1. SelectionCriteria with no budget constraint serde roundtrip
    #[test]
    fn selection_criteria_no_budget_serde() {
        let c = SelectionCriteria {
            required_specializations: vec![],
            min_confidence: 0.5,
            max_execution_time_ms: 2000,
            prioritize_speed: false,
            prioritize_quality: true,
            budget_constraint: None,
        };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: SelectionCriteria = serde_json::from_str(&json).unwrap();
        assert!(decoded.budget_constraint.is_none());
        assert!(decoded.prioritize_quality);
    }

    // 2. PerformanceRecord with failed status serde roundtrip
    #[test]
    fn performance_record_failed_serde() {
        let r = PerformanceRecord {
            id: "rec_fail".into(),
            task: "Bad analysis".into(),
            execution_time_ms: 9999,
            confidence: 0.1,
            success: false,
            timestamp: Utc::now(),
            quality_metrics: QualityMetrics {
                completeness: 0.2,
                accuracy: 0.1,
                relevance: 0.3,
                novel_insights: 0.0,
                overall_quality: 0.15,
            },
        };
        let json = serde_json::to_string(&r).unwrap();
        let decoded: PerformanceRecord = serde_json::from_str(&json).unwrap();
        assert!(!decoded.success);
        assert_eq!(decoded.execution_time_ms, 9999);
    }

    // 3. AgentPerformanceProfile with non-empty history serde roundtrip
    #[test]
    fn agent_profile_with_history_serde() {
        let p = AgentPerformanceProfile {
            role: AgentRole::Researcher,
            total_analyses: 3,
            successful_analyses: 2,
            avg_execution_time_ms: 250.0,
            avg_confidence: 0.80,
            specializations: vec!["Performance".into()],
            performance_history: vec![PerformanceRecord {
                id: "r1".into(),
                task: "Profile loop".into(),
                execution_time_ms: 200,
                confidence: 0.85,
                success: true,
                timestamp: Utc::now(),
                quality_metrics: QualityMetrics {
                    completeness: 0.9,
                    accuracy: 0.9,
                    relevance: 0.9,
                    novel_insights: 0.5,
                    overall_quality: 0.85,
                },
            }],
            last_updated: Utc::now(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let decoded: AgentPerformanceProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.performance_history.len(), 1);
        assert_eq!(decoded.specializations.len(), 1);
    }

    // 4. Profiler detects Performance specialization
    #[test]
    fn profiler_detects_performance_specialization() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.8,
            overall_quality: 0.9,
        };
        for _ in 0..5 {
            profiler.record_performance(
                AgentRole::Researcher,
                "performance optimization analysis".into(),
                100,
                0.9,
                true,
                qm.clone(),
            );
        }
        let profile = profiler.get_profile(AgentRole::Researcher).unwrap();
        assert!(profile.specializations.contains(&"Performance".to_string()));
    }

    // 5. Profiler detects Testing specialization
    #[test]
    fn profiler_detects_testing_specialization() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.8,
            overall_quality: 0.9,
        };
        for _ in 0..5 {
            profiler.record_performance(
                AgentRole::Reviewer,
                "test coverage analysis".into(),
                100,
                0.9,
                true,
                qm.clone(),
            );
        }
        let profile = profiler.get_profile(AgentRole::Reviewer).unwrap();
        assert!(profile.specializations.contains(&"Testing".to_string()));
    }

    // 6. Profiler detects Documentation specialization
    #[test]
    fn profiler_detects_documentation_specialization() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.8,
            overall_quality: 0.9,
        };
        for _ in 0..5 {
            profiler.record_performance(
                AgentRole::Worker,
                "document review analysis".into(),
                100,
                0.9,
                true,
                qm.clone(),
            );
        }
        let profile = profiler.get_profile(AgentRole::Worker).unwrap();
        assert!(profile
            .specializations
            .contains(&"Documentation".to_string()));
    }

    // 7. Profiler detects Architecture specialization
    #[test]
    fn profiler_detects_architecture_specialization() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.8,
            overall_quality: 0.9,
        };
        for _ in 0..5 {
            profiler.record_performance(
                AgentRole::Reviewer,
                "architecture design review".into(),
                100,
                0.9,
                true,
                qm.clone(),
            );
        }
        let profile = profiler.get_profile(AgentRole::Reviewer).unwrap();
        assert!(profile
            .specializations
            .contains(&"Architecture".to_string()));
    }

    // 8. get_profile returns None for unknown agent
    #[test]
    fn profiler_get_profile_unknown_returns_none() {
        let profiler = AgentProfiler::new();
        assert!(profiler.get_profile(AgentRole::Reviewer).is_none());
    }

    // 9. QualityMetrics with zero values serde roundtrip
    #[test]
    fn quality_metrics_zero_values_serde() {
        let m = QualityMetrics {
            completeness: 0.0,
            accuracy: 0.0,
            relevance: 0.0,
            novel_insights: 0.0,
            overall_quality: 0.0,
        };
        let json = serde_json::to_string(&m).unwrap();
        let decoded: QualityMetrics = serde_json::from_str(&json).unwrap();
        assert!(decoded.completeness.abs() < f64::EPSILON);
        assert!(decoded.overall_quality.abs() < f64::EPSILON);
    }

    // 10. SelectionCriteria with empty specializations serde
    #[test]
    fn selection_criteria_empty_specializations_serde() {
        let c = SelectionCriteria {
            required_specializations: vec![],
            min_confidence: 0.0,
            max_execution_time_ms: 0,
            prioritize_speed: false,
            prioritize_quality: false,
            budget_constraint: None,
        };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: SelectionCriteria = serde_json::from_str(&json).unwrap();
        assert!(decoded.required_specializations.is_empty());
        assert!(decoded.min_confidence.abs() < f64::EPSILON);
    }

    // 11. BudgetConstraint zero cost serde roundtrip
    #[test]
    fn budget_constraint_zero_cost_serde() {
        let b = BudgetConstraint {
            max_total_time_ms: 0,
            max_agents: 0,
            cost_per_agent: 0.0,
            max_total_cost: 0.0,
        };
        let json = serde_json::to_string(&b).unwrap();
        let decoded: BudgetConstraint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_agents, 0);
        assert!(decoded.cost_per_agent.abs() < f64::EPSILON);
    }

    // 12. ProfilerStatistics zero values serde roundtrip
    #[test]
    fn profiler_statistics_zero_values_serde() {
        let s = ProfilerStatistics {
            total_agents: 0,
            total_analyses: 0,
            total_successful: 0,
            overall_success_rate: 0.0,
            avg_execution_time_ms: 0.0,
            avg_confidence: 0.0,
        };
        let json = serde_json::to_string(&s).unwrap();
        let decoded: ProfilerStatistics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_agents, 0);
        assert_eq!(decoded.total_analyses, 0);
    }

    // 13. Multiple agents recording
    #[test]
    fn profiler_multiple_agents_recording() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.5,
            overall_quality: 0.85,
        };
        let roles = [
            AgentRole::Skeptic,
            AgentRole::Reviewer,
            AgentRole::Researcher,
        ];
        for role in &roles {
            profiler.record_performance(*role, "task".into(), 100, 0.9, true, qm.clone());
        }
        assert_eq!(profiler.get_all_profiles().len(), 3);
    }

    // 14. Export empty profiles produces valid JSON
    #[test]
    fn profiler_export_empty_profiles() {
        let profiler = AgentProfiler::new();
        let json = profiler.export_profiles().unwrap();
        assert_eq!(json, "{}");
    }

    // 15. Import empty profiles
    #[test]
    fn profiler_import_empty_profiles() {
        let mut profiler = AgentProfiler::new();
        profiler.record_performance(
            AgentRole::Reviewer,
            "task".into(),
            100,
            0.9,
            true,
            QualityMetrics {
                completeness: 0.9,
                accuracy: 0.9,
                relevance: 0.9,
                novel_insights: 0.5,
                overall_quality: 0.85,
            },
        );
        assert_eq!(profiler.get_all_profiles().len(), 1);
        profiler.import_profiles("{}").unwrap();
        assert_eq!(profiler.get_all_profiles().len(), 0);
    }

    #[test]
    fn profiler_select_agents_min_confidence_filters() {
        let mut profiler = AgentProfiler::new();
        let qm = QualityMetrics {
            completeness: 0.9,
            accuracy: 0.9,
            relevance: 0.9,
            novel_insights: 0.5,
            overall_quality: 0.85,
        };
        profiler.record_performance(
            AgentRole::Reviewer,
            "task".to_string(),
            100,
            0.5, // Low confidence
            true,
            qm,
        );

        let criteria = SelectionCriteria {
            required_specializations: vec![],
            min_confidence: 0.99, // Very high threshold
            max_execution_time_ms: 10000,
            prioritize_speed: false,
            prioritize_quality: false,
            budget_constraint: None,
        };

        let selected = profiler.select_agents("task", &[AgentRole::Reviewer], &criteria);
        // Score should be < 0.99, so no agent selected
        assert!(selected.is_empty());
    }
}
