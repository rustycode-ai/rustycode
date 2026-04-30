//! Comprehensive Task Evaluation Benchmark System
//!
//! This module provides a framework for evaluating RustyCode's performance
//! on real-world coding tasks with scoring and continuous monitoring.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Difficulty classification for tasks
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TaskDifficulty {
    Easy,
    Medium,
    Hard,
    Expert,
}

impl TaskDifficulty {
    /// Get estimated time for manual completion (in minutes)
    pub fn estimated_manual_time(&self) -> u64 {
        match self {
            TaskDifficulty::Easy => 15,    // 15 minutes
            TaskDifficulty::Medium => 60,  // 1 hour
            TaskDifficulty::Hard => 180,   // 3 hours
            TaskDifficulty::Expert => 480, // 8 hours
        }
    }

    /// Get complexity multiplier for scoring
    pub fn complexity_multiplier(&self) -> f64 {
        match self {
            TaskDifficulty::Easy => 1.0,
            TaskDifficulty::Medium => 2.0,
            TaskDifficulty::Hard => 4.0,
            TaskDifficulty::Expert => 8.0,
        }
    }
}

/// Category of task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskCategory {
    CodeGeneration,
    BugFix,
    Refactoring,
    Testing,
    Documentation,
    Analysis,
    MultiFile,
}

/// A benchmark task with clear success criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkTask {
    /// Unique task identifier
    pub id: String,

    /// Task name
    pub name: String,

    /// Task description
    pub description: String,

    /// Difficulty level
    pub difficulty: TaskDifficulty,

    /// Task category
    pub category: TaskCategory,

    /// Expected files to be created/modified
    pub expected_outputs: Vec<String>,

    /// Success criteria for evaluation
    pub success_criteria: SuccessCriteria,

    /// Starting code state (if applicable)
    pub initial_state: Option<String>,

    /// Reference solution (for comparison)
    pub reference_solution: Option<String>,

    /// Time limit for the task
    pub time_limit: Duration,
}

/// Success criteria for task completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriteria {
    /// Required files must be created
    pub required_files: Vec<String>,

    /// Required functionality must work
    pub functionality_tests: Vec<String>,

    /// Code quality requirements
    pub quality_standards: QualityStandards,

    /// Minimum test coverage required
    pub min_test_coverage: f64,
}

/// Code quality standards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityStandards {
    pub must_compile: bool,
    pub no_warnings: bool,
    pub follows_style: bool,
    pub has_documentation: bool,
    pub error_handling: bool,
}

/// Task execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID
    pub task_id: String,

    /// Start time
    pub start_time: DateTime<Utc>,

    /// End time
    pub end_time: DateTime<Utc>,

    /// Duration taken
    pub duration: Duration,

    /// Success status
    pub success: bool,

    /// Files created
    pub files_created: Vec<String>,

    /// Files modified
    pub files_modified: Vec<String>,

    /// Tests passed
    pub tests_passed: usize,
    pub tests_total: usize,

    /// Code quality score
    pub quality_score: QualityScore,

    /// Performance metrics
    pub performance_score: PerformanceScore,

    /// Comparison to manual development
    pub comparison_score: ComparisonScore,

    /// Errors encountered
    pub errors: Vec<String>,

    /// Agent iterations used
    pub agent_iterations: usize,

    /// Tokens consumed
    pub tokens_consumed: u64,
}

/// Quality score (0-100)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScore {
    /// Functionality correctness (0-40)
    pub functionality: u8,

    /// Code style and readability (0-20)
    pub style: u8,

    /// Error handling (0-20)
    pub error_handling: u8,

    /// Documentation (0-10)
    pub documentation: u8,

    /// Testing (0-10)
    pub testing: u8,

    /// Overall quality score
    pub overall: u8,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceScore {
    /// Speed score (0-30)
    pub speed: u8,

    /// Efficiency (0-30)
    pub efficiency: u8,

    /// Resource usage (0-20)
    pub resource_usage: u8,

    /// Scalability (0-20)
    pub scalability: u8,

    /// Overall performance score (0-100)
    pub overall: u8,
}

/// Comparison to manual development
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonScore {
    /// Time comparison (how much faster/slower)
    pub time_ratio: f64, // < 1.0 = faster than manual, > 1.0 = slower

    /// Quality comparison (relative to manual)
    pub quality_ratio: f64,

    /// Cost effectiveness
    pub cost_effectiveness: f64,

    /// Overall comparison score (0-100)
    pub overall: u8,
}

/// Task evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvaluation {
    /// Task that was evaluated
    pub task: BenchmarkTask,

    /// Result of execution
    pub result: TaskResult,

    /// Overall score (0-100)
    pub overall_score: u8,

    /// Breakdown by category
    pub breakdown: ScoreBreakdown,

    /// Comparison to baseline (manual development)
    pub baseline_comparison: BaselineComparison,

    /// Recommendations for improvement
    pub recommendations: Vec<String>,
}

/// Score breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// Task completion score (0-30)
    pub completion: u8,

    /// Quality score (0-30)
    pub quality: u8,

    /// Performance score (0-20)
    pub performance: u8,

    /// Efficiency score (0-20)
    pub efficiency: u8,

    /// Overall breakdown
    pub overall: u8,
}

/// Baseline comparison data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineComparison {
    /// Manual development time (estimated)
    pub manual_time: Duration,

    /// Automated time (actual)
    pub automated_time: Duration,

    /// Time speedup/slowdown
    pub speedup_ratio: f64,

    /// Quality comparison
    pub quality_comparison: QualityComparison,
}

/// Quality comparison metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityComparison {
    /// Manual quality score (estimated)
    pub manual_quality: u8,

    /// Automated quality score
    pub automated_quality: u8,

    /// Quality ratio
    pub quality_ratio: f64,
}

/// Task evaluator
pub struct TaskEvaluator {
    /// Historical results for comparison
    historical_results: Vec<TaskEvaluation>,

    /// Current benchmark configuration
    #[allow(dead_code)] // Kept for future use
    config: EvaluatorConfig,
}

/// Configuration for task evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorConfig {
    /// Enable detailed logging
    pub verbose_logging: bool,

    /// Save results to file
    pub save_results: bool,

    /// Results directory
    pub results_dir: String,

    /// Time limit multiplier (over estimated manual time)
    pub time_limit_multiplier: f64,
}

impl Default for TaskEvaluator {
    fn default() -> Self {
        Self {
            historical_results: Vec::new(),
            config: EvaluatorConfig {
                verbose_logging: true,
                save_results: true,
                results_dir: "benchmark_results".to_string(),
                time_limit_multiplier: 2.0,
            },
        }
    }
}

impl TaskEvaluator {
    /// Create a new task evaluator
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a task library with standard benchmark tasks
    pub fn create_task_library() -> Vec<BenchmarkTask> {
        vec![
            // Easy Tasks
            BenchmarkTask {
                id: "easy_hello_world".to_string(),
                name: "Hello World Function".to_string(),
                description: "Create a simple 'Hello, World!' function in Rust that returns a string".to_string(),
                difficulty: TaskDifficulty::Easy,
                category: TaskCategory::CodeGeneration,
                expected_outputs: vec!["src/hello.rs".to_string()],
                success_criteria: SuccessCriteria {
                    required_files: vec!["src/hello.rs".to_string()],
                    functionality_tests: vec!["Function returns 'Hello, World!'".to_string()],
                    quality_standards: QualityStandards {
                        must_compile: true,
                        no_warnings: true,
                        follows_style: true,
                        has_documentation: false,
                        error_handling: false,
                    },
                    min_test_coverage: 0.0,
                },
                initial_state: None,
                reference_solution: Some("pub fn hello() -> String { String::from(\"Hello, World!\") }".to_string()),
                time_limit: Duration::from_mins(5), // 5 minutes
            },

            // Medium Tasks
            BenchmarkTask {
                id: "medium_calculator".to_string(),
                name: "CLI Calculator".to_string(),
                description: "Create a command-line calculator that supports basic operations (+, -, *, /) and maintains a running total. The calculator should handle errors gracefully and provide clear user feedback.".to_string(),
                difficulty: TaskDifficulty::Medium,
                category: TaskCategory::CodeGeneration,
                expected_outputs: vec!["src/main.rs".to_string()],
                success_criteria: SuccessCriteria {
                    required_files: vec!["src/main.rs".to_string()],
                    functionality_tests: vec![
                        "Addition works correctly".to_string(),
                        "Division by zero is handled".to_string(),
                        "User can exit the calculator".to_string(),
                        "Running total is maintained".to_string(),
                    ],
                    quality_standards: QualityStandards {
                        must_compile: true,
                        no_warnings: true,
                        follows_style: true,
                        has_documentation: true,
                        error_handling: true,
                    },
                    min_test_coverage: 50.0,
                },
                initial_state: None,
                reference_solution: None,
                time_limit: Duration::from_mins(30), // 30 minutes
            },

            // Hard Tasks
            BenchmarkTask {
                id: "hard_file_processor".to_string(),
                name: "Multi-File Data Processor".to_string(),
                description: "Create a data processing tool that reads from multiple input files, processes the data according to configurable rules, and writes output to multiple formats (CSV, JSON, plain text). The tool should handle errors gracefully, provide progress feedback, and include comprehensive tests.".to_string(),
                difficulty: TaskDifficulty::Hard,
                category: TaskCategory::CodeGeneration,
                expected_outputs: vec!["src/main.rs".to_string(), "src/processor.rs".to_string(), "src/config.rs".to_string(), "tests/integration.rs".to_string()],
                success_criteria: SuccessCriteria {
                    required_files: vec!["src/main.rs".to_string(), "src/processor.rs".to_string()],
                    functionality_tests: vec![
                        "Processes multiple input files".to_string(),
                        "Handles missing files gracefully".to_string(),
                        "Writes to multiple output formats".to_string(),
                        "Configuration is parsed correctly".to_string(),
                        "Progress feedback is provided".to_string(),
                    ],
                    quality_standards: QualityStandards {
                        must_compile: true,
                        no_warnings: true,
                        follows_style: true,
                        has_documentation: true,
                        error_handling: true,
                    },
                    min_test_coverage: 80.0,
                },
                initial_state: None,
                reference_solution: None,
                time_limit: Duration::from_mins(90), // 90 minutes
            },
        ]
    }

    /// Evaluate a single task execution
    pub fn evaluate_task(
        &mut self,
        task: &BenchmarkTask,
        execution_result: &TaskResult,
    ) -> TaskEvaluation {
        // Calculate scores
        let completion_score = self.calculate_completion_score(task, execution_result);
        let quality_score = execution_result.quality_score.clone();
        let performance_score = self.calculate_performance_score(task, execution_result);
        let efficiency_score = self.calculate_efficiency_score(task, execution_result);

        let overall_score = Self::calculate_overall_score(
            completion_score,
            quality_score.overall,
            performance_score.overall,
            efficiency_score,
        );

        let breakdown = ScoreBreakdown {
            completion: completion_score,
            quality: quality_score.overall,
            performance: performance_score.overall,
            efficiency: efficiency_score,
            overall: overall_score,
        };

        let baseline_comparison = self.calculate_baseline_comparison(task, execution_result);

        let recommendations = self.generate_recommendations(&breakdown, &baseline_comparison);

        TaskEvaluation {
            task: task.clone(),
            result: execution_result.clone(),
            overall_score,
            breakdown,
            baseline_comparison,
            recommendations,
        }
    }

    /// Calculate task completion score
    fn calculate_completion_score(&self, task: &BenchmarkTask, result: &TaskResult) -> u8 {
        let mut score = 0u8;

        // Check if task completed within time limit
        if result.duration <= task.time_limit {
            score += 10;
        } else {
            score += 5; // Partial credit
        }

        // Check required files
        let required_files_count = task.success_criteria.required_files.len();
        let files_created = result.files_created.len()
            + result.files_modified.len()
            + result
                .files_modified
                .iter()
                .filter(|f| task.success_criteria.required_files.contains(f))
                .count();

        if files_created >= required_files_count {
            score += 10;
        } else {
            score += (files_created * 10 / required_files_count.max(1)) as u8;
        }

        // Check test success
        if result.tests_total > 0 {
            let test_ratio = result.tests_passed as f64 / result.tests_total as f64;
            score += (test_ratio * 10.0) as u8;
        }

        score
    }

    /// Calculate performance score
    fn calculate_performance_score(
        &self,
        task: &BenchmarkTask,
        result: &TaskResult,
    ) -> PerformanceScore {
        let estimated_manual = task.difficulty.estimated_manual_time() as f64 * 60.0; // to seconds as f64
        let actual_time = result.duration.as_secs() as f64;

        let speed = if actual_time < estimated_manual {
            let ratio: f64 = estimated_manual / actual_time;
            (ratio.min(3.0) / 3.0 * 30.0) as u8 // Max score for 3x faster
        } else {
            let ratio: f64 = actual_time / estimated_manual;
            ((3.0 - (ratio.min(3.0) - 1.0) / 2.0) * 30.0) as u8 // Reduced score if slower
        };

        let efficiency = if result.tokens_consumed > 0 {
            let value_per_token =
                (task.difficulty.complexity_multiplier() * 1000.0) / result.tokens_consumed as f64;
            (value_per_token.min(100.0) / 100.0 * 30.0) as u8
        } else {
            15u8
        };

        let resource_usage = 20u8; // Placeholder - would measure actual resource usage

        let scalability = if task.difficulty == TaskDifficulty::Easy {
            20u8
        } else if task.difficulty == TaskDifficulty::Medium {
            15u8
        } else {
            10u8
        };

        let overall =
            ((speed as u16 + efficiency as u16 + resource_usage as u16 + scalability as u16) / 4)
                as u8;

        PerformanceScore {
            speed,
            efficiency,
            resource_usage,
            scalability,
            overall,
        }
    }

    /// Calculate efficiency score
    fn calculate_efficiency_score(&self, task: &BenchmarkTask, result: &TaskResult) -> u8 {
        let mut score = 0u8;

        // Agent iteration efficiency
        let expected_iterations = match task.difficulty {
            TaskDifficulty::Easy => 3,
            TaskDifficulty::Medium => 5,
            TaskDifficulty::Hard => 10,
            TaskDifficulty::Expert => 15,
        };

        if result.agent_iterations <= expected_iterations {
            score += 10;
        } else {
            let ratio = expected_iterations as f64 / result.agent_iterations.max(1) as f64;
            score += (ratio * 10.0) as u8;
        }

        // Error handling efficiency
        if result.errors.is_empty() {
            score += 10;
        } else {
            score += 5; // Some errors but task completed
        }

        score
    }

    /// Calculate overall score
    fn calculate_overall_score(completion: u8, quality: u8, performance: u8, efficiency: u8) -> u8 {
        // Weighted average: completion(30), quality(30), performance(20), efficiency(20)
        let overall = (completion as u16 * 3
            + quality as u16 * 3
            + performance as u16 * 2
            + efficiency as u16 * 2)
            / 10;
        overall.min(100) as u8
    }

    /// Calculate baseline comparison
    fn calculate_baseline_comparison(
        &self,
        task: &BenchmarkTask,
        result: &TaskResult,
    ) -> BaselineComparison {
        let manual_time = Duration::from_secs(task.difficulty.estimated_manual_time() * 60);
        let speedup_ratio = manual_time.as_secs_f64() / result.duration.as_secs_f64();

        let quality_comparison = QualityComparison {
            manual_quality: 85, // Assume manual development is 85% quality
            automated_quality: result.quality_score.overall,
            quality_ratio: result.quality_score.overall as f64 / 85.0,
        };

        BaselineComparison {
            manual_time,
            automated_time: result.duration,
            speedup_ratio,
            quality_comparison,
        }
    }

    /// Generate recommendations based on scores
    fn generate_recommendations(
        &self,
        breakdown: &ScoreBreakdown,
        baseline: &BaselineComparison,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Performance recommendations
        if breakdown.performance < 20 {
            recommendations.push(
                "Consider optimizing agent iteration count to improve performance".to_string(),
            );
        }

        // Quality recommendations
        if breakdown.quality < 20 {
            recommendations.push(
                "Focus on improving code quality scores through better testing and documentation"
                    .to_string(),
            );
        }

        // Comparison recommendations
        if baseline.speedup_ratio < 1.0 {
            recommendations.push(
                "Automated approach is slower than manual; consider task selection optimization"
                    .to_string(),
            );
        }

        if baseline.quality_comparison.quality_ratio < 0.9 {
            recommendations.push(
                "Automated quality is below manual standards; review and improve code generation"
                    .to_string(),
            );
        }

        recommendations
    }

    /// Get benchmark summary statistics
    pub fn get_summary_stats(&self) -> BenchmarkSummary {
        let total_evaluations = self.historical_results.len();

        let avg_score = if total_evaluations > 0 {
            let sum: u32 = self
                .historical_results
                .iter()
                .map(|e| e.overall_score as u32)
                .sum();
            (sum / total_evaluations as u32) as u8
        } else {
            0
        };

        let avg_speedup = if total_evaluations > 0 {
            let sum: f64 = self
                .historical_results
                .iter()
                .map(|e| e.baseline_comparison.speedup_ratio)
                .sum();
            sum / total_evaluations as f64
        } else {
            1.0
        };

        let success_rate = {
            let successes = self
                .historical_results
                .iter()
                .filter(|e| e.result.success)
                .count();
            (successes * 100)
                .checked_div(total_evaluations)
                .unwrap_or(0) as u8
        };

        BenchmarkSummary {
            total_evaluations,
            average_score: avg_score,
            average_speedup: avg_speedup,
            success_rate,
            last_evaluation: self.historical_results.last().map(|e| e.result.end_time),
        }
    }
}

/// Benchmark summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub total_evaluations: usize,
    pub average_score: u8,
    pub average_speedup: f64,
    pub success_rate: u8,
    pub last_evaluation: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_difficulty_time_estimates() {
        assert_eq!(TaskDifficulty::Easy.estimated_manual_time(), 15);
        assert_eq!(TaskDifficulty::Medium.estimated_manual_time(), 60);
        assert_eq!(TaskDifficulty::Hard.estimated_manual_time(), 180);
        assert_eq!(TaskDifficulty::Expert.estimated_manual_time(), 480);
    }

    #[test]
    fn test_complexity_multipliers() {
        assert_eq!(TaskDifficulty::Easy.complexity_multiplier(), 1.0);
        assert_eq!(TaskDifficulty::Medium.complexity_multiplier(), 2.0);
        assert_eq!(TaskDifficulty::Hard.complexity_multiplier(), 4.0);
        assert_eq!(TaskDifficulty::Expert.complexity_multiplier(), 8.0);
    }

    #[test]
    fn test_task_evaluator_creation() {
        let evaluator = TaskEvaluator::new();
        assert_eq!(evaluator.historical_results.len(), 0);
    }

    #[test]
    fn test_task_library_creation() {
        let tasks = TaskEvaluator::create_task_library();
        assert_eq!(tasks.len(), 3); // Easy, Medium, Hard tasks
    }
}
