//! Benchmark Leaderboard and Continuous Monitoring
//!
//! This module provides leaderboard functionality and continuous monitoring
//! for tracking RustyCode performance over time.

use crate::benchmark::task_evaluator::{TaskDifficulty, TaskEvaluation};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info};

/// Leaderboard entry for a specific session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    /// Session identifier
    pub session_id: String,

    pub timestamp: DateTime<Utc>,

    /// RustyCode version
    pub version: String,

    pub overall_score: f64,

    pub success_rate: f64,

    pub average_speedup: f64,

    pub tasks_completed: usize,

    pub total_tasks: usize,

    pub metadata: LeaderboardMetadata,
}

/// Additional metadata for leaderboard entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardMetadata {
    pub llm_provider: String,

    /// Model used
    pub model: String,

    /// System information
    pub system_info: String,

    /// Configuration tags
    pub tags: Vec<String>,
}

/// Performance trend data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrend {
    pub timestamps: Vec<DateTime<Utc>>,

    /// Overall scores over time
    pub overall_scores: Vec<f64>,

    /// Success rates over time
    pub success_rates: Vec<f64>,

    /// Speedup ratios over time
    pub speedup_ratios: Vec<f64>,

    /// Moving average (7 sessions)
    pub moving_average: Vec<f64>,
}

/// Difficulty-specific performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyPerformance {
    /// Easy task performance
    pub easy: f64,

    /// Medium task performance
    pub medium: f64,

    /// Hard task performance
    pub hard: f64,

    /// Expert task performance
    pub expert: f64,
}

/// Benchmark leaderboard
pub struct BenchmarkLeaderboard {
    /// Leaderboard entries
    entries: Vec<LeaderboardEntry>,

    storage_path: PathBuf,

    /// Maximum entries to keep
    max_entries: usize,
}

impl BenchmarkLeaderboard {
    pub fn new(storage_path: PathBuf, max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            storage_path,
            max_entries,
        }
    }

    /// Add an entry to the leaderboard
    pub fn add_entry(&mut self, entry: LeaderboardEntry) {
        self.entries.push(entry);
        self.entries.sort_by(|a, b| {
            b.overall_score
                .partial_cmp(&a.overall_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Keep only the top entries
        if self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }
    }

    /// Get the top N entries
    pub fn top(&self, n: usize) -> Vec<&LeaderboardEntry> {
        self.entries.iter().take(n).collect()
    }

    /// Get the current best score
    pub fn best_score(&self) -> Option<f64> {
        self.entries.first().map(|e| e.overall_score)
    }

    /// Get average score across all entries
    pub fn average_score(&self) -> f64 {
        if self.entries.is_empty() {
            return 0.0;
        }
        self.entries.iter().map(|e| e.overall_score).sum::<f64>() / self.entries.len() as f64
    }

    /// Calculate performance trend
    pub fn calculate_trend(&self) -> PerformanceTrend {
        let mut timestamps = Vec::new();
        let mut overall_scores = Vec::new();
        let mut success_rates = Vec::new();
        let mut speedup_ratios = Vec::new();

        // Sort by timestamp
        let mut sorted_entries = self.entries.clone();
        sorted_entries.sort_by_key(|a| a.timestamp);

        for entry in &sorted_entries {
            timestamps.push(entry.timestamp);
            overall_scores.push(entry.overall_score);
            success_rates.push(entry.success_rate);
            speedup_ratios.push(entry.average_speedup);
        }

        // Calculate moving average
        let moving_average = self.calculate_moving_average(&overall_scores, 7);

        PerformanceTrend {
            timestamps,
            overall_scores,
            success_rates,
            speedup_ratios,
            moving_average,
        }
    }

    /// Calculate moving average
    fn calculate_moving_average(&self, data: &[f64], window: usize) -> Vec<f64> {
        if data.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        for i in 0..data.len() {
            let start = i.saturating_sub(window);
            let slice = &data[start..=i];
            let avg: f64 = slice.iter().sum::<f64>() / slice.len() as f64;
            result.push(avg);
        }
        result
    }

    /// Calculate difficulty-specific performance
    pub fn calculate_difficulty_performance(
        &self,
        evaluations: &[TaskEvaluation],
    ) -> DifficultyPerformance {
        let mut scores: HashMap<TaskDifficulty, Vec<f64>> = HashMap::new();

        for eval in evaluations {
            scores
                .entry(eval.task.difficulty)
                .or_default()
                .push(eval.overall_score as f64);
        }

        let avg = |difficulty: TaskDifficulty| -> f64 {
            scores.get(&difficulty).map_or(0.0, |scores| {
                scores.iter().sum::<f64>() / scores.len() as f64
            })
        };

        DifficultyPerformance {
            easy: avg(TaskDifficulty::Easy),
            medium: avg(TaskDifficulty::Medium),
            hard: avg(TaskDifficulty::Hard),
            expert: avg(TaskDifficulty::Expert),
        }
    }

    /// Save leaderboard to disk
    pub async fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| format!("Failed to serialize leaderboard: {}", e))?;

        fs::write(&self.storage_path, json)
            .await
            .map_err(|e| format!("Failed to write leaderboard: {}", e))?;

        info!("Saved leaderboard to {:?}", self.storage_path);
        Ok(())
    }

    /// Load leaderboard from disk
    pub async fn load(&mut self) -> Result<(), String> {
        if !self.storage_path.exists() {
            debug!("Leaderboard file does not exist, starting fresh");
            return Ok(());
        }

        let content = fs::read_to_string(&self.storage_path)
            .await
            .map_err(|e| format!("Failed to read leaderboard: {}", e))?;

        self.entries = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to deserialize leaderboard: {}", e))?;

        info!("Loaded leaderboard with {} entries", self.entries.len());
        Ok(())
    }

    /// Generate leaderboard report
    pub fn generate_report(&self) -> String {
        let mut report = String::from("# Benchmark Leaderboard\n\n");

        if self.entries.is_empty() {
            report.push_str("No entries yet.\n");
            return report;
        }

        report.push_str("## Top Performances\n\n");
        report.push_str("| Rank | Session | Date | Score | Success Rate | Speedup |\n");
        report.push_str("|------|---------|------|-------|--------------|--------|\n");

        for (i, entry) in self.entries.iter().take(10).enumerate() {
            report.push_str(&format!(
                "| {} | {} | {} | {:.1} | {:.1}% | {:.2}x |\n",
                i + 1,
                entry.session_id.chars().take(8).collect::<String>(),
                entry.timestamp.format("%Y-%m-%d"),
                entry.overall_score,
                entry.success_rate,
                entry.average_speedup
            ));
        }

        report.push_str(&format!(
            "\n**Best Score**: {:.1}\n\
             **Average Score**: {:.1}\n\
             **Total Entries**: {}\n",
            self.best_score().unwrap_or(0.0),
            self.average_score(),
            self.entries.len()
        ));

        report
    }
}

/// Continuous monitoring system
pub struct ContinuousMonitor {
    leaderboard: BenchmarkLeaderboard,

    pub alert_thresholds: AlertThresholds,

    historical_evaluations: Vec<TaskEvaluation>,
}

/// Alert thresholds for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Minimum success rate alert
    pub min_success_rate: f64,

    /// Minimum overall score alert
    pub min_overall_score: f64,

    /// Performance regression threshold
    pub regression_threshold: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            min_success_rate: 70.0,
            min_overall_score: 75.0,
            regression_threshold: 5.0,
        }
    }
}

impl ContinuousMonitor {
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            leaderboard: BenchmarkLeaderboard::new(storage_path, 100),
            alert_thresholds: AlertThresholds::default(),
            historical_evaluations: Vec::new(),
        }
    }

    /// Add evaluation and check for alerts
    pub async fn add_evaluation(&mut self, evaluation: TaskEvaluation) -> Vec<Alert> {
        self.historical_evaluations.push(evaluation.clone());

        let mut alerts = Vec::new();

        // Check success rate
        if !evaluation.result.success {
            alerts.push(Alert::TaskFailure {
                task_id: evaluation.task.id.clone(),
                task_name: evaluation.task.name.clone(),
            });
        }

        // Check score thresholds
        let current_score = evaluation.overall_score as f64;
        let min_threshold = self.alert_thresholds.min_overall_score;
        if current_score < min_threshold {
            alerts.push(Alert::LowScore {
                task_id: evaluation.task.id.clone(),
                score: current_score,
                threshold: min_threshold,
            });
        }

        // Check for performance regression
        if let Some(trend) = self.check_regression(&evaluation) {
            alerts.push(trend);
        }

        alerts
    }

    /// Check for performance regression
    fn check_regression(&self, evaluation: &TaskEvaluation) -> Option<Alert> {
        // Find previous evaluations of similar tasks
        let similar_tasks: Vec<_> = self
            .historical_evaluations
            .iter()
            .filter(|e| {
                e.task.id == evaluation.task.id && e.task.difficulty == evaluation.task.difficulty
            })
            .collect();

        if similar_tasks.len() < 3 {
            return None; // Not enough data
        }

        // Calculate average of previous similar tasks
        let avg_previous: f64 = similar_tasks
            .iter()
            .take(similar_tasks.len() - 1) // Exclude current
            .map(|e| e.overall_score as f64)
            .sum::<f64>()
            / (similar_tasks.len() - 1).max(1) as f64;

        let current_score = evaluation.overall_score as f64;
        let regression = avg_previous - current_score;

        if regression > self.alert_thresholds.regression_threshold {
            Some(Alert::PerformanceRegression {
                task_id: evaluation.task.id.clone(),
                previous_avg: avg_previous,
                current_score,
                regression,
            })
        } else {
            None
        }
    }

    /// Get monitoring summary
    pub fn get_summary(&self) -> MonitoringSummary {
        let trend = self.leaderboard.calculate_trend();
        let difficulty_perf = self
            .leaderboard
            .calculate_difficulty_performance(&self.historical_evaluations);

        MonitoringSummary {
            total_evaluations: self.historical_evaluations.len(),
            trend,
            difficulty_performance: difficulty_perf,
            best_score: self.leaderboard.best_score().unwrap_or(0.0),
            average_score: self.leaderboard.average_score(),
        }
    }
}

/// Monitoring alert
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Alert {
    TaskFailure {
        task_id: String,
        task_name: String,
    },
    LowScore {
        task_id: String,
        score: f64,
        threshold: f64,
    },
    PerformanceRegression {
        task_id: String,
        previous_avg: f64,
        current_score: f64,
        regression: f64,
    },
}

/// Monitoring summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringSummary {
    pub total_evaluations: usize,
    pub trend: PerformanceTrend,
    pub difficulty_performance: DifficultyPerformance,
    pub best_score: f64,
    pub average_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaderboard_creation() {
        let leaderboard = BenchmarkLeaderboard::new(PathBuf::from("leaderboard.json"), 10);
        assert_eq!(leaderboard.entries.len(), 0);
    }

    #[test]
    fn test_leaderboard_add_entry() {
        let mut leaderboard = BenchmarkLeaderboard::new(PathBuf::from("leaderboard.json"), 10);

        let entry = LeaderboardEntry {
            session_id: "test".to_string(),
            timestamp: Utc::now(),
            version: "1.0.0".to_string(),
            overall_score: 85.0,
            success_rate: 90.0,
            average_speedup: 2.5,
            tasks_completed: 3,
            total_tasks: 3,
            metadata: LeaderboardMetadata {
                llm_provider: "test".to_string(),
                model: "test".to_string(),
                system_info: "test".to_string(),
                tags: vec!["test".to_string()],
            },
        };

        leaderboard.add_entry(entry);
        assert_eq!(leaderboard.entries.len(), 1);
        assert_eq!(leaderboard.best_score(), Some(85.0));
    }

    #[test]
    fn test_moving_average() {
        let leaderboard = BenchmarkLeaderboard::new(PathBuf::from("leaderboard.json"), 10);
        let data = vec![80.0, 82.0, 85.0, 83.0, 87.0, 89.0, 88.0];
        let result = leaderboard.calculate_moving_average(&data, 3);

        assert!(!result.is_empty());
        assert_eq!(result.len(), data.len());
    }

    // --- Serde roundtrips for data types ---

    #[test]
    fn leaderboard_entry_serde_roundtrip() {
        let entry = LeaderboardEntry {
            session_id: "sess_1".into(),
            timestamp: Utc::now(),
            version: "0.1.0".into(),
            overall_score: 92.5,
            success_rate: 100.0,
            average_speedup: 3.0,
            tasks_completed: 5,
            total_tasks: 5,
            metadata: LeaderboardMetadata {
                llm_provider: "anthropic".into(),
                model: "claude-3".into(),
                system_info: "macos".into(),
                tags: vec!["fast".into()],
            },
        };
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: LeaderboardEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, "sess_1");
        assert_eq!(decoded.overall_score, 92.5);
        assert_eq!(decoded.metadata.tags.len(), 1);
    }

    #[test]
    fn leaderboard_metadata_serde_roundtrip() {
        let meta = LeaderboardMetadata {
            llm_provider: "openai".into(),
            model: "gpt-4".into(),
            system_info: "linux".into(),
            tags: vec!["baseline".into(), "v2".into()],
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: LeaderboardMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.llm_provider, "openai");
        assert_eq!(decoded.tags.len(), 2);
    }

    #[test]
    fn performance_trend_serde_roundtrip() {
        let trend = PerformanceTrend {
            timestamps: vec![Utc::now()],
            overall_scores: vec![85.0],
            success_rates: vec![90.0],
            speedup_ratios: vec![2.5],
            moving_average: vec![85.0],
        };
        let json = serde_json::to_string(&trend).unwrap();
        let decoded: PerformanceTrend = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.overall_scores.len(), 1);
    }

    #[test]
    fn difficulty_performance_serde_roundtrip() {
        let dp = DifficultyPerformance {
            easy: 95.0,
            medium: 80.0,
            hard: 60.0,
            expert: 40.0,
        };
        let json = serde_json::to_string(&dp).unwrap();
        let decoded: DifficultyPerformance = serde_json::from_str(&json).unwrap();
        assert!((decoded.easy - 95.0).abs() < f64::EPSILON);
        assert!((decoded.expert - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn alert_serde_variants() {
        let alerts = vec![
            Alert::TaskFailure {
                task_id: "T01".into(),
                task_name: "fix bug".into(),
            },
            Alert::LowScore {
                task_id: "T02".into(),
                score: 30.0,
                threshold: 50.0,
            },
            Alert::PerformanceRegression {
                task_id: "T03".into(),
                previous_avg: 80.0,
                current_score: 60.0,
                regression: 20.0,
            },
        ];
        for alert in &alerts {
            let json = serde_json::to_string(alert).unwrap();
            let decoded: Alert = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn monitoring_summary_serde_roundtrip() {
        let summary = MonitoringSummary {
            total_evaluations: 10,
            trend: PerformanceTrend {
                timestamps: vec![],
                overall_scores: vec![],
                success_rates: vec![],
                speedup_ratios: vec![],
                moving_average: vec![],
            },
            difficulty_performance: DifficultyPerformance {
                easy: 90.0,
                medium: 70.0,
                hard: 50.0,
                expert: 30.0,
            },
            best_score: 95.0,
            average_score: 75.0,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: MonitoringSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_evaluations, 10);
    }

    // --- Leaderboard logic ---

    #[test]
    fn leaderboard_add_entries_sorted_by_score() {
        let mut lb = BenchmarkLeaderboard::new(PathBuf::from("lb.json"), 10);
        for score in [70.0, 90.0, 80.0] {
            lb.add_entry(LeaderboardEntry {
                session_id: format!("s{}", score as i32),
                timestamp: Utc::now(),
                version: "1.0".into(),
                overall_score: score,
                success_rate: score,
                average_speedup: 1.0,
                tasks_completed: 1,
                total_tasks: 1,
                metadata: LeaderboardMetadata {
                    llm_provider: "test".into(),
                    model: "test".into(),
                    system_info: "test".into(),
                    tags: vec![],
                },
            });
        }
        assert_eq!(lb.best_score(), Some(90.0));
        let top = lb.top(3);
        assert_eq!(top[0].overall_score, 90.0);
        assert_eq!(top[1].overall_score, 80.0);
        assert_eq!(top[2].overall_score, 70.0);
    }

    #[test]
    fn leaderboard_truncates_to_max() {
        let mut lb = BenchmarkLeaderboard::new(PathBuf::from("lb.json"), 2);
        for score in [70.0, 90.0, 80.0] {
            lb.add_entry(LeaderboardEntry {
                session_id: format!("s{}", score as i32),
                timestamp: Utc::now(),
                version: "1.0".into(),
                overall_score: score,
                success_rate: score,
                average_speedup: 1.0,
                tasks_completed: 1,
                total_tasks: 1,
                metadata: LeaderboardMetadata {
                    llm_provider: "test".into(),
                    model: "test".into(),
                    system_info: "test".into(),
                    tags: vec![],
                },
            });
        }
        assert_eq!(lb.entries.len(), 2);
        assert_eq!(lb.best_score(), Some(90.0));
    }

    #[test]
    fn leaderboard_average_score() {
        let mut lb = BenchmarkLeaderboard::new(PathBuf::from("lb.json"), 10);
        assert!((lb.average_score() - 0.0).abs() < f64::EPSILON);
        lb.add_entry(LeaderboardEntry {
            session_id: "s1".into(),
            timestamp: Utc::now(),
            version: "1.0".into(),
            overall_score: 80.0,
            success_rate: 80.0,
            average_speedup: 1.0,
            tasks_completed: 1,
            total_tasks: 1,
            metadata: LeaderboardMetadata {
                llm_provider: "test".into(),
                model: "test".into(),
                system_info: "test".into(),
                tags: vec![],
            },
        });
        assert!((lb.average_score() - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn leaderboard_get_top_limited() {
        let mut lb = BenchmarkLeaderboard::new(PathBuf::from("lb.json"), 10);
        for i in 0..5 {
            lb.add_entry(LeaderboardEntry {
                session_id: format!("s{}", i),
                timestamp: Utc::now(),
                version: "1.0".into(),
                overall_score: (i + 1) as f64 * 10.0,
                success_rate: 100.0,
                average_speedup: 1.0,
                tasks_completed: 1,
                total_tasks: 1,
                metadata: LeaderboardMetadata {
                    llm_provider: "test".into(),
                    model: "test".into(),
                    system_info: "test".into(),
                    tags: vec![],
                },
            });
        }
        let top2 = lb.top(2);
        assert_eq!(top2.len(), 2);
        assert_eq!(top2[0].overall_score, 50.0);
    }
}
