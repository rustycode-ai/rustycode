//! Benchmark System for RustyCode Evaluation
//!
//! This module provides comprehensive benchmarking capabilities to evaluate
//! RustyCode's performance on real-world coding tasks with scoring and continuous monitoring.
//!
//! # Features
//!
//! - **Multi-dimensional Scoring**: Quality, Performance, Efficiency, Comparison
//! - **Task Library**: Predefined tasks with difficulty levels (Easy/Medium/Hard/Expert)
//! - **Baseline Comparison**: Compare automated vs manual development
//! - **Continuous Monitoring**: Track performance over time
//! - **Detailed Analytics**: Comprehensive breakdowns and recommendations
//!
//! # Usage
//!
//! ```ignore
//! use rustycode_runtime::benchmark::{BenchmarkHarness, BenchmarkConfig};
//!
//! # async fn example() -> Result<(), String> {
//! let harness = BenchmarkHarness::with_defaults();
//! let session = harness.run_task_library().await?;
//! println!("Success rate: {:.1}%", session.success_rate);
//! # Ok(())
//! # }
//! ```

pub mod execution_harness;
pub mod leaderboard;
pub mod task_evaluator;

pub use execution_harness::{BenchmarkConfig, BenchmarkHarness, BenchmarkSession, SessionMetadata};
pub use leaderboard::{
    Alert, AlertThresholds, BenchmarkLeaderboard, ContinuousMonitor, DifficultyPerformance,
    LeaderboardEntry, LeaderboardMetadata, MonitoringSummary, PerformanceTrend,
};
pub use task_evaluator::{
    BaselineComparison, BenchmarkSummary, BenchmarkTask, ComparisonScore, EvaluatorConfig,
    PerformanceScore, QualityScore, QualityStandards, ScoreBreakdown, SuccessCriteria,
    TaskCategory, TaskDifficulty, TaskEvaluation, TaskEvaluator, TaskResult,
};
