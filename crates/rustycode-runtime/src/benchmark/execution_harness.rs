//! Benchmark Execution Harness
//!
//! This module provides the execution framework for running benchmarks on RustyCode,
//! including task execution, result collection, and persistence.

use crate::benchmark::task_evaluator::{
    BenchmarkTask, ComparisonScore, PerformanceScore, QualityScore, TaskEvaluation, TaskEvaluator,
    TaskResult,
};
use crate::error::BenchmarkError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::fs;
use tracing::{debug, error, info};

/// Benchmark execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Root directory for benchmark operations
    pub workspace_root: PathBuf,

    /// Directory for storing results
    pub results_directory: PathBuf,

    /// Whether to save detailed logs
    pub verbose_logging: bool,

    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,

    /// Timeout multiplier (over task time limit)
    pub timeout_multiplier: f64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from(".benchmark_workspace"),
            results_directory: PathBuf::from("benchmark_results"),
            verbose_logging: true,
            max_concurrent_tasks: 1,
            timeout_multiplier: 2.0,
        }
    }
}

/// Result of a benchmark execution session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSession {
    /// Session identifier
    pub session_id: String,

    pub start_time: DateTime<Utc>,

    pub end_time: DateTime<Utc>,

    /// Total duration
    pub duration: Duration,

    pub tasks_executed: Vec<String>,

    /// Overall results
    pub evaluations: Vec<TaskEvaluation>,

    pub success_rate: f64,

    pub average_score: f64,

    /// System metadata
    pub metadata: SessionMetadata,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub rustycode_version: String,

    /// LLM provider used
    pub llm_provider: String,

    /// Model used
    pub model: String,

    /// System information
    pub system_info: String,

    /// Configuration used
    pub config_snapshot: String,
}

/// Benchmark execution harness
pub struct BenchmarkHarness {
    /// Benchmark configuration
    config: BenchmarkConfig,

    /// Task evaluator
    evaluator: TaskEvaluator,

    current_session_id: Option<String>,
}

impl BenchmarkHarness {
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            config,
            evaluator: TaskEvaluator::new(),
            current_session_id: None,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(BenchmarkConfig::default())
    }

    /// Run a single benchmark task
    pub async fn run_single_task(
        &mut self,
        task: &BenchmarkTask,
    ) -> Result<TaskEvaluation, BenchmarkError> {
        let session_id = self
            .current_session_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        info!("Starting benchmark task: {} ({})", task.name, task.id);
        let start_time = Instant::now();
        let _start_datetime = Utc::now();

        // Execute the task using RustyCode agent
        let task_result = match self.execute_rustycode_task(task).await {
            Ok(result) => result,
            Err(e) => {
                error!("Task execution failed: {}", e);
                return Err(BenchmarkError::TaskFailed(format!(
                    "Task execution failed: {}",
                    e
                )));
            }
        };

        let duration = start_time.elapsed();
        info!("Task completed in {:?}", duration);

        // Evaluate the result
        let evaluation = self.evaluator.evaluate_task(task, &task_result);

        // Save results if configured
        if self.config.verbose_logging {
            self.save_task_result(&session_id, &evaluation).await?;
        }

        Ok(evaluation)
    }

    /// Run all tasks from the task library
    pub async fn run_task_library(&mut self) -> Result<BenchmarkSession, BenchmarkError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        self.current_session_id = Some(session_id.clone());

        let start_time = Utc::now();
        let tasks = TaskEvaluator::create_task_library();

        info!(
            "Starting benchmark session {} with {} tasks",
            session_id,
            tasks.len()
        );

        let mut evaluations = Vec::new();
        let mut tasks_executed = Vec::new();

        for task in &tasks {
            match self.run_single_task(task).await {
                Ok(evaluation) => {
                    tasks_executed.push(task.id.clone());
                    evaluations.push(evaluation);
                }
                Err(e) => {
                    error!("Failed to execute task {}: {}", task.id, e);
                    // Continue with other tasks
                }
            }
        }

        let end_time = Utc::now();
        let duration = (end_time - start_time).to_std().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "clock skew detected in benchmark duration, using zero");
            std::time::Duration::ZERO
        });

        // Calculate session statistics
        let success_rate = if evaluations.is_empty() {
            0.0
        } else {
            (evaluations.iter().filter(|e| e.result.success).count() as f64
                / evaluations.len() as f64)
                * 100.0
        };

        let average_score = if evaluations.is_empty() {
            0.0
        } else {
            evaluations
                .iter()
                .map(|e| e.overall_score as f64)
                .sum::<f64>()
                / evaluations.len() as f64
        };

        let session = BenchmarkSession {
            session_id: session_id.clone(),
            start_time,
            end_time,
            duration,
            tasks_executed,
            evaluations: evaluations.clone(),
            success_rate,
            average_score,
            metadata: self.collect_metadata(),
        };

        // Save session results
        self.save_session_results(&session).await?;

        info!(
            "Benchmark session {} completed: {:.1}% success, {:.1} avg score",
            session_id, success_rate, average_score
        );

        Ok(session)
    }

    /// Execute a task using RustyCode agent
    async fn execute_rustycode_task(
        &self,
        task: &BenchmarkTask,
    ) -> Result<TaskResult, BenchmarkError> {
        // This is a mock implementation - in production, this would:
        // 1. Set up a temporary workspace
        // 2. Initialize a RustyCode agent session
        // 3. Provide the task description
        // 4. Monitor execution
        // 5. Collect results (files created, tests passed, etc.)

        let start_time = Utc::now();
        let execution_start = Instant::now();

        // Simulate task execution
        tokio::time::sleep(Duration::from_millis(100)).await;

        let duration = execution_start.elapsed();
        let end_time = Utc::now();

        // Mock result - replace with actual execution
        Ok(TaskResult {
            task_id: task.id.clone(),
            start_time,
            end_time,
            duration,
            success: true, // Would be determined by actual execution
            files_created: task.expected_outputs.clone(),
            files_modified: Vec::new(),
            tests_passed: task.success_criteria.functionality_tests.len(),
            tests_total: task.success_criteria.functionality_tests.len(),
            quality_score: QualityScore {
                functionality: 35,
                style: 15,
                error_handling: 15,
                documentation: 8,
                testing: 9,
                overall: 82,
            },
            performance_score: PerformanceScore {
                speed: 25,
                efficiency: 22,
                resource_usage: 15,
                scalability: 15,
                overall: 77,
            },
            comparison_score: ComparisonScore {
                time_ratio: 0.8,
                quality_ratio: 0.95,
                cost_effectiveness: 1.2,
                overall: 85,
            },
            errors: Vec::new(),
            agent_iterations: 3,
            tokens_consumed: 1500,
        })
    }

    /// Save task result to disk
    async fn save_task_result(
        &self,
        session_id: &str,
        evaluation: &TaskEvaluation,
    ) -> Result<(), BenchmarkError> {
        let filename = format!(
            "{}_{}_{}.json",
            session_id,
            evaluation.task.id,
            Utc::now().timestamp()
        );
        let path = self.config.results_directory.join(filename);

        fs::create_dir_all(&self.config.results_directory)
            .await
            .map_err(|e| {
                BenchmarkError::SaveFailed(format!("Failed to create results directory: {}", e))
            })?;

        let json = serde_json::to_string_pretty(evaluation).map_err(|e| {
            BenchmarkError::SaveFailed(format!("Failed to serialize evaluation: {}", e))
        })?;

        fs::write(&path, json).await.map_err(|e| {
            BenchmarkError::SaveFailed(format!("Failed to write result file: {}", e))
        })?;

        debug!("Saved task result to {:?}", path);
        Ok(())
    }

    /// Save session results to disk
    async fn save_session_results(&self, session: &BenchmarkSession) -> Result<(), BenchmarkError> {
        let filename = format!("session_{}.json", session.session_id);
        let path = self.config.results_directory.join(filename);

        let json = serde_json::to_string_pretty(session).map_err(|e| {
            BenchmarkError::SaveFailed(format!("Failed to serialize session: {}", e))
        })?;

        fs::write(&path, json).await.map_err(|e| {
            BenchmarkError::SaveFailed(format!("Failed to write session file: {}", e))
        })?;

        info!("Saved session results to {:?}", path);
        Ok(())
    }

    /// Collect session metadata
    fn collect_metadata(&self) -> SessionMetadata {
        SessionMetadata {
            rustycode_version: env!("CARGO_PKG_VERSION").to_string(),
            llm_provider: "unknown".to_string(), // Would be detected from config
            model: "unknown".to_string(),
            system_info: format!("{:?}", std::env::consts::OS),
            config_snapshot: format!("{:?}", self.config),
        }
    }

    /// Load historical results for comparison
    pub async fn load_historical_results(&mut self) -> Result<(), BenchmarkError> {
        let results_dir = &self.config.results_directory;

        if !results_dir.exists() {
            debug!("No historical results found");
            return Ok(());
        }

        let mut entries = fs::read_dir(results_dir).await.map_err(|e| {
            BenchmarkError::LoadFailed(format!("Failed to read results directory: {}", e))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            BenchmarkError::LoadFailed(format!("Failed to read directory entry: {}", e))
        })? {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(&path).await.map_err(|e| {
                    BenchmarkError::LoadFailed(format!("Failed to read file {:?}: {}", path, e))
                })?;

                // Try to parse as TaskEvaluation
                if let Ok(evaluation) = serde_json::from_str::<TaskEvaluation>(&content) {
                    // Add to evaluator's historical results
                    // Note: We'd need to add this method to TaskEvaluator
                    debug!("Loaded historical evaluation: {}", evaluation.task.id);
                }
            }
        }

        Ok(())
    }

    /// Generate benchmark report
    pub fn generate_report(&self, session: &BenchmarkSession) -> String {
        let mut report = format!(
            "# Benchmark Session Report\n\n\
             **Session ID**: {}\n\
             **Duration**: {:?}\n\
             **Tasks Executed**: {}\n\
             **Success Rate**: {:.1}%\n\
             **Average Score**: {:.1}/100\n\n",
            session.session_id,
            session.duration,
            session.tasks_executed.len(),
            session.success_rate,
            session.average_score
        );

        report.push_str("## Task Results\n\n");

        for evaluation in &session.evaluations {
            report.push_str(&format!(
                "### {} ({})\n\
                 - **Difficulty**: {:?}\n\
                 - **Score**: {}/100\n\
                 - **Success**: {}\n\
                 - **Duration**: {:?}\n\
                 - **Quality**: {}/100\n\
                 - **Performance**: {}/100\n\n",
                evaluation.task.name,
                evaluation.task.id,
                evaluation.task.difficulty,
                evaluation.overall_score,
                evaluation.result.success,
                evaluation.result.duration,
                evaluation.breakdown.quality,
                evaluation.breakdown.performance
            ));
        }

        report.push_str("## Recommendations\n\n");

        for evaluation in &session.evaluations {
            if !evaluation.recommendations.is_empty() {
                report.push_str(&format!("### {}:\n", evaluation.task.name));
                for rec in &evaluation.recommendations {
                    report.push_str(&format!("- {}\n", rec));
                }
                report.push('\n');
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_benchmark_harness_creation() {
        let harness = BenchmarkHarness::with_defaults();
        assert_eq!(harness.config.max_concurrent_tasks, 1);
    }

    #[tokio::test]
    async fn test_run_single_task() {
        let mut harness = BenchmarkHarness::with_defaults();
        let tasks = TaskEvaluator::create_task_library();

        if let Some(task) = tasks.first() {
            let result = harness.run_single_task(task).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_benchmark_config_default() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.max_concurrent_tasks, 1);
        assert_eq!(config.timeout_multiplier, 2.0);
    }

    // --- BenchmarkConfig serde ---

    #[test]
    fn benchmark_config_serde_roundtrip() {
        let config = BenchmarkConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: BenchmarkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_concurrent_tasks, 1);
        assert!((decoded.timeout_multiplier - 2.0).abs() < f64::EPSILON);
        assert!(decoded.verbose_logging);
    }

    #[test]
    fn benchmark_config_custom() {
        let config = BenchmarkConfig {
            workspace_root: PathBuf::from("/tmp/bench"),
            results_directory: PathBuf::from("/tmp/results"),
            verbose_logging: false,
            max_concurrent_tasks: 4,
            timeout_multiplier: 1.5,
        };
        assert_eq!(config.max_concurrent_tasks, 4);
        assert!(!config.verbose_logging);
    }

    // --- BenchmarkSession serde ---

    #[test]
    fn benchmark_session_serde_roundtrip() {
        let session = BenchmarkSession {
            session_id: "bench-001".into(),
            start_time: Utc::now(),
            end_time: Utc::now(),
            duration: Duration::from_mins(2),
            tasks_executed: vec!["task1".into()],
            evaluations: vec![],
            success_rate: 100.0,
            average_score: 85.0,
            metadata: SessionMetadata {
                rustycode_version: "0.1.0".into(),
                llm_provider: "anthropic".into(),
                model: "claude-3".into(),
                system_info: "macos".into(),
                config_snapshot: "default".into(),
            },
        };
        let json = serde_json::to_string(&session).unwrap();
        let decoded: BenchmarkSession = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, "bench-001");
        assert_eq!(decoded.success_rate, 100.0);
        assert_eq!(decoded.duration, Duration::from_mins(2));
    }

    // --- SessionMetadata serde ---

    #[test]
    fn session_metadata_serde_roundtrip() {
        let meta = SessionMetadata {
            rustycode_version: "0.2.0".into(),
            llm_provider: "openai".into(),
            model: "gpt-4".into(),
            system_info: "linux".into(),
            config_snapshot: "custom".into(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.rustycode_version, "0.2.0");
        assert_eq!(decoded.model, "gpt-4");
    }

    // --- Harness with_defaults ---

    #[test]
    fn harness_with_defaults_matches_config() {
        let harness = BenchmarkHarness::with_defaults();
        let config = BenchmarkConfig::default();
        assert_eq!(
            harness.config.max_concurrent_tasks,
            config.max_concurrent_tasks
        );
        assert!(harness.current_session_id.is_none());
    }
}
